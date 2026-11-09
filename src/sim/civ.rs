//! Civilization tiers and trade.

use crate::rngx::name;
use super::res::Res;
use rand::RngExt;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Serialize, Deserialize)]
pub enum CivTier {
    /// Pre-space. No interaction beyond observation.
    T1,
    /// Space age, but no FTL: local markets only, limited stock, bad prices.
    T2,
    /// Interstellar (space-age+): full resupply station and trade.
    T3,
}

impl CivTier {
    pub fn name(&self) -> &'static str {
        match self {
            CivTier::T1 => "Pre-space civilization",
            CivTier::T2 => "Planetary civilization (no FTL)",
            CivTier::T3 => "Interstellar civilization (trade-capable)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Civ {
    pub tier: CivTier,
    pub name: String,
}

impl Civ {
    pub fn generate(rng: &mut StdRng) -> Civ {
        let tier = match rng.random::<f64>() {
            r if r < 0.50 => CivTier::T1,
            r if r < 0.85 => CivTier::T2,
            _ => CivTier::T3,
        };
        Civ { tier, name: name(rng) }
    }
}

/// Base market price in credits per kg.
pub fn base_price(r: Res) -> f64 {
    match r {
        Res::H2 => 2.0,
        Res::Ice => 0.5,
        Res::Metal => 1.5,
        Res::Silicate => 0.8,
        Res::Carbon => 1.2,
        Res::Nitrogen => 1.0,
        Res::Water => 3.0,
        Res::Oxygen => 8.0,
        Res::Food => 12.0,
        Res::Mat => 6.0,
        Res::Med => 40.0,
    }
}

/// What a T2 (planetary) civ can sell you, in kg of stock.
pub const T2_STOCK: [(Res, f64); 4] = [
    (Res::Food, 60_000.0),
    (Res::Water, 60_000.0),
    (Res::Med, 2_500.0),
    (Res::Mat, 12_000.0),
];
