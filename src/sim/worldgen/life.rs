//! Native life, sentient species and native societies (port of
//! `Nativelife.gs`, `LifeTech.gs`, `Sentients.gs` and `Natives.gs`).
//!
//! There is no hand-picked "target world" in the game: each system's most
//! habitable body plays that role (full life chance, sentient roll), and
//! every other body gets the old background roll.

use super::bodies::{Kind, Proto};
use super::dice::{self, percent, r2d6, weighted};
use super::terrain;
use crate::rngx;
use crate::sim::planet::{AtmoKind, Geosphere, LifeType, NativeSociety, NativeTech, TempBand};
use rand::rngs::StdRng;

/// Global knob on sentient odds: keeps native civilisations about as rare
/// as the game's civilisations have always been.
pub const SENTIENT_SCALE: f64 = 0.5;
/// Background bodies' life chance relative to the system's best world.
pub const BACKGROUND_LIFE: f64 = 0.35;

#[derive(PartialEq)]
enum Context {
    None,
    Aerial,
    Frozen,
    Living,
    Extreme,
}

fn context(b: &Proto) -> Context {
    match b.kind {
        Kind::Belt => Context::None,
        Kind::GasGiant => {
            if b.life_eligible() { Context::Aerial } else { Context::None }
        }
        Kind::Ice => Context::Frozen,
        Kind::Terrestrial if b.temp == TempBand::Frozen => Context::Frozen,
        Kind::Terrestrial => {
            if b.life_eligible() { Context::Living } else { Context::Extreme }
        }
    }
}

/// Percent chance a body has native life.
fn chance(b: &Proto) -> f64 {
    match context(b) {
        Context::Aerial => 4.0,
        Context::Frozen => {
            if terrain::subsurface_ocean(&b.terrain) { 20.0 } else { 2.0 }
        }
        Context::Extreme => 2.0,
        Context::None => 0.0,
        Context::Living => {
            let base = match b.atmo {
                Some(AtmoKind::Breathable) => 30.0,
                Some(AtmoKind::Dense) => 8.0,
                Some(AtmoKind::Thin) => 6.0,
                Some(AtmoKind::Toxic) => 5.0,
                _ => 0.0,
            };
            if b.temp == TempBand::Temperate { base } else { (base * 0.35_f64).round() }
        }
    }
}

fn weights(b: &Proto, colony_active: bool) -> Vec<(LifeType, f64)> {
    use LifeType::*;
    match context(b) {
        Context::Aerial => vec![(SkyPlankton, 55.0), (SkyEcosystem, 30.0), (SkyLeviathans, 15.0)],
        Context::Frozen => vec![(Subsurface, 70.0), (Microbial, 30.0)],
        Context::Extreme => vec![(Microbial, 60.0), (Subsurface, 40.0)],
        Context::None => vec![],
        Context::Living => {
            let ocean = match b.geo {
                Some(Geosphere::Water) => 45.0,
                Some(Geosphere::Wet) => 18.0,
                Some(Geosphere::TemperateWet) => 12.0,
                Some(Geosphere::TemperateDry) => 5.0,
                Some(Geosphere::Arid) => 2.0,
                _ => 0.0,
            };
            let water = b.geo == Some(Geosphere::Water);
            let col = |w: f64| if colony_active { w } else { 0.0 };
            if b.temp == TempBand::Temperate {
                vec![
                    (Microbial, 8.0),
                    (Simple, 22.0),
                    (Ocean, ocean),
                    (Subsurface, 3.0),
                    (Forests, if water { 8.0 } else { 25.0 }),
                    (Giant, if water { 2.0 } else { 8.0 }),
                    (Hostile, 8.0),
                    (Extinct, 5.0),
                    (ColonyAltered, col(25.0)),
                ]
            } else {
                vec![
                    (Microbial, 30.0),
                    (Simple, 30.0),
                    (Ocean, (ocean / 2.0_f64).round()),
                    (Subsurface, 12.0),
                    (Forests, 6.0),
                    (Giant, 1.0),
                    (Hostile, 2.0),
                    (Extinct, 8.0),
                    (ColonyAltered, col(8.0)),
                ]
            }
        }
    }
}

pub fn roll_life(rng: &mut StdRng, b: &Proto, scale: f64) -> Option<LifeType> {
    let c = chance(b) * scale;
    if c <= 0.0 || !percent(rng, c) {
        return None;
    }
    let colony_active = b.colony.as_ref().is_some_and(|c| c.tech.active());
    let w = weights(b, colony_active);
    if w.iter().all(|(_, x)| *x <= 0.0) {
        return None;
    }
    Some(weighted(rng, &w))
}

const SEA_FEATURES: [&str; 5] = [
    "Toxic sea creatures floating with the currents",
    "Reef shelves stretching for thousands of kilometres",
    "Bioluminescent coastal waters",
    "Kelp-like forests in shallow seas",
    "Floating rafts of vegetation on calm seas",
];

fn life_fits_feature(life: Option<LifeType>, feature: &str) -> bool {
    if !terrain::entry(feature).is_some_and(|e| e.tags & terrain::BIO != 0) {
        return true;
    }
    match life {
        Some(l) if l.visible() => true,
        Some(LifeType::Ocean) => SEA_FEATURES.contains(&feature),
        _ => false,
    }
}

/// Forests, jungles and sea creatures mean life: either upgrade the life to
/// a full ecosystem or, where life is impossible, reroll those features.
pub fn reconcile_terrain(rng: &mut StdRng, b: &mut Proto) {
    if !terrain::has_tag(&b.terrain, terrain::BIO) {
        return;
    }
    if b.terrain.iter().all(|f| life_fits_feature(b.life, f)) {
        return;
    }
    if !b.life_eligible() {
        let ctx = b.ctx();
        for i in 0..b.terrain.len() {
            if !life_fits_feature(b.life, &b.terrain[i]) {
                let existing = b.terrain.clone();
                b.terrain[i] = terrain::pick_replacement(rng, &ctx, &existing, false);
            }
        }
        return;
    }
    let land: Vec<&String> = b.terrain.iter().filter(|f| !life_fits_feature(Some(LifeType::Ocean), f)).collect();
    if land.is_empty() {
        b.life = Some(LifeType::Ocean);
        return;
    }
    let colony = b.colony.as_ref().is_some_and(|c| c.tech.active());
    b.life = Some(weighted(rng, &[
        (LifeType::Forests, 60.0),
        (LifeType::Giant, 15.0),
        (LifeType::Hostile, 15.0),
        (LifeType::ColonyAltered, if colony { 15.0 } else { 0.0 }),
    ]));
}

/// Colony-Altered life only exists where a colony brought its crops.
pub fn enforce_colony_requirement(b: &mut Proto) {
    if b.life == Some(LifeType::ColonyAltered) && b.colony.is_none() {
        b.life = Some(LifeType::Forests);
    }
}

/// `adv` scales the spacefaring levels (the civilisation option).
pub fn tech_weights(rarer: bool, adv: f64) -> Vec<(NativeTech, f64)> {
    use NativeTech::*;
    let r = |a: f64, b: f64| if rarer { a } else { b };
    vec![
        (StoneAge, r(26.0, 24.0)),
        (BronzeIron, r(20.0, 18.0)),
        (Medieval, r(16.0, 14.0)),
        (EarlyIndustrial, 12.0),
        (Industrial, 10.0),
        (Atomic, r(7.0, 8.0)),
        (SpaceAge, r(4.0, 6.0) * adv),
        (Interplanetary, r(2.0, 3.0) * adv),
        (Interstellar, r(1.0, 2.0) * adv),
        (AdvancedInterstellar, r(0.5, 1.0) * adv),
        (PostApocStone, r(7.0, 8.0)),
        (PostApocIndustrial, r(4.0, 6.0)),
        (PostApocAtomic, r(2.0, 4.0)),
        (PostApocSpace, r(1.0, 2.0) * adv),
        (ExtinctRuins, r(0.5, 1.0)),
    ]
}

/// How many sentient species evolved here (each needs the previous one).
/// `mult` scales the first roll (the civilisation option).
pub fn roll_sentients(rng: &mut StdRng, life: LifeType, mult: f64) -> u8 {
    let [a, b, c] = life.sentient_odds();
    let mut n = 0;
    if percent(rng, (a * SENTIENT_SCALE * mult).min(95.0)) {
        n += 1;
        if percent(rng, b * 2.0) {
            n += 1;
            if percent(rng, c * 4.0) {
                n += 1;
            }
        }
    }
    n
}

/// Worlds a species could evolve on or settle.
pub fn habitable_candidate(b: &Proto) -> bool {
    b.kind == Kind::Terrestrial
        && !b.is_moon
        && b.zone == 2
        && b.temp == TempBand::Temperate
        && !matches!(b.atmo, Some(AtmoKind::Corrosive | AtmoKind::Infiltrating))
}

pub fn expansion_chance(tech: NativeTech) -> f64 {
    match tech {
        NativeTech::Interplanetary => 10.0,
        NativeTech::Interstellar => 30.0,
        NativeTech::AdvancedInterstellar => 60.0,
        _ => 0.0,
    }
}

pub fn species_name(rng: &mut StdRng) -> String {
    rngx::name(rng)
}

fn base_population(tech: NativeTech) -> f64 {
    use NativeTech::*;
    match tech {
        StoneAge => 1e5,
        BronzeIron => 1e6,
        Medieval => 1e7,
        EarlyIndustrial => 5e7,
        Industrial => 1e8,
        Atomic => 4e8,
        SpaceAge => 8e8,
        Interplanetary => 1.5e9,
        Interstellar => 3e9,
        AdvancedInterstellar => 6e9,
        PostApocStone => 2e4,
        PostApocIndustrial => 2e5,
        PostApocAtomic => 1e6,
        PostApocSpace => 5e6,
        ExtinctRuins => 0.0,
    }
}

/// Rounds to 2 significant figures, at least to the nearest 10.
pub fn round_population(n: f64) -> f64 {
    if n <= 0.0 {
        return 0.0;
    }
    let mag = 10f64.powf((n.log10().floor() - 1.0).max(1.0));
    ((n / mag).round() * mag).max(10.0)
}

/// The civilisation on the world where a species evolved.
pub fn society(rng: &mut StdRng, species: String, count: u8, tech: NativeTech, colony_tech: Option<crate::sim::planet::ColonyTech>) -> NativeSociety {
    let tier = tech.tier();
    let base = base_population(tech);
    let mut pop = base * r2d6(rng) as f64;
    for _ in 1..count {
        pop += base * r2d6(rng) as f64 * 0.3;
    }
    let factions_t: &[(&str, f64)] = match tier {
        "tribal" => &[("Scattered bands", 30.0), ("Many tribes", 35.0), ("A few large tribes", 25.0), ("Tribal confederation", 10.0)],
        "early" => &[("Many city-states", 25.0), ("Several kingdoms", 35.0), ("Two rival empires", 25.0), ("One dominant empire", 15.0)],
        "spacefaring" => &[("Rival power blocs", 20.0), ("Two superpowers", 20.0), ("Federation of nations", 30.0), ("One world government", 30.0)],
        "collapsed" => &[("Isolated survivor enclaves", 35.0), ("Warring remnant states", 30.0), ("One remnant state and outlying tribes", 25.0), ("Scattered nomads", 10.0)],
        _ => &[("Many nations", 30.0), ("Rival power blocs", 30.0), ("Two superpowers", 25.0), ("One dominant nation", 15.0)],
    };
    let power_t: &[(&str, f64)] = match tier {
        "tribal" => &[("Elders", 30.0), ("Shamans", 25.0), ("Warrior chiefs", 30.0), ("Hive collective", 15.0)],
        "early" => &[("Hereditary rulers", 35.0), ("Priesthood", 25.0), ("Warrior caste", 20.0), ("Merchant houses", 12.0), ("Hive collective", 8.0)],
        "spacefaring" => &[("Technocrats", 25.0), ("Corporate oligarchy", 20.0), ("Elected assembly", 20.0), ("Military command", 15.0), ("Machine intelligence", 10.0), ("Hive collective", 10.0)],
        "collapsed" => &[("Warlords", 35.0), ("Scavenger clans", 25.0), ("Remnant government", 20.0), ("Doomsday cults", 20.0)],
        _ => &[("Industrialists", 25.0), ("Military junta", 20.0), ("Elected assembly", 25.0), ("Hereditary rulers", 15.0), ("Theocracy", 15.0)],
    };
    let mut factions = weighted(rng, factions_t).to_string();
    if count > 1 && percent(rng, 40.0) {
        factions.push_str(", divided by species");
    }
    let dominant_power = weighted(rng, power_t).to_string();
    use NativeTech::*;
    let in_orbit = match tech {
        Atomic => "Native satellites",
        SpaceAge => "Native satellites, native space station",
        Interplanetary => "Native orbital stations, native shipyards",
        Interstellar => "Native orbital habitats, shipyards and defence platforms",
        AdvancedInterstellar => "Native orbital megastructures, fleet anchorage",
        PostApocAtomic => "Dead native satellites",
        PostApocSpace => "Derelict native stations, orbital debris field",
        _ => "",
    }
    .to_string();
    let relations = colony_tech
        .map(|ct| {
            use crate::sim::planet::ColonyTech as C;
            let hostile = if ct == C::Fallen { 20.0 } else { 0.0 };
            let t: Vec<(&str, f64)> = if ct == C::Derelict {
                vec![("Drove the colonists out", 30.0), ("Shun the abandoned colony", 30.0), ("Scavenge the ruins", 25.0), ("Never noticed the colony", 15.0)]
            } else if matches!(tier, "tribal" | "early" | "collapsed") {
                vec![("Unaware of the colony", 15.0), ("Curious", 20.0), ("Worship the colonists", 8.0), ("Wary", 20.0), ("Hostile", 15.0 + hostile), ("Trade with the colony", 15.0), ("Exploited by the colony", 7.0)]
            } else if tier == "industrial" {
                vec![("Wary", 25.0), ("Hostile", 15.0 + hostile), ("Trading partners", 25.0), ("Uneasy truce", 20.0), ("Exploited by the colony", 15.0)]
            } else {
                vec![("Formal diplomatic relations", 30.0), ("Trading partners", 25.0), ("Cold war", 15.0), ("Open conflict", 10.0 + hostile), ("Colony exists by native permission", 20.0)]
            };
            weighted(rng, &t).to_string()
        })
        .unwrap_or_default();
    NativeSociety {
        species,
        species_count: count,
        tech,
        population: round_population(pop),
        factions,
        dominant_power,
        in_orbit,
        relations,
    }
}

/// "4,200,000,000" -> "4.2 billion".
pub fn fmt_population(n: f64) -> String {
    if n >= 1e9 {
        format!("{:.1} billion", n / 1e9)
    } else if n >= 1e6 {
        format!("{:.1} million", n / 1e6)
    } else if n >= 1e3 {
        let s = format!("{:.0}", n);
        let mut out = String::new();
        for (i, ch) in s.chars().enumerate() {
            if i > 0 && (s.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(ch);
        }
        out
    } else {
        format!("{n:.0}")
    }
}

/// Picks one item uniformly (re-exported for the pipeline).
pub fn pick_index(rng: &mut StdRng, n: usize) -> usize {
    dice::below(rng, n as i32) as usize
}
