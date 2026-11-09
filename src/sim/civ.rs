//! Civilisation tiers and trade. A system's civilisation is derived from
//! what the world generator put there — native species or a colony — not
//! rolled on its own (see `worldgen::derive_civ`).

use super::res::Res;
use super::state::Loc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Serialize, Deserialize)]
pub enum CivTier {
    /// Pre-space (or collapsed / fallen). No interaction beyond observation.
    T1,
    /// Space age natives or a working colony: a local market, limited stock.
    T2,
    /// Interstellar natives or an industrial colony: full resupply station.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CivKind {
    Natives,
    Colony,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Civ {
    pub tier: CivTier,
    /// Species name (natives) or colony name.
    pub name: String,
    pub kind: CivKind,
    /// The world they live on.
    pub seat: Loc,
    /// A small outpost market (half stock).
    pub small: bool,
}

impl Civ {
    /// "the Zaquel" / "Fort Kyra".
    pub fn display(&self) -> String {
        match self.kind {
            CivKind::Natives => format!("the {}", self.name),
            CivKind::Colony => self.name.clone(),
        }
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
