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
        sys.restock_cargo_market(self.seed, self.t);
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

    /// Where a partner can be sought here: (cost, chance, at a port). Any
    /// port, or a pre-space native people (few of whom will leave with a
    /// stranger from the sky).
    pub fn partner_search(&self, gal: &Galaxy) -> Option<(f64, f64, bool)> {
        let civ = gal.system_ref(self.sys_id).civ.as_ref()?;
        let (cost, chance, port) = match civ.tier {
            CivTier::T3 => (recruit_cost(CivTier::T3), 0.5, true),
            CivTier::T2 => (recruit_cost(CivTier::T2), 0.3, true),
            CivTier::T1 if civ.kind == CivKind::Natives => (recruit_cost(CivTier::T2), 0.1, false),
            CivTier::T1 => return None,
        };
        Some((cost * self.background.recruit_cost_mult(), (chance + self.background.recruit_chance_bonus()).min(1.0), port))
    }

    /// Search the port for a partner of the opposite sex to the captain.
    /// Costs credits and an hour, and may fail. Returns true on success.
    pub fn recruit_partner(&mut self, gal: &mut Galaxy) -> bool {
        if self.spouse.is_some() {
            self.log.push(self.t, Level::Info, "You already have a partner aboard.");
            return false;
        }
        let Some((cost, chance, _)) = self.partner_search(gal) else {
            self.log.push(self.t, Level::Warn, "Recruiting needs a port, or a native people (even pre-space).");
            return false;
        };
        if self.credits < cost {
            self.log.push(self.t, Level::Warn, format!("A search costs {cost:.0} cr. You cannot afford it."));
            return false;
        }
        self.credits -= cost;
        self.rng_step += 1;
        let mut rng = derive(self.seed ^ self.rng_step.rotate_left(29), 0x5e_a7c4);
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
            self.log.push(self.t, Level::Info, "A human-native couple needs a Genetic Modification Kit to have children. Your children will be Hybrids, who have children only with other Hybrids.");
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

    /// Pre-space natives can be courted (at 1 in 10), and their partner is
    /// native; a pre-space human colony has nobody to recruit.
    #[test]
    fn pre_space_natives_can_be_recruited_from_at_long_odds() {
        let mut gal = Galaxy::new(42);
        let mut sim = Sim::new_game(42, 0, &mut gal);
        let id = sim.sys_id;
        port(&mut gal, id, CivTier::T1, CivKind::Colony);
        assert!(sim.partner_search(&gal).is_none());
        port(&mut gal, id, CivTier::T1, CivKind::Natives);
        let (_, chance, at_port) = sim.partner_search(&gal).unwrap();
        assert!(!at_port && chance < 0.3);
        sim.credits = 1e7;
        let mut tries = 0;
        while sim.spouse.is_none() && tries < 200 {
            sim.recruit_partner(&mut gal);
            tries += 1;
        }
        assert!(tries > 1, "long odds");
        assert!(sim.spouse.as_ref().expect("a partner within 200 tries").species.is_some());
    }

    #[test]
    fn port_stock_recovers_after_buying_it_out() {
        let mut gal = Galaxy::new(42);
        let mut sim = Sim::new_game(42, 1, &mut gal);
        let id = sim.sys_id;
        gal.system(id).civ = Some(crate::sim::civ::Civ { tier: CivTier::T2, name: "Port".into(), kind: crate::sim::civ::CivKind::Colony, seat: Loc::Planet(0), small: false });
        gal.system(id).market = None;
        gal.system(id).refresh_cargo_market(sim.seed);
        let usual = gal.system(id).market.as_ref().unwrap().offers[&Good::MedKit].stock;
        sim.credits = 1e9;
        let mut bought = 0;
        while bought < usual {
            let b = sim.hold.get(Good::MedKit);
            sim.trade_good(&mut gal, Good::MedKit, true, usual);
            let got = sim.hold.get(Good::MedKit) - b;
            if got == 0 { break; }
            bought += got;
            sim.hold.take(Good::MedKit, got);
        }
        let stock = |gal: &mut Galaxy| gal.system(id).market.as_ref().unwrap().offers[&Good::MedKit].stock;
        assert_eq!(stock(&mut gal), 0, "bought out");
        sim.advance(24.0 * 30.0, &mut gal);
        let after_month = stock(&mut gal);
        assert!(after_month as f64 > usual as f64 * 0.5 && after_month <= usual, "{after_month} of {usual} after a month");
        sim.advance(24.0 * 150.0, &mut gal);
        assert!(stock(&mut gal) as f64 >= usual as f64 * 0.95, "nearly full after half a year");
    }
}
