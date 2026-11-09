//! The infinite galaxy.
//!
//! Space is partitioned into `CELL`-lightyear grid cells. How many systems a
//! cell holds (0–3) is a deterministic function of (seed, cell), and each
//! system's ID **encodes its cell coordinates reversibly**:
//!
//! ```text
//! id = (cx' << 33) | (cy' << 2) | k         (31 bits per axis)
//! ```
//!
//! so any ID can be unpacked back to its cell, and its position/planets/civ
//! are pure functions of (seed, id). Cells are materialised on demand in any
//! direction, forever — the universe is infinite and always has more systems.

use super::civ::CivTier;
use super::system::StarSystem;
use crate::rngx::{derive, mix64, unit2};
use rand::RngExt;
use std::collections::HashMap;

pub const CELL: f64 = 10.0; // ly per grid cell

// System IDs reversibly encode their grid cell so any ID can be unpacked back
// to a position and regenerated deterministically. Layout (64 bits total):
//   [ cx: 31 bits ][ cy: 31 bits ][ k: 2 bits ]
// 31 signed bits per axis = ±2^30 cells = ±10.7 billion light-years, which is
// effectively infinite for gameplay while always fitting in a u64.
const K_BITS: u32 = 2;
const CY_BITS: u32 = 31;
const CX_SHIFT: u32 = 33;
const AX_MASK: u64 = (1u64 << CY_BITS) - 1;
const AX_OFFSET: i64 = 1 << (CY_BITS as i64 - 1); // 2^30
const K_MASK: u64 = (1u64 << K_BITS) - 1; // 3

pub struct Galaxy {
    pub seed: u64,
    pub cache: HashMap<u64, StarSystem>,
}

/// Lightweight handle to a system for list/map display.
#[derive(Clone)]
pub struct Neighbor {
    pub id: u64,
    pub pos: [f64; 2],
    pub dist_ly: f64,
    pub star_name: String,
    pub star_class: char,
    pub class: super::star::StarClass,
    pub giant: bool,
    pub civ: Option<CivTier>,
    pub civ_kind: Option<super::civ::CivKind>,
    /// Has a binary companion star.
    pub binary: bool,
    pub n_planets: usize,
    pub richness: f64,
}

impl Galaxy {
    pub fn new(seed: u64) -> Self {
        Self { seed, cache: HashMap::new() }
    }

    // ---------- ID <-> cell packing ----------

    /// Pack (cell_x, cell_y, index) into a u64 ID. Reversible via [`unpack_id`].
    pub fn pack_id(cx: i64, cy: i64, k: u32) -> u64 {
        let cx = (cx + AX_OFFSET) as u64 & AX_MASK;
        let cy = (cy + AX_OFFSET) as u64 & AX_MASK;
        let k = k as u64 & K_MASK;
        (cx << CX_SHIFT) | (cy << K_BITS) | k
    }

    /// Reversibly unpack an ID back into (cell_x, cell_y, index).
    pub fn unpack_id(id: u64) -> (i64, i64, u32) {
        let cx = ((id >> CX_SHIFT) & AX_MASK) as i64 - AX_OFFSET;
        let cy = ((id >> K_BITS) & AX_MASK) as i64 - AX_OFFSET;
        let k = (id & K_MASK) as u32;
        (cx, cy, k)
    }

    pub fn home_id(&self) -> u64 {
        Self::pack_id(0, 0, 0)
    }

    fn cell_key(cx: i64, cy: i64) -> u64 {
        mix64(
            (cx as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
                ^ (cy as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9),
        )
    }

    /// IDs of the systems in one cell (0–3, deterministic).
    pub fn ids_in_cell(&self, cx: i64, cy: i64) -> Vec<u64> {
        let mut rng = derive(self.seed, Self::cell_key(cx, cy));
        let r = rng.random::<f64>();
        let n = if r < 0.15 {
            0
        } else if r < 0.72 {
            1
        } else if r < 0.90 {
            2
        } else {
            3
        };
        (0..n).map(|k| Self::pack_id(cx, cy, k)).collect()
    }

    /// Deterministic position (ly) of a system ID.
    pub fn pos_of(id: u64) -> [f64; 2] {
        let (cx, cy, _) = Self::unpack_id(id);
        let (ux, uy) = unit2(id, 0x517c);
        // Anywhere in the cell (5% margin), as a fraction of CELL. This used
        // to add 0.4 + 0.6·u light-years, which pinned every star within
        // 1 ly of its cell corner: a visible 10-ly lattice, and systems that
        // share a cell sat ~0.5 ly apart.
        [
            (cx as f64 + 0.05 + 0.9 * ux) * CELL,
            (cy as f64 + 0.05 + 0.9 * uy) * CELL,
        ]
    }

    /// Full system for an ID, generated lazily and cached.
    pub fn system(&mut self, id: u64) -> &mut StarSystem {
        let seed = self.seed;
        if !self.cache.contains_key(&id) {
            let is_home = id == self.home_id();
            let sys = StarSystem::generate(seed, id, Self::pos_of(id), is_home);
            self.cache.insert(id, sys);
        }
        self.cache.get_mut(&id).unwrap()
    }

    pub fn system_ref(&self, id: u64) -> &StarSystem {
        self.cache.get(&id).unwrap()
    }

    /// All systems within `radius` ly of `pos`, nearest first. Excludes `exclude`.
    pub fn neighbors(&mut self, pos: [f64; 2], radius: f64, exclude: u64) -> Vec<Neighbor> {
        let (cx0, cy0) = ((pos[0] / CELL).floor() as i64, (pos[1] / CELL).floor() as i64);
        let span = (radius / CELL).ceil() as i64 + 1;
        let mut out = Vec::new();
        for cx in (cx0 - span)..=(cx0 + span) {
            for cy in (cy0 - span)..=(cy0 + span) {
                for id in self.ids_in_cell(cx, cy) {
                    if id == exclude {
                        continue;
                    }
                    let sys = self.system(id);
                    let dx = sys.pos[0] - pos[0];
                    let dy = sys.pos[1] - pos[1];
                    let d = (dx * dx + dy * dy).sqrt();
                    if d <= radius {
                        out.push(Neighbor {
                            id,
                            pos: sys.pos,
                            dist_ly: d,
                            star_name: sys.star.name.clone(),
                            star_class: sys.star.class.glyph(),
                            class: sys.star.class,
                            giant: sys.star.giant(),
                            civ: sys.civ.as_ref().map(|c| c.tier),
                            civ_kind: sys.civ.as_ref().map(|c| c.kind),
                            binary: sys.companion.is_some(),
                            n_planets: sys.planets.len(),
                            richness: sys.richness(),
                        });
                    }
                }
            }
        }
        out.sort_by(|a, b| a.dist_ly.total_cmp(&b.dist_ly));
        out
    }
}
