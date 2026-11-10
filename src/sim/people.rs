//! People: sex, ages and the game calendar (365-day years).

use serde::{Deserialize, Serialize};

pub const SECS_PER_DAY: f64 = 86_400.0;
pub const DAYS_PER_YEAR: f64 = 365.0;
pub const SECS_PER_YEAR: f64 = SECS_PER_DAY * DAYS_PER_YEAR;
pub const HOURS_PER_YEAR: f64 = 24.0 * DAYS_PER_YEAR;

/// Game seconds → game years since the start.
pub fn years(t_sec: f64) -> f64 {
    t_sec / SECS_PER_YEAR
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Sex {
    #[default]
    Male,
    Female,
}

impl Sex {
    pub const ALL: [Sex; 2] = [Sex::Male, Sex::Female];

    pub fn opposite(self) -> Sex {
        match self {
            Sex::Male => Sex::Female,
            Sex::Female => Sex::Male,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Sex::Male => "Male",
            Sex::Female => "Female",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Sex::Male => "♂",
            Sex::Female => "♀",
        }
    }

    pub fn is_female(self) -> bool {
        self == Sex::Female
    }

    pub fn from_female(f: bool) -> Sex {
        if f { Sex::Female } else { Sex::Male }
    }
}
