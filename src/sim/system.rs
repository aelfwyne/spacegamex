//! A full star system: star (and companion), bodies, civilisation, trade.

use super::civ::Civ;
use super::planet::{Host, Planet, PlanetClass};
use super::res::Res;
use super::star::Star;
use super::state::Loc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryLayout {
    Close,
    Intermediate,
    Wide,
    Distant,
}

impl BinaryLayout {
    pub fn name(&self) -> &'static str {
        match self {
            BinaryLayout::Close => "Close",
            BinaryLayout::Intermediate => "Intermediate",
            BinaryLayout::Wide => "Wide",
            BinaryLayout::Distant => "Distant",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            BinaryLayout::Close => "The two stars orbit each other tightly. Planets circle both stars together and cannot exist inside the circumbinary limit; in the sky the stars appear as a close pair that rise and set together.",
            BinaryLayout::Intermediate => "The companion orbits just beyond the planets. Its gravity has stripped away every world past the primary's stability limit, and it shines as a brilliant second sun that changes in brightness over its eccentric orbit.",
            BinaryLayout::Wide => "Each star keeps its own planets, trimmed where the other star's pull takes over. From either system the other star is a dazzling point of light, bright enough to cast shadows.",
            BinaryLayout::Distant => "The companion is so distant it barely affects the planets and may have a full system of its own. It appears as the brightest star in the night sky.",
        }
    }
}

/// A binary companion (star B) and the orbital stability limits it sets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Companion {
    pub star: Star,
    pub layout: BinaryLayout,
    /// P-type: planets circle both stars. Otherwise S-type.
    pub p_type: bool,
    pub separation_au: f64,
    pub ecc: f64,
    /// S-type: outermost stable orbit around A / around B (AU).
    pub limit_a: f64,
    pub limit_b: f64,
    /// P-type: innermost stable circumbinary orbit (AU).
    pub circumbinary: f64,
    pub mass_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarSystem {
    pub id: u64,
    pub pos: [f64; 2], // ly
    pub star: Star,
    pub companion: Option<Companion>,
    pub planets: Vec<Planet>,
    pub civ: Option<Civ>,
    /// Trade stock still available (kg). T3 is effectively deep.
    pub trade_stock: BTreeMap<Res, f64>,
    /// Local market price (credits/kg).
    pub trade_prices: BTreeMap<Res, f64>,
    /// Commercial cargo market (any civilisation, even pre-space natives).
    #[serde(default)]
    pub market: Option<super::cargo::CargoMarket>,
}

impl StarSystem {
    pub fn generate(seed: u64, id: u64, pos: [f64; 2], is_home: bool, opts: &super::opts::GalaxyOpts) -> Self {
        super::worldgen::generate(seed, id, pos, is_home, opts)
    }

    /// Population behind the local civilisation (natives or colony).
    pub fn civ_population(&self) -> f64 {
        let Some(civ) = &self.civ else { return 0.0 };
        self.site(civ.seat)
            .map(|p| match civ.kind {
                super::civ::CivKind::Natives => p.natives.as_ref().map(|n| n.population).unwrap_or(0.0),
                super::civ::CivKind::Colony => p.colony.as_ref().map(|c| c.population).unwrap_or(0.0),
            })
            .unwrap_or(0.0)
    }

    /// (Re)build the cargo market from the civilisation, if there is none
    /// yet (new systems, and systems from saves made before markets).
    pub fn refresh_cargo_market(&mut self, seed: u64) {
        if self.market.is_some() {
            return;
        }
        if let Some(civ) = &self.civ {
            // Population scale: ~0 for a hamlet, ~1 for a billion.
            let size = (self.civ_population().max(1.0).log10() / 9.0).clamp(0.0, 1.0);
            self.market = Some(super::cargo::CargoMarket::generate(seed, self.id, civ, size));
        }
    }

    /// Every dockable body: planets, belts, and the moons of gas giants.
    pub fn sites(&self) -> Vec<(Loc, &Planet)> {
        let mut v = Vec::new();
        for (i, p) in self.planets.iter().enumerate() {
            v.push((Loc::Planet(i), p));
            for (m, s) in p.satellites.iter().enumerate() {
                v.push((Loc::Moon(i, m), s));
            }
        }
        v
    }

    pub fn site(&self, loc: Loc) -> Option<&Planet> {
        match loc {
            Loc::Planet(i) => self.planets.get(i),
            Loc::Moon(i, m) => self.planets.get(i).and_then(|p| p.satellites.get(m)),
            _ => None,
        }
    }

    pub fn site_mut(&mut self, loc: Loc) -> Option<&mut Planet> {
        match loc {
            Loc::Planet(i) => self.planets.get_mut(i),
            Loc::Moon(i, m) => self.planets.get_mut(i).and_then(|p| p.satellites.get_mut(m)),
            _ => None,
        }
    }

    /// Position along the system's radial line (AU from star A), the model
    /// shuttle distances use. Star B sits at the binary separation; its
    /// planets lie beyond it.
    pub fn pos_au(&self, loc: Loc) -> f64 {
        let sep = self.companion.as_ref().filter(|c| !c.p_type).map(|c| c.separation_au).unwrap_or(0.0);
        match loc {
            Loc::Star => 0.0,
            Loc::Companion => sep,
            _ => match self.site(loc) {
                Some(p) if p.host == Host::B => sep + p.au,
                Some(p) => p.au,
                None => 1.0,
            },
        }
    }

    /// Shuttle distance between two places in the system (AU).
    pub fn dist_au(&self, a: Loc, b: Loc) -> f64 {
        (self.pos_au(a) - self.pos_au(b)).abs()
    }

    /// A star the ship can dock at for scooping (S-type companions only:
    /// a close pair is scooped together from `Loc::Star`).
    pub fn companion_dockable(&self) -> bool {
        self.companion.as_ref().is_some_and(|c| !c.p_type)
    }

    /// Stellar wind (scoop multiplier) at a stellar location.
    pub fn scoop_at(&self, loc: Loc) -> f64 {
        match (loc, &self.companion) {
            (Loc::Star, Some(c)) if c.p_type => self.star.scoop_base() + c.star.scoop_base(),
            (Loc::Star, _) => self.star.scoop_base(),
            (Loc::Companion, Some(c)) => c.star.scoop_base(),
            _ => 0.0,
        }
    }

    /// Stellar radiation where the ship is (0 between the worlds).
    pub fn star_radiation_at(&self, loc: Loc) -> f64 {
        match (loc, &self.companion) {
            (Loc::Star, Some(c)) if c.p_type => self.star.radiation() + c.star.radiation(),
            (Loc::Star, _) => self.star.radiation(),
            (Loc::Companion, Some(c)) => c.star.radiation(),
            _ => 0.0,
        }
    }

    pub fn host_star(&self, host: Host) -> &Star {
        match (host, &self.companion) {
            (Host::B, Some(c)) => &c.star,
            _ => &self.star,
        }
    }

    /// Resource "richness" score 0..100 — used for destination choice and map display.
    pub fn richness(&self) -> f64 {
        let mut s = 0.0;
        if self.planets.iter().any(|p| p.class == PlanetClass::GasGiant) {
            s += 30.0;
        }
        for (_, p) in self.sites() {
            for d in &p.deposits {
                if d.tons >= 1000.0 {
                    s += 5.0;
                }
                if d.tons >= 100_000.0 {
                    s += 5.0;
                }
            }
        }
        s += self.star.scoop_base() * 15.0;
        s.min(100.0)
    }
}
