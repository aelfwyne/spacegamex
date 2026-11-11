//! Quests: relics found in ruins, and jobs offered at ports — recover an
//! artifact (deliver it for a reward, or keep it for its power), bring a
//! relic to a collector, or deliver cargo to another system.

use super::cargo::Good;
use super::civ::CivTier;
use super::galaxy::Galaxy;
use super::logbuf::Level;
use super::state::{loc_label, Loc, Sim};
use crate::rngx::{derive, mix64, name};
use crate::rngx::GameRng;
use rand::RngExt;
use serde::{Deserialize, Serialize};

/// Most quests you can carry at once.
pub const MAX_QUESTS: usize = 3;
/// Job boards change every 30 days.
const BOARD_PERIOD_H: f64 = 24.0 * 30.0;
/// Hours of searching to recover an artifact once docked at its site.
pub const SEARCH_H: f64 = 6.0;

/// What a kept artifact does for you, permanently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Power {
    /// +15% scoop intake.
    Scoop,
    /// −25% hull damage taken.
    Shields,
    /// +1 attack in battle.
    Attack,
    /// The colony's tech grows 25% faster.
    ColonyTech,
    /// +5% colony health.
    ColonyHealth,
}

impl Power {
    pub const ALL: [Power; 5] = [Power::Scoop, Power::Shields, Power::Attack, Power::ColonyTech, Power::ColonyHealth];

    pub fn describe(self) -> &'static str {
        match self {
            Power::Scoop => "+15% fuel-scoop intake",
            Power::Shields => "hull damage −25%",
            Power::Attack => "+1 Attack in battle",
            Power::ColonyTech => "colony tech grows 25% faster",
            Power::ColonyHealth => "+5% colony health",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artifact {
    pub name: String,
    pub power: Power,
}

/// A collectable found in ruins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relic {
    pub name: String,
    /// Where it was found.
    pub from: String,
    /// What a collector pays (credits).
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Job {
    /// Find the artifact at (system, body) and bring it back.
    Recover { artifact: Artifact, sys: u64, sys_name: String, loc: Loc, site: String, reward: f64 },
    /// Bring any relic to the giver.
    Relic { mult: f64 },
    /// Bring `units` of `good` to a port in another system.
    Deliver { good: Good, units: u32, sys: u64, sys_name: String, reward: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuestState {
    Active,
    /// The artifact is aboard.
    Recovered,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quest {
    pub id: u64,
    pub job: Job,
    /// Who asked, and where.
    pub giver: String,
    pub giver_sys: u64,
    pub giver_sys_name: String,
    pub state: QuestState,
    /// Hours searched so far (recover jobs).
    #[serde(default)]
    pub searched_h: f64,
}

impl Quest {
    pub fn title(&self) -> String {
        match &self.job {
            Job::Recover { artifact, sys_name, .. } => format!("Recover the {} at {}", artifact.name, sys_name),
            Job::Relic { .. } => format!("Bring a relic to {} at {}", self.giver, self.giver_sys_name),
            Job::Deliver { good, units, sys_name, .. } => format!("Deliver {units} × {good} to {sys_name}"),
        }
    }

    pub fn detail(&self) -> String {
        match (&self.job, self.state) {
            (Job::Recover { site, artifact, reward, .. }, QuestState::Active) => {
                format!("Dock at {site} and search ({SEARCH_H:.0} h). Then return it to {} for {reward:.0} cr and a ship upgrade, or keep it: {}.", self.giver_sys_name, artifact.power.describe())
            }
            (Job::Recover { artifact, reward, .. }, QuestState::Recovered) => {
                format!("The {} is aboard. Deliver it at {} for {reward:.0} cr and an upgrade, or keep it ({}).", artifact.name, self.giver_sys_name, artifact.power.describe())
            }
            (Job::Relic { mult }, _) => format!("Any relic from the ruins: they pay {:.0}% of its value.", mult * 100.0),
            (Job::Deliver { reward, .. }, _) => format!("Paid {reward:.0} cr on delivery (any port in that system)."),
        }
    }

    /// The system this quest sends you to next.
    pub fn target_sys(&self) -> u64 {
        match (&self.job, self.state) {
            (Job::Recover { sys, .. }, QuestState::Active) => *sys,
            (Job::Deliver { sys, .. }, _) => *sys,
            _ => self.giver_sys,
        }
    }
}

fn artifact_name(rng: &mut GameRng) -> String {
    const KIND: [&str; 10] = ["Lens", "Engine", "Codex", "Shard", "Crown", "Lattice", "Seed", "Compass", "Heart", "Mirror"];
    format!("{} of {}", KIND[rng.random_range(0..KIND.len())], name(rng))
}

fn relic_name(rng: &mut GameRng) -> String {
    const KIND: [&str; 12] = ["idol", "tablet", "mask", "star chart", "reliquary", "music box", "seal", "urn", "astrolabe", "data core", "funeral crown", "flag"];
    const ADJ: [&str; 8] = ["Gilded", "Shattered", "Ancient", "Singing", "Obsidian", "Crystal", "Bronze", "Painted"];
    format!("{} {}", ADJ[rng.random_range(0..ADJ.len())], KIND[rng.random_range(0..KIND.len())])
}

impl Sim {
    fn powers(&self, p: Power) -> usize {
        self.artifacts.iter().filter(|a| a.power == p).count()
    }

    pub fn artifact_attack(&self) -> f64 {
        self.powers(Power::Attack) as f64
    }

    pub fn artifact_tech_boost(&self) -> f64 {
        1.0 + 0.25 * self.powers(Power::ColonyTech) as f64
    }

    pub fn artifact_health_boost(&self) -> f64 {
        0.05 * self.powers(Power::ColonyHealth) as f64
    }

    pub fn artifact_scoop(&self) -> f64 {
        1.0 + 0.15 * self.powers(Power::Scoop) as f64
    }

    pub fn artifact_shield(&self) -> f64 {
        0.75f64.powi(self.powers(Power::Shields) as i32)
    }

    /// Salvage crews turned up a relic.
    pub(crate) fn find_relic(&mut self, rng: &mut GameRng, place: &str) {
        let r = Relic { name: relic_name(rng), from: place.to_string(), value: rng.random_range(1_500.0..9_000.0f64).round() };
        self.log.push(self.t, Level::Good, format!("Among the ruins of {place}: a {} (worth about {:.0} cr to a collector).", r.name.to_lowercase(), r.value));
        self.relics.push(r);
        self.stats.relics_found += 1;
    }

    /// Jobs on offer at this port (deterministic for the month).
    pub fn job_board(&self, gal: &mut Galaxy) -> Vec<Quest> {
        let sys = gal.system(self.sys_id).clone();
        let Some(civ) = sys.civ.clone().filter(|c| c.tier >= CivTier::T2) else { return vec![] };
        let epoch = (self.t_h() / BOARD_PERIOD_H).floor() as u64;
        let mut rng = derive(self.seed ^ mix64(self.sys_id), 0x0b_0a4d ^ epoch);
        let near = gal.neighbors(sys.pos, 25.0, self.sys_id);
        let mut out = Vec::new();
        let n = if civ.tier == CivTier::T3 { 3 } else { 2 };
        for k in 0..n {
            let id = mix64(self.sys_id ^ epoch.rotate_left(20) ^ k as u64);
            let pick = rng.random_range(0..10);
            let job = if pick < 5 && !near.is_empty() {
                // An artifact somewhere nearby, at a body of that system.
                let n = &near[rng.random_range(0..near.len().min(8))];
                let ts = gal.system(n.id).clone();
                let sites = ts.sites();
                let bodies: Vec<Loc> = sites.iter().filter(|(_, p)| p.class != super::planet::PlanetClass::GasGiant).map(|(l, _)| *l).collect();
                let Some(&loc) = bodies.get(rng.random_range(0..bodies.len().max(1))) else { continue };
                let power = Power::ALL[rng.random_range(0..Power::ALL.len())];
                Job::Recover {
                    artifact: Artifact { name: artifact_name(&mut rng), power },
                    sys: n.id,
                    sys_name: n.star_name.clone(),
                    loc,
                    site: loc_label(loc, &ts),
                    reward: (4_000.0 + 600.0 * n.dist_ly * rng.random_range(0.8..1.3)).round(),
                }
            } else if pick < 7 {
                Job::Relic { mult: rng.random_range(1.4..1.9) }
            } else if !near.is_empty() {
                let n = &near[rng.random_range(0..near.len().min(10))];
                const GOODS: [Good; 6] = [Good::Machinery, Good::Electronics, Good::Pharma, Good::Textiles, Good::MedKit, Good::Luxury];
                let good = GOODS[rng.random_range(0..GOODS.len())];
                let units = (rng.random_range(2.0..12.0) / good.mass_t().max(0.2)).round().clamp(1.0, 40.0) as u32;
                let reward = (good.base_price() * units as f64 * rng.random_range(1.25..1.6) + 300.0 * n.dist_ly).round();
                Job::Deliver { good, units, sys: n.id, sys_name: n.star_name.clone(), reward }
            } else {
                continue;
            };
            out.push(Quest { id, job, giver: civ.display(), giver_sys: self.sys_id, giver_sys_name: sys.star.name.clone(), state: QuestState::Active, searched_h: 0.0 });
        }
        // Jobs already taken are not offered again.
        out.retain(|q| !self.quests.iter().any(|a| a.id == q.id) && !self.done_quests.contains(&q.id));
        out
    }

    pub fn accept_quest(&mut self, q: Quest) -> bool {
        if self.quests.len() >= MAX_QUESTS {
            self.log.push(self.t, Level::Warn, format!("You can take at most {MAX_QUESTS} jobs at a time."));
            return false;
        }
        self.log.push(self.t, Level::Info, format!("Job taken: {}.", q.title()));
        self.quests.push(q);
        true
    }

    pub fn drop_quest(&mut self, id: u64) {
        if let Some(i) = self.quests.iter().position(|q| q.id == id) {
            let q = self.quests.remove(i);
            self.done_quests.push(q.id);
            self.stats.quests_dropped += 1;
            self.log.push(self.t, Level::Info, format!("Job abandoned: {}.", q.title()));
        }
    }

    /// Search the current site for a quest artifact (takes hours).
    pub fn search_artifact(&mut self, gal: &mut Galaxy) -> bool {
        let here = (self.sys_id, self.loc);
        let Some(i) = self.quests.iter().position(|q| q.state == QuestState::Active && matches!(&q.job, Job::Recover { sys, loc, .. } if (*sys, *loc) == here)) else {
            self.log.push(self.t, Level::Warn, "No artifact you are looking for is here.");
            return false;
        };
        if self.trip.is_some() || self.shuttle.is_some() {
            return false;
        }
        self.advance(SEARCH_H, gal);
        let q = &mut self.quests[i];
        q.searched_h += SEARCH_H;
        q.state = QuestState::Recovered;
        let Job::Recover { artifact, .. } = &q.job else { return false };
        let msg = format!("After {SEARCH_H:.0} hours of searching you recover the {}.", artifact.name);
        self.log.push(self.t, Level::Good, msg);
        true
    }

    /// Hand in what a quest asks for, here. `keep`: keep a recovered
    /// artifact for its power instead of delivering it.
    pub fn complete_quest(&mut self, id: u64, keep: bool) -> bool {
        let Some(i) = self.quests.iter().position(|q| q.id == id) else { return false };
        let q = self.quests[i].clone();
        let docked_port = self.trip.is_none();
        match (&q.job, q.state) {
            (Job::Recover { artifact, .. }, QuestState::Recovered) if keep => {
                self.artifacts.push(artifact.clone());
                self.stats.artifacts_kept += 1;
                self.log.push(self.t, Level::Good, format!("You keep the {}: {}.", artifact.name, artifact.power.describe()));
            }
            (Job::Recover { artifact, reward, .. }, QuestState::Recovered) => {
                if self.sys_id != q.giver_sys || !docked_port {
                    self.log.push(self.t, Level::Warn, format!("Deliver it at {}.", q.giver_sys_name));
                    return false;
                }
                self.credits += reward;
                let up = if self.attack_lvl <= self.defense_lvl && self.attack_lvl < super::combat::MAX_LEVEL {
                    self.attack_lvl += 1;
                    format!("Attack level {}", self.attack_lvl)
                } else if self.defense_lvl < super::combat::MAX_LEVEL {
                    self.defense_lvl += 1;
                    format!("Defense level {}", self.defense_lvl)
                } else {
                    "nothing more to fit".into()
                };
                self.log.push(self.t, Level::Good, format!("{} receives the {}: +{reward:.0} cr, and their yard fits {up}.", q.giver, artifact.name));
                self.stats.artifacts_delivered += 1;
            }
            (Job::Relic { mult }, _) => {
                if self.sys_id != q.giver_sys || !docked_port {
                    self.log.push(self.t, Level::Warn, format!("Bring it to {}.", q.giver_sys_name));
                    return false;
                }
                let Some(best) = self.relics.iter().enumerate().max_by(|a, b| a.1.value.total_cmp(&b.1.value)).map(|(i, _)| i) else {
                    self.log.push(self.t, Level::Warn, "You have no relic. Salvage ruins to find one.");
                    return false;
                };
                let r = self.relics.remove(best);
                let pay = (r.value * mult).round();
                self.credits += pay;
                self.log.push(self.t, Level::Good, format!("{} pays {pay:.0} cr for the {}.", q.giver, r.name.to_lowercase()));
            }
            (Job::Deliver { good, units, sys, sys_name, reward }, _) => {
                if self.sys_id != *sys || !docked_port {
                    self.log.push(self.t, Level::Warn, format!("Deliver it at {sys_name}."));
                    return false;
                }
                if self.hold.get(*good) < *units {
                    self.log.push(self.t, Level::Warn, format!("You need {units} × {good} aboard."));
                    return false;
                }
                self.hold.take(*good, *units);
                self.credits += reward;
                self.log.push(self.t, Level::Good, format!("Delivered {units} × {good}: +{reward:.0} cr."));
            }
            _ => return false,
        }
        self.quests.remove(i);
        self.done_quests.push(q.id);
        self.stats.quests_done += 1;
        true
    }

    /// Sell a relic at an interstellar station.
    pub fn sell_relic(&mut self, gal: &Galaxy, idx: usize) -> bool {
        if gal.system_ref(self.sys_id).civ.as_ref().map(|c| c.tier) != Some(CivTier::T3) {
            self.log.push(self.t, Level::Warn, "Only interstellar stations have collectors.");
            return false;
        }
        if idx >= self.relics.len() {
            return false;
        }
        let r = self.relics.remove(idx);
        self.credits += r.value;
        self.stats.relics_sold += 1;
        self.log.push(self.t, Level::Good, format!("Sold the {} for {:.0} cr.", r.name.to_lowercase(), r.value));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::civ::{Civ, CivKind};

    fn at_port() -> (Sim, Galaxy) {
        let mut gal = Galaxy::new(42);
        let mut sim = Sim::new_game(42, 1, &mut gal);
        let id = sim.sys_id;
        gal.system(id).civ = Some(Civ { tier: CivTier::T3, name: "Port".into(), kind: CivKind::Colony, seat: Loc::Planet(0), small: false });
        sim.credits = 0.0;
        (sim, gal)
    }

    #[test]
    fn boards_are_stable_for_a_month() {
        let (sim, mut gal) = at_port();
        let a = sim.job_board(&mut gal);
        let b = sim.job_board(&mut gal);
        assert!(!a.is_empty());
        assert_eq!(a, b);
    }

    #[test]
    fn recover_then_deliver_or_keep() {
        for keep in [false, true] {
            let (mut sim, mut gal) = at_port();
            let q = (0..40)
                .find_map(|m| {
                    sim.t = m as f64 * BOARD_PERIOD_H * 3600.0;
                    sim.job_board(&mut gal).into_iter().find(|q| matches!(q.job, Job::Recover { .. }))
                })
                .expect("an artifact job");
            let Job::Recover { sys, loc, reward, .. } = q.job.clone() else { unreachable!() };
            let home = sim.sys_id;
            assert!(sim.accept_quest(q.clone()));
            // Go there (teleport for the test) and search.
            gal.system(sys);
            sim.sys_id = sys;
            sim.loc = loc;
            assert!(sim.search_artifact(&mut gal));
            sim.encounter = None;
            assert_eq!(sim.quests[0].state, QuestState::Recovered);
            if keep {
                assert!(sim.complete_quest(q.id, true));
                assert_eq!(sim.artifacts.len(), 1);
            } else {
                // Not here: the giver is elsewhere.
                assert!(!sim.complete_quest(q.id, false));
                sim.sys_id = home;
                let lvl = sim.attack_lvl + sim.defense_lvl;
                let cr = sim.credits;
                assert!(sim.complete_quest(q.id, false));
                assert_eq!(sim.credits, cr + reward);
                assert_eq!(sim.attack_lvl + sim.defense_lvl, lvl + 1);
            }
            assert!(sim.quests.is_empty());
            assert!(!sim.job_board(&mut gal).iter().any(|b| b.id == q.id), "a finished job is not offered again");
        }
    }

    #[test]
    fn relics_sell_and_fill_relic_jobs() {
        let (mut sim, mut gal) = at_port();
        let mut rng = derive(1, 2);
        sim.find_relic(&mut rng, "Old Ruins");
        sim.find_relic(&mut rng, "Old Ruins");
        let v = sim.relics[0].value;
        assert!(sim.sell_relic(&gal, 0));
        assert_eq!(sim.credits, v);
        let q = Quest { id: 7, job: Job::Relic { mult: 1.5 }, giver: "x".into(), giver_sys: sim.sys_id, giver_sys_name: "here".into(), state: QuestState::Active, searched_h: 0.0 };
        sim.accept_quest(q);
        assert!(sim.complete_quest(7, false));
        assert!(sim.relics.is_empty());
        let _ = &mut gal;
    }
}
