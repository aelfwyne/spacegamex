//! The three ship prototypes. All rates are per in-game hour at 100% setting.

use super::res::Res;

#[derive(Debug, Clone, Copy)]
pub struct ShipSpec {
    pub key: &'static str,
    pub name: &'static str,
    pub blurb: &'static str,
    pub crew: usize,
    pub pax: usize,

    pub hull: f64,
    pub reactor_mw: f64,
    /// H2 burn kg/h at 100% reactor output.
    pub reactor_burn: f64,

    pub scoop_mw: f64,
    /// H2 kg/h at a solar-mass G star.
    pub scoop_rate: f64,

    pub drones: usize,
    /// kg/h per drone.
    pub drone_yield: f64,
    pub drone_mw: f64,

    /// kg of raw material processed per hour.
    pub fab_rate: f64,
    pub fab_mw: f64,

    pub ftl_mw: f64,
    /// H2 kg/h at 100% throttle.
    pub ftl_burn: f64,
    /// ly/h at 100% throttle.
    pub ftl_speed: f64,
    pub precharge_mw: f64,
    /// MW·h of capacitor charge required per ly of jump.
    pub precharge_mwh_ly: f64,

    /// Whole-ship life support load at 100% efficiency.
    pub ls_mw: f64,
    pub shields_mw: f64,

    /// Hull wear per light-year of FTL travel.
    pub wear_per_ly: f64,

    pub move_time_per_au: f64, // h per AU of in-system shuttle
    pub move_fuel_per_au: f64, // kg H2 per AU

    pub credits: f64,

    pub tanks: [(Res, f64); 11],
    /// Starting stock as a fraction of capacity.
    pub start_frac: [(Res, f64); 11],
}

const SOLO: ShipSpec = ShipSpec {
    key: "SOLO",
    name: "Keelclipper 'Meridian'",
    blurb: "Single-crew private ship. Light, efficient, forgiving — but everything runs on one person's judgement.",
    crew: 1,
    pax: 0,
    hull: 100.0,
    reactor_mw: 100.0,
    reactor_burn: 12.0,
    scoop_mw: 5.0,
    scoop_rate: 20.0,
    drones: 1,
    drone_yield: 5.0,
    drone_mw: 2.0,
    fab_rate: 80.0,
    fab_mw: 25.0,
    ftl_mw: 60.0,
    ftl_burn: 40.0,
    ftl_speed: 8.0,
    precharge_mw: 25.0,
    precharge_mwh_ly: 25.0,
    ls_mw: 15.0,
    shields_mw: 15.0,
    wear_per_ly: 0.08,
    move_time_per_au: 0.15,
    move_fuel_per_au: 0.5,
    credits: 2_000.0,
    tanks: [
        (Res::H2, 5_000.0),
        (Res::Ice, 2_000.0),
        (Res::Metal, 3_000.0),
        (Res::Silicate, 2_000.0),
        (Res::Carbon, 1_500.0),
        (Res::Nitrogen, 1_000.0),
        (Res::Water, 2_000.0),
        (Res::Oxygen, 600.0),
        (Res::Food, 1_000.0),
        (Res::Mat, 800.0),
        (Res::Med, 50.0),
    ],
    start_frac: [
        (Res::H2, 0.7),
        (Res::Ice, 0.4),
        (Res::Metal, 0.4),
        (Res::Silicate, 0.4),
        (Res::Carbon, 0.3),
        (Res::Nitrogen, 0.3),
        (Res::Water, 0.5),
        (Res::Oxygen, 0.5),
        (Res::Food, 0.5),
        (Res::Mat, 0.5),
        (Res::Med, 0.8),
    ],
};

const COMPANY: ShipSpec = ShipSpec {
    key: "COMPANY",
    name: "Cargomaster 'Providence'",
    blurb: "Company ship, crew of five. Real industrial capacity — mining, fab and fuel on a proper scale.",
    crew: 5,
    pax: 0,
    hull: 250.0,
    reactor_mw: 250.0,
    reactor_burn: 30.0,
    scoop_mw: 10.0,
    scoop_rate: 80.0,
    drones: 3,
    drone_yield: 8.0,
    drone_mw: 2.0,
    fab_rate: 300.0,
    fab_mw: 60.0,
    ftl_mw: 150.0,
    ftl_burn: 150.0,
    ftl_speed: 8.0,
    precharge_mw: 60.0,
    precharge_mwh_ly: 60.0,
    ls_mw: 40.0,
    shields_mw: 30.0,
    wear_per_ly: 0.06,
    move_time_per_au: 0.18,
    move_fuel_per_au: 1.0,
    credits: 8_000.0,
    tanks: [
        (Res::H2, 12_000.0),
        (Res::Ice, 5_000.0),
        (Res::Metal, 8_000.0),
        (Res::Silicate, 5_000.0),
        (Res::Carbon, 4_000.0),
        (Res::Nitrogen, 3_000.0),
        (Res::Water, 5_000.0),
        (Res::Oxygen, 1_500.0),
        (Res::Food, 2_500.0),
        (Res::Mat, 2_000.0),
        (Res::Med, 150.0),
    ],
    start_frac: [
        (Res::H2, 0.7),
        (Res::Ice, 0.4),
        (Res::Metal, 0.4),
        (Res::Silicate, 0.4),
        (Res::Carbon, 0.3),
        (Res::Nitrogen, 0.3),
        (Res::Water, 0.5),
        (Res::Oxygen, 0.5),
        (Res::Food, 0.5),
        (Res::Mat, 0.5),
        (Res::Med, 0.8),
    ],
};

const COLONY: ShipSpec = ShipSpec {
    key: "COLONY",
    name: "Ark 'Hesperide'",
    blurb: "Colony ark: 10 crew, 500 sleeping passengers. Enormous industrial plant — and an enormous mouth to feed. The hardest way to survive.",
    crew: 10,
    pax: 500,
    hull: 600.0,
    reactor_mw: 700.0,
    reactor_burn: 80.0,
    scoop_mw: 25.0,
    scoop_rate: 400.0,
    drones: 40,
    drone_yield: 35.0,
    drone_mw: 2.0,
    fab_rate: 1_600.0,
    fab_mw: 200.0,
    ftl_mw: 400.0,
    ftl_burn: 500.0,
    ftl_speed: 6.0,
    precharge_mw: 120.0,
    precharge_mwh_ly: 140.0,
    ls_mw: 250.0,
    shields_mw: 60.0,
    wear_per_ly: 0.05,
    move_time_per_au: 0.22,
    move_fuel_per_au: 2.0,
    credits: 25_000.0,
    tanks: [
        (Res::H2, 40_000.0),
        (Res::Ice, 20_000.0),
        (Res::Metal, 25_000.0),
        (Res::Silicate, 15_000.0),
        (Res::Carbon, 12_000.0),
        // An ark carries months of provisions, not days: nitrogen is the
        // rarest fab raw (frozen deep in the outer systems), and a colony
        // that must find its first usable deposit can afford to burn slow
        // while it searches.
        (Res::Nitrogen, 16_000.0),
        (Res::Water, 25_000.0),
        (Res::Oxygen, 6_000.0),
        (Res::Food, 16_000.0),
        (Res::Mat, 8_000.0),
        (Res::Med, 500.0),
    ],
    start_frac: [
        (Res::H2, 0.7),
        (Res::Ice, 0.5),
        (Res::Metal, 0.4),
        (Res::Silicate, 0.4),
        (Res::Carbon, 0.4),
        (Res::Nitrogen, 0.6),
        (Res::Water, 0.6),
        (Res::Oxygen, 0.6),
        (Res::Food, 0.6),
        (Res::Mat, 0.6),
        (Res::Med, 0.8),
    ],
};

pub const SHIPS: [ShipSpec; 3] = [SOLO, COMPANY, COLONY];

impl ShipSpec {
    pub fn spec(idx: usize) -> &'static ShipSpec {
        &SHIPS[idx % SHIPS.len()]
    }

    /// Throttle multipliers: (speed, burn) at 50/75/100.
    pub fn throttle_factor(t: u8) -> (f64, f64) {
        match t {
            50 => (0.55, 0.5),
            75 => (0.8, 0.85),
            _ => (1.0, 1.0),
        }
    }
}
