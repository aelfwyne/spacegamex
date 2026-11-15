//! The colony census: every colonist's life, partner, parents and children.
//!
//! While the colony is small (up to [`DETAIL_LIMIT`] living records) every
//! colonist is an individual record. Past that, newborns join anonymous age
//! cohorts and only the records already kept (and the captain's family)
//! stay individual. Anyone the player looks at is *materialised* from the
//! cohorts on demand — with parents, a partner and children — and kept, so
//! the census looks as if everyone always existed.
//!
//! Nobody stores a name: names are derived from the record id, with the
//! family name passed down the father's line.

use crate::rngx::derive;
use rand::RngExt;

/// The census's own random numbers: the game generator, keyed through the
/// MurmurHash3 finalizer so its streams stay apart from the galaxy's.
type CensusRng = crate::rngx::GameRng;

fn fmix64(mut k: u64) -> u64 {
    k ^= k >> 33;
    k = k.wrapping_mul(0xff51_afd7_ed55_8ccd);
    k ^= k >> 33;
    k = k.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    k ^ (k >> 33)
}
use serde::{Deserialize, Serialize};

/// Living individual records before newborns go to the cohorts.
pub const DETAIL_LIMIT: usize = 10_000;
pub const MAX_AGE: usize = 110;
pub const ADULT: f64 = 18.0;
pub const FERTILE_END: f64 = 45.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    /// Came with the founding ship.
    Founder,
    /// Born in the colony.
    Born,
    /// A captive bought from pre-space natives.
    Captive,
    /// Drawn from the demographic bulk when someone looked at them.
    Settler,
}

fn no(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    #[serde(rename = "f")]
    pub female: bool,
    /// Birth date (game years).
    #[serde(rename = "b")]
    pub born: f32,
    #[serde(rename = "d", default, skip_serializing_if = "Option::is_none")]
    pub died: Option<f32>,
    #[serde(rename = "m", default, skip_serializing_if = "Option::is_none")]
    pub mother: Option<u32>,
    #[serde(rename = "p", default, skip_serializing_if = "Option::is_none")]
    pub father: Option<u32>,
    #[serde(rename = "s", default, skip_serializing_if = "Option::is_none")]
    pub spouse: Option<u32>,
    /// A native species (captives and their children): cannot have children
    /// with humans, except a couple licensed by a Genetic Modification Kit.
    #[serde(rename = "a", default, skip_serializing_if = "no")]
    pub alien: bool,
    /// A child of a human and a native (or of hybrids): has children only
    /// with other hybrids.
    #[serde(rename = "h", default, skip_serializing_if = "no")]
    pub hybrid: bool,
    /// Married to a human (or native) under a Genetic Modification Kit:
    /// the couple can have (hybrid) children.
    #[serde(rename = "k", default, skip_serializing_if = "no")]
    pub licensed: bool,
    #[serde(rename = "o")]
    pub origin: Origin,
    /// Pregnancy due date (game years).
    #[serde(rename = "g", default, skip_serializing_if = "Option::is_none")]
    pub due: Option<f32>,
    /// Children born while the colony was demographic and not yet looked
    /// at: their birth dates (on the mother).
    #[serde(rename = "u", default, skip_serializing_if = "Vec::is_empty")]
    pub unseen: Vec<f32>,
    /// Parents not yet materialised.
    #[serde(rename = "q", default, skip_serializing_if = "no")]
    pub parents_unseen: bool,
    /// The captain's family line: their children always become records.
    #[serde(rename = "l", default, skip_serializing_if = "no")]
    pub line: bool,
}

impl Person {
    pub fn alive(&self) -> bool {
        self.died.is_none()
    }

    pub fn age(&self, now: f64) -> f64 {
        let end = self.died.map(|d| d as f64).unwrap_or(now);
        (end - self.born as f64).max(0.0)
    }

    pub fn species(&self) -> Species {
        if self.hybrid {
            Species::Hybrid
        } else if self.alien {
            Species::Native
        } else {
            Species::Human
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Species {
    Human,
    Native,
    Hybrid,
}

impl Species {
    pub fn name(self) -> &'static str {
        match self {
            Species::Human => "Human",
            Species::Native => "Native",
            Species::Hybrid => "Hybrid",
        }
    }
}

/// A human and a native: they need a Genetic Modification Kit to have
/// children together.
pub fn mixed(a: &Person, b: &Person) -> bool {
    !a.hybrid && !b.hybrid && a.alien != b.alien
}

/// Can this couple have children?
pub fn fertile_pair(a: &Person, b: &Person) -> bool {
    a.species() == b.species() || (mixed(a, b) && a.licensed && b.licensed)
}

/// (alien, hybrid) of a child of `mother` and `father`.
fn child_species(mother: &Person, father: Option<&Person>) -> (bool, bool) {
    match father {
        Some(f) if mother.hybrid || f.hybrid || f.alien != mother.alien => (false, true),
        _ => (mother.alien && !mother.hybrid, mother.hybrid),
    }
}

/// Someone who left on a colony ark, as their new world will know them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Emigrant {
    pub female: bool,
    /// Birth date (game years).
    pub born: f32,
    pub given: String,
    pub family: String,
    /// Of the captain's royal line.
    pub royal: bool,
    pub alien: bool,
    #[serde(default)]
    pub hybrid: bool,
}

/// Per-year rates the colony's conditions set for this step.
#[derive(Debug, Clone, Copy)]
pub struct Rates {
    /// Multiplier on age mortality (1 = a healthy modern colony).
    pub mort_mult: f64,
    /// Background death hazard per year (world dangers).
    pub hazard: f64,
    /// Births per year for a partnered woman of child-bearing age.
    pub fert: f64,
    /// Share of babies who die in their first year.
    pub infant: f64,
    /// Chance a birth kills the mother.
    pub maternal: f64,
}

impl Default for Rates {
    fn default() -> Self {
        Rates { mort_mult: 1.0, hazard: 0.002, fert: 0.2, infant: 0.01, maternal: 0.001 }
    }
}

/// Annual chance of dying at `age` (Gompertz) before multipliers.
pub fn base_q(age: f64) -> f64 {
    (0.000_22 * (0.085 * age).exp()).min(1.0)
}

pub fn q_year(age: f64, r: &Rates) -> f64 {
    (base_q(age) * r.mort_mult + r.hazard).clamp(0.0, 1.0)
}

/// Something worth telling the player.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// (mother, father) — the captain's own family.
    Pregnant(u32, u32),
    Born(u32),
    Died(u32),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct YearStats {
    pub births: f64,
    pub deaths: f64,
    pub infant_deaths: f64,
    pub maternal_deaths: f64,
    /// Sum of ages at death (for the mean lifespan).
    pub death_age_sum: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Census {
    pub seed: u64,
    /// Every individual record ever made; the index is the id.
    pub people: Vec<Person>,
    /// Ids of living records.
    #[serde(default)]
    pub living: Vec<u32>,
    /// Anonymous bulk by age: [male, female].
    #[serde(default)]
    pub cohorts: Vec<[f64; 2]>,
    pub demographic: bool,
    /// Draws made for materialisation (keeps it deterministic).
    #[serde(default)]
    pub draws: u64,
    /// Babies' sexes drawn so far: their own stream, apart from every
    /// other roll.
    #[serde(default)]
    pub sex_draws: u64,
    /// Given names fixed by the story (the founding crew).
    #[serde(default)]
    pub given: std::collections::BTreeMap<u32, String>,
    /// Family names fixed by the story (the captain's), by line founder.
    #[serde(default)]
    pub family: std::collections::BTreeMap<u32, String>,
    /// Records who left on a colony ark, and when (game years).
    #[serde(default)]
    pub emigrated: std::collections::BTreeMap<u32, f32>,
    /// Added to the yearly chance that singles pair up (laws).
    #[serde(default)]
    pub pair_bonus: f64,
    #[serde(default)]
    pub this_year: YearStats,
    #[serde(default)]
    pub last_year: YearStats,
    /// When the colony was founded (game years): nobody born earlier has
    /// parents in the census.
    pub founded: f32,
    /// Test harness clock (whole years run).
    #[cfg(test)]
    #[serde(skip)]
    pub draws_year: usize,
}

impl Census {
    pub fn new(seed: u64, founded: f64) -> Census {
        Census { seed, founded: founded as f32, cohorts: vec![[0.0; 2]; MAX_AGE + 1], ..Default::default() }
    }

    pub fn get(&self, id: u32) -> Option<&Person> {
        self.people.get(id as usize)
    }

    fn rng(&mut self, salt: u64) -> CensusRng {
        self.draws += 1;
        crate::rngx::seeded(fmix64(self.seed ^ fmix64(self.draws ^ salt.rotate_left(29))))
    }

    /// A baby's sex: a fair coin from a stream used for nothing else.
    pub fn sex_roll(&mut self) -> bool {
        self.sex_draws += 1;
        let mut r = crate::rngx::seeded(fmix64(self.seed ^ 0x5e_c5_0f_ba_b1_e5 ^ fmix64(self.sex_draws)));
        r.random::<u64>() >> 63 == 1
    }

    pub fn add(&mut self, p: Person) -> u32 {
        let id = self.people.len() as u32;
        if p.alive() {
            self.living.push(id);
        }
        self.people.push(p);
        id
    }

    /// Living and still here (not gone on an ark).
    pub fn here(&self, id: u32) -> bool {
        self.get(id).is_some_and(|p| p.alive()) && !self.emigrated.contains_key(&id)
    }

    /// Send `n` people away on an ark: the records in `first` (with their
    /// partners, unless a partner is in `keep`), then adults from the
    /// anonymous bulk, then other adult records not in `keep`.
    pub fn emigrate(&mut self, first: &[u32], n: usize, keep: &std::collections::BTreeSet<u32>, now: f64) -> Vec<Emigrant> {
        let mut out = Vec::new();
        let take = |c: &mut Census, id: u32, out: &mut Vec<Emigrant>| {
            if out.len() >= n || !c.here(id) || keep.contains(&id) {
                return;
            }
            let p = c.people[id as usize].clone();
            out.push(Emigrant { female: p.female, born: p.born, given: c.given_name(id), family: c.family_name(id), royal: p.line, alien: p.alien, hybrid: p.hybrid });
            c.emigrated.insert(id, now as f32);
            c.living.retain(|&i| i != id);
        };
        for &id in first {
            take(self, id, &mut out);
            if let Some(sp) = self.people.get(id as usize).and_then(|p| p.spouse) {
                if keep.contains(&sp) {
                    // Parted: the one who stays may marry again.
                    self.people[sp as usize].spouse = None;
                } else {
                    take(self, sp, &mut out);
                }
            }
        }
        // The anonymous bulk: adults, in proportion to who is there.
        let adults = |c: &Census| -> f64 { c.cohorts.iter().enumerate().filter(|(a, _)| (18..=45).contains(a)).map(|(_, v)| v[0] + v[1]).sum() };
        let mut rng = self.rng(0xa7_c5);
        while out.len() < n && adults(self) >= 1.0 {
            let mut r = rng.random_range(0.0..adults(self));
            let mut pick = None;
            'find: for a in 18..=45usize {
                for sex in 0..2 {
                    let v = self.cohorts[a][sex];
                    if r < v && v >= 1.0 {
                        pick = Some((a, sex));
                        break 'find;
                    }
                    r -= v;
                }
            }
            let Some((a, sex)) = pick else { break };
            self.cohorts[a][sex] -= 1.0;
            let mut nr = derive(self.seed ^ 0xe1_9a, self.draws.wrapping_add(out.len() as u64));
            out.push(Emigrant {
                female: sex == 1,
                born: (now - a as f64 - rng.random_range(0.0..1.0)) as f32,
                given: crate::rngx::name(&mut nr),
                family: crate::rngx::name(&mut nr),
                royal: false,
                alien: false,
                hybrid: false,
            });
        }
        // A small colony is all records: take adults at random (not of the
        // royal line: its share was chosen above).
        if out.len() < n {
            let mut ids: Vec<u32> = self
                .living
                .iter()
                .copied()
                .filter(|&i| {
                    let p = &self.people[i as usize];
                    !keep.contains(&i) && !p.line && (18.0..46.0).contains(&p.age(now))
                })
                .collect();
            for k in (1..ids.len()).rev() {
                ids.swap(k, rng.random_range(0..=k));
            }
            for id in ids {
                take(self, id, &mut out);
            }
        }
        // Partners left behind are free to marry again.
        let living = self.living.clone();
        for i in living {
            if let Some(sp) = self.people[i as usize].spouse
                && self.emigrated.contains_key(&sp)
            {
                self.people[i as usize].spouse = None;
            }
        }
        out
    }

    pub fn cohort_total(&self) -> f64 {
        self.cohorts.iter().map(|c| c[0] + c[1]).sum()
    }

    /// Everyone alive: records plus the anonymous bulk.
    pub fn population(&self) -> f64 {
        self.living.len() as f64 + self.cohort_total()
    }

    /// Living records of `sp` (natives and hybrids are always records).
    pub fn count_species(&self, sp: Species) -> usize {
        self.living.iter().filter(|&&i| self.people[i as usize].species() == sp).count()
    }

    /// Living people of working age (16–65).
    pub fn workers(&self, now: f64) -> f64 {
        let rec = self
            .living
            .iter()
            .filter(|&&i| (16.0..65.0).contains(&self.people[i as usize].age(now)))
            .count() as f64;
        rec + self.cohorts.iter().enumerate().filter(|(a, _)| (16..65).contains(a)).map(|(_, c)| c[0] + c[1]).sum::<f64>()
    }

    /// Population by age band (records + cohorts): [male, female] per
    /// 5-year band, for the age pyramid.
    pub fn pyramid(&self, now: f64) -> Vec<[f64; 2]> {
        let mut out = vec![[0.0; 2]; MAX_AGE / 5 + 1];
        for &i in &self.living {
            let p = &self.people[i as usize];
            let b = ((p.age(now) as usize).min(MAX_AGE)) / 5;
            out[b][p.female as usize] += 1.0;
        }
        for (a, c) in self.cohorts.iter().enumerate() {
            out[a / 5][0] += c[0];
            out[a / 5][1] += c[1];
        }
        out
    }

    // ------------------------------------------------------------ names

    /// "Given Family". The family name comes down the father's line.
    pub fn name(&self, id: u32) -> String {
        format!("{} {}", self.given_name(id), self.family_name(id))
    }

    pub fn given_name(&self, id: u32) -> String {
        if let Some(g) = self.given.get(&id) {
            return g.clone();
        }
        let mut rng = derive(self.seed ^ 0x6e_a3e5, id as u64);
        crate::rngx::name(&mut rng)
    }

    pub fn family_name(&self, id: u32) -> String {
        // Walk up the father's line to the first ancestor without one.
        let mut root = id;
        let mut steps = 0;
        while let Some(f) = self.get(root).and_then(|p| p.father) {
            root = f;
            steps += 1;
            if steps > 10_000 {
                break;
            }
        }
        if let Some(f) = self.family.get(&root) {
            return f.clone();
        }
        let mut rng = derive(self.seed ^ 0xfa_3117, root as u64);
        crate::rngx::name(&mut rng)
    }

    // ------------------------------------------------------------ family

    pub fn children(&self, id: u32) -> Vec<u32> {
        let mut v: Vec<u32> = self
            .people
            .iter()
            .enumerate()
            .filter(|(_, p)| p.mother == Some(id) || p.father == Some(id))
            .map(|(i, _)| i as u32)
            .collect();
        v.sort_by(|a, b| self.people[*a as usize].born.total_cmp(&self.people[*b as usize].born));
        v
    }

    /// Make sure `id`'s parents, partner and children exist as records, so
    /// the family view can show them. Returns false if `id` is unknown.
    pub fn ensure_family(&mut self, id: u32, now: f64) -> bool {
        if self.get(id).is_none() {
            return false;
        }
        self.ensure_parents(id, now);
        // Children are held on the mother.
        let p = &self.people[id as usize];
        let mother = if p.female { Some(id) } else { p.spouse.filter(|s| self.people[*s as usize].female) };
        if let Some(m) = mother {
            self.materialize_children(m, now);
        }
        true
    }

    fn ensure_parents(&mut self, id: u32, now: f64) {
        let p = self.people[id as usize].clone();
        if !p.parents_unseen {
            return;
        }
        self.people[id as usize].parents_unseen = false;
        if p.born < self.founded + 1.0 {
            return;
        }
        let born = p.born as f64;
        // A known couple of the right age, if the records hold one.
        let cands: Vec<u32> = self
            .people
            .iter()
            .enumerate()
            .filter(|(i, m)| {
                *i as u32 != id
                    && m.female
                    && m.alien == p.alien
                    && (ADULT..FERTILE_END).contains(&(born - m.born as f64))
                    && m.died.is_none_or(|d| d as f64 >= born)
            })
            .map(|(i, _)| i as u32)
            .collect();
        let mut rng = self.rng(0x9a_2e47);
        let mother = if !cands.is_empty() && rng.random_bool(0.5) {
            cands[rng.random_range(0..cands.len())]
        } else {
            let age_then = rng.random_range(19.0..38.0);
            self.conjure(born - age_then, true, p.alien, now)
        };
        let father = match self.people[mother as usize].spouse {
            Some(f) => f,
            None => {
                let age_then = rng.random_range(20.0..42.0);
                let f = self.conjure(born - age_then, false, p.alien, now);
                self.people[f as usize].spouse = Some(mother);
                self.people[mother as usize].spouse = Some(f);
                f
            }
        };
        let rec = &mut self.people[id as usize];
        rec.mother = Some(mother);
        rec.father = Some(father);
        // If they were already living records, their line flag carries on.
        if self.people[mother as usize].line || self.people[father as usize].line {
            self.people[id as usize].line = true;
        }
    }

    /// A person born at `born` who must have existed: alive (drawn out of
    /// the cohorts) if they would plausibly still be, else long dead.
    fn conjure(&mut self, born: f64, female: bool, alien: bool, now: f64) -> u32 {
        let mut rng = self.rng(0xc0_271e);
        let age = (now - born).max(0.0);
        let survive = survival(age, &Rates::default());
        let a = (age as usize).min(MAX_AGE);
        let alive = rng.random_bool(survive.clamp(0.0, 1.0)) && self.cohorts.get(a).is_some_and(|c| c[female as usize] >= 1.0);
        let died = if alive {
            self.cohorts[a][female as usize] -= 1.0;
            None
        } else {
            // Died at some age short of today.
            let at = born + rng.random_range(0.4..1.0) * age;
            Some(at.min(now) as f32)
        };
        self.add(Person {
            female,
            born: born as f32,
            died,
            mother: None,
            father: None,
            spouse: None,
            alien,
            origin: Origin::Settler,
            due: None,
            unseen: vec![],
            parents_unseen: born as f32 >= self.founded + 1.0,
            line: false,
            hybrid: false,
            licensed: false,
        })
    }

    fn materialize_children(&mut self, mother: u32, now: f64) {
        let births = std::mem::take(&mut self.people[mother as usize].unseen);
        if births.is_empty() {
            return;
        }
        let father = self.people[mother as usize].spouse;
        let alien = self.people[mother as usize].alien;
        let line = self.people[mother as usize].line || father.is_some_and(|f| self.people[f as usize].line);
        for b in births {
            let female = self.sex_roll();
            let mut rng = self.rng(0xc4_11d);
            let age = (now - b as f64).max(0.0);
            let a0 = (age as usize).min(MAX_AGE);
            let lived = rng.random_bool(survival(age, &Rates::default()).clamp(0.0, 1.0));
            let a = [a0, a0.saturating_sub(1), (a0 + 1).min(MAX_AGE)].into_iter().find(|&a| self.cohorts.get(a).is_some_and(|c| c[female as usize] >= 1.0));
            let (b, died) = match (lived, a) {
                (true, Some(a)) => {
                    self.cohorts[a][female as usize] -= 1.0;
                    // Born within the year that matches the cohort found.
                    ((now - a as f64 - rng.random_range(0.0..1.0)).max(self.founded as f64) as f32, None)
                }
                // Should be alive, but nobody that age is here: there was
                // no such child.
                (true, None) => continue,
                (false, _) => (b, Some((b as f64 + rng.random_range(0.0..1.0) * age.max(0.1)) as f32)),
            };
            self.add(Person {
                female,
                born: b,
                died,
                mother: Some(mother),
                father,
                spouse: None,
                alien,
                origin: Origin::Born,
                due: None,
                unseen: vec![],
                parents_unseen: false,
                line,
                hybrid: false,
                licensed: false,
            });
        }
    }

    /// Draw someone of `age` out of the cohorts, with a plausible family
    /// (partner and children not yet seen). None if that cohort is empty.
    pub fn materialize(&mut self, age: usize, female: bool, now: f64, with_children: bool) -> Option<u32> {
        self.materialize_inner(age, female, now, with_children, true)
    }

    fn materialize_inner(&mut self, age: usize, female: bool, now: f64, with_children: bool, with_partner: bool) -> Option<u32> {
        if self.cohorts.get(age).is_none_or(|c| c[female as usize] < 1.0) {
            return None;
        }
        self.cohorts[age][female as usize] -= 1.0;
        let mut rng = self.rng(0x3a_7e51);
        let born = now - age as f64 - rng.random_range(0.0..1.0);
        let id = self.add(Person {
            female,
            born: born as f32,
            died: None,
            mother: None,
            father: None,
            spouse: None,
            alien: false,
            origin: Origin::Settler,
            due: None,
            unseen: vec![],
            parents_unseen: born as f32 >= self.founded + 1.0,
            line: false,
            hybrid: false,
            licensed: false,
        });
        if with_partner && age >= 20 && (with_children || rng.random_bool(0.8)) {
            // A partner of about the same age.
            // Someone who is really here, of about the same age (nobody
            // nearby: single, not married to an invented dead partner).
            let pa = (age as i64 + rng.random_range(-4..=4)).clamp(18, MAX_AGE as i64);
            let Some(partner) = [0i64, 1, -1, 2, -2, 3, -3, 4, -4]
                .iter()
                .map(|d| (pa + d).clamp(18, MAX_AGE as i64) as usize)
                .find_map(|a| self.materialize_inner(a, !female, now, false, false))
            else {
                return Some(id);
            };
            self.people[partner as usize].spouse = Some(id);
            self.people[id as usize].spouse = Some(partner);
            let mother = if female { id } else { partner };
            let mborn = self.people[mother as usize].born as f64;
            // Only children born here: what came before belongs to the
            // world the settlers left.
            let from = (mborn + 20.0).max(self.founded as f64);
            let to = now.min(mborn + FERTILE_END);
            let window = (to - from).max(0.0);
            let mut n = (window * 0.12 * rng.random_range(0.5..1.6)).round() as usize;
            if with_children && window > 0.0 {
                n = n.max(1);
            }
            let mut kids: Vec<f32> = (0..n).map(|_| (from + rng.random_range(0.0..window.max(1e-3))) as f32).collect();
            kids.sort_by(|a, b| a.total_cmp(b));
            self.people[mother as usize].unseen = kids;
        }
        Some(id)
    }

    // ------------------------------------------------------------ queries

    /// A random living parent (materialised from the cohorts if needed).
    pub fn random_parent(&mut self, now: f64) -> Option<u32> {
        let mut rng = self.rng(0x9a_7e47);
        let mut parents = std::collections::HashSet::new();
        for p in &self.people {
            parents.extend(p.mother);
            parents.extend(p.father);
        }
        let recs: Vec<u32> = self
            .living
            .iter()
            .copied()
            .filter(|&i| parents.contains(&i) || !self.people[i as usize].unseen.is_empty())
            .collect();
        let bulk_parents: f64 = self.cohorts.iter().enumerate().filter(|(a, _)| (25..70).contains(a)).map(|(_, c)| c[0] + c[1]).sum();
        let take_record = !recs.is_empty() && rng.random::<f64>() * (recs.len() as f64 + bulk_parents * 0.7) < recs.len() as f64;
        if take_record {
            return Some(recs[rng.random_range(0..recs.len())]);
        }
        for _ in 0..40 {
            let age = rng.random_range(25..70);
            let female = rng.random_bool(0.5);
            if let Some(id) = self.materialize(age, female, now, true) {
                let p = &self.people[id as usize];
                let mother = if p.female { Some(id) } else { p.spouse };
                if mother.is_some_and(|m| !self.people[m as usize].unseen.is_empty()) {
                    return Some(id);
                }
            }
        }
        recs.first().copied()
    }

    /// The oldest living colonist (materialised if the oldest is in bulk).
    pub fn oldest(&mut self, now: f64) -> Option<u32> {
        let rec = self
            .living
            .iter()
            .copied()
            .max_by(|a, b| self.people[*a as usize].born.total_cmp(&self.people[*b as usize].born).reverse());
        let rec_age = rec.map(|i| self.people[i as usize].age(now)).unwrap_or(-1.0);
        let bulk = self.cohorts.iter().rposition(|c| c[0] + c[1] >= 1.0);
        match bulk {
            Some(a) if a as f64 > rec_age.floor() => {
                let female = self.cohorts[a][1] >= self.cohorts[a][0];
                self.materialize(a, female, now, false).or(rec)
            }
            _ => rec,
        }
    }

    // ------------------------------------------------------------ the step

    /// Kill a living record now.
    pub fn kill(&mut self, id: u32, now: f64) {
        self.mark_dead(id, now);
        self.living.retain(|&i| i != id);
    }

    /// Record a death without touching `living` (callers prune it once).
    fn mark_dead(&mut self, id: u32, now: f64) {
        let p = &mut self.people[id as usize];
        if p.died.is_some() {
            return;
        }
        p.died = Some(now as f32);
        p.due = None;
        let age = p.age(now);
        self.this_year.deaths += 1.0;
        self.this_year.death_age_sum += age;
    }

    fn prune_living(&mut self) {
        let people = &self.people;
        self.living.retain(|&i| people[i as usize].died.is_none());
    }

    /// Remove `n` people at random (a disaster), never `spare`.
    pub fn cull(&mut self, n: f64, now: f64, spare: Option<u32>) {
        let total = self.population();
        if n <= 0.0 || total <= 0.0 {
            return;
        }
        let share = (n / total).clamp(0.0, 1.0);
        for c in self.cohorts.iter_mut() {
            for v in c.iter_mut() {
                let k = *v * share;
                *v -= k;
                self.this_year.deaths += k;
            }
        }
        let mut rng = self.rng(0xc0_11);
        let ids: Vec<u32> = self.living.clone();
        for id in ids {
            if Some(id) != spare && rng.random_bool(share) {
                self.mark_dead(id, now);
            }
        }
        self.prune_living();
    }

    /// One month (dt = 1/12 year). `player` is the captain's record, kept
    /// alive (retirement, not death, ends their turn); `player_home` is
    /// whether the captain is at the colony (their couple only conceives
    /// then).
    pub fn step(&mut self, now: f64, dt: f64, r: &Rates, player: Option<u32>, player_home: bool, events: &mut Vec<Event>) {
        let mut rng = self.rng(0x57_e9);
        let ids: Vec<u32> = self.living.clone();
        let mut dead: Vec<u32> = Vec::new();
        let mut babies: Vec<Person> = Vec::new();
        for id in ids {
            let p = &self.people[id as usize];
            if p.died.is_some() {
                continue;
            }
            let age = p.age(now);
            // Death.
            if Some(id) != player && rng.random_bool((q_year(age, r) * dt).clamp(0.0, 1.0)) {
                dead.push(id);
                continue;
            }
            if !p.female {
                continue;
            }
            // Birth.
            if let Some(due) = p.due
                && due as f64 <= now
            {
                let father = p.spouse;
                let (alien, hybrid) = child_species(p, father.map(|f| &self.people[f as usize]));
                let p_line = p.line;
                self.people[id as usize].due = None;
                let line = p_line || father.is_some_and(|f| self.people[f as usize].line) || Some(id) == player || father == player;
                self.this_year.births += 1.0;
                let infant_dies = rng.random_bool(r.infant.clamp(0.0, 1.0));
                if infant_dies {
                    self.this_year.infant_deaths += 1.0;
                }
                if !self.demographic || line || alien || hybrid {
                    let female = self.sex_roll();
                    let died = infant_dies.then(|| (now + rng.random_range(0.0..0.9)) as f32);
                    if died.is_some() {
                        self.this_year.deaths += 1.0;
                    }
                    babies.push(Person {
                        female,
                        born: now as f32,
                        died,
                        mother: Some(id),
                        father,
                        spouse: None,
                        alien,
                        origin: Origin::Born,
                        due: None,
                        unseen: vec![],
                        parents_unseen: false,
                        line,
                        hybrid,
                        licensed: false,
                    });
                } else if !infant_dies {
                    let s = self.sex_roll() as usize;
                    self.cohorts[0][s] += 1.0;
                    self.people[id as usize].unseen.push(now as f32);
                } else {
                    self.this_year.deaths += 1.0;
                }
                if rng.random_bool(r.maternal.clamp(0.0, 1.0)) && Some(id) != player {
                    self.this_year.maternal_deaths += 1.0;
                    dead.push(id);
                }
                continue;
            }
            // Conception.
            let p = &self.people[id as usize];
            let Some(sp) = p.spouse else { continue };
            if p.due.is_some() || !(ADULT..FERTILE_END).contains(&age) {
                continue;
            }
            let partner = &self.people[sp as usize];
            if partner.died.is_some() {
                continue;
            }
            if !fertile_pair(p, partner) {
                continue;
            }
            let theirs = Some(id) == player || Some(sp) == player;
            if theirs && !player_home {
                continue;
            }
            if rng.random_bool((r.fert * dt).clamp(0.0, 1.0)) {
                self.people[id as usize].due = Some((now + 0.75) as f32);
                if theirs {
                    events.push(Event::Pregnant(id, sp));
                }
            }
        }
        for &id in &dead {
            let was_player_kin = self.people[id as usize].line;
            self.mark_dead(id, now);
            if was_player_kin {
                events.push(Event::Died(id));
            }
        }
        if !dead.is_empty() {
            self.prune_living();
        }
        for b in babies {
            let line = b.line;
            let alive = b.alive();
            let died = b.died;
            let id = self.add(b);
            if let Some(d) = died {
                self.this_year.death_age_sum += (d as f64 - now).max(0.0);
            }
            if line && alive {
                events.push(Event::Born(id));
            }
        }
        if self.demographic {
            self.step_cohorts(dt, r);
        } else if self.living.iter().filter(|&&i| self.people[i as usize].species() == Species::Human).count() > DETAIL_LIMIT {
            self.demographic = true;
        }
    }

    fn step_cohorts(&mut self, dt: f64, r: &Rates) {
        let (mut men, mut women_fertile, mut women_adult) = (0.0, 0.0, 0.0);
        for (a, c) in self.cohorts.iter().enumerate() {
            let af = a as f64;
            if af >= ADULT {
                men += c[0];
                women_adult += c[1];
                if af < FERTILE_END {
                    women_fertile += c[1];
                }
            }
        }
        let paired = if women_adult > 0.0 { (men / women_adult).min(1.0) * 0.9 } else { 0.0 };
        let births = women_fertile * paired * r.fert * dt;
        let infant = births * r.infant;
        let maternal = births * r.maternal;
        for (a, c) in self.cohorts.iter_mut().enumerate() {
            let q = (q_year(a as f64, r) * dt).clamp(0.0, 1.0);
            for v in c.iter_mut() {
                let d = *v * q;
                *v -= d;
                self.this_year.deaths += d;
                self.this_year.death_age_sum += d * a as f64;
            }
        }
        if women_fertile > 0.0 {
            // Mothers who die in childbirth, spread over the fertile ages.
            for a in (ADULT as usize)..(FERTILE_END as usize) {
                let c = &mut self.cohorts[a];
                let k = maternal * c[1] / women_fertile;
                c[1] -= k;
            }
        }
        let born = (births - infant).max(0.0);
        self.cohorts[0][0] += born / 2.0;
        self.cohorts[0][1] += born / 2.0;
        self.this_year.births += births;
        self.this_year.infant_deaths += infant;
        self.this_year.maternal_deaths += maternal;
        self.this_year.deaths += infant + maternal;
        self.this_year.death_age_sum += maternal * 30.0;
    }

    /// Once a year: everyone in the bulk gets a year older, singles pair
    /// up, and Genetic Modification Kits (`kits`, used up) license human
    /// and native couples.
    pub fn yearly(&mut self, now: f64, kits: &mut u32) {
        if self.demographic {
            let last = self.cohorts[MAX_AGE];
            self.this_year.deaths += last[0] + last[1];
            for a in (1..=MAX_AGE).rev() {
                self.cohorts[a] = self.cohorts[a - 1];
            }
            self.cohorts[0] = [0.0; 2];
        }
        self.use_gene_kits(now, kits);
        self.pair_up(now);
        self.last_year = std::mem::take(&mut self.this_year);
    }

    fn parent_child(&self, a: u32, b: u32) -> bool {
        let (pa, pb) = (&self.people[a as usize], &self.people[b as usize]);
        pa.mother == Some(b) || pa.father == Some(b) || pb.mother == Some(a) || pb.father == Some(a)
    }

    fn related(&self, a: u32, b: u32) -> bool {
        let (pa, pb) = (&self.people[a as usize], &self.people[b as usize]);
        // Parent and child, or siblings.
        pa.mother == Some(b) || pa.father == Some(b) || pb.mother == Some(a) || pb.father == Some(a)
            || (pa.mother.is_some() && pa.mother == pb.mother)
            || (pa.father.is_some() && pa.father == pb.father)
    }

    /// Each kit lets one human and one native have children together:
    /// first couples already married across the line (the captain's own
    /// first), then single natives, each married to a single human.
    pub fn use_gene_kits(&mut self, now: f64, kits: &mut u32) {
        if *kits == 0 {
            return;
        }
        let mut couples: Vec<(u32, u32)> = self
            .living
            .iter()
            .filter_map(|&i| {
                let p = &self.people[i as usize];
                let sp = p.spouse?;
                let q = &self.people[sp as usize];
                (p.female && q.alive() && mixed(p, q) && !(p.licensed && q.licensed)).then_some((i, sp))
            })
            .collect();
        couples.sort_by_key(|&(a, b)| !(self.people[a as usize].line || self.people[b as usize].line));
        for (a, b) in couples {
            if *kits == 0 {
                return;
            }
            *kits -= 1;
            self.people[a as usize].licensed = true;
            self.people[b as usize].licensed = true;
        }
        let single = |p: &Person, people: &[Person]| {
            let age = p.age(now);
            (ADULT..60.0).contains(&age) && p.spouse.is_none_or(|s| people[s as usize].died.is_some())
        };
        let natives: Vec<u32> = self.living.iter().copied().filter(|&i| {
            let p = &self.people[i as usize];
            p.species() == Species::Native && single(p, &self.people)
        }).collect();
        for n in natives {
            if *kits == 0 {
                return;
            }
            let (female, born) = (self.people[n as usize].female, self.people[n as usize].born);
            // A single human of about the same age: a record, else someone
            // drawn from the anonymous bulk.
            let found = self
                .living
                .iter()
                .copied()
                .filter(|&h| {
                    let q = &self.people[h as usize];
                    q.species() == Species::Human && q.female != female && single(q, &self.people) && (q.born - born).abs() <= 15.0
                })
                .min_by(|&a, &b| (self.people[a as usize].born - born).abs().total_cmp(&(self.people[b as usize].born - born).abs()));
            let age = (now - born as f64).max(0.0) as i64;
            let human = found.or_else(|| {
                [0i64, 1, -1, 2, -2, 3, -3, 4, -4, 5, -5]
                    .iter()
                    .map(|d| (age + d).clamp(ADULT as i64, 59) as usize)
                    .find_map(|a| self.materialize_inner(a, !female, now, false, false))
            });
            let Some(h) = human else { continue };
            *kits -= 1;
            for (x, y) in [(n, h), (h, n)] {
                self.people[x as usize].spouse = Some(y);
                self.people[x as usize].licensed = true;
            }
        }
    }

    fn pair_up(&mut self, now: f64) {
        let single = |p: &Person, id: u32, people: &Vec<Person>| {
            let _ = id;
            p.age(now) >= ADULT && p.age(now) < 60.0 && p.spouse.is_none_or(|s| people[s as usize].died.is_some())
        };
        let mut men: Vec<u32> = Vec::new();
        let mut women: Vec<u32> = Vec::new();
        for &i in &self.living {
            let p = &self.people[i as usize];
            if single(p, i, &self.people) {
                if p.female { women.push(i) } else { men.push(i) }
            }
        }
        let mut rng = self.rng(0x9a_12);
        // Shuffle lightly, then match by age.
        for v in [&mut men, &mut women] {
            for i in (1..v.len()).rev() {
                let j = rng.random_range(0..=i);
                v.swap(i, j);
            }
        }
        let born = |id: &u32, people: &Vec<Person>| people[*id as usize].born;
        men.sort_by(|a, b| born(a, &self.people).total_cmp(&born(b, &self.people)));
        women.sort_by(|a, b| born(a, &self.people).total_cmp(&born(b, &self.people)));
        let mut taken = vec![false; women.len()];
        for m in men {
            let mp = &self.people[m as usize];
            let mut best: Option<(usize, bool, f32)> = None;
            for (wi, &w) in women.iter().enumerate() {
                if taken[wi] {
                    continue;
                }
                let wp = &self.people[w as usize];
                if wp.species() != mp.species() {
                    continue;
                }
                let gap = (wp.born - mp.born).abs();
                if gap > 15.0 {
                    continue;
                }
                if self.parent_child(m, w) {
                    continue;
                }
                let rel = self.related(m, w);
                // Prefer unrelated partners; relatives only if nobody else.
                let better = match best {
                    None => true,
                    Some((_, brel, bgap)) => (brel && !rel) || (brel == rel && gap < bgap),
                };
                if better {
                    best = Some((wi, rel, gap));
                }
            }
            if let Some((wi, _, _)) = best
                && rng.random_bool((0.7 + self.pair_bonus).clamp(0.0, 1.0))
            {
                taken[wi] = true;
                let w = women[wi];
                self.people[m as usize].spouse = Some(w);
                self.people[w as usize].spouse = Some(m);
                self.people[m as usize].licensed = false;
                self.people[w as usize].licensed = false;
            }
        }
    }
}

/// Chance of living from birth to `age` under rates `r` (ignoring infancy).
pub fn survival(age: f64, r: &Rates) -> f64 {
    let mut s = 1.0;
    let mut a = 0.0;
    while a < age {
        s *= 1.0 - q_year(a, r);
        a += 1.0;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn founders(n: usize, seed: u64) -> Census {
        let mut c = Census::new(seed, 0.0);
        let mut rng = derive(seed, 1);
        for i in 0..n {
            let age = rng.random_range(20.0..35.0);
            c.add(Person {
                female: i % 2 == 1,
                born: -age as f32,
                died: None,
                mother: None,
                father: None,
                spouse: None,
                alien: false,
                origin: Origin::Founder,
                due: None,
                unseen: vec![],
                parents_unseen: false,
                line: false,
                hybrid: false,
                licensed: false,
            });
        }
        c.pair_up(0.0);
        c
    }

    /// Run `years` more years; the census's own history says where it is.
    fn run(c: &mut Census, years: usize, r: &Rates) {
        let mut ev = Vec::new();
        let start = c.draws_year;
        for y in start..start + years {
            for m in 0..12 {
                let now = y as f64 + m as f64 / 12.0;
                c.step(now, 1.0 / 12.0, r, None, true, &mut ev);
            }
            c.yearly(y as f64 + 1.0, &mut 0);
            c.draws_year += 1;
        }
    }

    #[test]
    fn a_healthy_colony_grows_and_dies_of_old_age() {
        let mut c = founders(500, 7);
        let r = Rates::default();
        run(&mut c, 60, &r);
        let pop = c.population();
        assert!(pop > 1_500.0, "500 founders, 60 years: {pop}");
        // Mean age at death well into old age for the founders who died.
        let founders_dead: Vec<f64> = c.people[..500].iter().filter_map(|p| p.died.map(|d| d as f64 - p.born as f64)).collect();
        assert!(!founders_dead.is_empty());
    }

    /// Each Genetic Modification Kit marries one native to one human; their
    /// children are Hybrids, who marry only Hybrids. Without kits natives
    /// and humans never have children together.
    #[test]
    fn gene_kits_make_hybrid_couples_one_per_kit() {
        let mut c = Census::new(5, 0.0);
        for i in 0..40 {
            c.add(Person {
                female: i % 2 == 1,
                born: -(20.0 + (i % 10) as f32),
                died: None,
                mother: None,
                father: None,
                spouse: None,
                alien: i >= 20,
                origin: if i >= 20 { Origin::Captive } else { Origin::Founder },
                due: None,
                unseen: vec![],
                parents_unseen: false,
                line: false,
                hybrid: false,
                licensed: false,
            });
        }
        let r = Rates { fert: 0.5, ..Rates::default() };
        let mut ev = Vec::new();
        // No kits: two peoples, nobody mixed.
        c.yearly(0.0, &mut 0);
        for &i in &c.living {
            let p = &c.people[i as usize];
            if let Some(s) = p.spouse {
                assert_eq!(p.alien, c.people[s as usize].alien, "no mixed marriage without a kit");
            }
        }
        // Widow everyone, then three kits: three mixed couples.
        for i in 0..40 {
            c.people[i].spouse = None;
        }
        let mut kits = 3;
        c.yearly(0.0, &mut kits);
        assert_eq!(kits, 0, "kits used up");
        let mixed_couples = c.living.iter().filter(|&&i| {
            let p = &c.people[i as usize];
            p.female && p.spouse.is_some_and(|s| mixed(p, &c.people[s as usize]))
        });
        assert_eq!(mixed_couples.count(), 3, "one couple per kit");
        // Their children are Hybrids; nobody else's.
        for y in 0..6 {
            for m in 0..12 {
                c.step(y as f64 + m as f64 / 12.0, 1.0 / 12.0, &r, None, true, &mut ev);
            }
            c.yearly(y as f64 + 1.0, &mut 0);
        }
        let born: Vec<&Person> = c.people.iter().filter(|p| p.origin == Origin::Born).collect();
        let hybrids: Vec<&Person> = born.iter().copied().filter(|p| p.hybrid).collect();
        assert!(!hybrids.is_empty(), "the licensed couples have children");
        for h in &hybrids {
            let (m, f) = (&c.people[h.mother.unwrap() as usize], &c.people[h.father.unwrap() as usize]);
            assert!(mixed(m, f) && m.licensed && f.licensed);
            assert!(!h.alien);
        }
        assert!(born.iter().any(|p| !p.hybrid && p.alien) && born.iter().any(|p| !p.hybrid && !p.alien));
        // Hybrids marry only Hybrids.
        let h = Person { hybrid: true, ..c.people[0].clone() };
        assert!(!fertile_pair(&h, &c.people[1]) && !fertile_pair(&h, &c.people[21]));
        assert!(fertile_pair(&h, &Person { female: true, ..h.clone() }));
        assert_eq!(c.count_species(Species::Hybrid), hybrids.iter().filter(|p| p.alive()).count());
    }

    /// A colony of adult settlers half a year old: looking at people never
    /// invents relatives who died before it existed (children born before
    /// it, or a partner nobody of that age could be).
    #[test]
    fn a_new_colony_shows_no_invented_dead_relatives() {
        let now = 100.5;
        let mut c = Census::new(9, 100.0);
        c.demographic = true;
        for a in 16..49 {
            c.cohorts[a] = [2.0, 2.0];
        }
        for k in 0..60 {
            let id = if k % 2 == 0 { c.random_parent(now) } else { c.materialize(20 + k % 28, k % 3 == 0, now, k % 4 == 0) };
            let Some(id) = id else { continue };
            c.ensure_family(id, now);
            if let Some(sp) = c.people[id as usize].spouse {
                c.ensure_family(sp, now);
            }
        }
        let mut dead_kids = 0;
        for p in &c.people {
            if p.origin == Origin::Born {
                assert!(p.born >= c.founded, "a child born before the colony: {p:?}");
                dead_kids += p.died.is_some() as usize;
            } else {
                assert!(p.died.is_none(), "invented dead partner: {p:?}");
            }
        }
        // Children of a half-year-old colony are babies, who almost all
        // survive their first months.
        assert_eq!(dead_kids, 0);
    }

    #[test]
    fn bad_conditions_shrink_a_colony() {
        let mut c = founders(200, 3);
        let r = Rates { mort_mult: 6.0, hazard: 0.05, fert: 0.03, infant: 0.3, maternal: 0.03 };
        run(&mut c, 40, &r);
        assert!(c.population() < 200.0);
    }

    #[test]
    fn past_the_limit_newborns_join_the_cohorts_and_totals_hold() {
        let mut c = founders(2_000, 11);
        let r = Rates { fert: 0.35, ..Rates::default() };
        let mut years = 0;
        while !c.demographic && years < 200 {
            run(&mut c, 1, &r);
            years += 1;
        }
        assert!(c.demographic, "switched to demographic mode");
        let records = c.living.len();
        run(&mut c, 30, &r);
        assert!(c.cohort_total() > 0.0);
        assert!(c.living.len() <= records + 200, "records stop growing (apart from the captain's line)");
        let pyr: f64 = c.pyramid(c.draws_year as f64).iter().map(|b| b[0] + b[1]).sum();
        assert!((pyr - c.population()).abs() < 1e-6);
    }

    #[test]
    fn materialised_people_have_families_and_stay_put() {
        let mut c = founders(2_000, 5);
        let r = Rates { fert: 0.35, ..Rates::default() };
        while !c.demographic {
            run(&mut c, 1, &r);
        }
        run(&mut c, 40, &r);
        let now = c.draws_year as f64;
        let before = c.population();
        let id = c.random_parent(now).expect("a parent");
        // Drawing someone out of the bulk does not change the head count.
        assert!((c.population() - before).abs() < 1e-6);
        c.ensure_family(id, now);
        let kids = c.children(id);
        assert!(!kids.is_empty(), "a parent has children");
        let name = c.name(id);
        assert_eq!(name, c.name(id), "names are stable");
        // Looking again changes nothing.
        let n = c.people.len();
        c.ensure_family(id, now);
        assert_eq!(c.people.len(), n);
        assert!(c.oldest(now).is_some());
    }

    /// A million colonists: a year must be quick and the save small.
    #[test]
    fn a_million_is_cheap_to_simulate_and_save() {
        let mut c = founders(2_000, 13);
        let r = Rates { fert: 0.35, ..Rates::default() };
        while !c.demographic {
            run(&mut c, 1, &r);
        }
        let bulk = c.cohort_total();
        let k = 1_000_000.0 / bulk.max(1.0);
        for a in c.cohorts.iter_mut() {
            a[0] *= k;
            a[1] *= k;
        }
        let t0 = std::time::Instant::now();
        run(&mut c, 1, &r);
        let per_year = t0.elapsed();
        assert!(per_year.as_millis() < 1_500, "a year at 1M took {per_year:?}");
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.len() < 5_000_000, "census save is {} bytes", json.len());
        assert!(c.population() > 900_000.0);
    }

    #[test]
    fn two_founders_can_found_a_family() {
        let mut c = founders(2, 9);
        let r = Rates { fert: 0.3, ..Rates::default() };
        run(&mut c, 30, &r);
        assert!(c.people.len() > 2, "children were born");
        // Children take their father's family name.
        let kid = c.people.iter().position(|p| p.father.is_some()).unwrap() as u32;
        let dad = c.people[kid as usize].father.unwrap();
        assert_eq!(c.family_name(kid), c.family_name(dad));
    }

    #[test]
    fn babies_sexes_are_a_fair_coin_without_streaks() {
        for seed in [1u64, 42, 10_171_699_353_412_291_677] {
            let mut c = Census::new(seed, 0.0);
            let n = 100_000;
            let rolls: Vec<bool> = (0..n).map(|_| c.sex_roll()).collect();
            let f = rolls.iter().filter(|&&b| b).count() as f64 / n as f64;
            let same = rolls.windows(2).filter(|w| w[0] == w[1]).count() as f64 / (n - 1) as f64;
            // Runs of one sex: about as long as a fair coin makes them.
            let (mut longest, mut run) = (1, 1);
            for w in rolls.windows(2) {
                run = if w[0] == w[1] { run + 1 } else { 1 };
                longest = longest.max(run);
            }
            assert!((0.49..0.51).contains(&f), "girls {f}");
            assert!((0.49..0.51).contains(&same), "same as last {same}");
            assert!(longest < 25, "a run of {longest}");
        }
    }
}
