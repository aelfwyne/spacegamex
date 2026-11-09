//! Body rolls (port of the body half of `Generator.gs`): gas giants and their
//! moon systems, terrestrial planets, ice planets and asteroid belts.

use super::dice::{self, below, d6, percent, r2d6};
use super::stars::Mods;
use super::terrain::{self, Ctx};
use crate::sim::planet::{
    AtmoKind, BeltFeature, Colony, GgFeature, Geosphere, Host, LifeType, NativeSociety, Salvage, TempBand,
};
use rand::rngs::StdRng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Terrestrial,
    Ice,
    GasGiant,
    Belt,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BodyMods {
    pub size: i32,
    pub atmo: i32,
    pub temp: i32,
    pub geo: i32,
    pub terrain: i32,
    pub colony: i32,
    pub mission: i32,
    pub orbit: i32,
}

/// A body while it is being generated (orbit in the old 0–10,000 "units",
/// where 5,000 is the habitable zone).
#[derive(Debug, Clone)]
pub struct Proto {
    pub kind: Kind,
    pub is_moon: bool,
    pub mods: BodyMods,
    pub size_roll: i32,
    pub size_km: f64,
    pub gravity: f64,
    pub atmo: Option<AtmoKind>,
    pub temp: TempBand,
    pub geo: Option<Geosphere>,
    pub terrain: Vec<String>,
    pub surface: String,
    pub orbit: f64,
    pub au: f64,
    pub host: Host,
    pub zone: u8,
    pub gg: Option<GgFeature>,
    pub belt: Option<BeltFeature>,
    pub moon_count: usize,
    pub ring: bool,
    pub moons: Vec<Proto>,
    pub tidally_locked: bool,
    pub life: Option<LifeType>,
    pub natives: Option<NativeSociety>,
    pub colony: Option<Colony>,
    pub salvage: Option<Salvage>,
    pub settlers: Vec<String>,
    pub name: String,
}

impl Proto {
    fn new(kind: Kind) -> Proto {
        Proto {
            kind,
            is_moon: false,
            mods: BodyMods::default(),
            size_roll: 7,
            size_km: 0.0,
            gravity: 0.0,
            atmo: None,
            temp: TempBand::Temperate,
            geo: None,
            terrain: vec![],
            surface: String::new(),
            orbit: 5000.0,
            au: 1.0,
            host: Host::A,
            zone: 2,
            gg: None,
            belt: None,
            moon_count: 0,
            ring: false,
            moons: vec![],
            tidally_locked: false,
            life: None,
            natives: None,
            colony: None,
            salvage: None,
            settlers: vec![],
            name: String::new(),
        }
    }

    /// Can this body support visible surface (or sky) life?
    pub fn life_eligible(&self) -> bool {
        match self.kind {
            Kind::GasGiant => {
                zone_of(self.orbit) == 2
                    && !matches!(self.gg, Some(GgFeature::SingleSuperStorm | GgFeature::IntenseRadiation))
            }
            Kind::Terrestrial => {
                !matches!(self.atmo, Some(AtmoKind::Corrosive | AtmoKind::Infiltrating))
                    && matches!(self.temp, TempBand::Cold | TempBand::Temperate | TempBand::Hot)
            }
            _ => false,
        }
    }

    pub fn ctx(&self) -> Ctx {
        Ctx {
            temp: self.temp,
            geo: self.geo,
            atmo: self.atmo,
            is_moon: self.is_moon,
            life_eligible: self.life_eligible(),
        }
    }
}

pub fn zone_of(orbit: f64) -> u8 {
    if orbit < 3500.0 {
        1
    } else if orbit < 6500.0 {
        2
    } else {
        3
    }
}

/// Temperature band an orbit falls in (for gas giants and belts).
pub fn band_of(orbit: f64) -> TempBand {
    match orbit {
        o if o < 2000.0 => TempBand::Burning,
        o if o < 4000.0 => TempBand::Hot,
        o if o < 6000.0 => TempBand::Temperate,
        o if o < 8000.0 => TempBand::Cold,
        _ => TempBand::Frozen,
    }
}

fn roll_size(rng: &mut StdRng, b: &mut Proto) {
    let roll = r2d6(rng) + b.mods.size;
    let (km, g) = match roll {
        r if r <= 2 => (1_000.0, 0.02),
        r if r <= 4 => (2_000.0, 0.1),
        r if r <= 6 => (4_000.0, 0.2),
        7 => (7_000.0, 0.5),
        8 => (10_000.0, 0.7),
        r if r <= 10 => (12_500.0, 1.0),
        11 => (15_000.0, 1.3),
        _ => (20_000.0, 2.0),
    };
    // A little spread so worlds of one size class are not identical.
    let jitter = 0.9 + 0.2 * dice::unit(rng);
    b.size_km = km * jitter;
    b.gravity = g * jitter;
    if roll <= 6 {
        b.mods.atmo -= 6;
        b.mods.colony -= 3;
    } else if roll <= 7 {
        b.mods.atmo -= 2;
    }
    b.size_roll = roll;
}

fn roll_atmosphere(rng: &mut StdRng, b: &mut Proto) {
    let roll = r2d6(rng) + b.mods.atmo;
    let a = if roll <= 3 {
        b.mods.temp -= 4;
        b.mods.geo -= 4;
        AtmoKind::Thin
    } else if roll <= 6 {
        b.mods.colony += 1;
        b.mods.mission += 1;
        AtmoKind::Breathable
    } else if roll <= 8 {
        b.mods.mission -= 6;
        AtmoKind::Toxic
    } else if roll <= 9 {
        b.mods.temp += 1;
        AtmoKind::Dense
    } else {
        b.mods.temp += 6;
        b.mods.geo -= 4;
        b.mods.colony -= 2;
        b.mods.mission -= 6;
        if roll <= 10 { AtmoKind::Corrosive } else { AtmoKind::Infiltrating }
    };
    b.atmo = Some(a);
}

fn roll_temperature(rng: &mut StdRng, b: &mut Proto, forced: Option<i32>) {
    let roll = forced.unwrap_or_else(|| r2d6(rng) + b.mods.temp);
    let (band, base) = if roll <= 3 {
        b.mods.geo -= 2;
        b.mods.terrain -= 2;
        (TempBand::Frozen, 8000.0)
    } else if roll <= 5 {
        (TempBand::Cold, 6000.0)
    } else if roll <= 7 {
        b.mods.geo += 2;
        (TempBand::Temperate, 4000.0)
    } else if roll <= 10 {
        b.mods.geo -= 2;
        (TempBand::Hot, 2000.0)
    } else {
        b.mods.geo -= 4;
        (TempBand::Burning, 0.0)
    };
    b.temp = band;
    b.orbit = base + below(rng, 2000) as f64;
}

fn roll_geosphere(rng: &mut StdRng, b: &mut Proto) {
    let roll = r2d6(rng) + b.mods.geo;
    let g = if roll <= 4 {
        b.mods.terrain -= 3;
        Geosphere::Desert
    } else if roll <= 6 {
        b.mods.terrain -= 2;
        Geosphere::Arid
    } else if roll <= 8 {
        Geosphere::TemperateDry
    } else if roll <= 10 {
        Geosphere::TemperateWet
    } else if roll <= 11 {
        b.mods.terrain += 2;
        Geosphere::Wet
    } else {
        b.mods.terrain += 3;
        Geosphere::Water
    };
    b.geo = Some(g);
}

fn roll_terrain(rng: &mut StdRng, b: &mut Proto) {
    let ctx = b.ctx();
    let count = terrain::feature_count(&ctx);
    b.terrain = if b.temp == TempBand::Frozen {
        terrain::ice_features(rng, count)
    } else {
        terrain::pick_features(rng, &ctx, count, true)
    };
    b.surface = terrain::surface_layout(rng, &ctx, false);
}

fn roll_ice_terrain(rng: &mut StdRng, b: &mut Proto) {
    let ctx = b.ctx();
    let count = terrain::feature_count(&ctx);
    b.terrain = terrain::ice_features(rng, count);
    b.surface = terrain::surface_layout(rng, &ctx, b.kind == Kind::Ice && !b.is_moon);
}

/// Natural rings and moons (port of `rollAlienNaturalOrbit_`).
fn roll_natural_orbit(rng: &mut StdRng, b: &mut Proto) {
    let roll = r2d6(rng);
    match roll {
        5 => b.ring = true,
        6..=9 => b.moon_count = 1 + below(rng, 3) as usize,
        10 | 11 => {
            b.ring = true;
            b.moon_count = 1 + below(rng, 3) as usize;
        }
        _ => {}
    }
    // "Huge moon dominates the sky" / "fast moons" terrain needs real moons.
    if terrain::has_tag(&b.terrain, terrain::MOON) && b.moon_count == 0 {
        b.moon_count = if b.terrain.iter().any(|t| t.contains("fast moons")) { 2 + below(rng, 2) as usize } else { 1 };
    }
}

/// All bodies for one star (port of `buildAlienStarPlanets_`), plus the
/// gas giants' moon systems (`addGasGiantMoonSystems_`).
pub fn star_bodies(rng: &mut StdRng, mods: &Mods) -> Vec<Proto> {
    let mut bodies = Vec::new();

    let gas = (d6(rng) - 1 + mods.jovian).max(0);
    for _ in 0..gas {
        let mut g = Proto::new(Kind::GasGiant);
        g.orbit = if percent(rng, 10.0) { 1000.0 + below(rng, 1500) as f64 } else { 2500.0 + below(rng, 7500) as f64 };
        g.mods.orbit = match g.orbit {
            o if o < 2000.0 => 5,
            o if o < 4000.0 => 2,
            o if o < 6000.0 => 0,
            o if o < 8000.0 => -2,
            _ => -4,
        };
        g.gg = Some(GgFeature::ALL[(d6(rng) - 1) as usize]);
        g.ring = g.gg == Some(GgFeature::Rings);
        g.temp = band_of(g.orbit);
        g.moon_count = (d6(rng) + 4) as usize;
        for _ in 0..g.moon_count {
            let mut m = Proto::new(Kind::Terrestrial);
            m.is_moon = true;
            m.mods.size -= 2;
            m.mods.temp += g.mods.orbit;
            roll_size(rng, &mut m);
            roll_atmosphere(rng, &mut m);
            roll_temperature(rng, &mut m, None);
            if m.temp == TempBand::Frozen {
                m.kind = Kind::Ice;
                m.terrain = terrain::ice_features(rng, 1);
            } else {
                roll_geosphere(rng, &mut m);
                roll_terrain(rng, &mut m);
            }
            g.moons.push(m);
        }
        bodies.push(g);
    }

    let terr = (d6(rng) + mods.terrestrial).max(0);
    for _ in 0..terr {
        let mut b = Proto::new(Kind::Terrestrial);
        roll_size(rng, &mut b);
        roll_atmosphere(rng, &mut b);
        roll_temperature(rng, &mut b, None);
        roll_geosphere(rng, &mut b);
        roll_terrain(rng, &mut b);
        roll_natural_orbit(rng, &mut b);
        bodies.push(b);
    }

    let ice = (d6(rng) + 1 + mods.ice).max(0);
    for _ in 0..ice {
        let mut b = Proto::new(Kind::Ice);
        b.mods.size -= 2;
        roll_size(rng, &mut b);
        roll_atmosphere(rng, &mut b);
        roll_temperature(rng, &mut b, Some(2));
        roll_ice_terrain(rng, &mut b);
        roll_natural_orbit(rng, &mut b);
        bodies.push(b);
    }

    let belts = (d6(rng) - 3 + mods.asteroid).max(0);
    for _ in 0..belts {
        let mut b = Proto::new(Kind::Belt);
        b.orbit = 1000.0 * r2d6(rng) as f64;
        b.temp = band_of(b.orbit);
        b.belt = Some(BeltFeature::ALL[(d6(rng) - 1) as usize]);
        bodies.push(b);
    }

    bodies
}

/// Old orbit units → AU around a star whose habitable zone is `hz` AU.
/// Linear up to the frost line (which matches `Star::temp_at`), then an
/// exponential tail so outer worlds lie properly far out.
pub fn units_to_au(orbit: f64, hz: f64) -> f64 {
    let o = orbit.max(150.0);
    if o <= 8000.0 {
        hz * o / 5000.0
    } else if o <= 10_000.0 {
        hz * 1.6 * 16f64.powf((o - 8000.0) / 2000.0)
    } else {
        hz * 25.6 * (o / 10_000.0)
    }
}
