//! Tabletop-style rolls on the game's seeded RNG (port of `Prng.gs`).

use crate::rngx::GameRng;
use rand::RngExt;

pub fn d(rng: &mut GameRng, sides: i32) -> i32 {
    rng.random_range(1..=sides)
}

pub fn d6(rng: &mut GameRng) -> i32 {
    d(rng, 6)
}

pub fn r2d6(rng: &mut GameRng) -> i32 {
    d6(rng) + d6(rng)
}

pub fn r3d6(rng: &mut GameRng) -> i32 {
    d6(rng) + d6(rng) + d6(rng)
}

/// Uniform integer in 0..n (0 when n <= 1), like the old `alienRandom_`.
pub fn below(rng: &mut GameRng, n: i32) -> i32 {
    if n <= 1 { 0 } else { rng.random_range(0..n) }
}

pub fn unit(rng: &mut GameRng) -> f64 {
    rng.random::<f64>()
}

pub fn percent(rng: &mut GameRng, pct: f64) -> bool {
    unit(rng) * 100.0 < pct
}

pub fn pick<'a, T>(rng: &mut GameRng, list: &'a [T]) -> &'a T {
    &list[rng.random_range(0..list.len())]
}

/// Weighted pick; fractional weights allowed, zero weights never chosen.
pub fn weighted<T: Clone>(rng: &mut GameRng, items: &[(T, f64)]) -> T {
    let total: f64 = items.iter().map(|(_, w)| w.max(0.0)).sum();
    let mut roll = unit(rng) * total;
    for (v, w) in items {
        let w = w.max(0.0);
        if w > 0.0 && roll < w {
            return v.clone();
        }
        roll -= w;
    }
    items
        .iter()
        .rev()
        .find(|(_, w)| *w > 0.0)
        .map(|(v, _)| v.clone())
        .unwrap_or_else(|| items[0].0.clone())
}

/// `count` distinct entries from `list`.
pub fn distinct<T: Clone>(rng: &mut GameRng, list: &[T], count: usize) -> Vec<T> {
    let mut pool: Vec<T> = list.to_vec();
    let mut out = Vec::new();
    while out.len() < count && !pool.is_empty() {
        let i = rng.random_range(0..pool.len());
        out.push(pool.remove(i));
    }
    out
}

/// Log-uniform value between `min` and `max`.
pub fn log_uniform(rng: &mut GameRng, min: f64, max: f64) -> f64 {
    (min.ln() + unit(rng) * (max.ln() - min.ln())).exp()
}

/// 10^(lo..hi), the deposit-size distribution.
pub fn log10_range(rng: &mut GameRng, lo: f64, hi: f64) -> f64 {
    10f64.powf(lo + (hi - lo) * unit(rng))
}
