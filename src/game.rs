//! Toolkit-agnostic game shell: screen state, player actions, real-time tick.
//!
//! Nothing here knows about windows, widgets or terminals. Front ends (the
//! egui GUI today) turn clicks and key presses into [`Action`]s and call
//! [`App::apply`]; keyboard bindings live in [`App::handle_key`] so every
//! front end shares the same shortcuts. That split is what keeps a future
//! port (another toolkit, another OS) to the drawing code alone.

use crate::sim::civ::CivTier;
use crate::sim::galaxy::Neighbor;
use crate::sim::save;
use crate::sim::state::{loc_label, Focus, ShuttleBlock};
use crate::sim::{fmt_hours, Galaxy, Good, Level, Loc, Res, Sex, Sim};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Title,
    Cockpit,
    Map,
    Planet,
    Star,
    Resources,
    Crew,
    Systems,
    Trade,
    Colony,
    Help,
    GameOver,
    /// The colony reached a million.
    Victory,
}

impl Screen {
    /// Screens that show the running game (and so need a sim).
    pub fn in_game(self) -> bool {
        !matches!(self, Screen::Title | Screen::GameOver | Screen::Help | Screen::Victory)
    }
}

/// Tabs of the Trade screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeTab {
    Resources,
    Cargo,
    Port,
}

/// Tabs of the colony panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColonyTab {
    Overview,
    Supplies,
    Build,
    Census,
    Family,
    Market,
}

/// Time-compression steps, in the order `F` cycles through them (0 = pause).
pub const TIME_SCALES: [u32; 5] = [1, 10, 60, 600, 0];

/// Toolkit-neutral key, as front ends report it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Up,
    Down,
    Left,
    Right,
    Enter,
    Esc,
    Tab,
    /// Ctrl+C / window-manager quit shortcut.
    CtrlC,
}

/// Everything the player can do. Buttons and keys both produce these.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    // ---- title ----
    SelectShip(usize),
    SetSeed(u64),
    SetOpts(crate::sim::GalaxyOpts),
    SetSex(Sex),
    NewGame,
    Continue,
    // ---- navigation ----
    Open(Screen),
    Back,
    /// Save (if a live game) and exit.
    Quit,
    /// Leave the game-over screen for the title.
    AckGameOver,
    // ---- ship systems ----
    ToggleScoop,
    ToggleMining,
    CycleFocus,
    SetFocus(Focus),
    CycleReactor,
    SetReactor(u8),
    CycleThrottle,
    SetThrottle(u8),
    ToggleShields,
    CycleLsEff,
    SetLsEff(u8),
    ToggleLights,
    ToggleRepair,
    ToggleDroneAuto,
    CycleMineTarget,
    /// (body, deposit)
    SetMineTarget(Loc, usize),
    /// Drones strip wreckage instead of mining.
    ToggleSalvage,
    // ---- in-system ----
    SelectPlanet(usize),
    /// Select any body (planet or moon).
    SelectBody(Loc),
    /// Step the planet selection by ±1 (wraps).
    StepPlanet(i32),
    /// Star ↔ selected planet (the cockpit `S` key).
    ShuttleToggle,
    ShuttleTo(Loc),
    // ---- FTL ----
    MapSelect(usize),
    MapStep(i32),
    /// Set the selected map system as destination, or clear it if it
    /// already is (the map `Enter` key).
    ToggleDestination(u64),
    ClearDestination,
    Jump,
    /// Make `id` the destination and jump now (the map `T` key).
    JumpTo(u64),
    SkipToArrival,
    // ---- time ----
    CycleTimeScale,
    SetTimeScale(u32),
    // ---- trade ----
    TradeSelect(usize),
    TradeStep(i32),
    TradeQtyScale(f64),
    SetTradeQty(f64),
    Buy(Res),
    Sell(Res),
    RepairHull,
    SetTradeTab(TradeTab),
    SetCargoQty(u32),
    BuyGood(Good),
    SellGood(Good),
    RecruitPartner,
    HirePilot,
    /// Buy an Attack (true) or Defense (false) level at a T3 shipyard.
    BuyUpgrade(bool),
    // ---- colony ----
    FoundColony,
    LeaveColony,
    EnterColony,
    AbandonColony,
    UnloadCargo,
    Build(crate::sim::colony::Project),
    CancelBuild(usize),
    ToggleAutoBuild,
    DockBuy(Good),
    DockSellMaterials(f64),
    SetColonyTab(ColonyTab),
    /// Show a colonist in the family view (materialises their family).
    ViewPerson(u32),
    RandomParent,
    OldestColonist,
    /// Victory: end the game in triumph.
    EndInVictory,
    /// Victory: give the colony up and keep flying.
    VictoryAbandon,
    // ---- pirates ----
    Encounter(crate::sim::combat::Choice),
    // ---- misc ----
    Save,
}

/// One line of the status history. `level` is `None` for UI flashes (no
/// severity) and `Some(...)` for events drained from the sim log.
#[derive(Debug, Clone)]
pub struct StatusEntry {
    pub t: Option<f64>, // game seconds, if the sim existed
    pub level: Option<Level>,
    pub msg: String,
}

pub struct App {
    pub running: bool,
    pub screen: Screen,
    /// Where `Back` goes from Help (which is reachable from the title too).
    pub help_return: Screen,
    pub sim: Option<Sim>,
    pub gal: Galaxy,

    pub title_sel: usize,
    pub sel_planet: usize,
    /// A moon of the selected planet, if one is selected.
    pub sel_moon: Option<usize>,
    pub map_list: Vec<Neighbor>,
    pub map_sel: usize,
    pub trade_row: usize,
    pub trade_qty: f64,
    pub trade_tab: TradeTab,
    /// Units per cargo buy/sell click.
    pub cargo_qty: u32,
    /// The captain's sex for the next new game (title screen).
    pub title_sex: Sex,
    pub colony_tab: ColonyTab,
    /// The colonist shown in the family view.
    pub person: Option<u32>,
    /// Real-clock time of the last colony-mode autosave.
    last_colony_save: f64,
    /// Founding asks twice.
    pub confirm_found: bool,
    /// (msg, expiry on the real clock)
    pub flash: Option<(String, f64)>,
    /// Every status message shown — UI flashes and game events, newest last.
    pub flash_history: Vec<StatusEntry>,
    /// How many sim log entries have been drained into `flash_history`.
    pub log_seen: u64,
    /// Seconds of real time the app has ticked (drives flash expiry).
    pub real_clock: f64,
}

pub const STATUS_CAP: usize = 400;
const FLASH_SECS: f64 = 4.0;

impl App {
    pub fn new(seed: u64) -> App {
        App {
            running: true,
            screen: Screen::Title,
            help_return: Screen::Title,
            sim: None,
            gal: Galaxy::new(seed),
            title_sel: 0,
            sel_planet: 0,
            sel_moon: None,
            map_list: vec![],
            map_sel: 0,
            trade_row: 0,
            trade_qty: 100.0,
            trade_tab: TradeTab::Resources,
            cargo_qty: 1,
            title_sex: Sex::Male,
            colony_tab: ColonyTab::Overview,
            person: None,
            last_colony_save: 0.0,
            confirm_found: false,
            flash: None,
            flash_history: Vec::new(),
            log_seen: 0,
            real_clock: 0.0,
        }
    }

    /// A fresh app already in the cockpit (tests, previews).
    pub fn with_game(seed: u64, ship: usize) -> App {
        let mut app = App::new(seed);
        app.start_new_game(ship);
        app
    }

    pub fn sim(&self) -> &Sim {
        self.sim.as_ref().expect("no sim")
    }

    /// The live flash message, if it has not expired.
    pub fn live_flash(&self) -> Option<&str> {
        self.flash
            .as_ref()
            .filter(|(_, exp)| *exp > self.real_clock)
            .map(|(m, _)| m.as_str())
    }

    pub fn flash(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        self.push_status(StatusEntry { t: self.sim.as_ref().map(|s| s.t), level: None, msg: msg.clone() });
        self.flash = Some((msg, self.real_clock + FLASH_SECS));
    }

    fn push_status(&mut self, e: StatusEntry) {
        self.flash_history.push(e);
        if self.flash_history.len() > STATUS_CAP {
            let over = self.flash_history.len() - STATUS_CAP;
            self.flash_history.drain(..over);
        }
    }

    /// Pull sim log entries the status history has not seen yet.
    pub fn drain_log(&mut self) {
        let Some(sim) = self.sim.as_ref() else { return };
        let total = sim.log.total;
        let new = total.saturating_sub(self.log_seen) as usize;
        if new == 0 {
            return;
        }
        let start = sim.log.entries.len().saturating_sub(new);
        let v: Vec<StatusEntry> = sim
            .log
            .entries
            .iter()
            .skip(start)
            .map(|e| StatusEntry { t: Some(e.t), level: Some(e.level), msg: e.msg.clone() })
            .collect();
        for e in v {
            self.push_status(e);
        }
        self.log_seen = total;
    }

    pub fn start_new_game(&mut self, ship_idx: usize) {
        let seed = self.gal.seed;
        self.gal = Galaxy::with_opts(seed, self.gal.opts);
        let mut sim = Sim::new_game(seed, ship_idx, &mut self.gal);
        sim.player_sex = self.title_sex;
        self.sim = Some(sim);
        self.screen = Screen::Cockpit;
        self.sel_planet = 0;
        self.flash_history.clear();
        self.log_seen = 0;
        self.drain_log();
        self.flash("New game started. Good luck, pilot.");
    }

    pub fn continue_game(&mut self) {
        match save::load() {
            Ok((sim, gal)) => {
                self.flash_history.clear();
                // No backfill: only events after the load drain.
                self.log_seen = sim.log.total;
                let over = sim.over.is_some();
                self.sim = Some(sim);
                self.gal = gal;
                self.sel_planet = 0;
                self.screen = if over { Screen::GameOver } else { Screen::Cockpit };
                self.flash("Save loaded.");
            }
            Err(e) => self.flash(e),
        }
    }

    fn do_save(&mut self) -> bool {
        let Some(sim) = self.sim.as_ref() else { return false };
        match save::save(sim, &self.gal) {
            Ok(p) => {
                self.flash(format!("Saved to {}", p.display()));
                true
            }
            Err(e) => {
                self.flash(format!("Save failed: {e}"));
                false
            }
        }
    }

    /// Save (if there is a live game) and stop running. Front ends call this
    /// when the window is closed, too.
    pub fn quit(&mut self) {
        if self.sim.as_ref().is_some_and(|s| s.over.is_none()) {
            self.do_save();
        }
        self.running = false;
    }

    /// A refused shuttle leg, in words the player can act on.
    fn shuttle_block_msg(&self, blocked: ShuttleBlock) -> String {
        let sim = self.sim();
        let sys = self.gal.system_ref(sim.sys_id);
        let msg = match blocked {
            ShuttleBlock::Tripping => {
                let trip = sim.trip.as_ref().unwrap();
                format!(
                    "In FTL to {} — {} to arrival. No shuttling between stars.",
                    trip.dest_name,
                    fmt_hours(trip.remaining_h)
                )
            }
            ShuttleBlock::Shuttling => {
                let leg = sim.shuttle.as_ref().unwrap();
                format!(
                    "Already shuttling to {} — {} left. It can't change course mid-leg.",
                    loc_label(leg.to, sys),
                    fmt_hours(leg.remaining_h)
                )
            }
            ShuttleBlock::AlreadyThere => "Already docked here.".to_string(),
            ShuttleBlock::Nowhere => "There is no such place in this system.".to_string(),
        };
        if sim.ff == 0 { format!("{msg} (paused — resume time to fly)") } else { msg }
    }

    fn shuttle(&mut self, to: Loc) {
        let blocked = {
            let Some(sim) = self.sim.as_mut() else { return };
            sim.begin_shuttle(&mut self.gal, to).err()
        };
        if let Some(b) = blocked {
            let m = self.shuttle_block_msg(b);
            self.flash(m);
        }
    }

    pub fn do_skip_to_arrival(&mut self) {
        let Some(sim) = self.sim.as_mut() else { return };
        if sim.trip.is_none() || sim.over.is_some() {
            return;
        }
        let mut chunks = 0;
        while sim.trip.is_some() && sim.over.is_none() && sim.encounter.is_none() && chunks < 200_000 {
            sim.advance(0.5, &mut self.gal);
            chunks += 1;
            if sim.autosave_h >= 1.0 {
                sim.autosave_h = 0.0;
                let _ = save::save(sim, &self.gal);
            }
        }
        if sim.over.is_some() {
            save::delete();
            self.screen = Screen::GameOver;
        }
        self.drain_log();
    }

    pub fn refresh_map(&mut self) {
        let Some(sim) = self.sim.as_ref() else { return };
        let pos = self.gal.system_ref(sim.sys_id).pos;
        let sel_id = self.map_list.get(self.map_sel).map(|n| n.id);
        self.map_list = self.gal.neighbors(pos, 30.0, sim.sys_id);
        // Keep the same system selected across refreshes when possible;
        // otherwise start on the destination, or the nearest.
        let want = sel_id.or(sim.destination);
        self.map_sel = want
            .and_then(|id| self.map_list.iter().position(|n| n.id == id))
            .unwrap_or(0);
    }

    fn local_civ(&self) -> Option<CivTier> {
        let sim = self.sim.as_ref()?;
        self.gal.system_ref(sim.sys_id).civ.as_ref().map(|c| c.tier)
    }

    /// Why trade is unavailable here, or `None` if it is. Pre-space
    /// peoples still barter goods (the Cargo tab).
    pub fn trade_block_reason(&self) -> Option<&'static str> {
        match self.local_civ() {
            Some(_) => None,
            None => Some("No one here to trade with. Just stars and rock."),
        }
    }

    /// Resources (tanks) trade needs a planetary or interstellar market.
    pub fn resource_trade_ok(&self) -> bool {
        matches!(self.local_civ(), Some(CivTier::T2) | Some(CivTier::T3))
    }

    /// The selected body as a location (planet, or one of its moons).
    pub fn selected_body(&self) -> Loc {
        match self.sel_moon {
            Some(m) => Loc::Moon(self.sel_planet, m),
            None => Loc::Planet(self.sel_planet),
        }
    }

    fn planet_count(&self) -> usize {
        self.sim
            .as_ref()
            .map(|s| self.gal.system_ref(s.sys_id).planets.len())
            .unwrap_or(0)
    }

    // ------------------------------------------------------------------
    // Actions
    // ------------------------------------------------------------------

    pub fn apply(&mut self, a: Action) {
        // In-game actions need a live sim; ignore them otherwise (they can
        // only arrive from a stale key press during a screen change).
        let needs_sim = !matches!(
            a,
            Action::SelectShip(_)
                | Action::SetSeed(_)
                | Action::SetOpts(_)
                | Action::SetSex(_)
                | Action::NewGame
                | Action::Continue
                | Action::Open(Screen::Help)
                | Action::Open(Screen::Title)
                | Action::Back
                | Action::Quit
                | Action::AckGameOver
        );
        if needs_sim && self.sim.as_ref().is_none_or(|s| s.over.is_some()) {
            return;
        }
        match a {
            Action::SelectShip(i) => self.title_sel = i.min(crate::sim::SHIPS.len() - 1),
            Action::SetSeed(s) => self.gal = Galaxy::with_opts(s, self.gal.opts),
            Action::SetOpts(o) => self.gal = Galaxy::with_opts(self.gal.seed, o),
            Action::SetSex(x) => self.title_sex = x,
            Action::NewGame => self.start_new_game(self.title_sel),
            Action::Continue => self.continue_game(),
            Action::Open(s) => self.open(s),
            Action::Back => self.back(),
            Action::Quit => self.quit(),
            Action::AckGameOver => {
                save::delete();
                let seed = self.gal.seed;
                self.sim = None;
                self.gal = Galaxy::with_opts(seed, self.gal.opts);
                self.screen = Screen::Title;
            }

            Action::ToggleScoop => self.with_sim(|s| s.toggle_scoop()),
            Action::ToggleMining => self.with_sim(|s| s.toggle_mining()),
            Action::CycleFocus => self.with_sim(|s| s.cycle_focus()),
            Action::SetFocus(f) => self.with_sim(|s| {
                while s.focus != f {
                    s.cycle_focus();
                }
            }),
            Action::CycleReactor => self.with_sim(|s| s.cycle_reactor()),
            Action::SetReactor(p) => self.with_sim(|s| {
                for _ in 0..4 {
                    if s.reactor_pct == p {
                        break;
                    }
                    s.cycle_reactor();
                }
            }),
            Action::CycleThrottle => self.with_sim(|s| s.cycle_throttle()),
            Action::SetThrottle(p) => self.with_sim(|s| {
                for _ in 0..3 {
                    if s.ftl_throttle == p {
                        break;
                    }
                    s.cycle_throttle();
                }
            }),
            Action::ToggleShields => self.with_sim(|s| s.toggle_shields()),
            Action::CycleLsEff => self.with_sim(|s| s.cycle_ls_eff()),
            Action::SetLsEff(p) => self.with_sim(|s| {
                for _ in 0..3 {
                    if s.ls_eff == p {
                        break;
                    }
                    s.cycle_ls_eff();
                }
            }),
            Action::ToggleLights => self.with_sim(|s| s.toggle_lights()),
            Action::ToggleRepair => self.with_sim(|s| s.toggle_repair()),
            Action::ToggleDroneAuto => {
                let sim = self.sim.as_mut().unwrap();
                sim.toggle_drone_auto(&self.gal);
            }
            Action::CycleMineTarget => {
                let sim = self.sim.as_mut().unwrap();
                if !matches!(sim.loc, Loc::Planet(_)) {
                    self.flash("Dock at a planet to pick a drone target.");
                } else {
                    sim.cycle_mine_target(&self.gal);
                }
            }
            Action::ToggleSalvage => self.with_sim(|s| s.toggle_salvage()),
            Action::SetMineTarget(p, d) => {
                let sim = self.sim.as_mut().unwrap();
                sim.set_mine_target(&self.gal, p, d);
            }

            Action::SelectPlanet(i) => {
                let n = self.planet_count();
                self.sel_planet = i.min(n.saturating_sub(1));
                self.sel_moon = None;
            }
            Action::SelectBody(l) => match l {
                Loc::Planet(i) => self.apply(Action::SelectPlanet(i)),
                Loc::Moon(i, m) => {
                    self.apply(Action::SelectPlanet(i));
                    self.sel_moon = Some(m);
                }
                _ => {}
            },
            Action::StepPlanet(d) => {
                let n = self.planet_count().max(1) as i64;
                self.sel_planet = (self.sel_planet as i64 + d as i64).rem_euclid(n) as usize;
                self.sel_moon = None;
            }
            Action::ShuttleToggle => {
                let n = self.planet_count();
                self.sel_planet = self.sel_planet.min(n.saturating_sub(1));
                let to = match self.sim().loc {
                    Loc::Star | Loc::Companion => self.selected_body(),
                    l => {
                        self.sel_planet = l.planet().unwrap_or(0);
                        self.sel_moon = match l {
                            Loc::Moon(_, m) => Some(m),
                            _ => None,
                        };
                        Loc::Star
                    }
                };
                self.shuttle(to);
            }
            Action::ShuttleTo(to) => self.shuttle(to),

            Action::MapSelect(i) => {
                if i < self.map_list.len() {
                    self.map_sel = i;
                }
            }
            Action::MapStep(d) => {
                let n = self.map_list.len();
                if n > 0 {
                    self.map_sel = (self.map_sel as i64 + d as i64).clamp(0, n as i64 - 1) as usize;
                }
            }
            Action::ToggleDestination(id) => {
                let sim = self.sim.as_mut().unwrap();
                if sim.trip.is_some() {
                    self.flash("Already in FTL — the course is locked until arrival.");
                } else if sim.destination == Some(id) {
                    sim.clear_destination();
                    self.flash("Destination cleared.");
                } else {
                    sim.set_destination(id, &mut self.gal);
                }
            }
            Action::ClearDestination => {
                let sim = self.sim.as_mut().unwrap();
                if sim.trip.is_none() && sim.destination.is_some() {
                    sim.clear_destination();
                    self.flash("Destination cleared.");
                }
            }
            Action::Jump => self.jump(),
            Action::JumpTo(id) => {
                let sim = self.sim.as_mut().unwrap();
                if sim.trip.is_none() && sim.destination != Some(id) {
                    sim.set_destination(id, &mut self.gal);
                }
                self.jump();
            }
            Action::SkipToArrival => {
                if self.sim().trip.is_some() {
                    self.do_skip_to_arrival();
                } else {
                    self.flash("Not in FTL transit.");
                }
            }

            Action::CycleTimeScale => {
                let cur = self.sim().ff;
                let i = TIME_SCALES.iter().position(|&x| x == cur).unwrap_or(0);
                self.set_time_scale(TIME_SCALES[(i + 1) % TIME_SCALES.len()]);
            }
            Action::SetTimeScale(ff) => self.set_time_scale(ff),

            Action::TradeSelect(i) => self.trade_row = i.min(self.trade_rows() - 1),
            Action::TradeStep(d) => {
                self.trade_row = (self.trade_row as i64 + d as i64).clamp(0, self.trade_rows() as i64 - 1) as usize
            }
            Action::TradeQtyScale(f) => self.trade_qty = (self.trade_qty * f).clamp(10.0, 100_000.0),
            Action::SetTradeQty(q) => self.trade_qty = q.clamp(1.0, 1_000_000.0),
            Action::Buy(r) | Action::Sell(r) => {
                if let Some(why) = self.trade_block_reason() {
                    self.flash(why);
                } else if !self.resource_trade_ok() {
                    self.flash("Pre-space peoples only barter goods (the Cargo tab).");
                } else {
                    let buy = matches!(a, Action::Buy(_));
                    let qty = self.trade_qty;
                    let sim = self.sim.as_mut().unwrap();
                    sim.trade(&mut self.gal, r, buy, qty);
                }
            }
            Action::SetTradeTab(t) => {
                self.trade_tab = t;
                self.trade_row = 0;
            }
            Action::SetCargoQty(n) => self.cargo_qty = n.clamp(1, 10_000),
            Action::BuyGood(g) | Action::SellGood(g) => {
                if let Some(why) = self.trade_block_reason() {
                    self.flash(why);
                } else {
                    let buy = matches!(a, Action::BuyGood(_));
                    let n = self.cargo_qty;
                    let sim = self.sim.as_mut().unwrap();
                    sim.trade_good(&mut self.gal, g, buy, n);
                }
            }
            Action::RecruitPartner => {
                let sim = self.sim.as_mut().unwrap();
                sim.recruit_partner(&mut self.gal);
            }
            Action::HirePilot => {
                let sim = self.sim.as_mut().unwrap();
                sim.hire_pilot(&mut self.gal);
            }
            Action::BuyUpgrade(attack) => {
                let sim = self.sim.as_mut().unwrap();
                sim.buy_upgrade(&self.gal, attack);
            }
            Action::Encounter(c) => self.with_sim(|s| s.resolve_encounter(c)),
            Action::FoundColony => {
                if !self.confirm_found {
                    self.confirm_found = true;
                    self.flash("Founding is for life: press Found colony again to confirm.");
                } else {
                    self.confirm_found = false;
                    let sim = self.sim.as_mut().unwrap();
                    match sim.found_colony(&mut self.gal) {
                        Ok(()) => {
                            self.screen = Screen::Cockpit;
                            self.colony_tab = ColonyTab::Overview;
                            self.person = None;
                        }
                        Err(e) => self.flash(e),
                    }
                }
            }
            Action::LeaveColony => {
                if let Err(e) = self.sim.as_mut().unwrap().leave_colony() {
                    self.flash(e);
                }
            }
            Action::EnterColony => {
                if let Err(e) = self.sim.as_mut().unwrap().enter_colony() {
                    self.flash(e);
                } else {
                    self.screen = Screen::Cockpit;
                    let sim = self.sim.as_mut().unwrap();
                    if sim.ff > 60 {
                        sim.ff = 1;
                    }
                }
            }
            Action::AbandonColony => {
                let sim = self.sim.as_mut().unwrap();
                if let Err(e) = sim.abandon_colony(&mut self.gal) {
                    self.flash(e);
                } else {
                    self.screen = Screen::Cockpit;
                }
            }
            Action::UnloadCargo => {
                let m = self.sim.as_mut().unwrap().unload_to_colony();
                self.flash(m);
            }
            Action::Build(p) => {
                if let Err(e) = self.sim.as_mut().unwrap().build(p) {
                    self.flash(e);
                }
            }
            Action::CancelBuild(i) => self.with_sim(|s| s.cancel_build(i)),
            Action::ToggleAutoBuild => self.with_sim(|s| {
                if let Some(c) = s.colony.as_mut() {
                    c.auto_build = !c.auto_build;
                }
            }),
            Action::DockBuy(g) => {
                let n = self.cargo_qty;
                self.with_sim(|s| s.dock_buy(g, n));
            }
            Action::DockSellMaterials(t) => self.with_sim(|s| s.dock_sell_materials(t)),
            Action::SetColonyTab(t) => {
                self.colony_tab = t;
                if t == ColonyTab::Family && self.person.is_none() {
                    let me = self.sim().colony.as_ref().map(|c| c.player);
                    if let Some(id) = me {
                        self.apply(Action::ViewPerson(id));
                    }
                }
            }
            Action::ViewPerson(id) => {
                let sim = self.sim.as_mut().unwrap();
                let now = crate::sim::people::years(sim.t);
                if let Some(c) = sim.colony.as_mut()
                    && c.census.ensure_family(id, now)
                {
                    self.person = Some(id);
                    self.colony_tab = ColonyTab::Family;
                }
            }
            Action::RandomParent | Action::OldestColonist => {
                let sim = self.sim.as_mut().unwrap();
                let now = crate::sim::people::years(sim.t);
                let found = sim.colony.as_mut().and_then(|c| if a == Action::RandomParent { c.census.random_parent(now) } else { c.census.oldest(now) });
                match found {
                    Some(id) => self.apply(Action::ViewPerson(id)),
                    None => self.flash("Nobody fits."),
                }
            }
            Action::EndInVictory => {
                let sim = self.sim.as_mut().unwrap();
                sim.victory_seen = true;
                let name = sim.colony.as_ref().map(|c| c.name.clone()).unwrap_or_default();
                sim.over = Some(crate::sim::state::GameOver {
                    cause: "Victory".into(),
                    detail: format!("{name} reached a million people. Your line will be remembered."),
                    t: sim.t,
                });
                save::delete();
                self.screen = Screen::GameOver;
            }
            Action::VictoryAbandon => {
                let sim = self.sim.as_mut().unwrap();
                sim.victory_seen = true;
                if let Err(e) = sim.abandon_colony(&mut self.gal) {
                    self.flash(e);
                }
                self.screen = Screen::Cockpit;
            }
            Action::RepairHull => {
                let sim = self.sim.as_mut().unwrap();
                sim.repair_hull_t3(&mut self.gal);
            }
            Action::Save => {
                self.do_save();
            }
        }
        self.drain_log();
    }

    /// Rows in the current Trade tab's list.
    fn trade_rows(&self) -> usize {
        match self.trade_tab {
            TradeTab::Cargo => Good::ALL.len(),
            _ => Res::ALL.len(),
        }
    }

    fn with_sim(&mut self, f: impl FnOnce(&mut Sim)) {
        if let Some(s) = self.sim.as_mut() {
            f(s);
        }
    }

    fn set_time_scale(&mut self, ff: u32) {
        self.sim.as_mut().unwrap().ff = ff;
        self.flash(if ff == 0 {
            "Paused — resume time with F; skip-to-arrival still works.".to_string()
        } else {
            format!("Time compression x{ff}")
        });
    }

    fn jump(&mut self) {
        let ok = {
            let sim = self.sim.as_mut().unwrap();
            sim.start_trip(&mut self.gal)
        };
        if ok {
            if self.screen == Screen::Map {
                self.screen = Screen::Cockpit;
            }
            return;
        }
        let sim = self.sim();
        let (trip, no_dest, charge, shuttling) =
            (sim.trip.is_some(), sim.destination.is_none(), sim.ftl_charge, sim.shuttle.is_some());
        if trip {
            self.flash("Already in FTL transit.");
        } else if no_dest {
            self.refresh_map();
            self.screen = Screen::Map;
            self.flash("Pick a destination on the star map first.");
        } else if charge < 100.0 {
            self.flash(format!("FTL capacitors at {charge:.0}% — not ready to jump."));
        } else if shuttling {
            self.flash("Finish the in-system shuttle first.");
        }
    }

    fn open(&mut self, s: Screen) {
        match s {
            Screen::Help => {
                if self.screen != Screen::Help {
                    self.help_return = self.screen;
                }
                self.screen = Screen::Help;
            }
            Screen::Title => self.screen = Screen::Title,
            Screen::GameOver | Screen::Victory => {}
            Screen::Colony => {
                if self.sim().colony.is_some() {
                    self.screen = Screen::Colony;
                } else {
                    self.flash("You have no colony yet. Dock at a rocky world and found one from the Planets screen.");
                }
            }
            Screen::Map => {
                self.refresh_map();
                self.screen = Screen::Map;
            }
            Screen::Trade => match self.trade_block_reason() {
                None => self.screen = Screen::Trade,
                Some(why) => self.flash(why),
            },
            Screen::Planet => {
                let n = self.planet_count();
                self.sel_planet = self.sel_planet.min(n.saturating_sub(1));
                self.screen = Screen::Planet;
            }
            other => self.screen = other,
        }
    }

    fn back(&mut self) {
        self.screen = match self.screen {
            // Help is reachable from the title, where there is no sim for
            // the cockpit to draw.
            Screen::Help => {
                let r = self.help_return;
                if r.in_game() && self.sim.is_none() { Screen::Title } else { r }
            }
            Screen::Title | Screen::GameOver | Screen::Victory => self.screen,
            _ => {
                if self.sim.is_some() { Screen::Cockpit } else { Screen::Title }
            }
        };
    }

    // ------------------------------------------------------------------
    // Keyboard (shared by every front end)
    // ------------------------------------------------------------------

    pub fn handle_key(&mut self, key: Key) {
        if key == Key::CtrlC {
            self.apply(Action::Quit);
            return;
        }
        let k = match key {
            Key::Char(c) => Key::Char(c.to_ascii_lowercase()),
            other => other,
        };
        // Pirates hailing: only the answer keys work until it is settled.
        if let Some(stage) = self.sim.as_ref().filter(|s| s.over.is_none()).and_then(|s| s.encounter.as_ref()).map(|e| e.stage) {
            use crate::sim::combat::{Choice, Stage};
            let act = match (stage, k) {
                (Stage::Approach, Key::Char('1')) => Some(Action::Encounter(Choice::Fight)),
                (Stage::Approach, Key::Char('2')) => Some(Action::Encounter(Choice::Flee)),
                (Stage::Approach, Key::Char('3')) => Some(Action::Encounter(Choice::Pay)),
                (Stage::Won, Key::Char('1')) => Some(Action::Encounter(Choice::Destroy)),
                (Stage::Won, Key::Char('2')) => Some(Action::Encounter(Choice::Release)),
                (_, Key::Char('q')) => Some(Action::Quit),
                (_, Key::Char('g')) => Some(Action::Save),
                _ => None,
            };
            if let Some(a) = act {
                self.apply(a);
            }
            return;
        }
        let act: Option<Action> = match self.screen {
            Screen::Title => match k {
                Key::Char(c @ '1'..='3') => Some(Action::SelectShip(c as usize - '1' as usize)),
                Key::Left | Key::Up => Some(Action::SelectShip((self.title_sel + 2) % 3)),
                Key::Right | Key::Down => Some(Action::SelectShip((self.title_sel + 1) % 3)),
                Key::Enter => Some(Action::NewGame),
                Key::Char('c') => Some(Action::Continue),
                Key::Char('h') => Some(Action::Open(Screen::Help)),
                Key::Char('q') => Some(Action::Quit),
                _ => None,
            },
            Screen::GameOver => match k {
                Key::Enter | Key::Esc | Key::Char('q') => Some(Action::AckGameOver),
                _ => None,
            },
            Screen::Victory => None,
            Screen::Colony => match k {
                Key::Esc | Key::Char('o') => Some(Action::Back),
                _ => self.global_key(k),
            },
            Screen::Help => match k {
                Key::Esc | Key::Char('h') => Some(Action::Back),
                _ => None,
            },
            Screen::Map => match k {
                Key::Esc | Key::Char('m') => Some(Action::Back),
                Key::Up => Some(Action::MapStep(-1)),
                Key::Down => Some(Action::MapStep(1)),
                Key::Char(c @ '1'..='9') => Some(Action::MapSelect(c as usize - '1' as usize)),
                Key::Enter => self.map_list.get(self.map_sel).map(|n| Action::ToggleDestination(n.id)),
                Key::Char('t') => self.map_list.get(self.map_sel).map(|n| Action::JumpTo(n.id)),
                Key::Char('c') => Some(Action::ClearDestination),
                _ => self.global_key(k),
            },
            Screen::Planet => match k {
                Key::Esc => Some(Action::Back),
                Key::Char('p') | Key::Right => Some(Action::StepPlanet(1)),
                Key::Left => Some(Action::StepPlanet(-1)),
                Key::Char('s') | Key::Enter => Some(Action::ShuttleTo(Loc::Planet(self.sel_planet))),
                Key::Char('a') => Some(Action::ToggleDroneAuto),
                Key::Char('n') => Some(Action::CycleMineTarget),
                _ => self.global_key(k),
            },
            Screen::Star => match k {
                Key::Esc | Key::Char('i') => Some(Action::Back),
                Key::Char('s') | Key::Enter => Some(Action::ShuttleTo(Loc::Star)),
                _ => self.global_key(k),
            },
            Screen::Trade => match k {
                Key::Esc | Key::Char('k') => Some(Action::Back),
                Key::Up => Some(Action::TradeStep(-1)),
                Key::Down => Some(Action::TradeStep(1)),
                Key::Char('+') | Key::Char('=') => Some(Action::TradeQtyScale(10.0)),
                Key::Char('-') | Key::Char('_') => Some(Action::TradeQtyScale(0.1)),
                Key::Char('b') => Some(match self.trade_tab {
                    TradeTab::Cargo => Action::BuyGood(Good::ALL[self.trade_row.min(Good::ALL.len() - 1)]),
                    _ => Action::Buy(Res::ALL[self.trade_row.min(Res::ALL.len() - 1)]),
                }),
                Key::Char('s') => Some(match self.trade_tab {
                    TradeTab::Cargo => Action::SellGood(Good::ALL[self.trade_row.min(Good::ALL.len() - 1)]),
                    _ => Action::Sell(Res::ALL[self.trade_row.min(Res::ALL.len() - 1)]),
                }),
                Key::Tab => Some(Action::SetTradeTab(match self.trade_tab {
                    TradeTab::Resources => TradeTab::Cargo,
                    TradeTab::Cargo => TradeTab::Port,
                    TradeTab::Port => TradeTab::Resources,
                })),
                Key::Char('y') => Some(Action::RepairHull),
                _ => self.global_key(k),
            },
            Screen::Systems => match k {
                Key::Esc | Key::Char('6') => Some(Action::Back),
                Key::Char('y') => Some(Action::ToggleShields),
                Key::Char('l') => Some(Action::CycleLsEff),
                Key::Char('o') => Some(Action::ToggleLights),
                Key::Char('x') => Some(Action::ToggleRepair),
                Key::Char('a') => Some(Action::ToggleDroneAuto),
                Key::Char('n') => Some(Action::CycleMineTarget),
                Key::Char('b') => Some(Action::CycleFocus),
                _ => self.global_key(k),
            },
            Screen::Resources | Screen::Crew => match k {
                Key::Esc => Some(Action::Back),
                _ => self.global_key(k),
            },
            Screen::Cockpit => match k {
                Key::Char('p') => {
                    // Cockpit P: next planet, then show it.
                    self.apply(Action::StepPlanet(1));
                    Some(Action::Open(Screen::Planet))
                }
                Key::Enter => Some(Action::Jump),
                Key::Left => Some(Action::StepPlanet(-1)),
                Key::Right => Some(Action::StepPlanet(1)),
                _ => self.global_key(k),
            },
        };
        if let Some(a) = act {
            self.apply(a);
        }
    }

    /// Keys that work on every in-game screen (unless that screen binds the
    /// same key to something local).
    fn global_key(&self, k: Key) -> Option<Action> {
        Some(match k {
            Key::Char('1') => Action::ToggleScoop,
            Key::Char('2') => Action::ToggleMining,
            Key::Char('3') => Action::CycleFocus,
            Key::Char('4') => Action::CycleReactor,
            Key::Char('5') => Action::CycleThrottle,
            Key::Char('6') => Action::Open(Screen::Systems),
            Key::Char('m') => Action::Open(Screen::Map),
            Key::Char('t') => Action::Jump,
            Key::Char('s') => Action::ShuttleToggle,
            Key::Char('p') => Action::Open(Screen::Planet),
            Key::Char('i') => Action::Open(Screen::Star),
            Key::Char('c') => Action::Open(Screen::Crew),
            Key::Char('r') => Action::Open(Screen::Resources),
            Key::Char('k') => Action::Open(Screen::Trade),
            Key::Char('o') => Action::Open(Screen::Colony),
            Key::Char('f') => Action::CycleTimeScale,
            Key::Char('x') => Action::SkipToArrival,
            Key::Char('v') => Action::ToggleSalvage,
            Key::Char('g') => Action::Save,
            Key::Char('h') => Action::Open(Screen::Help),
            Key::Char('q') => Action::Quit,
            Key::Esc => Action::Back,
            _ => return None,
        })
    }

    // ------------------------------------------------------------------
    // Real-time tick
    // ------------------------------------------------------------------

    /// Advance the game by `dt_real` seconds of wall-clock time (scaled by
    /// the time-compression setting); autosave hourly (game time).
    pub fn tick(&mut self, dt_real: f64) {
        // Clamp: a stalled frame (window hidden, debugger) must not fast-
        // forward the ship by minutes in one step.
        let dt = dt_real.clamp(0.0, 0.5);
        self.real_clock += dt;
        let Some(sim) = self.sim.as_mut() else { return };
        if sim.over.is_some() {
            return;
        }
        if sim.in_colony {
            // Colony time: about 24 real seconds a year at 1x.
            if sim.ff > 60 {
                sim.ff = 60;
            }
            if sim.ff > 0 {
                let dt_h = dt * Sim::colony_hours_per_sec(sim.ff);
                sim.advance(dt_h, &mut self.gal);
            }
            if self.real_clock - self.last_colony_save >= 30.0 {
                self.last_colony_save = self.real_clock;
                let _ = save::save(sim, &self.gal);
            }
            if sim.colony.as_ref().is_some_and(|c| c.won) && !sim.victory_seen {
                self.screen = Screen::Victory;
            }
            self.drain_log();
            return;
        }
        if sim.colony.as_ref().is_some_and(|c| c.won) && !sim.victory_seen {
            self.screen = Screen::Victory;
        }
        if sim.ff > 0 {
            let dt_h = dt * sim.ff as f64 / 3600.0;
            if sim.autopilot_on {
                sim.autopilot(&mut self.gal);
            }
            sim.advance(dt_h, &mut self.gal);
        }
        if sim.autosave_h >= 1.0 {
            sim.autosave_h = 0.0;
            let _ = save::save(sim, &self.gal);
        }
        if sim.over.is_some() {
            save::delete();
            self.screen = Screen::GameOver;
        }
        self.drain_log();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Help is reachable from the title, where there is no sim: Back must
    /// return to the title, not to a cockpit that cannot draw.
    #[test]
    fn help_from_title_goes_back_to_title() {
        let mut app = App::new(42);
        app.handle_key(Key::Char('h'));
        assert_eq!(app.screen, Screen::Help);
        app.handle_key(Key::Esc);
        assert_eq!(app.screen, Screen::Title);
    }

    #[test]
    fn help_from_cockpit_goes_back_to_cockpit() {
        let mut app = App::with_game(42, 0);
        app.handle_key(Key::Char('h'));
        app.handle_key(Key::Esc);
        assert_eq!(app.screen, Screen::Cockpit);
    }

    /// Up at the top of a list used to wrap to usize::MAX and then index
    /// out of bounds on the next buy/sell.
    #[test]
    fn list_navigation_saturates() {
        let mut app = App::with_game(42, 0);
        app.trade_row = 0;
        app.handle_key(Key::Up);
        app.apply(Action::TradeStep(-1));
        assert_eq!(app.trade_row, 0);
        app.apply(Action::TradeStep(99));
        assert_eq!(app.trade_row, Res::ALL.len() - 1);

        app.apply(Action::Open(Screen::Map));
        assert_eq!(app.screen, Screen::Map);
        app.handle_key(Key::Up);
        assert_eq!(app.map_sel, 0);
        // '0' used to underflow ('0' - '1').
        app.handle_key(Key::Char('0'));
        app.handle_key(Key::Down);
        assert_eq!(app.map_sel, 1.min(app.map_list.len() - 1));
    }

    /// Every key on every screen, from a fresh game: nothing may panic.
    #[test]
    fn key_fuzz_does_not_panic() {
        let keys: Vec<Key> = "abcdefgijklmnoprstuvwxyz0123456789+-="
            .chars()
            .map(Key::Char)
            .chain([Key::Up, Key::Down, Key::Left, Key::Right, Key::Enter])
            .collect();
        for ship in 0..3 {
            let mut app = App::with_game(7, ship);
            for (i, k) in keys.iter().cycle().take(600).enumerate() {
                app.handle_key(*k);
                if i % 37 == 0 {
                    app.handle_key(Key::Esc);
                }
                app.tick(0.1);
                if !app.running {
                    app.running = true;
                }
            }
        }
    }

    #[test]
    fn blocked_trade_explains_why() {
        let mut app = App::with_game(42, 0); // home has no civ
        app.apply(Action::Open(Screen::Trade));
        assert_eq!(app.screen, Screen::Cockpit);
        assert!(app.live_flash().unwrap().contains("No one here"));
    }
}
