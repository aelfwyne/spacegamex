//! Pirates: rare random interceptions, resolved in a few narrated rounds.
//!
//! The player chooses Fight, Flee or Pay. A loss costs a share of the
//! cargo, capped by the Risk option: Tourist never more than half, Normal
//! never more than three quarters and never the fuel, and only Risky can
//! lose the ship (after the fifth defeat; rare, because destroying a ship
//! brings the corporate police).

use super::cargo::Good;
use super::civ::CivTier;
use super::galaxy::Galaxy;
use super::logbuf::Level;
use super::opts::Risk;
use super::res::Res;
use super::state::Sim;
use crate::rngx::{derive, name};
use rand::rngs::StdRng;
use rand::RngExt;
use serde::{Deserialize, Serialize};

/// Mean hours between interceptions on Normal risk, carrying nothing.
pub const MEAN_HOURS: f64 = 400.0;
/// Highest Attack / Defense upgrade level.
pub const MAX_LEVEL: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
    /// They have hailed you: fight, flee or pay.
    Approach,
    /// You won: destroy their ship (bounty) or let them go.
    Won,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Encounter {
    /// "the Varn Kites"
    pub band: String,
    pub attack: f64,
    pub defense: f64,
    pub stage: Stage,
    /// Bounty for destroying their ship (paid at the next port).
    pub bounty: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Fight,
    Flee,
    Pay,
    Destroy,
    Release,
}

impl Risk {
    /// Multiplier on how often pirates turn up.
    pub fn combat_mult(self) -> f64 {
        match self {
            Risk::Tourist => 0.3,
            Risk::Normal => 1.0,
            Risk::Risky => 2.5,
        }
    }
}

/// Upgrade price for the next level (credits).
pub fn upgrade_cost(level: u8) -> f64 {
    4_000.0 * (level as f64 + 1.0)
}

fn band_name(rng: &mut StdRng) -> String {
    const KIND: [&str; 8] = ["Kites", "Reavers", "Jackals", "Corsairs", "Wolves", "Knives", "Ravens", "Drifters"];
    format!("the {} {}", name(rng), KIND[rng.random_range(0..KIND.len())])
}

impl Sim {
    /// Your ship's attack and defense (hull class + upgrades + pilot).
    pub fn combat_stats(&self) -> (f64, f64) {
        let spec = self.ship();
        let class = match spec.crew {
            1 => (2.0, 2.0),
            n if n <= 10 && spec.pax == 0 => (3.0, 3.0),
            _ => (2.5, 4.0),
        };
        let pilot = self.pilot.as_ref().map(|p| p.skill as f64 * 0.5).unwrap_or(0.0);
        (
            class.0 + self.attack_lvl as f64 + self.artifact_attack() + pilot,
            class.1 + self.defense_lvl as f64 + pilot * 0.5,
        )
    }

    /// Chance of winning a fight against the current encounter, 0..1.
    pub fn win_chance(&self) -> f64 {
        let Some(e) = &self.encounter else { return 0.0 };
        let p = self.round_p(e);
        // Best of five rounds (first to three hits).
        let mut win = 0.0;
        for k in 0..3 {
            // Exactly k losses before the third win.
            let c = [1.0, 3.0, 6.0][k];
            win += c * p.powi(3) * (1.0 - p).powi(k as i32);
        }
        win
    }

    fn round_p(&self, e: &Encounter) -> f64 {
        let (a, d) = self.combat_stats();
        let you = a + 0.5 * d;
        let them = e.attack + 0.5 * e.defense;
        (you / (you + them)).clamp(0.05, 0.95)
    }

    pub fn flee_chance(&self) -> f64 {
        let agile = if self.ship().crew == 1 { 0.1 } else { 0.0 };
        let pilot = self.pilot.as_ref().map(|p| p.skill as f64 * 0.12).unwrap_or(0.0);
        (0.3 + agile + pilot).min(0.85)
    }

    /// The share of the cargo a tribute (paying up front) costs.
    pub fn tribute_share(&self) -> f64 {
        if self.opts.risk == Risk::Tourist { 0.15 } else { 0.25 }
    }

    fn combat_rng(&mut self, salt: u64) -> StdRng {
        self.rng_step = self.rng_step.wrapping_add(1);
        derive(self.seed ^ self.rng_step.rotate_left(23), salt)
    }

    /// Hourly interception check, and bounty collection at ports.
    pub(crate) fn combat_tick(&mut self, dt_h: f64, gal: &mut Galaxy) {
        if self.over.is_some() || self.encounter.is_some() {
            return;
        }
        let tier = gal.system_ref(self.sys_id).civ.as_ref().map(|c| c.tier);
        if self.pending_bounty > 0.0 && self.trip.is_none() && tier.is_some_and(|t| t >= CivTier::T2) {
            let b = self.pending_bounty;
            self.credits += b;
            self.pending_bounty = 0.0;
            self.log.push(self.t, Level::Good, format!("The port pays your pirate bounty: +{b:.0} cr."));
        }
        // Home is safe: nobody hunts the origin system.
        if self.sys_id == gal.home_id() || self.in_colony {
            return;
        }
        let cargo = 1.0 + (self.hold.value() / 50_000.0).min(1.0);
        let civ = if tier == Some(CivTier::T3) { 0.5 } else { 1.0 };
        let rate = self.opts.risk.combat_mult() * cargo * civ / MEAN_HOURS;
        let p = 1.0 - (-rate * dt_h).exp();
        let mut rng = self.combat_rng(0xb1_7a7e);
        if !rng.random_bool(p.clamp(0.0, 1.0)) {
            return;
        }
        let (a, d) = self.combat_stats();
        let level = (a + d) / 2.0;
        let edge = match self.opts.risk {
            Risk::Tourist => -0.8,
            Risk::Normal => 0.0,
            Risk::Risky => 1.0,
        };
        let attack = (level + edge + rng.random_range(-1.5..2.0)).max(1.0);
        let defense = (level + edge + rng.random_range(-1.5..2.0)).max(1.0);
        let band = band_name(&mut rng);
        let bounty = (1_500.0 * (attack + defense) / 2.0 * rng.random_range(0.7..1.3)).round();
        self.log.push(self.t, Level::Critical, format!("PIRATES! {} have locked on to you. Fight, flee or pay?", capitalize(&band)));
        self.encounter = Some(Encounter { band, attack, defense, stage: Stage::Approach, bounty });
    }

    /// Resolve the player's choice for the current encounter.
    pub fn resolve_encounter(&mut self, choice: Choice) {
        let Some(e) = self.encounter.clone() else { return };
        let band = capitalize(&e.band);
        match (e.stage, choice) {
            (Stage::Approach, Choice::Pay) => {
                let share = self.tribute_share();
                let lost = self.take_tribute(share, false);
                self.log.push(self.t, Level::Warn, format!("You pay {band} off: {lost}."));
                self.encounter = None;
            }
            (Stage::Approach, Choice::Flee) => {
                let mut rng = self.combat_rng(0xf1_ee);
                if rng.random_bool(self.flee_chance()) {
                    self.log.push(self.t, Level::Good, format!("You outrun {band}. They break off."));
                    self.encounter = None;
                } else {
                    self.log.push(self.t, Level::Bad, format!("{band} cut you off. You have to fight, already damaged."));
                    self.fight(&e, 2);
                }
            }
            (Stage::Approach, Choice::Fight) => self.fight(&e, 3),
            (Stage::Won, Choice::Destroy) => {
                let mut rng = self.combat_rng(0xde_57);
                self.pending_bounty += e.bounty;
                let mat = self.inv.add(Res::Mat, rng.random_range(40.0..160.0));
                let metal = self.inv.add(Res::Metal, rng.random_range(200.0..800.0));
                self.log.push(self.t, Level::Good, format!(
                    "You destroy {}'s ship. Salvage: {mat:.0} kg MAT, {metal:.0} kg metal. A {:.0} cr bounty waits at the next port.",
                    e.band, e.bounty
                ));
                self.encounter = None;
            }
            (Stage::Won, Choice::Release) => {
                self.log.push(self.t, Level::Info, format!("You let {} limp away.", e.band));
                self.encounter = None;
            }
            _ => {}
        }
    }

    /// Up to five rounds: first to three hits wins. `hp` is how many hits
    /// you can take (a failed escape starts you damaged).
    fn fight(&mut self, e: &Encounter, hp: u32) {
        let p = self.round_p(e);
        let (_, def) = self.combat_stats();
        let mut rng = self.combat_rng(0x00f1_9a75);
        let (mut mine, mut theirs) = (hp, 3u32);
        let mut round = 0;
        let band = capitalize(&e.band);
        while mine > 0 && theirs > 0 {
            round += 1;
            if rng.random_bool(p) {
                theirs -= 1;
                self.log.push(self.t, Level::Info, format!("Round {round}: you hit {}.", e.band));
            } else {
                mine -= 1;
                let dmg = self.ship().hull * 0.04 * (e.attack / (def + 1.0)).clamp(0.3, 2.0);
                self.hull = (self.hull - dmg).max(1.0);
                self.last_hull_source = "pirate fire".into();
                self.log.push(self.t, Level::Bad, format!("Round {round}: {band} hit you (hull −{dmg:.0})."));
            }
        }
        if theirs == 0 {
            self.battles_won += 1;
            let loot = self.salvage_pirates(&mut rng, e);
            self.log.push(self.t, Level::Good, format!("Victory over {}! Salvaged {loot}.", e.band));
            if rng.random_bool(0.1) {
                if rng.random_bool(0.5) && self.attack_lvl < MAX_LEVEL {
                    self.attack_lvl += 1;
                    self.log.push(self.t, Level::Good, format!("Their weapons tech is usable: Attack upgraded to level {}.", self.attack_lvl));
                } else if self.defense_lvl < MAX_LEVEL {
                    self.defense_lvl += 1;
                    self.log.push(self.t, Level::Good, format!("Their armour tech is usable: Defense upgraded to level {}.", self.defense_lvl));
                }
            }
            if let Some(enc) = self.encounter.as_mut() {
                enc.stage = Stage::Won;
            }
            self.log.push(self.t, Level::Info, format!("Destroy their ship for a {:.0} cr bounty and more salvage, or let them go?", e.bounty));
        } else {
            self.battles_lost += 1;
            self.lose_battle(&mut rng, e);
            self.encounter = None;
        }
    }

    fn lose_battle(&mut self, rng: &mut StdRng, e: &Encounter) {
        let band = capitalize(&e.band);
        let (lo, hi) = match self.opts.risk {
            Risk::Tourist => (0.25, 0.5),
            Risk::Normal => (0.25, 0.75),
            Risk::Risky if self.battles_lost > 5 => (0.25, 0.95),
            Risk::Risky => (0.25, 0.75),
        };
        if self.opts.risk == Risk::Risky && self.battles_lost > 5 && rng.random_bool(0.03) {
            self.game_over(
                "Destroyed by pirates",
                format!("{band} decided a witness was worth more dead, police or no police."),
            );
            return;
        }
        let share = rng.random_range(lo..=hi);
        let lost = self.take_tribute(share, self.opts.risk == Risk::Risky);
        self.log.push(self.t, Level::Bad, format!("Defeated. {band} board you and take {:.0}% of your cargo: {lost}.", share * 100.0));
    }

    /// Hand over `share` of the cargo, credits and stores. Fuel is only at
    /// stake on Risky; life-support stocks never are.
    pub(crate) fn take_tribute(&mut self, share: f64, fuel_too: bool) -> String {
        let share = share.clamp(0.0, 1.0);
        let mut parts = Vec::new();
        let goods: Vec<(Good, u32)> = self.hold.units.iter().map(|(g, n)| (*g, *n)).collect();
        let mut units = 0;
        for (g, n) in goods {
            let k = (n as f64 * share).round() as u32;
            units += self.hold.take(g, k);
        }
        if units > 0 {
            parts.push(format!("{units} units of cargo"));
        }
        let cr = (self.credits * share).floor();
        if cr >= 1.0 {
            self.credits -= cr;
            parts.push(format!("{cr:.0} cr"));
        }
        let mut kg = 0.0;
        for r in [Res::Ice, Res::Metal, Res::Silicate, Res::Carbon, Res::Nitrogen, Res::Mat, Res::Med] {
            kg += self.inv.take(r, self.inv.get(r) * share);
        }
        if fuel_too {
            kg += self.inv.take(Res::H2, self.inv.get(Res::H2) * share);
        }
        if kg >= 1.0 {
            parts.push(format!("{kg:.0} kg of stores"));
        }
        if parts.is_empty() { "nothing worth taking".into() } else { parts.join(", ") }
    }

    fn salvage_pirates(&mut self, rng: &mut StdRng, e: &Encounter) -> String {
        let mut parts = Vec::new();
        let cr = (rng.random_range(300.0..1_500.0) * (e.attack + e.defense) / 4.0).round();
        self.credits += cr;
        parts.push(format!("{cr:.0} cr"));
        const LOOT: [Good; 6] = [Good::Machinery, Good::Electronics, Good::Pharma, Good::Luxury, Good::Textiles, Good::MedKit];
        for _ in 0..rng.random_range(1..=2) {
            let g = LOOT[rng.random_range(0..LOOT.len())];
            let room = (self.cargo_room_t() / g.mass_t()).floor() as u32;
            let n = rng.random_range(1..=4).min(room);
            if n > 0 {
                self.hold.add(g, n);
                parts.push(format!("{n} × {g}"));
            }
        }
        let h2 = self.inv.add(Res::H2, rng.random_range(100.0..600.0));
        if h2 >= 1.0 {
            parts.push(format!("{h2:.0} kg H2"));
        }
        parts.join(", ")
    }

    /// Buy an Attack or Defense level at an interstellar shipyard.
    pub fn buy_upgrade(&mut self, gal: &Galaxy, attack: bool) -> bool {
        if gal.system_ref(self.sys_id).civ.as_ref().map(|c| c.tier) != Some(CivTier::T3) {
            self.log.push(self.t, Level::Warn, "Weapons and armour are fitted at interstellar (T3) shipyards.");
            return false;
        }
        let lvl = if attack { self.attack_lvl } else { self.defense_lvl };
        if lvl >= MAX_LEVEL {
            self.log.push(self.t, Level::Info, "Already at the highest level.");
            return false;
        }
        let cost = upgrade_cost(lvl);
        if self.credits < cost {
            self.log.push(self.t, Level::Warn, format!("That upgrade costs {cost:.0} cr."));
            return false;
        }
        self.credits -= cost;
        let what = if attack {
            self.attack_lvl += 1;
            format!("Attack level {}", self.attack_lvl)
        } else {
            self.defense_lvl += 1;
            format!("Defense level {}", self.defense_lvl)
        };
        self.log.push(self.t, Level::Good, format!("Shipyard fitted {what} for {cost:.0} cr."));
        true
    }

    /// What the autopilot (and the headless balance runs) does when hailed.
    pub fn auto_resolve_encounter(&mut self) {
        let Some(e) = &self.encounter else { return };
        let choice = match e.stage {
            Stage::Won => Choice::Destroy,
            Stage::Approach if self.win_chance() >= 0.55 => Choice::Fight,
            Stage::Approach => Choice::Pay,
        };
        self.resolve_encounter(choice);
    }
}

pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::opts::GalaxyOpts;

    fn sim_with(risk: Risk) -> (Sim, Galaxy) {
        let opts = GalaxyOpts { risk, ..Default::default() };
        let mut gal = Galaxy::with_opts(42, opts);
        let mut sim = Sim::new_game(42, 1, &mut gal);
        sim.credits = 100_000.0;
        sim.hold.add(Good::Luxury, 20);
        (sim, gal)
    }

    fn hail(sim: &mut Sim, strength: f64) {
        sim.encounter = Some(Encounter { band: "the Test Jackals".into(), attack: strength, defense: strength, stage: Stage::Approach, bounty: 5_000.0 });
    }

    #[test]
    fn losses_respect_the_risk_caps() {
        for risk in Risk::ALL {
            let (mut sim, _gal) = sim_with(risk);
            let cap = match risk {
                Risk::Tourist => 0.5,
                _ => 0.75,
            };
            for i in 0..40 {
                let (cr, lux, h2) = (sim.credits, sim.hold.get(Good::Luxury), sim.inv.get(Res::H2));
                // Unbeatable pirates: every fight is a loss.
                hail(&mut sim, 1_000.0);
                sim.resolve_encounter(Choice::Fight);
                if sim.over.is_some() {
                    assert_eq!(risk, Risk::Risky, "only Risky can destroy the ship");
                    assert!(sim.battles_lost > 5, "never within the first five defeats");
                    break;
                }
                let took = 1.0 - sim.credits / cr;
                let cap_now = if risk == Risk::Risky && i >= 5 { 0.95 } else { cap };
                assert!(took <= cap_now + 1e-9, "{risk:?}: took {took} > {cap_now}");
                assert!(took >= 0.25 - 1e-9);
                assert!(sim.hold.get(Good::Luxury) as f64 >= (lux as f64 * (1.0 - cap_now)).floor());
                if risk != Risk::Risky {
                    assert_eq!(sim.inv.get(Res::H2), h2, "{risk:?} pirates never take fuel");
                }
                sim.credits = 100_000.0;
                sim.hold.add(Good::Luxury, 20);
            }
        }
    }

    #[test]
    fn a_win_pays_salvage_and_a_bounty_at_the_next_port() {
        let (mut sim, mut gal) = sim_with(Risk::Normal);
        let cr = sim.credits;
        hail(&mut sim, 0.01);
        sim.resolve_encounter(Choice::Fight);
        assert_eq!(sim.battles_won, 1);
        assert!(sim.credits > cr, "salvage includes credits");
        assert_eq!(sim.encounter.as_ref().map(|e| e.stage), Some(Stage::Won));
        sim.resolve_encounter(Choice::Destroy);
        assert!(sim.encounter.is_none());
        assert_eq!(sim.pending_bounty, 5_000.0);
        // No port at home: the bounty waits.
        sim.combat_tick(0.1, &mut gal);
        assert_eq!(sim.pending_bounty, 5_000.0);
        let id = sim.sys_id;
        gal.system(id).civ = Some(crate::sim::civ::Civ {
            tier: CivTier::T2,
            name: "Port".into(),
            kind: crate::sim::civ::CivKind::Colony,
            seat: crate::sim::Loc::Planet(0),
            small: false,
        });
        let before = sim.credits;
        sim.combat_tick(0.1, &mut gal);
        assert_eq!(sim.pending_bounty, 0.0);
        assert_eq!(sim.credits, before + 5_000.0);
    }

    #[test]
    fn paying_costs_the_tribute_share_and_ends_it() {
        let (mut sim, _gal) = sim_with(Risk::Normal);
        let cr = sim.credits;
        hail(&mut sim, 5.0);
        sim.resolve_encounter(Choice::Pay);
        assert!(sim.encounter.is_none());
        assert!((sim.credits - cr * 0.75).abs() < 1.0);
        assert_eq!(sim.battles_lost, 0);
    }

    #[test]
    fn pirates_turn_up_more_often_on_risky() {
        let count = |risk: Risk| {
            let (mut sim, mut gal) = sim_with(risk);
            // Away from home, where pirates hunt.
            sim.sys_id = gal.neighbors([0.0, 0.0], 30.0, gal.home_id())[0].id;
            let mut n = 0;
            for _ in 0..20_000 {
                sim.combat_tick(1.0, &mut gal);
                if sim.encounter.take().is_some() {
                    n += 1;
                }
            }
            n
        };
        let (t, n, r) = (count(Risk::Tourist), count(Risk::Normal), count(Risk::Risky));
        assert!(t < n && n < r, "tourist {t}, normal {n}, risky {r}");
        // Rare: on Normal, about one per 400 h (×2 with valuable cargo).
        assert!((20..=150).contains(&n), "normal {n} per 20,000 h");
    }
}
