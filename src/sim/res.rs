//! Resources, inventory and small formatting helpers.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Every material the ship knows about. First six are raw (from scoop / drones),
/// the rest are processed on board by the fabrication bay.
/// Toolkit-neutral sRGB color. The sim describes *what* things look like;
/// the GUI decides how to draw them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Res {
    // raw
    H2,
    Ice,
    Metal,
    Silicate,
    Carbon,
    Nitrogen,
    // processed
    Water,
    Oxygen,
    Food,
    Mat,
    Med,
}

impl Res {
    pub const ALL: [Res; 11] = [
        Res::H2,
        Res::Ice,
        Res::Metal,
        Res::Silicate,
        Res::Carbon,
        Res::Nitrogen,
        Res::Water,
        Res::Oxygen,
        Res::Food,
        Res::Mat,
        Res::Med,
    ];

    pub const RAW: [Res; 6] = [
        Res::H2,
        Res::Ice,
        Res::Metal,
        Res::Silicate,
        Res::Carbon,
        Res::Nitrogen,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Res::H2 => "Hydrogen (H2)",
            Res::Ice => "Water ice",
            Res::Metal => "Metal (Fe/Ni)",
            Res::Silicate => "Silicate",
            Res::Carbon => "Carbon",
            Res::Nitrogen => "Nitrogen",
            Res::Water => "Potable water",
            Res::Oxygen => "Oxygen",
            Res::Food => "Synthetic food",
            Res::Mat => "Structural mat",
            Res::Med => "Med supplies",
        }
    }

    pub fn short(&self) -> &'static str {
        match self {
            Res::H2 => "H2",
            Res::Ice => "ICE",
            Res::Metal => "MET",
            Res::Silicate => "SIL",
            Res::Carbon => "CAR",
            Res::Nitrogen => "NIT",
            Res::Water => "WAT",
            Res::Oxygen => "OXY",
            Res::Food => "FOO",
            Res::Mat => "MAT",
            Res::Med => "MED",
        }
    }

    pub fn desc(&self) -> &'static str {
        match self {
            Res::H2 => "Reactor fuel. Scoop it from stars & gas giants; electrolysis also makes some.",
            Res::Ice => "Mined from ice worlds & KBOs. Feed to water reclamation.",
            Res::Metal => "Mined from rocky worlds. Hull repair, materials, medical.",
            Res::Silicate => "Mined from rocky worlds. Mixed into structural material.",
            Res::Carbon => "Mined from carbonaceous bodies. Food synthesis.",
            Res::Nitrogen => "Mined from ice giants / N2 ice. Food synthesis, atmospheres.",
            Res::Water => "Drinking + processing feed. From ice reclamation.",
            Res::Oxygen => "Breathing. From water electrolysis (yields bonus H2).",
            Res::Food => "Crew & passenger nutrition. From bio-manufacturing.",
            Res::Mat => "Processed hull/structure material. Repairs the ship.",
            Res::Med => "Medical supplies. Accelerate recovery of hurt personnel.",
        }
    }

    pub fn color(&self) -> Rgb {
        match self {
            Res::H2 => Rgb(90, 220, 235),
            Res::Ice => Rgb(150, 200, 255),
            Res::Metal => Rgb(200, 205, 215),
            Res::Silicate => Rgb(225, 205, 140),
            Res::Carbon => Rgb(150, 210, 120),
            Res::Nitrogen => Rgb(130, 160, 255),
            Res::Water => Rgb(70, 140, 255),
            Res::Oxygen => Rgb(90, 220, 140),
            Res::Food => Rgb(240, 200, 80),
            Res::Mat => Rgb(220, 130, 230),
            Res::Med => Rgb(240, 90, 90),
        }
    }

    pub fn is_raw(&self) -> bool {
        matches!(self, Res::H2 | Res::Ice | Res::Metal | Res::Silicate | Res::Carbon | Res::Nitrogen)
    }
}

impl std::fmt::Display for Res {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.short())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Inventory {
    pub stock: BTreeMap<Res, f64>,
    pub caps: BTreeMap<Res, f64>,
}

impl Inventory {
    pub fn new(caps: &[(Res, f64)]) -> Self {
        let mut inv = Self::default();
        for (r, c) in caps {
            inv.stock.insert(*r, 0.0);
            inv.caps.insert(*r, *c);
        }
        inv
    }

    pub fn get(&self, r: Res) -> f64 {
        self.stock.get(&r).copied().unwrap_or(0.0)
    }

    pub fn cap(&self, r: Res) -> f64 {
        self.caps.get(&r).copied().unwrap_or(0.0)
    }

    /// Add, clamped to capacity. Returns the amount actually added.
    pub fn add(&mut self, r: Res, amt: f64) -> f64 {
        if amt <= 0.0 {
            return 0.0;
        }
        let room = (self.cap(r) - self.get(r)).max(0.0);
        let x = amt.min(room);
        *self.stock.entry(r).or_insert(0.0) += x;
        x
    }

    /// Remove, clamped to stock. Returns the amount actually removed.
    pub fn take(&mut self, r: Res, amt: f64) -> f64 {
        if amt <= 0.0 {
            return 0.0;
        }
        let have = self.get(r);
        let x = amt.min(have);
        *self.stock.entry(r).or_insert(0.0) = have - x;
        x
    }

    pub fn fraction(&self, r: Res) -> f64 {
        let c = self.cap(r);
        if c <= 0.0 {
            return 1.0;
        }
        (self.get(r) / c).clamp(0.0, 1.0)
    }

    pub fn fill_to_fraction(&mut self, r: Res, f: f64) {
        let target = self.cap(r) * f;
        let cur = self.get(r);
        if target > cur {
            *self.stock.entry(r).or_insert(0.0) = target;
        }
    }
}

// ---------- formatting ----------

pub fn fmt_kg(v: f64) -> String {
    let v = v.abs();
    if v >= 1_000_000.0 {
        format!("{:.2}M", v / 1_000_000.0)
    } else if v >= 10_000.0 {
        format!("{:.1}k", v / 1_000.0)
    } else if v >= 100.0 {
        format!("{:.0}", v)
    } else {
        format!("{:.1}", v)
    }
}

/// Hours → "3d 4h" / "4h 20m" / "12m" / "45s"
pub fn fmt_hours(h: f64) -> String {
    if h.is_nan() || h.is_infinite() {
        return "—".into();
    }
    let h = h.max(0.0);
    let d = (h / 24.0) as u64;
    let rem = h - d as f64 * 24.0;
    let total_min = (rem * 60.0) as u64; // minutes within the day: 0..1439
    let hh = total_min / 60;
    let mm = total_min % 60;
    let ss = ((rem * 3600.0) as u64) % 60;
    if d > 0 {
        format!("{}d {}h", d, hh)
    } else if hh > 0 {
        format!("{}h {}m", hh, mm)
    } else if mm > 0 {
        format!("{}m", mm)
    } else if ss > 0 {
        format!("{}s", ss)
    } else {
        format!("{}h", hh)
    }
}

/// In-game seconds → "Year 1 · Day 12 · 08:34" (365-day years).
pub fn fmt_daytime(t_sec: f64) -> String {
    let t = t_sec.max(0.0);
    let days = (t / 86_400.0) as u64;
    let (y, d) = (days / 365, days % 365);
    let rem = t - days as f64 * 86_400.0;
    let h = (rem / 3600.0) as u64;
    let m = ((rem % 3600.0) / 60.0) as u64;
    format!("Year {} · Day {} · {:02}:{:02}", y + 1, d + 1, h, m)
}

/// In-game seconds → "Year 12, Day 140" (no clock: for colony time).
pub fn fmt_date(t_sec: f64) -> String {
    let days = (t_sec.max(0.0) / 86_400.0) as u64;
    format!("Year {}, Day {}", days / 365 + 1, days % 365 + 1)
}

pub fn fmt_f1(v: f64) -> String {
    format!("{:.1}", v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_rolls_over_at_365_days() {
        assert_eq!(fmt_daytime(0.0), "Year 1 · Day 1 · 00:00");
        assert_eq!(fmt_daytime(365.0 * 86_400.0 + 3_660.0), "Year 2 · Day 1 · 01:01");
        assert_eq!(fmt_date(364.5 * 86_400.0), "Year 1, Day 365");
    }

    /// Resource labels are drawn in their resource color on a dark theme.
    /// Carbon was once mapped to black and lost its label entirely; keep
    /// every resource bright enough to read (perceived luma well above 0).
    #[test]
    fn no_resource_is_too_dark() {
        for r in Res::ALL {
            let c = r.color();
            let luma = 0.299 * c.0 as f64 + 0.587 * c.1 as f64 + 0.114 * c.2 as f64;
            assert!(luma > 90.0, "{r} is unreadable on a dark background");
        }
    }
}
