//! eframe/egui front end.
//!
//! Draw functions take the game state read-only and push [`Action`]s; the
//! frame loop applies them afterwards. That keeps every rule in
//! [`crate::game`], and lets tests render every screen headlessly.

pub mod mapview;
pub mod paint;
pub mod panels;
pub mod sysview;
pub mod theme;
pub mod title;
pub mod tripview;

use crate::game::{Action, App, Key, Screen, TIME_SCALES};
use crate::sim::state::loc_label;
use crate::sim::{fmt_daytime, fmt_hours, Res, Sim};
use eframe::egui::{self, Align, Align2, Color32, Layout, RichText, Ui};
use std::time::Instant;
use theme::*;

/// GUI-only state: camera, animation clocks, text fields.
pub struct View {
    /// Real seconds since start (animations).
    pub anim_t: f64,
    /// Warp-field clock: only advances while the game clock runs.
    pub warp_t: f64,
    /// Star-map camera: pan offset (ly, from the current system) and zoom
    /// (px per ly; 0 = fit on next draw).
    pub map_pan: egui::Vec2,
    pub map_zoom: f32,
    pub last_map_sel: usize,
    pub seed_text: String,
    /// System-view camera: zoom multiplier, pan (px), framing, and the
    /// system it was set up for (reset on arrival somewhere new).
    pub sys_zoom: f32,
    pub sys_pan: egui::Vec2,
    pub sys_focus: sysview::Focus,
    pub sys_last: u64,
}

impl Default for View {
    fn default() -> Self {
        View {
            anim_t: 0.0,
            warp_t: 0.0,
            map_pan: egui::Vec2::ZERO,
            map_zoom: 0.0,
            last_map_sel: usize::MAX,
            seed_text: String::new(),
            sys_zoom: 1.0,
            sys_pan: egui::Vec2::ZERO,
            sys_focus: sysview::Focus::A,
            sys_last: u64::MAX,
        }
    }
}

pub struct GuiApp {
    pub app: App,
    pub view: View,
    last: Option<Instant>,
}

impl GuiApp {
    pub fn new(cc: &eframe::CreationContext<'_>, seed: u64) -> Self {
        theme::apply(&cc.egui_ctx);
        let mut app = App::new(seed);
        // Debug builds only: `SPACEGAME_PREVIEW=map,2,40` starts ship 2,
        // runs 40 h of autopilot, and opens the map — for screenshots and
        // layout work without clicking through. Never in release builds.
        if cfg!(debug_assertions)
            && let Ok(spec) = std::env::var("SPACEGAME_PREVIEW")
        {
            preview(&mut app, &spec);
        }
        GuiApp { app, view: View::default(), last: None }
    }
}

/// Debug preview: move the ship to the nearest system showing `feature`
/// and select the body that shows it.
fn preview_feature(app: &mut App, feature: &str) {
    use crate::sim::planet::{AtmoKind, GgFeature, Host, LifeType, PlanetClass};
    use crate::sim::Loc;
    for r in 0..60i64 {
        for cx in -r..=r {
            for cy in -r..=r {
                if cx.abs() != r && cy.abs() != r {
                    continue;
                }
                for id in app.gal.ids_in_cell(cx, cy) {
                    let s = app.gal.system(id).clone();
                    let pick: Option<Loc> = match feature {
                        "pbin" => s.companion.as_ref().filter(|c| c.p_type).map(|_| Loc::Star),
                        "sbin" => (s.companion_dockable() && s.planets.iter().filter(|p| p.host == Host::B).count() >= 2).then_some(Loc::Companion),
                        "natives" => s.sites().into_iter().find(|(_, p)| p.natives.as_ref().is_some_and(|n| n.species_count > 0)).map(|(l, _)| l),
                        "colony" => s.sites().into_iter().find(|(_, p)| p.colony.as_ref().is_some_and(|c| c.tech.active())).map(|(l, _)| l),
                        "belt" => s.sites().into_iter().find(|(_, p)| p.class == PlanetClass::AsteroidBelt).map(|(l, _)| l),
                        "rings" => s.sites().into_iter().find(|(_, p)| p.gg_feature == Some(GgFeature::Rings) && p.satellites.len() >= 6).map(|(l, _)| l),
                        "storm" => s.sites().into_iter().find(|(_, p)| p.gg_feature == Some(GgFeature::SingleSuperStorm)).map(|(l, _)| l),
                        "life" => s.sites().into_iter().find(|(_, p)| p.life == Some(LifeType::Forests) && !p.is_moon && p.geosphere.is_some_and(|g| g.is_wet())).map(|(l, _)| l),
                        "locked" => s.sites().into_iter().find(|(_, p)| p.tidally_locked && p.atmosphere == Some(AtmoKind::Breathable)).map(|(l, _)| l),
                        "corrosive" => s.sites().into_iter().find(|(_, p)| p.atmosphere == Some(AtmoKind::Corrosive) && !p.is_moon).map(|(l, _)| l),
                        "ice" => s.sites().into_iter().find(|(_, p)| p.class == PlanetClass::IceWorld && !p.is_moon && p.terrain.iter().any(|t| t.contains("fissure"))).map(|(l, _)| l),
                        _ => None,
                    };
                    if let Some(l) = pick {
                        let sim = app.sim.as_mut().unwrap();
                        sim.sys_id = id;
                        sim.trip = None;
                        sim.shuttle = None;
                        sim.loc = Loc::Star;
                        if l.is_body() {
                            app.apply(Action::SelectBody(l));
                        }
                        return;
                    }
                }
            }
        }
    }
}

fn preview(app: &mut App, spec: &str) {
    let mut it = spec.split(',');
    let screen = it.next().unwrap_or("cockpit");
    let ship = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let hours: u32 = it.next().and_then(|s| s.parse().ok()).unwrap_or(30);
    if screen == "title" {
        return;
    }
    app.start_new_game(ship);
    for _ in 0..hours * 4 {
        let sim = app.sim.as_mut().unwrap();
        sim.autopilot(&mut app.gal);
        sim.advance(0.25, &mut app.gal);
        if sim.over.is_some() {
            break;
        }
    }
    app.drain_log();
    if let Some(feature) = it.next() {
        preview_feature(app, feature);
    }
    app.refresh_map();
    let s = match screen {
        "map" => Screen::Map,
        "planet" => Screen::Planet,
        "star" => Screen::Star,
        "systems" => Screen::Systems,
        "resources" => Screen::Resources,
        "crew" => Screen::Crew,
        "trade" => Screen::Trade,
        "help" => Screen::Help,
        "trip" => {
            let sim = app.sim.as_mut().unwrap();
            if sim.trip.is_none()
                && let Some(n) = app.map_list.first().cloned() {
                    sim.shuttle = None;
                    sim.destination = Some(n.id);
                    sim.ftl_charge = 100.0;
                    sim.start_trip(&mut app.gal);
                }
            Screen::Cockpit
        }
        _ => Screen::Cockpit,
    };
    app.screen = s;
}

impl eframe::App for GuiApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        let dt = self.last.map(|l| now.duration_since(l).as_secs_f64()).unwrap_or(0.0);
        self.last = Some(now);
        self.app.tick(dt);
        self.view.anim_t += dt.min(0.5);
        if self.app.sim.as_ref().is_some_and(|s| s.ff > 0 && s.over.is_none()) {
            self.view.warp_t += dt.min(0.5);
        }

        // Window close button / Alt+F4: autosave like Q does.
        if ctx.input(|i| i.viewport().close_requested()) && self.app.running {
            self.app.quit();
        }
        if !self.app.running {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // The sim runs in real time, so keep frames coming. Animated
        // screens get every frame; the rest a steady ~20 fps.
        let animated = matches!(self.app.screen, Screen::Title)
            || self.app.sim.as_ref().is_some_and(|s| s.trip.is_some());
        if animated {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        handle_keys(ui.ctx(), &mut self.app);
        let actions = draw(ui, &self.app, &mut self.view);
        for a in actions {
            self.app.apply(a);
        }
    }
}

/// Route keyboard input to the shared bindings — unless a text field has
/// focus (typing a seed must not fire "Q = quit").
fn handle_keys(ctx: &egui::Context, app: &mut App) {
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    let events = ctx.input(|i| i.events.clone());
    for ev in events {
        let key = match ev {
            egui::Event::Key { key, pressed: true, modifiers, .. } => match key {
                egui::Key::C if modifiers.command => Some(Key::CtrlC),
                egui::Key::Escape => Some(Key::Esc),
                egui::Key::Enter => Some(Key::Enter),
                egui::Key::ArrowUp => Some(Key::Up),
                egui::Key::ArrowDown => Some(Key::Down),
                egui::Key::ArrowLeft => Some(Key::Left),
                egui::Key::ArrowRight => Some(Key::Right),
                _ => None,
            },
            egui::Event::Text(t) => {
                for ch in t.chars() {
                    app.handle_key(Key::Char(ch));
                }
                None
            }
            _ => None,
        };
        if let Some(k) = key {
            app.handle_key(k);
        }
    }
}

/// Draw the current screen; returns the actions the player triggered.
pub fn draw(ui: &mut Ui, app: &App, view: &mut View) -> Vec<Action> {
    let mut acts = Vec::new();
    match app.screen {
        Screen::Title => title::title(ui, app, view, &mut acts),
        Screen::GameOver => title::game_over(ui, app, view, &mut acts),
        Screen::Help if app.sim.is_none() => {
            egui::CentralPanel::default().show(ui, |ui| panels::help(ui, app, &mut acts));
        }
        _ => in_game(ui, app, view, &mut acts),
    }
    toast(ui.ctx(), app);
    if view.last_map_sel != app.map_sel && app.screen != Screen::Map {
        view.last_map_sel = usize::MAX;
    }
    acts
}

fn in_game(ui: &mut Ui, app: &App, view: &mut View, acts: &mut Vec<Action>) {
    let sim = app.sim();
    egui::Panel::top("top").show(ui, |ui| top_bar(ui, app, sim, acts));
    egui::Panel::bottom("log")
        .resizable(true)
        .default_size(150.0)
        .min_size(60.0)
        .show(ui, |ui| status_log(ui, app));
    match app.screen {
        Screen::Cockpit => {
            egui::Panel::right("controls")
                .resizable(true)
                .default_size(360.0)
                .min_size(300.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| panels::controls(ui, app, acts));
                });
            egui::CentralPanel::default().frame(egui::Frame::new().fill(BG_DEEP)).show(ui, |ui| {
                if sim.trip.is_some() {
                    tripview::draw(ui, app, view, acts);
                } else {
                    sysview::draw(ui, app, view, acts);
                }
            });
        }
        Screen::Map => mapview::draw(ui, app, view, acts),
        _ => {
            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match app.screen {
                    Screen::Planet => panels::planet(ui, app, view, acts),
                    Screen::Star => panels::star(ui, app, view, acts),
                    Screen::Resources => panels::resources(ui, app, acts),
                    Screen::Crew => panels::crew(ui, app, acts),
                    Screen::Systems => panels::systems(ui, app, acts),
                    Screen::Trade => panels::trade(ui, app, acts),
                    Screen::Help => panels::help(ui, app, acts),
                    _ => {}
                });
            });
        }
    }
}

fn top_bar(ui: &mut Ui, app: &App, sim: &Sim, acts: &mut Vec<Action>) {
    let spec = sim.ship();
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("FTL DRIFT").strong().size(20.0).color(ACCENT));
        ui.label(RichText::new(spec.name).strong().size(16.0));
        ui.label(dim(format!("seed 0x{:012X}", sim.seed)));
        ui.separator();
        ui.label(RichText::new(fmt_daytime(sim.t)).monospace().size(16.0));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button(with_key("Quit", "Q")).on_hover_text("Save and exit").clicked() {
                acts.push(Action::Quit);
            }
            if ui.button(with_key("Help", "H")).clicked() {
                acts.push(Action::Open(Screen::Help));
            }
            if ui.button(with_key("Save", "G")).clicked() {
                acts.push(Action::Save);
            }
            ui.separator();
            if sim.trip.is_some()
                && ui.button(RichText::new("Skip to arrival  [X]").color(CIV)).clicked()
            {
                acts.push(Action::SkipToArrival);
            }
            for &ff in TIME_SCALES.iter().rev() {
                let label = if ff == 0 { "Pause".to_string() } else { format!("{ff}x") };
                let txt = if ff == 0 && sim.ff == 0 {
                    RichText::new("PAUSED").color(BAD).strong()
                } else {
                    RichText::new(label)
                };
                if ui.selectable_label(sim.ff == ff, txt).clicked() {
                    acts.push(Action::SetTimeScale(ff));
                }
            }
            ui.label(dim("time [F]"));
        });
    });

    ui.horizontal_wrapped(|ui| {
        let sys = app.gal.system_ref(sim.sys_id);
        if let Some(trip) = &sim.trip {
            let done = trip.dist_ly * (1.0 - trip.remaining_h / trip.total_h.max(1e-9));
            ui.label(RichText::new("FTL TRANSIT").strong().color(CIV));
            ui.label(format!("→ {}   {:.1}/{:.1} ly   ETA {}", trip.dest_name, done, trip.dist_ly, fmt_hours(trip.remaining_h)));
        } else {
            ui.label(dim("System"));
            ui.label(RichText::new(format!("{} ({})", sys.star.name, sys.star.catalog)).color(c(sys.star.class.color())).strong());
            ui.label(dim("at"));
            ui.label(loc_label(sim.loc, sys));
            if let Some(leg) = &sim.shuttle {
                ui.label(RichText::new(format!("→ {} ({})", loc_label(leg.to, sys), fmt_hours(leg.remaining_h))).color(WARN));
            }
            if let Some(civ) = &sys.civ {
                let what = match civ.tier {
                    crate::sim::civ::CivTier::T3 => "station",
                    crate::sim::civ::CivTier::T2 => "market",
                    crate::sim::civ::CivTier::T1 => "no trade",
                };
                ui.label(RichText::new(format!("· {} ({what})", civ.display())).color(CIV));
            }
        }
        ui.separator();
        for (r, label) in [(Res::H2, "FUEL"), (Res::Oxygen, "O2"), (Res::Water, "H2O"), (Res::Food, "FOOD")] {
            let f = sim.inv.fraction(r);
            bar(ui, f, format!("{label} {:.0}%", f * 100.0), 92.0);
        }
        let hf = sim.hull / spec.hull;
        bar(ui, hf, format!("HULL {:.0}%", hf * 100.0), 92.0);
        if !sim.faults.is_empty() {
            ui.label(RichText::new(format!("⚠ {} fault", sim.faults.len())).color(BAD))
                .on_hover_text(sim.faults.iter().map(|f| f.kind.name()).collect::<Vec<_>>().join("\n"));
        }
    });

    ui.horizontal(|ui| {
        let tabs: [(Screen, &str, &str); 8] = [
            (Screen::Cockpit, "Cockpit", "Esc"),
            (Screen::Map, "Star map", "M"),
            (Screen::Planet, "Planets", "P"),
            (Screen::Star, "Star", "I"),
            (Screen::Systems, "Ship systems", "6"),
            (Screen::Resources, "Resources", "R"),
            (Screen::Crew, "Crew", "C"),
            (Screen::Trade, "Trade", "K"),
        ];
        for (s, label, key) in tabs {
            let blocked = s == Screen::Trade && app.trade_block_reason().is_some();
            let text = RichText::new(format!("{label} [{key}]")).strong();
            let r = ui.add_enabled(!blocked, egui::Button::selectable(app.screen == s, text));
            let r = if blocked { r.on_disabled_hover_text(app.trade_block_reason().unwrap_or("")) } else { r };
            if r.clicked() {
                acts.push(if s == Screen::Cockpit { Action::Back } else { Action::Open(s) });
            }
        }
    });
    ui.add_space(2.0);
}

/// The event/status history: sim log events and UI messages, newest last.
fn status_log(ui: &mut Ui, app: &App) {
    ui.horizontal(|ui| {
        section(ui, "LOG");
        ui.label(dim("ship events and messages — drag the edge to resize"));
    });
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            if app.flash_history.is_empty() {
                ui.label(dim("status messages appear here"));
            }
            for e in &app.flash_history {
                let (tag, col) = level_style(e.level);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if let Some(t) = e.t {
                        ui.label(RichText::new(format!("[{}]", fmt_daytime(t))).monospace().color(DIM).size(12.5));
                    }
                    ui.label(RichText::new(format!("{tag}{}", e.msg)).color(col));
                });
            }
        });
}

/// The newest flash message, floating over the top of the window.
fn toast(ctx: &egui::Context, app: &App) {
    let Some(msg) = app.live_flash() else { return };
    egui::Area::new(egui::Id::unique("toast"))
        .anchor(Align2::CENTER_TOP, [0.0, 118.0])
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(15, 30, 45, 235))
                .stroke(egui::Stroke::new(1.0, ACCENT))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.label(RichText::new(msg).color(Color32::WHITE).strong());
                });
        });
}

/// Power budget: (generation MW, draw MW) for the current settings.
pub fn power_budget(sim: &Sim) -> (f64, f64) {
    use crate::sim::state::{FaultKind, Focus};
    let spec = sim.ship();
    let reactor_fault = sim.faults.iter().any(|f| f.kind == FaultKind::Reactor);
    let moving = sim.shuttle.is_some();
    let tripping = sim.trip.is_some();
    let mut load = spec.ls_mw * sim.ls_eff as f64 / 100.0;
    if sim.scoop_on && matches!(sim.loc, crate::sim::Loc::Star) && !moving && !tripping {
        load += spec.scoop_mw;
    }
    if sim.mining_on && matches!(sim.loc, crate::sim::Loc::Planet(_)) && !moving && !tripping {
        load += spec.drones as f64 * spec.drone_mw;
    }
    if sim.focus != Focus::Idle {
        load += spec.fab_mw;
    }
    if sim.shields {
        load += spec.shields_mw;
    }
    if tripping {
        load += spec.ftl_mw;
    }
    if !tripping && sim.ftl_charge < 100.0 {
        load += spec.precharge_mw;
    }
    let gen_mw = spec.reactor_mw * sim.reactor_pct as f64 / 100.0 * if reactor_fault { 0.55 } else { 1.0 };
    (gen_mw, load)
}

#[cfg(test)]
mod smoke_tests {
    use super::*;
    use crate::sim::state::{GameOver, Trip};

    fn frame(ctx: &egui::Context, app: &App, view: &mut View, w: f32, h: f32) -> Vec<Action> {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h))),
            ..Default::default()
        };
        let mut out = Vec::new();
        // Two passes: egui lays some things out on the second frame.
        for _ in 0..2 {
            let mut full = ctx.run_ui(input.clone(), |ui| out = draw(ui, app, view));
            // Headless: no renderer consumes the font-atlas uploads.
            full.textures_delta.clear();
        }
        out
    }

    /// Draw every screen, mid-flight, paused and game-over, for all three
    /// ships at several window sizes, with no GPU. A layout bug or an
    /// out-of-range index panics here instead of on a player's screen.
    #[test]
    fn every_screen_renders() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        for &(w, h) in &[(800.0f32, 500.0f32), (1280.0, 800.0), (2560.0, 1440.0), (320.0, 240.0)] {
            let mut view = View::default();
            let mut title_app = App::new(42);
            for s in [Screen::Title, Screen::Help] {
                title_app.screen = s;
                frame(&ctx, &title_app, &mut view, w, h);
            }
            for ship in 0..3usize {
                let mut app = App::with_game(42, ship);
                for _ in 0..60 {
                    {
                        let sim = app.sim.as_mut().unwrap();
                        sim.autopilot(&mut app.gal);
                        sim.advance(1.0, &mut app.gal);
                    }
                    app.drain_log();
                }
                app.flash("smoke test");
                app.refresh_map();
                for s in [
                    Screen::Cockpit,
                    Screen::Map,
                    Screen::Planet,
                    Screen::Star,
                    Screen::Resources,
                    Screen::Crew,
                    Screen::Systems,
                    Screen::Trade,
                    Screen::Help,
                ] {
                    app.screen = s;
                    frame(&ctx, &app, &mut view, w, h);
                }
                // Mid-shuttle.
                {
                    let sim = app.sim.as_mut().unwrap();
                    sim.trip = None;
                    sim.shuttle = None;
                    let _ = sim.begin_shuttle(&mut app.gal, crate::sim::Loc::Planet(0));
                }
                app.screen = Screen::Cockpit;
                frame(&ctx, &app, &mut view, w, h);
                // Mid-flight, running and paused.
                if let Some(n) = app.map_list.first().cloned() {
                    {
                        let sim = app.sim.as_mut().unwrap();
                        sim.shuttle = None;
                        sim.destination = Some(n.id);
                        sim.trip = Some(Trip {
                            dest_id: n.id,
                            dest_pos: n.pos,
                            dest_name: n.star_name.clone(),
                            dist_ly: n.dist_ly,
                            total_h: 60.0,
                            remaining_h: 30.0,
                            dust_rate: 0.1,
                        });
                    }
                    app.screen = Screen::Cockpit;
                    frame(&ctx, &app, &mut view, w, h);
                    app.sim.as_mut().unwrap().ff = 0;
                    frame(&ctx, &app, &mut view, w, h);
                    app.screen = Screen::Map;
                    frame(&ctx, &app, &mut view, w, h);
                    app.sim.as_mut().unwrap().trip = None;
                }
                app.sim.as_mut().unwrap().over =
                    Some(GameOver { cause: "smoke test".into(), detail: "drawn, not died".into(), t: 100.0 });
                app.screen = Screen::GameOver;
                frame(&ctx, &app, &mut view, w, h);
            }
        }
    }

    /// Every new kind of place, docked and viewed: close and wide binaries,
    /// natives, colonies, salvage, belts, gas-giant moons, the companion
    /// star. Systems are found by scanning the galaxy, then visited directly.
    #[test]
    fn feature_tour_renders() {
        use crate::sim::planet::PlanetClass;
        use crate::sim::Loc;
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut view = View::default();
        let mut app = App::with_game(42, 2);
        let mut want: Vec<(&str, Option<u64>)> = vec![
            ("p-type", None),
            ("s-type", None),
            ("natives", None),
            ("colony", None),
            ("salvage", None),
            ("belt", None),
        ];
        'scan: for cx in -40..40 {
            for cy in -40..40 {
                for id in app.gal.ids_in_cell(cx, cy) {
                    let s = app.gal.system(id).clone();
                    for (k, slot) in want.iter_mut() {
                        if slot.is_some() {
                            continue;
                        }
                        let hit = match *k {
                            "p-type" => s.companion.as_ref().is_some_and(|c| c.p_type),
                            "s-type" => s.companion_dockable() && s.planets.iter().any(|p| p.host == crate::sim::planet::Host::B),
                            "natives" => s.sites().iter().any(|(_, p)| p.natives.as_ref().is_some_and(|n| n.species_count > 0)),
                            "colony" => s.sites().iter().any(|(_, p)| p.colony.is_some()),
                            "salvage" => s.sites().iter().any(|(_, p)| p.salvage.is_some()),
                            "belt" => s.planets.iter().any(|p| p.class == PlanetClass::AsteroidBelt),
                            _ => false,
                        };
                        if hit {
                            *slot = Some(id);
                        }
                    }
                    if want.iter().all(|(_, s)| s.is_some()) {
                        break 'scan;
                    }
                }
            }
        }
        for (k, slot) in &want {
            let id = slot.unwrap_or_else(|| panic!("no {k} system found"));
            let sys = app.gal.system(id).clone();
            let mut locs = vec![Loc::Star];
            if sys.companion_dockable() {
                locs.push(Loc::Companion);
            }
            for (l, p) in sys.sites() {
                let interesting = p.natives.is_some() || p.colony.is_some() || p.salvage.is_some() || p.class == PlanetClass::AsteroidBelt || matches!(l, Loc::Moon(_, 0));
                if interesting && locs.len() < 8 {
                    locs.push(l);
                }
            }
            {
                let sim = app.sim.as_mut().unwrap();
                sim.sys_id = id;
                sim.trip = None;
                sim.shuttle = None;
            }
            app.refresh_map();
            for l in locs {
                app.sim.as_mut().unwrap().loc = l;
                if l.is_body() {
                    app.apply(Action::SelectBody(l));
                }
                for scr in [Screen::Cockpit, Screen::Planet, Screen::Star, Screen::Trade, Screen::Map] {
                    app.screen = scr;
                    frame(&ctx, &app, &mut view, 1280.0, 800.0);
                }
                // Zoomed in, and framing the companion.
                view.sys_zoom = 4.0;
                view.sys_focus = sysview::Focus::B;
                app.screen = Screen::Cockpit;
                frame(&ctx, &app, &mut view, 1280.0, 800.0);
                view.sys_focus = sysview::Focus::Both;
                frame(&ctx, &app, &mut view, 1280.0, 800.0);
                view.sys_zoom = 1.0;
                view.sys_focus = sysview::Focus::A;
            }
        }
    }
}
