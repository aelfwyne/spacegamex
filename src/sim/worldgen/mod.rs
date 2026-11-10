//! Detailed star-system generation.
//!
//! Adapted from the user's "Alien RPG system generator" (Google Apps
//! Script): star types, binary companions with Holman–Wiegert stability,
//! gas giants with moon systems, terrestrial and ice worlds (size,
//! atmosphere, temperature, geosphere, terrain, surface layout, tidal
//! locking), asteroid belts, native life, sentient species, native
//! societies and colonies. The roll tables and modifiers are ported
//! faithfully; the manual-entry parts (overrides, domains, sheets) are not —
//! systems here are only ever generated.
//!
//! On top of the port, this module turns the description into gameplay:
//! orbits in AU, deposits for the drones, salvage, and the civilisation the
//! trade screen deals with.

pub mod bodies;
pub mod colony;
pub mod dice;
pub mod life;
pub mod stars;
pub mod terrain;

use super::civ::{base_price, Civ, CivKind, CivTier, T2_STOCK};
use super::opts::GalaxyOpts;
use super::planet::{
    planet_name, Atmo, AtmoKind, BeltFeature, ColonyTech, Deposit, GgFeature, Host, LifeType, NativeTech,
    Planet, PlanetClass, Salvage, TempBand,
};
use super::res::Res;
use super::star::{Star, StarClass, StarKind};
use super::state::Loc;
use super::system::{Companion, StarSystem};
use crate::rngx::derive;
use bodies::{Kind, Proto};
use dice::{log10_range, percent, unit, weighted};
use rand::rngs::StdRng;
use rand::RngExt;
use std::collections::BTreeMap;

pub fn generate(seed: u64, id: u64, pos: [f64; 2], is_home: bool, opts: &GalaxyOpts) -> StarSystem {
    let mut rng = derive(seed, id.wrapping_add(0x5eed_b10c));
    let (star, mods) = if is_home {
        (stars::build_star(&mut rng, StarClass::G, StarKind::MainSequence, 2, Some(1.0)), stars::Mods { ice: -1, ..Default::default() })
    } else {
        stars::roll_star(&mut rng)
    };
    let raw_a = bodies::star_bodies(&mut rng, &mods);
    let companion = if is_home { None } else { stars::roll_binary(&mut rng, &star) };

    // ---- orbits in AU, binary trimming ----
    let mut protos: Vec<Proto> = Vec::new();
    let hz_a = star.habitable_au();
    match &companion {
        None => {
            for mut b in raw_a {
                place(&mut b, Host::A, hz_a);
                protos.push(b);
            }
        }
        Some((c, mods_b)) if c.p_type => {
            let hz = (star.lum + c.star.lum).sqrt();
            for mut b in raw_a {
                place(&mut b, Host::AB, hz);
                if b.au >= c.circumbinary {
                    protos.push(b);
                }
            }
            let _ = mods_b;
        }
        Some((c, mods_b)) => {
            for mut b in raw_a {
                place(&mut b, Host::A, hz_a);
                if b.au <= c.limit_a {
                    protos.push(b);
                }
            }
            let hz_b = c.star.habitable_au();
            if c.limit_b >= 0.1 * hz_b {
                for mut b in bodies::star_bodies(&mut rng, mods_b) {
                    place(&mut b, Host::B, hz_b);
                    if b.au <= c.limit_b {
                        protos.push(b);
                    }
                }
            }
        }
    }
    protos.sort_by(|a, b| (a.host == Host::B).cmp(&(b.host == Host::B)).then(a.au.total_cmp(&b.au)));

    // ---- names ----
    let (mut ia, mut ib) = (0usize, 0usize);
    for b in protos.iter_mut() {
        b.name = if b.host == Host::B {
            ib += 1;
            planet_name(&format!("{} B", star.name), ib - 1)
        } else {
            ia += 1;
            planet_name(&star.name, ia - 1)
        };
        for (k, m) in b.moons.iter_mut().enumerate() {
            m.name = format!("{} {}", b.name, (b'a' + k as u8) as char);
        }
    }

    // ---- tidal locking around red dwarfs ----
    let host_is_red_dwarf = |h: Host| {
        let s = match (h, &companion) {
            (Host::B, Some((c, _))) => &c.star,
            _ => &star,
        };
        s.class == StarClass::M && s.kind == StarKind::MainSequence
    };
    for b in protos.iter_mut() {
        if b.kind == Kind::Terrestrial && matches!(b.zone, 1 | 2) && host_is_red_dwarf(b.host) && percent(&mut rng, 15.0) {
            b.tidally_locked = true;
            b.surface = if b.temp == TempBand::Temperate {
                "Tidally locked: scorched dayside, frozen nightside, habitable twilight ring".into()
            } else {
                "Tidally locked: one face forever toward the star".into()
            };
        }
    }

    if !is_home {
        populate(&mut rng, &mut protos, opts);
    }

    // ---- into game bodies ----
    let mut planets: Vec<Planet> = protos.iter().map(|b| finalize(&mut rng, b, &star, companion.as_ref().map(|c| &c.0), opts)).collect();
    if is_home {
        curate_home(&mut planets, &star);
    }

    let civ = derive_civ(&planets);
    let (trade_prices, trade_stock) = market(&mut rng, civ.as_ref());

    let mut sys = StarSystem {
        id,
        pos,
        star,
        companion: companion.map(|c| c.0),
        planets,
        civ,
        trade_stock,
        trade_prices,
        market: None,
    };
    // Its own RNG stream, so adding cargo markets changed no world.
    sys.refresh_cargo_market(seed);
    sys
}

fn place(b: &mut Proto, host: Host, hz: f64) {
    b.host = host;
    b.au = bodies::units_to_au(b.orbit, hz);
    b.zone = bodies::zone_of(b.orbit);
    for m in b.moons.iter_mut() {
        m.host = host;
        m.au = b.au;
        m.zone = b.zone;
    }
}

/// Visit every body and moon mutably, with a stable index path.
fn for_each_body(protos: &mut [Proto], mut f: impl FnMut(&mut Proto)) {
    for b in protos.iter_mut() {
        f(b);
        for m in b.moons.iter_mut() {
            f(m);
        }
    }
}

fn body_mut(protos: &mut [Proto], at: (usize, Option<usize>)) -> &mut Proto {
    match at.1 {
        None => &mut protos[at.0],
        Some(m) => &mut protos[at.0].moons[m],
    }
}

fn all_paths(protos: &[Proto]) -> Vec<(usize, Option<usize>)> {
    let mut v = Vec::new();
    for (i, b) in protos.iter().enumerate() {
        v.push((i, None));
        for m in 0..b.moons.len() {
            v.push((i, Some(m)));
        }
    }
    v
}

/// Colony, life, sentients, societies and salvage.
fn populate(rng: &mut StdRng, protos: &mut [Proto], opts: &GalaxyOpts) {
    let paths = all_paths(protos);

    // ---- colony (first: Colony-Altered life depends on it) ----
    if percent(rng, colony::COLONY_CHANCE * opts.colony_mult()) {
        let cands: Vec<((usize, Option<usize>), f64)> = paths
            .iter()
            .filter_map(|&p| {
                let b = body_mut(protos, p);
                if b.kind != Kind::Terrestrial
                    || matches!(b.atmo, Some(AtmoKind::Corrosive | AtmoKind::Infiltrating))
                    || b.temp == TempBand::Burning
                {
                    return None;
                }
                let w = match (b.atmo, b.temp, b.is_moon) {
                    (_, _, true) => 1.0,
                    (Some(AtmoKind::Breathable), TempBand::Temperate, _) => 10.0,
                    (_, TempBand::Temperate, _) => 4.0,
                    _ => 2.0,
                };
                Some((p, w))
            })
            .collect();
        if !cands.is_empty() {
            let at = weighted(rng, &cands);
            let founders = life::species_name(rng);
            let c = colony::found_colony(rng, body_mut(protos, at), founders);
            body_mut(protos, at).colony = Some(c);
        }
    }

    // ---- life ----
    let habit = |b: &Proto| -> f64 {
        if !b.life_eligible() {
            return 0.0;
        }
        let mut s = 1.0;
        if b.temp == TempBand::Temperate {
            s += 3.0;
        }
        if b.atmo == Some(AtmoKind::Breathable) {
            s += 2.0;
        }
        if b.geo.is_some_and(|g| g.is_wet()) {
            s += 1.0;
        }
        if b.is_moon {
            s *= 0.5;
        }
        s
    };
    let best = paths
        .iter()
        .copied()
        .map(|p| (p, habit(body_mut(protos, p))))
        .filter(|(_, s)| *s > 0.0)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(p, _)| p);
    for &p in &paths {
        let scale = if Some(p) == best { 1.0 } else { life::BACKGROUND_LIFE };
        let b = body_mut(protos, p);
        b.life = life::roll_life(rng, b, scale);
        life::reconcile_terrain(rng, b);
        life::enforce_colony_requirement(b);
    }

    // ---- sentients, on the system's best world ----
    let Some(origin) = best else {
        salvage_pass(rng, protos);
        return;
    };
    let mut count = match body_mut(protos, origin).life {
        Some(l) => life::roll_sentients(rng, l, opts.sentient_mult()),
        None => 0,
    };
    if count > 0 {
        let tech = weighted(rng, &life::tech_weights(false, opts.advanced_tech_mult()));
        if tech == NativeTech::ExtinctRuins {
            let b = body_mut(protos, origin);
            if b.life.is_none() {
                b.life = Some(LifeType::Extinct);
            }
            let name = life::species_name(rng);
            b.natives = Some(life::society(rng, name, 0, tech, None));
        } else {
            // Further species may have evolved on another habitable world.
            let others: Vec<(usize, Option<usize>)> = paths
                .iter()
                .copied()
                .filter(|&p| p != origin && life::habitable_candidate(body_mut(protos, p)))
                .collect();
            let mut taken: Vec<(usize, Option<usize>)> = Vec::new();
            let mut elsewhere: Vec<((usize, Option<usize>), NativeTech)> = Vec::new();
            for id in 2..=count {
                let free: Vec<_> = others.iter().copied().filter(|p| !taken.contains(p)).collect();
                let ch = if id == 2 { 35.0 } else if free.len() >= 2 { 20.0 } else { 10.0 };
                if !free.is_empty() && percent(rng, ch) {
                    let p = free[life::pick_index(rng, free.len())];
                    taken.push(p);
                    elsewhere.push((p, weighted(rng, &life::tech_weights(true, opts.advanced_tech_mult()))));
                }
            }
            count -= elsewhere.len() as u8;
            let mut species: Vec<(String, NativeTech, (usize, Option<usize>))> = Vec::new();
            {
                let name = life::species_name(rng);
                let colony_tech = body_mut(protos, origin).colony.as_ref().map(|c| c.tech);
                let soc = life::society(rng, name.clone(), count, tech, colony_tech);
                body_mut(protos, origin).natives = Some(soc);
                species.push((name, tech, origin));
            }
            for (p, t) in elsewhere {
                let name = life::species_name(rng);
                let b = body_mut(protos, p);
                if !b.life.is_some_and(|l| l.sentient_odds()[0] > 0.0) {
                    b.life = Some(LifeType::Forests);
                }
                let colony_tech = b.colony.as_ref().map(|c| c.tech);
                let soc = if t == NativeTech::ExtinctRuins {
                    life::society(rng, name.clone(), 0, t, None)
                } else {
                    life::society(rng, name.clone(), 1, t, colony_tech)
                };
                body_mut(protos, p).natives = Some(soc);
                species.push((name, t, p));
            }
            // Spacefaring species settle the system's other good worlds.
            for (name, t, from) in species {
                let ch = life::expansion_chance(t);
                if ch <= 0.0 {
                    continue;
                }
                for &p in &others {
                    if p != from && percent(rng, ch) {
                        body_mut(protos, p).settlers.push(name.clone());
                    }
                }
                if from != origin && percent(rng, ch) {
                    body_mut(protos, origin).settlers.push(name.clone());
                }
            }
        }
    }
    salvage_pass(rng, protos);
}

fn salvage_pass(rng: &mut StdRng, protos: &mut [Proto]) {
    for_each_body(protos, |b| {
        let mut parts: Vec<String> = Vec::new();
        let mut kg = 0.0;
        let mut add = |rng: &mut StdRng, what: &str, lo: f64, hi: f64| {
            parts.push(what.to_string());
            kg += (lo + (hi - lo) * unit(rng)) * 1000.0;
        };
        if let Some(c) = &b.colony {
            match c.tech {
                ColonyTech::Derelict => add(rng, "Derelict colony", 20.0, 80.0),
                ColonyTech::Fallen => add(rng, "Abandoned districts of a fallen colony", 5.0, 30.0),
                _ => {}
            }
            let orbit = c.in_orbit.clone();
            if orbit.contains("Abandoned Space Station") {
                add(rng, "Abandoned space station", 10.0, 30.0);
            }
            if orbit.contains("Abandoned Satellite") {
                add(rng, "Abandoned satellite", 1.0, 3.0);
            }
            if orbit.contains("Wreckage") {
                add(rng, "Orbital wreckage", 5.0, 15.0);
            }
        }
        if let Some(n) = &b.natives {
            match n.tech {
                NativeTech::ExtinctRuins => add(rng, "Ruins of an extinct civilisation", 10.0, 60.0),
                NativeTech::PostApocSpace => add(rng, "Derelict native stations and orbital debris", 30.0, 100.0),
                NativeTech::PostApocAtomic => add(rng, "Dead native satellites", 3.0, 10.0),
                _ => {}
            }
        }
        if kg > 0.0 {
            b.salvage = Some(Salvage { what: parts.join("; "), remaining: kg, initial: kg });
        }
    });
}

fn host_star<'a>(host: Host, a: &'a Star, c: Option<&'a Companion>) -> &'a Star {
    match (host, c) {
        (Host::B, Some(c)) => &c.star,
        _ => a,
    }
}

fn finalize(rng: &mut StdRng, b: &Proto, a: &Star, comp: Option<&Companion>, opts: &GalaxyOpts) -> Planet {
    let s = host_star(b.host, a, comp);
    let (lum, mass) = match (b.host, comp) {
        (Host::AB, Some(c)) => (a.lum + c.star.lum, a.mass + c.star.mass),
        _ => (s.lum, s.mass),
    };
    let class = match b.kind {
        Kind::Terrestrial if b.is_moon => PlanetClass::Terrestrial,
        Kind::Terrestrial if b.size_km >= 14_000.0 => PlanetClass::SuperEarth,
        Kind::Terrestrial => PlanetClass::Terrestrial,
        Kind::Ice => PlanetClass::IceWorld,
        Kind::GasGiant if b.gg == Some(GgFeature::Small) => PlanetClass::IceGiant,
        Kind::GasGiant => PlanetClass::GasGiant,
        Kind::Belt => PlanetClass::AsteroidBelt,
    };
    let (radius, pmass, gravity, size_km) = match class {
        PlanetClass::GasGiant => {
            let mj: f64 = rng.random_range(0.4..15.0);
            let r = (mj.powf(0.25) * 11.2).min(12.5);
            (r, mj * 317.8, mj * 317.8 / (r * r), r * 12_742.0)
        }
        PlanetClass::IceGiant => {
            let m: f64 = rng.random_range(8.0..40.0);
            let r = m.powf(0.25) * 1.3;
            (r, m, m / (r * r), r * 12_742.0)
        }
        PlanetClass::AsteroidBelt => (0.0, 0.0, 0.0, 0.0),
        _ => {
            let r = b.size_km / 12_742.0;
            (r, b.gravity * r * r, b.gravity, b.size_km)
        }
    };
    let eq = 278.0 * lum.max(1e-9).powf(0.25) / b.au.max(1e-4).sqrt();
    let temp = match b.kind {
        Kind::Terrestrial | Kind::Ice => {
            let k = eq + b.atmo.map(|a| a.greenhouse_k()).unwrap_or(0.0);
            let (lo, hi) = match b.temp {
                TempBand::Frozen => (30.0, 223.0),
                TempBand::Cold => (223.0, 273.0),
                TempBand::Temperate => (273.0, 303.0),
                TempBand::Hot => (303.0, 353.0),
                TempBand::Burning => (353.0, 1500.0),
            };
            k.clamp(lo, hi)
        }
        _ => eq,
    };
    let atmo = b.atmo.map(|k| atmo_detail(rng, k));
    let bulk = match class {
        PlanetClass::GasGiant => "H/He envelope, metallic H",
        PlanetClass::IceGiant => "H/He + water/ammonia ices",
        PlanetClass::IceWorld => "rock and water ice",
        PlanetClass::AsteroidBelt => "rock, metal and ice fragments",
        _ => "iron core / silicate mantle",
    }
    .to_string();
    let deposits = deposits(rng, b, class, opts);
    let satellites: Vec<Planet> = b.moons.iter().map(|m| finalize(rng, m, a, comp, opts)).collect();
    Planet {
        name: b.name.clone(),
        class,
        mass: pmass,
        radius,
        au: b.au,
        period: (b.au.powi(3) / mass.max(0.02)).sqrt(),
        temp,
        gravity,
        atmo,
        bulk,
        moons: if b.kind == Kind::GasGiant { satellites.len() } else { b.moon_count },
        deposits,
        phase: rng.random_range(0.0..std::f64::consts::TAU),
        host: b.host,
        zone: b.zone,
        is_moon: b.is_moon,
        size_km,
        atmosphere: b.atmo,
        temp_band: b.temp,
        geosphere: b.geo,
        terrain: b.terrain.clone(),
        surface: b.surface.clone(),
        gg_feature: b.gg,
        belt_feature: b.belt,
        ring: b.ring,
        tidally_locked: b.tidally_locked,
        satellites,
        life: b.life,
        natives: b.natives.clone(),
        colony: b.colony.clone(),
        salvage: b.salvage.clone(),
        settlers: b.settlers.clone(),
    }
}

fn atmo_detail(rng: &mut StdRng, k: AtmoKind) -> Atmo {
    let r = |rng: &mut StdRng, lo: f64, hi: f64| lo + (hi - lo) * unit(rng);
    match k {
        AtmoKind::Thin => Atmo { pressure_bar: r(rng, 0.01, 0.3), n2: 0.8, o2: 0.0, co2: 0.15 },
        AtmoKind::Breathable => {
            let o2 = r(rng, 0.17, 0.3);
            Atmo { pressure_bar: r(rng, 0.7, 1.6), n2: 1.0 - o2 - 0.01, o2, co2: 0.01 }
        }
        AtmoKind::Toxic => {
            let co2 = r(rng, 0.3, 0.8);
            Atmo { pressure_bar: r(rng, 0.5, 4.0), n2: 1.0 - co2, o2: 0.0, co2 }
        }
        AtmoKind::Dense => Atmo { pressure_bar: r(rng, 5.0, 90.0), n2: 0.4, o2: 0.0, co2: 0.6 },
        AtmoKind::Corrosive => Atmo { pressure_bar: r(rng, 10.0, 90.0), n2: 0.05, o2: 0.0, co2: 0.95 },
        AtmoKind::Infiltrating => Atmo { pressure_bar: r(rng, 20.0, 100.0), n2: 0.05, o2: 0.0, co2: 0.95 },
    }
}

/// Deposit tuning (percent chances). Carbon and nitrogen are the scarce
/// feedstock: their rarity is what makes most systems waypoints rather than
/// bases, as the game has always been balanced.
mod tune {
    pub const TERR_CARBON: f64 = 12.0;
    pub const TERR_CARBON_LIFE: f64 = 40.0;
    pub const TERR_N_COLD: f64 = 3.0;
    pub const TERR_N_COLD_AIR: f64 = 5.0;
    pub const ICE_N: f64 = 3.0;
    pub const ICE_C: f64 = 5.0;
    pub const BIO_N: f64 = 10.0;
    pub const BELT_C: f64 = 10.0;
    pub const GIANT_N: f64 = 18.0;
    pub const GIANT_C: f64 = 35.0;
}

/// Bigger worlds hold bigger ore bodies: deposits scale with volume
/// (Earth = 1), so a 2,000 km moon is a pocket, not a mine.
fn size_factor(km: f64) -> f64 {
    (km / 12_742.0).powi(3).clamp(0.001, 4.0)
}

/// Mineable deposits (tons). Magnitudes follow the game's long-standing
/// balance; what a world holds now follows from what it is.
fn deposits(rng: &mut StdRng, b: &Proto, class: PlanetClass, opts: &GalaxyOpts) -> Vec<Deposit> {
    use tune::*;
    // The resources option scales how often optional deposits appear and
    // how big every deposit is.
    let (cm, tm) = (opts.deposit_chance_mult(), opts.deposit_tons_mult());
    let percent = |rng: &mut StdRng, p: f64| percent(rng, (p * cm).min(95.0));
    let mut d: BTreeMap<Res, f64> = BTreeMap::new();
    let mut add = |r: Res, t: f64| *d.entry(r).or_insert(0.0) += t * tm;
    match class {
        PlanetClass::Terrestrial | PlanetClass::SuperEarth => {
            let s = size_factor(b.size_km) * if class == PlanetClass::SuperEarth { 4.0 } else { 1.0 };
            add(Res::Metal, log10_range(rng, 4.0, 6.2) * s);
            add(Res::Silicate, log10_range(rng, 4.0, 6.2) * s);
            let biomass = b.life.is_some_and(|l| l.biomass());
            if percent(rng, if biomass { TERR_CARBON_LIFE } else { TERR_CARBON }) {
                add(Res::Carbon, log10_range(rng, 3.0, 4.5) * s);
            }
            let cold = matches!(b.temp, TempBand::Cold | TempBand::Frozen);
            if cold {
                add(Res::Ice, log10_range(rng, 4.0, 6.0) * s);
                let n_chance = if matches!(b.atmo, Some(AtmoKind::Dense | AtmoKind::Breathable)) { TERR_N_COLD_AIR } else { TERR_N_COLD };
                if percent(rng, n_chance) {
                    add(Res::Nitrogen, log10_range(rng, 3.0, 4.5) * s);
                }
            } else if b.temp != TempBand::Burning
                && let Some(g) = b.geo.filter(|g| g.is_wet()) {
                    add(Res::Ice, log10_range(rng, 3.5, 5.5) * s * g.sea_fraction() as f64);
                }
            if biomass {
                add(Res::Carbon, log10_range(rng, 3.5, 5.0) * s);
                if percent(rng, BIO_N) {
                    add(Res::Nitrogen, log10_range(rng, 3.0, 4.5) * s);
                }
            }
        }
        PlanetClass::IceWorld => {
            let s = size_factor(b.size_km);
            add(Res::Ice, log10_range(rng, 5.0, 6.5) * s);
            if percent(rng, ICE_N) {
                add(Res::Nitrogen, log10_range(rng, 4.0, 5.5) * s);
            }
            if percent(rng, ICE_C) {
                add(Res::Carbon, log10_range(rng, 3.5, 5.0) * s);
            }
            if percent(rng, 30.0) {
                add(Res::Metal, log10_range(rng, 3.5, 5.0) * s);
            }
        }
        PlanetClass::IceGiant => {
            add(Res::Ice, log10_range(rng, 5.0, 7.0));
            if percent(rng, GIANT_N) {
                add(Res::Nitrogen, log10_range(rng, 4.0, 6.0));
            }
            if percent(rng, GIANT_C) {
                add(Res::Carbon, log10_range(rng, 3.5, 5.0));
            }
            if percent(rng, 30.0) {
                add(Res::Metal, log10_range(rng, 3.5, 5.0));
            }
        }
        PlanetClass::GasGiant => {
            if b.ring {
                add(Res::Ice, log10_range(rng, 4.5, 6.0));
            }
        }
        PlanetClass::AsteroidBelt => {
            let k = match b.belt {
                Some(BeltFeature::Dust) => 0.2,
                Some(BeltFeature::Wide) => 2.0,
                _ => 1.0,
            };
            let metal_k = if b.belt == Some(BeltFeature::MineralRich) { 3.0 } else { 1.0 };
            add(Res::Metal, log10_range(rng, 5.0, 7.0) * k * metal_k);
            add(Res::Silicate, log10_range(rng, 5.0, 6.5) * k);
            if percent(rng, BELT_C) {
                add(Res::Carbon, log10_range(rng, 4.0, 5.5) * k);
            }
            if b.zone == 3 && percent(rng, 30.0) {
                add(Res::Ice, log10_range(rng, 4.5, 6.0) * k);
            }
        }
    }
    let mut v: Vec<Deposit> = d.into_iter().map(|(res, tons)| Deposit { res, tons }).collect();
    v.sort_by(|a, b| b.tons.total_cmp(&a.tons));
    v
}

/// The civilisation trade deals with: the most capable one in the system.
fn derive_civ(planets: &[Planet]) -> Option<Civ> {
    let mut best: Option<Civ> = None;
    let mut consider = |c: Civ| {
        if best.as_ref().is_none_or(|b| c.tier > b.tier) {
            best = Some(c);
        }
    };
    for (i, p) in planets.iter().enumerate() {
        let mut visit = |p: &Planet, seat: Loc| {
            if let Some(n) = &p.natives {
                let tier = match n.tech {
                    NativeTech::ExtinctRuins => None,
                    NativeTech::SpaceAge | NativeTech::Interplanetary => Some(CivTier::T2),
                    NativeTech::Interstellar | NativeTech::AdvancedInterstellar => Some(CivTier::T3),
                    _ => Some(CivTier::T1),
                };
                if let Some(tier) = tier {
                    consider(Civ {
                        tier,
                        name: n.species.clone(),
                        kind: CivKind::Natives,
                        seat,
                        small: false,
                    });
                }
            }
            if let Some(c) = &p.colony {
                let (tier, small) = match c.tech {
                    ColonyTech::FrontierOutpost => (Some(CivTier::T2), true),
                    // A survey post is scientists and sensors: nothing to sell.
                    ColonyTech::SurveyInfrastructure => (Some(CivTier::T1), false),
                    ColonyTech::Settled => (Some(CivTier::T2), false),
                    ColonyTech::Industrial | ColonyTech::Advanced => (Some(CivTier::T3), false),
                    ColonyTech::Fallen => (Some(CivTier::T1), false),
                    ColonyTech::Derelict => (None, false),
                };
                if let Some(tier) = tier {
                    consider(Civ { tier, name: c.name.clone(), kind: CivKind::Colony, seat, small });
                }
            }
        };
        visit(p, Loc::Planet(i));
        for (m, s) in p.satellites.iter().enumerate() {
            visit(s, Loc::Moon(i, m));
        }
    }
    best
}

fn market(rng: &mut StdRng, civ: Option<&Civ>) -> (BTreeMap<Res, f64>, BTreeMap<Res, f64>) {
    let mut prices = BTreeMap::new();
    let mut stock = BTreeMap::new();
    match civ.map(|c| (c.tier, c.small)) {
        Some((CivTier::T3, _)) => {
            for r in Res::ALL {
                prices.insert(r, base_price(r) * rng.random_range(0.7..1.4));
                stock.insert(r, 2_000_000.0);
            }
        }
        Some((CivTier::T2, small)) => {
            for r in Res::ALL {
                prices.insert(r, base_price(r) * rng.random_range(0.7..1.4));
            }
            for (r, amt) in T2_STOCK {
                stock.insert(r, amt * rng.random_range(0.5..1.5) * if small { 0.5 } else { 1.0 });
            }
        }
        _ => {}
    }
    (prices, stock)
}

/// The player's start: a friendly G dwarf whose system is guaranteed
/// workable — fuel (a gas giant), a big ice giant (water, nitrogen, carbon)
/// and rocky inner worlds with metal, silicate and carbon.
fn curate_home(planets: &mut Vec<Planet>, star: &Star) {
    let hz = star.habitable_au();
    let blank = |name: String, class: PlanetClass, au: f64| Planet {
        name,
        class,
        mass: 1.0,
        radius: 1.0,
        au,
        period: (au.powi(3) / star.mass).sqrt(),
        temp: star.temp_at(au),
        gravity: 1.0,
        atmo: None,
        bulk: String::new(),
        moons: 0,
        deposits: vec![],
        phase: au * 2.3,
        host: Host::A,
        zone: if au < 0.7 * hz { 1 } else if au < 1.3 * hz { 2 } else { 3 },
        is_moon: false,
        size_km: 12_742.0,
        atmosphere: None,
        temp_band: bodies::band_of(au / hz * 5000.0),
        geosphere: None,
        terrain: vec![],
        surface: String::new(),
        gg_feature: None,
        belt_feature: None,
        ring: false,
        tidally_locked: false,
        satellites: vec![],
        life: None,
        natives: None,
        colony: None,
        salvage: None,
        settlers: vec![],
    };

    // 1) A gas giant (fuel + N2 scoop).
    if !planets.iter().any(|p| p.class == PlanetClass::GasGiant) {
        let mut g = blank(String::new(), PlanetClass::GasGiant, hz * 6.4);
        g.mass = 318.0;
        g.radius = 11.2;
        g.size_km = 11.2 * 12_742.0;
        g.gravity = 2.5;
        g.bulk = "H/He envelope, metallic H".into();
        g.gg_feature = Some(GgFeature::Storms);
        planets.push(g);
    }
    // 2) A big ice giant: water, nitrogen and carbon in one place.
    let ice_idx = match planets.iter().position(|p| p.class == PlanetClass::IceGiant) {
        Some(i) => i,
        None => {
            let mut g = blank(String::new(), PlanetClass::IceGiant, hz * 4.1);
            g.mass = 17.0;
            g.radius = 3.9;
            g.size_km = 3.9 * 12_742.0;
            g.gravity = 1.1;
            g.bulk = "H/He + water/ammonia ices".into();
            g.gg_feature = Some(GgFeature::Small);
            planets.push(g);
            planets.len() - 1
        }
    };
    ensure(&mut planets[ice_idx], &[(Res::Ice, 320_000.0), (Res::Nitrogen, 60_000.0), (Res::Carbon, 25_000.0)]);
    // 3) Workable metals, silicates and carbon on the inner rocky worlds.
    planets.sort_by(|a, b| a.au.total_cmp(&b.au));
    let rocky: Vec<usize> = planets
        .iter()
        .enumerate()
        .filter(|(_, p)| matches!(p.class, PlanetClass::Terrestrial | PlanetClass::SuperEarth))
        .map(|(i, _)| i)
        .take(3)
        .collect();
    if rocky.is_empty() {
        let mut r = blank(String::new(), PlanetClass::Terrestrial, hz * 0.6);
        r.atmosphere = Some(AtmoKind::Thin);
        r.temp_band = TempBand::Hot;
        r.terrain = vec!["Huge impact crater".into(), "Meteor-scarred highlands".into()];
        r.surface = "Scorched, cratered rock".into();
        planets.insert(0, r);
        ensure(&mut planets[0], &[(Res::Metal, 40_000.0), (Res::Silicate, 40_000.0), (Res::Carbon, 10_000.0)]);
    } else {
        for (k, &i) in rocky.iter().enumerate() {
            let p = &mut planets[i];
            ensure(p, &[(Res::Metal, 8_000.0), (Res::Silicate, 8_000.0)]);
            if k < 2 {
                ensure(p, &[(Res::Carbon, 5_000.0)]);
            }
            // The home rocks must be safe to work.
            if matches!(p.atmosphere, Some(AtmoKind::Corrosive | AtmoKind::Infiltrating)) {
                p.atmosphere = Some(AtmoKind::Toxic);
            }
        }
    }
    for (i, p) in planets.iter_mut().enumerate() {
        let old = p.name.clone();
        p.name = planet_name(&star.name, i);
        for (k, m) in p.satellites.iter_mut().enumerate() {
            m.name = format!("{} {}", p.name, (b'a' + k as u8) as char);
        }
        let _ = old;
    }
}

fn ensure(p: &mut Planet, want: &[(Res, f64)]) {
    for &(r, min) in want {
        match p.deposits.iter_mut().find(|d| d.res == r) {
            Some(d) if d.tons < min => d.tons = min,
            Some(_) => {}
            None => p.deposits.push(Deposit { res: r, tons: min }),
        }
    }
    p.deposits.sort_by(|a, b| b.tons.total_cmp(&a.tons));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::galaxy::Galaxy;
    use crate::sim::planet::Geosphere;

    fn sample(seed: u64, r: i64) -> Vec<StarSystem> {
        sample_with(seed, r, GalaxyOpts::default())
    }

    fn sample_with(seed: u64, r: i64, opts: GalaxyOpts) -> Vec<StarSystem> {
        let mut gal = Galaxy::with_opts(seed, opts);
        let mut out = Vec::new();
        for cx in -r..r {
            for cy in -r..r {
                for id in gal.ids_in_cell(cx, cy) {
                    out.push(gal.system(id).clone());
                }
            }
        }
        out
    }

    fn all_bodies(s: &StarSystem) -> Vec<&Planet> {
        s.sites().into_iter().map(|(_, p)| p).collect()
    }

    /// The galaxy is a pure function of (seed, id): the same system twice.
    #[test]
    fn generation_is_deterministic() {
        let mut g1 = Galaxy::new(77);
        let mut g2 = Galaxy::new(77);
        for id in g1.ids_in_cell(3, -4).into_iter().chain(g1.ids_in_cell(0, 0)) {
            let a = serde_json::to_string(g1.system(id)).unwrap();
            let b = serde_json::to_string(g2.system(id)).unwrap();
            assert_eq!(a, b);
        }
    }

    /// Holman–Wiegert limits are respected: nothing orbits where a binary
    /// companion would have thrown it out.
    #[test]
    fn binary_orbits_are_stable() {
        let mut binaries = 0;
        for s in sample(5, 30) {
            let Some(c) = &s.companion else { continue };
            binaries += 1;
            for p in &s.planets {
                match p.host {
                    Host::A => assert!(!c.p_type && p.au <= c.limit_a + 1e-9, "{} A-body beyond limit", s.star.name),
                    Host::B => assert!(!c.p_type && p.au <= c.limit_b + 1e-9, "{} B-body beyond limit", s.star.name),
                    Host::AB => assert!(c.p_type && p.au >= c.circumbinary - 1e-9, "{} inside circumbinary limit", s.star.name),
                }
            }
        }
        assert!(binaries > 50, "too few binaries sampled: {binaries}");
    }

    /// Life and terrain stay consistent with each other and with colonies.
    #[test]
    fn life_and_terrain_rules_hold() {
        for s in sample(9, 25) {
            for p in all_bodies(&s) {
                if p.life == Some(LifeType::ColonyAltered) {
                    assert!(p.colony.is_some(), "{}: colony-altered life without a colony", p.name);
                }
                if let Some(n) = &p.natives
                    && n.tech != NativeTech::ExtinctRuins {
                        assert!(n.species_count > 0, "{}: living natives with no species", p.name);
                        assert!(p.life.is_some_and(|l| l.sentient_odds()[0] > 0.0), "{}: natives on a world that can't evolve them", p.name);
                    }
                let bio_needed = p.terrain.iter().any(|t| terrain::entry(t).is_some_and(|e| e.tags & terrain::BIO != 0));
                if bio_needed {
                    assert!(p.life.is_some_and(|l| l.visible() || l == LifeType::Ocean), "{}: living terrain on a dead world", p.name);
                }
                for t in &p.terrain {
                    let Some(e) = terrain::entry(t) else {
                        assert!(terrain::ICE_CATALOG.contains(&t.as_str()), "{}: unknown terrain {t}", p.name);
                        continue;
                    };
                    if e.tags & terrain::MOON_ONLY != 0 {
                        assert!(p.is_moon, "{}: moon-only terrain on a planet", p.name);
                    }
                    if e.tags & terrain::MOON != 0 {
                        assert!(!p.is_moon && p.moons > 0, "{}: moon terrain without moons", p.name);
                    }
                    if e.tags & terrain::WATER != 0 {
                        assert!(p.temp_band != TempBand::Burning && p.geosphere != Some(Geosphere::Desert), "{}: water terrain on a dry world", p.name);
                    }
                    if e.tags & terrain::LAND != 0 {
                        assert!(p.geosphere != Some(Geosphere::Water), "{}: land terrain on a water world", p.name);
                    }
                    if e.tags & terrain::CLOUD != 0 {
                        assert!(p.atmosphere != Some(AtmoKind::Thin), "{}: cloud terrain under a thin atmosphere", p.name);
                    }
                }
            }
        }
    }

    /// Civilisations stay about as rare as the game has always had them:
    /// trade is a rare respite, not a supermarket on every corner.
    #[test]
    fn civilisation_rarity_is_preserved() {
        let systems: Vec<StarSystem> = sample(11, 50);
        let n = systems.len() as f64;
        let count = |f: &dyn Fn(&StarSystem) -> bool| systems.iter().filter(|s| f(s)).count() as f64 / n * 100.0;
        let any = count(&|s| s.civ.is_some());
        let t2 = count(&|s| s.civ.as_ref().is_some_and(|c| c.tier == CivTier::T2));
        let t3 = count(&|s| s.civ.as_ref().is_some_and(|c| c.tier == CivTier::T3));
        assert!((1.0..=4.0).contains(&any), "any civ {any:.2}%");
        assert!((0.4..=1.2).contains(&t2), "T2 {t2:.2}%");
        assert!((0.15..=0.6).contains(&t3), "T3 {t3:.2}%");
    }

    /// The home system is always workable and civilisation-free.
    #[test]
    fn home_is_curated() {
        for seed in [1u64, 7, 42, 999] {
            let mut gal = Galaxy::new(seed);
            let s = gal.system(gal.home_id()).clone();
            assert!(s.civ.is_none() && s.companion.is_none());
            assert!(s.planets.iter().any(|p| p.class == PlanetClass::GasGiant));
            assert!(s.planets.iter().any(|p| p.class == PlanetClass::IceGiant
                && p.deposits.iter().any(|d| d.res == Res::Nitrogen && d.tons >= 60_000.0)));
        }
    }

    fn opts(resources: crate::sim::Abundance, civs: crate::sim::Abundance) -> GalaxyOpts {
        GalaxyOpts { resources, civs, ..GalaxyOpts::default() }
    }

    /// Percent of systems with any civ, and with a trading (T2/T3) one.
    fn civ_rates(systems: &[StarSystem]) -> (f64, f64) {
        let n = systems.len() as f64;
        let any = systems.iter().filter(|s| s.civ.is_some()).count() as f64 / n * 100.0;
        let trade = systems.iter().filter(|s| s.civ.as_ref().is_some_and(|c| c.tier >= CivTier::T2)).count() as f64 / n * 100.0;
        (any, trade)
    }

    /// The civilisation option crowds the galaxy step by step, and more of
    /// the extra civs are ones you can trade with.
    #[test]
    fn civilisation_option_scales_civs() {
        use crate::sim::Abundance::*;
        let rates: Vec<(f64, f64)> = [Scarce, Normal, Abundant]
            .into_iter()
            .map(|c| civ_rates(&sample_with(11, 50, opts(Normal, c))))
            .collect();
        assert!(rates[0].0 < rates[1].0 && rates[1].0 < rates[2].0, "{rates:?}");
        assert!(rates[0].1 < rates[1].1 && rates[1].1 < rates[2].1, "{rates:?}");
        assert!((5.0..=12.0).contains(&rates[1].0), "Normal any civ {:.2}%", rates[1].0);
        assert!((14.0..=28.0).contains(&rates[2].0), "Abundant any civ {:.2}%", rates[2].0);
    }

    /// The resources option scales how many carbon/nitrogen deposits exist
    /// and how much is in them.
    #[test]
    fn resource_option_scales_deposits() {
        use crate::sim::Abundance::*;
        let stats: Vec<(usize, f64)> = [Scarce, Normal, Abundant]
            .into_iter()
            .map(|r| {
                let systems = sample_with(3, 20, opts(r, Scarce));
                let mut count = 0;
                let mut tons = 0.0;
                for s in &systems {
                    for p in all_bodies(s) {
                        for d in &p.deposits {
                            if matches!(d.res, Res::Nitrogen | Res::Carbon) {
                                count += 1;
                                tons += d.tons;
                            }
                        }
                    }
                }
                (count, tons)
            })
            .collect();
        assert!(stats[0].0 < stats[1].0 && stats[1].0 < stats[2].0, "{stats:?}");
        assert!(stats[0].1 < stats[1].1 && stats[1].1 < stats[2].1, "{stats:?}");
    }

    /// Options change what systems hold, not where the stars are; and the
    /// same seed with the same options is always the same galaxy.
    #[test]
    fn options_keep_the_star_map() {
        use crate::sim::Abundance::*;
        let rich = opts(Abundant, Abundant);
        let mut a = Galaxy::with_opts(77, rich);
        let mut b = Galaxy::with_opts(77, rich);
        let mut plain = Galaxy::new(77);
        for id in a.ids_in_cell(3, -4).into_iter().chain(a.ids_in_cell(1, 2)) {
            assert_eq!(serde_json::to_string(a.system(id)).unwrap(), serde_json::to_string(b.system(id)).unwrap());
            assert_eq!(a.system(id).star.name, plain.system(id).star.name);
            assert_eq!(a.system(id).pos, plain.system(id).pos);
        }
    }

    /// The home system stays workable however scarce resources are.
    #[test]
    fn home_is_curated_at_every_resource_level() {
        for r in crate::sim::Abundance::ALL {
            let mut gal = Galaxy::with_opts(9, opts(r, crate::sim::Abundance::Abundant));
            let s = gal.system(gal.home_id()).clone();
            assert!(s.civ.is_none());
            assert!(s.planets.iter().any(|p| p.class == PlanetClass::IceGiant
                && p.deposits.iter().any(|d| d.res == Res::Nitrogen && d.tons >= 60_000.0)));
        }
    }
}
