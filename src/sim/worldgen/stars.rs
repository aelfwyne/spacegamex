//! Star and binary-companion rolls (port of the star half of `Generator.gs`
//! and of `Binary.gs`).

use super::dice::{self, below, percent, unit, weighted};
use crate::rngx;
use crate::sim::star::{Star, StarClass, StarKind};
use crate::sim::system::{BinaryLayout, Companion};
use crate::rngx::GameRng;
use rand::RngExt;

/// Planet-count modifiers a star imposes on its system.
#[derive(Debug, Clone, Copy, Default)]
pub struct Mods {
    pub jovian: i32,
    pub terrestrial: i32,
    pub ice: i32,
    pub asteroid: i32,
}

fn age_metal(rng: &mut GameRng, kind: StarKind, class: StarClass) -> (f64, f64) {
    let age = match (kind, class) {
        (StarKind::Giant, _) | (StarKind::WhiteDwarf, _) => rng_range(rng, 3.0, 11.0),
        (_, StarClass::B) => rng_range(rng, 0.05, 0.4),
        (_, StarClass::A) => rng_range(rng, 0.1, 1.5),
        _ => rng_range(rng, 0.5, 12.0),
    };
    let met = (unit(rng) + unit(rng) + unit(rng) - 1.5) * 0.4;
    (age, met)
}

fn rng_range(rng: &mut GameRng, lo: f64, hi: f64) -> f64 {
    lo + (hi - lo) * unit(rng)
}

fn lum_of(temp: f64, radius: f64) -> f64 {
    (temp / 5_772.0).powi(4) * radius * radius
}

/// Main-sequence mass for a class and subtype digit (0 = hottest).
fn ms_mass(class: StarClass, fine: u32) -> f64 {
    let (lo, hi) = class.mass_range();
    (hi.ln() - (fine as f64 / 10.0) * (hi.ln() - lo.ln())).exp()
}

fn ms_temp(class: StarClass, fine: u32) -> f64 {
    let (lo, hi) = class.temp_range();
    hi - (hi - lo) * (fine as f64 + 0.5) / 10.0
}

fn name_and_catalog(rng: &mut GameRng) -> (String, String) {
    let catalog = format!("HD {}", rng.random_range(1_000..999_999));
    (rngx::name(rng), catalog)
}

pub fn build_star(rng: &mut GameRng, class: StarClass, kind: StarKind, fine: u32, mass_hint: Option<f64>) -> Star {
    let (name, catalog) = name_and_catalog(rng);
    let (age, metallicity) = age_metal(rng, kind, class);
    let (mass, radius, temp, code, desc) = match kind {
        StarKind::MainSequence => {
            let m = mass_hint.unwrap_or_else(|| ms_mass(class, fine));
            let t = ms_temp(class, fine);
            let desc = if class == StarClass::M {
                "Red Dwarf".to_string()
            } else {
                format!("{} Main Sequence", class.color_name())
            };
            (m, m.powf(0.8), t, format!("{}{} V", class.glyph(), fine), desc)
        }
        StarKind::Subgiant => {
            let m = (ms_mass(class, fine) * 1.1).max(1.0);
            let t = ms_temp(class, fine) * 0.9;
            let r = m.powf(0.8) * rng_range(rng, 1.8, 3.0);
            (m, r, t, format!("{}{} IV", class.glyph(), fine), format!("{} Subgiant", class.color_name()))
        }
        StarKind::Giant => {
            let m = match class {
                StarClass::B => 6.0,
                StarClass::A => 3.0,
                StarClass::F | StarClass::G => 2.0,
                StarClass::K => 1.5,
                _ => 1.3,
            } * rng_range(rng, 0.85, 1.15);
            let (r, t) = match class {
                StarClass::B | StarClass::A => (rng_range(rng, 5.0, 15.0), ms_temp(class, fine) * 0.85),
                StarClass::F | StarClass::G => (rng_range(rng, 6.0, 20.0), ms_temp(class, fine) * 0.85),
                _ => (rng_range(rng, 15.0, 80.0), rng_range(rng, 3_000.0, 4_500.0)),
            };
            (m, r, t, format!("{}{} III", class.glyph(), fine), format!("{} Giant", class.color_name()))
        }
        StarKind::WhiteDwarf => {
            let m = mass_hint.unwrap_or_else(|| rng_range(rng, 0.5, 0.8));
            let sub = dice::pick(rng, &["DA", "DA", "DA", "DB", "DQ", "DZ", "DC"]).to_string();
            let f = fine.clamp(1, 9);
            let t = 50_400.0 / f as f64;
            let r = 0.012 * (0.6 / m).powf(1.0 / 3.0);
            (m, r, t, format!("{sub}{f}"), "White Dwarf".to_string())
        }
        StarKind::BrownDwarf => {
            let m = mass_hint.unwrap_or(0.05);
            let (lo, hi) = class.temp_range();
            let t = hi - (hi - lo) * (fine as f64 + 0.5) / 10.0;
            (m, 0.1, t, format!("{}{}", class.glyph(), fine), "Brown Dwarf".to_string())
        }
    };
    Star {
        name,
        catalog,
        class,
        kind,
        code,
        desc,
        mass,
        radius,
        lum: lum_of(temp, radius),
        temp,
        age,
        metallicity,
    }
}

/// The primary star roll (port of `buildAlienStar_`).
pub fn roll_star(rng: &mut GameRng) -> (Star, Mods) {
    let mut mods = Mods::default();
    let spec = dice::d(rng, 100);
    let fine = below(rng, 10) as u32;
    let star = if spec <= 5 {
        let secondary = dice::d(rng, 93);
        let color = dice::d(rng, 100);
        if secondary <= 24 {
            mods.ice -= 1;
            let class = match color {
                0..=20 => StarClass::B,
                21..=40 => StarClass::A,
                41..=80 => StarClass::K,
                _ => StarClass::M,
            };
            build_star(rng, class, StarKind::Giant, fine, None)
        } else {
            mods.jovian -= 1;
            mods.ice -= 1;
            mods.asteroid -= 2;
            let class = match color {
                0..=20 => StarClass::B,
                21..=40 => StarClass::A,
                41..=60 => StarClass::K,
                _ => StarClass::M,
            };
            build_star(rng, class, StarKind::Subgiant, fine, None)
        }
    } else if spec <= 95 {
        mods.ice -= 1;
        let class = match dice::d(rng, 100) {
            1 => StarClass::B,
            2..=3 => StarClass::A,
            4..=8 => StarClass::F,
            9..=15 => StarClass::G,
            16..=31 => StarClass::K,
            _ => StarClass::M,
        };
        if class == StarClass::M {
            mods.terrestrial -= 3;
        }
        build_star(rng, class, StarKind::MainSequence, fine, None)
    } else {
        mods.jovian -= 4;
        mods.terrestrial -= 3;
        mods.asteroid -= 2;
        let f = 1 + below(rng, 9) as u32;
        build_star(rng, StarClass::D, StarKind::WhiteDwarf, f, None)
    };
    (star, mods)
}

fn ms_from_mass(rng: &mut GameRng, mass: f64) -> (Star, Mods) {
    let classes = [StarClass::B, StarClass::A, StarClass::F, StarClass::G, StarClass::K, StarClass::M];
    let class = classes.iter().copied().find(|c| mass >= c.mass_range().0).unwrap_or(StarClass::M);
    let (lo, hi) = class.mass_range();
    let m = mass.clamp(lo, hi);
    let fine = ((10.0 * (hi.ln() - m.ln()) / (hi.ln() - lo.ln())).round() as i64).clamp(0, 9) as u32;
    let mut mods = Mods { ice: -1, ..Default::default() };
    if class == StarClass::M {
        mods.terrestrial -= 3;
    }
    (build_star(rng, class, StarKind::MainSequence, fine, Some(mass)), mods)
}

fn dwarf_mods() -> Mods {
    Mods { jovian: -4, terrestrial: -3, ice: 0, asteroid: -2 }
}

/// Holman & Wiegert (1999) S-type critical orbit as a fraction of separation.
pub fn hw_s(e: f64, mu: f64) -> f64 {
    let e = e.clamp(0.0, 0.8);
    let mu = mu.clamp(0.1, 0.9);
    (0.464 - 0.380 * mu - 0.631 * e + 0.586 * mu * e + 0.150 * e * e - 0.198 * mu * e * e).max(0.02)
}

/// Holman & Wiegert (1999) P-type critical orbit as a multiple of separation.
pub fn hw_p(e: f64, mu: f64) -> f64 {
    let e = e.clamp(0.0, 0.7);
    let mu = mu.clamp(0.1, 0.5);
    1.60 + 5.10 * e - 2.22 * e * e + 4.12 * mu - 4.27 * e * mu - 5.09 * mu * mu + 4.61 * e * e * mu * mu
}

/// Mass ratio q = m2/m1 after Moe & Di Stefano (2017), simplified.
fn sample_q(rng: &mut GameRng, m1: f64, layout: BinaryLayout, q_min: f64) -> f64 {
    use BinaryLayout::*;
    let (gamma, twin) = if m1 < 0.5 {
        match layout {
            Close => (1.5, 20.0),
            Intermediate => (1.0, 12.0),
            Wide => (0.8, 8.0),
            Distant => (0.5, 5.0),
        }
    } else {
        let massive = m1 >= 2.0;
        match (layout, massive) {
            (Close, true) => (0.0, 10.0),
            (Close, false) => (0.3, 25.0),
            (Intermediate, true) => (-1.2, 3.0),
            (Intermediate, false) => (-0.5, 10.0),
            (Wide | Distant, true) => (-2.0, 0.0),
            (Wide, false) => (-0.6, 5.0),
            (Distant, false) => (-0.6, 3.0),
        }
    };
    if percent(rng, twin) {
        return 0.95 + unit(rng) * 0.05;
    }
    let u = unit(rng);
    let g1: f64 = gamma + 1.0;
    if g1.abs() < 1e-6 {
        q_min * (1.0 / q_min).powf(u)
    } else {
        (q_min.powf(g1) + u * (1.0 - q_min.powf(g1))).powf(1.0 / g1)
    }
}

pub const BINARY_CHANCE: f64 = 5.0;

/// Roll a companion for `primary` (port of `buildAlienBinary_`).
/// Returns the companion description plus star B's planet modifiers.
pub fn roll_binary(rng: &mut GameRng, primary: &Star) -> Option<(Companion, Mods)> {
    if !percent(rng, BINARY_CHANCE) {
        return None;
    }
    let m1 = primary.mass;
    let evolved = matches!(primary.kind, StarKind::Giant | StarKind::Subgiant);
    let layout = weighted(rng, &[
        (BinaryLayout::Close, if evolved { 0.0 } else { 25.0 }),
        (BinaryLayout::Intermediate, 20.0),
        (BinaryLayout::Wide, 35.0),
        (BinaryLayout::Distant, 20.0),
    ]);

    // Star B: equal or lower mass than A, from a realistic mass ratio.
    let (mut star_b, mods_b) = if primary.kind == StarKind::WhiteDwarf {
        match weighted(rng, &[("M", 60.0), ("K", 20.0), ("WD", 20.0)]) {
            "WD" => {
                let m = 0.5 + unit(rng) * 0.3;
                let f = 1 + below(rng, 9) as u32;
                (build_star(rng, StarClass::D, StarKind::WhiteDwarf, f, Some(m)), dwarf_mods())
            }
            c => {
                let class = if c == "M" { StarClass::M } else { StarClass::K };
                let (lo, hi) = class.mass_range();
                let m = lo + unit(rng) * (hi - lo);
                ms_from_mass(rng, m)
            }
        }
    } else if primary.class != StarClass::B && percent(rng, 8.0) {
        let m = (m1 * 0.95).min(0.5 + unit(rng) * 0.4);
        let f = 1 + below(rng, 9) as u32;
        (build_star(rng, StarClass::D, StarKind::WhiteDwarf, f, Some(m)), dwarf_mods())
    } else {
        let bd_chance = if layout == BinaryLayout::Close { 1.0 } else if m1 < 0.5 { 6.0 } else { 2.0 };
        if percent(rng, bd_chance) {
            let m = 0.075f64.min(m1 * 0.9).min(0.02 + unit(rng) * 0.055);
            let class = if m >= 0.06 { StarClass::L } else { StarClass::T };
            let f = below(rng, 10) as u32;
            (build_star(rng, class, StarKind::BrownDwarf, f, Some(m)), dwarf_mods())
        } else {
            let q_min = (0.08 / m1).clamp(0.1, 0.95);
            let m2 = (sample_q(rng, m1, layout, q_min) * m1).max(0.08);
            ms_from_mass(rng, m2)
        }
    };
    star_b.name = format!("{} B", primary.name);
    star_b.catalog = format!("{} B", primary.catalog);

    // Separation: the old "orbit units" (5000 = the habitable zone), in AU.
    let (min, max, e_min, e_max) = match layout {
        BinaryLayout::Close => (150.0, 1_000.0, 0.0, 0.15),
        BinaryLayout::Intermediate => (8_000.0, 40_000.0, 0.05, 0.7),
        BinaryLayout::Wide => (40_000.0, 300_000.0, 0.05, 0.75),
        BinaryLayout::Distant => (300_000.0, 3_000_000.0, 0.2, 0.9),
    };
    let hz = primary.habitable_au();
    let separation = dice::log_uniform(rng, min, max) / 5000.0 * hz;
    let ecc = ((e_min + unit(rng) * (e_max - e_min)) * 100.0).round() / 100.0;
    let m2 = star_b.mass;
    let total = m1 + m2;
    let (mu_a, mu_b) = (m2 / total, m1 / total);
    let p_type = layout == BinaryLayout::Close;
    let c = Companion {
        star: star_b,
        layout,
        p_type,
        separation_au: separation,
        ecc,
        limit_a: if p_type { 0.0 } else { separation * hw_s(ecc, mu_a) },
        limit_b: if p_type { 0.0 } else { separation * hw_s(ecc, mu_b) },
        circumbinary: if p_type { separation * hw_p(ecc, mu_a) } else { 0.0 },
        mass_ratio: m2 / m1,
    };
    Some((c, mods_b))
}
