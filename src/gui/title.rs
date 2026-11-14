//! Title / ship selection, and the game-over screen.

use super::paint;
use super::theme::*;
use super::View;
use crate::game::{Action, App, Screen};
use crate::sim::background::Captain;
use crate::sim::save;
use crate::sim::score::{self, Outcome};
use crate::sim::{fmt_daytime, fmt_kg, Abundance, Background, GalaxyOpts, Risk, Sex, SHIPS, STARTERS};
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Pos2, RichText, Sense, Shape, Stroke, Ui};

/// The galaxy option rows under the title buttons. Each change rebuilds the
/// galaxy (same seed, new options), so the next New game uses them.
fn galaxy_options(ui: &mut Ui, opts: GalaxyOpts, acts: &mut Vec<Action>) {
    const LABEL_W: f32 = 110.0;
    const BTN_W: f32 = 96.0;
    let row_w = LABEL_W + 3.0 * BTN_W + 3.0 * ui.spacing().item_spacing.x;
    // One option row: a label, then three exclusive choices with tooltips.
    let mut row = |ui: &mut Ui, label: &str, choices: [(&str, &str, bool, GalaxyOpts); 3]| {
        ui.horizontal(|ui| {
            ui.add_space(((ui.available_width() - row_w) / 2.0).max(0.0));
            ui.add_sized(vec2(LABEL_W, 24.0), egui::Label::new(dim(label)));
            for (name, tip, sel, next) in choices {
                let b = egui::Button::selectable(sel, RichText::new(name).color(if sel { KEY } else { TEXT })).min_size(vec2(BTN_W, 24.0));
                if ui.add(b).on_hover_text(tip).clicked() && !sel {
                    acts.push(Action::SetOpts(next));
                }
            }
        });
    };
    let res = Abundance::ALL.map(|a| (a.name(), GalaxyOpts::resources_blurb(a), opts.resources == a, GalaxyOpts { resources: a, ..opts }));
    let civ = Abundance::ALL.map(|a| (a.name(), GalaxyOpts::civs_blurb(a), opts.civs == a, GalaxyOpts { civs: a, ..opts }));
    let risk = Risk::ALL.map(|r| (r.name(), r.blurb(), opts.risk == r, GalaxyOpts { risk: r, ..opts }));
    row(ui, "Resources", res);
    row(ui, "Civilization", civ);
    row(ui, "Risk", risk);
}

/// A simple vector silhouette per hull, pointing right.
fn ship_art(painter: &egui::Painter, c0: Pos2, s: f32, idx: usize, col: Color32) {
    let p = |x: f32, y: f32| c0 + vec2(x, y) * s;
    let line = Stroke::new(1.5, col);
    let fill = col.gamma_multiply(0.25);
    let poly = |pts: Vec<Pos2>| Shape::convex_polygon(pts, fill, line);
    match idx {
        0 => {
            // Keelclipper: a slim dart.
            painter.add(poly(vec![p(1.0, 0.0), p(-0.6, -0.32), p(-0.8, 0.0), p(-0.6, 0.32)]));
            painter.add(poly(vec![p(-0.2, -0.18), p(-0.55, -0.6), p(-0.7, -0.55), p(-0.55, -0.18)]));
            painter.add(poly(vec![p(-0.2, 0.18), p(-0.55, 0.18), p(-0.7, 0.55), p(-0.55, 0.6)]));
        }
        1 => {
            // Cargomaster: cab, spine, container blocks.
            painter.add(poly(vec![p(1.0, -0.15), p(1.0, 0.15), p(0.7, 0.25), p(0.7, -0.25)]));
            painter.line_segment([p(0.7, 0.0), p(-0.9, 0.0)], line);
            for k in 0..4 {
                let x = 0.5 - k as f32 * 0.35;
                painter.add(poly(vec![p(x, -0.42), p(x, -0.05), p(x - 0.3, -0.05), p(x - 0.3, -0.42)]));
                painter.add(poly(vec![p(x, 0.05), p(x, 0.42), p(x - 0.3, 0.42), p(x - 0.3, 0.05)]));
            }
            painter.add(poly(vec![p(-0.9, -0.25), p(-0.9, 0.25), p(-1.05, 0.35), p(-1.05, -0.35)]));
        }
        _ => {
            // Ark: long spine with habitat rings.
            painter.add(poly(vec![p(1.0, 0.0), p(0.8, -0.14), p(-0.95, -0.14), p(-0.95, 0.14), p(0.8, 0.14)]));
            for k in 0..3 {
                let x = 0.45 - k as f32 * 0.55;
                painter.add(Shape::ellipse_stroke(p(x, 0.0), vec2(0.12, 0.62) * s, line));
            }
            painter.add(poly(vec![p(-0.95, -0.3), p(-0.95, 0.3), p(-1.1, 0.4), p(-1.1, -0.4)]));
        }
    }
}

pub fn title(ui: &mut Ui, app: &App, view: &mut View, acts: &mut Vec<Action>) {
    egui::CentralPanel::default().frame(egui::Frame::new().fill(BG_DEEP)).show(ui, |ui| {
        let rect = ui.max_rect();
        let t = view.anim_t as f32;
        let painter = ui.painter().clone();
        paint::starfield(&painter, rect, 0x7171e, 2.2, t);
        // A slow star rising on the left.
        paint::star(&painter, pos2(rect.left() + rect.width() * 0.12, rect.bottom() - rect.height() * 0.15), 70.0, crate::sim::Rgb(255, 200, 130), t);

        ui.vertical_centered(|ui| {
            ui.add_space((rect.height() * 0.06).min(50.0));
            ui.label(RichText::new("COLONIA  GALACTICA").size(56.0).strong().color(ACCENT));
            ui.label(RichText::new("Survive the drift. Found a colony. Grow it to a million.").size(17.0).color(DIM));
            ui.add_space(18.0);
        });

        let card_w = ((ui.available_width() - 60.0) / 3.0).clamp(160.0, 380.0);
        ui.horizontal(|ui| {
            let total = card_w * 3.0 + 2.0 * ui.spacing().item_spacing.x + 30.0;
            ui.add_space(((ui.available_width() - total) / 2.0).max(0.0));
            for (i, sh) in SHIPS[..STARTERS].iter().enumerate() {
                let sel = app.title_sel == i;
                let frame = card_frame()
                    .stroke(Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { KEY } else { Color32::from_rgb(40, 52, 75) }))
                    .fill(if sel { Color32::from_rgb(26, 34, 52) } else { BG_CARD });
                let r = frame
                    .show(ui, |ui| ui.vertical(|ui| {
                        ui.set_width(card_w);
                        ui.set_min_height(300.0);
                        let (art, p) = ui.allocate_painter(vec2(card_w, 90.0), Sense::hover());
                        ship_art(&p, art.rect.center() + vec2((t * 0.8 + i as f32).sin() * 3.0, 0.0), 42.0, i, if sel { KEY } else { ACCENT });
                        ui.label(RichText::new(format!("{}  {}", i + 1, sh.name)).strong().size(17.0).color(if sel { KEY } else { TEXT }));
                        ui.label(dim(if sh.pax > 0 { format!("{} crew + {} passengers", sh.crew, sh.pax) } else { format!("{} crew", sh.crew) }));
                        ui.add_space(4.0);
                        ui.label(sh.blurb);
                        ui.add_space(6.0);
                        egui::Grid::new(("shipstats", i)).num_columns(2).spacing([10.0, 2.0]).show(ui, |ui| {
                            for (k, v) in [
                                ("Hull", format!("{:.0}", sh.hull)),
                                ("Reactor", format!("{:.0} MW", sh.reactor_mw)),
                                ("Drones", format!("{} × {:.0} kg/h", sh.drones, sh.drone_yield)),
                                ("Fuel tank", format!("{} kg", fmt_kg(sh.tanks[0].1))),
                                ("Cargo hold", format!("{:.0} t", sh.cargo_t)),
                                ("FTL", format!("{:.0} ly/h", sh.ftl_speed)),
                            ] {
                                ui.label(dim(k));
                                ui.label(v);
                                ui.end_row();
                            }
                        });
                    }))
                    .response
                    .interact(Sense::click());
                if r.double_clicked() {
                    acts.push(Action::SelectShip(i));
                    acts.push(Action::NewGame);
                } else if r.clicked() {
                    acts.push(Action::SelectShip(i));
                }
            }
        });

        ui.add_space(16.0);
        ui.vertical_centered(|ui| {
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - 640.0) / 2.0).max(0.0));
                ui.label("Galaxy seed");
                let hint = format!("current: {}", app.gal.seed);
                let r = ui.add(egui::TextEdit::singleline(&mut view.seed_text).hint_text(hint).desired_width(220.0));
                if r.changed() {
                    let s = view.seed_text.trim();
                    let parsed = s.parse::<u64>().ok().or_else(|| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok());
                    if let Some(n) = parsed {
                        acts.push(Action::SetSeed(n));
                    }
                }
                ui.label(dim("try 42, 7, 2026"));
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - 720.0) / 2.0).max(0.0));
                let big = |s: &str| egui::Button::new(RichText::new(s).size(17.0).strong()).min_size(vec2(130.0, 36.0));
                if ui.add(big("New game  [Enter]")).clicked() {
                    acts.push(Action::NewGame);
                }
                let has_save = save::exists();
                if ui
                    .add_enabled(has_save, big("Continue  [C]"))
                    .on_disabled_hover_text("No saved game yet.")
                    .clicked()
                {
                    acts.push(Action::Continue);
                }
                if ui.add(big("Scoreboard  [S]")).clicked() {
                    acts.push(Action::Open(Screen::Scores));
                }
                if ui.add(big("Help  [H]")).clicked() {
                    acts.push(Action::Open(Screen::Help));
                }
                if ui.add(big("Quit  [Q]")).clicked() {
                    acts.push(Action::Quit);
                }
            });
            ui.add_space(14.0);
            galaxy_options(ui, app.gal.opts, acts);
            ui.add_space(8.0);
            ui.label(dim("Pick a ship with 1–3 or the arrow keys, or double-click a card."));
            ui.add_space(6.0);
            ui.label(dim(crate::BUILD));
        });
    });
    if app.captain_open {
        captain_popup(ui.ctx(), app, view, acts);
    }
}

/// New game: who is the captain?
fn captain_popup(ctx: &egui::Context, app: &App, view: &mut View, acts: &mut Vec<Action>) {
    if view.cap_given.is_empty() && view.cap_surname.is_empty() {
        let (g, s) = app.random_names(view.anim_t.to_bits());
        view.cap_given = g;
        view.cap_surname = s;
    }
    let ship = SHIPS[app.title_sel.min(STARTERS - 1)];
    egui::Window::new(RichText::new("YOUR CAPTAIN").strong().color(ACCENT))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.label(dim(format!("Fresh out of flight school, taking command of the {}.", ship.name)));
            ui.add_space(6.0);
            egui::Grid::new("cap_names").num_columns(3).spacing([10.0, 6.0]).show(ui, |ui| {
                for (label, which) in [("Given name", 0u64), ("Surname", 1u64)] {
                    ui.label(label);
                    let buf = if which == 0 { &mut view.cap_given } else { &mut view.cap_surname };
                    ui.add(egui::TextEdit::singleline(buf).desired_width(260.0).char_limit(24));
                    if ui.button("↻").on_hover_text("Another name").clicked() {
                        let (g, s) = app.random_names(view.anim_t.to_bits() ^ (which + 1));
                        if which == 0 { view.cap_given = g } else { view.cap_surname = s }
                    }
                    ui.end_row();
                }
                ui.label("Sex");
                ui.horizontal(|ui| {
                    for x in Sex::ALL {
                        if ui.add(egui::Button::selectable(app.title_sex == x, x.name())).on_hover_text("A partner recruited at a port is of the opposite sex.").clicked() {
                            acts.push(Action::SetSex(x));
                        }
                    }
                });
                ui.end_row();
            });
            ui.add_space(8.0);
            ui.label(RichText::new("Background").strong());
            egui::Grid::new("cap_bg").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                for (i, b) in Background::ALL.into_iter().enumerate() {
                    let sel = app.title_background == b;
                    let r = card_frame()
                        .stroke(Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { KEY } else { Color32::from_rgb(40, 52, 75) }))
                        .show(ui, |ui| {
                            // Grid cells lay out horizontally; the card's lines must stack.
                            ui.vertical(|ui| {
                                ui.set_width(250.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(b.name()).strong().color(if sel { KEY } else { TEXT }));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(RichText::new(format!("+{:.0} cr", b.money())).color(GOOD));
                                    });
                                });
                                ui.add(egui::Label::new(dim(b.blurb())).wrap());
                                for bonus in b.bonuses() {
                                    ui.add(egui::Label::new(format!("•  {bonus}")).wrap());
                                }
                            });
                        })
                        .response
                        .interact(Sense::click());
                    if r.clicked() {
                        acts.push(Action::SetBackground(b));
                    }
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let ok = !view.cap_given.trim().is_empty() && !view.cap_surname.trim().is_empty();
                let start = egui::Button::new(RichText::new("Take command").strong().size(16.0)).min_size(vec2(160.0, 32.0));
                if ui.add_enabled(ok, start).on_disabled_hover_text("Your captain needs a name and a surname.").clicked() {
                    acts.push(Action::StartGame(Captain {
                        given: view.cap_given.trim().to_string(),
                        surname: view.cap_surname.trim().to_string(),
                        sex: app.title_sex,
                        background: app.title_background,
                    }));
                    view.cap_given.clear();
                    view.cap_surname.clear();
                }
                if ui.add(egui::Button::new("Cancel  [Esc]").min_size(vec2(100.0, 32.0))).clicked() {
                    acts.push(Action::CancelCaptain);
                }
            });
        });
}

/// The scoreboard: every finished game, best first.
pub fn scores(ui: &mut Ui, _app: &App, view: &mut View, acts: &mut Vec<Action>) {
    egui::CentralPanel::default().frame(egui::Frame::new().fill(BG_DEEP)).show(ui, |ui| {
        let rect = ui.max_rect();
        paint::starfield(ui.painter(), rect, 0x5c0e, 1.2, view.anim_t as f32);
        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            ui.label(RichText::new("SCOREBOARD").size(40.0).strong().color(ACCENT));
            ui.add_space(10.0);
        });
        let board = score::load();
        if board.is_empty() {
            ui.vertical_centered(|ui| ui.label(dim("No finished games yet. Retire, win, or go down fighting to make the board.")));
        } else {
            egui::ScrollArea::vertical().max_height((rect.height() - 180.0).max(120.0)).show(ui, |ui| {
                egui::Grid::new("board").num_columns(9).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                    for h in ["#", "Captain", "Score", "End", "Ship", "Background", "Years", "Colony peak", "Date"] {
                        ui.label(dim(h));
                    }
                    ui.end_row();
                    for (i, e) in board.iter().enumerate() {
                        let col = match e.outcome {
                            Outcome::Victory => GOOD,
                            Outcome::Retired => TEXT,
                            Outcome::Lost => BAD,
                        };
                        ui.label(dim(format!("{}", i + 1)));
                        ui.label(RichText::new(&e.captain).strong());
                        ui.label(RichText::new(super::colony::fmt_pop(e.score as f64)).strong().color(KEY));
                        ui.label(RichText::new(e.outcome.name()).color(col));
                        ui.label(&e.ship);
                        ui.label(&e.background);
                        ui.label(format!("{:.1}", e.years));
                        ui.label(if e.colony_peak > 0.0 { super::colony::fmt_pop(e.colony_peak) } else { "—".into() });
                        ui.label(dim(&e.date));
                        ui.end_row();
                    }
                });
            });
        }
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            if ui.add(egui::Button::new(RichText::new("Back  [Esc]").size(17.0)).min_size(vec2(160.0, 36.0))).clicked() {
                acts.push(Action::Back);
            }
        });
    });
}

pub fn game_over(ui: &mut Ui, app: &App, view: &mut View, acts: &mut Vec<Action>) {
    egui::CentralPanel::default().frame(egui::Frame::new().fill(BG_DEEP)).show(ui, |ui| {
        let rect = ui.max_rect();
        let painter = ui.painter().clone();
        paint::starfield(&painter, rect, 0xdead, 1.2, view.anim_t as f32);
        let Some(sim) = app.sim.as_ref() else { return };
        let Some(over) = sim.over.as_ref() else { return };
        let outcome = sim.outcome();
        if outcome == Outcome::Lost {
            painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(60, 0, 0, 70));
        }
        ui.vertical_centered(|ui| {
            ui.add_space((rect.height() * 0.12).min(90.0));
            let (head, col) = match outcome {
                Outcome::Victory => ("COLONY TRIUMPHANT", GOOD),
                Outcome::Retired => ("RETIRED", ACCENT),
                Outcome::Lost => ("SHIP LOST", CRIT),
            };
            ui.label(RichText::new(head).size(52.0).strong().color(col));
            ui.add_space(6.0);
            ui.label(RichText::new(&over.cause).size(22.0).color(TEXT));
            ui.label(RichText::new(&over.detail).size(16.0).color(DIM));
            ui.add_space(14.0);
            card_frame().show(ui, |ui| {
                ui.set_max_width(460.0);
                egui::Grid::new("go").num_columns(2).spacing([20.0, 4.0]).show(ui, |ui| {
                    let st = &sim.stats;
                    for (k, v) in [
                        ("Ship", sim.ship().name.to_string()),
                        ("Survived until", fmt_daytime(over.t)),
                        ("Colony", sim.colony.as_ref().map(|c| format!("{} — {:.0} people", c.name, c.population())).unwrap_or_else(|| "none".into())),
                        ("Battles won / lost", format!("{} / {}", sim.battles_won, sim.battles_lost)),
                        ("Jumps", format!("{}  ({:.1} ly)", st.jumps, st.dist_ly)),
                        ("Systems visited", st.systems.to_string()),
                        ("Mined / scooped", format!("{} / {} kg", fmt_kg(st.mined), fmt_kg(st.scooped))),
                        ("Crew lost", st.crew_lost.to_string()),
                        ("Passengers lost", st.pax_lost.to_string()),
                        ("Galaxy seed", format!("0x{:012X}", sim.seed)),
                        ("Galaxy", sim.opts.summary()),
                    ] {
                        ui.label(dim(k));
                        ui.label(v);
                        ui.end_row();
                    }
                });
            });
            ui.add_space(10.0);
            let card = sim.score();
            card_frame().show(ui, |ui| {
                ui.set_max_width(460.0);
                ui.label(RichText::new("SCORE").strong().color(ACCENT));
                egui::Grid::new("score").num_columns(3).spacing([20.0, 2.0]).show(ui, |ui| {
                    for (k, v, pts) in &card.lines {
                        if *pts <= 0.0 {
                            continue;
                        }
                        ui.label(dim(*k));
                        ui.label(v);
                        ui.label(format!("{:.0}", pts));
                        ui.end_row();
                    }
                    ui.label(dim(format!("Outcome: {}", card.outcome.name())));
                    ui.label("");
                    ui.label(format!("×{}", card.outcome.mult()));
                    ui.end_row();
                });
                ui.label(RichText::new(format!("Final score {}", super::colony::fmt_pop(card.total as f64))).size(22.0).strong().color(KEY));
                match app.last_rank {
                    Some(r) => ui.label(RichText::new(format!("#{r} on the scoreboard")).color(GOOD)),
                    None if sim.scored => ui.label(dim("Not in the top 100 this time.")),
                    None => ui.label(dim("")),
                };
            });
            ui.add_space(16.0);
            if ui.add(egui::Button::new(RichText::new("Return to title  [Enter]").size(17.0)).min_size(vec2(220.0, 38.0))).clicked() {
                acts.push(Action::AckGameOver);
            }
        });
        let _ = (Align2::CENTER_CENTER, FontId::default());
    });
}
