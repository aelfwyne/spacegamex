//! Ships and the fleet: buying hulls at interstellar shipyards (trading
//! the old one in, or keeping it in a fleet at the colony), blueprints for
//! the colony's own shipyard, and switching between laid-up ships.
//!
//! Every ship keeps its own stores, cargo, hull, crew and fittings. Only
//! the captain, their partner and their pilot move between ships.

use super::cargo::Hold;
use super::civ::CivTier;
use super::galaxy::Galaxy;
use super::logbuf::Level;
use super::res::{Inventory, Res};
use super::ship::{ShipSpec, SHIPS, STARTERS};
use super::state::{CrewInfo, Sim};
use crate::rngx::{derive, mix64, name};
use serde::{Deserialize, Serialize};

/// A ship laid up at the colony.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Berthed {
    pub ship_idx: usize,
    pub inv: Inventory,
    pub hold: Hold,
    pub hull: f64,
    pub ftl_wear: f64,
    /// The crew who belong to this ship (not the captain, partner or pilot).
    pub crew: Vec<String>,
    pub pax: CrewInfo,
    pub attack_lvl: u8,
    pub defense_lvl: u8,
    /// Mining-drone refits (see `Sim::drone_lvl`).
    #[serde(default)]
    pub drone_lvl: u8,
    /// New Ship Morale Boost still pending on these stores.
    #[serde(default)]
    pub new_ship_boost: Vec<Res>,
    /// Who its passengers are (emigrants from a colony).
    #[serde(default)]
    pub settlers: Vec<super::colony::census::Emigrant>,
}

impl Berthed {
    pub fn spec(&self) -> &'static ShipSpec {
        ShipSpec::spec(self.ship_idx)
    }
}

/// How a bought ship is paid for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purchase {
    /// Hand the current ship over against the price, and fly the new one.
    TradeIn,
    /// Pay in full; the new ship is ferried to the colony's fleet.
    Fleet,
}

/// The colony ark's index in `SHIPS`.
pub const ARK: usize = 2;

/// Settlers who sign on with an ark bought at a port.
pub fn settlers(spec: &ShipSpec) -> CrewInfo {
    CrewInfo { count: spec.pax, names: vec![], health: 100.0, morale: 70.0 }
}

/// Hours of life support (and reactor fuel) a new ship is delivered
/// with: enough to board it safely, far too little to be worth selling.
pub const RESERVE_H: f64 = 72.0;

/// A new ship's survival reserve for `people` aboard.
fn reserve(inv: &mut Inventory, spec: &ShipSpec, people: usize) {
    use super::state::{FOOD_PER_P_H, O2_PER_P_H, WATER_PER_P_H};
    let people = people as f64;
    for (r, per_h) in [(Res::Oxygen, O2_PER_P_H), (Res::Water, WATER_PER_P_H), (Res::Food, FOOD_PER_P_H)] {
        inv.add(r, people * per_h * RESERVE_H);
    }
    inv.add(Res::H2, spec.cruise_burn() * RESERVE_H);
}

/// The stores the New Ship Morale Boost covers: until each first passes
/// the morale line, running it low does not worry the crew.
pub fn new_ship_boost() -> Vec<Res> {
    vec![Res::Oxygen, Res::Water, Res::Food]
}

impl Sim {
    /// A hull's numbers as this captain would fly it (background applied).
    pub fn spec_for(&self, idx: usize) -> ShipSpec {
        let mut s = *ShipSpec::spec(idx);
        self.background.apply(&mut s);
        s
    }

    /// What a shipyard pays for the current ship, by condition.
    pub fn trade_in_value(&self) -> f64 {
        if self.ship_dormant {
            return 0.0;
        }
        let spec = self.ship();
        let hull = (self.hull / spec.hull.max(1.0)).clamp(0.0, 1.0);
        let wear = 1.0 - (self.ftl_wear / 200.0).clamp(0.0, 0.5);
        spec.trade_in() * hull * wear
    }

    /// Ships on sale at this port: every interstellar yard sells the
    /// Company ship; each stocks its own selection of the others.
    pub fn ships_for_sale(&self, gal: &Galaxy) -> Vec<usize> {
        let tier = gal.system_ref(self.sys_id).civ.as_ref().map(|c| c.tier);
        if tier != Some(CivTier::T3) || self.trip.is_some() {
            return vec![];
        }
        let hegemony = self.hegemony();
        (0..SHIPS.len())
            .filter(|&i| {
                // Under Hegemony every interstellar yard will sell you an
                // ark, settlers and all.
                (i == ARK && hegemony) || (SHIPS[i].for_sale && (i == 1 || mix64(self.sys_id ^ (i as u64).wrapping_mul(0x9e37_79b9)) % 10 < 6))
            })
            .collect()
    }

    /// Why the current company and hold could not move into hull `idx`.
    pub fn can_crew(&self, idx: usize) -> Result<(), String> {
        let spec = self.spec_for(idx);
        let people = self.crew.count + self.pax.count;
        if people > spec.berths() {
            return Err(format!("It has berths for {}; you have {people} aboard.", spec.berths()));
        }
        if self.hold.mass_t() > spec.cargo_t + 1e-9 {
            return Err(format!("It holds {:.0} t; your cargo is {:.0} t. Sell or unload some first.", spec.cargo_t, self.hold.mass_t()));
        }
        Ok(())
    }

    /// What buying hull `idx` here costs, or why not.
    pub fn ship_quote(&self, gal: &Galaxy, idx: usize, how: Purchase) -> Result<f64, String> {
        if !self.ships_for_sale(gal).contains(&idx) {
            return Err("Not on sale here.".into());
        }
        if self.shuttle.is_some() || self.encounter.is_some() {
            return Err("Hold still at the port first.".into());
        }
        let list = SHIPS[idx].price * self.background.buy_mult();
        let price = match how {
            Purchase::TradeIn => {
                self.can_crew(idx)?;
                (list - self.trade_in_value()).max(0.0)
            }
            Purchase::Fleet => {
                if self.colony.is_none() {
                    return Err("A fleet needs a colony to lay its ships up at.".into());
                }
                list
            }
        };
        if self.credits < price {
            return Err(format!("Costs {price:.0} cr; you have {:.0} cr.", self.credits));
        }
        Ok(price)
    }

    /// Buy hull `idx` at this port.
    pub fn buy_ship(&mut self, gal: &Galaxy, idx: usize, how: Purchase) -> Result<(), String> {
        let price = self.ship_quote(gal, idx, how)?;
        let new = SHIPS[idx].name;
        self.credits -= price;
        self.stats.ships_bought += 1;
        match how {
            Purchase::TradeIn => {
                let old = self.ship().name;
                let tradein = self.trade_in_value();
                let lost = self.trade_into(idx);
                self.log.push(self.t, Level::Good, format!("You trade in the {old} ({tradein:.0} cr) and take command of the {new} for {price:.0} cr."));
                if !lost.is_empty() {
                    self.log.push(self.t, Level::Warn, format!("The new tanks could not take everything: {lost}."));
                }
            }
            Purchase::Fleet => {
                let b = self.new_berthed_with(idx, idx == ARK);
                self.fleet.push(b);
                let at = self.colony.as_ref().map(|c| c.world.clone()).unwrap_or_default();
                self.log.push(self.t, Level::Good, format!("You buy the {new} for {price:.0} cr. A delivery crew ferries it to {at}, where it joins your fleet."));
            }
        }
        Ok(())
    }

    /// Buy plans for hull `idx` for the colony's shipyard.
    pub fn buy_blueprint(&mut self, idx: usize) -> Result<(), String> {
        let Some(c) = self.colony.as_ref() else { return Err("You have no colony.".into()) };
        if !c.shipyard {
            return Err("Blueprints need a shipyard to build from.".into());
        }
        if !(STARTERS..SHIPS.len()).contains(&idx) {
            return Err("The shipyard already knows that design.".into());
        }
        if c.blueprints.contains(&idx) {
            return Err("You already own those blueprints.".into());
        }
        let price = SHIPS[idx].blueprint_price() * self.background.buy_mult();
        if self.credits < price {
            return Err(format!("The blueprints cost {price:.0} cr."));
        }
        self.credits -= price;
        self.colony.as_mut().unwrap().blueprints.push(idx);
        self.log.push(self.t, Level::Good, format!("Blueprints for the {} bought for {price:.0} cr. The shipyard can build it now.", SHIPS[idx].name));
        Ok(())
    }

    /// A new ship with its full crew and opening stores (the captain and
    /// company are not aboard). A bought ark brings its settlers.
    pub fn new_berthed(&mut self, idx: usize) -> Berthed {
        self.new_berthed_with(idx, false)
    }

    /// An ark a colony's shipyard has launched, its passengers drawn from
    /// that colony's own people.
    pub fn launched_ark(&mut self, emigrants: Vec<super::colony::census::Emigrant>) -> Berthed {
        let mut b = self.new_berthed_with(ARK, false);
        let spec = self.spec_for(ARK);
        b.pax = CrewInfo { count: emigrants.len(), names: vec![], health: 100.0, morale: 70.0 };
        reserve(&mut b.inv, &spec, emigrants.len());
        b.settlers = emigrants;
        b
    }

    fn new_berthed_with(&mut self, idx: usize, with_settlers: bool) -> Berthed {
        let spec = self.spec_for(idx);
        let pax = if with_settlers { settlers(&spec) } else { CrewInfo { count: 0, names: vec![], health: 0.0, morale: 0.0 } };
        let mut inv = Inventory::new(&spec.tanks);
        reserve(&mut inv, &spec, spec.crew + pax.count);
        self.rng_step = self.rng_step.wrapping_add(1);
        let mut rng = derive(self.seed ^ self.rng_step.rotate_left(17), 0x5b_1d);
        Berthed {
            ship_idx: idx,
            inv,
            hold: Hold::default(),
            hull: spec.hull,
            ftl_wear: 0.0,
            crew: (1..spec.crew).map(|_| name(&mut rng)).collect(),
            pax,
            attack_lvl: 0,
            defense_lvl: 0,
            drone_lvl: 0,
            new_ship_boost: new_ship_boost(),
            settlers: vec![],
        }
    }

    /// Is the captain where the fleet lies (and free to change ships)?
    pub fn at_fleet(&self) -> Result<(), String> {
        let Some(c) = self.colony.as_ref() else { return Err("You have no colony.".into()) };
        if self.encounter.is_some() {
            return Err("Not in the middle of a fight.".into());
        }
        let docked = self.sys_id == c.sys_id && self.loc == c.loc && self.trip.is_none() && self.shuttle.is_none();
        if self.ship_dormant || self.in_colony || docked {
            Ok(())
        } else {
            Err(format!("Dock at {} to change ships.", c.world))
        }
    }

    /// Names of the people who go where the captain goes.
    pub(crate) fn own_party(&self) -> Vec<String> {
        let mut v = vec![self.player_name().to_string()];
        for r in [&self.spouse, &self.pilot].into_iter().flatten() {
            if self.crew.names.contains(&r.name) {
                v.push(r.name.clone());
            }
        }
        v
    }

    /// Switch to the laid-up ship in `slot`. The current ship takes its
    /// place in the fleet, stores, cargo and crew included.
    pub fn switch_ship(&mut self, slot: usize) -> Result<(), String> {
        self.at_fleet()?;
        if slot >= self.fleet.len() {
            return Err("No such ship.".into());
        }
        let own = self.own_party();
        let b = self.fleet.remove(slot);
        let (sys_id, loc) = self.colony.as_ref().map(|c| (c.sys_id, c.loc)).unwrap();
        if !self.ship_dormant {
            let ship_crew: Vec<String> = self.crew.names.iter().filter(|n| !own.contains(n)).cloned().collect();
            let old = Berthed {
                ship_idx: self.ship_idx,
                inv: std::mem::take(&mut self.inv),
                hold: std::mem::take(&mut self.hold),
                hull: self.hull,
                ftl_wear: self.ftl_wear,
                crew: ship_crew,
                pax: self.pax.clone(),
                attack_lvl: self.attack_lvl,
                defense_lvl: self.defense_lvl,
                drone_lvl: self.drone_lvl,
                new_ship_boost: std::mem::take(&mut self.new_ship_boost),
                settlers: std::mem::take(&mut self.settlers),
            };
            self.fleet.insert(slot, old);
        } else {
            // The old ship is gone, and the pilot with it.
            self.pilot = None;
        }
        let own = self.own_party();
        self.ship_idx = b.ship_idx;
        self.inv = b.inv;
        self.hold = b.hold;
        self.hull = b.hull;
        self.ftl_wear = b.ftl_wear;
        self.attack_lvl = b.attack_lvl;
        self.defense_lvl = b.defense_lvl;
        self.drone_lvl = b.drone_lvl;
        self.new_ship_boost = b.new_ship_boost;
        self.settlers = b.settlers;
        self.crew.names = own.into_iter().chain(b.crew).collect();
        self.crew.count = self.crew.names.len();
        if self.crew.health <= 0.0 {
            self.crew.health = 100.0;
            self.crew.morale = 80.0;
        }
        self.pax = b.pax;
        self.ftl_charge = 0.0;
        self.faults.clear();
        self.repairing_hull = false;
        self.repairing_wear = false;
        self.mine_target = None;
        self.committed_ore = None;
        self.sys_id = sys_id;
        self.loc = loc;
        self.trip = None;
        self.shuttle = None;
        let revived = self.ship_dormant;
        self.ship_dormant = false;
        let name = self.ship().name;
        if revived {
            self.log.push(self.t, Level::Good, format!("You have a ship again: you take command of the {name}."));
        } else {
            self.log.push(self.t, Level::Good, format!("You transfer your flag to the {name}. The old ship is laid up with the fleet."));
        }
        Ok(())
    }

    /// Break up a laid-up ship for the colony's stockpile.
    pub fn scrap_ship(&mut self, slot: usize) -> Result<f64, String> {
        let Some(b) = self.fleet.get(slot) else { return Err("No such ship.".into()) };
        let Some(_) = self.colony.as_ref() else { return Err("You have no colony.".into()) };
        let spec = b.spec();
        let t = spec.build_cost().0 * 0.4 * (b.hull / spec.hull.max(1.0)).clamp(0.0, 1.0);
        let b = self.fleet.remove(slot);
        let c = self.colony.as_mut().unwrap();
        c.materials += t;
        // Its crew settle in the colony; its cargo is lost to the breakers.
        self.log.push(self.t, Level::Info, format!("The {} is broken up for {t:.0} t of materials; its {} crew settle at {}.", b.spec().name, b.crew.len(), c.name));
        Ok(t)
    }

    /// Move into hull `idx` (trade-in): fresh hull and drive, stores and
    /// cargo carried over up to the new tanks, crew topped up to the new
    /// complement, fittings moved across. Returns what did not fit.
    fn trade_into(&mut self, idx: usize) -> String {
        self.ship_idx = idx;
        let spec = self.ship();
        let mut inv = Inventory::new(&spec.tanks);
        let mut lost = Vec::new();
        for (&r, &have) in &self.inv.stock {
            let kept = inv.add(r, have);
            if have - kept >= 1.0 {
                lost.push(format!("{} of {}", super::res::fmt_kg(have - kept), r.name()));
            }
        }
        // Only what the old ship carried moves across: no free stores. A
        // bigger ship starts with half-empty tanks, which is normal for a
        // new ship and so does not worry the crew.
        self.new_ship_boost = new_ship_boost();
        self.inv = inv;
        self.hull = spec.hull;
        self.ftl_wear = 0.0;
        self.ftl_charge = 0.0;
        self.faults.clear();
        self.repairing_hull = false;
        self.repairing_wear = false;
        self.shuttle = None;
        self.trip = None;
        self.rng_step = self.rng_step.wrapping_add(1);
        let mut rng = derive(self.seed ^ self.rng_step.rotate_left(17), 0x5b_1e);
        while self.crew.count < spec.crew {
            self.crew.names.push(name(&mut rng));
            self.crew.count += 1;
        }
        if idx == ARK && self.pax.count == 0 {
            // Settlers sign on with an ark bought at port. They need air
            // and food from the first hour: the yard sends them aboard
            // with three days' worth, and no more.
            self.pax = settlers(&spec);
            self.settlers.clear();
            reserve(&mut self.inv, &spec, spec.pax);
        }
        lost.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::civ::{Civ, CivKind};
    use crate::sim::colony::{Project, HANGAR_CAP};
    use crate::sim::{Good, Loc, Res};

    fn make_t3(sim: &Sim, gal: &mut Galaxy) {
        let sys = gal.system(sim.sys_id);
        sys.civ = Some(Civ { tier: CivTier::T3, name: "Test".into(), kind: CivKind::Colony, seat: Loc::Star, small: false });
    }

    fn at_t3() -> (Sim, Galaxy) {
        let mut gal = Galaxy::new(42);
        let mut sim = Sim::new_game(42, 0, &mut gal);
        sim.shuttle = None;
        make_t3(&sim, &mut gal);
        (sim, gal)
    }

    /// An ark that has founded a colony in the home system.
    fn with_colony() -> (Sim, Galaxy) {
        let mut gal = Galaxy::new(42);
        let mut sim = Sim::new_game(42, 2, &mut gal);
        let home = gal.home_id();
        let idx = gal.system(home).planets.iter().position(|p| crate::sim::colony::world::colonizable(p).is_ok()).unwrap();
        sim.shuttle = None;
        sim.loc = Loc::Planet(idx);
        sim.found_colony(&mut gal).unwrap();
        (sim, gal)
    }

    #[test]
    fn every_interstellar_port_sells_the_company_ship_and_never_the_ark() {
        let (sim, gal) = at_t3();
        let sale = sim.ships_for_sale(&gal);
        assert!(sale.contains(&1));
        assert!(!sale.contains(&2));
        assert_eq!(SHIPS[1].price, 100_000.0);
        for s in &SHIPS[STARTERS..] {
            assert!(s.for_sale && s.price > 0.0);
        }
    }

    #[test]
    fn trading_in_keeps_the_stores_and_tops_up_the_crew() {
        let (mut sim, gal) = at_t3();
        sim.credits = 50_000.0;
        assert!(sim.ship_quote(&gal, 1, Purchase::TradeIn).is_err(), "too poor");
        assert!(sim.ship_quote(&gal, 1, Purchase::Fleet).is_err(), "no colony, no fleet");
        sim.credits = 200_000.0;
        let h2 = sim.inv.get(Res::H2);
        let before = sim.inv.stock.clone();
        let tradein = sim.trade_in_value();
        assert!((tradein - 12_000.0).abs() < 1.0, "a fresh solo ship trades in at 40%: {tradein}");
        sim.buy_ship(&gal, 1, Purchase::TradeIn).unwrap();
        assert_eq!(sim.ship_idx, 1);
        assert!((sim.credits - (100_000.0 + tradein)).abs() < 1.0);
        assert_eq!(sim.crew.count, 10, "crew topped up to the complement");
        assert_eq!(sim.hull, sim.ship().hull);
        assert!((sim.inv.get(Res::H2) - h2).abs() < 1e-6, "stores carried over");
        // Nothing free: exactly the old stores, and the morale boost is on.
        for r in [Res::Oxygen, Res::Water, Res::Food, Res::Metal] {
            assert!((sim.inv.get(r) - before[&r]).abs() < 1e-6, "{r:?} changed in the trade");
        }
        assert_eq!(sim.new_ship_boost, new_ship_boost());
        // Ten aboard cannot squeeze into a two-seat interceptor (4 berths).
        assert!(sim.can_crew(3).is_err());
    }

    /// Morale on a new ship: low stores do not count until each has first
    /// passed the line; after that they do.
    #[test]
    fn the_new_ship_morale_boost_lasts_until_the_stores_are_up() {
        let (mut sim, mut gal) = at_t3();
        sim.credits = 1e6;
        sim.buy_ship(&gal, 1, Purchase::TradeIn).unwrap();
        for r in [Res::Oxygen, Res::Water, Res::Food] {
            sim.inv.stock.insert(r, sim.inv.cap(r) * 0.3);
        }
        sim.crew.morale = 60.0;
        sim.advance(2.0, &mut gal);
        assert!(sim.crew.morale >= 60.0, "a new ship's empty tanks do not hurt: {}", sim.crew.morale);
        // Fill oxygen past the line: the boost is spent for oxygen alone.
        sim.inv.stock.insert(Res::Oxygen, sim.inv.cap(Res::Oxygen) * 0.9);
        sim.advance(0.1, &mut gal);
        assert!(!sim.new_ship_boost.contains(&Res::Oxygen));
        assert!(sim.new_ship_boost.contains(&Res::Water));
        // Run oxygen low again: now it worries the crew.
        sim.inv.stock.insert(Res::Oxygen, sim.inv.cap(Res::Oxygen) * 0.3);
        let m = sim.crew.morale;
        sim.advance(2.0, &mut gal);
        assert!(sim.crew.morale < m, "low O2 after the boost hurts morale: {} -> {}", m, sim.crew.morale);
    }

    /// A new fleet ship carries only a small survival reserve: worth far
    /// less than the money lost buying and trading it straight back.
    #[test]
    fn new_ships_carry_a_reserve_not_a_fortune() {
        let (mut sim, _gal) = at_t3();
        for idx in 0..SHIPS.len() {
            let b = sim.new_berthed(idx);
            let value: f64 = b.inv.stock.iter().map(|(r, kg)| crate::sim::civ::base_price(*r) * kg).sum();
            let spec = b.spec();
            assert!(value < spec.price * 0.6 * 0.25, "{}: {value:.0} cr of free stores", spec.name);
            if idx == ARK {
                // Even with 500 settlers' reserve, buying an ark, selling
                // its stores (T3 pays 70%) and trading it in loses money.
                let full = sim.new_berthed_with(ARK, true);
                let v: f64 = full.inv.stock.iter().map(|(r, kg)| crate::sim::civ::base_price(*r) * kg).sum();
                assert_eq!(full.pax.count, 500);
                assert!(v * 0.7 + spec.trade_in() < spec.price, "ark round trip pays: {v:.0}");
            }
            let people = spec.crew as f64;
            assert!(b.inv.get(Res::Oxygen) >= people * crate::sim::state::O2_PER_P_H * 48.0, "{}: two days of air", spec.name);
            assert_eq!(b.new_ship_boost, new_ship_boost());
        }
    }

    /// The ark's reactor runs everything it has in a system at once.
    #[test]
    fn the_ark_can_power_all_its_work() {
        let s = &SHIPS[ARK];
        let load = s.ls_mw + s.fab_mw + s.shields_mw + s.scoop_mw + s.drones as f64 * s.drone_mw + s.precharge_mw;
        assert!(load <= s.reactor_mw, "{load} MW on a {} MW reactor", s.reactor_mw);
    }

    #[test]
    fn fighters_fight_better_and_freighters_carry_more() {
        for s in SHIPS.iter().filter(|s| s.role == crate::sim::Role::Fighter) {
            assert!(s.attack > SHIPS[1].attack && s.defense > SHIPS[0].defense, "{}", s.name);
        }
        for s in SHIPS.iter().filter(|s| s.role == crate::sim::Role::Cargo && s.price > SHIPS[1].price) {
            assert!(s.cargo_t > SHIPS[1].cargo_t, "{}", s.name);
        }
    }

    #[test]
    fn a_fleet_ship_is_ferried_home_and_each_ship_keeps_its_own_cargo() {
        let (mut sim, mut gal) = with_colony();
        make_t3(&sim, &mut gal);
        sim.in_colony = false;
        sim.credits = 1e6;
        sim.buy_ship(&gal, 1, Purchase::Fleet).unwrap();
        assert_eq!(sim.ship_idx, 2, "still flying the ark");
        assert_eq!(sim.fleet.len(), 1);
        assert_eq!(sim.fleet[0].crew.len(), 9, "the company ship comes crewed");
        // Load the ark, then switch: the cargo stays with the ark.
        sim.hold.add(Good::MedKit, 3);
        let ark_h2 = sim.inv.get(Res::H2);
        let captain = sim.player_name().to_string();
        sim.switch_ship(0).unwrap();
        assert_eq!(sim.ship_idx, 1);
        assert_eq!(sim.hold.get(Good::MedKit), 0);
        assert_eq!(sim.crew.names[0], captain);
        assert_eq!(sim.crew.count, 10);
        assert_eq!(sim.fleet[0].ship_idx, 2);
        assert_eq!(sim.fleet[0].hold.get(Good::MedKit), 3);
        // ...and back again.
        sim.inv.take(Res::H2, 100.0);
        sim.switch_ship(0).unwrap();
        assert_eq!(sim.ship_idx, 2);
        assert_eq!(sim.hold.get(Good::MedKit), 3);
        assert!((sim.inv.get(Res::H2) - ark_h2).abs() < 1e-6);
        // Away from the colony, no switching.
        sim.loc = Loc::Star;
        assert!(sim.switch_ship(0).is_err());
    }

    #[test]
    fn the_colony_shipyard_builds_starters_and_bought_designs() {
        let (mut sim, mut gal) = with_colony();
        let c = sim.colony.as_mut().unwrap();
        assert!(c.can_build(Project::Shipyard).is_err(), "needs dock, business and 250k");
        assert!(c.can_build(Project::Ship(1)).is_err(), "needs a shipyard");
        c.shipyard = true;
        assert!(c.can_build(Project::Ship(2)).is_ok(), "the ark can be built");
        assert!(c.can_build(Project::Ship(5)).is_err(), "no blueprints yet");
        sim.credits = 1e6;
        sim.buy_blueprint(5).unwrap();
        let c = sim.colony.as_mut().unwrap();
        assert!(c.can_build(Project::Ship(5)).is_ok());
        for _ in 0..HANGAR_CAP {
            c.queue.push(crate::sim::colony::Construction { kind: Project::Ship(0), done: 0.0 });
        }
        assert!(c.can_build(Project::Ship(0)).is_err(), "slips full");
        // A launched ship joins the fleet on the next tick.
        c.queue.clear();
        c.hangar.push(1);
        sim.colony_tick(&mut gal);
        assert_eq!(sim.fleet.len(), 1);
        assert_eq!(sim.fleet[0].ship_idx, 1);
        let m = sim.colony.as_ref().unwrap().materials;
        sim.scrap_ship(0).unwrap();
        assert!(sim.colony.as_ref().unwrap().materials > m);
    }

    #[test]
    fn a_fleet_gives_a_shipless_captain_a_ship_again() {
        let (mut sim, mut gal) = with_colony();
        sim.ship_lost_to_colony("Lost", "Test");
        sim.colony.as_mut().unwrap().hangar.push(0);
        sim.colony_tick(&mut gal);
        sim.switch_ship(0).unwrap();
        assert!(!sim.ship_dormant);
        assert!(sim.fleet.is_empty(), "the lost ship is not laid up");
        assert!(sim.leave_colony().is_ok());
        assert!(sim.inv.get(Res::H2) > 0.0);
    }
}
