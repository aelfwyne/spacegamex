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
    /// Commercial cargo hold (tonnes).
    pub cargo_t: f64,

    pub tanks: [(Res, f64); 11],
    /// Starting stock as a fraction of capacity.
    pub start_frac: [(Res, f64); 11],

    /// List price at an interstellar shipyard (credits). Also sets the
    /// trade-in value, blueprint price and colony build cost.
    pub price: f64,
    /// Interstellar shipyards sell it (the ark is never on the market).
    pub for_sale: bool,
    /// Hull class in battle (before upgrades and pilot).
    pub attack: f64,
    pub defense: f64,
    /// Small and nimble: better odds of running from pirates.
    pub agile: bool,
    /// "Fighter", "Cargo" or "General" (for the shipyard lists).
    pub role: Role,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    General,
    Fighter,
    Cargo,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::General => "General",
            Role::Fighter => "Fighter",
            Role::Cargo => "Cargo",
        }
    }
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
    scoop_rate: 30.0,
    drones: 2,
    drone_yield: 8.0,
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
    cargo_t: 8.0,
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
    price: 30_000.0,
    for_sale: true,
    attack: 2.0,
    defense: 2.0,
    agile: true,
    role: Role::General,
};

const COMPANY: ShipSpec = ShipSpec {
    key: "COMPANY",
    name: "Cargomaster 'Providence'",
    blurb: "Company ship, crew of ten. Real industrial capacity, and a hold big enough to found a small colony.",
    crew: 10,
    pax: 0,
    hull: 250.0,
    reactor_mw: 250.0,
    reactor_burn: 30.0,
    scoop_mw: 10.0,
    scoop_rate: 80.0,
    drones: 3,
    drone_yield: 12.0,
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
    cargo_t: 80.0,
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
    price: 100_000.0,
    for_sale: true,
    attack: 3.0,
    defense: 3.0,
    agile: false,
    role: Role::General,
};

const COLONY: ShipSpec = ShipSpec {
    key: "COLONY",
    name: "Ark 'Hesperide'",
    blurb: "Colony ark: 10 crew, 500 sleeping passengers, and a full colony kit in the hold. Enormous industrial plant, and an enormous mouth to feed.",
    crew: 10,
    pax: 500,
    hull: 600.0,
    // Enough to run everything in-system at once (life support, fab,
    // drones, scoop, shields and precharge: 735 MW). The burn keeps the
    // old fuel cost per MW.
    reactor_mw: 800.0,
    reactor_burn: 91.0,
    scoop_mw: 25.0,
    scoop_rate: 400.0,
    drones: 40,
    drone_yield: 40.0,
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
    cargo_t: 600.0,
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
    price: 400_000.0,
    for_sale: false,
    attack: 2.5,
    defense: 4.0,
    agile: false,
    role: Role::General,
};

// ---- hulls bought at interstellar shipyards (or built from blueprints) ----
// Priced against the Company ship at 100,000 cr.

#[allow(clippy::too_many_arguments)]
const fn tanks(h2: f64, ice: f64, metal: f64, sil: f64, carbon: f64, n2: f64, water: f64, o2: f64, food: f64, mat: f64, med: f64) -> [(Res, f64); 11] {
    [
        (Res::H2, h2),
        (Res::Ice, ice),
        (Res::Metal, metal),
        (Res::Silicate, sil),
        (Res::Carbon, carbon),
        (Res::Nitrogen, n2),
        (Res::Water, water),
        (Res::Oxygen, o2),
        (Res::Food, food),
        (Res::Mat, mat),
        (Res::Med, med),
    ]
}

const LANCER: ShipSpec = ShipSpec {
    key: "LANCER",
    name: "Interceptor 'Kestrel'",
    blurb: "Two-seat interceptor. Fast, hard-hitting and quick to break off; little hold and a thirsty drive.",
    crew: 2,
    hull: 150.0,
    reactor_mw: 150.0,
    reactor_burn: 18.0,
    scoop_rate: 40.0,
    drones: 2,
    fab_rate: 100.0,
    fab_mw: 25.0,
    ftl_mw: 90.0,
    ftl_burn: 60.0,
    ftl_speed: 10.0,
    precharge_mw: 40.0,
    precharge_mwh_ly: 30.0,
    ls_mw: 18.0,
    shields_mw: 25.0,
    wear_per_ly: 0.07,
    move_time_per_au: 0.12,
    move_fuel_per_au: 0.6,
    cargo_t: 10.0,
    tanks: tanks(6_000.0, 2_500.0, 3_500.0, 2_500.0, 1_800.0, 1_200.0, 2_500.0, 800.0, 1_200.0, 1_200.0, 80.0),
    price: 60_000.0,
    attack: 5.0,
    defense: 3.5,
    agile: true,
    role: Role::Fighter,
    ..SOLO
};

const WARDEN: ShipSpec = ShipSpec {
    key: "WARDEN",
    name: "Gunship 'Sentinel'",
    blurb: "Patrol gunship, crew of six. Heavy guns and armour on a working hull with a modest hold.",
    crew: 6,
    hull: 400.0,
    reactor_mw: 320.0,
    reactor_burn: 38.0,
    scoop_rate: 90.0,
    fab_rate: 260.0,
    fab_mw: 55.0,
    ftl_mw: 180.0,
    ftl_burn: 160.0,
    ftl_speed: 8.5,
    precharge_mw: 70.0,
    precharge_mwh_ly: 65.0,
    ls_mw: 30.0,
    shields_mw: 50.0,
    wear_per_ly: 0.05,
    move_time_per_au: 0.16,
    cargo_t: 30.0,
    tanks: tanks(14_000.0, 5_000.0, 9_000.0, 5_000.0, 4_000.0, 3_000.0, 5_000.0, 1_500.0, 2_500.0, 3_000.0, 200.0),
    price: 160_000.0,
    attack: 7.0,
    defense: 6.0,
    role: Role::Fighter,
    ..COMPANY
};

const BASTION: ShipSpec = ShipSpec {
    key: "BASTION",
    name: "Heavy Frigate 'Aegis'",
    blurb: "A warship in all but flag, crew of fourteen. Pirates think twice; the fuel bill does not.",
    crew: 14,
    hull: 850.0,
    reactor_mw: 650.0,
    reactor_burn: 75.0,
    scoop_mw: 20.0,
    scoop_rate: 200.0,
    drones: 8,
    drone_yield: 20.0,
    fab_rate: 600.0,
    fab_mw: 110.0,
    ftl_mw: 360.0,
    ftl_burn: 380.0,
    ftl_speed: 7.5,
    precharge_mw: 120.0,
    precharge_mwh_ly: 120.0,
    ls_mw: 60.0,
    shields_mw: 100.0,
    wear_per_ly: 0.04,
    move_time_per_au: 0.2,
    move_fuel_per_au: 1.8,
    cargo_t: 60.0,
    tanks: tanks(30_000.0, 9_000.0, 16_000.0, 9_000.0, 7_000.0, 5_000.0, 9_000.0, 3_000.0, 5_000.0, 6_000.0, 400.0),
    price: 380_000.0,
    attack: 10.0,
    defense: 9.0,
    role: Role::Fighter,
    ..COMPANY
};

const COURIER: ShipSpec = ShipSpec {
    key: "COURIER",
    name: "Fast Packet 'Swallow'",
    blurb: "Two-crew courier with an oversized drive. The fastest thing in the lanes, with room for a little freight.",
    crew: 2,
    hull: 120.0,
    reactor_mw: 140.0,
    reactor_burn: 16.0,
    scoop_rate: 45.0,
    fab_rate: 90.0,
    ftl_mw: 85.0,
    ftl_burn: 50.0,
    ftl_speed: 12.0,
    precharge_mw: 40.0,
    precharge_mwh_ly: 22.0,
    ls_mw: 16.0,
    wear_per_ly: 0.06,
    move_time_per_au: 0.1,
    cargo_t: 40.0,
    tanks: tanks(7_000.0, 2_000.0, 3_000.0, 2_000.0, 1_500.0, 1_000.0, 2_200.0, 700.0, 1_100.0, 900.0, 60.0),
    price: 55_000.0,
    attack: 2.0,
    defense: 2.0,
    agile: true,
    role: Role::Cargo,
    ..SOLO
};

const HAULER: ShipSpec = ShipSpec {
    key: "HAULER",
    name: "Bulk Freighter 'Ox'",
    blurb: "Crew of eight and a 300-tonne hold. Slow, sturdy and profitable on any trade run.",
    crew: 8,
    hull: 380.0,
    reactor_mw: 300.0,
    reactor_burn: 36.0,
    scoop_rate: 100.0,
    drones: 4,
    fab_rate: 320.0,
    fab_mw: 65.0,
    ftl_mw: 190.0,
    ftl_burn: 190.0,
    ftl_speed: 7.0,
    precharge_mw: 70.0,
    precharge_mwh_ly: 75.0,
    ls_mw: 38.0,
    wear_per_ly: 0.055,
    move_time_per_au: 0.2,
    move_fuel_per_au: 1.3,
    cargo_t: 300.0,
    tanks: tanks(16_000.0, 6_000.0, 9_000.0, 6_000.0, 5_000.0, 3_500.0, 6_000.0, 1_800.0, 3_000.0, 2_500.0, 180.0),
    price: 180_000.0,
    attack: 2.5,
    defense: 4.0,
    role: Role::Cargo,
    ..COMPANY
};

const LEVIATHAN: ShipSpec = ShipSpec {
    key: "LEVIATHAN",
    name: "Super-Freighter 'Atlas'",
    blurb: "Sixteen crew and a 1,200-tonne hold: a whole colony's supplies in one run. Lumbering and hungry.",
    crew: 16,
    hull: 900.0,
    reactor_mw: 650.0,
    reactor_burn: 80.0,
    scoop_mw: 20.0,
    scoop_rate: 250.0,
    drones: 12,
    drone_yield: 20.0,
    fab_rate: 700.0,
    fab_mw: 130.0,
    ftl_mw: 380.0,
    ftl_burn: 420.0,
    ftl_speed: 6.0,
    precharge_mw: 120.0,
    precharge_mwh_ly: 130.0,
    ls_mw: 70.0,
    shields_mw: 50.0,
    wear_per_ly: 0.045,
    move_time_per_au: 0.24,
    move_fuel_per_au: 2.2,
    cargo_t: 1_200.0,
    tanks: tanks(34_000.0, 12_000.0, 18_000.0, 11_000.0, 9_000.0, 7_000.0, 12_000.0, 3_500.0, 7_000.0, 5_000.0, 400.0),
    price: 450_000.0,
    attack: 2.5,
    defense: 5.0,
    role: Role::Cargo,
    ..COMPANY
};

/// The first three are the starting ships (and what a colony shipyard
/// builds without blueprints); the rest are bought or built later.
pub const SHIPS: [ShipSpec; 9] = [SOLO, COMPANY, COLONY, LANCER, WARDEN, BASTION, COURIER, HAULER, LEVIATHAN];
/// How many of `SHIPS` are starting ships.
pub const STARTERS: usize = 3;

/// The reactor never idles below this share of its rated burn.
pub const REACTOR_IDLE: f64 = 0.1;

impl ShipSpec {
    pub fn spec(idx: usize) -> &'static ShipSpec {
        &SHIPS[idx % SHIPS.len()]
    }

    /// Credits a shipyard pays for this hull in perfect condition.
    pub fn trade_in(&self) -> f64 {
        self.price * 0.4
    }

    /// What a colony shipyard needs to build one: (tonnes of materials,
    /// labour-years).
    pub fn build_cost(&self) -> (f64, f64) {
        (self.price / 50.0, self.price / 100.0)
    }

    /// Price of the plans a colony shipyard needs for a non-starter hull.
    pub fn blueprint_price(&self) -> f64 {
        self.price * 0.5
    }

    /// People aboard it can carry: its crew and passengers, plus quarters
    /// for a partner and a pilot.
    pub fn berths(&self) -> usize {
        self.crew + self.pax + 2
    }

    /// H2 kg/h the reactor burns delivering `mw` (fuel follows the power
    /// actually drawn, with a small idle floor).
    pub fn burn_at(&self, mw: f64) -> f64 {
        self.reactor_burn * (mw / self.reactor_mw).clamp(REACTOR_IDLE, 1.0)
    }

    /// Typical burn while parked and working: life support, fab, shields
    /// and scoop or drones running. Used for planning, not the sim itself.
    pub fn cruise_burn(&self) -> f64 {
        let work = self.scoop_mw.max(self.drones as f64 * self.drone_mw);
        self.burn_at(self.ls_mw + self.fab_mw + self.shields_mw + work)
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

