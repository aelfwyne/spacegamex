//! The player's colony: founding, supplies, buildings, the census, and the
//! long game of growing to a million people.
//!
//! Time runs fast in colony mode (about 24 real seconds per year at 1x, so
//! a lifetime is roughly half an hour) and at ordinary ship time while the
//! captain is away buying supplies.

pub mod census;
pub mod world;

use super::cargo::{CargoMarket, Good, Offer};
use super::galaxy::Galaxy;
use super::logbuf::Level;
use super::people::{years, HOURS_PER_YEAR};
use super::planet::{Colony, ColonyTech};
use super::res::Res;
use super::state::{loc_label, Loc, Sim};
use census::{Census, Event, Origin, Person, Rates};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub use world::Hab;

/// Population that wins the game.
pub const WIN_POP: f64 = 1_000_000.0;
/// Real seconds per colony year at 1x in colony mode.
pub const SECS_PER_COLONY_YEAR: f64 = 24.0;
/// The galactic standard tech level (a well-founded colony starts here).
pub const STANDARD_TECH: f64 = 5.0;
pub const MAX_TECH: f64 = 8.0;
const MONTH: f64 = 1.0 / 12.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Batch {
    pub units: u32,
    /// Wears out at this date (game years).
    pub until: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Project {
    /// Programmable Manufacturing Plant: makes supplies from raw materials.
    Pmp,
    BioDome,
    SpaceDock,
    Business,
}

impl Project {
    pub const ALL: [Project; 4] = [Project::Pmp, Project::BioDome, Project::SpaceDock, Project::Business];

    pub fn name(self) -> &'static str {
        match self {
            Project::Pmp => "Programmable Manufacturing Plant",
            Project::BioDome => "Bio-Dome",
            Project::SpaceDock => "Space Dock",
            Project::Business => "Business Center",
        }
    }

    pub fn desc(self) -> &'static str {
        match self {
            Project::Pmp => "Builds every colony supply from raw materials, replacing worn-out kits and equipping new colonists, for up to 10,000 people. Ends the need to import supplies.",
            Project::BioDome => "A permanent habitat for 10,000 colonists on a world they cannot live on unprotected.",
            Project::SpaceDock => "Brings visiting merchants: sell the colony's materials and buy supplies without leaving. Needs 100,000 colonists.",
            Project::Business => "Improves the economy: better prices and stock at the space dock, and a higher standard of living.",
        }
    }

    /// (tonnes of materials, labour-years)
    pub fn cost(self) -> (f64, f64) {
        match self {
            Project::Pmp => (800.0, 300.0),
            Project::BioDome => (400.0, 150.0),
            Project::SpaceDock => (8_000.0, 4_000.0),
            Project::Business => (2_000.0, 800.0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Construction {
    pub kind: Project,
    /// 0..1
    pub done: f64,
}

/// What the colony's conditions were last month (for the screens).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Indicators {
    pub coverage: BTreeMap<Good, f64>,
    /// Power share that reaches everything else (0.25..1).
    pub power_eff: f64,
    /// Share of colonists sheltered (1 on habitable worlds).
    pub sheltered: f64,
    pub labour_eff: f64,
    pub genetic: f64,
    pub shortage: bool,
    pub materials_net_y: f64,
    pub workers: f64,
    pub rates_mort: f64,
    pub rates_fert: f64,
    pub rates_infant: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerColony {
    pub name: String,
    pub sys_id: u64,
    pub loc: Loc,
    pub world: String,
    /// Founding date and how far the colony has been simulated (game years).
    pub founded: f64,
    pub clock: f64,
    pub month: u32,
    pub hab: Hab,
    pub danger: f64,
    /// Mineral wealth of the world, 0..1.
    pub richness: f64,
    pub hybrid: bool,
    /// A Genetic Modification Kit was delivered (Hybrid colonies need one
    /// before humans and natives can have children).
    pub gene_kit: bool,
    pub founders: u32,
    /// Genetic penalty from a small founding population (before fading).
    pub genetic0: f64,
    pub tech: f64,
    pub health: f64,
    pub happiness: f64,
    pub wealth: f64,
    pub supplies: BTreeMap<Good, Vec<Batch>>,
    /// Raw-material stockpile (tonnes).
    pub materials: f64,
    pub pmp: u32,
    pub biodomes: u32,
    pub dock: bool,
    pub business: bool,
    pub queue: Vec<Construction>,
    pub auto_build: bool,
    pub census: Census,
    /// The captain's record.
    pub player: u32,
    /// (year, population), once a year.
    pub history: Vec<[f32; 2]>,
    pub won: bool,
    /// The space dock's merchants.
    pub market: Option<CargoMarket>,
    #[serde(default)]
    pub ind: Indicators,
}

/// Something the colony wants to tell the captain.
pub type Note = (Level, String);

impl PlayerColony {
    pub fn population(&self) -> f64 {
        self.census.population()
    }

    pub fn units(&self, g: Good) -> u32 {
        self.supplies.get(&g).map(|v| v.iter().map(|b| b.units).sum()).unwrap_or(0)
    }

    pub fn add_supply(&mut self, g: Good, n: u32, now: f64) {
        if let Some(s) = g.supply()
            && n > 0
        {
            self.supplies.entry(g).or_default().push(Batch { units: n, until: now + s.life_y });
        }
    }

    pub fn species(&self) -> &'static str {
        if self.hybrid { "Hybrid" } else { "Human" }
    }

    /// Shelter capacity (people) from domes, before power.
    pub fn shelter_capacity(&self, power_eff: f64) -> f64 {
        self.units(Good::HazardDome) as f64 * 25.0 * power_eff + self.biodomes as f64 * 10_000.0
    }

    pub fn wealth_label(&self) -> &'static str {
        label(self.wealth, ["Destitute", "Poor", "Modest", "Comfortable", "Prosperous", "Rich"])
    }

    pub fn happiness_label(&self) -> &'static str {
        label(self.happiness, ["Miserable", "Unhappy", "Restless", "Content", "Happy", "Joyful"])
    }

    pub fn health_label(&self) -> &'static str {
        label(self.health, ["Dying", "Sickly", "Poor", "Fair", "Good", "Excellent"])
    }

    /// Mean lifespan of those who died last year (years).
    pub fn lifespan(&self) -> Option<f64> {
        let y = &self.census.last_year;
        (y.deaths >= 1.0).then(|| y.death_age_sum / y.deaths)
    }

    /// How many more of a project can be started (limits per colony).
    pub fn can_build(&self, p: Project) -> Result<(), String> {
        let pop = self.population();
        let queued = self.queue.iter().filter(|c| c.kind == p).count() as u32;
        let per_10k = (pop / 10_000.0).ceil().max(1.0) as u32;
        match p {
            Project::Pmp if self.pmp + queued >= per_10k => Err(format!("One plant per 10,000 colonists: {per_10k} allowed now.")),
            Project::BioDome if !self.hab.needs_domes() => Err("This world needs no domes.".into()),
            Project::BioDome if self.biodomes + queued > per_10k => Err(format!("One Bio-Dome per 10,000 colonists (plus one ahead): {} allowed now.", per_10k + 1)),
            Project::SpaceDock if self.dock || queued > 0 => Err("The colony already has its space dock.".into()),
            Project::SpaceDock if pop < 100_000.0 => Err("A space dock needs 100,000 colonists.".into()),
            Project::Business if self.business || queued > 0 => Err("The colony already has its Business Center.".into()),
            _ => Ok(()),
        }
    }

    /// Simulate up to `now` (game years), one month at a time.
    pub fn advance(&mut self, now: f64, player_home: bool, boost: (f64, f64), notes: &mut Vec<Note>) {
        // A long absence is simulated in full, but never more than a few
        // centuries in one call (a stalled clock must not hang the game).
        let mut steps = 0;
        while self.clock + MONTH <= now && steps < 12 * 400 {
            self.clock += MONTH;
            self.month = (self.month + 1) % 12;
            self.step_month(player_home, boost, notes);
            steps += 1;
        }
    }

    fn step_month(&mut self, player_home: bool, (tech_boost, health_boost): (f64, f64), notes: &mut Vec<Note>) {
        let now = self.clock;
        let dt = MONTH;
        let pop = self.population().max(1.0);

        // ---- supplies wear out ----
        for (g, batches) in self.supplies.iter_mut() {
            let worn: u32 = batches.iter().filter(|b| b.until <= now).map(|b| b.units).sum();
            if worn > 0 {
                batches.retain(|b| b.until > now);
                notes.push((Level::Warn, format!("{worn} × {g} wore out.")));
            }
        }
        let cov = |g: Good, s: &Self| -> f64 {
            let per = g.supply().map(|x| x.per).unwrap_or(1.0);
            (s.units(g) as f64 * per / pop).min(1.0)
        };
        let power = cov(Good::PowerGen, self);
        let eff = 0.25 + 0.75 * power;
        let mut coverage = BTreeMap::new();
        coverage.insert(Good::PowerGen, power);
        for g in [Good::HydroKit, Good::Recycler, Good::ResDrones, Good::AiCluster, Good::MedKit] {
            coverage.insert(g, cov(g, self) * eff);
        }

        // ---- shelter ----
        let sheltered = if self.hab.needs_domes() { (self.shelter_capacity(eff) / pop).min(1.0) } else { 1.0 };
        let exposed = 1.0 - sheltered;
        if self.hab.needs_domes() {
            coverage.insert(Good::HazardDome, sheltered);
        }
        let short = |g: Good| 1.0 - coverage[&g];
        if self.hab == Hab::Lethal && exposed > 1e-6 {
            let n = (pop * exposed).floor();
            if n >= 1.0 {
                self.census.cull(n, now, Some(self.player));
                notes.push((Level::Critical, format!("{n:.0} colonists without shelter died. Build domes!")));
            }
        }
        let (dome_health, dome_fert) = match self.hab {
            Hab::Marginal => (0.15 * exposed, 1.0 - 0.2 * exposed),
            Hab::Hostile => (0.5 * exposed, 1.0 - 0.6 * exposed),
            _ => (0.0, 1.0),
        };

        // ---- genetic penalty: from the second generation, fading ----
        let age = now - self.founded;
        let genetic = if age > 40.0 { self.genetic0 * (-(age - 40.0) / 150.0).exp() * (2_000.0 / (2_000.0 + pop)).sqrt() } else { 0.0 };

        // ---- materials ----
        let tech_k = (self.tech / STANDARD_TECH).clamp(0.3, 1.5);
        let labour_k = 1.0 - 0.5 * (1.0 - coverage[&Good::HydroKit]);
        // Hands dig a little; drones dig a lot.
        let produce = pop * (0.15 * labour_k + 1.4 * coverage[&Good::ResDrones]) * (0.4 + 0.6 * self.richness) * tech_k;
        let upkeep = pop * 0.3 * (1.0 + 0.5 * short(Good::Recycler));
        self.materials += (produce - upkeep) * dt;
        let shortage = self.materials < 0.0;
        if shortage {
            self.materials = 0.0;
        }

        // ---- the colony's condition ----
        let captives = self.census.living.iter().filter(|&&i| self.census.people[i as usize].origin == Origin::Captive).count() as f64;
        let cs = captives / pop;
        let short_f = if shortage { 1.0 } else { 0.0 };
        let labour_eff = 1.0 - 0.5 * short(Good::HydroKit);
        // Wealth: what the world can support versus how many share it.
        let support = 20_000.0 * 10f64.powf(2.0 * self.richness) * (1.0 + 0.5 * (self.pmp > 0) as u8 as f64) * if self.business { 2.0 } else { 1.0 } * if self.dock { 1.5 } else { 1.0 };
        let w_target = (0.12 + 0.55 * support / (support + pop) + 0.08 * (self.tech - 3.0) / 2.0 + 0.12 * self.business as u8 as f64 + 0.08 * self.dock as u8 as f64
            - 0.15 * short_f)
            .clamp(0.02, 1.0);
        let h_target = (0.92 - 0.25 * short(Good::HydroKit) - 0.1 * short(Good::Recycler) - 0.15 * short(Good::MedKit) - dome_health - 0.5 * genetic
            - (self.danger - 0.002) * 5.0
            - 0.2 * cs
            + 0.02 * (self.tech - STANDARD_TECH)
            + 0.05 * self.wealth
            - 0.05 * short_f
            + health_boost)
            .clamp(0.05, 1.0);
        let hab_q = match self.hab {
            Hab::Habitable => 0.08,
            Hab::Marginal => 0.0,
            Hab::Hostile => -0.06,
            Hab::Lethal => -0.1,
        };
        let p_target = (0.55 + 0.35 * (self.wealth - 0.4) + 0.4 * (self.health - 0.8) + hab_q - 0.8 * cs - 0.15 * short_f - 0.1 * short(Good::HydroKit)
            + 0.08 * self.business as u8 as f64)
            .clamp(0.0, 1.0);
        let k = dt / 1.0; // ~1-year response
        self.wealth += (w_target - self.wealth) * k;
        self.health += (h_target - self.health) * k;
        self.happiness += (p_target - self.happiness) * k;

        // ---- tech: AI clusters keep it; without them it collapses ----
        let ai = coverage[&Good::AiCluster];
        if ai < 0.999 {
            self.tech -= 0.5 * (1.0 - ai) * dt;
        } else {
            let slow = if self.tech >= STANDARD_TECH { 0.4 } else { 1.0 };
            self.tech += 0.125 * (pop / (pop + 3_000.0)) * slow * tech_boost * dt;
        }
        self.tech = self.tech.clamp(0.5, MAX_TECH);

        // ---- people ----
        let tech_gap = (STANDARD_TECH - self.tech).max(0.0);
        let rates = Rates {
            mort_mult: ((1.0 + 2.5 * (0.9 - self.health)) * (1.0 + 0.08 * (STANDARD_TECH - self.tech))).clamp(0.6, 8.0),
            hazard: self.danger + if self.hab == Hab::Hostile { 0.02 * exposed } else { 0.0 },
            fert: 0.28 * (self.health / 0.9) * (0.6 + 0.4 * self.happiness) * (1.0 - 0.4 * short(Good::HydroKit)) * dome_fert * (1.15 - 0.3 * self.wealth),
            infant: (0.004 + 0.15 * (1.0 - self.health) + 0.01 * tech_gap + 0.1 * genetic).clamp(0.002, 0.4),
            maternal: (0.000_3 + 0.02 * (1.0 - self.health) + 0.002 * tech_gap).clamp(0.000_1, 0.05),
            hybrid: self.hybrid && self.gene_kit,
        };
        let mut events = Vec::new();
        self.census.step(now, dt, &rates, Some(self.player), player_home, &mut events);
        for e in events {
            match e {
                Event::Pregnant(m, f) => {
                    let (mn, fname) = (self.census.name(m), self.census.name(f));
                    let who = if m == self.player { "You are".to_string() } else if f == self.player { format!("Your partner {mn} is") } else { format!("{mn} and {fname} are") };
                    notes.push((Level::Good, format!("{who} expecting a child.")));
                }
                Event::Born(id) => {
                    let p = &self.census.people[id as usize];
                    if p.mother == Some(self.player) || p.father == Some(self.player) {
                        let what = if p.female { "daughter" } else { "son" };
                        notes.push((Level::Good, format!("Your {what} {} was born.", self.census.name(id))));
                    }
                }
                Event::Died(id) => {
                    let p = &self.census.people[id as usize];
                    let me = &self.census.people[self.player as usize];
                    let kin = me.spouse == Some(id) || p.mother == Some(self.player) || p.father == Some(self.player);
                    if kin {
                        notes.push((Level::Bad, format!("{} of your family died at {:.0}.", self.census.name(id), p.age(now).floor())));
                    }
                }
            }
        }

        // ---- construction ----
        let workers = self.census.workers(now);
        let labour = workers * 0.25 * labour_eff * eff * dt;
        if let Some(c) = self.queue.first_mut() {
            let (mat_t, lab_y) = c.kind.cost();
            let mut frac = labour / lab_y;
            let need = frac * mat_t;
            if need > self.materials {
                frac *= self.materials / need.max(1e-9);
            }
            frac = frac.min(1.0 - c.done);
            self.materials -= frac * mat_t;
            c.done += frac;
            if c.done >= 1.0 - 1e-9 {
                let kind = c.kind;
                self.queue.remove(0);
                match kind {
                    Project::Pmp => self.pmp += 1,
                    Project::BioDome => self.biodomes += 1,
                    Project::SpaceDock => self.dock = true,
                    Project::Business => self.business = true,
                }
                notes.push((Level::Good, format!("Construction complete: {}.", kind.name())));
                if self.dock {
                    self.refresh_market();
                }
            }
        }

        self.ind = Indicators {
            coverage,
            power_eff: eff,
            sheltered,
            labour_eff,
            genetic,
            shortage,
            materials_net_y: produce - upkeep,
            workers,
            rates_mort: rates.mort_mult,
            rates_fert: rates.fert,
            rates_infant: rates.infant,
        };

        if self.month == 0 {
            self.yearly(now, notes);
        }
    }

    fn yearly(&mut self, now: f64, notes: &mut Vec<Note>) {
        self.census.yearly(now, self.hybrid && self.gene_kit);
        let pop = self.population();
        self.history.push([now as f32, pop as f32]);
        // The plant keeps everyone equipped (up to its capacity).
        if self.pmp > 0 {
            let served = pop.min(self.pmp as f64 * 10_000.0);
            let mut made = 0;
            for g in Good::SUPPLIES {
                if g == Good::HazardDome && (!self.hab.needs_domes() || self.biodomes as f64 * 10_000.0 >= served) {
                    continue;
                }
                let target = g.units_for(served);
                let have = self.units(g);
                if target > have {
                    let cost = g.mass_t() * 1.5;
                    let k = (target - have).min((self.materials / cost).floor() as u32);
                    if k > 0 {
                        self.materials -= k as f64 * cost;
                        self.add_supply(g, k, now);
                        made += k;
                    }
                }
            }
            if made > 0 && self.materials < 1.0 {
                notes.push((Level::Warn, "The manufacturing plant ran out of raw materials.".into()));
            }
        }
        if self.auto_build && self.queue.is_empty() {
            let pick = if self.hab.needs_domes() && self.shelter_capacity(1.0) < pop * 1.1 {
                Some(Project::BioDome)
            } else if (self.pmp as f64) * 10_000.0 < pop {
                Some(Project::Pmp)
            } else {
                None
            };
            if let Some(p) = pick.filter(|p| self.can_build(*p).is_ok()) {
                self.queue.push(Construction { kind: p, done: 0.0 });
            }
        }
        if self.dock {
            self.refresh_market();
        }
        if !self.won && pop >= WIN_POP {
            self.won = true;
            notes.push((Level::Good, format!("{} has reached a million people. You have won!", self.name)));
        }
        if self.tech < 2.0 && self.units(Good::AiCluster) == 0 {
            notes.push((Level::Warn, "With no AI clusters, the colony is losing its knowledge.".into()));
        }
    }

    /// Restock the space dock's visiting merchants (yearly).
    pub fn refresh_market(&mut self) {
        let mut offers = BTreeMap::new();
        let mut rng = crate::rngx::derive(self.census.seed ^ 0x0d_0c4, (self.clock * 12.0) as u64);
        let k = if self.business { 2.0 } else { 1.0 };
        for g in Good::SUPPLIES.iter().copied().chain([Good::GeneKit]) {
            let price = g.base_price() * if self.business { 0.95 } else { 1.15 } * rng.random_range(0.9..1.15);
            let stock = (g.units_for(self.population() * 0.05).max(2) as f64 * k * rng.random_range(0.4..1.2)).round() as u32;
            offers.insert(g, Offer { price, stock, sells: rng.random_bool(0.85), buys: false });
        }
        self.market = Some(CargoMarket { offers });
    }

    /// What a tonne of the stockpile fetches at the dock.
    pub fn materials_price(&self) -> f64 {
        (180.0 + 120.0 * self.wealth) * if self.business { 1.3 } else { 1.0 }
    }

    /// The colony as an independent settlement (after it is abandoned).
    pub fn as_independent(&self) -> Colony {
        let pop = self.population();
        let tech = if self.tech >= 5.5 && pop >= 100_000.0 {
            ColonyTech::Advanced
        } else if self.tech >= 4.0 && pop >= 10_000.0 {
            ColonyTech::Industrial
        } else if pop >= 1_000.0 {
            ColonyTech::Settled
        } else if pop >= 1.0 {
            ColonyTech::FrontierOutpost
        } else {
            ColonyTech::Derelict
        };
        let size = if pop >= 1e6 { "Large" } else if pop >= 1e4 { "Medium" } else { "Small" };
        Colony {
            name: self.name.clone(),
            founders: format!("{} settlers (independent since its founder left)", self.species()),
            size: size.into(),
            tech,
            population: pop.round(),
            missions: "Independent".into(),
            factions: "One".into(),
            faction_type: "Elected council".into(),
            in_orbit: if self.dock { "A space dock".into() } else { "None".into() },
        }
    }
}

fn label(v: f64, names: [&'static str; 6]) -> &'static str {
    names[((v.clamp(0.0, 0.999)) * 6.0) as usize]
}

/// Tech level a colony starts at for `n` founders: the standard for 500,
/// far below it for a handful.
pub fn founding_tech(n: f64) -> f64 {
    2.0 + 3.0 * ((n / 2.0).ln() / 250f64.ln()).clamp(0.0, 1.0)
}

/// Genetic penalty from a small founding population.
pub fn founding_genetic(n: f64) -> f64 {
    if n >= 500.0 { 0.0 } else { (0.6 / (n / 2.0).max(1.0).sqrt()).min(0.6) }
}

/// Who would found the colony right now, and why not.
#[derive(Debug, Clone)]
pub struct FoundingPlan {
    pub colonists: u32,
    pub hab: Hab,
    pub hybrid: bool,
    pub gene_kit: bool,
    /// (supply, have, needed)
    pub kit: Vec<(Good, u32, u32)>,
    pub warnings: Vec<String>,
}

impl Sim {
    /// Why a colony cannot be founded here, or the plan if it can.
    pub fn founding_plan(&self, gal: &Galaxy) -> Result<FoundingPlan, String> {
        if self.colony.is_some() {
            return Err("You already have a colony. You can only keep one.".into());
        }
        if self.trip.is_some() || self.shuttle.is_some() {
            return Err("Dock at the world first.".into());
        }
        if !self.loc.is_body() {
            return Err("Dock at a planet or moon to found a colony there.".into());
        }
        let sys = gal.system_ref(self.sys_id);
        let Some(p) = sys.site(self.loc) else { return Err("No world here.".into()) };
        world::colonizable(p).map_err(|e| e.to_string())?;
        let spec = self.ship();
        let pilot = self.pilot.is_some() as usize;
        let colonists = (self.crew.count - pilot + self.pax.count) as u32;
        if colonists < 2 {
            return Err("A colony needs at least a couple: recruit a partner at a port first.".into());
        }
        let hab = world::habitability(p);
        let hybrid = self.spouse.as_ref().is_some_and(|s| s.species.is_some());
        let gene_kit = self.hold.get(Good::GeneKit) > 0;
        let mut kit = Vec::new();
        let mut warnings = Vec::new();
        for g in Good::SUPPLIES {
            if g == Good::HazardDome && !hab.needs_domes() {
                continue;
            }
            kit.push((g, self.hold.get(g), g.units_for(colonists as f64)));
        }
        if kit.iter().any(|(_, have, need)| have < need) {
            warnings.push("Some supplies are short: the colony will suffer until you bring more.".into());
        }
        if hab == Hab::Lethal {
            let domes = self.hold.get(Good::HazardDome) as f64 * 25.0;
            if domes < colonists as f64 {
                warnings.push(format!("{} colonists would have no dome on a lethal world, and die.", (colonists as f64 - domes) as u32));
            }
        }
        if hybrid && !gene_kit {
            warnings.push("Your partner is a native: without a Genetic Modification Kit you cannot have children together.".into());
        }
        if spec.crew == 1 && colonists == 2 {
            warnings.push("Two founders: the colony starts far below the galactic tech level and needs near-ideal conditions.".into());
        }
        Ok(FoundingPlan { colonists, hab, hybrid, gene_kit, kit, warnings })
    }

    /// Found the colony here. The crew (and passengers) settle, the hold is
    /// unloaded, and the ship is mothballed.
    pub fn found_colony(&mut self, gal: &mut Galaxy) -> Result<(), String> {
        let plan = self.founding_plan(gal)?;
        let now = years(self.t);
        let (world_name, hab, danger, richness) = {
            let sys = gal.system_ref(self.sys_id);
            let p = sys.site(self.loc).unwrap();
            let rich = ((world::richness_t(p).max(1.0).log10() - 3.5) / 3.5).clamp(0.0, 1.0);
            (loc_label(self.loc, sys), plan.hab, world::danger(p), rich)
        };
        let seed = crate::rngx::mix64(self.seed ^ self.sys_id ^ (self.t as u64));
        let mut rng = crate::rngx::derive(seed, 0xf0_0d);
        let mut census = Census::new(seed, now);
        // The captain.
        let player = census.add(Person {
            female: self.player_sex.is_female(),
            born: self.player_birth_y as f32,
            died: None,
            mother: None,
            father: None,
            spouse: None,
            alien: false,
            origin: Origin::Founder,
            due: None,
            unseen: vec![],
            parents_unseen: false,
            line: true,
        });
        census.given.insert(player, self.player_name().to_string());
        census.people[player as usize].line = true;
        let mut spouse_id = None;
        if let Some(sp) = &self.spouse {
            let id = census.add(Person {
                female: sp.sex.is_female(),
                born: (now - rng.random_range(22.0..34.0)) as f32,
                died: None,
                mother: None,
                father: None,
                spouse: Some(player),
                alien: false,
                origin: Origin::Founder,
                due: None,
                unseen: vec![],
                parents_unseen: false,
                line: false,
            });
            census.given.insert(id, sp.name.clone());
            census.people[player as usize].spouse = Some(id);
            spouse_id = Some(id);
        }
        // The rest of the crew (not the pilot, who stays with the ship),
        // then the passengers. Sexes balance across the whole company.
        let others: Vec<String> = self
            .crew
            .names
            .iter()
            .skip(1)
            .filter(|n| self.spouse.as_ref().is_none_or(|s| &s.name != *n) && self.pilot.as_ref().is_none_or(|p| &p.name != *n))
            .cloned()
            .collect();
        let total = plan.colonists as usize;
        let mut females = census.people.iter().filter(|p| p.female).count();
        let mut placed = census.people.len();
        let mut add = |census: &mut Census, given: Option<String>, rng: &mut rand::rngs::StdRng, age: f64| {
            let remaining = total - placed;
            let want_f = (total / 2).saturating_sub(females);
            let female = if remaining > 0 { rng.random_range(0..remaining) < want_f } else { rng.random_bool(0.5) };
            if female {
                females += 1;
            }
            placed += 1;
            let id = census.add(Person {
                female,
                born: (now - age) as f32,
                died: None,
                mother: None,
                father: None,
                spouse: None,
                alien: false,
                origin: Origin::Founder,
                due: None,
                unseen: vec![],
                parents_unseen: false,
                line: false,
            });
            if let Some(g) = given {
                census.given.insert(id, g);
            }
        };
        for n in others {
            let age = rng.random_range(22.0..46.0);
            add(&mut census, Some(n), &mut rng, age);
        }
        for _ in 0..self.pax.count {
            let age = rng.random_range(16.0..48.0);
            add(&mut census, None, &mut rng, age);
        }
        census.yearly(now, false);
        census.last_year = Default::default();

        let n = plan.colonists as f64;
        let mut colony = PlayerColony {
            name: format!("New {}", gal.system_ref(self.sys_id).star.name),
            sys_id: self.sys_id,
            loc: self.loc,
            world: world_name.clone(),
            founded: now,
            clock: now,
            month: 0,
            hab,
            danger,
            richness,
            hybrid: plan.hybrid,
            gene_kit: plan.hybrid && plan.gene_kit,
            founders: plan.colonists,
            genetic0: founding_genetic(n) + if plan.hybrid { 0.1 } else { 0.0 },
            tech: founding_tech(n),
            health: 0.85,
            happiness: 0.7,
            wealth: 0.4,
            supplies: BTreeMap::new(),
            materials: 0.0,
            pmp: 0,
            biodomes: 0,
            dock: false,
            business: false,
            queue: vec![],
            auto_build: true,
            census,
            player,
            history: vec![[now as f32, n as f32]],
            won: false,
            market: None,
            ind: Indicators::default(),
        };
        let _ = spouse_id;
        if plan.hybrid && plan.gene_kit {
            self.hold.take(Good::GeneKit, 1);
        }
        self.colony = Some(colony);
        self.unload_to_colony();
        // The ship keeps only the captain and the pilot.
        let mut names = vec![self.player_name().to_string()];
        if let Some(p) = &self.pilot {
            names.push(p.name.clone());
        }
        self.crew.count = names.len();
        self.crew.names = names;
        self.pax.count = 0;
        self.spouse = None;
        self.in_colony = true;
        self.ship_dormant = false;
        colony = self.colony.take().unwrap();
        let msg = format!(
            "{} is founded on {} by {} colonists ({}). The ship is mothballed in orbit.",
            colony.name,
            world_name,
            plan.colonists,
            hab.name().to_lowercase()
        );
        self.colony = Some(colony);
        self.log.push(self.t, Level::Good, msg);
        self.log.push(self.t, Level::Info, "Colony mode: a year passes in about 24 seconds at 1x. Leave to fly for supplies; the colony lives on without you.");
        Ok(())
    }

    /// Unload colony supplies, captives and spare raw materials into the
    /// colony (the ship must be docked at it).
    pub fn unload_to_colony(&mut self) -> String {
        let now = years(self.t);
        let (sys_id, loc) = match &self.colony {
            Some(c) => (c.sys_id, c.loc),
            None => return "You have no colony.".into(),
        };
        if self.sys_id != sys_id || self.loc != loc || self.trip.is_some() || self.shuttle.is_some() {
            return "Dock at your colony to unload.".into();
        }
        let mut parts = Vec::new();
        let goods: Vec<(Good, u32)> = self.hold.units.iter().map(|(g, n)| (*g, *n)).collect();
        let col = self.colony.as_mut().unwrap();
        for (g, n) in goods {
            if g.supply().is_some() {
                self.hold.take(g, n);
                col.add_supply(g, n, now);
                parts.push(format!("{n} × {g}"));
            } else if g == Good::GeneKit && col.hybrid && !col.gene_kit {
                self.hold.take(g, 1);
                col.gene_kit = true;
                parts.push("a Genetic Modification Kit".into());
            } else if g == Good::Slaves {
                self.hold.take(g, n);
                let mut rng = crate::rngx::derive(col.census.seed ^ 0x51_a7e5, col.census.people.len() as u64);
                for _ in 0..n {
                    col.census.add(Person {
                        female: rng.random_bool(0.5),
                        born: (now - rng.random_range(16.0..40.0)) as f32,
                        died: None,
                        mother: None,
                        father: None,
                        spouse: None,
                        alien: true,
                        origin: Origin::Captive,
                        due: None,
                        unseen: vec![],
                        parents_unseen: false,
                        line: false,
                    });
                }
                parts.push(format!("{n} captives"));
            }
        }
        // Raw materials beyond what the ship needs to keep running.
        let mut t = 0.0;
        for r in [Res::Metal, Res::Silicate, Res::Carbon, Res::Mat] {
            let spare = (self.inv.get(r) - self.inv.cap(r) * 0.3).max(0.0);
            t += self.inv.take(r, spare) / 1_000.0;
        }
        if t >= 0.01 {
            col.materials += t;
            parts.push(format!("{t:.1} t of raw materials"));
        }
        let msg = if parts.is_empty() { "Nothing to unload.".to_string() } else { format!("Unloaded {}.", parts.join(", ")) };
        self.log.push(self.t, Level::Info, msg.clone());
        msg
    }

    /// Back to the ship (space mode). The colony lives on at ship time.
    pub fn leave_colony(&mut self) -> Result<(), String> {
        if !self.in_colony {
            return Ok(());
        }
        if self.ship_dormant {
            return Err("There is no ship to leave in.".into());
        }
        self.in_colony = false;
        self.log.push(self.t, Level::Info, "You take the ship out. The colony carries on without you.");
        Ok(())
    }

    /// Settle back in at the colony (the ship must be docked there).
    pub fn enter_colony(&mut self) -> Result<(), String> {
        let Some(c) = &self.colony else { return Err("You have no colony.".into()) };
        if self.sys_id != c.sys_id || self.loc != c.loc || self.trip.is_some() || self.shuttle.is_some() {
            return Err(format!("Dock at {} to return to your colony.", c.world));
        }
        self.in_colony = true;
        self.unload_to_colony();
        self.log.push(self.t, Level::Info, "Home at the colony. The ship is mothballed.");
        Ok(())
    }

    /// Give the colony up: it becomes an independent settlement, and you
    /// fly on with the ship.
    pub fn abandon_colony(&mut self, gal: &mut Galaxy) -> Result<(), String> {
        if self.ship_dormant {
            return Err("Without a ship there is nowhere to go.".into());
        }
        let Some(c) = self.colony.take() else { return Err("You have no colony.".into()) };
        let sys = gal.system(c.sys_id);
        if let Some(p) = sys.site_mut(c.loc) {
            p.colony = Some(c.as_independent());
        }
        sys.civ = crate::sim::worldgen::derive_civ(&sys.planets);
        sys.market = None;
        sys.refresh_cargo_market(self.seed);
        self.in_colony = false;
        self.log.push(self.t, Level::Warn, format!("You leave {}. It carries on as an independent colony of {:.0}.", c.name, c.population()));
        Ok(())
    }

    /// Queue a building project.
    pub fn build(&mut self, p: Project) -> Result<(), String> {
        let Some(c) = self.colony.as_mut() else { return Err("You have no colony.".into()) };
        c.can_build(p)?;
        c.queue.push(Construction { kind: p, done: 0.0 });
        self.log.push(self.t, Level::Info, format!("Queued: {}.", p.name()));
        Ok(())
    }

    pub fn cancel_build(&mut self, idx: usize) {
        if let Some(c) = self.colony.as_mut()
            && idx < c.queue.len()
        {
            let k = c.queue.remove(idx);
            self.log.push(self.t, Level::Info, format!("Cancelled: {} (materials used so far are lost).", k.kind.name()));
        }
    }

    /// Buy supplies from the merchants at the colony's space dock.
    pub fn dock_buy(&mut self, g: Good, n: u32) {
        let now = years(self.t);
        let Some(c) = self.colony.as_mut() else { return };
        let Some(m) = c.market.as_mut() else {
            self.log.push(self.t, Level::Warn, "The colony has no space dock yet.");
            return;
        };
        let Some(price) = m.buy_price(g) else {
            self.log.push(self.t, Level::Warn, format!("No merchant has {g} this year."));
            return;
        };
        let stock = m.offers[&g].stock;
        let k = n.min(stock).min((self.credits / price).floor() as u32);
        if k == 0 {
            self.log.push(self.t, Level::Warn, format!("You cannot afford {g} ({price:.0} cr each)."));
            return;
        }
        if let Some(o) = m.offers.get_mut(&g) {
            o.stock -= k;
        }
        self.credits -= k as f64 * price;
        if g == Good::GeneKit {
            c.gene_kit = c.gene_kit || c.hybrid;
        } else {
            c.add_supply(g, k, now);
        }
        self.log.push(self.t, Level::Good, format!("The dock delivers {k} × {g} for {:.0} cr.", k as f64 * price));
    }

    /// Sell tonnes of the colony's stockpile at the dock.
    pub fn dock_sell_materials(&mut self, tonnes: f64) {
        let Some(c) = self.colony.as_mut() else { return };
        if !c.dock {
            self.log.push(self.t, Level::Warn, "The colony has no space dock yet.");
            return;
        }
        let t = tonnes.min(c.materials).max(0.0);
        if t < 0.01 {
            self.log.push(self.t, Level::Warn, "The stockpile is empty.");
            return;
        }
        let pay = t * c.materials_price();
        c.materials -= t;
        self.credits += pay;
        self.log.push(self.t, Level::Good, format!("Sold {t:.0} t of colony materials for {pay:.0} cr."));
    }

    /// Time passes for the colony (and the captain ages).
    pub(crate) fn colony_tick(&mut self, gal: &mut Galaxy) {
        let now = years(self.t);
        let home = self.in_colony;
        let boost = (self.artifact_tech_boost(), self.artifact_health_boost());
        let mut notes = Vec::new();
        if let Some(c) = self.colony.as_mut() {
            c.advance(now, home, boost, &mut notes);
        }
        for (lvl, msg) in notes {
            self.log.push(self.t, lvl, msg);
        }
        if self.colony.is_some() && self.player_age() >= self.retire_age {
            self.retire(gal);
        }
    }

    /// The captain retires; a colonist takes over, seamlessly.
    pub fn retire(&mut self, _gal: &mut Galaxy) {
        let now = years(self.t);
        let old_age = self.player_age().floor();
        let Some(c) = self.colony.as_mut() else { return };
        let old = c.player;
        let old_name = c.census.name(old);
        // Prefer an adult descendant, then any adult.
        let mut line = Vec::new();
        let mut frontier = vec![old];
        let mut guard = 0;
        while let Some(x) = frontier.pop() {
            c.census.ensure_family(x, now);
            for k in c.census.children(x) {
                line.push(k);
                frontier.push(k);
            }
            guard += 1;
            if guard > 200 {
                break;
            }
        }
        let adult = |p: &Person| p.alive() && !p.alien && (22.0..55.0).contains(&p.age(now));
        let pick = line
            .iter()
            .copied()
            .filter(|&i| adult(&c.census.people[i as usize]))
            .max_by(|a, b| c.census.people[*a as usize].born.total_cmp(&c.census.people[*b as usize].born).reverse())
            .or_else(|| c.census.living.iter().copied().filter(|&i| i != old && adult(&c.census.people[i as usize])).max_by_key(|&i| (c.census.people[i as usize].born * 10.0) as i64))
            .or_else(|| c.census.materialize(35, !c.census.people[old as usize].female, now, false));
        let Some(new) = pick else {
            // Nobody left to take over: the captain carries on.
            self.retire_age += 5.0;
            return;
        };
        // The old captain lives out their days as a colonist.
        c.census.people[old as usize].line = true;
        c.census.people[new as usize].line = true;
        c.player = new;
        let p = c.census.people[new as usize].clone();
        let new_name = c.census.name(new);
        let kin = if p.mother == Some(old) || p.father == Some(old) {
            if p.female { "daughter" } else { "son" }
        } else if line.contains(&new) {
            "descendant"
        } else {
            "colonist"
        };
        self.player_sex = crate::sim::Sex::from_female(p.female);
        self.player_birth_y = p.born as f64;
        let mut rng = crate::rngx::derive(self.seed ^ new as u64, 0x7e_71e);
        self.retire_age = rng.random_range(60.0f64..80.0).max(p.age(now) + 5.0);
        self.spouse = None;
        if let Some(first) = self.crew.names.first_mut() {
            *first = new_name.clone();
        }
        self.log.push(self.t, Level::Good, format!("{old_name} retires at {old_age:.0}. A Suitable Replacement was Chosen: your {kin} {new_name}, {:.0}.", p.age(now).floor()));
    }

    /// The ship was lost while a colony stands: play goes on there.
    pub(crate) fn ship_lost_to_colony(&mut self, cause: &str, detail: &str) {
        self.ship_dormant = true;
        self.in_colony = true;
        self.trip = None;
        self.shuttle = None;
        self.encounter = None;
        self.log.push(self.t, Level::Critical, format!("{cause}. {detail}"));
        self.log.push(self.t, Level::Critical, "The ship is lost, but your colony endures. Its people will carry your name on.");
    }

    /// Hours of game time per real second in colony mode at time scale `ff`.
    pub fn colony_hours_per_sec(ff: u32) -> f64 {
        HOURS_PER_YEAR / SECS_PER_COLONY_YEAR * ff as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::planet::{AtmoKind, TempBand};

    /// Dock the ship at a rocky body of the home system and shape it.
    fn at_world(ship: usize, hab: Hab) -> (Sim, Galaxy) {
        let mut gal = Galaxy::new(42);
        let mut sim = Sim::new_game(42, ship, &mut gal);
        let home = gal.home_id();
        let idx = gal.system(home).planets.iter().position(|p| world::colonizable(p).is_ok()).expect("a rocky world");
        {
            let p = &mut gal.system(home).planets[idx];
            p.gravity = 1.0;
            p.life = None;
            match hab {
                Hab::Habitable => {
                    p.atmosphere = Some(AtmoKind::Breathable);
                    p.temp_band = TempBand::Temperate;
                }
                Hab::Lethal => p.atmosphere = Some(AtmoKind::Corrosive),
                _ => p.atmosphere = Some(AtmoKind::Toxic),
            }
        }
        sim.loc = Loc::Planet(idx);
        (sim, gal)
    }

    fn run_years(sim: &mut Sim, gal: &mut Galaxy, y: f64) {
        let mut done = 0.0;
        while done < y {
            sim.advance(HOURS_PER_YEAR / 12.0, gal);
            done += 1.0 / 12.0;
        }
    }

    #[test]
    fn the_ark_founds_a_growing_colony() {
        let (mut sim, mut gal) = at_world(2, Hab::Habitable);
        sim.found_colony(&mut gal).expect("founded");
        assert!(sim.in_colony);
        let c = sim.colony.as_ref().unwrap();
        assert_eq!(c.population(), 510.0);
        assert!(c.units(Good::HydroKit) >= 6, "the kit was unloaded");
        assert!(sim.hold.units.keys().all(|g| !g.is_supply()));
        assert!((c.tech - STANDARD_TECH).abs() < 0.01);
        run_years(&mut sim, &mut gal, 100.0);
        let c = sim.colony.as_ref().unwrap();
        assert!(c.population() > 1_500.0, "grew to {}", c.population());
        assert!(c.history.len() >= 99);
    }

    #[test]
    fn founders_without_shelter_die_on_a_lethal_world() {
        let (mut sim, mut gal) = at_world(1, Hab::Lethal);
        sim.found_colony(&mut gal).expect("founded");
        run_years(&mut sim, &mut gal, 1.0);
        // Only the captain (whose turn ends by retirement) is left.
        assert!(sim.colony.as_ref().unwrap().population() <= 1.0);
    }

    #[test]
    fn a_lone_captain_cannot_found_a_colony() {
        let (sim, gal) = at_world(0, Hab::Habitable);
        assert!(sim.founding_plan(&gal).is_err());
    }

    #[test]
    fn the_company_ship_splits_five_and_five() {
        let (mut sim, mut gal) = at_world(1, Hab::Habitable);
        sim.found_colony(&mut gal).unwrap();
        let c = sim.colony.as_ref().unwrap();
        let f = c.census.people.iter().filter(|p| p.female).count();
        assert_eq!(c.census.people.len(), 10);
        assert_eq!(f, 5);
        assert!(c.tech < STANDARD_TECH, "ten founders start below the standard");
        assert_eq!(sim.crew.count, 1, "the captain flies alone");
    }

    #[test]
    fn retirement_hands_over_to_a_colonist() {
        let (mut sim, mut gal) = at_world(2, Hab::Habitable);
        sim.found_colony(&mut gal).unwrap();
        let first = sim.player_name().to_string();
        sim.retire_age = sim.player_age() + 2.0;
        run_years(&mut sim, &mut gal, 3.0);
        assert_ne!(sim.player_name(), first);
        assert!(sim.player_age() < sim.retire_age);
    }

    #[test]
    fn abandoning_leaves_an_independent_colony() {
        let (mut sim, mut gal) = at_world(2, Hab::Habitable);
        sim.found_colony(&mut gal).unwrap();
        let loc = sim.loc;
        sim.abandon_colony(&mut gal).unwrap();
        assert!(sim.colony.is_none() && !sim.in_colony);
        let sys = gal.system(sim.sys_id);
        assert!(sys.site(loc).unwrap().colony.is_some());
        assert!(sys.civ.is_some(), "the system now has a civilisation");
    }

    #[test]
    fn supplies_wear_out_and_the_plant_replaces_them() {
        let (mut sim, mut gal) = at_world(2, Hab::Habitable);
        sim.found_colony(&mut gal).unwrap();
        // Med kits last a year.
        run_years(&mut sim, &mut gal, 1.5);
        assert_eq!(sim.colony.as_ref().unwrap().units(Good::MedKit), 0);
        let c = sim.colony.as_mut().unwrap();
        c.pmp = 1;
        c.materials = 10_000.0;
        run_years(&mut sim, &mut gal, 1.1);
        assert!(sim.colony.as_ref().unwrap().units(Good::MedKit) > 0);
    }

    #[test]
    fn founding_tech_and_genetics_scale_with_founders() {
        assert!((founding_tech(510.0) - STANDARD_TECH).abs() < 0.01);
        assert!(founding_tech(10.0) < 3.6 && founding_tech(2.0) <= 2.0 + 1e-9);
        assert!(founding_genetic(2.0) > founding_genetic(10.0));
        assert_eq!(founding_genetic(500.0), 0.0);
    }
}
