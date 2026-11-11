//! Seeded RNG helpers. All procedural content derives from (seed, salt),
//! so the same (seed, key) always yields the same world — which is what
//! makes the infinite galaxy stable.
//!
//! The generator is xoshiro256++, keyed through Pelle Evensen's "moremur"
//! 64-bit mixer. (This replaced ChaCha12 keyed through splitmix64.)

use rand::rngs::Xoshiro256PlusPlus;
use rand::SeedableRng;

/// The game's random number generator.
pub type GameRng = Xoshiro256PlusPlus;

/// Moremur mixer — strong avalanche for hashing keys into independent
/// streams and uniform values.
pub fn mix64(mut z: u64) -> u64 {
    z ^= z >> 27;
    z = z.wrapping_mul(0x3c79_ac49_2ba7_b653);
    z ^= z >> 33;
    z = z.wrapping_mul(0x1c69_b3f7_4ac4_ae35);
    z ^ (z >> 27)
}

/// A generator whose whole 256-bit state comes from `key` through the
/// mixer (no splitmix in the seeding either).
pub fn seeded(key: u64) -> GameRng {
    let mut bytes = [0u8; 32];
    for (i, chunk) in bytes.chunks_mut(8).enumerate() {
        let w = mix64(key.wrapping_add((i as u64 + 1).wrapping_mul(0xd1b5_4a32_d192_ed03)));
        chunk.copy_from_slice(&w.to_le_bytes());
    }
    // An all-zero state is impossible in practice; guard it anyway.
    if bytes.iter().all(|&b| b == 0) {
        bytes[0] = 1;
    }
    GameRng::from_seed(bytes)
}

/// Derive an independent RNG from a seed and a salt.
pub fn derive(seed: u64, salt: u64) -> GameRng {
    seeded(mix64(seed) ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(23))
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

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngExt;

    /// Mean, lag-1 correlation, and runs above/below the median of a
    /// sequence of uniforms: a fair source gives ~0.5, ~0 and ~n/2.
    fn check(name: &str, xs: &[f64]) {
        let n = xs.len() as f64;
        let mean = xs.iter().sum::<f64>() / n;
        let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
        let cov = xs.windows(2).map(|w| (w[0] - mean) * (w[1] - mean)).sum::<f64>() / (n - 1.0);
        let corr = cov / var;
        let runs = 1 + xs.windows(2).filter(|w| (w[0] < 0.5) != (w[1] < 0.5)).count();
        let run_ratio = runs as f64 / (n / 2.0);
        // Buckets: a chi-square over 20 bins.
        let mut bins = [0f64; 20];
        for x in xs {
            bins[((x * 20.0) as usize).min(19)] += 1.0;
        }
        let e = n / 20.0;
        let chi: f64 = bins.iter().map(|b| (b - e).powi(2) / e).sum();
        assert!((mean - 0.5).abs() < 0.005, "{name}: mean {mean}");
        assert!(corr.abs() < 0.01, "{name}: lag-1 correlation {corr}");
        assert!((run_ratio - 1.0).abs() < 0.02, "{name}: runs {run_ratio}");
        assert!(chi < 45.0, "{name}: chi-square {chi} over 19 dof");
    }

    #[test]
    fn streams_from_consecutive_keys_are_independent() {
        let n = 100_000u64;
        for seed in [0u64, 1, 42, 0xdead_beef] {
            // The first, second and fifth value of fresh streams keyed
            // 1, 2, 3, … (how the colony and combat draw each step).
            for skip in [0usize, 1, 4] {
                let xs: Vec<f64> = (1..=n)
                    .map(|k| {
                        let mut r = derive(seed ^ mix64(k), 0x57_e9);
                        for _ in 0..skip {
                            let _: f64 = r.random();
                        }
                        r.random::<f64>()
                    })
                    .collect();
                check(&format!("seed {seed} draw {skip}"), &xs);
            }
            // Salts in sequence (one stream per id).
            let xs: Vec<f64> = (0..n).map(|k| derive(seed, k).random::<f64>()).collect();
            check(&format!("seed {seed} salts"), &xs);
            // One long stream.
            let mut r = derive(seed, 7);
            let xs: Vec<f64> = (0..n).map(|_| r.random::<f64>()).collect();
            check(&format!("seed {seed} stream"), &xs);
            // Hash rolls of neighbouring keys (star positions).
            let xs: Vec<f64> = (0..n).map(|k| unit2(seed.wrapping_add(k), 0x517c).0).collect();
            check(&format!("seed {seed} unit2"), &xs);
        }
    }
}
