//! Galaxy options chosen on the title screen. Together with the seed they
//! fully determine the galaxy: the same seed keeps the same star map, while
//! the options change what the systems hold (deposits, life, civilisations)
//! and how often things go wrong aboard.
//!
//! `GalaxyOpts::default()` is the game's original balance.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Abundance {
    Scarce,
    Normal,
    Abundant,
}

impl Abundance {
    pub const ALL: [Abundance; 3] = [Abundance::Scarce, Abundance::Normal, Abundance::Abundant];

    pub fn name(self) -> &'static str {
        match self {
            Abundance::Scarce => "Scarce",
            Abundance::Normal => "Normal",
            Abundance::Abundant => "Abundant",
        }
    }

    fn pick(self, s: f64, n: f64, a: f64) -> f64 {
        match self {
            Abundance::Scarce => s,
            Abundance::Normal => n,
            Abundance::Abundant => a,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Risk {
    Tourist,
    Normal,
    Risky,
}

impl Risk {
    pub const ALL: [Risk; 3] = [Risk::Tourist, Risk::Normal, Risk::Risky];

    pub fn name(self) -> &'static str {
        match self {
            Risk::Tourist => "Tourist",
            Risk::Normal => "Normal",
            Risk::Risky => "Risky",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Risk::Tourist => "Breakdowns, hazards and impacts are much rarer.",
            Risk::Normal => "The standard odds of things going wrong.",
            Risk::Risky => "Breakdowns, hazards and impacts come far more often.",
        }
    }

    /// Multiplier on the rate of negative random events.
    pub fn event_mult(self) -> f64 {
        match self {
            Risk::Tourist => 0.4,
            Risk::Normal => 1.0,
            Risk::Risky => 1.8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GalaxyOpts {
    pub resources: Abundance,
    pub civs: Abundance,
    pub risk: Risk,
}

impl Default for GalaxyOpts {
    fn default() -> Self {
        Self { resources: Abundance::Normal, civs: Abundance::Scarce, risk: Risk::Normal }
    }
}

impl GalaxyOpts {
    pub fn resources_blurb(a: Abundance) -> &'static str {
        match a {
            Abundance::Scarce => "Fewer, smaller deposits; slower drones and scoops.",
            Abundance::Normal => "The standard spread of ore, ice, carbon and nitrogen.",
            Abundance::Abundant => "Common, deep deposits; faster drones and scoops.",
        }
    }

    pub fn civs_blurb(a: Abundance) -> &'static str {
        match a {
            Abundance::Scarce => "Inhabited systems are rare: a lonely galaxy.",
            Abundance::Normal => "Several times more natives and colonies, and more of them trade.",
            Abundance::Abundant => "A crowded galaxy, rich in spacefaring peoples.",
        }
    }

    /// Multiplier on the chance that a world holds a given (optional) deposit.
    pub fn deposit_chance_mult(&self) -> f64 {
        self.resources.pick(0.6, 1.0, 1.8)
    }

    /// Multiplier on deposit tonnage.
    pub fn deposit_tons_mult(&self) -> f64 {
        self.resources.pick(0.5, 1.0, 2.0)
    }

    /// Multiplier on drone mining speed (ore grade).
    pub fn drone_mult(&self) -> f64 {
        self.resources.pick(0.75, 1.0, 1.4)
    }

    /// Multiplier on fuel-scoop yield.
    pub fn scoop_mult(&self) -> f64 {
        self.resources.pick(0.85, 1.0, 1.2)
    }

    /// Multiplier on the per-system colony chance.
    pub fn colony_mult(&self) -> f64 {
        self.civs.pick(1.0, 3.0, 9.0)
    }

    /// Multiplier on the odds that a living world evolves sentients.
    pub fn sentient_mult(&self) -> f64 {
        self.civs.pick(1.0, 3.0, 8.0)
    }

    /// Multiplier on the weights of spacefaring tech levels.
    pub fn advanced_tech_mult(&self) -> f64 {
        self.civs.pick(1.0, 1.5, 2.5)
    }

    pub fn summary(&self) -> String {
        format!("Resources {} · Civs {} · Risk {}", self.resources.name(), self.civs.name(), self.risk.name())
    }
}
