//! Cargo trading, recruiting and hiring at ports.

use super::cargo::Good;
use super::civ::{CivKind, CivTier};
use super::galaxy::Galaxy;
use super::logbuf::Level;
use super::people::{years, Sex};
use super::state::{Recruit, Sim};
use crate::rngx::{derive, name};
use rand::RngExt;

/// What a partner search costs (credits) at a planetary / interstellar port.
pub fn recruit_cost(tier: CivTier) -> f64 {
    if tier == CivTier::T3 { 800.0 } else { 400.0 }
}

pub fn pilot_cost(tier: CivTier) -> f64 {
    if tier == CivTier::T3 { 3_000.0 } else { 2_000.0 }
}

impl Sim {
    pub fn player_name(&self) -> &str {
        self.crew.names.first().map(|s| s.as_str()).unwrap_or("the captain")
    }

    /// The captain's given name (old saves: the first word of the name).
    pub fn player_given(&self) -> String {
        if !self.given_name.is_empty() {
            return self.given_name.clone();
        }
        self.player_name().split_whitespace().next().unwrap_or("Captain").to_string()
    }

    /// The captain's surname (old saves: the rest of the name, or a
    /// derived one).
    pub fn player_surname(&self) -> String {
        if !self.surname.is_empty() {
            return self.surname.clone();
        }
        let rest: Vec<&str> = self.player_name().split_whitespace().skip(1).collect();
        if rest.is_empty() {
            let mut rng = derive(self.seed, 0x5a_7a3e);
            name(&mut rng)
        } else {
            rest.join(" ")
        }
    }

    /// Set up the captain chosen at the start of a new game.
    pub fn apply_captain(&mut self, c: &super::background::Captain) {
        self.given_name = c.given.trim().to_string();
        self.surname = c.surname.trim().to_string();
        self.player_sex = c.sex;
        self.background = c.background;
        let full = format!("{} {}", self.given_name, self.surname);
        match self.crew.names.first_mut() {
            Some(n) => *n = full,
            None => self.crew.names.push(full),
        }
        self.credits += c.background.money();
        let (a, d) = c.background.combat_levels();
        self.attack_lvl += a;
        self.defense_lvl += d;
        self.log.push(self.t, Level::Info, format!("Captain {} ({}) takes command.", self.player_name(), c.background.name()));
    }

    /// The captain's age in years.
    pub fn player_age(&self) -> f64 {
        years(self.t) - self.player_birth_y
    }

    pub fn cargo_room_t(&self) -> f64 {
        (self.ship().cargo_t - self.hold.mass_t()).max(0.0)
    }

    /// Buy or sell `n` units of a good on the local cargo market. Returns
    /// the credits received (negative when paid).
    pub fn trade_good(&mut self, gal: &mut Galaxy, g: Good, buy: bool, n: u32) -> f64 {
        let sys = gal.system(self.sys_id);
        sys.refresh_cargo_market(self.seed);
        let Some(market) = sys.market.as_mut() else {
            self.log.push(self.t, Level::Warn, "There is no market here.");
            return 0.0;
        };
        if buy {
            let Some(price) = market.buy_price(g) else {
                self.log.push(self.t, Level::Warn, format!("Nobody here sells {g}."));
                return 0.0;
            };
            let price = price * self.background.buy_mult();
            let stock = market.offers[&g].stock;
            let room = (self.ship().cargo_t - self.hold.mass_t()).max(0.0);
            let fit = (room / g.mass_t() + 1e-9).floor() as u32;
            let afford = (self.credits / price).floor() as u32;
            let k = n.min(stock).min(fit).min(afford);
            if k == 0 {
                let why = if fit == 0 {
                    format!("No room in the hold for {g} ({:.1} t each).", g.mass_t())
                } else if afford == 0 {
                    format!("You cannot afford {g} ({price:.0} cr each).")
                } else {
                    format!("They have no {g} left.")
                };
                self.log.push(self.t, Level::Warn, why);
                return 0.0;
            }
            if let Some(o) = market.offers.get_mut(&g) {
                o.stock -= k;
            }
            let cost = k as f64 * price;
            self.credits -= cost;
            self.hold.add(g, k);
            self.stats.trades += 1;
            self.stats.goods_bought += k;
            self.log.push(self.t, Level::Good, format!("Bought {k} × {g} for {cost:.0} cr."));
            -cost
        } else {
            let Some(price) = market.sell_price(g) else {
                self.log.push(self.t, Level::Warn, format!("Nobody here buys {g}."));
                return 0.0;
            };
            let price = price * self.background.sell_mult();
            let k = self.hold.take(g, n);
            if k == 0 {
                self.log.push(self.t, Level::Warn, format!("You have no {g} to sell."));
                return 0.0;
            }
            // Selling into a market restocks it (for sellers of that good).
            if let Some(o) = market.offers.get_mut(&g)
                && o.sells
            {
                o.stock += k;
            }
            let pay = k as f64 * price;
            self.credits += pay;
            self.stats.trades += 1;
            self.stats.goods_sold += k;
            self.log.push(self.t, Level::Good, format!("Sold {k} × {g} for {pay:.0} cr."));
            pay
        }
    }

    /// Where a partner or pilot can be found: a T2/T3 civilisation here.
    pub fn port_tier(&self, gal: &Galaxy) -> Option<CivTier> {
        gal.system_ref(self.sys_id).civ.as_ref().map(|c| c.tier).filter(|t| *t >= CivTier::T2)
    }

    /// Search the port for a partner of the opposite sex to the captain.
    /// Costs credits and an hour, and may fail. Returns true on success.
    pub fn recruit_partner(&mut self, gal: &mut Galaxy) -> bool {
        if self.spouse.is_some() {
            self.log.push(self.t, Level::Info, "You already have a partner aboard.");
            return false;
        }
        let Some(tier) = self.port_tier(gal) else {
            self.log.push(self.t, Level::Warn, "Recruiting needs a planetary or interstellar civilisation.");
            return false;
        };
        let cost = recruit_cost(tier) * self.background.recruit_cost_mult();
        if self.credits < cost {
            self.log.push(self.t, Level::Warn, format!("A search costs {cost:.0} cr. You cannot afford it."));
            return false;
        }
        self.credits -= cost;
        self.rng_step += 1;
        let mut rng = derive(self.seed ^ self.rng_step.rotate_left(29), 0x5e_a7c4);
        let chance = (if tier == CivTier::T3 { 0.5 } else { 0.3 } + self.background.recruit_chance_bonus()).min(1.0);
        // The search takes an hour of ship time.
        self.advance(1.0, gal);
        let civ = gal.system_ref(self.sys_id).civ.clone();
        let Some(civ) = civ else { return false };
        if !rng.random_bool(chance) {
            self.log.push(self.t, Level::Info, format!("You spent {cost:.0} cr and an hour among {}: nobody wanted to leave with you. Try again?", civ.display()));
            return false;
        }
        let species = (civ.kind == CivKind::Natives).then(|| civ.name.clone());
        let who = Recruit { name: name(&mut rng), sex: self.player_sex.opposite(), species, skill: 0 };
        let what = match &who.species {
            Some(s) => format!("{} of the {s}", who.name),
            None => who.name.clone(),
        };
        self.log.push(self.t, Level::Good, format!("{what} ({}) joins you as your partner.", who.sex.name().to_lowercase()));
        self.stats.partners += 1;
        if who.species.is_some() {
            self.log.push(self.t, Level::Info, "A human-native couple needs a Genetic Modification Kit to have children. A colony you found together will be Hybrid.");
        }
        self.crew.names.push(who.name.clone());
        self.crew.count += 1;
        self.spouse = Some(who);
        true
    }

    /// Hire a combat pilot (better odds in battle and when fleeing).
    pub fn hire_pilot(&mut self, gal: &mut Galaxy) -> bool {
        if self.pilot.is_some() {
            self.log.push(self.t, Level::Info, "You already have a pilot.");
            return false;
        }
        let Some(tier) = self.port_tier(gal) else {
            self.log.push(self.t, Level::Warn, "Pilots are hired at planetary or interstellar ports.");
            return false;
        };
        let cost = pilot_cost(tier) * self.background.recruit_cost_mult();
        if self.credits < cost {
            self.log.push(self.t, Level::Warn, format!("A pilot costs {cost:.0} cr. You cannot afford one."));
            return false;
        }
        self.credits -= cost;
        self.rng_step += 1;
        let mut rng = derive(self.seed ^ self.rng_step.rotate_left(31), 0x91_107);
        let skill = if tier == CivTier::T3 { rng.random_range(2..=3) } else { rng.random_range(1..=2) };
        let sex = if rng.random_bool(0.5) { Sex::Male } else { Sex::Female };
        let who = Recruit { name: name(&mut rng), sex, species: None, skill };
        self.log.push(self.t, Level::Good, format!("Hired pilot {} (skill {skill}) for {cost:.0} cr.", who.name));
        self.stats.pilots += 1;
        self.crew.names.push(who.name.clone());
        self.crew.count += 1;
        self.pilot = Some(who);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::civ::Civ;
    use crate::sim::state::Loc;

    fn port(gal: &mut Galaxy, id: u64, tier: CivTier, kind: CivKind) {
        let sys = gal.system(id);
        sys.civ = Some(Civ { tier, name: "Zaquel".into(), kind, seat: Loc::Planet(0), small: false });
        sys.market = None;
        sys.refresh_cargo_market(42);
    }

    #[test]
    fn the_ark_sails_with_a_colony_kit() {
        let mut gal = Galaxy::new(42);
        let sim = Sim::new_game(42, 2, &mut gal);
        assert!(sim.hold.get(Good::HydroKit) >= 6);
        assert!(sim.hold.get(Good::MedKit) >= 51);
        assert!(sim.hold.mass_t() <= sim.ship().cargo_t);
        let solo = Sim::new_game(42, 0, &mut gal);
        assert!(solo.hold.is_empty());
        assert!((22.0..25.0).contains(&solo.player_age()));
        assert!((60.0..80.0).contains(&solo.retire_age));
    }

    #[test]
    fn cargo_buys_respect_hold_credits_and_stock() {
        let mut gal = Galaxy::new(42);
        let mut sim = Sim::new_game(42, 0, &mut gal);
        let id = sim.sys_id;
        port(&mut gal, id, CivTier::T3, CivKind::Colony);
        sim.credits = 1_000_000.0;
        // 8 t hold: two Power Generators do not fit (8 t each → one).
        sim.trade_good(&mut gal, Good::PowerGen, true, 5);
        assert_eq!(sim.hold.get(Good::PowerGen), 1);
        assert!(sim.hold.mass_t() <= sim.ship().cargo_t + 1e-9);
        let before = sim.credits;
        let got = sim.trade_good(&mut gal, Good::PowerGen, false, 1);
        assert!(got > 0.0 && sim.credits > before);
        assert!(sim.hold.is_empty());
        // Selling what you do not have does nothing.
        assert_eq!(sim.trade_good(&mut gal, Good::Luxury, false, 3), 0.0);
    }

    #[test]
    fn partners_are_the_opposite_sex_and_natives_are_marked() {
        for (sex, kind) in [(Sex::Male, CivKind::Natives), (Sex::Female, CivKind::Colony)] {
            let mut gal = Galaxy::new(42);
            let mut sim = Sim::new_game(42, 0, &mut gal);
            sim.player_sex = sex;
            let id = sim.sys_id;
            port(&mut gal, id, CivTier::T3, kind);
            sim.credits = 1_000_000.0;
            let mut tries = 0;
            while sim.spouse.is_none() && tries < 50 {
                sim.recruit_partner(&mut gal);
                tries += 1;
            }
            let sp = sim.spouse.as_ref().expect("a partner within 50 tries");
            assert_eq!(sp.sex, sex.opposite());
            assert_eq!(sp.species.is_some(), kind == CivKind::Natives);
            assert_eq!(sim.crew.count, 2);
            assert!(sim.crew_bonus() > 1.0);
        }
    }
}
