//! Quests: relics found in ruins, and artifacts recovered for (or kept
//! from) the civilisations that ask for them.

use super::state::Sim;
use serde::{Deserialize, Serialize};

/// What a kept artifact does for you, permanently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Power {
    /// +15% scoop intake.
    Scoop,
    /// −25% hull damage taken.
    Shields,
    /// +1 attack in battle.
    Attack,
    /// The colony's tech grows 25% faster.
    ColonyTech,
    /// +5% colony health.
    ColonyHealth,
}

impl Power {
    pub fn describe(self) -> &'static str {
        match self {
            Power::Scoop => "+15% fuel-scoop intake",
            Power::Shields => "hull damage −25%",
            Power::Attack => "+1 Attack in battle",
            Power::ColonyTech => "colony tech grows 25% faster",
            Power::ColonyHealth => "+5% colony health",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub name: String,
    pub power: Power,
}

impl Sim {
    fn powers(&self, p: Power) -> usize {
        self.artifacts.iter().filter(|a| a.power == p).count()
    }

    pub fn artifact_attack(&self) -> f64 {
        self.powers(Power::Attack) as f64
    }
}
