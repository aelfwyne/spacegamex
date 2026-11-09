//! A full star system: star + planets + optional civilization + trade.

use super::civ::{base_price, Civ, CivTier, T2_STOCK};
use super::planet::{Planet, PlanetClass};
use super::res::Res;
use super::star::{Star, StarClass};
use crate::rngx::derive;
use rand::RngExt;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarSystem {
    pub id: u64,
    pub pos: [f64; 2], // ly
    pub star: Star,
    pub planets: Vec<Planet>,
    pub civ: Option<Civ>,
    /// Trade stock still available (kg). T3 is effectively deep.
    pub trade_stock: BTreeMap<Res, f64>,
    /// Local market price (credits/kg).
    pub trade_prices: BTreeMap<Res, f64>,
}

impl StarSystem {
    pub fn generate(seed: u64, id: u64, pos: [f64; 2], is_home: bool) -> Self {
        let mut rng = derive(seed, id.wrapping_add(0x5eed_b10c));

        let star = if is_home {
            home_star(&mut rng)
        } else {
            Star::generate(&mut rng)
        };

        let n_planets = if is_home {
            6
        } else if star.giant {
            rng.random_range(1..=3)
        } else {
            let base = (1 + (star.mass * 2.5) as usize).min(8);
            let extra = if rng.random_bool(0.35) { 1 } else { 0 };
            (base + extra).min(8)
        };

        let mut planets: Vec<Planet> = (0..n_planets)
            .map(|i| super::planet::generate(&mut rng, &star, i, &star.name))
            .collect();

        if is_home {
            curate_home(&mut planets);
            // curate_home sorts by orbit; renumber so the Roman numerals
            // follow orbital order again (I = innermost).
            for (i, p) in planets.iter_mut().enumerate() {
                p.name = super::planet::planet_name(&star.name, i);
            }
        }

        let civ = if is_home {
            None
        } else if rng.random_bool(0.02) {
            // ~1 in 50 systems is inhabited
            Some(Civ::generate(&mut rng))
        } else {
            None
        };

        let (trade_prices, trade_stock) = match civ.as_ref().map(|c| c.tier) {
            Some(CivTier::T3) => {
                let mut prices = BTreeMap::new();
                let mut stock = BTreeMap::new();
                for r in Res::ALL {
                    prices.insert(r, base_price(r) * rng.random_range(0.7..1.4));
                    stock.insert(r, 2_000_000.0);
                }
                (prices, stock)
            }
            Some(CivTier::T2) => {
                let mut prices = BTreeMap::new();
                let mut stock = BTreeMap::new();
                for r in Res::ALL {
                    prices.insert(r, base_price(r) * rng.random_range(0.7..1.4));
                }
                for (r, amt) in T2_STOCK {
                    stock.insert(r, amt * rng.random_range(0.5..1.5));
                }
                (prices, stock)
            }
            _ => (BTreeMap::new(), BTreeMap::new()),
        };

        StarSystem {
            id,
            pos,
            star,
            planets,
            civ,
            trade_stock,
            trade_prices,
        }
    }

    /// Resource "richness" score 0..100 — used for destination choice and map display.
    pub fn richness(&self) -> f64 {
        let mut s = 0.0;
        if self.planets.iter().any(|p| p.class == PlanetClass::GasGiant) {
            s += 30.0;
        }
        for p in &self.planets {
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

/// The player's starting star: a friendly G dwarf with a guaranteed workable
/// set of worlds (fuel, ice, metals, carbon, nitrogen, gas scoop).
fn home_star(rng: &mut StdRng) -> Star {
    let catalog = format!("HD {}", rng.random_range(1_000..999_999));
    Star {
        name: crate::rngx::name(rng),
        catalog,
        class: StarClass::G,
        giant: false,
        mass: 0.98,
        radius: 0.97,
        lum: 0.96,
        temp: 5_700.0,
        age: 4.5,
        metallicity: 0.0,
    }
}

fn ensure_deposits(p: &mut Planet, want: &[(Res, f64)]) {
    for (r, min_tons) in want {
        let has = p.deposits.iter().find(|d| d.res == *r).map(|d| d.tons).unwrap_or(0.0);
        if has < *min_tons {
            if let Some(d) = p.deposits.iter_mut().find(|d| d.res == *r) {
                d.tons = *min_tons;
            } else {
                p.deposits.push(super::planet::Deposit { res: *r, tons: *min_tons });
            }
        }
    }
}

fn curate_home(planets: &mut [Planet]) {
    // Sort by distance so we can reason about inner/outer.
    planets.sort_by(|a, b| a.au.total_cmp(&b.au));

    let last = planets.len() - 1;

    // 1) Guarantee a gas giant (fuel + N2 scoop) — convert the outermost world.
    if !planets.iter().any(|p| p.class == PlanetClass::GasGiant) {
        let p = &mut planets[last];
        p.class = PlanetClass::GasGiant;
        p.mass = 635.6;
        p.radius = 11.2;
        p.bulk = "H/He envelope, metallic H".into();
        p.deposits.clear();
        p.atmo = Some(super::planet::Atmo { pressure_bar: 0.0, n2: 0.0, o2: 0.0, co2: 0.0 });
        p.moons = 7;
    }

    // 2) Guarantee a big ice world with N2 and carbon — convert the coldest
    //    non-giant (the second-outermost).
    let ice_idx = if planets[last].class == PlanetClass::GasGiant && planets.len() > 1 {
        planets.len() - 2
    } else {
        last
    };
    let has_ice = planets.iter().any(|p| {
        p.deposits.iter().any(|d| d.res == Res::Ice && d.tons >= 20_000.0)
    });
    if !has_ice {
        let p = &mut planets[ice_idx];
        p.class = PlanetClass::IceGiant;
        p.mass = 30.0;
        p.radius = 3.6;
        p.bulk = "H/He + water/ammonia ices".into();
        p.moons = 5;
        p.deposits = vec![
            super::planet::Deposit { res: Res::Ice, tons: 320_000.0 },
            super::planet::Deposit { res: Res::Nitrogen, tons: 60_000.0 },
            super::planet::Deposit { res: Res::Carbon, tons: 25_000.0 },
        ];
    }

    // 3) Guarantee workable metals + silicates + carbon on inner rocks.
    let mut metal_ok = false;
    for (i, p) in planets.iter_mut().take(3).enumerate() {
        if !p.class.mineable() {
            continue;
        }
        ensure_deposits(p, &[(Res::Metal, 8_000.0), (Res::Silicate, 8_000.0)]);
        if i < 2 {
            ensure_deposits(p, &[(Res::Carbon, 5_000.0)]);
        }
        metal_ok = true;
    }
    if !metal_ok {
        // No rocky inner worlds at all — force-convert the first world.
        let p = &mut planets[0];
        p.class = PlanetClass::Terrestrial;
        p.deposits = vec![
            super::planet::Deposit { res: Res::Metal, tons: 40_000.0 },
            super::planet::Deposit { res: Res::Silicate, tons: 40_000.0 },
            super::planet::Deposit { res: Res::Carbon, tons: 10_000.0 },
        ];
    }
}
