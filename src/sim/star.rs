//! Stars: spectral classification with correlated physical properties.

use crate::rngx::name;
use rand::RngExt;
use rand::rngs::StdRng;
use super::res::Rgb;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StarClass {
    O,
    B,
    A,
    F,
    G,
    K,
    M,
}

impl StarClass {
    pub const ALL: [StarClass; 7] = [StarClass::O, StarClass::B, StarClass::A, StarClass::F, StarClass::G, StarClass::K, StarClass::M];

    /// Relative frequency in the galaxy (main sequence, by number).
    pub fn weight(&self) -> f64 {
        match self {
            StarClass::O => 0.015,
            StarClass::B => 0.04,
            StarClass::A => 0.09,
            StarClass::F => 0.13,
            StarClass::G => 0.18,
            StarClass::K => 0.26,
            StarClass::M => 0.285,
        }
    }

    pub fn temp_range(&self) -> (f64, f64) {
        match self {
            StarClass::O => (30_000.0, 50_000.0),
            StarClass::B => (10_000.0, 30_000.0),
            StarClass::A => (7_500.0, 10_000.0),
            StarClass::F => (6_000.0, 7_500.0),
            StarClass::G => (5_200.0, 6_000.0),
            StarClass::K => (3_700.0, 5_200.0),
            StarClass::M => (2_400.0, 3_700.0),
        }
    }

    pub fn mass_range(&self) -> (f64, f64) {
        match self {
            StarClass::O => (16.0, 45.0),
            StarClass::B => (2.1, 16.0),
            StarClass::A => (1.4, 2.1),
            StarClass::F => (0.8, 1.4),
            StarClass::G => (0.8, 1.05),
            StarClass::K => (0.45, 0.8),
            StarClass::M => (0.08, 0.45),
        }
    }

    /// Magnetic-scoop yield factor (stellar wind density proxy).
    pub fn scoop_factor(&self) -> f64 {
        match self {
            StarClass::O => 2.6,
            StarClass::B => 2.3,
            StarClass::A => 1.7,
            StarClass::F => 1.25,
            StarClass::G => 1.0,
            StarClass::K => 0.7,
            StarClass::M => 0.45,
        }
    }

    /// Hazard level 0..1 (X-ray / particle flux at 1 AU).
    pub fn radiation(&self) -> f64 {
        match self {
            StarClass::O => 1.5,
            StarClass::B => 1.2,
            StarClass::A => 0.7,
            StarClass::F => 0.3,
            StarClass::G => 0.1,
            StarClass::K => 0.05,
            StarClass::M => 0.03,
        }
    }

    pub fn color(&self) -> Rgb {
        match self {
            StarClass::O => Rgb(155, 176, 255),
            StarClass::B => Rgb(170, 191, 255),
            StarClass::A => Rgb(202, 215, 255),
            StarClass::F => Rgb(248, 247, 255),
            StarClass::G => Rgb(255, 236, 170),
            StarClass::K => Rgb(255, 190, 110),
            StarClass::M => Rgb(255, 130, 80),
        }
    }

    pub fn glyph(&self) -> char {
        match self {
            StarClass::O => 'O',
            StarClass::B => 'B',
            StarClass::A => 'A',
            StarClass::F => 'F',
            StarClass::G => 'G',
            StarClass::K => 'K',
            StarClass::M => 'M',
        }
    }

    pub fn class_name(&self) -> &'static str {
        match self {
            StarClass::O => "O (blue giant)",
            StarClass::B => "B (blue-white)",
            StarClass::A => "A (white)",
            StarClass::F => "F (yellow-white)",
            StarClass::G => "G (yellow dwarf)",
            StarClass::K => "K (orange dwarf)",
            StarClass::M => "M (red dwarf)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Star {
    pub name: String,
    pub catalog: String,
    pub class: StarClass,
    pub giant: bool,
    pub mass: f64,    // solar masses
    pub radius: f64,  // solar radii
    pub lum: f64,     // solar luminosities
    pub temp: f64,    // K
    pub age: f64,     // Gyr
    pub metallicity: f64, // dex
}

impl Star {
    pub fn generate(rng: &mut StdRng) -> Star {
        // Weighted class pick
        let mut r = rng.random::<f64>();
        let class = StarClass::ALL
            .iter()
            .find(|c| {
                r -= c.weight();
                r < 0.0
            })
            .copied()
            .unwrap_or(StarClass::M);

        let (mlo, mhi) = class.mass_range();
        // Slightly favour the low-mass end (Salpeter-ish)
        let mass = mlo + (mhi - mlo) * rng.random::<f64>().powf(1.7);
        let (tlo, thi) = class.temp_range();
        let temp = tlo + (thi - tlo) * ((mass - mlo) / (mhi - mlo).max(1e-9)).clamp(0.05, 1.0);

        // Small chance K/M star is a red giant (ancient, swollen — great scoop, few worlds)
        let giant = matches!(class, StarClass::K | StarClass::M) && rng.random_bool(0.04);

        let (radius, temp): (f64, f64) = if giant {
            (rng.random_range(8.0..80.0), rng.random_range(3_000.0..4_500.0))
        } else {
            // Main sequence radius ≈ M^0.8
            (mass.powf(0.8), temp)
        };

        // Luminosity from blackbody: L = (T/5772)^4 (R/Rsun)^2
        let lum = (temp / 5_772.0).powi(4) * radius * radius;

        // Age: massive stars are young; dwarfs can be old.
        let max_age = match class {
            StarClass::O | StarClass::B => rng.random_range(0.05..0.4),
            StarClass::A => rng.random_range(0.1..1.5),
            _ => rng.random_range(0.5..12.0),
        };
        let age = if giant { rng.random_range(4.0..10.0) } else { max_age };

        let metallicity = (rng.random::<f64>() + rng.random::<f64>() + rng.random::<f64>() - 1.5) * 0.4;

        let catalog = format!("HD {}", rng.random_range(1_000..999_999));
        let name = name(rng);

        Star {
            name,
            catalog,
            class,
            giant,
            mass,
            radius,
            lum,
            temp,
            age,
            metallicity,
        }
    }

    /// Distance (AU) of the habitable zone for this star.
    pub fn habitable_au(&self) -> f64 {
        self.lum.sqrt()
    }

    /// Orbital period (years) at `au` for this star.
    pub fn period_at(&self, au: f64) -> f64 {
        (au * au * au / self.mass.max(0.05)).sqrt()
    }

    /// Equilibrium temperature (K) at `au`, assuming albedo 0.3.
    pub fn temp_at(&self, au: f64) -> f64 {
        278.0 * self.lum.powf(0.25) / au.sqrt()
    }

    /// Base H2 scoop rate multiplier for this star (mass + class wind).
    pub fn scoop_base(&self) -> f64 {
        let c = if self.giant { 2.2 } else { 1.0 };
        self.class.scoop_factor() * self.mass.powf(0.3) * c
    }
}
