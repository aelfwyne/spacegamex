//! Commercial cargo: colony supplies and trade goods, counted in units and
//! carried in the ship's cargo hold (separate from the resource tanks).
//!
//! Every system with a civilisation has a cargo market. What it sells and
//! buys depends on who lives there: pre-space natives barter handicrafts,
//! biologicals and captives; planetary civilisations sell basic colony
//! supplies; interstellar stations sell everything. Prices vary per system,
//! so carrying goods between them pays.

use super::civ::{Civ, CivKind, CivTier};
use crate::rngx::derive;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Good {
    // ---- colony supplies ----
    HydroKit,
    Recycler,
    ResDrones,
    AiCluster,
    PowerGen,
    MedKit,
    HazardDome,
    GeneKit,
    // ---- trade goods ----
    Machinery,
    Electronics,
    Pharma,
    Luxury,
    Textiles,
    Artworks,
    Biologicals,
    Slaves,
}

/// What a colony supply does: one unit serves `per` colonists for
/// `life_y` years.
#[derive(Debug, Clone, Copy)]
pub struct SupplySpec {
    pub per: f64,
    pub life_y: f64,
}

impl Good {
    pub const ALL: [Good; 16] = [
        Good::HydroKit,
        Good::Recycler,
        Good::ResDrones,
        Good::AiCluster,
        Good::PowerGen,
        Good::MedKit,
        Good::HazardDome,
        Good::GeneKit,
        Good::Machinery,
        Good::Electronics,
        Good::Pharma,
        Good::Luxury,
        Good::Textiles,
        Good::Artworks,
        Good::Biologicals,
        Good::Slaves,
    ];

    /// Supplies a colony installs and wears out (Genetic Modification Kits
    /// are used once, at founding, and are not listed here).
    pub const SUPPLIES: [Good; 7] = [
        Good::HydroKit,
        Good::Recycler,
        Good::ResDrones,
        Good::AiCluster,
        Good::PowerGen,
        Good::MedKit,
        Good::HazardDome,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Good::HydroKit => "Hydroponics Kit",
            Good::Recycler => "Recycling Center",
            Good::ResDrones => "Resource Drones",
            Good::AiCluster => "AI Cluster",
            Good::PowerGen => "Power Generator",
            Good::MedKit => "Medical Kit",
            Good::HazardDome => "Temporary Hazard Dome",
            Good::GeneKit => "Genetic Modification Kit",
            Good::Machinery => "Machinery",
            Good::Electronics => "Electronics",
            Good::Pharma => "Pharmaceuticals",
            Good::Luxury => "Luxury Goods",
            Good::Textiles => "Textiles",
            Good::Artworks => "Alien Artworks",
            Good::Biologicals => "Exotic Biologicals",
            Good::Slaves => "Alien Slaves",
        }
    }

    pub fn desc(self) -> &'static str {
        match self {
            Good::HydroKit => "Grows food for 100 colonists. Lasts 50 years.",
            Good::Recycler => "Recycles air, water and waste for 100 colonists. Lasts 100 years.",
            Good::ResDrones => "Mines and hauls raw materials for 200 colonists. Lasts 25 years.",
            Good::AiCluster => "Keeps the knowledge base of 1,000 colonists. Without them a colony's tech level collapses. Lasts 25 years.",
            Good::PowerGen => "Powers 500 colonists. Without power every other supply works badly. Lasts 50 years.",
            Good::MedKit => "Medical care for 10 colonists. Lasts 1 year.",
            Good::HazardDome => "Shelters 25 colonists on a world they cannot live on unprotected. Lasts 50 years.",
            Good::GeneKit => "Lets a human and a native partner have children: the colony becomes Hybrid.",
            Good::Machinery => "Industrial machinery. Wanted everywhere that builds.",
            Good::Electronics => "Processors, sensors and control systems.",
            Good::Pharma => "Medicines and biochemicals.",
            Good::Luxury => "Fine goods for wealthy markets.",
            Good::Textiles => "Cloth and fibre. Cheap, bulky, always sells.",
            Good::Artworks => "Handicrafts of pre-space peoples. Collectors pay well.",
            Good::Biologicals => "Rare plants, spores and compounds from living worlds.",
            Good::Slaves => "Captives bought from pre-space natives. They can work a colony, at a heavy cost to its happiness and health, and cannot have children with humans unless the colony is Hybrid. No spacefaring market will buy them.",
        }
    }

    /// Mass per unit (tonnes).
    pub fn mass_t(self) -> f64 {
        match self {
            Good::HydroKit => 4.0,
            Good::Recycler => 5.0,
            Good::ResDrones => 3.0,
            Good::AiCluster => 2.0,
            Good::PowerGen => 8.0,
            Good::MedKit => 0.2,
            Good::HazardDome => 3.0,
            Good::GeneKit => 0.5,
            Good::Slaves => 0.2,
            _ => 1.0,
        }
    }

    /// Galactic base price per unit (credits).
    pub fn base_price(self) -> f64 {
        match self {
            Good::HydroKit => 3_000.0,
            Good::Recycler => 4_000.0,
            Good::ResDrones => 2_500.0,
            Good::AiCluster => 15_000.0,
            Good::PowerGen => 6_000.0,
            Good::MedKit => 150.0,
            Good::HazardDome => 2_000.0,
            Good::GeneKit => 10_000.0,
            Good::Machinery => 900.0,
            Good::Electronics => 2_500.0,
            Good::Pharma => 4_000.0,
            Good::Luxury => 6_000.0,
            Good::Textiles => 400.0,
            Good::Artworks => 5_000.0,
            Good::Biologicals => 3_000.0,
            Good::Slaves => 1_500.0,
        }
    }

    pub fn is_supply(self) -> bool {
        Good::SUPPLIES.contains(&self) || self == Good::GeneKit
    }

    pub fn supply(self) -> Option<SupplySpec> {
        let (per, life_y) = match self {
            Good::HydroKit => (100.0, 50.0),
            Good::Recycler => (100.0, 100.0),
            Good::ResDrones => (200.0, 25.0),
            Good::AiCluster => (1_000.0, 25.0),
            Good::PowerGen => (500.0, 50.0),
            Good::MedKit => (10.0, 1.0),
            Good::HazardDome => (25.0, 50.0),
            _ => return None,
        };
        Some(SupplySpec { per, life_y })
    }

    /// Units needed to serve `colonists` fully.
    pub fn units_for(self, colonists: f64) -> u32 {
        self.supply().map(|s| (colonists / s.per).ceil().max(0.0) as u32).unwrap_or(0)
    }
}

impl std::fmt::Display for Good {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// The ship's cargo hold: units per good.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Hold {
    pub units: BTreeMap<Good, u32>,
}

impl Hold {
    pub fn get(&self, g: Good) -> u32 {
        self.units.get(&g).copied().unwrap_or(0)
    }

    pub fn add(&mut self, g: Good, n: u32) {
        if n > 0 {
            *self.units.entry(g).or_insert(0) += n;
        }
    }

    /// Remove up to `n`; returns how many were removed.
    pub fn take(&mut self, g: Good, n: u32) -> u32 {
        let have = self.get(g);
        let k = n.min(have);
        if k == have {
            self.units.remove(&g);
        } else if k > 0 {
            self.units.insert(g, have - k);
        }
        k
    }

    pub fn mass_t(&self) -> f64 {
        self.units.iter().map(|(g, n)| g.mass_t() * *n as f64).sum()
    }

    /// Market value at base prices.
    pub fn value(&self) -> f64 {
        self.units.iter().map(|(g, n)| g.base_price() * *n as f64).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }
}

/// One good on a local market.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offer {
    /// Price per unit here before the buy/sell spread.
    pub price: f64,
    /// Units for sale (0 = they buy it but sell none).
    pub stock: u32,
    pub sells: bool,
    pub buys: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CargoMarket {
    pub offers: BTreeMap<Good, Offer>,
}

/// Sellers keep this share of the price as their margin: you pay `price`
/// and receive `price * SELL_SPREAD`.
pub const SELL_SPREAD: f64 = 0.8;

impl CargoMarket {
    pub fn buy_price(&self, g: Good) -> Option<f64> {
        self.offers.get(&g).filter(|o| o.sells && o.stock > 0).map(|o| o.price)
    }

    pub fn sell_price(&self, g: Good) -> Option<f64> {
        self.offers.get(&g).filter(|o| o.buys).map(|o| o.price * SELL_SPREAD)
    }

    /// Generate the market for a civilisation (deterministic per system).
    /// `size` is the population scale 0..1 (log of the population).
    pub fn generate(seed: u64, sys_id: u64, civ: &Civ, size: f64) -> CargoMarket {
        let mut rng = derive(seed ^ sys_id.rotate_left(17), 0x000c_a790);
        let mut offers = BTreeMap::new();
        let natives = civ.kind == CivKind::Natives;
        let stock_scale = (0.4 + size) * if civ.small { 0.5 } else { 1.0 };
        // (good, sells, buys, price factor, typical stock)
        let rows: Vec<(Good, bool, bool, f64, f64)> = match civ.tier {
            CivTier::T1 => {
                if !natives {
                    // Survey posts and fallen colonies: they want parts.
                    vec![
                        (Good::Machinery, false, true, 1.3, 0.0),
                        (Good::Electronics, false, true, 1.3, 0.0),
                        (Good::MedKit, false, true, 1.4, 0.0),
                    ]
                } else {
                    vec![
                        (Good::Artworks, true, false, 0.45, 12.0),
                        (Good::Biologicals, true, false, 0.5, 15.0),
                        (Good::Slaves, true, false, 1.0, 40.0),
                        (Good::Textiles, false, true, 1.5, 0.0),
                        (Good::Machinery, false, true, 1.6, 0.0),
                        (Good::Electronics, false, true, 1.4, 0.0),
                        (Good::Luxury, false, true, 1.1, 0.0),
                    ]
                }
            }
            CivTier::T2 => {
                let mut v = vec![
                    (Good::MedKit, true, true, 1.1, 120.0),
                    (Good::HydroKit, true, true, 1.15, 6.0),
                    (Good::HazardDome, true, true, 1.15, 10.0),
                    (Good::Recycler, true, true, 1.2, 4.0),
                    (Good::Textiles, true, true, 0.8, 40.0),
                    (Good::Machinery, true, true, 1.0, 20.0),
                    (Good::Electronics, false, true, 1.2, 0.0),
                    (Good::Pharma, false, true, 1.25, 0.0),
                    (Good::Luxury, false, true, 1.2, 0.0),
                    (Good::Artworks, false, true, 1.0, 0.0),
                    (Good::Biologicals, false, true, 1.05, 0.0),
                ];
                if natives {
                    v.push((Good::Biologicals, true, true, 0.7, 10.0));
                }
                v
            }
            CivTier::T3 => vec![
                (Good::HydroKit, true, true, 1.0, 20.0),
                (Good::Recycler, true, true, 1.0, 15.0),
                (Good::ResDrones, true, true, 1.0, 12.0),
                (Good::AiCluster, true, true, 1.0, 4.0),
                (Good::PowerGen, true, true, 1.0, 8.0),
                (Good::MedKit, true, true, 0.9, 400.0),
                (Good::HazardDome, true, true, 1.0, 40.0),
                (Good::GeneKit, true, true, 1.0, 3.0),
                (Good::Machinery, true, true, 0.85, 60.0),
                (Good::Electronics, true, true, 0.85, 40.0),
                (Good::Pharma, true, true, 0.9, 25.0),
                (Good::Luxury, true, true, 0.95, 10.0),
                (Good::Textiles, true, true, 1.0, 50.0),
                (Good::Artworks, false, true, 1.35, 0.0),
                (Good::Biologicals, false, true, 1.3, 0.0),
            ],
        };
        for (g, sells, buys, factor, typical) in rows {
            let price = g.base_price() * factor * rng.random_range(0.8..1.25);
            let stock = if sells { (typical * stock_scale * rng.random_range(0.5..1.5)).round().max(1.0) as u32 } else { 0 };
            offers.insert(g, Offer { price, stock, sells, buys });
        }
        CargoMarket { offers }
    }
}

/// The colony kit a ship carries for `colonists` founders (one unit of each
/// supply at minimum, domes only if asked).
pub fn colony_kit(colonists: f64, domes: bool) -> Vec<(Good, u32)> {
    Good::SUPPLIES
        .iter()
        .filter(|g| domes || **g != Good::HazardDome)
        .map(|g| (*g, g.units_for(colonists).max(1)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kit_masses_match_the_ship_classes() {
        let mass = |kit: Vec<(Good, u32)>| kit.iter().map(|(g, n)| g.mass_t() * *n as f64).sum::<f64>();
        // A ten-person kit fits the Cargomaster with room for trade.
        assert!(mass(colony_kit(10.0, true)) < 40.0);
        // The Keelclipper cannot carry even a two-person kit in one go.
        assert!(mass(colony_kit(2.0, true)) > 8.0);
        // The Ark's full kit fits its hold.
        assert!(mass(colony_kit(510.0, true)) < 300.0);
    }

    #[test]
    fn hold_take_never_underflows() {
        let mut h = Hold::default();
        h.add(Good::MedKit, 3);
        assert_eq!(h.take(Good::MedKit, 5), 3);
        assert_eq!(h.get(Good::MedKit), 0);
        assert!(h.is_empty());
    }
}
