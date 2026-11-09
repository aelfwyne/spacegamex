//! Colonies founded by spacefaring peoples from other systems (port of
//! `Colony.gs`; allegiance tables dropped — the founders are named instead).

use super::bodies::Proto;
use super::dice::{self, d6, r2d6, r3d6, weighted};
use super::life::round_population;
use crate::rngx;
use crate::sim::planet::{Colony, ColonyTech};
use rand::rngs::StdRng;

/// Per-system chance (%) that someone planted a colony here, given a world
/// that can take one. Tuned with `life::SENTIENT_SCALE` so trade stays about
/// as rare as it has always been.
pub const COLONY_CHANCE: f64 = 1.6;
/// Colonies are planted by interstellar powers, so they skew established.
const FOUNDER_BONUS: i32 = 2;

fn colony_name(rng: &mut StdRng) -> String {
    let root = rngx::name(rng);
    match dice::below(rng, 6) {
        0 => format!("New {root}"),
        1 => format!("{root} Station"),
        2 => format!("{root} Landing"),
        3 => format!("Fort {root}"),
        4 => format!("{root}'s Hope"),
        _ => format!("{root} {}", dice::pick(rng, &["Outpost", "Anchor", "Prospect", "Terminal", "Base"])),
    }
}

fn missions(rng: &mut StdRng, count: i32, m: i32) -> String {
    let mut out: Vec<&str> = Vec::new();
    for _ in 0..count {
        let r = r2d6(rng) + m;
        let mission = match r {
            r if r <= 2 => "Terraforming",
            3 => "Research",
            4 => "Survey and Prospecting",
            5 => "Prison/Secluded or Exile",
            6 => "Mining and Refining",
            7 => "Mineral Drilling",
            8 => "Communications Relay",
            9 => "Military",
            10 => "Ranching/Logging",
            11 => "Corporate HQ",
            _ => "Government HQ",
        };
        if !out.contains(&mission) {
            out.push(mission);
        }
    }
    out.sort();
    out.join(", ")
}

fn orbit_result(rng: &mut StdRng, roll: i32, items: &mut Vec<&'static str>) {
    let it = match roll {
        r if r <= 4 => {
            if dice::below(rng, 2) == 1 { Some("Wreckage") } else { None }
        }
        5 => None, // ring (natural)
        6 => Some(*dice::pick(rng, &[
            "Abandoned Satellite",
            "Repurposed Satellite",
            "Abandoned Space Station",
            "Repurposed Space Station",
        ])),
        7 | 8 => None, // moons (natural)
        9 => Some("Survey Station"),
        10 => Some("Survey and Communications Satellites"),
        _ => Some("Transfer Station"),
    };
    if let Some(i) = it
        && !items.contains(&i) {
            items.push(i);
        }
}

fn colonial_orbit(rng: &mut StdRng, m: i32) -> String {
    let mut items = Vec::new();
    let roll = r2d6(rng) + m;
    if roll <= 11 {
        orbit_result(rng, roll, &mut items);
    } else {
        for _ in 0..d6(rng) {
            let r = r2d6(rng) + m;
            orbit_result(rng, r, &mut items);
        }
    }
    items.sort();
    items.join(", ")
}

pub fn found_colony(rng: &mut StdRng, b: &Proto, founders: String) -> Colony {
    let roll = r2d6(rng) + b.mods.colony + FOUNDER_BONUS;
    let mut mission_mod = b.mods.mission;
    let mut orbit_mod = b.mods.orbit;
    let (mut size, base, count) = if roll <= 7 {
        mission_mod -= 1;
        ("Start-Up", 10.0 * r3d6(rng) as f64, 1)
    } else if roll <= 10 {
        orbit_mod += 1;
        ("Young", 100.0 * r3d6(rng) as f64, 1 + dice::below(rng, 2))
    } else {
        mission_mod += 4;
        orbit_mod += 2;
        ("Established", 1000.0 * r2d6(rng) as f64, 1 + dice::below(rng, 3))
    };
    use ColonyTech::*;
    let tech = match size {
        "Start-Up" => weighted(rng, &[(FrontierOutpost, 50.0), (SurveyInfrastructure, 38.0), (Settled, 12.0)]),
        "Young" => weighted(rng, &[
            (FrontierOutpost, 10.0),
            (SurveyInfrastructure, 20.0),
            (Settled, 45.0),
            (Industrial, 20.0),
            (Advanced, 5.0),
        ]),
        _ => weighted(rng, &[(Settled, 20.0), (Industrial, 40.0), (Advanced, 25.0), (Fallen, 10.0), (Derelict, 5.0)]),
    };
    let factor = match tech {
        FrontierOutpost => 0.5,
        SurveyInfrastructure => 0.3,
        Industrial => 1.5,
        Advanced => 2.0,
        _ => 1.0,
    };
    let mut population = round_population(base * factor);
    let mut missions = missions(rng, count, mission_mod);
    let mut in_orbit = colonial_orbit(rng, orbit_mod);
    let fr = d6(rng);
    let mut factions = if fr <= 5 {
        ["One dominant faction", "Two balanced factions", "Two competing factions", "One dominant, one weak faction", "Three competing factions"][(fr - 1) as usize].to_string()
    } else {
        let alt = d6(rng);
        if alt == 1 { "One faction".into() } else { format!("{alt} factions") }
    };
    let mut faction_type = dice::pick(rng, &[
        "Newcomers",
        "Corporate representatives",
        "Scientists",
        "Workers",
        "Security/Military",
        "Colonial leadership",
    ])
    .to_string();
    let mut size_s = size.to_string();
    match tech {
        Derelict => {
            size = "Abandoned";
            size_s = size.into();
            population = 0.0;
            if !missions.is_empty() {
                missions = format!("Former {missions}");
            }
            factions.clear();
            faction_type.clear();
            if !in_orbit.is_empty() {
                in_orbit = format!("Derelict {in_orbit}");
            }
        }
        Fallen => {
            let survivors = population * (5 + dice::below(rng, 26)) as f64 / 100.0;
            population = round_population(survivors).max(10.0);
            size_s = format!("{size} (Fallen)");
        }
        _ => {}
    }
    Colony {
        name: colony_name(rng),
        founders,
        size: size_s,
        tech,
        population,
        missions,
        factions,
        faction_type,
        in_orbit,
    }
}
