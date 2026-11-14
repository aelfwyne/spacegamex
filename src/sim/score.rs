//! The final score, and the scoreboard kept across games.

use super::people::years;
use super::state::Sim;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Victory,
    Retired,
    Lost,
}

impl Outcome {
    pub fn name(self) -> &'static str {
        match self {
            Outcome::Victory => "Victory",
            Outcome::Retired => "Retired",
            Outcome::Lost => "Lost",
        }
    }

    pub fn mult(self) -> f64 {
        match self {
            Outcome::Victory => 1.5,
            Outcome::Retired => 1.0,
            Outcome::Lost => 0.75,
        }
    }
}

/// Cause of a game that ended by retirement.
pub const RETIRED: &str = "Retired";

#[derive(Debug, Clone)]
pub struct ScoreCard {
    /// (what, how much, points)
    pub lines: Vec<(&'static str, String, f64)>,
    pub base: f64,
    pub outcome: Outcome,
    pub total: u64,
}

impl Sim {
    pub fn outcome(&self) -> Outcome {
        match self.over.as_ref().map(|o| o.cause.as_str()) {
            Some("Victory") => Outcome::Victory,
            // A dynasty that reached a million and ruled on still won.
            Some(RETIRED) if self.colony.as_ref().is_some_and(|c| c.won) => Outcome::Victory,
            Some(RETIRED) => Outcome::Retired,
            _ => Outcome::Lost,
        }
    }

    /// Game years played.
    pub fn years_played(&self) -> f64 {
        years(self.over.as_ref().map(|o| o.t).unwrap_or(self.t))
    }

    pub fn score(&self) -> ScoreCard {
        let st = &self.stats;
        let y = self.years_played();
        let lines = vec![
            ("Years survived", format!("{y:.1}"), 100.0 * y),
            ("Systems visited", st.systems.to_string(), 50.0 * st.systems as f64),
            ("Light-years flown", format!("{:.0}", st.dist_ly), 2.0 * st.dist_ly),
            ("Credits", format!("{:.0}", self.credits), self.credits.max(0.0) / 100.0),
            ("Battles won", self.battles_won.to_string(), 250.0 * self.battles_won as f64),
            ("Jobs completed", st.quests_done.to_string(), 300.0 * st.quests_done as f64),
            ("Artifacts kept", self.artifacts.len().to_string(), 500.0 * self.artifacts.len() as f64),
            ("Colony peak population", format!("{:.0}", st.colony_peak), 2_000.0 * (st.colony_peak + 1.0).log10()),
            ("Colony births", format!("{:.0}", st.colony_births), 25.0 * st.colony_births),
        ];
        let base: f64 = lines.iter().map(|l| l.2).sum();
        let outcome = self.outcome();
        ScoreCard { lines, base, outcome, total: (base * outcome.mult()).round().max(0.0) as u64 }
    }
}

// ---------------------------------------------------------------- board

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub captain: String,
    pub score: u64,
    pub outcome: Outcome,
    pub ship: String,
    pub background: String,
    pub years: f64,
    pub colony_peak: f64,
    pub seed: u64,
    /// Real-world date (YYYY-MM-DD).
    pub date: String,
}

/// Entries kept on the board.
pub const KEEP: usize = 100;

pub fn path() -> std::path::PathBuf {
    super::save::data_dir().join("scores.json")
}

/// Every entry, best first (empty if there is no board yet).
pub fn load() -> Vec<Entry> {
    std::fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Add an entry; returns its rank (1 = best), if it made the board.
pub fn record(e: Entry) -> Option<usize> {
    let mut v = load();
    v.push(e.clone());
    v.sort_by(|a, b| b.score.cmp(&a.score));
    v.truncate(KEEP);
    let rank = v.iter().position(|x| x.score == e.score && x.captain == e.captain && x.date == e.date && x.seed == e.seed).map(|i| i + 1);
    let p = path();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(&v) {
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
    rank
}

/// Today's date from the system clock (UTC).
pub fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (secs / 86_400) as i64;
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

impl Sim {
    /// This game's scoreboard entry.
    pub fn score_entry(&self) -> Entry {
        Entry {
            captain: self.player_name().to_string(),
            score: self.score().total,
            outcome: self.outcome(),
            ship: self.ship().name.to_string(),
            background: self.background.name().to_string(),
            years: self.years_played(),
            colony_peak: self.stats.colony_peak,
            seed: self.seed,
            date: today(),
        }
    }

    /// End the game by choice. Unlike a loss, a standing colony does not
    /// carry play on.
    pub fn retire_now(&mut self) {
        if self.over.is_some() {
            return;
        }
        let detail = format!("Captain {} hangs up the flight jacket after {:.1} years.", self.player_name(), self.years_played());
        self.log.push(self.t, super::logbuf::Level::Good, detail.clone());
        self.over = Some(super::state::GameOver { cause: RETIRED.into(), detail, t: self.t });
    }
}
