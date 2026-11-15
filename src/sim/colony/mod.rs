//! The player's colony: founding, supplies, buildings, the census, and the
//! long game of growing to a million people.
//!
//! Time runs fast in colony mode (about 24 real seconds per year at 1x, so
//! a lifetime is roughly two hours) and at ordinary ship time while the
//! captain is away buying supplies.

pub mod census;
pub mod law;
pub mod world;

use super::cargo::{CargoMarket, Good, Offer};
use super::galaxy::Galaxy;
use super::logbuf::Level;
use super::people::{years, HOURS_PER_YEAR};
use super::planet::{Colony, ColonyTech};
use super::res::Res;
use super::state::{loc_label, Loc, Sim};
use census::{Census, Emigrant, Event, Origin, Person, Rates, ADULT};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub use world::Hab;

/// Population that wins the game.
pub const WIN_POP: f64 = 1_000_000.0;
/// Real seconds per colony year at 1x in colony mode.
pub const SECS_PER_COLONY_YEAR: f64 = 96.0;
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

/// Of `batches`, the `need` units nearest wearing out are in use and age;
/// the rest sit idle in store, so their wear-out date moves on by `dt`.
fn idle_keep(batches: &mut Vec<Batch>, need: u32, dt: f64) {
    batches.sort_by(|a, b| a.until.total_cmp(&b.until));
    let mut left = need;
    let mut out: Vec<Batch> = Vec::with_capacity(batches.len() + 1);
    for b in batches.drain(..) {
        let used = b.units.min(left);
        left -= used;
        if used > 0 {
            out.push(Batch { units: used, until: b.until });
        }
        if b.units > used {
            out.push(Batch { units: b.units - used, until: b.until + dt });
        }
    }
    // Merge batches that wear out together.
    out.sort_by(|a, b| a.until.total_cmp(&b.until));
    for b in out {
        match batches.last_mut() {
            Some(l) if (l.until - b.until).abs() < 1e-6 => l.units += b.units,
            _ => batches.push(b),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Project {
    /// Programmable Manufacturing Plant: makes supplies from raw materials.
    Pmp,
    BioDome,
    SpaceDock,
    Business,
    /// The colony's crowning work: builds ships.
    Shipyard,
    /// A ship on the shipyard's slip (index into `SHIPS`).
    Ship(usize),
}

/// Colonists a shipyard needs.
pub const SHIPYARD_POP: f64 = 250_000.0;
/// Ships the shipyard can have on its slips at once.
pub const HANGAR_CAP: usize = 3;

impl Project {
    /// The buildings (ships are listed by the shipyard).
    pub const ALL: [Project; 5] = [Project::Pmp, Project::BioDome, Project::SpaceDock, Project::Business, Project::Shipyard];

    pub fn name(self) -> &'static str {
        match self {
            Project::Pmp => "Programmable Manufacturing Plant",
            Project::BioDome => "Bio-Dome",
            Project::SpaceDock => "Space Dock",
            Project::Business => "Business Center",
            Project::Shipyard => "Shipyard",
            Project::Ship(i) => crate::sim::ShipSpec::spec(i).name,
        }
    }

    pub fn desc(self) -> &'static str {
        match self {
            Project::Pmp => "Builds every colony supply from raw materials, replacing worn-out kits and equipping new colonists, for up to 10,000 people. Ends the need to import supplies. With a space dock, each plant also keeps supplies for 1,000 more colonists on sale there.",
            Project::BioDome => "A permanent habitat for 10,000 colonists on a world they cannot live on unprotected.",
            Project::SpaceDock => "Brings visiting merchants: sell the colony's materials and buy supplies without leaving. Needs 100,000 colonists.",
            Project::Business => "Improves the economy: better prices and stock at the space dock, and a higher standard of living.",
            Project::Shipyard => "The colony's crowning work: slips and foundries that build ships from the stockpile — any of the three founding designs, and others once you buy their blueprints. Needs a space dock, a Business Center and 250,000 colonists.",
            Project::Ship(i) => crate::sim::ShipSpec::spec(i).blurb,
        }
    }

    /// (tonnes of materials, labour-years)
    pub fn cost(self) -> (f64, f64) {
        match self {
            Project::Pmp => (800.0, 300.0),
            Project::BioDome => (400.0, 150.0),
            Project::SpaceDock => (8_000.0, 4_000.0),
            Project::Business => (2_000.0, 800.0),
            Project::Shipyard => (20_000.0, 8_000.0),
            Project::Ship(i) => crate::sim::ShipSpec::spec(i).build_cost(),
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
    /// Share of the colony the land itself feeds by hand (hunting,
    /// gathering, farming) where hydroponics fall short.
    #[serde(default)]
    pub forage: f64,
    /// Materials flows (t/yr): dug and hauled, consumed by upkeep, and
    /// drawn by construction (at last month's rate).
    #[serde(default)]
    pub materials_made_y: f64,
    #[serde(default)]
    pub materials_upkeep_y: f64,
    #[serde(default)]
    pub materials_build_y: f64,
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
    /// Genetic Modification Kits in store: each lets one human and one
    /// native have (hybrid) children, used up yearly as couples marry.
    #[serde(default)]
    pub gene_kits: u32,
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
    /// The space dock's merchants. Its `stocked_t` is in game years.
    pub market: Option<CargoMarket>,
    /// The dock's usual stock per good (merchants' plus the plants'
    /// export share): it restocks toward this over time.
    #[serde(default)]
    pub dock_usual: BTreeMap<Good, u32>,
    #[serde(default)]
    pub ind: Indicators,
    /// Laws in force.
    #[serde(default)]
    pub laws: Vec<law::Enacted>,
    /// The founding captain's background (some of it shapes the colony).
    #[serde(default)]
    pub founder_bg: super::background::Background,
    /// Births and deaths since founding (for the statistics).
    #[serde(default)]
    pub births_total: f64,
    #[serde(default)]
    pub deaths_total: f64,
    /// The colony has built its shipyard.
    #[serde(default)]
    pub shipyard: bool,
    /// Ship designs (indices into `SHIPS`) whose plans have been bought.
    #[serde(default)]
    pub blueprints: Vec<usize>,
    /// A further colony founded under Hegemony by settlers alone (the
    /// captain's family lives at the capital, `Sim::colony`).
    #[serde(default)]
    pub outpost: bool,
    /// Materials the manufacturing plant used last year (t).
    #[serde(default)]
    pub plant_spent_t: f64,
    /// Ships just launched, not yet handed to the captain's fleet.
    #[serde(default)]
    pub hangar: Vec<usize>,
    /// Resource Drones refits from quest rewards: each level lets a unit
    /// serve 10% more colonists.
    #[serde(default)]
    pub drone_lvl: u8,
    /// Small permanent gains from quest rewards.
    #[serde(default)]
    pub boons: Boons,
}

/// Highest Resource Drones refit level.
pub const MAX_DRONE_LVL: u8 = 5;

/// Permanent +1% gains a colony earns from quest rewards, one count each.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Boons {
    pub produce: u8,
    pub health: u8,
    pub tech: u8,
    pub fert: u8,
}

impl Boons {
    pub fn apply(&self, f: &mut law::LawFx) {
        f.produce *= 1.0 + 0.01 * self.produce as f64;
        f.health += 0.01 * self.health as f64;
        f.tech_growth *= 1.0 + 0.01 * self.tech as f64;
        f.fert *= 1.0 + 0.01 * self.fert as f64;
    }

    /// Grant one at random; returns what it was.
    pub fn grant(&mut self, pick: usize) -> &'static str {
        match pick % 4 {
            0 => {
                self.produce += 1;
                "materials output +1%"
            }
            1 => {
                self.health += 1;
                "colony health +1%"
            }
            2 => {
                self.tech += 1;
                "tech growth +1%"
            }
            _ => {
                self.fert += 1;
                "birth rate +1%"
            }
        }
    }

    /// One line per boon held, for the colony screen.
    pub fn lines(&self) -> Vec<String> {
        [(self.produce, "materials output"), (self.health, "health"), (self.tech, "tech growth"), (self.fert, "birth rate")]
            .iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, what)| format!("{what} +{n}%"))
            .collect()
    }
}

/// Something the colony wants to tell the captain.
pub type Note = (Level, String);

impl PlayerColony {
    /// The captain and every descendant of theirs on record: the heirs,
    /// who never leave the capital.
    pub fn heirs(&self) -> std::collections::BTreeSet<u32> {
        let mut out = std::collections::BTreeSet::new();
        if self.outpost {
            return out;
        }
        let mut stack = vec![self.player];
        while let Some(x) = stack.pop() {
            if out.insert(x) && out.len() < 100_000 {
                stack.extend(self.census.children(x));
            }
        }
        out
    }

    /// Draw `n` settlers for an ark this colony built: anyone may go
    /// (colonists' descendants or not), and a share of the royal line if
    /// it is here, but never the captain's heirs. Returns the settlers.
    pub fn ark_settlers(&mut self, n: usize, now: f64) -> Vec<Emigrant> {
        let n = n.min((self.population() - 100.0).max(0.0) as usize);
        let keep = self.heirs();
        let mut royals: Vec<u32> = self
            .census
            .living
            .iter()
            .copied()
            .filter(|&i| {
                let p = &self.census.people[i as usize];
                p.line && !keep.contains(&i) && (16.0..60.0).contains(&p.age(now))
            })
            .collect();
        // A quarter of the royal line present, at least one, at most a
        // tenth of the ark.
        let want = royals.len().div_ceil(4).min(n / 10);
        let mut rng = crate::rngx::derive(self.census.seed ^ 0xa5_c0, (now * 12.0) as u64);
        for k in (1..royals.len()).rev() {
            royals.swap(k, rng.random_range(0..=k));
        }
        royals.truncate(want);
        self.census.emigrate(&royals, n, &keep, now)
    }

    /// Net change of the stockpile per year, everything counted: what is
    /// dug, less upkeep, construction and the plant's yearly draw.
    pub fn materials_trend_y(&self) -> f64 {
        self.ind.materials_made_y - self.ind.materials_upkeep_y - self.ind.materials_build_y - self.plant_spent_t
    }

    pub fn population(&self) -> f64 {
        self.census.population()
    }

    /// Colonists one unit of supply `g` serves here.
    pub fn per(&self, g: Good) -> f64 {
        let base = g.supply().map(|x| x.per).unwrap_or(1.0);
        if g == Good::ResDrones { base * (1.0 + 0.1 * self.drone_lvl as f64) } else { base }
    }

    /// Units of `g` needed to serve `colonists` fully here.
    pub fn units_for(&self, g: Good, colonists: f64) -> u32 {
        if g.supply().is_none() {
            return 0;
        }
        (colonists / self.per(g)).ceil().max(0.0) as u32
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
            Project::Shipyard if self.shipyard || queued > 0 => Err("The colony already has its shipyard.".into()),
            Project::Shipyard if !self.dock || !self.business => Err("A shipyard needs a space dock and a Business Center first.".into()),
            Project::Shipyard if pop < SHIPYARD_POP => Err("A shipyard needs 250,000 colonists.".into()),
            Project::Ship(_) if !self.shipyard => Err("Ships are built at a shipyard.".into()),
            Project::Ship(i) if i >= crate::sim::SHIPS.len() => Err("No such design.".into()),
            Project::Ship(i) if i >= crate::sim::STARTERS && !self.blueprints.contains(&i) => Err("Buy the blueprints first.".into()),
            Project::Ship(_) => {
                let slips = self.queue.iter().filter(|c| matches!(c.kind, Project::Ship(_))).count();
                if slips >= HANGAR_CAP {
                    Err(format!("The shipyard has {HANGAR_CAP} slips, all in use."))
                } else {
                    Ok(())
                }
            }
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
        self.refresh_cover();
        self.restock_dock(now);
    }

    /// What the supplies in store cover right now: (coverage by supply,
    /// power efficiency, share sheltered).
    fn supply_cover(&self) -> (BTreeMap<Good, f64>, f64, f64) {
        let pop = self.population().max(1.0);
        let cov = |g: Good| -> f64 { (self.units(g) as f64 * self.per(g) / pop).min(1.0) };
        let power = cov(Good::PowerGen);
        let eff = 0.25 + 0.75 * power;
        let mut coverage = BTreeMap::new();
        coverage.insert(Good::PowerGen, power);
        for g in [Good::HydroKit, Good::Recycler, Good::ResDrones, Good::AiCluster, Good::MedKit] {
            coverage.insert(g, cov(g) * eff);
        }
        // ---- shelter ----
        let sheltered = if self.hab.needs_domes() { (self.shelter_capacity(eff) / pop).min(1.0) } else { 1.0 };
        if self.hab.needs_domes() {
            coverage.insert(Good::HazardDome, sheltered);
        }
        (coverage, eff, sheltered)
    }

    /// Bring the supply indicators up to date with the store at once (a
    /// new colony, or supplies just delivered, need not wait a month).
    pub fn refresh_cover(&mut self) {
        let (coverage, eff, sheltered) = self.supply_cover();
        self.ind.coverage = coverage;
        self.ind.power_eff = eff;
        self.ind.sheltered = sheltered;
    }

    fn step_month(&mut self, player_home: bool, (tech_boost, health_boost): (f64, f64), notes: &mut Vec<Note>) {
        let now = self.clock;
        let dt = MONTH;
        let pop = self.population().max(1.0);

        // ---- supplies wear out ----
        // Only kits in use wear: the colony draws on those nearest the end
        // of their life first, and spares in the store keep.
        let dome_need = if self.hab.needs_domes() { (pop - self.biodomes as f64 * 10_000.0).max(0.0) } else { 0.0 };
        let needs: Vec<(Good, u32)> = self.supplies.keys().map(|&g| (g, self.units_for(g, if g == Good::HazardDome { dome_need } else { pop }))).collect();
        for (g, need) in needs {
            if let Some(batches) = self.supplies.get_mut(&g) {
                idle_keep(batches, need, dt);
            }
        }
        for (g, batches) in self.supplies.iter_mut() {
            let worn: u32 = batches.iter().filter(|b| b.until <= now).map(|b| b.units).sum();
            if worn > 0 {
                batches.retain(|b| b.until > now);
                notes.push((Level::Warn, format!("{worn} × {g} wore out.")));
            }
        }
        let (coverage, eff, sheltered) = self.supply_cover();
        let exposed = 1.0 - sheltered;
        let short = |g: Good| 1.0 - coverage[&g];
        // Food: hydroponics, or the land. Hunting and gathering feed a
        // small colony at once; fields take a few years to clear and plant
        // but then feed any number. Either way it is hard, slow labour
        // (`toil`), and a hostile world gives nothing outside the domes.
        let mut fx = law::effects(&self.laws);
        self.boons.apply(&mut fx);
        let land = match self.hab {
            Hab::Habitable => 1.0,
            Hab::Marginal => 0.6,
            _ => 0.0,
        } * self.founder_bg.land_mult();
        let forage = (land * 300.0 / (300.0 + pop)).min(1.0);
        let farm = (land * (0.3 + 0.7 * ((now - self.founded) / 5.0).clamp(0.0, 1.0))).min(1.0);
        let hydro = coverage[&Good::HydroKit];
        let fed_by_land = forage.max(farm);
        let food_short = 1.0 - hydro.max(fed_by_land);
        let toil = 1.0 - hydro;
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
        let labour_k = 1.0 - 0.5 * toil;
        // Hands dig a little; drones dig a lot.
        let produce = pop * (0.15 * labour_k + 1.4 * coverage[&Good::ResDrones]) * (0.4 + 0.6 * self.richness) * tech_k * fx.produce;
        let upkeep = pop * 0.3 * (1.0 + 0.5 * short(Good::Recycler)) * fx.upkeep;
        self.materials += (produce - upkeep) * dt;
        let shortage = self.materials < 0.0;
        if shortage {
            self.materials = 0.0;
        }

        // ---- the colony's condition ----
        let captives = self.census.living.iter().filter(|&&i| self.census.people[i as usize].origin == Origin::Captive).count() as f64;
        let cs = captives / pop * fx.captives;
        let short_f = if shortage { 1.0 } else { 0.0 };
        let labour_eff = 1.0 - 0.5 * toil;
        // Wealth: what the world can support versus how many share it.
        let support = 20_000.0 * 10f64.powf(2.0 * self.richness) * (1.0 + 0.5 * (self.pmp > 0) as u8 as f64) * if self.business { 2.0 } else { 1.0 } * if self.dock { 1.5 } else { 1.0 };
        let w_target = (0.12 + 0.55 * support / (support + pop) + 0.08 * (self.tech - 3.0) / 2.0 + 0.12 * self.business as u8 as f64 + 0.08 * self.dock as u8 as f64
            - 0.15 * short_f
            + fx.wealth)
            .clamp(0.02, 1.0);
        let h_target = (0.92 - 0.25 * food_short - 0.1 * short(Good::Recycler) - 0.15 * short(Good::MedKit) - dome_health - 0.25 * genetic
            - (self.danger - 0.002) * 5.0
            - 0.2 * cs
            + 0.02 * (self.tech - STANDARD_TECH)
            + 0.05 * self.wealth
            - 0.05 * short_f
            + health_boost
            + fx.health)
            .clamp(0.05, 1.0);
        let hab_q = match self.hab {
            Hab::Habitable => 0.08,
            Hab::Marginal => 0.0,
            Hab::Hostile => -0.06,
            Hab::Lethal => -0.1,
        };
        let p_target = (0.55 + 0.35 * (self.wealth - 0.4) + 0.4 * (self.health - 0.8) + hab_q - 0.8 * cs - 0.15 * short_f - 0.1 * toil
            + 0.08 * self.business as u8 as f64
            + fx.happy)
            .clamp(0.0, 1.0);
        let k = dt / 1.0; // ~1-year response
        self.wealth += (w_target - self.wealth) * k;
        self.health += (h_target - self.health) * k;
        self.happiness += (p_target - self.happiness) * k;

        // ---- tech: AI clusters keep it; without them it collapses ----
        let ai = coverage[&Good::AiCluster];
        if ai < 0.999 {
            self.tech -= 0.5 * (1.0 - ai) * fx.tech_decay * dt;
        } else {
            let slow = if self.tech >= STANDARD_TECH { 0.4 } else { 1.0 };
            self.tech += 0.125 * (pop / (pop + 3_000.0)) * slow * tech_boost * fx.tech_growth * dt;
        }
        self.tech = self.tech.clamp(0.5, MAX_TECH);

        // ---- people ----
        let tech_gap = (STANDARD_TECH - self.tech).max(0.0);
        // Family size: a handful of settlers on an empty world have big
        // families whatever their comforts (only real sickness slows them).
        // As the colony grows, how many children people have comes to
        // depend on health, contentment, food and wealth.
        let frontier = 150.0 / (150.0 + pop);
        let comfort = (self.health / 0.9) * (0.6 + 0.4 * self.happiness) * (1.0 - 0.4 * food_short) * (1.15 - 0.3 * self.wealth);
        let hardy = 0.6 + 0.4 * (self.health / 0.9).min(1.0);
        let fert = (0.28 + 0.35 * frontier) * (frontier * hardy + (1.0 - frontier) * comfort) * dome_fert * fx.fert;
        let bd = self.founder_bg.birth_death_mult();
        let rates = Rates {
            mort_mult: ((1.0 + 2.5 * (0.9 - self.health)) * (1.0 + 0.08 * (STANDARD_TECH - self.tech))).clamp(0.6, 8.0) * fx.mort,
            hazard: (self.danger + if self.hab == Hab::Hostile { 0.02 * exposed } else { 0.0 }) * fx.hazard,
            fert,
            infant: ((0.004 + 0.15 * (1.0 - self.health) + 0.01 * tech_gap + 0.1 * genetic) * fx.infant * bd).clamp(0.001, 0.4),
            maternal: ((0.000_3 + 0.02 * (1.0 - self.health) + 0.002 * tech_gap) * bd).clamp(0.000_05, 0.05),
        };
        self.census.pair_bonus = fx.pairing;
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
        let workers = self.census.workers(now) * fx.workers;
        let labour = workers * 0.25 * labour_eff * eff * fx.build * dt;
        let mut built_t = 0.0;
        if let Some(c) = self.queue.first_mut() {
            let (mat_t, lab_y) = c.kind.cost();
            let mut frac = labour / lab_y;
            let need = frac * mat_t;
            if need > self.materials {
                frac *= self.materials / need.max(1e-9);
            }
            frac = frac.min(1.0 - c.done);
            self.materials -= frac * mat_t;
            built_t = frac * mat_t;
            c.done += frac;
            if c.done >= 1.0 - 1e-9 {
                let kind = c.kind;
                self.queue.remove(0);
                match kind {
                    Project::Pmp => self.pmp += 1,
                    Project::BioDome => self.biodomes += 1,
                    Project::SpaceDock => self.dock = true,
                    Project::Business => self.business = true,
                    Project::Shipyard => self.shipyard = true,
                    Project::Ship(i) => self.hangar.push(i),
                }
                if let Project::Ship(_) = kind {
                    notes.push((Level::Good, format!("The shipyard has launched a new {}. It joins your fleet at the colony.", kind.name())));
                } else {
                    notes.push((Level::Good, format!("Construction complete: {}.", kind.name())));
                }
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
            materials_made_y: produce,
            materials_upkeep_y: upkeep,
            materials_build_y: built_t / dt,
            workers,
            rates_mort: rates.mort_mult,
            rates_fert: rates.fert,
            rates_infant: rates.infant,
            forage: fed_by_land,
        };

        // Kits waiting in store are used as soon as a couple can be made.
        if self.gene_kits > 0 {
            let before = self.gene_kits;
            self.census.use_gene_kits(now, &mut self.gene_kits);
            if self.gene_kits < before {
                let k = before - self.gene_kits;
                notes.push((Level::Good, format!("{k} human-native couple{} licensed with Genetic Modification Kits: their children will be Hybrids.", if k == 1 { "" } else { "s" })));
            }
        }
        if self.month == 0 {
            self.yearly(now, notes);
        }
    }

    fn yearly(&mut self, now: f64, notes: &mut Vec<Note>) {
        self.census.yearly(now, &mut self.gene_kits);
        self.births_total += self.census.last_year.births;
        self.deaths_total += self.census.last_year.deaths;
        let pop = self.population();
        self.history.push([now as f32, pop as f32]);
        // The plant keeps everyone equipped (up to its capacity).
        if self.pmp > 0 {
            let served = pop.min(self.pmp as f64 * 10_000.0);
            let mut made = 0;
            let before = self.materials;
            for g in Good::SUPPLIES {
                if g == Good::HazardDome && (!self.hab.needs_domes() || self.biodomes as f64 * 10_000.0 >= served) {
                    continue;
                }
                let target = self.units_for(g, served);
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
            self.plant_spent_t = before - self.materials;
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
        if !self.won && !self.outpost && pop >= WIN_POP {
            self.won = true;
            notes.push((Level::Good, format!("{} has reached a million people. You have won!", self.name)));
        }
        if self.tech < 2.0 && self.units(Good::AiCluster) == 0 {
            notes.push((Level::Warn, "With no AI clusters, the colony is losing its knowledge.".into()));
        }
    }

    /// Units of `g` the manufacturing plants keep at the dock for sale:
    /// each plant stocks enough to equip 1,000 more colonists.
    pub fn plant_stock(&self, g: Good) -> u32 {
        if self.pmp == 0 || g.supply().is_none() {
            return 0;
        }
        g.units_for(self.pmp as f64 * 1_000.0)
    }

    /// New prices and usual stock for the space dock (yearly, and when
    /// something is built). What is on the shelves stays; a new dock
    /// opens fully stocked.
    pub fn refresh_market(&mut self) {
        let mut rng = crate::rngx::derive(self.census.seed ^ 0x0d_0c4, (self.clock * 12.0) as u64);
        let k = if self.business { 2.0 } else { 1.0 };
        let fresh = self.market.is_none();
        let old = self.market.take().unwrap_or_default();
        let mut offers = BTreeMap::new();
        self.dock_usual.clear();
        for g in Good::SUPPLIES.iter().copied().chain([Good::GeneKit]) {
            let plant = self.plant_stock(g);
            // Home-made supplies skip the merchants' freight.
            let local = if plant > 0 { 0.9 } else { 1.0 };
            let price = g.base_price() * if self.business { 0.95 } else { 1.15 } * local * rng.random_range(0.9..1.15);
            let merchants = (g.units_for(self.population() * 0.05).max(2) as f64 * k * rng.random_range(0.4..1.2)).round() as u32;
            let visit = rng.random_bool(0.85);
            let usual = if visit { merchants } else { 0 } + plant;
            let stock = if fresh { usual } else { old.offers.get(&g).map_or(0, |o| o.stock) };
            offers.insert(g, Offer { price, stock, sells: usual > 0 || stock > 0, buys: false });
            self.dock_usual.insert(g, usual);
        }
        let stocked_t = if fresh { self.clock } else { old.stocked_t };
        self.market = Some(CargoMarket { offers, stocked_t });
    }

    /// Top the dock back up toward its usual stock, like any port: about
    /// two thirds of a shortfall returns within a month. The plants' share
    /// is made from the stockpile (as when they equip colonists); the
    /// merchants bring the rest.
    pub fn restock_dock(&mut self, now: f64) {
        if !self.dock {
            return;
        }
        if self.market.is_none() || self.dock_usual.is_empty() {
            self.refresh_market();
        }
        let since = self.market.as_ref().map_or(now, |m| m.stocked_t);
        let days = (now - since) * crate::sim::people::DAYS_PER_YEAR;
        if days < 1.0 {
            return;
        }
        let k = 1.0 - (-days / crate::sim::cargo::RESTOCK_DAYS).exp();
        let goods: Vec<Good> = self.dock_usual.keys().copied().collect();
        let mut changed = false;
        let mut short = false;
        for g in goods {
            let usual = self.dock_usual[&g];
            let plant = self.plant_stock(g).min(usual);
            let m = self.market.as_mut().unwrap();
            let Some(o) = m.offers.get_mut(&g) else { continue };
            if o.stock >= usual {
                continue;
            }
            let mut add = ((usual - o.stock) as f64 * k).floor() as u32;
            // The plants' part of the restock, paid in materials.
            let made = ((add as f64 * plant as f64 / usual.max(1) as f64).round() as u32).min(add);
            let cost = g.mass_t() * 1.5;
            let can = (self.materials / cost).floor() as u32;
            if made > can {
                add -= made - can;
            }
            self.materials -= made.min(can) as f64 * cost;
            if add > 0 {
                o.stock += add;
                o.sells = true;
                changed = true;
            }
            short |= o.stock < usual;
        }
        // Nothing grew (small shortfalls, short waits): keep counting from
        // the same moment so the time is not lost.
        if changed || !short {
            self.market.as_mut().unwrap().stocked_t = now;
        }
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
        let outpost = self.colony.is_some();
        if outpost {
            if !self.hegemony() {
                return Err("You already have a colony. Only under the Hegemony law (enacted at your capital) can you found more.".into());
            }
            if self.all_colonies().any(|c| c.sys_id == self.sys_id && c.loc == self.loc) {
                return Err("You already have a colony here.".into());
            }
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
        let spec = &self.ship();
        let colonists = if outpost {
            // Your family stays at the capital: only the ship's company
            // and passengers settle.
            (self.crew.count.saturating_sub(self.own_party().len()) + self.pax.count) as u32
        } else {
            let pilot = self.pilot.is_some() as usize;
            (self.crew.count - pilot + self.pax.count) as u32
        };
        if colonists < 2 {
            return Err(if outpost {
                "A further colony needs settlers: your family stays at the capital. Bring crew or passengers (an Ark carries 500).".into()
            } else {
                "A colony needs at least a couple: recruit a partner at a port first.".into()
            });
        }
        let hab = world::habitability(p);
        let hybrid = !outpost && self.spouse.as_ref().is_some_and(|s| s.species.is_some());
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
            warnings.push("Your partner is a native: without a Genetic Modification Kit you cannot have children together (bring one later and it is used for you first). Your children will be Hybrids, who have children only with other Hybrids.".into());
        }
        if outpost {
            warnings.push(format!(
                "Your family stays at {}: these {colonists} settlers found the colony without you, and the ship flies on.",
                self.colony.as_ref().map(|c| c.name.as_str()).unwrap_or("the capital")
            ));
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
        let outpost = self.colony.is_some();
        // The captain (and partner) found only the first colony.
        let player = (!outpost).then(|| census.add(Person {
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
            hybrid: false,
            licensed: false,
        }));
        if let Some(player) = player {
            census.given.insert(player, self.player_given());
            census.family.insert(player, self.player_surname());
            census.people[player as usize].line = true;
        }
        let mut spouse_id = None;
        if let (Some(player), Some(sp)) = (player, self.spouse.as_ref().filter(|_| !outpost)) {
            let id = census.add(Person {
                female: sp.sex.is_female(),
                // Near the captain's age (both adults).
                born: (now - (now - self.player_birth_y + rng.random_range(-4.0..4.0)).max(ADULT + 2.0)) as f32,
                died: None,
                mother: None,
                father: None,
                spouse: Some(player),
                alien: sp.species.is_some(),
                origin: Origin::Founder,
                due: None,
                unseen: vec![],
                parents_unseen: false,
                line: false,
                hybrid: false,
                licensed: plan.hybrid && plan.gene_kit,
            });
            census.given.insert(id, sp.name.clone());
            census.people[player as usize].spouse = Some(id);
            census.people[player as usize].licensed = plan.hybrid && plan.gene_kit;
            spouse_id = Some(id);
        }
        // The rest of the crew (not the pilot, who stays with the ship),
        // then the passengers. Sexes balance across the whole company.
        let own = self.own_party();
        let others: Vec<String> = self.crew.names.iter().filter(|n| !own.contains(n)).cloned().collect();
        let total = plan.colonists as usize;
        // Emigrants aboard (an ark from one of your colonies) found it as
        // themselves. At a further colony the whole people are kept by age
        // band, made individual only when looked at; the royal line (and
        // any natives) stay on record.
        let emigrants: Vec<Emigrant> = self.settlers.iter().take(self.pax.count).cloned().collect();
        let mut leader = None;
        for e in &emigrants {
            if !outpost || e.royal || e.alien || e.hybrid {
                let id = census.add(Person {
                    female: e.female,
                    born: e.born,
                    died: None,
                    mother: None,
                    father: None,
                    spouse: None,
                    alien: e.alien,
                    origin: Origin::Founder,
                    due: None,
                    unseen: vec![],
                    parents_unseen: false,
                    line: e.royal,
                    hybrid: e.hybrid,
                    licensed: false,
                });
                census.given.insert(id, e.given.clone());
                census.family.insert(id, e.family.clone());
                if e.royal && leader.is_none() {
                    leader = Some(id);
                }
            } else {
                let age = ((now - e.born as f64).max(0.0) as usize).min(census::MAX_AGE);
                census.cohorts[age][e.female as usize] += 1.0;
            }
        }
        let mut females = census.people.iter().filter(|p| p.female).count() + census.cohorts.iter().map(|c| c[1]).sum::<f64>() as usize;
        let mut placed = census.people.len() + census.cohort_total() as usize;
        let mut add = |census: &mut Census, given: Option<String>, rng: &mut crate::rngx::GameRng, age: f64| {
            let remaining = total.saturating_sub(placed);
            let want_f = (total / 2).saturating_sub(females);
            let female = if remaining > 0 { rng.random_range(0..remaining) < want_f } else { rng.random_bool(0.5) };
            if female {
                females += 1;
            }
            placed += 1;
            if outpost {
                census.cohorts[(age as usize).min(census::MAX_AGE)][female as usize] += 1.0;
                return;
            }
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
                hybrid: false,
                licensed: false,
            });
            if let Some(g) = given {
                census.given.insert(id, g);
            }
        };
        for n in others {
            let age = rng.random_range(22.0..46.0);
            add(&mut census, Some(n), &mut rng, age);
        }
        for _ in emigrants.len()..self.pax.count {
            let age = rng.random_range(16.0..48.0);
            add(&mut census, None, &mut rng, age);
        }
        census.yearly(now, &mut 0);
        census.last_year = Default::default();
        if outpost {
            census.demographic = true;
        }
        // A further colony's leader stands in for the captain on its
        // screens: one of the royal line if any came, else a settler.
        let player = player.or(leader).or_else(|| {
            (30..50).flat_map(|a| [(a, false), (a, true)]).find_map(|(a, f)| census.materialize(a, f, now, false))
        });
        let player = player.or_else(|| census.living.first().copied()).unwrap_or(0);

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
            gene_kits: 0,
            founders: plan.colonists,
            genetic0: (founding_genetic(n) + if plan.hybrid { 0.1 } else { 0.0 }) * self.background.genetic_mult(),
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
            laws: vec![],
            founder_bg: self.background,
            births_total: 0.0,
            deaths_total: 0.0,
            shipyard: false,
            outpost,
            plant_spent_t: 0.0,
            blueprints: vec![],
            hangar: vec![],
            drone_lvl: 0,
            boons: Boons::default(),
            census,
            player,
            history: vec![[now as f32, n as f32]],
            won: false,
            market: None,
            dock_usual: BTreeMap::new(),
            ind: Indicators::default(),
        };
        let _ = spouse_id;
        if plan.hybrid && plan.gene_kit {
            self.hold.take(Good::GeneKit, 1);
        }
        if outpost {
            colony.name = format!("{} {}", colony.name, ["II", "III", "IV", "V", "VI", "VII", "VIII", "IX"][self.outposts.len().min(7)]);
            let name = colony.name.clone();
            let n_out = self.outposts.len() + 1;
            self.outposts.push(colony);
            // Unload into it (the swap points `colony` at the new one).
            self.swap_colony(n_out);
            self.unload_to_colony();
            self.swap_colony(n_out);
            let own = self.own_party();
            self.crew.names.retain(|n| own.contains(n));
            self.crew.count = self.crew.names.len();
            self.pax.count = 0;
            self.settlers.clear();
            self.stats.colonies_founded += 1;
            self.log.push(
                self.t,
                Level::Good,
                format!("{name} is founded on {world_name} by {} settlers ({}). Your family remains at the capital; the ship flies on.", plan.colonists, hab.name().to_lowercase()),
            );
            return Ok(());
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
        self.settlers.clear();
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
        self.stats.colonies_founded += 1;
        self.log.push(self.t, Level::Good, msg);
        self.log.push(self.t, Level::Info, "Colony mode: a year passes in about a minute and a half at 1x. Leave to fly for supplies; the colony lives on without you.");
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
            } else if g == Good::GeneKit {
                self.hold.take(g, n);
                col.gene_kits += n;
                parts.push(format!("{n} × {g}"));
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
                        hybrid: false,
                        licensed: false,
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
        col.refresh_cover();
        // Kits (new, or waiting for a native) are used at once.
        let before = col.gene_kits;
        col.census.use_gene_kits(now, &mut col.gene_kits);
        if col.gene_kits < before {
            parts.push(format!("{} human-native couple{} licensed with Genetic Modification Kits", before - col.gene_kits, if before - col.gene_kits == 1 { "" } else { "s" }));
        }
        let msg = if parts.is_empty() { "Nothing to unload.".to_string() } else { format!("Unloaded {}.", parts.join(", ")) };
        self.log.push(self.t, Level::Info, msg.clone());
        msg
    }

    /// Hegemony is law at the capital.
    pub fn hegemony(&self) -> bool {
        self.colony.as_ref().is_some_and(|c| c.laws.iter().any(|e| e.law == law::Law::Hegemony))
    }

    /// The capital, then the other colonies.
    pub fn all_colonies(&self) -> impl Iterator<Item = &PlayerColony> {
        self.colony.iter().chain(self.outposts.iter())
    }

    /// Colony `i` (0 = the capital).
    pub fn colony_at(&self, i: usize) -> Option<&PlayerColony> {
        if i == 0 { self.colony.as_ref() } else { self.outposts.get(i - 1) }
    }

    pub fn colony_count(&self) -> usize {
        self.all_colonies().count()
    }

    /// Swap colony `i` into `colony` (or back: call twice), so colony
    /// actions written for the capital act on it. If the swapped-in colony
    /// was abandoned in between, the second call restores the capital.
    pub fn swap_colony(&mut self, i: usize) {
        if i == 0 || i > self.outposts.len() {
            return;
        }
        match self.colony.as_mut() {
            Some(c) => std::mem::swap(c, &mut self.outposts[i - 1]),
            None => self.colony = Some(self.outposts.remove(i - 1)),
        }
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
        if c.outpost {
            return Err(format!("Your family lives at your capital; {} is run by its settlers. Unload here, then fly on.", c.name));
        }
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
        if self.colony.as_ref().is_some_and(|c| !c.outpost) && !self.outposts.is_empty() {
            return Err("Your other colonies answer to this one: abandon them first.".into());
        }
        let Some(c) = self.colony.take() else { return Err("You have no colony.".into()) };
        self.stats.colonies_abandoned += 1;
        if !c.outpost && !self.fleet.is_empty() {
            self.log.push(self.t, Level::Warn, format!("The {} ship(s) laid up there stay behind with the colony.", self.fleet.len()));
            self.fleet.clear();
        }
        self.set_independent(&c, gal);
        if !c.outpost {
            self.in_colony = false;
        }
        self.log.push(self.t, Level::Warn, format!("You leave {}. It carries on as an independent colony of {:.0}.", c.name, c.population()));
        Ok(())
    }

    /// The colony's world now hosts it as an independent settlement.
    fn set_independent(&self, c: &PlayerColony, gal: &mut Galaxy) {
        let sys = gal.system(c.sys_id);
        if let Some(p) = sys.site_mut(c.loc) {
            p.colony = Some(c.as_independent());
        }
        sys.civ = crate::sim::worldgen::derive_civ(&sys.planets);
        sys.market = None;
        sys.refresh_cargo_market(self.seed);
    }

    /// What repealing Hegemony at the capital would do: (colonies set
    /// free, whether the captain is cast out of the capital too). None
    /// when Hegemony is not in force at the capital on screen.
    pub fn hegemony_repeal_fallout(&self) -> Option<(Vec<String>, bool)> {
        let c = self.colony.as_ref().filter(|c| !c.outpost && c.laws.iter().any(|e| e.law == law::Law::Hegemony))?;
        Some((self.outposts.iter().map(|o| o.name.clone()).collect(), c.population() >= WIN_POP))
    }

    /// Put a law in force (costs materials).
    pub fn enact_law(&mut self, l: law::Law) -> Result<(), String> {
        let Some(c) = self.colony.as_mut() else { return Err("You have no colony.".into()) };
        if c.laws.iter().any(|e| e.law == l) {
            return Err(format!("{} is already law.", l.name()));
        }
        if l == law::Law::Hegemony && c.outpost {
            return Err("Hegemony is enacted at your capital: it has no effect at a further colony.".into());
        }
        if let Some(x) = l.conflicts()
            && c.laws.iter().any(|e| e.law == x)
        {
            return Err(format!("{} cannot stand alongside {}. Repeal it first.", l.name(), x.name()));
        }
        let cost = law::Law::cost_t(c.population());
        if c.materials < cost {
            return Err(format!("Enacting {} takes {cost:.0} t of materials; the stockpile has {:.0} t.", l.name(), c.materials));
        }
        c.materials -= cost;
        c.laws.push(law::Enacted { law: l, since: c.clock });
        self.stats.laws_enacted += 1;
        self.log.push(self.t, Level::Good, format!("Law enacted: {}.", l.name()));
        Ok(())
    }

    /// Take a law off the books (not within a year of enacting it).
    /// Repealing Hegemony at the capital ends the dominion: every other
    /// colony goes its own way, and past a million the capital casts the
    /// captain out too (who flies on with the ship, colony-less).
    pub fn repeal_law(&mut self, l: law::Law, gal: &mut Galaxy) -> Result<(), String> {
        let fallout = if l == law::Law::Hegemony { self.hegemony_repeal_fallout() } else { None };
        if fallout.as_ref().is_some_and(|f| f.1) && self.ship_dormant {
            return Err("Past a million, repealing Hegemony casts you out of the capital, and you have no ship to leave in.".into());
        }
        let Some(c) = self.colony.as_mut() else { return Err("You have no colony.".into()) };
        let Some(i) = c.laws.iter().position(|e| e.law == l) else { return Err(format!("{} is not law.", l.name())) };
        let held = c.clock - c.laws[i].since;
        if held < law::MIN_TERM_Y {
            return Err(format!("{} was enacted too recently to repeal (in {:.0} months).", l.name(), ((law::MIN_TERM_Y - held) * 12.0).ceil()));
        }
        c.laws.remove(i);
        self.log.push(self.t, Level::Info, format!("Law repealed: {}.", l.name()));
        let Some((freed, cast_out)) = fallout else { return Ok(()) };
        for o in std::mem::take(&mut self.outposts) {
            self.set_independent(&o, gal);
            self.log.push(self.t, Level::Warn, format!("With Hegemony gone, {} declares independence ({:.0} people).", o.name, o.population()));
        }
        if !freed.is_empty() {
            self.stats.colonies_abandoned += freed.len() as u32;
        }
        if cast_out {
            let name = self.colony.as_ref().map(|c| c.name.clone()).unwrap_or_default();
            self.abandon_colony(gal)?;
            self.log.push(self.t, Level::Critical, format!("{name} no longer needs a sovereign: the people send you away. You fly on as a roaming captain, without a colony."));
        }
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

    /// Is the ship docked at the colony (the one in `colony`), so the
    /// dock can load straight into its hold?
    pub fn ship_at_colony(&self) -> bool {
        let Some(c) = self.colony.as_ref() else { return false };
        !self.ship_dormant && self.trip.is_none() && self.shuttle.is_none() && self.sys_id == c.sys_id && self.loc == c.loc
    }

    /// Buy supplies from the merchants at the colony's space dock, into
    /// the ship's hold (`to_ship`: refused, never redirected, unless the
    /// ship is docked at this colony) or straight into the colony.
    /// Returns the refusal, if any.
    pub fn dock_buy(&mut self, g: Good, n: u32, to_ship: bool) -> Option<String> {
        let now = years(self.t);
        if to_ship && !self.ship_at_colony() {
            let ship = self.ship().name;
            let (col, world) = self.colony.as_ref().map(|c| (c.name.clone(), c.world.clone())).unwrap_or_default();
            let msg = format!("Nothing bought: the {ship} is not docked at {col} ({world}). Fly there to load its dock's goods, or choose Deliver to: Colony.");
            self.log.push(self.t, Level::Warn, msg.clone());
            return Some(msg);
        }
        let fit = if to_ship { (self.cargo_room_t() / g.mass_t() + 1e-9).floor() as u32 } else { u32::MAX };
        let Some(c) = self.colony.as_mut() else { return Some("You have no colony.".into()) };
        c.restock_dock(now);
        let Some(m) = c.market.as_mut() else {
            self.log.push(self.t, Level::Warn, "The colony has no space dock yet.");
            return Some("The colony has no space dock yet.".into());
        };
        let Some(price) = m.buy_price(g) else {
            let msg = format!("No merchant has {g} this year.");
            self.log.push(self.t, Level::Warn, msg.clone());
            return Some(msg);
        };
        let stock = m.offers[&g].stock;
        let afford = (self.credits / price).floor() as u32;
        let k = n.min(stock).min(afford).min(fit);
        if k == 0 {
            let why = if fit == 0 {
                format!("No room in the hold for {g} ({:.1} t each).", g.mass_t())
            } else {
                format!("You cannot afford {g} ({price:.0} cr each).")
            };
            self.log.push(self.t, Level::Warn, why.clone());
            return Some(why);
        }
        if let Some(o) = m.offers.get_mut(&g) {
            o.stock -= k;
        }
        self.credits -= k as f64 * price;
        let cost = k as f64 * price;
        if to_ship {
            self.hold.add(g, k);
            let ship = self.ship().name;
            self.log.push(self.t, Level::Good, format!("The dock loads {k} × {g} aboard the {ship} for {cost:.0} cr."));
            return None;
        }
        if g == Good::GeneKit {
            c.gene_kits += k;
            c.census.use_gene_kits(now, &mut c.gene_kits);
        } else {
            c.add_supply(g, k, now);
            c.refresh_cover();
        }
        self.log.push(self.t, Level::Good, format!("The dock delivers {k} × {g} into {}'s stores for {cost:.0} cr.", c.name));
        None
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
            let built = |c: &PlayerColony| c.pmp + c.biodomes + c.dock as u32 + c.business as u32 + c.shipyard as u32;
            let (b0, d0, n0) = (c.births_total, c.deaths_total, built(c));
            c.advance(now, home, boost, &mut notes);
            self.stats.colony_births += c.births_total - b0;
            self.stats.colony_deaths += c.deaths_total - d0;
            self.stats.buildings += built(c).saturating_sub(n0);
            self.stats.colony_peak = self.stats.colony_peak.max(c.population());
        }
        for (lvl, msg) in notes {
            self.log.push(self.t, lvl, msg);
        }
        // The other colonies live on by themselves.
        for c in self.outposts.iter_mut() {
            let mut notes = Vec::new();
            let built = |c: &PlayerColony| c.pmp + c.biodomes + c.dock as u32 + c.business as u32 + c.shipyard as u32;
            let (b0, d0, n0) = (c.births_total, c.deaths_total, built(c));
            c.advance(now, false, boost, &mut notes);
            self.stats.colony_births += c.births_total - b0;
            self.stats.colony_deaths += c.deaths_total - d0;
            self.stats.buildings += built(c).saturating_sub(n0);
            for (lvl, msg) in notes {
                self.log.push(self.t, lvl, format!("{}: {msg}", c.name));
            }
        }
        // Launched ships join the fleet. An ark takes on settlers from the
        // colony that built it.
        let mut launched: Vec<(usize, usize)> = Vec::new();
        if let Some(c) = self.colony.as_mut() {
            launched.extend(std::mem::take(&mut c.hangar).into_iter().map(|s| (0, s)));
        }
        for (k, c) in self.outposts.iter_mut().enumerate() {
            launched.extend(std::mem::take(&mut c.hangar).into_iter().map(|s| (k + 1, s)));
        }
        for (from, idx) in launched {
            let b = if idx == crate::sim::shipyard::ARK {
                let pax = crate::sim::ShipSpec::spec(idx).pax;
                let src = if from == 0 { self.colony.as_mut() } else { self.outposts.get_mut(from - 1) };
                let (name, people) = match src {
                    Some(c) => (c.name.clone(), c.ark_settlers(pax, now)),
                    None => (String::new(), vec![]),
                };
                let royal = people.iter().filter(|e| e.royal).count();
                let n = people.len();
                let b = self.launched_ark(people);
                let kin = if royal > 0 { format!(", {royal} of them of the royal line") } else { String::new() };
                self.log.push(self.t, Level::Good, format!("An ark is launched at {name}: {n} of its people board as settlers{kin}. It waits with the fleet."));
                b
            } else {
                self.new_berthed(idx)
            };
            self.fleet.push(b);
            self.stats.ships_built += 1;
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
        let emigrated = c.census.emigrated.clone();
        let adult = |p: &Person, id: u32| p.alive() && !emigrated.contains_key(&id) && !p.alien && (22.0..55.0).contains(&p.age(now));
        let pick = line
            .iter()
            .copied()
            .filter(|&i| adult(&c.census.people[i as usize], i))
            .max_by(|a, b| c.census.people[*a as usize].born.total_cmp(&c.census.people[*b as usize].born).reverse())
            .or_else(|| c.census.living.iter().copied().filter(|&i| i != old && adult(&c.census.people[i as usize], i)).max_by_key(|&i| (c.census.people[i as usize].born * 10.0) as i64))
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
        let (new_given, new_surname) = (c.census.given_name(new), c.census.family_name(new));
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
        self.given_name = new_given;
        self.surname = new_surname;
        self.stats.successions += 1;
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

    /// The space dock restocks over time like any port, its plants'
    /// share made from the stockpile; purchases can go into the ship.
    #[test]
    fn the_dock_restocks_and_loads_the_ship() {
        let (mut sim, mut gal) = at_world(2, Hab::Habitable);
        sim.found_colony(&mut gal).unwrap();
        let unload = sim.hold.mass_t();
        let c = sim.colony.as_mut().unwrap();
        c.dock = true;
        c.pmp = 5;
        c.materials = 10_000.0;
        c.refresh_market();
        let g = Good::MedKit;
        let usual = c.dock_usual[&g];
        assert!(usual >= c.plant_stock(g) && c.plant_stock(g) == 500, "plants stock 5,000 colonists' med kits");
        assert_eq!(c.market.as_ref().unwrap().offers[&g].stock, usual, "a new dock opens full");

        // Buy it out into the ship's hold (docked at the colony).
        sim.credits = 1e9;
        assert!(sim.ship_at_colony());
        let before = sim.colony.as_ref().unwrap().units(g);
        sim.dock_buy(g, usual, true);
        assert_eq!(sim.hold.get(g), usual.min(((sim.ship().cargo_t - unload) / g.mass_t()) as u32));
        assert_eq!(sim.colony.as_ref().unwrap().units(g), before, "nothing went to the colony");
        let left = sim.colony.as_ref().unwrap().market.as_ref().unwrap().offers[&g].stock;

        // A month later about two thirds is back, the plants' part paid
        // for in materials.
        let c = sim.colony.as_mut().unwrap();
        let (now, mat) = (c.clock, c.materials);
        c.restock_dock(now + 1.0 / 12.0);
        let stock = c.market.as_ref().unwrap().offers[&g].stock;
        assert!(stock > left + (usual - left) / 2, "restocked {left} -> {stock} of {usual}");
        assert!(c.materials < mat, "the plants used materials");

        // Away from the colony, buying for the ship is refused outright:
        // nothing is bought, and nothing goes to the colony instead.
        sim.loc = Loc::Star;
        assert!(!sim.ship_at_colony());
        let (held, at_col, credits) = (sim.hold.get(g), sim.colony.as_ref().unwrap().units(g), sim.credits);
        let err = sim.dock_buy(g, 1, true).expect("refused");
        assert!(err.contains("not docked"), "{err}");
        assert_eq!(sim.hold.get(g), held);
        assert_eq!(sim.colony.as_ref().unwrap().units(g), at_col, "not sent to the colony");
        assert_eq!(sim.credits, credits, "nothing paid");
        // Only an explicit choice delivers to the colony.
        assert!(sim.dock_buy(g, 1, false).is_none());
        assert_eq!(sim.colony.as_ref().unwrap().units(g), at_col + 1);
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
        // Med kits last five years (no plant to replace them yet).
        sim.colony.as_mut().unwrap().auto_build = false;
        run_years(&mut sim, &mut gal, 4.0);
        assert!(sim.colony.as_ref().unwrap().units(Good::MedKit) > 0, "still good after four years");
        run_years(&mut sim, &mut gal, 1.5);
        assert_eq!(sim.colony.as_ref().unwrap().units(Good::MedKit), 0);
        let c = sim.colony.as_mut().unwrap();
        c.pmp = 1;
        c.materials = 10_000.0;
        run_years(&mut sim, &mut gal, 1.1);
        assert!(sim.colony.as_ref().unwrap().units(Good::MedKit) > 0);
    }

    fn captain(bg: crate::sim::background::Background) -> crate::sim::background::Captain {
        crate::sim::background::Captain { given: "Ada".into(), surname: "Vale".into(), sex: crate::sim::Sex::Male, background: bg }
    }

    #[test]
    fn backgrounds_give_money_and_bonuses() {
        use crate::sim::background::Background as B;
        for b in B::ALL {
            let (mut sim, _gal) = at_world(1, Hab::Habitable);
            let base = sim.credits;
            sim.apply_captain(&captain(b));
            assert_eq!(sim.credits, base + b.money(), "{b:?}");
        }
        let (mut sim, _) = at_world(1, Hab::Habitable);
        let cargo = sim.ship().cargo_t;
        sim.apply_captain(&captain(B::MerchantFamily));
        assert!((sim.ship().cargo_t - cargo * 1.2).abs() < 1e-9);
        let (mut sim, _) = at_world(1, Hab::Habitable);
        sim.apply_captain(&captain(B::NavalAcademy));
        assert_eq!((sim.attack_lvl, sim.defense_lvl), (1, 1));
        let (mut sim, _) = at_world(1, Hab::Habitable);
        let y = sim.ship().drone_yield;
        sim.apply_captain(&captain(B::AsteroidMiner));
        assert!(sim.ship().drone_yield > y);
        let (mut sim, mut gal) = at_world(1, Hab::Habitable);
        sim.apply_captain(&captain(B::ColonialSurveyor));
        sim.found_colony(&mut gal).unwrap();
        assert!((sim.colony.as_ref().unwrap().genetic0 - founding_genetic(10.0) * 0.6).abs() < 1e-9);
    }

    #[test]
    fn the_captains_surname_runs_in_the_colony() {
        let (mut sim, mut gal) = at_world(1, Hab::Habitable);
        sim.apply_captain(&captain(Default::default()));
        sim.found_colony(&mut gal).unwrap();
        let c = sim.colony.as_ref().unwrap();
        assert_eq!(c.census.name(c.player), "Ada Vale");
        // A child of the captain's line carries the name.
        let mut census = c.census.clone();
        let kid = census.add(census::Person {
            female: true,
            born: 1.0,
            died: None,
            mother: census.people[c.player as usize].spouse,
            father: Some(c.player),
            spouse: None,
            alien: false,
            origin: Origin::Born,
            due: None,
            unseen: vec![],
            parents_unseen: false,
            line: true,
            hybrid: false,
            licensed: false,
        });
        assert_eq!(census.family_name(kid), "Vale");
    }

    #[test]
    fn laws_cost_conflict_and_change_the_colony() {
        use law::Law;
        let (mut sim, mut gal) = at_world(2, Hab::Habitable);
        sim.found_colony(&mut gal).unwrap();
        sim.colony.as_mut().unwrap().materials = 0.0;
        assert!(sim.enact_law(Law::BeFruitful).is_err(), "no materials");
        sim.colony.as_mut().unwrap().materials = 100.0;
        sim.enact_law(Law::BeFruitful).unwrap();
        assert!(sim.colony.as_ref().unwrap().materials < 100.0);
        assert!(sim.enact_law(Law::FamilyPlanning).is_err(), "exclusive");
        assert!(sim.repeal_law(Law::BeFruitful, &mut gal).is_err(), "too soon");
        run_years(&mut sim, &mut gal, 1.1);
        sim.repeal_law(Law::BeFruitful, &mut gal).unwrap();
        assert_eq!(sim.stats.laws_enacted, 1);
        // Fertility follows the law.
        let fert = |laws: &[Law]| {
            let (mut sim, mut gal) = at_world(2, Hab::Habitable);
            sim.found_colony(&mut gal).unwrap();
            let c = sim.colony.as_mut().unwrap();
            c.laws = laws.iter().map(|&l| law::Enacted { law: l, since: c.clock }).collect();
            run_years(&mut sim, &mut gal, 0.5);
            sim.colony.as_ref().unwrap().ind.rates_fert
        };
        let (none, more, fewer) = (fert(&[]), fert(&[Law::BeFruitful]), fert(&[Law::FamilyPlanning]));
        assert!(more > none * 1.2 && fewer < none * 0.8, "{fewer} {none} {more}");
    }

    #[test]
    fn statistics_count_the_colony_and_the_worlds() {
        let (mut sim, mut gal) = at_world(2, Hab::Habitable);
        assert!(sim.stats.found.contains_key("Star classes"), "the home system is surveyed");
        assert!(sim.stats.extremes.contains_key("Hottest world"));
        sim.found_colony(&mut gal).unwrap();
        run_years(&mut sim, &mut gal, 3.0);
        assert_eq!(sim.stats.colonies_founded, 1);
        assert!(sim.stats.colony_births > 0.0);
        assert!(sim.stats.colony_peak >= 510.0);
    }

    #[test]
    fn founding_tech_and_genetics_scale_with_founders() {
        assert!((founding_tech(510.0) - STANDARD_TECH).abs() < 0.01);
        assert!(founding_tech(10.0) < 3.6 && founding_tech(2.0) <= 2.0 + 1e-9);
        assert!(founding_genetic(2.0) > founding_genetic(10.0));
        assert_eq!(founding_genetic(500.0), 0.0);
    }

    /// Two settlers on a kind world, with nothing but each other: big
    /// frontier families and what the land gives should carry most such
    /// colonies past the first generations.
    #[test]
    fn a_lone_couple_can_take_root_on_a_kind_world() {
        let n = 20;
        let mut grew = 0;
        for seed in 0..n {
            let mut gal = Galaxy::new(1000 + seed);
            let mut sim = Sim::new_game(1000 + seed, 0, &mut gal);
            let home = gal.home_id();
            let idx = gal.system(home).planets.iter().position(|p| world::colonizable(p).is_ok()).expect("a rocky world");
            {
                let p = &mut gal.system(home).planets[idx];
                p.gravity = 1.0;
                p.life = None;
                p.atmosphere = Some(AtmoKind::Breathable);
                p.temp_band = TempBand::Temperate;
            }
            sim.loc = Loc::Planet(idx);
            let who = crate::sim::state::Recruit { name: "Pat".into(), sex: sim.player_sex.opposite(), species: None, skill: 0 };
            sim.crew.names.push(who.name.clone());
            sim.crew.count += 1;
            sim.spouse = Some(who);
            sim.found_colony(&mut gal).expect("founded");
            sim.retire_age = 200.0;
            run_years(&mut sim, &mut gal, 75.0);
            if sim.colony.as_ref().unwrap().population() >= 4.0 {
                grew += 1;
            }
        }
        assert!(grew * 2 >= n, "only {grew}/{n} couples took root");
    }
}

#[cfg(test)]
mod medkit_unload_tests {
    use super::*;

    /// The path a player's kits take: bought into the hold, unloaded at the
    /// colony years after founding. They must last five years from delivery.
    #[test]
    fn unloaded_med_kits_last_five_years() {
        let mut gal = Galaxy::new(42);
        let mut sim = Sim::new_game(42, 2, &mut gal);
        let home = gal.home_id();
        let idx = gal.system(home).planets.iter().position(|p| world::colonizable(p).is_ok()).unwrap();
        sim.shuttle = None;
        sim.loc = Loc::Planet(idx);
        sim.found_colony(&mut gal).unwrap();
        sim.colony.as_mut().unwrap().auto_build = false;
        sim.colony.as_mut().unwrap().supplies.remove(&Good::MedKit);
        // Years later the ship comes back with fresh kits.
        sim.advance(3.0 * HOURS_PER_YEAR, &mut gal);
        sim.in_colony = false;
        sim.hold.add(Good::MedKit, 15);
        let now = years(sim.t);
        sim.unload_to_colony();
        let b = &sim.colony.as_ref().unwrap().supplies[&Good::MedKit];
        assert_eq!(b.len(), 1);
        assert!((b[0].until - (now + 5.0)).abs() < 1e-9, "until {} now {now}", b[0].until);
        sim.in_colony = true;
        sim.advance(2.0 * HOURS_PER_YEAR, &mut gal);
        assert_eq!(sim.colony.as_ref().unwrap().units(Good::MedKit), 15, "still there two years on");
    }

    /// Spare kits in store do not wear: only those the colony needs age.
    #[test]
    fn spare_supplies_keep_in_store() {
        let mut b = vec![Batch { units: 15, until: 5.0 }];
        // Fifteen colonists need two kits; thirteen sit idle.
        for _ in 0..60 {
            idle_keep(&mut b, 2, MONTH);
        }
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].units, 2);
        assert!((b[0].until - 5.0).abs() < 1e-9);
        assert_eq!(b[1].units, 13);
        assert!((b[1].until - 10.0).abs() < 1e-6, "idle kits kept: {}", b[1].until);
        // Nothing needed: nothing ages.
        let mut c = vec![Batch { units: 4, until: 3.0 }];
        idle_keep(&mut c, 0, 1.0);
        assert!((c[0].until - 4.0).abs() < 1e-9);
        // All needed: all age.
        idle_keep(&mut c, 9, 1.0);
        assert_eq!(c.len(), 1);
        assert!((c[0].until - 4.0).abs() < 1e-9);
    }
}

#[cfg(test)]
mod materials_display_tests {
    use super::*;

    /// The net the colony screens show is what really happens to the
    /// stockpile, construction included.
    #[test]
    fn the_shown_materials_trend_matches_the_stockpile() {
        let mut gal = Galaxy::new(42);
        let mut sim = Sim::new_game(42, 2, &mut gal);
        let home = gal.home_id();
        let idx = gal.system(home).planets.iter().position(|p| world::colonizable(p).is_ok()).unwrap();
        sim.shuttle = None;
        sim.loc = Loc::Planet(idx);
        sim.found_colony(&mut gal).unwrap();
        let c = sim.colony.as_mut().unwrap();
        c.auto_build = false;
        c.materials = 50_000.0;
        c.queue.push(Construction { kind: Project::Business, done: 0.0 });
        // Settle one month in, then compare the next month's change.
        sim.advance(HOURS_PER_YEAR / 12.0 + 1.0, &mut gal);
        let c = sim.colony.as_ref().unwrap();
        assert!(c.ind.materials_build_y > 0.0, "construction is drawing");
        let (m0, clock0) = (c.materials, c.clock);
        sim.advance(HOURS_PER_YEAR / 12.0, &mut gal);
        let c = sim.colony.as_ref().unwrap();
        let shown = c.ind.materials_made_y - c.ind.materials_upkeep_y - c.ind.materials_build_y;
        let real = (c.materials - m0) / (c.clock - clock0);
        assert!((real - shown).abs() <= shown.abs() * 0.02 + 1.0, "shown {shown:.1} t/yr, real {real:.1} t/yr");
    }
}

#[cfg(test)]
mod hegemony_colony_tests {
    use super::*;
    use crate::sim::civ::{Civ, CivKind, CivTier};
    use crate::sim::shipyard::{Purchase, ARK};

    /// A capital founded by the ark, and a second rocky world to settle.
    fn setup() -> (Sim, Galaxy, usize) {
        let mut gal = Galaxy::new(42);
        let mut sim = Sim::new_game(42, 2, &mut gal);
        let home = gal.home_id();
        let sys = gal.system(home);
        let idx = sys.planets.iter().position(|p| world::colonizable(p).is_ok()).unwrap();
        let other = (0..sys.planets.len()).find(|&j| j != idx).unwrap();
        let au = sys.planets[other].au;
        sys.planets[other] = sys.planets[idx].clone();
        sys.planets[other].au = au;
        sys.civ = Some(Civ { tier: CivTier::T3, name: "Test".into(), kind: CivKind::Colony, seat: Loc::Star, small: false });
        sim.shuttle = None;
        sim.loc = Loc::Planet(idx);
        sim.found_colony(&mut gal).unwrap();
        (sim, gal, other)
    }

    /// A capital under Hegemony with one further colony founded by ark.
    fn hegemony_with_outpost() -> (Sim, Galaxy) {
        let (mut sim, mut gal, other) = setup();
        sim.leave_colony().unwrap();
        sim.credits = 1e6;
        sim.colony.as_mut().unwrap().laws.push(law::Enacted { law: law::Law::Hegemony, since: -10.0 });
        sim.loc = Loc::Planet(other);
        sim.buy_ship(&gal, ARK, Purchase::TradeIn).unwrap();
        sim.found_colony(&mut gal).unwrap();
        assert_eq!(sim.outposts.len(), 1);
        (sim, gal)
    }

    /// Repealing Hegemony frees every other colony; the captain keeps the
    /// capital below a million.
    #[test]
    fn repealing_hegemony_frees_the_other_colonies() {
        let (mut sim, mut gal) = hegemony_with_outpost();
        let (freed, cast_out) = sim.hegemony_repeal_fallout().unwrap();
        assert_eq!(freed.len(), 1);
        assert!(!cast_out);
        let (sys, loc) = (sim.outposts[0].sys_id, sim.outposts[0].loc);
        sim.repeal_law(law::Law::Hegemony, &mut gal).unwrap();
        assert!(sim.outposts.is_empty(), "set free");
        assert!(gal.system(sys).site(loc).unwrap().colony.is_some(), "an independent colony on its world");
        assert!(sim.colony.as_ref().is_some_and(|c| !c.outpost), "the capital is kept");
        assert!(!sim.hegemony());
    }

    /// Past a million the capital casts the captain out too: they fly on
    /// with the ship, without a colony. Without a ship, no repeal.
    #[test]
    fn repealing_hegemony_past_a_million_casts_the_captain_out() {
        let (mut sim, mut gal) = hegemony_with_outpost();
        sim.colony.as_mut().unwrap().census.cohorts[30][0] += WIN_POP;
        assert!(sim.hegemony_repeal_fallout().unwrap().1);
        sim.ship_dormant = true;
        assert!(sim.repeal_law(law::Law::Hegemony, &mut gal).is_err(), "no ship to leave in");
        assert!(sim.hegemony() && sim.outposts.len() == 1, "nothing changed");
        sim.ship_dormant = false;
        let (sys, loc) = { let c = sim.colony.as_ref().unwrap(); (c.sys_id, c.loc) };
        sim.repeal_law(law::Law::Hegemony, &mut gal).unwrap();
        assert!(sim.colony.is_none() && sim.outposts.is_empty(), "no colonies left");
        assert!(!sim.in_colony);
        assert!(gal.system(sys).site(loc).unwrap().colony.is_some(), "the capital goes on, independent");
        // A roaming captain may found again.
        assert!(sim.founding_plan(&gal).map_or_else(|e| !e.contains("already have a colony"), |_| true));
    }

    fn royal(c: &mut PlayerColony, age: f64, father: Option<u32>, now: f64) -> u32 {
        c.census.add(Person {
            female: false,
            born: (now - age) as f32,
            died: None,
            mother: None,
            father,
            spouse: None,
            alien: false,
            origin: Origin::Born,
            due: None,
            unseen: vec![],
            parents_unseen: false,
            line: true,
            hybrid: false,
            licensed: false,
        })
    }

    /// An ark built at a colony carries that colony's own people: a share
    /// of the royal line present, never the heirs. They found the next
    /// colony as themselves, kept by age band but for the royal line.
    #[test]
    fn a_built_ark_carries_the_colonys_people_and_some_royals() {
        let (mut sim, mut gal, other) = setup();
        let now = years(sim.t);
        let c = sim.colony.as_mut().unwrap();
        c.laws.push(law::Enacted { law: law::Law::Hegemony, since: 0.0 });
        let player = c.player;
        let heir = royal(c, 30.0, Some(player), now);
        let cousins: Vec<u32> = (0..8).map(|k| royal(c, 25.0 + k as f64, None, now)).collect();
        c.hangar.push(ARK);
        let pop0 = c.population();
        sim.colony_tick(&mut gal);
        let c = sim.colony.as_ref().unwrap();
        let ark = sim.fleet.last().unwrap();
        let n = ark.settlers.len();
        assert!(n >= 300 && ark.pax.count == n, "{n} settlers");
        assert!((c.population() - (pop0 - n as f64)).abs() < 1.0, "they left the capital");
        assert!(c.census.here(player) && c.census.here(heir), "the captain and heirs stay");
        let royals: Vec<_> = ark.settlers.iter().filter(|e| e.royal).collect();
        assert_eq!(royals.len(), 2, "a quarter of the 8 royal cousins");
        assert_eq!(cousins.iter().filter(|&&i| c.census.emigrated.contains_key(&i)).count(), 2);
        let royal_name = royals[0].family.clone();
        // Board it, fly to the other world, and found the colony.
        let slot = sim.fleet.len() - 1;
        sim.switch_ship(slot).unwrap();
        sim.leave_colony().unwrap();
        sim.loc = Loc::Planet(other);
        sim.found_colony(&mut gal).unwrap();
        let out = &sim.outposts[0];
        assert!(out.census.demographic, "kept by age band");
        assert_eq!(out.census.living.len(), 2, "only the royal settlers are records");
        assert!(out.census.people[out.player as usize].line, "a royal leads it");
        assert_eq!(out.census.family_name(out.player), royal_name);
        assert!(out.population() >= n as f64, "everyone arrived");
        assert!(sim.settlers.is_empty() && sim.pax.count == 0);
    }

    #[test]
    fn hegemony_founds_more_colonies_with_settlers_not_family() {
        let (mut sim, mut gal, other) = setup();
        let capital_pop = sim.colony.as_ref().unwrap().population();
        sim.leave_colony().unwrap();
        sim.credits = 1e6;
        // Without Hegemony: one colony, and no ark for sale.
        assert!(!sim.ships_for_sale(&gal).contains(&ARK));
        sim.loc = Loc::Planet(other);
        assert!(sim.founding_plan(&gal).is_err());
        sim.colony.as_mut().unwrap().laws.push(law::Enacted { law: law::Law::Hegemony, since: 0.0 });
        // The captain alone has no settlers to found it with.
        let e = sim.founding_plan(&gal).unwrap_err();
        assert!(e.contains("settlers"), "{e}");
        // Buy an ark: it comes with 500 settlers.
        assert!(sim.ships_for_sale(&gal).contains(&ARK));
        sim.buy_ship(&gal, ARK, Purchase::TradeIn).unwrap();
        assert_eq!(sim.pax.count, 500);
        let plan = sim.founding_plan(&gal).unwrap();
        assert_eq!(plan.colonists, 500 + 9, "crew and passengers, not the captain");
        let captain = sim.player_name().to_string();
        // Supplies in the ark's hold go to the new colony, not the capital.
        sim.hold.add(Good::HydroKit, 3);
        sim.hold.add(Good::MedKit, 7);
        sim.hold.add(Good::HazardDome, 4);
        let cap_hydro = sim.colony.as_ref().unwrap().units(Good::HydroKit);
        sim.found_colony(&mut gal).unwrap();
        assert_eq!(sim.hold.get(Good::HydroKit) + sim.hold.get(Good::MedKit), 0, "hold unloaded");
        assert_eq!(sim.outposts[0].units(Good::HydroKit), 3);
        assert_eq!(sim.outposts[0].units(Good::MedKit), 7);
        assert_eq!(sim.colony.as_ref().unwrap().units(Good::HydroKit), cap_hydro);
        // Its screens show what was unloaded at once, not after a month.
        let ind = &sim.outposts[0].ind;
        assert!(ind.power_eff > 0.0 && ind.sheltered > 0.0 && ind.coverage[&Good::HydroKit] > 0.0, "{ind:?}");
        // A captive and two kits unloaded there: one kit marries the
        // captive to a settler at once, the other waits in store.
        sim.swap_colony(1);
        sim.hold.add(Good::Slaves, 1);
        sim.hold.add(Good::GeneKit, 2);
        let msg = sim.unload_to_colony();
        let c = sim.colony.as_ref().unwrap();
        assert!(c.outpost);
        assert_eq!(c.gene_kits, 1, "{msg}");
        let licensed: Vec<&Person> = c.census.living.iter().map(|&i| &c.census.people[i as usize]).filter(|p| p.licensed).collect();
        assert_eq!(licensed.len(), 2);
        assert!(licensed.iter().any(|p| p.alien) && licensed.iter().any(|p| !p.alien));
        sim.swap_colony(1);
        assert_eq!(sim.outposts.len(), 1);
        assert!(!sim.in_colony, "the captain flies on");
        assert_eq!(sim.crew.names, vec![captain], "only the captain stays aboard");
        assert_eq!(sim.pax.count, 0);
        let out = &sim.outposts[0];
        assert!(out.outpost && out.population() >= 500.0);
        assert_eq!(sim.colony.as_ref().unwrap().population(), capital_pop, "the family stays at the capital");
        // Both colonies live on.
        let (c0, o0) = (sim.colony.as_ref().unwrap().clock, sim.outposts[0].clock);
        sim.advance(HOURS_PER_YEAR * 0.5, &mut gal);
        assert!(sim.colony.as_ref().unwrap().clock > c0 && sim.outposts[0].clock > o0);
        // Actions on the outpost go through the swap; it cannot be "entered".
        sim.swap_colony(1);
        assert!(sim.colony.as_ref().unwrap().outpost);
        sim.colony.as_mut().unwrap().materials = 1e9;
        assert!(sim.enact_law(law::Law::Hegemony).is_err(), "Hegemony only at the capital");
        assert!(sim.enter_colony().is_err());
        sim.swap_colony(1);
        assert!(!sim.colony.as_ref().unwrap().outpost, "capital restored");
        // The capital cannot be abandoned while it has colonies.
        assert!(sim.abandon_colony(&mut gal).is_err());
        // Abandoning the outpost (swapped in) restores the capital.
        sim.swap_colony(1);
        sim.abandon_colony(&mut gal).unwrap();
        sim.swap_colony(1);
        assert!(sim.outposts.is_empty());
        assert!(sim.colony.as_ref().is_some_and(|c| !c.outpost));
    }
}
