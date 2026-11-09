//! Planets and their resource deposits.

use super::res::Res;
use super::star::Star;
use rand::RngExt;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanetClass {
    Terrestrial,
    SuperEarth,
    GasGiant,
    IceGiant,
}

impl PlanetClass {
    pub fn name(&self) -> &'static str {
        match self {
            PlanetClass::Terrestrial => "Terrestrial",
            PlanetClass::SuperEarth => "Super-Earth",
            PlanetClass::GasGiant => "Gas giant",
            PlanetClass::IceGiant => "Ice giant",
        }
    }

    /// Whether drones can mine solid deposits here.
    pub fn mineable(&self) -> bool {
        !matches!(self, PlanetClass::GasGiant)
    }

    /// Whether the atmospheric scoop can work here (gas/ice giants).
    pub fn scoopable(&self) -> bool {
        matches!(self, PlanetClass::GasGiant | PlanetClass::IceGiant)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Atmo {
    pub pressure_bar: f64,
    pub n2: f64,
    pub o2: f64,
    pub co2: f64,
}

impl Atmo {
    pub fn describe(&self) -> String {
        let main = if self.co2 > self.n2 && self.co2 > self.o2 {
            "CO2"
        } else if self.o2 > self.n2 {
            "O2"
        } else {
            "N2"
        };
        let mut s = format!("{}-dominated, {:.2} bar", main, self.pressure_bar);
        if self.o2 > 0.05 {
            s.push_str(&format!(", {:.0}% O2", self.o2 * 100.0));
        }
        s
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deposit {
    pub res: Res,
    /// Remaining amount in metric tons.
    pub tons: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Planet {
    pub name: String,
    pub class: PlanetClass,
    pub mass: f64,   // Earth masses
    pub radius: f64, // Earth radii
    pub au: f64,
    pub period: f64, // years
    pub temp: f64,   // K
    pub gravity: f64, // g (Earth = 1)
    pub atmo: Option<Atmo>,
    pub bulk: String,
    pub moons: usize,
    pub deposits: Vec<Deposit>,
    /// Initial orbital phase so the map looks alive and is deterministic.
    pub phase: f64, // radians
}

impl Planet {
    pub fn exhausted(&self) -> bool {
        self.deposits.iter().all(|d| d.tons < 1.0)
    }
}

fn log10_rng(rng: &mut StdRng, lo: f64, hi: f64) -> f64 {
    10f64.powf(lo + (hi - lo) * rng.random::<f64>())
}

/// Scale factor for a star's habitable zone — used to place planets.
const SLOT_AU: [f64; 8] = [0.22, 0.45, 0.9, 1.8, 3.6, 7.2, 13.0, 22.0];

pub fn generate(
    rng: &mut StdRng,
    star: &Star,
    index: usize,
    star_name: &str,
) -> Planet {
    let hab = star.habitable_au();
    let slot = SLOT_AU[index % SLOT_AU.len()];
    let au = (slot * hab) * rng.random_range(0.75..1.3);

    // Class by location: inner = rocky, outer = giants.
    let ratio = au / hab.max(0.2);
    let class = if ratio < 0.75 {
        if rng.random_bool(0.25) {
            PlanetClass::SuperEarth
        } else {
            PlanetClass::Terrestrial
        }
    } else if ratio < 2.2 {
        match rng.random::<f64>() {
            r if r < 0.5 => PlanetClass::Terrestrial,
            r if r < 0.8 => PlanetClass::SuperEarth,
            r if r < 0.93 => PlanetClass::IceGiant,
            _ => PlanetClass::GasGiant,
        }
    } else {
        if rng.random_bool(0.7) {
            PlanetClass::GasGiant
        } else {
            PlanetClass::IceGiant
        }
    };

    let (mass, radius): (f64, f64) = match class {
        PlanetClass::Terrestrial => {
            let m = rng.random_range(0.15..1.4);
            (m, m.powf(0.27))
        }
        PlanetClass::SuperEarth => {
            let m = rng.random_range(1.5..8.0);
            (m, (m.powf(0.27) * 1.15).min(1.9))
        }
        PlanetClass::GasGiant => {
            let mj = rng.random_range(0.4..15.0); // Jupiter masses
            let m = mj * 317.8;
            (m, (mj.powf(0.25) * 11.2).min(12.5))
        }
        PlanetClass::IceGiant => {
            let m = rng.random_range(8.0..40.0); // Earth masses
            (m, m.powf(0.25) * 1.3)
        }
    };

    let gravity = mass / (radius * radius);

    let mut temp = star.temp_at(au);
    let atmo = match class {
        PlanetClass::GasGiant | PlanetClass::IceGiant => Some(Atmo {
            pressure_bar: 0.0,
            n2: 0.0,
            o2: 0.0,
            co2: 0.0,
        }),
        _ => {
            if ratio > 0.4 && ratio < 2.6 && temp < 340.0 && rng.random_bool(0.45) {
                let bar = log10_rng(rng, 0.0, 2.0);
                let o2: f64 = if (230.0..310.0).contains(&temp) && rng.random_bool(0.35) {
                    rng.random_range(0.05..0.35)
                } else {
                    rng.random_range(0.0..0.02)
                };
                let co2: f64 = if temp > 280.0 { rng.random_range(0.05..0.6) } else { rng.random_range(0.0..0.1) };
                let n2 = (1.0 - o2 - co2).max(0.05);
                // Greenhouse nudge
                temp += bar.ln() * 8.0;
                Some(Atmo { pressure_bar: bar, n2, o2, co2 })
            } else if temp > 350.0 && rng.random_bool(0.4) {
                // Hot CO2 atmosphere
                let bar = log10_rng(rng, 0.0, 1.5);
                temp += bar.ln() * 10.0;
                Some(Atmo { pressure_bar: bar, n2: 0.05, o2: 0.0, co2: 0.95 })
            } else {
                None
            }
        }
    };

    let bulk = match class {
        PlanetClass::Terrestrial | PlanetClass::SuperEarth => "iron core / silicate mantle".into(),
        PlanetClass::GasGiant => "H/He envelope, metallic H".into(),
        PlanetClass::IceGiant => "H/He + water/ammonia ices".into(),
    };

    let moons = match class {
        PlanetClass::GasGiant => rng.random_range(0..=12),
        PlanetClass::IceGiant => rng.random_range(0..=8),
        _ => rng.random_bool(0.3) as usize * rng.random_range(1..=2),
    };

    // ---- Deposits (tons) ----
    let scale = match class {
        PlanetClass::Terrestrial => 1.0,
        PlanetClass::SuperEarth => 10.0,
        PlanetClass::IceGiant => 1.0,
        PlanetClass::GasGiant => 1.0,
    };
    let cold = temp < 273.0;
    let mut deposits: Vec<Deposit> = Vec::new();
    match class {
        PlanetClass::Terrestrial | PlanetClass::SuperEarth => {
            deposits.push(Deposit { res: Res::Metal, tons: log10_rng(rng, 4.0, 6.2) * scale });
            deposits.push(Deposit { res: Res::Silicate, tons: log10_rng(rng, 4.0, 6.2) * scale });
            if rng.random_bool(0.4) {
                deposits.push(Deposit { res: Res::Carbon, tons: log10_rng(rng, 3.0, 4.5) * scale });
            }
            if cold {
                deposits.push(Deposit { res: Res::Ice, tons: log10_rng(rng, 4.0, 6.0) * scale });
                // Nitrogen freezes out with water on cold worlds: N2
                // clathrates and nitrogen-rich organics in the regolith.
                // Smaller pockets than an ice giant's, but inside the inner
                // system where drones can actually reach them.
                if rng.random_bool(0.35) {
                    deposits.push(Deposit { res: Res::Nitrogen, tons: log10_rng(rng, 3.0, 4.5) * scale });
                }
            }
        }
        PlanetClass::IceGiant => {
            deposits.push(Deposit { res: Res::Ice, tons: log10_rng(rng, 5.0, 7.0) });
            if rng.random_bool(0.6) {
                deposits.push(Deposit { res: Res::Nitrogen, tons: log10_rng(rng, 4.0, 6.0) });
            }
            if rng.random_bool(0.5) {
                deposits.push(Deposit { res: Res::Carbon, tons: log10_rng(rng, 3.5, 5.0) });
            }
            if rng.random_bool(0.3) {
                deposits.push(Deposit { res: Res::Metal, tons: log10_rng(rng, 3.5, 5.0) });
            }
        }
        PlanetClass::GasGiant => {
            // Atmosphere is scoopable — no solid deposits for drones.
        }
    }
    deposits.sort_by(|a, b| b.tons.total_cmp(&a.tons));

    Planet {
        name: planet_name(star_name, index),
        class,
        mass,
        radius,
        au,
        period: star.period_at(au),
        temp,
        gravity,
        atmo,
        bulk,
        moons,
        deposits,
        phase: rng.random_range(0.0..std::f64::consts::TAU),
    }
}

/// "<star> <roman numeral>" for the index-th world (0 = innermost).
pub fn planet_name(star_name: &str, index: usize) -> String {
    const ROMAN: [&str; 8] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII"];
    format!("{} {}", star_name, ROMAN[index.min(ROMAN.len() - 1)])
}
