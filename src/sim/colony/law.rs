//! Colony laws: each trades something for something else.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Law {
    BeFruitful,
    FamilyPlanning,
    Matchmaking,
    LabourDraft,
    Rationing,
    PublicHealthcare,
    Schooling,
    FreeEnterprise,
    MartialLaw,
    Emancipation,
    /// The captain's line rules: a million is a beginning, not an end.
    Hegemony,
}

/// What the laws in force do, combined. Multipliers start at 1, offsets
/// at 0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LawFx {
    pub fert: f64,
    pub mort: f64,
    pub infant: f64,
    pub health: f64,
    pub happy: f64,
    pub wealth: f64,
    pub build: f64,
    pub produce: f64,
    pub upkeep: f64,
    pub tech_growth: f64,
    pub tech_decay: f64,
    pub workers: f64,
    pub hazard: f64,
    /// Added to the yearly chance that singles pair up.
    pub pairing: f64,
    pub captives: f64,
}

impl Default for LawFx {
    fn default() -> Self {
        LawFx {
            fert: 1.0,
            mort: 1.0,
            infant: 1.0,
            health: 0.0,
            happy: 0.0,
            wealth: 0.0,
            build: 1.0,
            produce: 1.0,
            upkeep: 1.0,
            tech_growth: 1.0,
            tech_decay: 1.0,
            workers: 1.0,
            hazard: 1.0,
            pairing: 0.0,
            captives: 1.0,
        }
    }
}

/// A law cannot be repealed this soon after it is enacted (years).
pub const MIN_TERM_Y: f64 = 1.0;

impl Law {
    pub const ALL: [Law; 11] = [
        Law::BeFruitful,
        Law::FamilyPlanning,
        Law::Matchmaking,
        Law::LabourDraft,
        Law::Rationing,
        Law::PublicHealthcare,
        Law::Schooling,
        Law::FreeEnterprise,
        Law::MartialLaw,
        Law::Emancipation,
        Law::Hegemony,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Law::BeFruitful => "Be Fruitful",
            Law::FamilyPlanning => "Family Planning",
            Law::Matchmaking => "Matchmaking Council",
            Law::LabourDraft => "Labour Draft",
            Law::Rationing => "Rationing",
            Law::PublicHealthcare => "Public Healthcare",
            Law::Schooling => "Compulsory Schooling",
            Law::FreeEnterprise => "Free Enterprise",
            Law::MartialLaw => "Martial Law",
            Law::Emancipation => "Emancipation",
            Law::Hegemony => "Hegemony",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Law::BeFruitful => "Large families are a civic duty, with rewards for every child.",
            Law::FamilyPlanning => "Fewer, better-provided children.",
            Law::Matchmaking => "A council makes sure nobody stays single for long.",
            Law::LabourDraft => "Every able adult owes the colony a work quota.",
            Law::Rationing => "Strict allowances stretch the stockpile.",
            Law::PublicHealthcare => "Free clinics for everyone, paid from the stockpile.",
            Law::Schooling => "Children learn instead of working.",
            Law::FreeEnterprise => "Let the market sort it out.",
            Law::MartialLaw => "Armed patrols keep the wilds and the restless at bay.",
            Law::Emancipation => "Captives are freed and given a stake in the colony.",
            Law::Hegemony => "The captain's house rules by right, and the crown passes down the Royal Lineage.",
        }
    }

    pub fn pros(self) -> &'static str {
        match self {
            Law::BeFruitful => "Fertility +30%",
            Law::FamilyPlanning => "Wealth +0.06, infant deaths −20%",
            Law::Matchmaking => "Singles pair up 95% of years instead of 70%",
            Law::LabourDraft => "Construction +30%, materials +15%",
            Law::Rationing => "Materials upkeep −25%",
            Law::PublicHealthcare => "Mortality −15%, infant deaths −25%",
            Law::Schooling => "Tech growth +40%, tech decay halved",
            Law::FreeEnterprise => "Wealth +0.10",
            Law::MartialLaw => "World dangers −50%",
            Law::Emancipation => "Captive penalties halved",
            Law::Hegemony => "Found more colonies with settlers; buy colony arks; at a million, continue the Royal Lineage",
        }
    }

    pub fn cons(self) -> &'static str {
        match self {
            Law::BeFruitful => "Happiness −0.08, wealth −0.04",
            Law::FamilyPlanning => "Fertility −30%",
            Law::Matchmaking => "Happiness −0.04",
            Law::LabourDraft => "Happiness −0.10",
            Law::Rationing => "Health −0.04, happiness −0.05",
            Law::PublicHealthcare => "Materials upkeep +20%",
            Law::Schooling => "Workers −10%",
            Law::FreeEnterprise => "Happiness −0.03, health −0.02",
            Law::MartialLaw => "Happiness −0.15",
            Law::Emancipation => "Materials −10%",
            Law::Hegemony => "Happiness −0.05, wealth −0.03",
        }
    }

    /// A law that cannot stand alongside this one.
    pub fn conflicts(self) -> Option<Law> {
        match self {
            Law::BeFruitful => Some(Law::FamilyPlanning),
            Law::FamilyPlanning => Some(Law::BeFruitful),
            _ => None,
        }
    }

    /// Materials (t) to enact the law in a colony of `pop`.
    pub fn cost_t(pop: f64) -> f64 {
        (pop * 0.01).max(5.0).round()
    }

    fn apply(self, f: &mut LawFx) {
        match self {
            Law::BeFruitful => {
                f.fert *= 1.3;
                f.happy -= 0.08;
                f.wealth -= 0.04;
            }
            Law::FamilyPlanning => {
                f.fert *= 0.7;
                f.wealth += 0.06;
                f.infant *= 0.8;
            }
            Law::Matchmaking => {
                f.pairing += 0.25;
                f.happy -= 0.04;
            }
            Law::LabourDraft => {
                f.build *= 1.3;
                f.produce *= 1.15;
                f.happy -= 0.10;
            }
            Law::Rationing => {
                f.upkeep *= 0.75;
                f.health -= 0.04;
                f.happy -= 0.05;
            }
            Law::PublicHealthcare => {
                f.mort *= 0.85;
                f.infant *= 0.75;
                f.upkeep *= 1.2;
            }
            Law::Schooling => {
                f.tech_growth *= 1.4;
                f.tech_decay *= 0.5;
                f.workers *= 0.9;
            }
            Law::FreeEnterprise => {
                f.wealth += 0.10;
                f.happy -= 0.03;
                f.health -= 0.02;
            }
            Law::MartialLaw => {
                f.hazard *= 0.5;
                f.happy -= 0.15;
            }
            Law::Emancipation => {
                f.captives *= 0.5;
                f.produce *= 0.9;
            }
            Law::Hegemony => {
                f.happy -= 0.05;
                f.wealth -= 0.03;
            }
        }
    }
}

/// A law in force, and since when (game years).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Enacted {
    pub law: Law,
    pub since: f64,
}

/// The combined effect of the laws in force.
pub fn effects(laws: &[Enacted]) -> LawFx {
    let mut f = LawFx::default();
    for e in laws {
        e.law.apply(&mut f);
    }
    f
}
