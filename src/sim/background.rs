//! The captain's background: starting money and two lasting bonuses.

use super::people::Sex;
use super::ship::ShipSpec;
use serde::{Deserialize, Serialize};

/// Who the player is, chosen when a new game starts.
#[derive(Debug, Clone, PartialEq)]
pub struct Captain {
    pub given: String,
    pub surname: String,
    pub sex: Sex,
    pub background: Background,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Background {
    #[default]
    NavalAcademy,
    MerchantFamily,
    AsteroidMiner,
    ShipsEngineer,
    FieldMedic,
    Smuggler,
    ColonialSurveyor,
    NobleHeir,
}

impl Background {
    pub const ALL: [Background; 8] = [
        Background::NavalAcademy,
        Background::MerchantFamily,
        Background::AsteroidMiner,
        Background::ShipsEngineer,
        Background::FieldMedic,
        Background::Smuggler,
        Background::ColonialSurveyor,
        Background::NobleHeir,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Background::NavalAcademy => "Naval Academy",
            Background::MerchantFamily => "Merchant Family",
            Background::AsteroidMiner => "Asteroid Miner",
            Background::ShipsEngineer => "Ship's Engineer",
            Background::FieldMedic => "Field Medic",
            Background::Smuggler => "Smuggler",
            Background::ColonialSurveyor => "Colonial Surveyor",
            Background::NobleHeir => "Noble Heir",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Background::NavalAcademy => "Top of your class in gunnery and damage control.",
            Background::MerchantFamily => "Raised on a trading barge; you know a margin when you see one.",
            Background::AsteroidMiner => "Years on the rock crews, running drones and smelters.",
            Background::ShipsEngineer => "You keep old hulls flying on spit and nanobots.",
            Background::FieldMedic => "Trained to keep people alive far from any hospital.",
            Background::Smuggler => "You know the quiet lanes, and who to pay off.",
            Background::ColonialSurveyor => "You have scouted a dozen worlds for settlers.",
            Background::NobleHeir => "Old money, and a name that opens doors.",
        }
    }

    /// Credits on top of the ship's own.
    pub fn money(self) -> f64 {
        match self {
            Background::NavalAcademy => 1_000.0,
            Background::MerchantFamily => 6_000.0,
            Background::AsteroidMiner => 500.0,
            Background::ShipsEngineer => 1_000.0,
            Background::FieldMedic => 1_500.0,
            Background::Smuggler => 3_000.0,
            Background::ColonialSurveyor => 2_000.0,
            Background::NobleHeir => 15_000.0,
        }
    }

    /// The two bonuses, for the screens.
    pub fn bonuses(self) -> [&'static str; 2] {
        match self {
            Background::NavalAcademy => ["Attack +1 level", "Defense +1 level"],
            Background::MerchantFamily => ["Buy 5% cheaper, sell 5% dearer", "Cargo hold +20%"],
            Background::AsteroidMiner => ["Drone yield +25%", "Fabrication +20%"],
            Background::ShipsEngineer => ["FTL wear −30%", "Hull self-repair +50%"],
            Background::FieldMedic => ["Crew health recovers 1.5× faster", "Colony infant and maternal deaths −25%"],
            Background::Smuggler => ["Pirate encounters −30%", "Pirate tribute −40%"],
            Background::ColonialSurveyor => ["Founding genetic penalty −40%", "Colony foraging and farming +25%"],
            Background::NobleHeir => ["Partner searches +20% likelier to succeed", "Recruiting costs −50%"],
        }
    }

    /// Ship numbers this background changes.
    pub fn apply(self, s: &mut ShipSpec) {
        match self {
            Background::MerchantFamily => s.cargo_t *= 1.2,
            Background::AsteroidMiner => {
                s.drone_yield *= 1.25;
                s.fab_rate *= 1.2;
            }
            Background::ShipsEngineer => s.wear_per_ly *= 0.7,
            _ => {}
        }
    }

    /// Starting (attack, defense) levels.
    pub fn combat_levels(self) -> (u8, u8) {
        if self == Background::NavalAcademy { (1, 1) } else { (0, 0) }
    }

    /// Multiplier on prices paid.
    pub fn buy_mult(self) -> f64 {
        if self == Background::MerchantFamily { 0.95 } else { 1.0 }
    }

    /// Multiplier on prices received.
    pub fn sell_mult(self) -> f64 {
        if self == Background::MerchantFamily { 1.05 } else { 1.0 }
    }

    pub fn hull_repair_mult(self) -> f64 {
        if self == Background::ShipsEngineer { 1.5 } else { 1.0 }
    }

    pub fn crew_recovery_mult(self) -> f64 {
        if self == Background::FieldMedic { 1.5 } else { 1.0 }
    }

    /// Multiplier on infant and maternal deaths in the colony.
    pub fn birth_death_mult(self) -> f64 {
        if self == Background::FieldMedic { 0.75 } else { 1.0 }
    }

    pub fn pirate_rate_mult(self) -> f64 {
        if self == Background::Smuggler { 0.7 } else { 1.0 }
    }

    pub fn tribute_mult(self) -> f64 {
        if self == Background::Smuggler { 0.6 } else { 1.0 }
    }

    pub fn genetic_mult(self) -> f64 {
        if self == Background::ColonialSurveyor { 0.6 } else { 1.0 }
    }

    pub fn land_mult(self) -> f64 {
        if self == Background::ColonialSurveyor { 1.25 } else { 1.0 }
    }

    /// Added to a partner search's success chance.
    pub fn recruit_chance_bonus(self) -> f64 {
        if self == Background::NobleHeir { 0.2 } else { 0.0 }
    }

    pub fn recruit_cost_mult(self) -> f64 {
        if self == Background::NobleHeir { 0.5 } else { 1.0 }
    }
}
