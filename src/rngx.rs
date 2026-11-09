//! Seeded RNG helpers. All procedural content derives from (seed, salt)
//! through a splitmix64 finalizer, so the same (seed, key) always yields
//! the same world — which is what makes the infinite galaxy stable.

use rand::rngs::StdRng;
use rand::SeedableRng;

/// splitmix64 finalizer — good avalanche for deriving independent streams.
pub fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

/// Derive an independent RNG from a seed and a salt.
pub fn derive(seed: u64, salt: u64) -> StdRng {
    StdRng::seed_from_u64(mix64(seed ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15)))
}

/// Two independent uniform [0,1) values from a key.
pub fn unit2(key: u64, salt: u64) -> (f64, f64) {
    let a = mix64(key.wrapping_add(salt.wrapping_mul(0x1_0000_0001)));
    let b = mix64(key.wrapping_add(salt.wrapping_mul(0x2_0000_0002).wrapping_add(0xdead_beef)));
    (a as f64 / u64::MAX as f64, b as f64 / u64::MAX as f64)
}

/// A fresh, well-mixed seed from the clock + pid.
pub fn random_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    mix64(t ^ (std::process::id() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15))
}

/// Deterministic human-friendly name from syllables.
pub fn name(rng: &mut impl rand::RngExt) -> String {
    const C: &[&str] = &["k", "z", "v", "th", "s", "n", "r", "l", "d", "b", "m", "x", "qu"];
    const V: &[&str] = &["a", "e", "i", "o", "u", "y", "ae", "io"];
    let n = if rng.random_bool(0.25) { 3 } else { 4 };
    let mut s = String::new();
    for _ in 0..n {
        s.push_str(C[rng.random_range(0..C.len())]);
        s.push_str(V[rng.random_range(0..V.len())]);
    }
    s.make_ascii_uppercase();
    let mut c = s.chars();
    let _ = c.next();
    format!("{}{}", s.chars().next().unwrap(), c.as_str().to_lowercase())
}
