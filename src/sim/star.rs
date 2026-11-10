//! Stars: spectral class and luminosity class with correlated physical
//! properties. Rolling a star lives in `worldgen::stars`.

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
    /// White dwarf (degenerate remnant).
    D,
    /// Brown dwarfs: L (warmer) and T (cooler).
    L,
    T,
}

impl StarClass {
    /// Magnetic-scoop yield factor (stellar wind density proxy).
    pub fn scoop_factor(&self) -> f64 {
        match self {
            StarClass::O => 2.6,
            StarClass::B => 2.3,
            StarClass::A => 1.7,
            StarClass::F => 1.25,
            StarClass::G => 1.0,
            StarClass::K => 0.8,
            // Red dwarfs are the common case: a small ship must be able to
            // live off one, if not comfortably.
            StarClass::M => 0.7,
            // A white dwarf has little wind; a brown dwarf barely any.
            StarClass::D => 0.35,
            StarClass::L => 0.15,
            StarClass::T => 0.08,
        }
    }

    /// Hazard level 0..1.5 (X-ray / particle flux near the star).
    pub fn radiation(&self) -> f64 {
        match self {
            StarClass::O => 1.5,
            StarClass::B => 1.2,
            StarClass::A => 0.7,
            StarClass::F => 0.3,
            StarClass::G => 0.1,
            StarClass::K => 0.05,
            StarClass::M => 0.03,
            StarClass::D => 0.45, // hard UV from a hot, naked core
            StarClass::L | StarClass::T => 0.0,
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
            StarClass::D => Rgb(225, 235, 255),
            StarClass::L => Rgb(200, 80, 70),
            StarClass::T => Rgb(170, 70, 120),
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
            StarClass::D => 'D',
            StarClass::L => 'L',
            StarClass::T => 'T',
        }
    }

    pub fn color_name(&self) -> &'static str {
        match self {
            StarClass::O | StarClass::B => "Blue-White",
            StarClass::A => "White",
            StarClass::F => "Yellow-White",
            StarClass::G => "Yellow",
            StarClass::K => "Orange",
            StarClass::M => "Red",
            StarClass::D => "White",
            StarClass::L | StarClass::T => "Dim Red",
        }
    }

    /// Main-sequence mass range (solar masses).
    pub fn mass_range(&self) -> (f64, f64) {
        match self {
            StarClass::O => (16.0, 45.0),
            StarClass::B => (2.5, 16.0),
            StarClass::A => (1.4, 2.5),
            StarClass::F => (1.05, 1.4),
            StarClass::G => (0.8, 1.05),
            StarClass::K => (0.45, 0.8),
            StarClass::M => (0.08, 0.45),
            StarClass::D => (0.5, 1.0),
            StarClass::L => (0.06, 0.075),
            StarClass::T => (0.02, 0.06),
        }
    }

    /// Main-sequence surface temperature range (K).
    pub fn temp_range(&self) -> (f64, f64) {
        match self {
            StarClass::O => (30_000.0, 50_000.0),
            StarClass::B => (10_000.0, 30_000.0),
            StarClass::A => (7_500.0, 10_000.0),
            StarClass::F => (6_000.0, 7_500.0),
            StarClass::G => (5_200.0, 6_000.0),
            StarClass::K => (3_700.0, 5_200.0),
            StarClass::M => (2_400.0, 3_700.0),
            StarClass::D => (6_000.0, 40_000.0),
            StarClass::L => (1_300.0, 2_200.0),
            StarClass::T => (550.0, 1_300.0),
        }
    }
}

/// Luminosity class / evolutionary state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StarKind {
    MainSequence,
    Subgiant,
    Giant,
    WhiteDwarf,
    BrownDwarf,
}

impl StarKind {
    pub fn name(&self) -> &'static str {
        match self {
            StarKind::MainSequence => "Main Sequence",
            StarKind::Subgiant => "Subgiant",
            StarKind::Giant => "Giant",
            StarKind::WhiteDwarf => "White Dwarf",
            StarKind::BrownDwarf => "Brown Dwarf",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Star {
    pub name: String,
    pub catalog: String,
    pub class: StarClass,
    pub kind: StarKind,
    /// Spectral code, e.g. "G2 V", "K4 III", "DA6", "T3".
    pub code: String,
    /// Plain-language type, e.g. "Red Dwarf", "Orange Giant".
    pub desc: String,
    pub mass: f64,   // solar masses
    pub radius: f64, // solar radii
    pub lum: f64,    // solar luminosities
    pub temp: f64,   // K
    pub age: f64,    // Gyr
    pub metallicity: f64, // dex
}

impl Star {
    pub fn giant(&self) -> bool {
        self.kind == StarKind::Giant
    }

    /// Distance (AU) of the habitable zone for this star.
    pub fn habitable_au(&self) -> f64 {
        self.lum.sqrt().max(0.003)
    }

    /// Orbital period (years) at `au` for this star.
    pub fn period_at(&self, au: f64) -> f64 {
        (au * au * au / self.mass.max(0.02)).sqrt()
    }

    /// Equilibrium temperature (K) at `au`, assuming albedo 0.3.
    pub fn temp_at(&self, au: f64) -> f64 {
        278.0 * self.lum.powf(0.25) / au.max(1e-4).sqrt()
    }

    /// Base H2 scoop rate multiplier for this star (mass + class wind).
    pub fn scoop_base(&self) -> f64 {
        let k = match self.kind {
            StarKind::Giant => 2.2,
            StarKind::Subgiant => 1.4,
            _ => 1.0,
        };
        self.class.scoop_factor() * self.mass.powf(0.3) * k
    }

    pub fn radiation(&self) -> f64 {
        self.class.radiation() * if self.kind == StarKind::Giant { 0.7 } else { 1.0 }
    }
}
