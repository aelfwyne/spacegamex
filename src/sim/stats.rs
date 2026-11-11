//! Career statistics: counters, extremes seen and kinds of things found.

use super::planet::Planet;
use super::system::StarSystem;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The most extreme example of something seen so far.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Extreme {
    pub value: f64,
    /// Body or star name.
    pub what: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Stats {
    // ---- travel ----
    pub jumps: u32,
    pub dist_ly: f64,
    pub systems: u32,
    pub landings: u32,
    pub farthest_ly: f64,
    // ---- work ----
    pub scooped: f64,
    pub mined: f64,
    pub fabbed: f64,
    pub salvage_cr: f64,
    // ---- trade ----
    pub trades: u32,
    pub credits_earned: f64,
    pub credits_spent: f64,
    pub goods_bought: u32,
    pub goods_sold: u32,
    pub partners: u32,
    pub pilots: u32,
    // ---- combat ----
    pub pirates_met: u32,
    pub pirates_fought: u32,
    pub pirates_fled: u32,
    pub pirates_paid: u32,
    pub hull_damage: f64,
    // ---- quests ----
    pub quests_done: u32,
    pub quests_dropped: u32,
    pub artifacts_kept: u32,
    pub artifacts_delivered: u32,
    pub relics_found: u32,
    pub relics_sold: u32,
    // ---- people ----
    pub crew_lost: u32,
    pub pax_lost: u32,
    pub successions: u32,
    // ---- colony ----
    pub colonies_founded: u32,
    pub colonies_abandoned: u32,
    pub colony_births: f64,
    pub colony_deaths: f64,
    pub colony_peak: f64,
    pub buildings: u32,
    pub laws_enacted: u32,
    // ---- discoveries ----
    /// Records by name ("Hottest world" …).
    pub extremes: BTreeMap<String, Extreme>,
    /// Category → kind → how many seen.
    pub found: BTreeMap<String, BTreeMap<String, u32>>,
}

/// The record names, in display order, and whether higher is the record.
pub const EXTREMES: [(&str, bool, &str); 10] = [
    ("Hottest world", true, "K"),
    ("Coldest world", false, "K"),
    ("Strongest gravity", true, "g"),
    ("Weakest gravity", false, "g"),
    ("Largest world", true, "km"),
    ("Smallest world", false, "km"),
    ("Most moons", true, ""),
    ("Hottest star", true, "K"),
    ("Coolest star", false, "K"),
    ("Most luminous star", true, "L☉"),
];

/// Kinds-found categories, in display order.
pub const CATEGORIES: [&str; 7] = ["Star classes", "Star types", "World classes", "Atmospheres", "Life", "Native civilisations", "Gas giant features"];

impl Stats {
    fn record(&mut self, key: &str, value: f64, what: &str) {
        let Some(&(_, high, _)) = EXTREMES.iter().find(|(k, _, _)| *k == key) else { return };
        let better = match self.extremes.get(key) {
            None => true,
            Some(e) => if high { value > e.value } else { value < e.value },
        };
        if better {
            self.extremes.insert(key.to_string(), Extreme { value, what: what.to_string() });
        }
    }

    fn count(&mut self, cat: &str, kind: &str) {
        *self.found.entry(cat.to_string()).or_default().entry(kind.to_string()).or_insert(0) += 1;
    }

    fn body(&mut self, p: &Planet) {
        self.count("World classes", p.class.name());
        if let Some(a) = p.atmosphere {
            self.count("Atmospheres", a.name());
        }
        if let Some(l) = p.life {
            self.count("Life", l.name());
        }
        if let Some(n) = &p.natives {
            self.count("Native civilisations", n.tech.name());
        }
        if let Some(g) = p.gg_feature {
            self.count("Gas giant features", g.name());
        }
        // Belts have no surface to measure.
        if p.radius > 0.0 && p.size_km > 0.0 {
            self.record("Hottest world", p.temp, &p.name);
            self.record("Coldest world", p.temp, &p.name);
            self.record("Strongest gravity", p.gravity, &p.name);
            self.record("Weakest gravity", p.gravity, &p.name);
            self.record("Largest world", p.size_km, &p.name);
            self.record("Smallest world", p.size_km, &p.name);
        }
        let moons = p.satellites.len().max(p.moons);
        if moons > 0 {
            self.record("Most moons", moons as f64, &p.name);
        }
    }

    /// Fold a newly visited system into the records.
    pub fn survey(&mut self, sys: &StarSystem) {
        let s = &sys.star;
        self.count("Star classes", &s.class.glyph().to_string());
        self.count("Star types", s.kind.name());
        self.record("Hottest star", s.temp, &s.name);
        self.record("Coolest star", s.temp, &s.name);
        self.record("Most luminous star", s.lum, &s.name);
        for p in &sys.planets {
            self.body(p);
            for m in &p.satellites {
                self.body(m);
            }
        }
    }
}
