//! Event log with severity levels.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Level {
    Info,
    Good,
    Warn,
    Bad,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub t: f64, // game seconds
    pub level: Level,
    pub msg: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Log {
    pub entries: VecDeque<Entry>,
    /// Total entries ever pushed. The ring evicts its front at CAP, so
    /// `entries.len()` is not a stable position marker — this is.
    #[serde(default)]
    pub total: u64,
}

impl Log {
    pub const CAP: usize = 500;

    pub fn push(&mut self, t: f64, level: Level, msg: impl Into<String>) {
        self.entries.push_back(Entry { t, level, msg: msg.into() });
        self.total += 1;
        while self.entries.len() > Self::CAP {
            self.entries.pop_front();
        }
    }

    pub fn last_n(&self, n: usize) -> Vec<&Entry> {
        let l = self.entries.len();
        let from = l.saturating_sub(n);
        self.entries.iter().skip(from).collect()
    }
}
