//! Mutable ship/mission state and the simulation step.
//!
//! All rates are per in-game hour. `advance` is continuous: it is correct for
//! any dt, which is what makes fast-forward and "skip to arrival" safe.

use super::civ::{CivKind, CivTier};
use super::galaxy::{Galaxy, Neighbor};
use super::logbuf::{Level, Log};
use super::cargo::Hold;
use super::opts::GalaxyOpts;
use super::people::Sex;
use super::res::{Inventory, Res};
use super::ship::ShipSpec;
use super::system::StarSystem;
use crate::rngx::{derive, name};
use rand::RngExt;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// Per-person life-support demand (kg/h).
pub const O2_PER_P_H: f64 = 0.15;
pub const WATER_PER_P_H: f64 = 0.5;
pub const FOOD_PER_P_H: f64 = 0.35;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Focus {
    LifeSupport,
    Repair,
    Medical,
    Stockpile,
    Idle,
}

impl Focus {
    pub const ALL: [Focus; 5] = [
        Focus::LifeSupport,
        Focus::Repair,
        Focus::Medical,
        Focus::Stockpile,
        Focus::Idle,
    ];

    pub fn cycle(self) -> Focus {
        let i = (Self::ALL.iter().position(|f| *f == self).unwrap_or(0) + 1) % Self::ALL.len();
        Self::ALL[i]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Focus::LifeSupport => "Life support",
            Focus::Repair => "Repair",
            Focus::Medical => "Medical",
            Focus::Stockpile => "Stockpile",
            Focus::Idle => "Idle",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FaultKind {
    Reactor,
    LifeSupport,
    Scoop,
    Drones,
    FtlCoil,
}

impl FaultKind {
    pub fn name(&self) -> &'static str {
        match self {
            FaultKind::Reactor => "Reactor efficiency fault",
            FaultKind::LifeSupport => "Life-support clog",
            FaultKind::Scoop => "Scoop grid degraded",
            FaultKind::Drones => "Mining drone swarm fault",
            FaultKind::FtlCoil => "FTL coil degradation",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fault {
    pub kind: FaultKind,
    pub until_h: f64,
    pub mat_fix: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Loc {
    /// The primary star (both stars, for a close binary).
    Star,
    Planet(usize),
    /// (planet, moon) — a moon of a gas giant.
    Moon(usize, usize),
    /// Star B of a wide binary.
    Companion,
}

impl Loc {
    /// Docked at a star (scoop territory) rather than a body.
    pub fn is_stellar(&self) -> bool {
        matches!(self, Loc::Star | Loc::Companion)
    }

    pub fn is_body(&self) -> bool {
        !self.is_stellar()
    }

    /// The planet index this location belongs to (a moon's parent).
    pub fn planet(&self) -> Option<usize> {
        match *self {
            Loc::Planet(i) | Loc::Moon(i, _) => Some(i),
            _ => None,
        }
    }
}

pub fn loc_label(loc: Loc, sys: &StarSystem) -> String {
    match loc {
        Loc::Star => format!("{} ({})", sys.star.name, sys.star.catalog),
        Loc::Companion => sys
            .companion
            .as_ref()
            .map(|c| format!("{} ({})", c.star.name, c.star.code))
            .unwrap_or_else(|| "companion star".into()),
        _ => sys.site(loc).map(|p| p.name.clone()).unwrap_or_else(|| "unknown body".into()),
    }
}

/// Why a shuttle leg could not start. The UI shows this to the player: a
/// refused action must always come with a reason, never a silent no-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShuttleBlock {
    /// The ship is in FTL transit between stars.
    Tripping,
    /// The ship is mid-leg on another in-system shuttle.
    Shuttling,
    /// The ship is already docked where it was asked to go.
    AlreadyThere,
    /// No such place in this system (e.g. no companion star to dock at).
    Nowhere,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Move {
    pub to: Loc,
    pub remaining_h: f64,
    pub total_h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trip {
    pub dest_id: u64,
    pub dest_pos: [f64; 2],
    pub dest_name: String,
    pub dist_ly: f64,
    pub total_h: f64,
    pub remaining_h: f64,
    /// Continuous interstellar-dust hull damage rate (hull units/h).
    pub dust_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrewInfo {
    pub count: usize,
    pub names: Vec<String>,
    pub health: f64,
    pub morale: f64,
}

/// Someone signed on at a port: the captain's partner, or a combat pilot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recruit {
    pub name: String,
    pub sex: Sex,
    /// Native species name; None for a human.
    pub species: Option<String>,
    /// Pilot skill 1..=3 (partners: 0).
    #[serde(default)]
    pub skill: u8,
}

impl Recruit {
    pub fn species_name(&self) -> &str {
        self.species.as_deref().unwrap_or("Human")
    }
}

fn default_birth_y() -> f64 {
    -30.0
}

fn default_retire_age() -> f64 {
    70.0
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    pub jumps: u32,
    pub dist_ly: f64,
    pub scooped: f64,
    pub mined: f64,
    pub fabbed: f64,
    pub trades: u32,
    pub crew_lost: u32,
    pub pax_lost: u32,
    pub systems: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameOver {
    pub cause: String,
    pub detail: String,
    pub t: f64,
}

/// Scoop intake per hour at one site.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoopYield {
    pub h2: f64,
    pub n2: f64,
    pub carbon: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sim {
    pub seed: u64,
    pub ship_idx: usize,
    pub t: f64, // game seconds
    pub ff: u32,

    pub inv: Inventory,
    pub hull: f64,
    pub credits: f64,

    pub ftl_charge: f64, // 0..100
    pub ftl_wear: f64,   // 0..100
    pub ftl_throttle: u8,
    pub reactor_pct: u8,
    pub shields: bool,
    pub ls_eff: u8,
    pub lights: bool,
    pub scoop_on: bool,
    pub mining_on: bool,
    pub drone_auto: bool,
    /// Hands-off mode (future feature): when true, the game loop runs this
    /// ship's autopilot brain every tick, so the ship flies itself in the
    /// TUI. Nothing can set this yet — wiring a key (and a top-bar
    /// indicator) to it is all the feature needs.
    #[serde(default)]
    pub autopilot_on: bool,
    pub mine_target: Option<(Loc, usize)>, // body, deposit idx
    pub focus: Focus,
    pub repair_on: bool,

    pub sys_id: u64,
    pub loc: Loc,
    pub shuttle: Option<Move>,
    pub trip: Option<Trip>,
    pub destination: Option<u64>,

    pub faults: Vec<Fault>,
    pub crew: CrewInfo,
    pub pax: CrewInfo,
    pub debt_crew: f64,
    pub debt_pax: f64,

    pub rates: BTreeMap<Res, f64>, // net kg/h
    pub stats: Stats,
    pub log: Log,
    pub over: Option<GameOver>,
    pub visited: Vec<u64>,
    pub autosave_h: f64,
    pub warned: BTreeMap<String, f64>,
    pub last_brownout: bool,
    /// Ore the drones are committed to working: once a critical ore is
    /// started, the ship finishes it (to its buffer target) before switching,
    /// so it does not ping-pong between planets that each hold a critical ore.
    #[serde(default)]
    pub committed_ore: Option<Res>,
    /// Water-return mode (hysteresis, not a threshold): the base has
    /// wandered onto ground its water cannot sustain and is going back for
    /// it. Enters well below the fab/drones' refill equilibrium and leaves
    /// only once the water is back above it, so it cannot flap at a healthy
    /// base — whose water+ice hovers at a fixed point (the fab refills
    /// water toward 95%, the drones buffer ice toward 50%).
    #[serde(default)]
    pub water_return: bool,
    /// Drones strip salvage (wreckage, ruins) instead of mining.
    #[serde(default)]
    pub salvage_on: bool,
    pub last_hull_source: String,
    /// Monotonic step counter that seeds each `advance` step's RNG. Seeding
    /// from the clock made every sub-second step share one seed, so random
    /// events fired ~10x less often at 1x than in fast-forward.
    #[serde(default)]
    pub rng_step: u64,
    /// The galaxy options this game was started with (title screen).
    #[serde(default)]
    pub opts: GalaxyOpts,
    /// Repair hysteresis: a repair starts once a whole percent is lost
    /// and runs until the damage is gone, instead of nibbling every tick.
    #[serde(default)]
    pub repairing_hull: bool,
    #[serde(default)]
    pub repairing_wear: bool,

    // ---- the captain ----
    /// The captain is `crew.names[0]`.
    #[serde(default)]
    pub player_sex: Sex,
    /// Birth date in game years (negative: before the game began).
    #[serde(default = "default_birth_y")]
    pub player_birth_y: f64,
    /// Age at which the captain retires and a successor takes over.
    #[serde(default = "default_retire_age")]
    pub retire_age: f64,
    /// The captain's partner, recruited at a port.
    #[serde(default)]
    pub spouse: Option<Recruit>,
    /// A hired combat pilot.
    #[serde(default)]
    pub pilot: Option<Recruit>,

    // ---- commerce ----
    #[serde(default)]
    pub hold: Hold,
}

fn poisson(rng: &mut StdRng, lambda: f64) -> u32 {
    let l = lambda.min(30.0);
    // Knuth: stop once the running product drops below e^-λ.
    let ll = (-l).exp();
    let mut n = 0u32;
    let mut p = 1.0f64;
    loop {
        p *= rng.random::<f64>();
        if p <= ll {
            break;
        }
        n += 1;
    }
    n
}

// Fab recipes.
struct Recipe {
    inputs: &'static [(Res, f64)],
    out: Res,
    byproduct: Option<(Res, f64)>,
    raw_eq: f64, // raw-kg consumed per kg of output
}

const REC_O2: Recipe = Recipe { inputs: &[(Res::Water, 1.125)], out: Res::Oxygen, byproduct: Some((Res::H2, 0.125)), raw_eq: 1.125 };
const REC_WATER: Recipe = Recipe { inputs: &[(Res::Ice, 1.0)], out: Res::Water, byproduct: None, raw_eq: 1.0 };
const REC_FOOD: Recipe = Recipe { inputs: &[(Res::Carbon, 0.5), (Res::Nitrogen, 0.25), (Res::Water, 0.5)], out: Res::Food, byproduct: None, raw_eq: 1.25 };
/// Time constant (game hours) of the displayed resource-rate average.
const RATE_SMOOTH_H: f64 = 0.5;
const REC_MAT: Recipe = Recipe { inputs: &[(Res::Metal, 0.5), (Res::Silicate, 0.5)], out: Res::Mat, byproduct: None, raw_eq: 1.0 };
const REC_MED: Recipe = Recipe { inputs: &[(Res::Metal, 0.4), (Res::Carbon, 0.2), (Res::Water, 0.3)], out: Res::Med, byproduct: None, raw_eq: 0.9 };

/// Nearest neighbor (by distance) in a candidate list.
fn nearest_by_dist<'a>(v: &[&'a Neighbor]) -> Option<&'a Neighbor> {
    v.iter().copied().min_by(|a, b| a.dist_ly.total_cmp(&b.dist_ly))
}

/// Fraction of a mining visit that is actually productive, given the
/// shuttle time star↔planet. Every ore ton mined costs two transits; a
/// world 22 h one-way yields a trickle, a close-in world is nearly all
/// mining. (Round trip ≤ 8 h costs ≤ 50% of the score.)
fn mining_duty(t_h: f64) -> f64 {
    1.0 / (1.0 + 2.0 * t_h / 8.0)
}

/// Autonomy (hours of fab draw) below which the autopilot treats a raw as
/// critical and commits the drones to it. The base test uses the same
/// figure as its safety margin, so a system that passes is one the mining
/// policy can actually hold.
const CRITICAL_HOURS: f64 = 24.0;

/// Can this system serve as a long-term base for *this ship*? Water from
/// ice, food from carbon *and* nitrogen — or a T3 (interstellar) market
/// standing in for any of them. (A T2 market's 60 t of water cannot outlast
/// a big ship, so it is a waypoint, not a base.) The test is a joint
/// work-cycle: for each required ore the
/// ship works its nearest viable world (full on-planet rate must beat the
/// fab's draw by 25%), and shuttles between those worlds while the fab
/// keeps drawing everything.
///
/// Let M be the drone throughput, D_r the fab draw per ore, d_h the worst-
/// case shuttle time between two ore worlds, D = Σ D_r. Per tour the ship
/// spends 2·d_h h in transit, so total mining time must satisfy
///
///   S_req = 1.25·D·2·d_h / (M − 1.25·D)      (no solution if M ≤ 1.25·D)
///
/// — production must cover the draw over the *whole* cycle, transit
/// included, with a 25% margin. Meanwhile each world r can hold the ship
/// for at most
///
///   S_r = (24 + 2·d_h)·D_r / (M − D_r),
///
/// the time to build a 24 h safety buffer plus the round trip before ore
/// r would run dry. A base exists iff S_req ≤ Σ S_r. One ore world
/// (d_h = 0) collapses the transit term and the test is plain viability;
/// small ships (tiny D) tolerate far wider geometries than big ones, which
/// is exactly what the fleet experience says. Fuel is judged separately.
/// Bodies the drones can work safely, with their shuttle distance from the
/// arrival star (AU). Worlds whose air eats the hull are not ore sources the
/// ship can live off.
fn ore_sites(sys: &StarSystem) -> Vec<(f64, &super::planet::Planet)> {
    sys.sites()
        .into_iter()
        .filter(|(_, p)| p.mineable() && p.corrosion() == 0.0)
        .map(|(l, p)| (sys.pos_au(l), p))
        .collect()
}

fn system_can_base(sys: &StarSystem, spec: &ShipSpec) -> bool {
    let civ = sys.civ.as_ref();
    // Only a T3 (interstellar) market is a full resupply station — 2,000,000 t
    // of everything. A T2 (planetary) market stocks ~60 t of water: a fortune
    // to a lone pilot, but a hundred-hour death sentence for a colony drawing
    // 600 kg/h. A T2 market is a waypoint the ship buys from and moves on
    // from, never a base it lives on; settling there is how the ark used to
    // drink a planetary economy dry and choke.
    let market = civ.is_some_and(|c| c.tier >= CivTier::T3);
    if market {
        return true;
    }
    let people = (spec.crew + spec.pax).max(1) as f64;
    let draw = |r: Res| match r {
        Res::Ice => people * (WATER_PER_P_H + O2_PER_P_H * 1.125 + FOOD_PER_P_H * 0.5),
        Res::Carbon => people * (FOOD_PER_P_H * 0.5 + 0.06),
        Res::Nitrogen => people * FOOD_PER_P_H * 0.25,
        _ => 0.0,
    };
    let mine = spec.drones as f64 * spec.drone_yield;
    // Nearest viable world per required ore: (draw, au).
    let mut ores: Vec<(f64, f64)> = Vec::new();
    for r in [Res::Ice, Res::Carbon, Res::Nitrogen] {
        let d = draw(r);
        let mut best_au: Option<f64> = None;
        for (au, p) in ore_sites(sys) {
            if p.deposits.iter().any(|d| d.res == r && d.tons >= 1000.0) {
                best_au = Some(best_au.map(|b| b.min(au)).unwrap_or(au));
            }
        }
        let au = match best_au {
            Some(a) => a,
            None => return false,
        };
        // Duty-weighted: the shuttle round trip to this world is real time
        // in which the fab keeps drawing, so what must cover 1.25x the draw
        // is the effective rate, not the drones' nominal one. Without the
        // duty here, a single ore world at 40+ au passed as a base (the
        // joint cycle below is skipped when all ores share a world) and the
        // colony mined its water net-negative to death on it.
        let duty = mining_duty(0.25 + au * spec.move_time_per_au);
        if mine * duty <= d * 1.25 {
            return false;
        }
        ores.push((d, au));
    }
    // Reachability. The ship lands at the star and must shuttle out to the
    // ore worlds to start work. A world so far out that the crossing drinks
    // more than half the ship's *full* water reserve is effectively
    // unreachable — the crew would die in transit before a drone could start,
    // no matter how rich the deposit. (A giant of ice at 1200 au is not a
    // water source; it is a monument.) Judge the furthest required world.
    let max_au = ores.iter().map(|(_, au)| *au).fold(0.0f64, f64::max);
    let star_to_ore_h = 0.25 + max_au * spec.move_time_per_au;
    let water_cap: f64 = spec
        .tanks
        .iter()
        .filter(|(r, _)| *r == Res::Water || *r == Res::Ice)
        .map(|(_, c)| *c)
        .sum();
    let max_water_h = water_cap / draw(Res::Ice).max(1e-9);
    if star_to_ore_h > max_water_h * 0.5 {
        return false;
    }
    // Worst-case shuttle time between two ore worlds (max au − min au).
    let mut aus: Vec<f64> = ores.iter().map(|(_, au)| *au).collect();
    aus.sort_by(|a, b| a.total_cmp(b));
    aus.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    let d_h = if aus.len() >= 2 {
        0.25 + (aus[aus.len() - 1] - aus[0]) * spec.move_time_per_au
    } else {
        0.0
    };
    let d_total: f64 = ores.iter().map(|(d, _)| *d).sum();
    if d_h <= 0.0 {
        // All required ores share one world: no inter-world shuttles, so the
        // single constraint is that the world's effective rate (duty-
        // weighted) carries the whole fab. Per-ore margins do not sum —
        // three ores each "passing" 1.25x still leaves the drones a third of
        // a shift short of the combined draw.
        let duty = mining_duty(0.25 + max_au * spec.move_time_per_au);
        return mine * duty > d_total * 1.25;
    }
    if mine <= d_total * 1.25 {
        return false;
    }
    let s_req = 1.25 * d_total * 2.0 * d_h / (mine - 1.25 * d_total);
    let s_max: f64 = ores
        .iter()
        .map(|(d, _)| (CRITICAL_HOURS + 2.0 * d_h) * d / (mine - d))
        .sum();
    s_req <= s_max
}

/// Fuel plus base-ability: the full "we could live here" test.
fn system_can_settle(gal: &mut Galaxy, id: u64, spec: &ShipSpec) -> bool {
    let s = gal.system(id);
    fuel_sustains(s, spec) && system_can_base(s, spec)
}

/// Can this system keep the reactor fed: a star wind strong enough, or a
/// giant's envelope to scoop?
fn fuel_sustains(s: &StarSystem, spec: &ShipSpec) -> bool {
    s.scoop_at(Loc::Star) * spec.scoop_rate > spec.cruise_burn() * 1.05
        || (s.companion_dockable() && s.scoop_at(Loc::Companion) * spec.scoop_rate > spec.cruise_burn() * 1.05)
        || s.planets.iter().any(|p| p.scoopable())
}

/// How far out an ore world can be and still be *usable*: the same margin
/// the base test applies to a whole system. The shuttle there must take
/// less than half the ship's full water reserve, or the crew drinks the
/// crossing dry before a drone could start. A giant of ice at 1200 au is
/// not a water source; it is a monument.
fn max_ore_au(spec: &ShipSpec) -> f64 {
    let water_cap: f64 = spec
        .tanks
        .iter()
        .filter(|(r, _)| *r == Res::Water || *r == Res::Ice)
        .map(|(_, c)| *c)
        .sum();
    let people = (spec.crew + spec.pax) as f64;
    let draw = people * (WATER_PER_P_H + O2_PER_P_H * 1.125 + FOOD_PER_P_H * 0.5).max(1e-9);
    ((0.5 * water_cap / draw - 0.25) / spec.move_time_per_au.max(1e-9)).max(0.0)
}

/// Can the drones mine ice here faster than the fab drinks it — the line
/// between a system where staying put rebuilds the water reserve and one
/// where every hour is a slow bleed. A duty-weighted single-ore version of
/// the base test's 1.25x margin: the shuttle round trip to the ore world is
/// real time in which the fab keeps drawing, and the drones also have to
/// share with carbon and nitrogen, so the margin must carry both.
fn system_works_ice(sys: &StarSystem, spec: &ShipSpec) -> bool {
    let people = (spec.crew + spec.pax).max(1) as f64;
    let draw = people * (WATER_PER_P_H + O2_PER_P_H * 1.125 + FOOD_PER_P_H * 0.5);
    let mine = spec.drones as f64 * spec.drone_yield;
    ore_sites(sys)
        .into_iter()
        .filter(|(_, p)| p.deposits.iter().any(|d| d.res == Res::Ice && d.tons >= 1000.0))
        .any(|(au, _)| {
            let duty = mining_duty(0.25 + au * spec.move_time_per_au);
            mine * duty >= draw * 1.25
        })
}

/// Can this system refill the ship's water — a usable ice deposit (close
/// enough to work), or a market that stocks water (T2 sells 60 t, T3 sells
/// everything)? A starved ship aims at the nearest such system even when no
/// full base is nearby: it mines or buys its way back to a buffer, then
/// moves on.
fn system_has_water(gal: &Galaxy, id: u64, max_au: f64) -> bool {
    let s = gal.system_ref(id);
    let ice = ore_sites(s)
        .into_iter()
        .any(|(au, p)| au <= max_au && p.deposits.iter().any(|d| d.res == Res::Ice && d.tons >= 1000.0));
    let market = s.civ.as_ref().is_some_and(|c| c.tier >= super::civ::CivTier::T2);
    ice || market
}

/// Does this system hold a usable mineable deposit of the ore (1000 t or
/// more, on a world close enough to work)?
fn system_mineable_ore(gal: &Galaxy, id: u64, r: Res, max_au: f64) -> bool {
    ore_sites(gal.system_ref(id))
        .into_iter()
        .any(|(au, p)| au <= max_au && p.deposits.iter().any(|d| d.res == r && d.tons >= 1000.0))
}

/// Can this system supply a given survival raw — a usable mineable deposit
/// (the real answer: a source the drones can keep working), or a market
/// that stocks it (the respite: T2 sells water and food, covering ice/
/// carbon/nitrogen needs; T3 stocks every raw)? Used to aim a starved ship
/// at the thing it is actually running out of, not just water in general:
/// a colony with full water tanks and an empty food fab must go find
/// carbon, not another lake of ice.
fn system_has_ore(gal: &Galaxy, id: u64, r: Res, max_au: f64) -> bool {
    if system_mineable_ore(gal, id, r, max_au) {
        return true;
    }
    let s = gal.system_ref(id);
    let Some(civ) = s.civ.as_ref() else {
        return false;
    };
    match r {
        // T2 markets sell water (the ice need) and food (the carbon and
        // nitrogen needs): finite, but enough to buy time until the ship
        // reaches a real source.
        Res::Ice | Res::Carbon | Res::Nitrogen => civ.tier >= CivTier::T2,
        // Anything else only an interstellar market stocks.
        _ => civ.tier >= CivTier::T3 && s.trade_stock.contains_key(&r),
    }
}

/// Tank target the mining drones refill ore toward (0 = not mined).
fn buffer_target(r: Res) -> f64 {
    match r {
        Res::Ice | Res::Metal | Res::Silicate => 0.5,
        Res::Carbon | Res::Nitrogen => 0.4,
        _ => 0.0,
    }
}

impl Sim {
    pub fn ship(&self) -> &'static ShipSpec {
        ShipSpec::spec(self.ship_idx)
    }

    pub fn people(&self) -> f64 {
        (self.crew.count + self.pax.count) as f64
    }

    /// Raw ore demand (kg/h) for a resource: just the population's fab
    /// needs (life support + food) plus hull/maintenance upkeep. This is the
    /// first claim on the mining drones.
    pub fn raw_base(&self, r: Res) -> f64 {
        let people = self.people().max(1.0);
        let hmax = self.ship().hull;
        match r {
            // Water is made 1:1 from ice; O2 fab draws 1.125 water per O2;
            // food 0.5 water per kg.
            Res::Ice => people * (WATER_PER_P_H + O2_PER_P_H * 1.125 + FOOD_PER_P_H * 0.5),
            Res::Carbon => people * (FOOD_PER_P_H * 0.5 + 0.06),
            Res::Nitrogen => people * FOOD_PER_P_H * 0.25,
            Res::Metal => people * 0.03
                + if self.hull < hmax * 0.75 {
                    3.0
                } else {
                    0.0
                }
                + if self.inv.fraction(Res::Med) < 0.3 {
                    1.5
                } else {
                    0.0
                },
            Res::Silicate => people * 0.03 + if self.hull < hmax * 0.75 { 3.0 } else { 0.0 },
            _ => 0.0,
        }
    }

    /// Can the ship survive a hop of `dist` light-years on the water it has
    /// now? Water (+ice, which converts 1:1) is drawn the whole time the ship
    /// is off a water source — the docked precharge, the flight, and the first
    /// hours at the destination — and the hop must still leave an 8 % buffer.
    /// Used to gate precharging: a ship that cannot outlast its committed
    /// crossing must stop charging and mine its water back up, not stand on
    /// the pad burning H2 and drinking the crew.
    fn hop_liveable(&self, spec: &ShipSpec, dist: f64) -> bool {
        let wi = self.inv.get(Res::Water) + self.inv.get(Res::Ice);
        let draw_ice = self.raw_base(Res::Ice);
        let cap_wi = self.inv.cap(Res::Water) + self.inv.cap(Res::Ice);
        let thr = ShipSpec::throttle_factor(self.ftl_throttle).0;
        let prep_per_ly = spec.precharge_mwh_ly * (1.0 + self.ftl_wear / 100.0)
            / spec.precharge_mw.max(1e-9);
        let trip_per_ly = 1.0 / (spec.ftl_speed * thr).max(1e-9);
        wi >= draw_ice * ((prep_per_ly + trip_per_ly) * dist + 6.0) + 0.08 * cap_wi
    }

    /// Full mining weight (kg/h) for a resource at an explicit buffer target:
    /// the population's fab draw plus a buffer term that refills the tank
    /// toward `target` (~5%/h of tank capacity per unit of deficit). The
    /// buffer only earns drones after the population demand is met, so a full
    /// tank can never starve the crew. The mining policy passes a market-aware
    /// target (working capital at a resupply station); `buffer_target` is the
    /// default.
    fn raw_need_target(&self, r: Res, target: f64) -> f64 {
        let buffer = (target - self.inv.fraction(r)).max(0.0) * self.inv.cap(r) * 0.05;
        self.raw_base(r) + buffer
    }

    /// What the scoop takes in per hour where the ship is now (before any
    /// brownout), or None if there is nothing to scoop here. Stars give H2;
    /// giants give H2 and N2 (and carbon, from sky life).
    pub fn scoop_yield(&self, sys: &StarSystem) -> Option<ScoopYield> {
        let faulted = self.faults.iter().any(|f| f.kind == FaultKind::Scoop);
        let base = self.scoop_rate() * if faulted { 0.5 } else { 1.0 };
        if self.loc.is_stellar() {
            let k = sys.scoop_at(self.loc);
            return (k > 0.0).then_some(ScoopYield { h2: base * k, n2: 0.0, carbon: 0.0 });
        }
        let p = sys.site(self.loc).filter(|p| p.scoopable())?;
        let mult = if p.class == super::planet::PlanetClass::GasGiant { 3.5 } else { 1.5 };
        let rate = base * mult * p.gg_feature.map(|f| f.scoop_mult()).unwrap_or(1.0);
        let sky_life = p.life.is_some_and(|l| matches!(l, super::planet::LifeType::SkyEcosystem | super::planet::LifeType::SkyLeviathans));
        Some(ScoopYield { h2: rate * 0.7, n2: rate * 0.3, carbon: if sky_life { rate * 0.05 } else { 0.0 } })
    }

    /// Reactor output (MW) at the current setting.
    pub fn power_gen(&self) -> f64 {
        let spec = self.ship();
        let faulted = self.faults.iter().any(|f| f.kind == FaultKind::Reactor);
        spec.reactor_mw * (self.reactor_pct as f64 / 100.0) * if faulted { 0.55 } else { 1.0 }
    }

    /// Power draw (MW) of everything switched on and working right now.
    /// The one place the load is defined: the sim and the UI both use it.
    pub fn power_load(&self, sys: &StarSystem) -> f64 {
        let spec = self.ship();
        let busy = self.shuttle.is_some() || self.trip.is_some();
        let mut load = spec.ls_mw * (self.ls_eff as f64 / 100.0);
        // The scoop draws power wherever it is actually scooping (stars
        // and giants alike).
        if self.scoop_on && !busy && self.scoop_yield(sys).is_some() {
            load += spec.scoop_mw;
        }
        if self.mining_on && !busy && self.loc.is_body() && sys.site(self.loc).is_some() {
            load += spec.drones as f64 * spec.drone_mw;
        }
        if self.focus != Focus::Idle {
            load += spec.fab_mw;
        }
        if self.shields {
            load += spec.shields_mw;
        }
        if self.trip.is_some() {
            load += spec.ftl_mw;
        }
        if self.trip.is_none() && self.ftl_charge < 100.0 {
            load += spec.precharge_mw;
        }
        load
    }

    /// H2 kg/h the reactor is burning at the current load.
    pub fn reactor_burn_now(&self, sys: &StarSystem) -> f64 {
        self.ship().burn_at(self.power_load(sys).min(self.power_gen()))
    }

    /// Full-crew drone output, kg/h (ship × the Resources option).
    pub fn drone_rate(&self) -> f64 {
        let spec = self.ship();
        spec.drones as f64 * spec.drone_yield * self.opts.drone_mult() * self.crew_bonus()
    }

    /// A partner aboard the single-crew Keelclipper helps with everything.
    pub fn crew_bonus(&self) -> f64 {
        if self.ship().crew == 1 && self.spouse.is_some() { 1.1 } else { 1.0 }
    }

    /// Scoop H2+N2 kg/h at a solar-mass G star (ship × Resources option).
    pub fn scoop_rate(&self) -> f64 {
        self.ship().scoop_rate * self.opts.scoop_mult() * self.crew_bonus()
    }

    /// Scale for the rate of negative random events (the Risk option).
    /// Every new kind of mishap should multiply its odds by this.
    pub fn risk_mult(&self) -> f64 {
        self.opts.risk.event_mult()
    }

    /// Nanobot repair of hull and FTL wear, paid in MAT.
    fn repair_hull_and_wear(&mut self, dt_h: f64, hmax: f64) {
        // Start only once a whole percent is lost (99.0%, not 99.9%), then
        // finish the job: trickle damage (radiation through shields) is
        // fixed in batches instead of a constant on/off flicker.
        if self.hull <= hmax * 0.99 {
            self.repairing_hull = true;
        } else if self.hull >= hmax {
            self.repairing_hull = false;
        }
        if self.ftl_wear >= 1.0 {
            self.repairing_wear = true;
        } else if self.ftl_wear <= 0.0 {
            self.repairing_wear = false;
        }
        // Spend only the MAT the damage actually needs, never the full
        // hourly rate for a sliver of damage.
        if self.repair_on && self.repairing_hull && self.hull < hmax && self.inv.get(Res::Mat) > 0.0 {
            let need = (hmax - self.hull) / 0.5 * 0.8;
            let use_mat = (0.8 * self.crew_bonus() * dt_h).min(need).min(self.inv.get(Res::Mat));
            self.inv.take(Res::Mat, use_mat);
            self.hull = (self.hull + (use_mat / 0.8) * 0.5).min(hmax);
        }
        if self.repair_on && self.repairing_wear && self.ftl_wear > 0.0 && self.inv.get(Res::Mat) > 0.0 {
            let need = self.ftl_wear / 0.25 * 1.5;
            let use_mat = (1.5 * self.crew_bonus() * dt_h).min(need).min(self.inv.get(Res::Mat));
            self.inv.take(Res::Mat, use_mat);
            self.ftl_wear = (self.ftl_wear - (use_mat / 1.5) * 0.25).max(0.0);
        }
    }

    pub fn t_h(&self) -> f64 {
        self.t / 3600.0
    }

    pub fn new_game(seed: u64, ship_idx: usize, gal: &mut Galaxy) -> Sim {
        let spec = ShipSpec::spec(ship_idx);
        let mut rng = derive(seed, 0xc0ffee);

        let mut inv = Inventory::new(&spec.tanks);
        for (r, f) in spec.start_frac {
            inv.fill_to_fraction(r, f);
        }

        let crew_names: Vec<String> = (0..spec.crew).map(|_| name(&mut rng)).collect();
        let crew = CrewInfo { count: spec.crew, names: crew_names, health: 100.0, morale: 80.0 };
        let pax = if spec.pax > 0 {
            CrewInfo { count: spec.pax, names: vec![], health: 100.0, morale: 70.0 }
        } else {
            CrewInfo { count: 0, names: vec![], health: 0.0, morale: 0.0 }
        };

        let home = gal.home_id();
        let (star_name, catalog) = {
            let st = &gal.system(home).star;
            (st.name.clone(), st.catalog.clone())
        };

        let mut sim = Sim {
            seed,
            ship_idx,
            t: 0.0,
            ff: 1,
            inv,
            hull: spec.hull,
            credits: spec.credits,
            ftl_charge: 0.0,
            ftl_wear: 0.0,
            ftl_throttle: 100,
            reactor_pct: 100,
            shields: false,
            ls_eff: 100,
            lights: true,
            scoop_on: true,
            mining_on: true,
            drone_auto: true,
            autopilot_on: false,
            mine_target: None,
            focus: Focus::LifeSupport,
            repair_on: true,
            sys_id: home,
            loc: Loc::Star,
            shuttle: None,
            trip: None,
            destination: None,
            faults: vec![],
            crew,
            pax,
            debt_crew: 0.0,
            debt_pax: 0.0,
            rates: BTreeMap::new(),
            stats: Stats { systems: 1, ..Default::default() },
            log: Log::default(),
            over: None,
            visited: vec![home],
            autosave_h: 0.0,
            warned: BTreeMap::new(),
            last_brownout: false,
            committed_ore: None,
            water_return: false,
            salvage_on: false,
            last_hull_source: "—".into(),
            rng_step: 0,
            opts: gal.opts,
            repairing_hull: false,
            repairing_wear: false,
            player_sex: Sex::Male,
            player_birth_y: 0.0,
            retire_age: 70.0,
            spouse: None,
            pilot: None,
            hold: Hold::default(),
        };
        // The captain's age and retirement: their own stream, so adding
        // them changed nothing else about a seed.
        let mut prng = derive(seed, 0x00ca_97a1);
        sim.player_birth_y = -prng.random_range(25.0..35.0);
        sim.retire_age = prng.random_range(60.0..80.0);
        // The Ark sails with everything its colony will need.
        if spec.pax > 0 {
            for (g, n) in super::cargo::colony_kit((spec.pax + spec.crew) as f64, true) {
                sim.hold.add(g, n);
            }
        }

        sim.log.push(0.0, Level::Info, format!("{} docked at {} ({}).", spec.name, star_name, catalog));
        sim.log.push(0.0, Level::Info, "Objective: survive. Scoop fuel, mine, process, jump. The galaxy does not care.");
        if spec.pax > 0 {
            sim.log.push(0.0, Level::Info, format!("{} passengers are aboard. Keep them alive.", spec.pax));
        }
        sim
    }

    // ---------------- player actions ----------------

    pub fn cycle_reactor(&mut self) {
        self.reactor_pct = match self.reactor_pct {
            25 => 50,
            50 => 75,
            75 => 100,
            _ => 25,
        };
        self.log.push(self.t, Level::Info, format!("Reactor output set to {}%.", self.reactor_pct));
    }

    pub fn cycle_throttle(&mut self) {
        self.ftl_throttle = match self.ftl_throttle {
            50 => 75,
            75 => 100,
            _ => 50,
        };
        self.log.push(self.t, Level::Info, format!("FTL throttle set to {}%.", self.ftl_throttle));
    }

    pub fn cycle_ls_eff(&mut self) {
        self.ls_eff = match self.ls_eff {
            50 => 75,
            75 => 100,
            _ => 50,
        };
        self.log.push(self.t, Level::Info, format!("Life support at {}% efficiency.", self.ls_eff));
    }

    pub fn toggle_scoop(&mut self) {
        self.scoop_on = !self.scoop_on;
        self.log.push(self.t, Level::Info, format!("Fuel scoop {}", if self.scoop_on { "ON" } else { "OFF" }));
    }

    pub fn toggle_mining(&mut self) {
        self.mining_on = !self.mining_on;
        self.log.push(self.t, Level::Info, format!("Mining drones {}", if self.mining_on { "deployed" } else { "recalled" }));
    }

    pub fn toggle_shields(&mut self) {
        self.shields = !self.shields;
        self.log.push(self.t, Level::Info, format!("Shields {}", if self.shields { "UP" } else { "down" }));
    }

    pub fn toggle_lights(&mut self) {
        self.lights = !self.lights;
    }

    pub fn toggle_salvage(&mut self) {
        self.salvage_on = !self.salvage_on;
        self.log.push(self.t, Level::Info, format!("Drones set to {}", if self.salvage_on { "salvage wreckage" } else { "mine ore" }));
    }

    pub fn toggle_repair(&mut self) {
        self.repair_on = !self.repair_on;
        self.log.push(self.t, Level::Info, format!("Nanobot repair {}", if self.repair_on { "engaged" } else { "standby" }));
    }

    pub fn cycle_focus(&mut self) {
        self.focus = self.focus.cycle();
        self.log.push(self.t, Level::Info, format!("Fabrication bay focus: {}", self.focus.name()));
    }

    pub fn toggle_drone_auto(&mut self, gal: &Galaxy) {
        self.drone_auto = !self.drone_auto;
        if !self.drone_auto && self.mine_target.is_none() {
            // Default target: biggest deposit on the current planet.
            let sys = gal.system_ref(self.sys_id);
            if let Some(p) = sys.site(self.loc) {
                let best = p
                    .deposits
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.tons.total_cmp(&b.1.tons))
                    .map(|(i, _)| i);
                if let Some(bi) = best {
                    self.mine_target = Some((self.loc, bi));
                }
            }
        }
        self.log.push(self.t, Level::Info, format!("Drone allocation: {}", if self.drone_auto { "automatic (nanobots)" } else { "manual" }));
    }

    /// Point the drones at one deposit (manual allocation). Drones only work
    /// it while the ship is docked at that planet.
    pub fn set_mine_target(&mut self, gal: &Galaxy, body: Loc, deposit: usize) {
        let sys = gal.system_ref(self.sys_id);
        let Some(p) = sys.site(body) else { return };
        let Some(d) = p.deposits.get(deposit) else { return };
        let msg = format!("Drones targeting {} on {}", d.res.name(), p.name);
        self.mine_target = Some((body, deposit));
        if self.drone_auto {
            self.drone_auto = false;
            self.log.push(self.t, Level::Info, "Drone allocation: manual");
        }
        self.log.push(self.t, Level::Info, msg);
    }

    pub fn cycle_mine_target(&mut self, gal: &Galaxy) {
        let sys = gal.system_ref(self.sys_id);
        let here = self.loc;
        if let Some(p) = sys.site(here) {
                if p.deposits.is_empty() {
                    return;
                }
                let cur = self
                    .mine_target
                    .filter(|(p0, _)| *p0 == here)
                    .map(|(_, d)| d)
                    .unwrap_or(usize::MAX);
                let next = cur.wrapping_add(1) % p.deposits.len();
                self.mine_target = Some((here, next));
                let d = &p.deposits[next];
                self.log.push(self.t, Level::Info, format!("Drones targeting {} on {}", d.res.name(), p.name));
            }
    }

    pub fn set_destination(&mut self, id: u64, gal: &mut Galaxy) {
        let name = gal
            .neighbors(gal.system_ref(self.sys_id).pos, 60.0, self.sys_id)
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.star_name.clone())
            .unwrap_or_else(|| format!("{id}"));
        self.retarget(Some(id));
        self.log.push(self.t, Level::Info, format!("Destination set: {name}."));
    }

    pub fn clear_destination(&mut self) {
        self.retarget(None);
    }

    /// Light-years the FTL capacitors are charging for: the destination's
    /// distance, or a nominal 8 ly with none set. Positions are pure
    /// functions of system IDs, so this needs no galaxy access.
    pub fn charge_target_ly(&self, dest: Option<u64>) -> f64 {
        let here = Galaxy::pos_of(self.sys_id);
        dest.filter(|&d| d != self.sys_id)
            .map(|d| {
                let p = Galaxy::pos_of(d);
                let (dx, dy) = (p[0] - here[0], p[1] - here[1]);
                (dx * dx + dy * dy).sqrt().max(0.5)
            })
            .unwrap_or(8.0)
    }

    /// Change destination, conserving the energy already in the
    /// capacitors. Charge is stored as a percentage of the *current* hop, so
    /// without the rescale a ship could charge to 100% for the nominal 8 ly
    /// and then point at a 25 ly star for free.
    fn retarget(&mut self, dest: Option<u64>) {
        if self.trip.is_none() && dest != self.destination {
            let old = self.charge_target_ly(self.destination);
            let new = self.charge_target_ly(dest);
            self.ftl_charge = (self.ftl_charge * old / new).min(100.0);
        }
        self.destination = dest;
    }

    pub fn can_depart(&self) -> bool {
        self.destination.is_some()
            && self.ftl_charge >= 100.0
            && self.trip.is_none()
            && self.shuttle.is_none()
    }

    pub fn start_trip(&mut self, gal: &mut Galaxy) -> bool {
        let dest = match self.destination {
            Some(d) if d != self.sys_id => d,
            _ => return false,
        };
        if !self.can_depart() {
            return false;
        }
        let spec = self.ship();
        let (tf_speed, _) = ShipSpec::throttle_factor(self.ftl_throttle);
        let from = gal.system_ref(self.sys_id).pos;
        let to_sys = gal.system_ref(dest);
        let to = to_sys.pos;
        let dx = to[0] - from[0];
        let dy = to[1] - from[1];
        let dist = (dx * dx + dy * dy).sqrt().max(0.5);
        let speed = spec.ftl_speed * tf_speed;
        let total_h = dist / speed;
        let dust_rate = (dist * spec.wear_per_ly) / total_h;
        let dest_name = format!("{} ({})", to_sys.star.name, to_sys.star.catalog);
        self.log.push(self.t, Level::Good, format!("FTL jump initiated for {dest_name} — {dist:.1} ly, ETA {total_h:.1} h."));
        self.trip = Some(Trip {
            dest_id: dest,
            dest_pos: to,
            dest_name,
            dist_ly: dist,
            total_h,
            remaining_h: total_h,
            dust_rate,
        });
        self.ftl_charge = 0.0;
        true
    }

    /// In-system shuttle between the star and a planet.
    pub fn begin_shuttle(&mut self, gal: &mut Galaxy, to: Loc) -> Result<(), ShuttleBlock> {
        if self.trip.is_some() {
            return Err(ShuttleBlock::Tripping);
        }
        if self.shuttle.is_some() {
            return Err(ShuttleBlock::Shuttling);
        }
        if to == self.loc {
            return Err(ShuttleBlock::AlreadyThere);
        }
        let spec = self.ship();
        let sys = gal.system_ref(self.sys_id);
        let exists = match to {
            Loc::Star => true,
            Loc::Companion => sys.companion_dockable(),
            _ => sys.site(to).is_some(),
        };
        if !exists {
            return Err(ShuttleBlock::Nowhere);
        }
        // Note: no minimum-distance guard — close-in worlds (au < 0.05) and
        // moon hops must still be reachable; the 0.25 h base leg covers them.
        let dist_au = sys.dist_au(self.loc, to);
        let t_h = 0.25 + dist_au * spec.move_time_per_au;
        let fuel = dist_au * spec.move_fuel_per_au;
        let taken = self.inv.take(Res::H2, fuel);
        if taken < fuel * 0.5 {
            self.log.push(self.t, Level::Warn, "Shuttle fuel low — burning reserves.");
        }
        self.shuttle = Some(Move { to, remaining_h: t_h, total_h: t_h });
        self.log.push(self.t, Level::Info, format!("Shuttling to {} — {:.1} h.", loc_label(to, sys), t_h));
        Ok(())
    }

    /// Trade with the local civilization. Positive return = credits received,
    /// negative = credits paid.
    pub fn trade(&mut self, gal: &mut Galaxy, r: Res, buy: bool, want_kg: f64) -> f64 {
        let sys = gal.system(self.sys_id);
        let Some(civ) = &sys.civ else {
            return 0.0;
        };
        let price = sys.trade_prices.get(&r).copied().unwrap_or_else(|| super::civ::base_price(r));
        let (can_sell, sell_mult, can_buy, buy_mult) = match civ.tier {
            CivTier::T3 => (true, 0.7, true, 1.0),
            CivTier::T2 => (r.is_raw(), 0.5, sys.trade_stock.contains_key(&r), 1.2),
            CivTier::T1 => (false, 0.0, false, 0.0),
        };

        if buy {
            if !can_buy {
                self.log.push(self.t, Level::Warn, "The local market has nothing to offer you.");
                return 0.0;
            }
            let stock = sys.trade_stock.get(&r).copied().unwrap_or(0.0);
            if stock < 1e-6 {
                self.log.push(self.t, Level::Warn, format!("The market is out of {}.", r.name()));
                return 0.0;
            }
            let room = (self.inv.cap(r) - self.inv.get(r)).max(0.0);
            if room < 1e-6 {
                self.log.push(self.t, Level::Warn, format!("No room: the {} tank is full.", r.name()));
                return 0.0;
            }
            let cost_per = price * buy_mult;
            let kg = want_kg.min(stock).min(room).min(self.credits / cost_per.max(1e-9));
            if kg <= 0.0 {
                self.log.push(self.t, Level::Warn, "Cannot afford that purchase.");
                return 0.0;
            }
            let added = self.inv.add(r, kg);
            self.credits -= added * cost_per;
            if let Some(st) = sys.trade_stock.get_mut(&r) {
                *st -= added;
            }
            self.stats.trades += 1;
            self.log.push(self.t, Level::Good, format!("Bought {added:.0} kg {} for {:.0} cr.", r.name(), added * cost_per));
            -added * cost_per
        } else {
            if !can_sell {
                self.log.push(self.t, Level::Warn, "They are not interested in that.");
                return 0.0;
            }
            let pay_per = price * sell_mult;
            let kg = want_kg.min(self.inv.get(r));
            if kg <= 0.0 {
                self.log.push(self.t, Level::Warn, format!("You have no {} to sell.", r.name()));
                return 0.0;
            }
            self.inv.take(r, kg);
            self.credits += kg * pay_per;
            self.stats.trades += 1;
            self.log.push(self.t, Level::Good, format!("Sold {kg:.0} kg {} for {:.0} cr.", r.name(), kg * pay_per));
            kg * pay_per
        }
    }

    /// T3 shipyards: buy hull repair with credits. Returns credits spent.
    pub fn repair_hull_t3(&mut self, gal: &mut Galaxy) -> f64 {
        let sys = gal.system(self.sys_id);
        if !sys.civ.as_ref().is_some_and(|c| c.tier == CivTier::T3) {
            self.log.push(self.t, Level::Warn, "No shipyard here — hull repair needs an interstellar (T3) station.");
            return 0.0;
        }
        let spec = self.ship();
        let deficit = (spec.hull - self.hull).max(0.0);
        if deficit < 1.0 {
            self.log.push(self.t, Level::Info, "The yard inspects the hull: nothing to repair.");
            return 0.0;
        }
        if self.credits < 1.0 {
            self.log.push(self.t, Level::Warn, "Cannot afford shipyard repairs.");
            return 0.0;
        }
        let cost = deficit * 8.0;
        let pay = cost.min(self.credits);
        self.hull += pay / 8.0;
        self.credits -= pay;
        if pay > 0.0 {
            self.stats.trades += 1;
            self.log.push(self.t, Level::Good, format!("Starship Yards repaired {:.0} hull for {pay:.0} cr.", pay / 8.0));
        }
        pay
    }

    fn game_over(&mut self, cause: &str, detail: String) {
        if self.over.is_some() {
            return;
        }
        self.log.push(self.t, Level::Critical, format!("FATAL: {cause}. {detail}"));
        self.over = Some(GameOver { cause: cause.into(), detail, t: self.t });
    }

    fn warn_throttled(&mut self, key: &str, level: Level, msg: impl Into<String>) {
        let t = self.t_h();
        if self.warned.get(key).is_some_and(|last| t - *last < 2.0) {
            return;
        }
        self.warned.insert(key.into(), t);
        self.log.push(self.t, level, msg);
    }

    // ---------------- the simulation step ----------------

    /// Advance by `dt_h` in-game hours.
    pub fn advance(&mut self, dt_h: f64, gal: &mut Galaxy) {
        if self.over.is_some() || dt_h <= 0.0 {
            return;
        }
        let spec = self.ship();
        self.rng_step = self.rng_step.wrapping_add(1);
        let mut rng = derive(self.seed ^ 0x57e9_57e9, self.rng_step);

        let before: BTreeMap<Res, f64> = self.inv.stock.clone();
        let t_h = self.t_h();
        let people = self.people();
        let hmax = spec.hull;
        let mut hull_src: Option<&'static str> = None;

        // ---------- fault state for this step ----------
        let fault = |k: FaultKind| self.faults.iter().any(|f| f.kind == k);
        let ls_faulted = fault(FaultKind::LifeSupport);
        let drones_faulted = fault(FaultKind::Drones);
        let coil_faulted = fault(FaultKind::FtlCoil);

        // ---------- trip progress (may arrive) ----------
        if let Some(mut trip) = self.trip.take() {
            let (_, tf_burn) = ShipSpec::throttle_factor(self.ftl_throttle);
            let burn_h2 = spec.ftl_burn * tf_burn * (1.0 + self.ftl_wear / 200.0) * dt_h;
            let have = self.inv.get(Res::H2);
            let fuel_scale = if have >= burn_h2 { 1.0 } else { if have > 0.0 { have / burn_h2 } else { 0.0 } };

            if fuel_scale < 0.35 {
                // FTL drive collapse: emergency re-entry drive flings us back
                // to the origin system, hull taking severe damage.
                let dmg = hmax * 0.35;
                self.hull -= dmg;
                hull_src = Some("FTL drive collapse");
                self.log.push(self.t, Level::Critical, "H2 DEPLETED MID-JUMP — FTL drive collapse!");
                self.log.push(self.t, Level::Critical, format!("Emergency re-entry drive fired. You tumble home to the origin system, hull −{:.0}.", dmg));
                self.shuttle = None;
                self.destination = None;
            } else {
                let advance_h = dt_h * (0.4 + 0.6 * fuel_scale);
                trip.remaining_h -= advance_h;
                self.inv.take(Res::H2, burn_h2.min(have));
                if trip.remaining_h <= 0.0 {
                    // The base dust damage (dist × wear_per_ly) was already
                    // applied continuously in flight via `dust_rate`; only the
                    // drive-wear surcharge lands on arrival.
                    let extra = trip.dist_ly * spec.wear_per_ly * (self.ftl_wear / 200.0);
                    self.hull -= extra;
                    let wear = trip.dist_ly * spec.wear_per_ly + extra;
                    hull_src = Some("interstellar dust (FTL)");
                    self.ftl_wear = (self.ftl_wear + trip.dist_ly * 0.02 + 0.5).min(100.0);
                    let (dest_id, dist) = (trip.dest_id, trip.dist_ly);
                    self.sys_id = dest_id;
                    self.loc = Loc::Star;
                    self.shuttle = None;
                    self.destination = None;
                    // New system, new geology: the old ore commitment is
                    // stale and must not bias mining toward a deposit that
                    // does not exist here.
                    self.committed_ore = None;
                    self.stats.jumps += 1;
                    self.stats.dist_ly += dist;
                    if !self.visited.contains(&dest_id) {
                        self.visited.push(dest_id);
                        self.stats.systems += 1;
                    }
                    let ds = gal.system(dest_id);
                    let civ_msg = match &ds.civ {
                        Some(c) => {
                            let seat = loc_label(c.seat, ds);
                            match (c.kind, c.tier) {
                                (CivKind::Natives, CivTier::T3) => format!("The {} of {seat} run an interstellar station — trade and shipyard services available.", c.name),
                                (CivKind::Natives, CivTier::T2) => format!("The {} of {seat} are a spacefaring people. Their market may trade some goods.", c.name),
                                (CivKind::Natives, CivTier::T1) => format!("You observe the {} on {seat}, a pre-space people. They do not know you are here.", c.name),
                                (CivKind::Colony, CivTier::T3) => format!("{} on {seat} is an industrial colony with a full resupply station and shipyard.", c.name),
                                (CivKind::Colony, CivTier::T2) => format!("{} on {seat} is a colony with a small market.", c.name),
                                (CivKind::Colony, CivTier::T1) => format!("{} on {seat} is a fallen colony. The survivors have nothing to trade.", c.name),
                            }
                        }
                        None => "No signs of civilization. Just stars, ice and rock.".into(),
                    };
                    let extra = ds.companion.as_ref().map(|c| format!(" A {} companion, {}, shares the system.", c.layout.name().to_lowercase(), c.star.desc.to_lowercase()));
                    self.log.push(self.t, Level::Good, format!("Arrived at {} ({}) — {:.1} ly traversed. FTL wear {:.0}%, hull −{:.1}.", ds.star.name, ds.star.catalog, dist, self.ftl_wear, wear));
                    self.log.push(self.t, Level::Info, civ_msg);
                    if let Some(x) = extra {
                        self.log.push(self.t, Level::Info, x.trim().to_string());
                    }
                } else {
                    self.trip = Some(trip);
                }
            }
        }

        // ---------- in-system shuttle progress ----------
        if let Some(mv) = self.shuttle.as_mut() {
            mv.remaining_h -= dt_h;
            if mv.remaining_h <= 0.0 {
                let to = mv.to;
                self.loc = to;
                self.shuttle = None;
                let sysref = gal.system_ref(self.sys_id);
                self.log.push(self.t, Level::Info, format!("Now at {}.", loc_label(to, sysref)));
            }
        }

        // ---------- long-lived borrow of the current system ----------
        let sys = gal.system(self.sys_id);
        let here = self.loc;
        let star_name = match here {
            Loc::Companion => sys.companion.as_ref().map(|c| c.star.name.clone()).unwrap_or_default(),
            _ => sys.star.name.clone(),
        };
        let at_star = here.is_stellar();
        let at_planet = here.is_body() && sys.site(here).is_some();
        // Traits of the body the ship is docked at (moons inherit their
        // parent giant's radiation belts).
        let parent_rad = here.planet().and_then(|i| sys.planets.get(i)).map(|p| p.radiation()).unwrap_or(0.0);
        let (body_corrosion, body_shore, body_danger, body_belt, body_rad, body_host, body_au) = match sys.site(here) {
            Some(p) => (
                p.corrosion(),
                p.shore_leave(),
                p.life.is_some_and(|l| l.dangerous()),
                match p.belt_feature {
                    Some(super::planet::BeltFeature::Dust) => 5.0,
                    _ if p.class == super::planet::PlanetClass::AsteroidBelt => 3.0,
                    _ => 1.0,
                },
                p.radiation().max(parent_rad),
                p.host,
                p.au,
            ),
            None => (0.0, false, false, 1.0, 0.0, super::planet::Host::A, 0.0),
        };
        let infiltrating = sys.site(here).is_some_and(|p| p.atmosphere == Some(super::planet::AtmoKind::Infiltrating));
        let moving = self.shuttle.is_some();
        let tripping = self.trip.is_some();

        // ---------- power budget ----------
        let load = self.power_load(sys);
        let mut output = self.power_gen();

        // Reactor fuel: burn follows the power actually drawn (capped by
        // the reactor setting), not the rating. A ship idling at a red
        // dwarf used to burn as if it were at full throttle.
        let reactor_need = spec.burn_at(load.min(output)) * dt_h;
        let have_h2 = self.inv.get(Res::H2);
        let fuel_scale_r = if reactor_need > 0.0 { (have_h2 / reactor_need).min(1.0) } else { 1.0 };
        if fuel_scale_r < 1.0 {
            output *= fuel_scale_r;
            self.warn_throttled("h2starve", Level::Critical, "REACTOR STARVED OF H2 — power output collapsing. Scoop fuel or die.");
        }
        self.inv.take(Res::H2, reactor_need * fuel_scale_r);

        let brownout = if load > 1e-9 { (output / load).min(1.0) } else { 1.0 };
        if brownout < 1.0 && !self.last_brownout {
            self.log.push(self.t, Level::Warn, format!("POWER BUDGET SHORT ({:.0} of {:.0} MW) — systems browning out.", output, load));
        }
        self.last_brownout = brownout < 1.0;

        // ---------- fuel scoop ----------
        if self.scoop_on && !moving && !tripping
            && let Some(y) = self.scoop_yield(sys)
        {
            self.stats.scooped += self.inv.add(Res::H2, y.h2 * brownout * dt_h);
            self.inv.add(Res::Nitrogen, y.n2 * brownout * dt_h);
            self.inv.add(Res::Carbon, y.carbon * brownout * dt_h);
        }

        // ---------- mining drones ----------
        if self.mining_on && at_planet && !moving && !tripping {
            let morale_eff = 0.75 + 0.25 * (self.crew.morale.min(100.0) / 100.0);
            if !self.salvage_on || sys.site(here).is_none_or(|p| p.salvage.as_ref().is_none_or(|s| s.remaining < 1.0)) {
                let p_name = sys.site(here).map(|p| p.name.clone());
                let p_mineable = sys.site(here).is_some_and(|p| p.mineable());
                if let Some(pname) = &p_name
                    && p_mineable && sys.site(here).is_some_and(|p| p.deposits.iter().any(|d| d.tons >= 1.0)) {
                    let drones = spec.drones as f64 * if drones_faulted { 0.5 } else { 1.0 };
                    let yield_kg = drones * spec.drone_yield * self.opts.drone_mult() * brownout * morale_eff * dt_h;

                    // Allocation: two stages. First the population's raw ore
                    // demand (life support, food, upkeep); second, buffer
                    // refills for low tanks. A nearly-empty buffer tank can
                    // never crowd out the crew's actual needs this way.
                    let deposit_idx: Vec<usize> = sys
                        .site(here)
                        .unwrap()
                        .deposits
                        .iter()
                        .enumerate()
                        .filter(|(_, d)| d.tons >= 1.0)
                        .map(|(di, _)| di)
                        .collect();
                    let alloc: Vec<(usize, f64)> = if self.drone_auto {
                        let res_of = |di: usize| sys.site(here).unwrap().deposits[di].res;
                        let base: Vec<f64> = deposit_idx.iter().map(|&di| self.raw_base(res_of(di))).collect();
                        // Fast lane: a deposit whose ore holds less than a day of fab
                        // draw is refilled to safe (24 h) stock *ahead of* the ordinary
                        // buffer refill. Without it the committed ore creeps back above
                        // the commitment line at the 5%/h buffer rate while the other
                        // ore bleeds at full fab draw for hours — the slow sawtooth
                        // that starves the vitals on a two-world base.
                        let crit: Vec<f64> = deposit_idx
                            .iter()
                            .map(|&di| {
                                let r = res_of(di);
                                let d = self.raw_base(r);
                                if d <= 1e-9 {
                                    return 0.0;
                                }
                                let stock = self.inv.get(r);
                                if stock / d > CRITICAL_HOURS {
                                    return 0.0;
                                }
                                ((CRITICAL_HOURS * d - stock).max(0.0)) / 2.0
                            })
                            .collect();
                        // Working capital: at a full resupply station (T3)
                        // with a short treasury, the bulk ores the fab barely
                        // touches (metal, silicate) are mined to the brim and
                        // sold — the treasury that buys feedstock and medicine
                        // the fab cannot make cheap. Feedstock keeps its
                        // ordinary target; the fab's own feed is never mined
                        // for the cash register.
                        let export_mode = sys
                            .civ
                            .as_ref()
                            .is_some_and(|c| c.tier >= CivTier::T3)
                            && self.credits < 4_000.0;
                        let buff: Vec<f64> = deposit_idx
                            .iter()
                            .map(|&di| {
                                let r = res_of(di);
                                let target = if export_mode && matches!(r, Res::Metal | Res::Silicate) {
                                    0.95
                                } else {
                                    buffer_target(r)
                                };
                                self.raw_need_target(r, target) - self.raw_base(r)
                            })
                            .collect();
                        let cap_kg_h = yield_kg / dt_h.max(1e-9);
                        let per_drone = (cap_kg_h / drones.max(1.0)).max(1e-9);
                        let total_base: f64 = base.iter().sum();
                        let used = cap_kg_h.min(total_base);
                        let spare = cap_kg_h - used;
                        // Critical ores take first claim on the spare capacity,
                        // pro-rata to how far below safe stock they sit; what is
                        // left over goes to the ordinary buffer refill.
                        let total_crit: f64 = crit.iter().sum();
                        let crit_used = total_crit.min(spare);
                        let total_buff: f64 = buff.iter().sum();
                        let buff_spare = (spare - crit_used).max(0.0);
                        let raw: Vec<f64> = base
                            .iter()
                            .zip(crit.iter())
                            .zip(buff.iter())
                            .map(|((b, c), u)| {
                                let k1 = if total_base > 1e-9 { b / total_base * used } else { 0.0 };
                                let k2 = if total_crit > 1e-9 { c / total_crit * crit_used } else { 0.0 };
                                let k3 = if buff_spare > 0.0 && total_buff > 1e-9 {
                                    u / total_buff * buff_spare
                                } else {
                                    0.0
                                };
                                (k1 + k2 + k3) / per_drone
                            })
                            .collect();
                        let mut out: Vec<(usize, f64)> = deposit_idx
                            .iter()
                            .zip(raw.iter())
                            .map(|(&di, r)| (di, r.floor()))
                            .collect();
                        let mut rem = (drones - raw.iter().map(|r| r.floor()).sum::<f64>()).floor();
                        let mut order: Vec<(usize, f64)> = (0..raw.len())
                            .map(|i| (i, raw[i] - raw[i].floor()))
                            .collect();
                        order.sort_by(|a, b| b.1.total_cmp(&a.1));
                        for (i, frac) in order {
                            if rem <= 0.0 {
                                break;
                            }
                            if frac > 1e-9 {
                                let di = deposit_idx[i];
                                if let Some(e) = out.iter_mut().find(|(d0, _)| *d0 == di) {
                                    e.1 += 1.0;
                                }
                                rem -= 1.0;
                            }
                        }
                        out
                    } else {
                        match self.mine_target {
                            Some((tp, td)) if tp == here => vec![(td, drones)],
                            _ => vec![],
                        }
                    };

                    for (di, nd) in alloc {
                        if nd <= 0.0 {
                            continue;
                        }
                        if let Some(d) = sys.site_mut(here).and_then(|p| p.deposits.get_mut(di)) {
                            if d.tons < 1.0 {
                                continue;
                            }
                            let kg = (yield_kg * (nd / drones.max(1.0))).min(d.tons * 1000.0);
                            d.tons -= kg / 1000.0;
                            let got = self.inv.add(d.res, kg);
                            self.stats.mined += got;
                        }
                    }

                    if sys.site(here).is_some_and(|p| p.exhausted()) {
                        self.warn_throttled(
                            &format!("exhausted-{}", self.sys_id),
                            Level::Warn,
                            format!("{} is mined out — no usable deposits left.", pname),
                        );
                    }
                    }
            }
        }

        // ---------- salvage ----------
        // With salvage switched on, the drones strip wreckage instead of
        // mining: structural material and metal, now and then something
        // worth selling.
        if self.salvage_on && self.mining_on && at_planet && !moving && !tripping {
            let morale_eff = 0.75 + 0.25 * (self.crew.morale.min(100.0) / 100.0);
            let drones = spec.drones as f64 * if drones_faulted { 0.5 } else { 1.0 };
            let rate = drones * spec.drone_yield * self.opts.drone_mult() * 2.0 * brownout * morale_eff * dt_h;
            let mut found_cr = 0.0;
            let mut done: Option<String> = None;
            if let Some(sv) = sys.site_mut(here).and_then(|p| p.salvage.as_mut())
                && sv.remaining >= 1.0
            {
                let kg = rate.min(sv.remaining);
                sv.remaining -= kg;
                let mat = self.inv.add(Res::Mat, kg * 0.5);
                let metal = self.inv.add(Res::Metal, kg * 0.5);
                self.stats.mined += mat + metal;
                if rng.random_bool((0.03 * dt_h).min(0.5)) {
                    found_cr = rng.random_range(80.0..600.0);
                }
                if sv.remaining < 1.0 {
                    done = Some(sv.what.clone());
                }
            }
            if found_cr > 0.0 {
                self.credits += found_cr;
                self.log.push(self.t, Level::Good, format!("Salvage crews recovered intact equipment worth {found_cr:.0} cr."));
            }
            if let Some(what) = done {
                self.log.push(self.t, Level::Info, format!("Salvage complete: nothing left worth taking from the {}.", what.to_lowercase()));
            }
        }

        // ---------- fabrication ----------
        // Can water be replenished — from a world with ice we can mine, or
        // from the ice tank itself (the fab keeps converting it in flight,
        // so board ice *is* renewable water)? If not, the fab defends the
        // emergency water reserve.
        let water_renewable = self.inv.fraction(Res::Ice) > 0.02
            || sys
                .site(here)
                .is_some_and(|p| p.mineable() && p.deposits.iter().any(|d| d.res == Res::Ice && d.tons >= 1.0));
        if self.focus != Focus::Idle && brownout > 0.05 {
            let morale_eff = 0.75 + 0.25 * (self.crew.morale.min(100.0) / 100.0);
            // Every non-Idle focus runs the full recipe list in priority
            // order: life support first (the crew dies otherwise), then hull
            // material, then medicine. Exclusive focus (Medical => medicine
            // only) deadlock: a tank hovering at its focus threshold would
            // flip the fab in and out of that mode forever, so nothing but
            // one product was ever made and the departure buffers never
            // filled. The want_h caps already encode urgency, so priority
            // order with caps is both safe and complete.
            let recipes: &[&Recipe] = match self.focus {
                Focus::Idle => &[],
                _ => &[&REC_O2, &REC_WATER, &REC_FOOD, &REC_MAT, &REC_MED],
            };
            let mut budget = spec.fab_rate * self.crew_bonus() * brownout * morale_eff * dt_h; // raw-kg
            // Buffer refills are earned from the *mining surplus*: the
            // drones' nominal yield minus the population's raw draw, halved
            // (the other half stockpiles raws). Without the cap, a 20 h
            // refill term on the big tanks outran a small ship's entire
            // mining output — the solo's ice reserve drained in days and no
            // sortie could ever refill it. In transit or mid-shuttle there
            // is no mining at all, so no buffer is built: the flight runs
            // on pure population demand, exactly as `feasible()` models.
            let pop_raw: f64 = Res::RAW.iter().map(|r| self.raw_base(*r)).sum::<f64>();
            let refill_cap_h = if moving || tripping {
                0.0
            } else {
                (self.drone_rate() - pop_raw).max(0.0) * 0.5
            };
            // Pass 1: what each recipe wants. Pass 2: produce them in
            // priority order. Splitting the passes lets the buffer refills
            // share one *global* cap — with a per-recipe cap the three
            // life-support refills alone drank more than the whole fab
            // budget and starved the repair and medical lines of it.
            let mut plans: Vec<(&Recipe, f64, f64)> = Vec::new(); // (rec, demand_h, refill_h)
            let mut refill_desired: f64 = 0.0;
            for rec in recipes {
                if self.inv.fraction(rec.out) >= 0.98 {
                    continue;
                }
                // Regulator, not a tank-filler: make what the population
                // consumes, plus a steady refill toward 95% over 20 h. 95%
                // keeps every tank measurably above the top departure gate
                // (90%), so a stocked ship can always leave.
                let (demand_h, target_frac) = match rec.out {
                    Res::Oxygen => (people * O2_PER_P_H, 0.95),
                    // Water drawn by drinking plus what the O2 and food fabs
                    // consume per kg they make.
                    Res::Water => (
                        people * (WATER_PER_P_H + O2_PER_P_H * 1.125 + FOOD_PER_P_H * 0.5),
                        0.95,
                    ),
                    Res::Food => (people * FOOD_PER_P_H, 0.95),
                    Res::Mat => (1.5 + if self.hull < hmax * 0.8 { 4.0 } else { 0.0 }, 0.95),
                    // Match the life-support draw exactly (0.02/person crew
                    // line plus 0.01 per sleeping passenger), or the tank
                    // bleeds dry in the first week and never refills.
                    Res::Med => (0.02 * people.max(1.0) + 0.01 * self.pax.count as f64, 0.95),
                    _ => (0.0, 0.95),
                };
                // A big buffer refill must never outrank feeding the crew:
                // while food is critically low, the water recipe makes only
                // what the population and the other fabs consume, so the
                // food fab keeps its share of the budget. (The water tank is
                // the largest one, so its refill term is the one that can
                // swamp the rest.)
                //
                // Likewise a refill spends the ore the crew needs to live.
                // While any ore backing the recipe holds less than ~a third
                // of a day of fab draw, the refill pauses and the fab runs
                // on pure demand; above the threshold the ore side's 5%/h
                // buffer term paces the refill so the two regulators settle
                // into a slow sawtooth instead of a fight. (The threshold
                // sits deliberately below what the ark's ice tank can ever
                // hold — a full day of its own draw is more ice than the
                // tank holds — so a tight base can still build its
                // processed buffers, and it sits above the ore side's
                // per-shuttle recovery, so the pause is not thrash.)
                let mut feed_starved = false;
                for (r, _) in rec.inputs {
                    let raw = if *r == Res::Water { Res::Ice } else { *r };
                    let d = self.raw_base(raw);
                    if d > 1e-9 && self.inv.get(raw) / d < 7.0 {
                        feed_starved = true;
                    }
                }
                let refill_h = if feed_starved
                    || (rec.out == Res::Water && self.inv.fraction(Res::Food) < 0.3)
                {
                    0.0
                } else {
                    (target_frac * self.inv.cap(rec.out) - self.inv.get(rec.out)).max(0.0) / 20.0
                };
                refill_desired += refill_h;
                plans.push((rec, demand_h, refill_h));
            }
            // Share the mining surplus: refills scale together toward the
            // cap instead of the first recipes hogging it.
            let refill_scale = if refill_desired > refill_cap_h && refill_desired > 1e-9 {
                refill_cap_h / refill_desired
            } else {
                1.0
            };
            for (rec, demand_h, refill_h) in plans {
                let want_h = demand_h + refill_h * refill_scale; // kg/h
                let mut max_by_raw = f64::INFINITY;
                for (r, per) in rec.inputs {
                    // The fab never spends water below the emergency reserve —
                    // unless it can mine more ice from right here.
                    let avail = if *r == Res::Water
                        && self.inv.fraction(Res::Water) < 0.55
                        && !water_renewable
                    {
                        ((self.inv.fraction(Res::Water) - 0.45) * self.inv.cap(Res::Water)).max(0.0)
                    } else {
                        self.inv.get(*r)
                    };
                    max_by_raw = max_by_raw.min(avail / per.max(1e-9));
                }
                let room = (self.inv.cap(rec.out) - self.inv.get(rec.out)).max(0.0);
                let out_kg = (budget / rec.raw_eq)
                    .min(max_by_raw)
                    .min(room)
                    .min(want_h * dt_h);
                if out_kg <= 0.0 {
                    continue;
                }
                for (r, per) in rec.inputs {
                    self.inv.take(*r, per * out_kg);
                }
                self.inv.add(rec.out, out_kg);
                if let Some((br, per)) = rec.byproduct {
                    self.inv.add(br, per * out_kg);
                }
                budget -= rec.raw_eq * out_kg;
                self.stats.fabbed += out_kg;
                if budget <= 0.0 {
                    break;
                }
            }
        }

        // ---------- life support ----------
        let ls_scale = if ls_faulted { 0.5 } else { 1.0 };
        let ineff = 2.0 - brownout; // 1.0 normal → 2.0 when blacked out
        let o2_req = people * O2_PER_P_H * ineff * dt_h;
        let w_req = people * WATER_PER_P_H * ineff * dt_h;
        let f_req = people * FOOD_PER_P_H * ineff * dt_h;

        let o2_want = o2_req * ls_scale;
        let o2_got = self.inv.take(Res::Oxygen, o2_want);
        let w_got = self.inv.take(Res::Water, w_req);
        let f_got = self.inv.take(Res::Food, f_req);

        let o2_frac = if o2_want > 0.0 { o2_got / o2_want } else { 1.0 };
        let w_frac = if w_req > 0.0 { w_got / w_req } else { 1.0 };
        let f_frac = if f_req > 0.0 { f_got / f_req } else { 1.0 };

        let crew_deficit = (1.0 - (o2_frac * 0.5 + w_frac * 0.3 + f_frac * 0.2)).max(0.0);
        let mut crew_drain = crew_deficit * 7.0;
        if o2_frac <= 0.001 {
            crew_drain += 18.0;
        }
        if w_frac <= 0.001 {
            crew_drain += 8.0;
        }
        if f_frac <= 0.001 {
            crew_drain += 1.5;
        }
        if brownout <= 0.001 {
            crew_drain += 10.0;
        }

        let med_used = if crew_drain < 1.0 && people > 0.0 {
            self.inv.take(Res::Med, 0.02 * people * dt_h)
        } else {
            0.0
        };
        if crew_drain < 1.0 {
            let rec = if med_used > 0.0 { 4.5 } else { 3.0 };
            self.crew.health = (self.crew.health + rec * dt_h).min(100.0);
        } else {
            self.crew.health = (self.crew.health - crew_drain * dt_h).max(0.0);
        }

        if self.pax.count > 0 {
            let pax_drain = crew_drain * 1.2;
            let pmed = if pax_drain < 1.0 { self.inv.take(Res::Med, 0.01 * self.pax.count as f64 * dt_h) } else { 0.0 };
            if pax_drain < 1.0 {
                let rec = if pmed > 0.0 { 2.0 } else { 1.5 };
                self.pax.health = (self.pax.health + rec * dt_h).min(100.0);
            } else {
                self.pax.health = (self.pax.health - pax_drain * dt_h).max(0.0);
            }
        }

        // ---------- deaths ----------
        if self.crew.count > 0 && self.crew.health <= 0.0 {
            self.debt_crew += dt_h * 0.5; // one crew loss per ~2h at zero health
            if self.debt_crew >= 1.0 {
                self.debt_crew -= 1.0;
                let name_lost = self.crew.names.pop().unwrap_or_else(|| "crew member".into());
                self.crew.count -= 1;
                if self.spouse.as_ref().is_some_and(|r| r.name == name_lost) {
                    self.spouse = None;
                }
                if self.pilot.as_ref().is_some_and(|r| r.name == name_lost) {
                    self.pilot = None;
                }
                self.stats.crew_lost += 1;
                if self.crew.count > 0 {
                    self.crew.health = 55.0;
                    self.log.push(self.t, Level::Bad, format!("Lost {name_lost}. {} crew member(s) remain.", self.crew.count));
                } else {
                    self.game_over("All crew lost", format!("The last crew member, {name_lost}, died in the silence of the ship."));
                }
            }
        } else if self.crew.health > 0.0 {
            self.debt_crew = 0.0;
        }

        if self.pax.count > 0 && self.pax.health <= 0.0 {
            self.debt_pax += dt_h * 5.0;
            let n = self.debt_pax as usize;
            if n > 0 {
                self.debt_pax -= n as f64;
                let n = n.min(self.pax.count);
                self.pax.count -= n;
                self.stats.pax_lost += n as u32;
                self.pax.health = 50.0;
                if self.pax.count == 0 {
                    self.game_over("The colony is lost", format!("Every one of the {} passengers is gone. The ark sails on, empty.", spec.pax));
                } else {
                    self.log.push(self.t, Level::Bad, format!("{n} passengers lost. {} remain.", self.pax.count));
                }
            }
        } else if self.pax.health > 0.0 {
            self.debt_pax = 0.0;
        }

        // ---------- radiation ----------
        let rad = if at_star {
            sys.star_radiation_at(here)
        } else if at_planet {
            let host = sys.host_star(body_host);
            let ratio = (body_au / host.habitable_au().max(0.2)).max(0.1);
            (host.radiation() * (1.0 / (ratio * ratio))).min(0.6) + body_rad
        } else {
            0.0
        };
        if rad > 0.05 {
            if self.shields {
                self.hull -= rad * 0.15 * dt_h;
            } else {
                self.hull -= rad * 0.9 * dt_h;
                hull_src = Some("stellar radiation");
                self.crew.health = (self.crew.health - rad * 1.0 * dt_h).max(0.0);
                if self.pax.count > 0 {
                    self.pax.health = (self.pax.health - rad * 1.0 * dt_h).max(0.0);
                }
                let src = if body_rad > 0.0 { "the giant's radiation belts".to_string() } else { star_name.clone() };
                self.warn_throttled("rad", Level::Warn, format!("High radiation from {src}. Raise shields or move away."));
            }
        }

        // ---------- micrometeorites + interstellar dust ----------
        // Corrosive air eats the hull while the ship sits in it.
        if at_planet && !moving && !tripping && body_corrosion > 0.0 {
            self.hull -= body_corrosion * if self.shields { 0.5 } else { 1.0 } * dt_h;
            hull_src = Some("corrosive atmosphere");
            self.warn_throttled("corrode", Level::Warn, "The atmosphere is eating the hull plating. Don't linger.");
        }
        let lam = if tripping {
            0.10
        } else if at_planet && !moving {
            0.05 * body_belt
        } else {
            0.05
        } * self.risk_mult();
        let n = poisson(&mut rng, lam * dt_h);
        for _ in 0..n {
            let d = rng.random_range(0.05..0.4);
            self.hull -= d;
            hull_src = Some("micrometeorite strike");
        }
        if let Some(trip) = self.trip.as_ref() {
            self.hull -= trip.dust_rate * dt_h;
            if trip.dust_rate > 0.0 {
                hull_src = Some("interstellar dust (FTL)");
            }
        }

        // ---------- faults: spawn / expire / repair ----------
        if self.faults.is_empty() && rng.random_bool((0.025 * self.risk_mult() * dt_h).min(0.5)) {
            let kind = match rng.random::<f64>() {
                r if r < 0.25 => FaultKind::Reactor,
                r if r < 0.5 => FaultKind::LifeSupport,
                r if r < 0.7 => FaultKind::Scoop,
                r if r < 0.9 => FaultKind::Drones,
                _ => FaultKind::FtlCoil,
            };
            let dur = rng.random_range(2.0..10.0);
            self.faults.push(Fault { kind, until_h: t_h + dur, mat_fix: rng.random_range(5.0..15.0) });
            self.log.push(self.t, Level::Warn, format!("{} — nanobots are on it (feed them MAT to finish sooner).", kind.name()));
        }
        // Local hazards: wildlife that tears into the drones, and air that
        // seeps through every seal into life support.
        if self.faults.is_empty() && at_planet && !moving && !tripping {
            let drone_risk = if body_danger && self.mining_on { 0.05 * self.risk_mult() } else { 0.0 };
            let ls_risk = if infiltrating { 0.05 * self.risk_mult() } else { 0.0 };
            let kind = if drone_risk > 0.0 && rng.random_bool((drone_risk * dt_h).min(0.5)) {
                Some((FaultKind::Drones, "Native wildlife attacked the drones"))
            } else if ls_risk > 0.0 && rng.random_bool((ls_risk * dt_h).min(0.5)) {
                Some((FaultKind::LifeSupport, "The atmosphere seeped past the seals into life support"))
            } else {
                None
            };
            if let Some((kind, why)) = kind {
                let dur = rng.random_range(2.0..10.0);
                self.faults.push(Fault { kind, until_h: t_h + dur, mat_fix: rng.random_range(5.0..15.0) });
                self.log.push(self.t, Level::Warn, format!("{why}: {} — nanobots are on it.", kind.name()));
            }
        }
        let mut cleared = Vec::new();
        self.faults.retain(|f| {
            if f.until_h <= t_h {
                cleared.push(f.kind);
                false
            } else {
                true
            }
        });
        for k in cleared {
            self.log.push(self.t, Level::Good, format!("{} cleared.", k.name()));
        }
        if self.repair_on && !self.faults.is_empty() && self.inv.get(Res::Mat) >= self.faults.first().map(|f| f.mat_fix).unwrap_or(f64::MAX) {
            let f = self.faults.remove(0);
            self.inv.take(Res::Mat, f.mat_fix);
            self.log.push(self.t, Level::Good, format!("{} repaired early (−{:.0} MAT).", f.kind.name(), f.mat_fix));
        }

        // ---------- hull & FTL wear repair ----------
        self.repair_hull_and_wear(dt_h, hmax);

        // ---------- morale ----------
        let good = self.inv.fraction(Res::Oxygen) > 0.5
            && self.inv.fraction(Res::Water) > 0.5
            && self.inv.fraction(Res::Food) > 0.5
            && self.hull > hmax * 0.5;
        let mut dm = if good { 1.0 } else { -2.0 };
        // Shore leave: fresh air under an open sky.
        if body_shore && !moving && !tripping {
            dm += 1.5;
        }
        if !self.lights {
            dm -= 0.5;
        }
        if tripping {
            dm -= 0.3;
        }
        if brownout < 1.0 {
            dm -= 3.0;
        }
        if self.crew.health < 50.0 {
            dm -= 1.0;
        }
        self.crew.morale = (self.crew.morale + dm * dt_h).clamp(0.0, 100.0);
        if self.pax.count > 0 {
            self.pax.morale = (self.pax.morale + (dm - 0.2) * dt_h).clamp(0.0, 100.0);
        }

        // ---------- hull floor & game over ----------
        if let Some(src) = hull_src {
            self.last_hull_source = src.into();
        }
        self.hull = self.hull.max(0.0);
        if self.hull <= 0.0 {
            self.game_over("Hull destroyed", format!("Final blow: {}.", self.last_hull_source));
        }

        // ---------- FTL charge (destination position is a pure fn of its ID) ----------
        if self.trip.is_none() && self.ftl_charge < 100.0 {
            let target_dist = self.charge_target_ly(self.destination);
            // Only charge for a crossing the crew can survive. Charging burns
            // H2 that a mining sortie would need; a ship whose water cannot
            // outlast the hop must stop charging and mine it back up, not
            // stand on the pad and drink the crew dry.
            if self.hop_liveable(spec, target_dist) {
                let need_mwh = target_dist * spec.precharge_mwh_ly * (1.0 + self.ftl_wear / 100.0) * if coil_faulted { 1.3 } else { 1.0 };
                let pw = spec.precharge_mw * brownout * if coil_faulted { 0.7 } else { 1.0 };
                self.ftl_charge = (self.ftl_charge + (pw * dt_h / need_mwh.max(1e-6)) * 100.0).min(100.0);
                if self.ftl_charge >= 100.0 {
                    self.log.push(self.t, Level::Good, "FTL capacitors charged — ready to jump.");
                }
            }
        }

        // ---------- warnings ----------
        if self.inv.fraction(Res::H2) < 0.1 {
            self.warn_throttled("h2crit", Level::Critical, format!("H2 FUEL CRITICAL: {:.1}% of tank.", self.inv.fraction(Res::H2) * 100.0));
        } else if self.inv.fraction(Res::H2) < 0.25 {
            self.warn_throttled("h2low", Level::Warn, format!("H2 fuel low: {:.1}% of tank.", self.inv.fraction(Res::H2) * 100.0));
        }
        for r in [Res::Oxygen, Res::Water, Res::Food] {
            let f = self.inv.fraction(r);
            if f < 0.1 {
                self.warn_throttled(&format!("crit{}", r), Level::Critical, format!("{} at {:.1}% — people are in danger.", r.name(), f * 100.0));
            } else if f < 0.25 {
                self.warn_throttled(&format!("low{}", r), Level::Warn, format!("{} at {:.1}% of capacity.", r.name(), f * 100.0));
            }
        }
        if self.hull < hmax * 0.5 {
            self.warn_throttled("hull", Level::Warn, format!("Hull at {:.0}% — run repairs.", self.hull / hmax * 100.0));
        }
        if self.crew.health < 50.0 {
            self.warn_throttled("health", Level::Warn, format!("Crew health at {:.0} — check life support.", self.crew.health));
        }
        if self.ftl_wear > 60.0 {
            self.warn_throttled("wear", Level::Warn, format!("FTL wear at {:.0}% — jumps cost more fuel and time.", self.ftl_wear));
        }

        // ---------- bookkeeping ----------
        // Displayed rates are averaged over ~half a game hour. A raw
        // per-step delta flickers red/green whenever something runs in
        // bursts (the fab topping a tank up, a repair batch, the drones).
        let k = 1.0 - (-dt_h / RATE_SMOOTH_H).exp();
        for r in Res::ALL {
            let now = (self.inv.get(r) - before.get(&r).copied().unwrap_or(0.0)) / dt_h;
            let prev = self.rates.get(&r).copied().unwrap_or(now);
            self.rates.insert(r, prev + (now - prev) * k);
        }
        self.t += dt_h * 3600.0;
        self.autosave_h += dt_h;
    }

    // ---------------- autopilot ----------------
    // The ship's autonomous brain: it picks destinations, shuttles to ore
    // worlds, allocates the drones, trades, and starts trips. Headless mode
    // (--sim, the balance regression tests) runs it every step to prove the
    // game is survivable. In the TUI it stays dormant unless `autopilot_on`
    // is set — hands-off mode, whose key wiring is the only piece that
    // feature still needs (see `App::tick`).

    pub fn autopilot(&mut self, gal: &mut Galaxy) {
        if self.over.is_some() {
            return;
        }
        let spec = self.ship();
        let hmax = spec.hull;
        let sys = gal.system_ref(self.sys_id);
        // Market info and local mineability up front: after the movement
        // block below, `sys` must not be used again (shuttle/destination
        // calls borrow `gal` mutably).
        let civ_tier = sys.civ.as_ref().map(|c| c.tier);
        let t2_stock: Vec<Res> = sys.trade_stock.keys().copied().collect();
        // What the drones can actually work here: a usable deposit (close
        // enough that the round trip is worth the water). A giant of ore at
        // 1200 au does not count — committing the drones to it, or believing
        // the ship can live off it, is how a colony once stood and starved
        // next to an ocean of ice it could never reach.
        let max_au = max_ore_au(spec);
        let raw_mineable: Vec<Res> = Res::RAW
            .iter()
            .filter(|&&r| {
                r != Res::H2
                    && ore_sites(sys)
                        .iter()
                        .any(|(au, p)| *au <= max_au && p.deposits.iter().any(|d| d.res == r && d.tons >= 1000.0))
            })
            .copied()
            .collect();
        // Can this system's water be rebuilt by staying and mining —
        // workable ice, or a station that sells it? A ship below its
        // water-return line at a system that CAN sustain it just needs to
        // stay put; only an unsustainably dry system sends it home. Without
        // this distinction a ship ping-ponged between two marginal bases
        // forever, each jump restarting the ice sawtooth before it could
        // cross the fab's refill gate.
        let ice_sustainable =
            system_works_ice(sys, spec) || matches!(civ_tier, Some(CivTier::T3));
        let at_star = self.loc.is_stellar();
        // Every dockable body (planets, moons, belts), indexed together.
        let sites = sys.sites();
        let site_locs: Vec<Loc> = sites.iter().map(|(l, _)| *l).collect();
        let site_au: Vec<f64> = site_locs.iter().map(|l| sys.pos_au(*l)).collect();
        let planet_idx = site_locs.iter().position(|l| *l == self.loc);
        let here_rad = sys.site(self.loc).map(|p| p.radiation()).unwrap_or(0.0)
            + self.loc.planet().and_then(|i| sys.planets.get(i)).map(|p| p.radiation()).unwrap_or(0.0);

        self.reactor_pct = 100;
        self.ls_eff = 100;
        self.lights = true;
        self.shields = sys.star_radiation_at(Loc::Star) > 0.05 || sys.star_radiation_at(self.loc) > 0.05 || here_rad > 0.0;
        self.drone_auto = true;
        self.salvage_on = false; // the autopilot lives off ore, not luck

        let h2f = self.inv.fraction(Res::H2);
        let fuel_crit = h2f < 0.7; // go hunt for fuel, stop doing everything else

        // Scoop anywhere there is atmosphere: at the star, or in a gas/ice
        // giant's envelope (which is far richer).
        let here_scoopable = planet_idx.is_some_and(|i| sites[i].1.scoopable());
        self.scoop_on = at_star || here_scoopable;

        // Best fuel source in the system: the star wind is always available
        // (duty 1); a giant's envelope is richer per hour but costs two
        // transits per visit, so it only wins when it is close in *and* rich.
        let star_fuel = sys.scoop_at(Loc::Star) * self.scoop_rate();
        let mut fuel_planet: Option<usize> = None;
        let mut fuel_best = star_fuel;
        for (i, (_, p)) in sites.iter().enumerate() {
            if !p.scoopable() {
                continue;
            }
            let mult = if p.class == super::planet::PlanetClass::GasGiant {
                3.5
            } else {
                1.5
            };
            let eff = self.scoop_rate() * mult * 0.7 * p.gg_feature.map(|f| f.scoop_mult()).unwrap_or(1.0)
                * mining_duty(0.25 + site_au[i] * spec.move_time_per_au);
            if eff > fuel_best {
                fuel_best = eff;
                fuel_planet = Some(i);
            }
        }

        // Can this system sustain us on fuel alone (star wind or a giant)?
        let fuel_sustain = star_fuel > spec.cruise_burn() * 1.05 || fuel_planet.is_some();
        // A "settled" system is a base: fuel AND everything life support
        // needs long-term (water source, food sources, or a market).
        // Everything else is a waypoint — work briefly, then move on.
        let settled = fuel_sustain && system_can_base(sys, spec);

        // Raw-material need, independent of fuel (fuel strategy is handled by
        // fuel_crit/out_gate separately) — gating it on fuel levels made the
        // ship ping-pong at the H2 boundary.
        let need_mine = self.inv.fraction(Res::Water) < 0.55
            || self.inv.fraction(Res::Oxygen) < 0.5
            || self.inv.fraction(Res::Food) < 0.5
            || self.inv.fraction(Res::Ice) < 0.45
            || self.inv.fraction(Res::Metal) < 0.4
            || self.inv.fraction(Res::Silicate) < 0.4
            || self.inv.fraction(Res::Carbon) < 0.35
            || self.inv.fraction(Res::Nitrogen) < 0.35
            || self.hull < hmax * 0.75
            || self.ftl_wear > 40.0;

        // Best planet to mine now. Two modes, with hysteresis:
        //  * buffer mode — some ore tank is below (target − 5%): work the
        //    planet that closes the biggest shortfall (kg short × size).
        //  * feed mode — buffers full: work the planet that feeds the fab
        //    (ongoing kg/h × size), keeping the life-support loop alive.
        // (Scoring one mode on the other would let a permanently-demanded
        // resource like ice shadow every other ore forever.)
        let ore_res: [Res; 5] = [Res::Ice, Res::Metal, Res::Silicate, Res::Carbon, Res::Nitrogen];
        let in_buffer_mode = ore_res
            .iter()
            .any(|r| self.inv.fraction(*r) < buffer_target(*r) - 0.05);
        // How many hours of each ore are left at the current fab draw? For ice
        // the real stock is water + ice (ice converts 1:1 to water).
        let ore_autonomy_h = |r: Res| {
            let kg = if r == Res::Ice {
                self.inv.get(Res::Water) + self.inv.get(Res::Ice)
            } else {
                self.inv.get(r)
            };
            kg / self.raw_base(r).max(1e-9)
        };
        // A resource with less than a day of draw left is critical. While any
        // is critical the drones work only the most critical one — that's what
        // keeps the crew alive, and it must be decided by time left, not by
        // which tank or deposit happens to be biggest.
        //
        // Commitment with two release conditions:
        //  * *safe* — the committed ore is back at its buffer target. NOT
        //    "back above a day of draw": releasing at the critical line lets
        //    the ore bleed straight back through it at full fab draw while
        //    the ship works the other ore, so two near-critical ores make the
        //    ship flip-flop at the shuttle rate forever, every buffer pinned
        //    to its critical line and nothing ever built up. Mining to the
        //    buffer target is what "finish it" means — the bout ends when
        //    the ore is actually stocked, not when it is merely alive.
        //  * *superseded* — another ore is four times more critical. Holding
        //    a 7 h ore while a 0 h ore starves the fab is the wrong triage,
        //    so the commitment yields to a much worse ore. The wide 4x band
        //    (not 2x) is what keeps the switches hours apart instead of
        //    minutes — a 2x band ping-ponged at the shuttle rate.
        // A committed ore that is no longer mineable here is dropped, or it
        // would lock the drones forever. The commitment resets on arrival at a
        // new system, so a stale ore never carries across a jump.
        //
        // Crucially the drones can only *mine*, not buy. A critical ore that
        // exists only at a market cannot be worked: committing to it would
        // score every mineable world at zero and ground the ship at the pad
        // while it drinks its water. Market ores are the trade logic's job;
        // the mining commitment only ever targets a real deposit.
        const SUPERSIDE: f64 = 0.25;
        let mineable_here = |r: Res| raw_mineable.contains(&r);
        let pick_most_critical = || -> Option<Res> {
            ore_res.iter()
                .copied()
                .filter(|r| ore_autonomy_h(*r) <= CRITICAL_HOURS && mineable_here(*r))
                .min_by(|a, b| ore_autonomy_h(*a).total_cmp(&ore_autonomy_h(*b)))
        };
        let most_critical: Option<Res> = match self.committed_ore {
            Some(c) => {
                let a_c = ore_autonomy_h(c);
                let safe = self.inv.fraction(c) >= buffer_target(c) && a_c > CRITICAL_HOURS;
                let superseded = ore_res
                    .iter()
                    .copied()
                    .filter(|r| *r != c && mineable_here(*r))
                    .any(|r| ore_autonomy_h(r) < a_c * SUPERSIDE);
                if safe || superseded || !mineable_here(c) {
                    let mc = pick_most_critical();
                    self.committed_ore = mc;
                    mc
                } else {
                    Some(c)
                }
            }
            None => {
                let mc = pick_most_critical();
                self.committed_ore = mc;
                mc
            }
        };
        // Score every mineable world. A round trip to a world costs transit
        // time in which the fab keeps drawing, so the score is weighted by
        // the duty cycle: a close-in world beats a rich-but-far one.
        let planet_score: Vec<f64> = sites
            .iter()
            .enumerate()
            .map(|(si, (l, p))| {
                if !p.mineable() || p.exhausted() {
                    return 0.0;
                }
                // Hazards: air that eats the hull, a giant's radiation belts.
                let parent_rad = l.planet().and_then(|i| sys.planets.get(i)).map(|g| g.radiation()).unwrap_or(0.0);
                let hazard = if p.corrosion() > 0.0 || p.radiation() > 0.0 || parent_rad > 0.0 { 0.3 } else { 1.0 };
                let duty = mining_duty(0.25 + site_au[si] * spec.move_time_per_au) * hazard;
                p.deposits
                    .iter()
                    .filter(|d| d.tons >= 1000.0)
                    .map(|d| -> f64 {
                        let want = match most_critical {
                            Some(mc) => {
                                if d.res == mc {
                                    (buffer_target(d.res) - self.inv.fraction(d.res))
                                        .max(0.0)
                                        * self.inv.cap(d.res)
                                } else {
                                    0.0 // something is critical: work only that ore
                                }
                            }
                            None if in_buffer_mode => {
                                (buffer_target(d.res) - self.inv.fraction(d.res))
                                    .max(0.0)
                                    * self.inv.cap(d.res)
                            }
                            None => self.raw_base(d.res),
                        };
                        want * d.tons.ln().max(1.0)
                    })
                    .sum::<f64>()
                    * duty
            })
            .collect();
        let best_planet: Option<usize> = planet_score
            .iter()
            .enumerate()
            .filter(|(_, s)| **s > 0.5)
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i);
        let best_score = best_planet.map(|i| planet_score[i]).unwrap_or(0.0);

        // ---- movement decisions ----
        // H2 level at which the ship is safe to send on a mining sortie from
        // the star. The return trigger (fuel_crit, 0.7) sits below it: that
        // hysteresis band is what keeps the ship from ping-ponging.
        let out_gate = if settled { 0.8 } else { 0.75 };
        if at_star {
            self.mining_on = false;
            if fuel_crit {
                // Hunt for fuel: a giant if there is one; otherwise no source
                // beats the star itself, so just bank the wind here.
                if let Some(pi) = fuel_planet {
                    let _ = self.begin_shuttle(gal, site_locs[pi]);
                }
            } else if need_mine && (h2f > out_gate || (most_critical.is_some() && h2f > 0.45)) {
                // A sortie is always worth taking: a ship that is not ready to
                // jump gains its departure buffers from the very mining the
                // sortie does, and a ready ship launches from wherever it
                // happens to be standing still. With an ore critical, the
                // sortie can't wait for a comfortable fuel margin.
                if let Some(pi) = best_planet {
                    let _ = self.begin_shuttle(gal, site_locs[pi]);
                }
            } else if let Some(fp) = fuel_planet {
                // A dim star's wind barely outpaces the reactor: waiting here
                // for the fuel gate is how a ship once stood at a red dwarf
                // until the crew died of thirst. Fill up at the giant instead.
                let _ = self.begin_shuttle(gal, site_locs[fp]);
            }
        } else if let Some(pi) = planet_idx {
            let p = sites[pi].1;
            let is_fuel = p.scoopable();
            // Hysteresis: the ship keeps working its current world while it
            // is within 1.5x of the best world's score. Without the band,
            // two similar worlds (e.g. two ice giants) would score in and out
            // of the top spot as tanks tick, and the ship would shuttle
            // between them forever without mining.
            let cur_score = planet_score.get(pi).copied().unwrap_or(0.0);
            let useful_mine = p.mineable()
                && !p.exhausted()
                && (best_planet.is_some_and(|i| i == pi) || cur_score * 1.5 >= best_score);
            if fuel_crit {
                // Tank low: get to fuel.
                self.mining_on = false;
                if is_fuel {
                    // Scoop this atmosphere. If we are at an ice giant and the
                    // system also has a gas giant, refuel there instead — its
                    // wind is several times richer.
                    if let Some(fp) = fuel_planet
                        .filter(|fp| *fp != pi)
                        && sites[fp].1.class == super::planet::PlanetClass::GasGiant
                            && p.class == super::planet::PlanetClass::IceGiant
                        {
                            let _ = self.begin_shuttle(gal, site_locs[fp]);
                        }
                } else {
                    // No atmosphere here: to the fuel source, or back to the
                    // star to bank whatever wind there is.
                    match fuel_planet {
                        Some(fp) => {
                            let _ = self.begin_shuttle(gal, site_locs[fp]);
                        }
                        None => {
                            let _ = self.begin_shuttle(gal, Loc::Star);
                        }
                    }
                }
            } else if is_fuel {
                // A refueling stop: mine if this is also our mine; leave once
                // the tank is topped up and the mining is done. If a better
                // mine exists elsewhere (e.g. the carbon that keeps the food
                // fab alive), go work it rather than falling back to the star.
                self.mining_on = useful_mine && need_mine;
                if h2f > 0.85 && (!useful_mine || !need_mine) {
                    match best_planet {
                        Some(bp) if bp != pi => {
                            let _ = self.begin_shuttle(gal, site_locs[bp]);
                        }
                        _ => {
                            let _ = self.begin_shuttle(gal, Loc::Star);
                        }
                    }
                }
            } else {
                // An ore stop: work it while it is worthwhile.
                self.mining_on = useful_mine && need_mine;
                if !useful_mine || !need_mine {
                    let _ = self.begin_shuttle(gal, Loc::Star);
                }
            }
        }

        // ---- fabrication ----
        self.focus = if self.hull < hmax * 0.6 || self.ftl_wear > 50.0 {
            Focus::Repair
        } else if self.inv.fraction(Res::Oxygen) < 0.6
            || self.inv.fraction(Res::Water) < 0.6
            || self.inv.fraction(Res::Food) < 0.6
        {
            Focus::LifeSupport
        } else if self.inv.fraction(Res::Med) < 0.3 {
            Focus::Medical
        } else {
            Focus::Stockpile
        };
        self.repair_on = self.hull < hmax * 0.98 || self.ftl_wear > 5.0;

        // ---- trade ----
        // The local market, if any. (Inputs were captured up front so that
        // `sys` is dead by the time the calls below borrow `gal` mutably.)
        if let Some(tier) = civ_tier
            && tier >= CivTier::T2 {
                let sellable = |r: Res| tier >= CivTier::T3 || t2_stock.contains(&r);
                // Fab feedstock first (T3 is the only tier that stocks the
                // raws). A kilo of ice, carbon and nitrogen on board makes a
                // kilo of water, oxygen and food the fab turns out for cents
                // (0.5–1.2 credits of ore per kilo of vitals), while buying
                // those processed goods costs 3–12 credits the kilo; medicine
                // is fabbed from metal, carbon and water. A colony that buys
                // oxygen while selling carbon pays a 10x markup on the meal
                // while pawnshoping the recipe. Skip what the local drones
                // already mine — the free deposit beats the market price.
                if tier >= CivTier::T3 {
                    for &r in [Res::Ice, Res::Carbon, Res::Nitrogen].iter() {
                        if self.credits <= 500.0 || raw_mineable.contains(&r) {
                            continue;
                        }
                        let fr = self.inv.fraction(r);
                        if fr < 0.9 {
                            self.trade(gal, r, true, ((0.95 - fr) * self.inv.cap(r)).max(0.0));
                        }
                    }
                }
                // Processed vitals are an emergency valve, not a grocery
                // list: only when a vital tank is genuinely low does buying
                // beat fabbing, and then only a measured respite — drinking a
                // planetary market (60 t) dry is how the ark used to die.
                for &r in [Res::Oxygen, Res::Water, Res::Food].iter() {
                    if self.credits <= 1_000.0 || !sellable(r) {
                        continue;
                    }
                    let fr = self.inv.fraction(r);
                    if fr < 0.5 {
                        self.trade(gal, r, true, ((0.85 - fr) * self.inv.cap(r)).max(0.0));
                    }
                }
                // Comfort goods only with money to spare: medicine the fab
                // cannot cover, and hull material.
                if self.credits > 8_000.0 {
                    for &r in [Res::Med, Res::Mat].iter() {
                        if !sellable(r) {
                            continue;
                        }
                        let fr = self.inv.fraction(r);
                        if fr < 0.8 {
                            self.trade(gal, r, true, ((0.98 - fr) * self.inv.cap(r)).max(0.0));
                        }
                    }
                }
                // Stay liquid: sell the bulk ores the fab barely touches
                // (metal, silicate) — never the feedstock that feeds the
                // crew, which would trade the recipe for the meal at a
                // markup.
                if tier >= CivTier::T3 && self.credits < 2_000.0 {
                    for &r in [Res::Metal, Res::Silicate].iter() {
                        let fr = self.inv.fraction(r);
                        if fr > 0.5 {
                            self.trade(gal, r, false, (fr - 0.5) * self.inv.cap(r));
                        }
                    }
                }
            }

        // ---- interstellar travel ----
        let w = self.inv.fraction(Res::Water);
        let f = self.inv.fraction(Res::Food);
        let o2 = self.inv.fraction(Res::Oxygen);
        let people = self.people().max(1.0);
        // How far is the next hop? (Position is a pure function of the ID.)
        let my_pos = gal.system_ref(self.sys_id).pos;
        let dest_dist = match self.destination {
            Some(d) if d != self.sys_id => {
                let p = Galaxy::pos_of(d);
                let dx = p[0] - my_pos[0];
                let dy = p[1] - my_pos[1];
                (dx * dx + dy * dy).sqrt().max(0.5)
            }
            _ => 10.0, // a typical neighbor hop, for margin estimates
        };
        // Life-support margin for the hop: precharge + flight + 24 h to
        // re-establish at the destination, with a 1.5x safety factor. A
        // waypoint can never drain a ship below its return margin, so no hop
        // can trap it.
        let prep_h = dest_dist * spec.precharge_mwh_ly * (1.0 + self.ftl_wear / 100.0)
            / spec.precharge_mw.max(1e-9);
        let trip_h = dest_dist / (spec.ftl_speed * ShipSpec::throttle_factor(self.ftl_throttle).0);
        let need_h = (prep_h + trip_h + 24.0) * 1.5;
        let gate = |r: Res, per_p_h: f64| {
            (need_h * people * per_p_h / self.inv.cap(r).max(1e-9)).clamp(0.3, 0.9)
        };
        let w_gate = gate(Res::Water, WATER_PER_P_H);
        let o2_gate = gate(Res::Oxygen, O2_PER_P_H);
        let f_gate = gate(Res::Food, FOOD_PER_P_H);
        // Can the ship survive a hop of `d` light-years on the water it has
        // now? (Coefficients hoisted so the closure holds no borrow of
        // `self` — it is used across the destination assignment below.) A
        // destination the ship cannot outlast is a death sentence in transit,
        // so the ship must not commit to it: it stays, mines its water back
        // up, and only jumps once the crossing is liveable.
        let wi_now = self.inv.get(Res::Water) + self.inv.get(Res::Ice);
        let draw_ice = self.raw_base(Res::Ice);
        let cap_wi = self.inv.cap(Res::Water) + self.inv.cap(Res::Ice);
        let prep_per_ly = spec.precharge_mwh_ly * (1.0 + self.ftl_wear / 100.0)
            / spec.precharge_mw.max(1e-9);
        let trip_per_ly = 1.0 / (spec.ftl_speed * ShipSpec::throttle_factor(self.ftl_throttle).0).max(1e-9);
        let reachable = |d: f64| {
            wi_now >= draw_ice * ((prep_per_ly + trip_per_ly) * d + 6.0) + 0.08 * cap_wi
        };
        // Water-return mode (hysteresis, not a threshold): a healthy base
        // holds water+ice at a fixed equilibrium (the fab refills water
        // toward 95%, the drones buffer ice toward 50%), so a single
        // threshold near that level would flap the base between "explore"
        // and "return" every few hours. Enter return mode well below the
        // equilibrium; leave only once the water is back above it.
        //
        // And low water only sends the ship home from ground it cannot make
        // good by staying: at a system whose ice the drones can out-mine the
        // fab, the cure for low water is the very mining a jump would
        // interrupt. (The update sits above the closures below, which hold
        // a borrow of `self` and would otherwise stretch across the
        // assignment.)
        if self.water_return {
            if wi_now > 0.72 * cap_wi {
                self.water_return = false;
            }
        } else if wi_now < 0.60 * cap_wi && !ice_sustainable {
            self.water_return = true;
        }
        let returning = self.water_return;
        // Is a hop of this distance survivable at all? The nanobot fab keeps
        // running in flight (power permitting), so the vital tanks buffer it
        // and are not a hard range limit. The real limits are H2 (never made in
        // flight) and the raw feedstock (ice, carbon, nitrogen) the fab needs
        // over the "isolation window": the flight plus the first 24 h at the
        // destination, before we can scoop or mine again. Crucially, the hop
        // must still *leave* a safety buffer of each raw in the tanks — a jump
        // that arrives with the feedstock at zero strands the crew at a system
        // they cannot yet replenish. Ice converts 1:1 to water, so the water
        // need is drawn from the combined water + ice stock.
        let feasible = |dist: f64| {
            let trip = dist / (spec.ftl_speed * ShipSpec::throttle_factor(self.ftl_throttle).0).max(1e-9);
            let prep = dist * spec.precharge_mwh_ly * (1.0 + self.ftl_wear / 100.0)
                / spec.precharge_mw.max(1e-9);
            // The fab runs in flight, drawing on the raw tanks. The "isolation
            // window" is the flight plus the short time to reach a mine at the
            // destination and resume replenishing — not 24 h, because the vital
            // tanks buffer the fab and the fab only needs raws until mining
            // resumes. (A 24 h window made every hop drain a quarter of the
            // feedstock, so a ship could never keep its buffers across hops.)
            let isolation_h = trip + 6.0;
            let post = 0.08; // keep at least 8% of each raw tank after the hop
            let ok_water = self.inv.get(Res::Water) + self.inv.get(Res::Ice)
                - self.raw_base(Res::Ice) * isolation_h
                >= post * (self.inv.cap(Res::Water) + self.inv.cap(Res::Ice));
            let ok_carbon = self.inv.get(Res::Carbon) - self.raw_base(Res::Carbon) * isolation_h
                >= post * self.inv.cap(Res::Carbon);
            let ok_nitrogen = self.inv.get(Res::Nitrogen) - self.raw_base(Res::Nitrogen) * isolation_h
                >= post * self.inv.cap(Res::Nitrogen);
            // H2: the reactor over precharge + flight, plus the FTL burn over flight.
            let h2_need = (spec.reactor_burn * (prep + trip)
                + spec.ftl_burn * trip * (1.0 + self.ftl_wear / 100.0))
                * 1.2;
            let ok_h2 = self.inv.get(Res::H2) >= h2_need;
            // Start with a vital buffer so a mid-jump fault is not instant death.
            let ok_buffers = self.inv.fraction(Res::Water) >= 0.2
                && self.inv.fraction(Res::Oxygen) >= 0.2
                && self.inv.fraction(Res::Food) >= 0.2;
            ok_water && ok_carbon && ok_nitrogen && ok_h2 && ok_buffers
        };
        let dest_dist_ok = feasible(dest_dist);
        // The floor no gate may waive: the ship must be able to *survive the
        // hop itself* — water (+ice) through precharge + flight + re-establish,
        // and H2 for the crossing. `dest_dist_ok` is the full check (raws at 8
        // %, vitals at 20 %); that may be waived for a stranded ship escaping,
        // but the hop must still be liveable or the jump is a one-way trip.
        let hop_ok = self.hop_liveable(spec, dest_dist);

        // A standing destination the ship can no longer outlast is dropped:
        // precharging for a crossing the crew will not survive only drains the
        // water that would keep them alive while the ship stands and mines.
        let dest_unreachable = self.destination
            .map(|d| d != self.sys_id && !reachable(dest_dist))
            .unwrap_or(false);
        if (self.destination.is_none() || dest_unreachable) && (fuel_sustain || h2f > 0.4) {
            let nbs = gal.neighbors(my_pos, 25.0, self.sys_id);
            let fresh = |n: &Neighbor| !self.visited.contains(&n.id);
            // A ship can afford a dead-end hop only if its life-support
            // autonomy is far beyond the hop's cost (true of small ships,
            // never of the ark).
            let autonomy_h = [
                (Res::Water, WATER_PER_P_H),
                (Res::Oxygen, O2_PER_P_H),
                (Res::Food, FOOD_PER_P_H),
            ]
            .iter()
            .map(|&(r, per)| self.inv.get(r) / (people * per).max(1e-9))
            .fold(f64::INFINITY, f64::min);
            let can_risk = autonomy_h > need_h * 4.0;
            let pick: Option<u64> = if !settled {
                // A starved ship wants the nearest system it can live on —
                // even one it has mined before. Look in widening rings: a base
                // 50 or 100 ly out is worth the hop, and *not* hopping is worse
                // than the old 25-ly-only search, which fell through to a random
                // neighbor and made a water-starved ship ping-pong between two
                // dead-end systems until the crew suffocated.
                //
                // And what is it actually running out of? Of the survival raws
                // (ice → water, carbon + nitrogen → food), the one with the
                // least autonomy that cannot be obtained here (no local mine,
                // no local market) is what the voyage is for: a colony with
                // full water tanks and an empty food fab must go find carbon,
                // not another lake of ice. (Inline, not the movement block's
                // `ore_autonomy_h` closure — that borrow of `self` is over by
                // now and may not stretch across the trade calls.)
                const STARVE_HOURS: f64 = 240.0;
                let local_market = civ_tier.is_some_and(|t| t >= CivTier::T2);
                let need_ore = [Res::Ice, Res::Carbon, Res::Nitrogen]
                    .iter()
                    .copied()
                    .filter(|r| !raw_mineable.contains(r) && !local_market)
                    .filter(|r| {
                        let kg = if *r == Res::Ice {
                            self.inv.get(Res::Water) + self.inv.get(Res::Ice)
                        } else {
                            self.inv.get(*r)
                        };
                        kg / self.raw_base(*r).max(1e-9) <= STARVE_HOURS
                    })
                    .min_by(|a, b| {
                        let hours = |r: Res| {
                            let kg = if r == Res::Ice {
                                self.inv.get(Res::Water) + self.inv.get(Res::Ice)
                            } else {
                                self.inv.get(r)
                            };
                            kg / self.raw_base(r).max(1e-9)
                        };
                        hours(*a).total_cmp(&hours(*b))
                    });
                let mut pick: Option<u64> = None;
                for &radius in [25.0, 50.0, 100.0].iter() {
                    let ring = gal.neighbors(my_pos, radius, self.sys_id);
                    // 1) A full base is best: settle and resupply for good.
                    //    Only a base the ship can *reach* — a beautiful colony
                    //    80 ly out is a painting, not a destination.
                    let settleable: Vec<&Neighbor> = ring
                        .iter()
                        .filter(|n| system_can_settle(gal, n.id, spec) && reachable(n.dist_ly))
                        .collect();
                    let fresh_settleable: Vec<&Neighbor> = settleable
                        .iter()
                        .copied()
                        .filter(|n| fresh(n))
                        .collect();
                    if let Some(p) = nearest_by_dist(&fresh_settleable)
                        .or_else(|| nearest_by_dist(&settleable))
                    {
                        pick = Some(p.id);
                        break;
                    }
                    // 2) Next, an interstellar (T3) market: full resupply for
                    //    whatever is missing, and the only one-stop answer in
                    //    a region whose ores are split across systems.
                    let with_t3: Vec<&Neighbor> = ring
                        .iter()
                        .filter(|n| n.civ == Some(CivTier::T3) && reachable(n.dist_ly))
                        .collect();
                    let fresh_t3: Vec<&Neighbor> = with_t3
                        .iter()
                        .copied()
                        .filter(|n| fresh(n))
                        .collect();
                    if let Some(p) = nearest_by_dist(&fresh_t3)
                        .or_else(|| nearest_by_dist(&with_t3))
                    {
                        pick = Some(p.id);
                        break;
                    }
                    // 3) Failing that, head for what is actually running out:
                    //    a mineable deposit of the critical raw is best (a
                    //    source the drones can keep working), a market selling
                    //    it is the respite.
                    if let Some(r) = need_ore {
                        let with_ore: Vec<&Neighbor> = ring
                            .iter()
                            .filter(|n| system_has_ore(gal, n.id, r, max_au) && reachable(n.dist_ly))
                            .collect();
                        let mineable_ore: Vec<&Neighbor> = with_ore
                            .iter()
                            .copied()
                            .filter(|n| system_mineable_ore(gal, n.id, r, max_au))
                            .collect();
                        let fresh_mineable: Vec<&Neighbor> = mineable_ore
                            .iter()
                            .copied()
                            .filter(|n| fresh(n))
                            .collect();
                        let fresh_ore: Vec<&Neighbor> = with_ore
                            .iter()
                            .copied()
                            .filter(|n| fresh(n))
                            .collect();
                        if let Some(p) = nearest_by_dist(&fresh_mineable)
                            .or_else(|| nearest_by_dist(&mineable_ore))
                            .or_else(|| nearest_by_dist(&fresh_ore))
                            .or_else(|| nearest_by_dist(&with_ore))
                        {
                            pick = Some(p.id);
                            break;
                        }
                    }
                    // 4) Failing that — and only when the ship is not chasing
                    //    a specific ore, which tier 3 already aimed at water
                    //    in that case — head for the nearest *water*: the
                    //    colony is too big to base most systems, so a full
                    //    base may not exist in the region, but a water source
                    //    always does, and it is the only place the crew will
                    //    outlast the crossing. Jumping at a rich-but-dry
                    //    neighbor instead is how the ark used to die: it
                    //    landed in a system with no ice and drank itself to
                    //    zero before it could move on.
                    if need_ore.is_none() {
                        let with_water: Vec<&Neighbor> = ring
                            .iter()
                            .filter(|n| system_has_water(gal, n.id, max_au) && reachable(n.dist_ly))
                            .collect();
                        let fresh_water: Vec<&Neighbor> = with_water
                            .iter()
                            .copied()
                            .filter(|n| fresh(n))
                            .collect();
                        if let Some(p) = nearest_by_dist(&fresh_water)
                            .or_else(|| nearest_by_dist(&with_water))
                        {
                            pick = Some(p.id);
                            break;
                        }
                    }
                }
                // No base, market, ore or water source in reach. A ship
                // starving for an ore the region cannot provide takes every
                // fresh system as a step toward it — the hop is water-safe
                // (reachable) and staying put is a countdown. Otherwise the
                // last resorts demand a richer gamble.
                let starved = need_ore.is_some();
                pick.or_else(|| {
                    nbs.iter()
                        .filter(|n| fresh(n) && reachable(n.dist_ly))
                        .filter(|n| can_risk || starved || n.civ.is_some() || n.richness > 30.0)
                        .map(|n| n.id)
                        .next()
                })
            } else {
                // A base expands, it does not wander: only a *fresh* base (or
                // a fresh market) earns the hop, and only at a distance the
                // life support can actually cross. Look in widening rings;
                // once the region is explored the base stays and mines — the
                // deposits outlast the crew by decades.
                //
                // Exception: a base whose water is genuinely running out
                // stops exploring and goes *back* for it — to the nearest
                // settleable system or resupply station, visited or not.
                // Marching only at fresh ground is how the ark walked off
                // the good close-in ice and drowned in a belt of distant
                // giants it could not mine fast enough.
                let mut pick: Option<u64> = None;
                for &radius in [25.0, 50.0, 100.0].iter() {
                    let ring = gal.neighbors(my_pos, radius, self.sys_id);
                    for n in ring.iter() {
                        let ok = if returning {
                            // Water-safe distance only — the ship is not
                            // gambling with a reserve it is already
                            // spending — and the destination must sustain
                            // water: a base the drones can work, a station
                            // that sells ice by the million tons, or at
                            // least ice the drones can out-mine the fab.
                            reachable(n.dist_ly)
                                && (system_can_settle(gal, n.id, spec)
                                    || n.civ == Some(CivTier::T3)
                                    || system_works_ice(gal.system_ref(n.id), spec))
                        } else {
                            fresh(n)
                                && system_can_settle(gal, n.id, spec)
                                && feasible(n.dist_ly)
                        };
                        if ok {
                            pick = Some(n.id); // ring is nearest-first
                            break;
                        }
                    }
                    if pick.is_some() {
                        break;
                    }
                }
                // The expansion gambles (rich fresh ground, fresh T3 stations)
                // stand down while the water comes home: only the water
                // itself decides now.
                if !returning {
                    pick = pick.or_else(|| {
                        can_risk
                            .then(|| {
                                nbs.iter()
                                    .find(|n| fresh(n) && n.richness > 30.0)
                                    .map(|n| n.id)
                            })
                            .flatten()
                    })
                    // A base does not wander to a planetary (T2) market —
                    // its 60 t of water is a waypoint, not a destination.
                    // Only an interstellar (T3) resupply station earns the
                    // hop.
                    .or_else(|| {
                        nbs.iter()
                            .find(|n| fresh(n) && n.civ == Some(CivTier::T3) && reachable(n.dist_ly))
                            .map(|n| n.id)
                    });
                }
                pick
            };
            // Commit to the pick, or drop the (now unreachable) standing
            // destination: with none, the ship stops precharging and mines its
            // water back up instead of standing for a crossing it can't make.
            self.retarget(pick);
        }
        // H2 margin: a jump burns only a few percent of the tank; the real
        // reserve is for weeks of scooping failure. (0.85 used to be a comfort
        // level that small ships' mining sorties could never re-reach, locking
        // them in forever.)
        let fuel_gate = if settled { 0.7 } else { 0.55 };
        // Raw-material readiness: starvation floors, not comfort targets — a
        // one-drone ship cannot refill five tanks at once, and the buffers
        // above refill toward 40–50% anyway. Don't abandon a base with the
        // fab's feed running out *and* the missing ore sitting right here;
        // ore this system cannot provide is no reason to wait.
        let raw_ok = [
            (Res::Ice, 0.10),
            (Res::Metal, 0.12),
            (Res::Silicate, 0.12),
            (Res::Carbon, 0.08),
            (Res::Nitrogen, 0.08),
        ]
        .iter()
        .all(|&(r, min)| {
            // Ice is judged on the combined water stock, not the ice tank
            // alone. The water fab turns board ice into water as fast as the
            // drones mine it, so in steady state the ice tank sits near empty
            // while the *water* tank holds the reserve. In flight the ship
            // cannot mine, so the water it actually has is water + ice; gating
            // on the ice tank alone would ground the ship forever at a tight
            // base that is otherwise perfectly sustainable.
            let frac = if r == Res::Ice {
                (self.inv.get(Res::Water) + self.inv.get(Res::Ice)) / self.inv.cap(Res::Ice)
            } else {
                self.inv.fraction(r)
            };
            frac > min || !raw_mineable.contains(&r)
        });
        // ---- escape clause ----
        // The vital gates assume the ship can refill what it spends. When
        // that is false — no ice (water/O2) or no carbon *or* nitrogen
        // (food), and no market, in a system that is not otherwise a base —
        // no gate level is ever reachable, and waiting is a death sentence:
        // the ship leaves with what it has. The fab keeps running in flight
        // off board raws, and a starved ship's destination is the nearest
        // settleable system, which has exactly what is missing. Likewise a
        // ship whose vitals, at a pessimistic flight draw, cannot outlast
        // the hop cannot afford to wait for any gate.
        let obtainable = |r: Res| raw_mineable.contains(&r) || t2_stock.contains(&r);
        let chain_broken = !settled
            && (!obtainable(Res::Ice) || !obtainable(Res::Carbon) || !obtainable(Res::Nitrogen));
        let vital_autonomy_h = [
            (Res::Water, WATER_PER_P_H),
            (Res::Oxygen, O2_PER_P_H),
            (Res::Food, FOOD_PER_P_H),
        ]
        .iter()
        .map(|&(r, per)| self.inv.get(r) / (people * per * 1.23).max(1e-9))
        .fold(f64::INFINITY, f64::min);
        let hop_need_h = prep_h + trip_h + 6.0;
        let escape = chain_broken || vital_autonomy_h < hop_need_h;
        let ready = h2f > fuel_gate
            && hop_ok
            && (escape || (w > w_gate && o2 > o2_gate && f > f_gate))
            && (dest_dist_ok || !settled)
            && (raw_ok || !settled)
            && self.hull > hmax * 0.7
            && self.shuttle.is_none()
            && self.trip.is_none();
        if ready
            && self.ftl_charge >= 100.0
            && self.destination.is_some()
            && self.destination != Some(self.sys_id)
        {
            self.start_trip(gal);
        }
    }
}


#[cfg(test)]
mod balance_tests {
    use super::*;
    use crate::sim::galaxy::Galaxy;

    /// Every ship must survive 500 h of autopilot on seed 42 without losing
    /// anyone. This guards the balance work: a "fixed" autopilot that only
    /// survives by ignoring the crew is not a fix.
    #[test]
    fn all_ships_survive_500h_seed42() {
        for ship in 0..3usize {
            let mut gal = Galaxy::new(42);
            let mut sim = Sim::new_game(42, ship, &mut gal);
            let dt = 0.25f64;
            for _ in 0..2000u64 { // 500 h at 0.25 h/step
                sim.autopilot(&mut gal);
                sim.advance(dt, &mut gal);
                if let Some(over) = &sim.over {
                    panic!("ship {} died at {:.1} h: {}", ship, sim.t_h(), over.cause);
                }
            }
            assert_eq!(
                sim.stats.crew_lost, 0,
                "ship {} lost crew at {:.1} h",
                ship,
                sim.t_h()
            );
            assert_eq!(
                sim.stats.pax_lost, 0,
                "ship {} lost passengers at {:.1} h",
                ship,
                sim.t_h()
            );
            // A ship that "survives" by never leaving home is not exploring.
            assert!(
                sim.stats.jumps >= 1,
                "ship {} made no jumps in 500 h",
                ship
            );
        }
    }

}

#[cfg(test)]
mod shuttle_tests {
    use super::{Loc, Sim, ShuttleBlock, Trip};
    use crate::sim::galaxy::Galaxy;

    /// A refused shuttle leg must always come with a reason — this is the
    /// contract the UI's "why can't I?" messages are built on. A silent
    /// refusal here is how the player is left pressing S at a wall.
    #[test]
    fn blocked_shuttles_report_a_reason() {
        let mut gal = Galaxy::new(42);
        let mut sim = Sim::new_game(42, 0, &mut gal);
        sim.shuttle = None;
        sim.trip = None;

        // Already docked where it was asked to go.
        sim.loc = Loc::Star;
        assert_eq!(
            sim.begin_shuttle(&mut gal, Loc::Star).err(),
            Some(ShuttleBlock::AlreadyThere)
        );

        // Mid-leg: a started shuttle blocks the next one.
        assert!(sim.begin_shuttle(&mut gal, Loc::Planet(0)).is_ok());
        assert_eq!(
            sim.begin_shuttle(&mut gal, Loc::Star).err(),
            Some(ShuttleBlock::Shuttling)
        );

        // In FTL: nothing moves until the trip is over.
        sim.shuttle = None;
        sim.trip = Some(Trip {
            dest_id: 1,
            dest_pos: [1.0, 0.0],
            dest_name: "Test".into(),
            dist_ly: 10.0,
            total_h: 10.0,
            remaining_h: 5.0,
            dust_rate: 0.0,
        });
        assert_eq!(
            sim.begin_shuttle(&mut gal, Loc::Star).err(),
            Some(ShuttleBlock::Tripping)
        );
    }
}

#[cfg(test)]
mod fix_tests {
    use super::*;
    use crate::sim::galaxy::Galaxy;

    /// Micrometeorites used `exp(-ln λ)` = 1/λ as the Knuth threshold, so
    /// for λ < 1 the count was always zero. The mean must track λ.
    #[test]
    fn poisson_mean_tracks_lambda() {
        for &lam in &[0.01f64, 0.3, 2.0] {
            let mut rng = derive(1, 2);
            let n = 200_000;
            let sum: u64 = (0..n).map(|_| poisson(&mut rng, lam) as u64).sum();
            let mean = sum as f64 / n as f64;
            assert!((mean - lam).abs() < lam * 0.1 + 0.002, "λ={lam} mean={mean}");
        }
    }

    /// The Risk option scales how often things break: over the same long,
    /// fixed-seed stay, Tourist sees fewer faults than Normal, Risky more.
    #[test]
    fn risk_option_scales_faults() {
        use crate::sim::{GalaxyOpts, Risk};
        let faults = |risk: Risk| {
            let mut gal = Galaxy::with_opts(42, GalaxyOpts { risk, ..GalaxyOpts::default() });
            let mut sim = Sim::new_game(42, 1, &mut gal);
            assert_eq!(sim.opts.risk, risk);
            // Early MAT repair clears a fault inside the step it spawns in.
            sim.repair_on = false;
            let mut n = 0;
            for _ in 0..3_000 {
                let had = !sim.faults.is_empty();
                sim.advance(0.1, &mut gal);
                if !had && !sim.faults.is_empty() {
                    n += 1;
                }
                if sim.over.is_some() {
                    break;
                }
            }
            n
        };
        let (t, n, r) = (faults(Risk::Tourist), faults(Risk::Normal), faults(Risk::Risky));
        assert!(t < n && n < r, "tourist {t}, normal {n}, risky {r}");
    }

    /// Repair waits for a whole percent of damage, then runs to full,
    /// spending MAT only for the damage it fixes.
    #[test]
    fn repair_waits_for_a_whole_percent_then_finishes() {
        let mut gal = Galaxy::new(42);
        let mut sim = Sim::new_game(42, 0, &mut gal);
        let hmax = sim.ship().hull;
        sim.repair_on = true;
        sim.ftl_wear = 0.0;
        let mat = |s: &Sim| s.inv.get(Res::Mat);

        // A sliver of damage: nothing is spent.
        sim.hull = hmax * 0.995;
        let before = mat(&sim);
        sim.repair_hull_and_wear(0.1, hmax);
        assert_eq!(mat(&sim), before);
        assert_eq!(sim.hull, hmax * 0.995);

        // A whole percent: repair starts and keeps going above 99%.
        sim.hull = hmax * 0.99;
        sim.repair_hull_and_wear(0.01, hmax);
        assert!(sim.repairing_hull && sim.hull > hmax * 0.99 && sim.hull < hmax);
        let before = mat(&sim);
        let missing = hmax - sim.hull;
        for _ in 0..10_000 {
            sim.repair_hull_and_wear(0.01, hmax);
        }
        assert_eq!(sim.hull, hmax);
        assert!(!sim.repairing_hull || sim.hull >= hmax);
        let spent = before - mat(&sim);
        assert!((spent - missing / 0.5 * 0.8).abs() < 1e-6, "spent {spent} for {missing} hull");
    }

    /// Per-step random rolls must not repeat just because several steps
    /// fall inside the same game second.
    #[test]
    fn rng_differs_between_substeps() {
        let mut gal = Galaxy::new(5);
        let mut sim = Sim::new_game(5, 0, &mut gal);
        let a = sim.rng_step;
        sim.advance(0.0001, &mut gal);
        sim.advance(0.0001, &mut gal);
        assert_eq!(sim.rng_step, a + 2);
    }

    /// Charge energy is conserved across a destination change: a capacitor
    /// full for the nominal 8 ly is not full for a 20 ly hop.
    #[test]
    fn retarget_rescales_charge() {
        let mut gal = Galaxy::new(42);
        let mut sim = Sim::new_game(42, 0, &mut gal);
        let pos = gal.system_ref(sim.sys_id).pos;
        let far = gal
            .neighbors(pos, 30.0, sim.sys_id)
            .into_iter()
            .find(|n| n.dist_ly > 12.0)
            .expect("a far neighbor");
        sim.ftl_charge = 100.0;
        sim.set_destination(far.id, &mut gal);
        let want = 100.0 * 8.0 / far.dist_ly;
        assert!((sim.ftl_charge - want).abs() < 1e-6, "{} vs {}", sim.ftl_charge, want);
        sim.clear_destination();
        assert!((sim.ftl_charge - 100.0).abs() < 1e-6);
    }

    /// Buying into a nearly full tank must take from the station only what
    /// fits, and charge only for that.
    #[test]
    fn trade_buy_does_not_leak_stock() {
        let mut gal = Galaxy::new(42);
        let mut sim = Sim::new_game(42, 0, &mut gal);
        let id = sim.sys_id;
        {
            let sys = gal.system(id);
            sys.civ = Some(super::super::civ::Civ { tier: CivTier::T3, name: "Test".into(), kind: CivKind::Colony, seat: Loc::Planet(0), small: false });
            sys.trade_stock.insert(Res::Water, 10_000.0);
            sys.trade_prices.insert(Res::Water, 1.0);
        }
        let room = sim.inv.cap(Res::Water) - sim.inv.get(Res::Water);
        let credits = sim.credits;
        sim.trade(&mut gal, Res::Water, true, room + 500.0);
        let stock = gal.system_ref(id).trade_stock[&Res::Water];
        assert!((stock - (10_000.0 - room)).abs() < 1e-6, "stock {stock}");
        assert!((credits - sim.credits - room).abs() < 1e-6);
        // Full tank now: a second buy changes nothing and says why.
        let trades = sim.stats.trades;
        sim.trade(&mut gal, Res::Water, true, 100.0);
        assert_eq!(sim.stats.trades, trades);
        assert!(sim.log.entries.back().unwrap().msg.contains("full"));
    }
}

#[cfg(test)]
mod base_rarity {
    use super::*;

    /// Most systems are waypoints, not bases — the core of the survival
    /// loop. Under the previous generator about 13% of systems could
    /// sustain a ship; the detailed generator must stay in the same regime
    /// (±50%).
    #[test]
    fn bases_stay_rare() {
        for ship in 0..3 {
            let spec = ShipSpec::spec(ship);
            let (mut n, mut base) = (0, 0);
            let mut gal = Galaxy::new(2);
            for cx in -18..18 {
                for cy in -18..18 {
                    for id in gal.ids_in_cell(cx, cy) {
                        n += 1;
                        if system_can_base(gal.system(id), spec) {
                            base += 1;
                        }
                    }
                }
            }
            let f = base as f64 / n as f64;
            assert!((0.065..=0.20).contains(&f), "ship {ship}: base fraction {f:.3}");
        }
    }
}
