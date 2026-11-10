//! How livable a world is for human colonists.

use crate::sim::planet::{AtmoKind, Planet, PlanetClass, TempBand};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Hab {
    /// People live in the open.
    Habitable,
    /// Livable with masks and care: unsheltered colonists suffer a little.
    Marginal,
    /// Unsheltered colonists sicken and have few children.
    Hostile,
    /// Unsheltered colonists die.
    Lethal,
}

impl Hab {
    pub fn name(self) -> &'static str {
        match self {
            Hab::Habitable => "Habitable",
            Hab::Marginal => "Marginal",
            Hab::Hostile => "Hostile",
            Hab::Lethal => "Lethal",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Hab::Habitable => "People can live in the open. No domes needed.",
            Hab::Marginal => "Livable with masks and care. Colonists without a dome suffer minor health and birth-rate penalties.",
            Hab::Hostile => "Colonists without a dome sicken and have few children.",
            Hab::Lethal => "Colonists without a dome die.",
        }
    }

    /// Does anyone need a dome here?
    pub fn needs_domes(self) -> bool {
        self != Hab::Habitable
    }
}

/// Can a colony be founded on this body at all?
pub fn colonizable(p: &Planet) -> Result<(), &'static str> {
    match p.class {
        PlanetClass::GasGiant | PlanetClass::IceGiant => Err("A giant has no surface to build on. Try one of its moons."),
        PlanetClass::AsteroidBelt => Err("A belt is rubble, not a world."),
        _ if p.natives.as_ref().is_some_and(|n| n.population > 0.0) => Err("This world already belongs to its natives."),
        _ if p.colony.as_ref().is_some_and(|c| c.population > 0.0) => Err("Someone already lives here."),
        _ => Ok(()),
    }
}

pub fn habitability(p: &Planet) -> Hab {
    use AtmoKind::*;
    if matches!(p.atmosphere, Some(Corrosive) | Some(Infiltrating)) || p.temp_band == TempBand::Burning {
        return Hab::Lethal;
    }
    let gravity_ok = (0.4..=1.8).contains(&p.gravity);
    let mild = matches!(p.temp_band, TempBand::Cold | TempBand::Temperate);
    match (p.atmosphere, mild, gravity_ok) {
        (Some(Breathable), true, true) => Hab::Habitable,
        (Some(Breathable), false, _) | (Some(Breathable), _, false) => Hab::Marginal,
        (Some(Toxic) | Some(Thin), true, true) => Hab::Marginal,
        (Some(Dense), _, _) if p.temp_band >= TempBand::Hot => Hab::Lethal,
        _ => Hab::Hostile,
    }
}

/// Background death hazard per year from the world itself.
pub fn danger(p: &Planet) -> f64 {
    let mut d = 0.002;
    if p.life.is_some_and(|l| l.dangerous()) {
        d += 0.004;
    }
    if p.radiation() > 0.0 {
        d += 0.006;
    }
    if !(0.6..=1.4).contains(&p.gravity) {
        d += 0.002;
    }
    if p.temp_band == TempBand::Frozen || p.temp_band == TempBand::Hot {
        d += 0.002;
    }
    d
}

/// Raw materials in the colony world's deposits (tonnes).
pub fn richness_t(p: &Planet) -> f64 {
    p.deposits.iter().map(|d| d.tons).sum::<f64>()
}
