//! The colony: a turning view of the world with its cities, and every
//! colony stat, in tabs.

use super::paint;
use super::theme::*;
use super::View;
use crate::game::{Action, App, ColonyTab};
use crate::sim::colony::census::Origin;
use crate::sim::colony::{Hab, PlayerColony, Project, WIN_POP};
use crate::sim::people::years;
use crate::sim::{fmt_date, Good, Sim};
use eframe::egui::{self, pos2, vec2, Color32, Pos2, RichText, Sense, Stroke, Ui};

/// Big-number formatting: 1,234,567.
pub fn fmt_pop(n: f64) -> String {
    let n = n.max(0.0).round() as u64;
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Cockpit in colony mode: the world on the left, the colony on the right.
pub fn cockpit(ui: &mut Ui, app: &App, view: &mut View, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let Some(col) = &sim.colony else { return };
    egui::Panel::right("colony_side")
        .resizable(true)
        .default_size(470.0)
        .min_size(340.0)
        .show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| panel(ui, app, acts));
        });
    egui::CentralPanel::default().frame(egui::Frame::new().fill(BG_DEEP)).show(ui, |ui| {
        world_view(ui, app, sim, col, view);
    });
}

/// The colony page (from the top tab).
pub fn page(ui: &mut Ui, app: &App, acts: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        if ui.button("← Back  [Esc]").clicked() {
            acts.push(Action::Back);
        }
        ui.heading(RichText::new("COLONY").color(ACCENT));
    });
    ui.separator();
    panel(ui, app, acts);
}

fn world_view(ui: &mut Ui, app: &App, sim: &Sim, col: &PlayerColony, view: &View) {
    let rect = ui.max_rect();
    let painter = ui.painter_at(rect);
    let t = view.anim_t as f32;
    paint::starfield(&painter, rect, col.census.seed, 1.4, t * 0.2);
    let sys = app.gal.system_ref(col.sys_id);
    let Some(world) = sys.site(col.loc) else { return };
    let r = (rect.width().min(rect.height()) * 0.36).max(20.0);
    let c = rect.center() + vec2(0.0, 10.0);
    let light = vec2(-0.75, -0.35);
    // A day lasts a few seconds of colony time (purely visual).
    let spin = t * 0.06;
    paint::planet(&painter, c, r, world, light, spin);
    city_lights(&painter, c, r, col, light, if world.tidally_locked { 0.0 } else { spin });
    // The space dock circles the world.
    if col.dock {
        let a = t * 0.4;
        let p = c + vec2(a.cos() * r * 1.35, a.sin() * r * 0.35);
        let front = a.sin() > 0.0;
        if front || (p - c).length() > r {
            painter.circle_stroke(p, (r * 0.05).max(3.0), Stroke::new(2.0, Color32::from_rgb(200, 220, 255)));
            painter.circle_filled(p, (r * 0.015).max(1.5), Color32::from_rgb(255, 230, 160));
        }
    }
    // Title and headline numbers.
    let pop = col.population();
    painter.text(rect.left_top() + vec2(16.0, 14.0), egui::Align2::LEFT_TOP, &col.name, egui::FontId::proportional(26.0), ACCENT);
    painter.text(
        rect.left_top() + vec2(16.0, 46.0),
        egui::Align2::LEFT_TOP,
        format!("{} · {} · {}", col.world, col.hab.name(), col.species()),
        egui::FontId::proportional(15.0),
        DIM,
    );
    painter.text(
        rect.left_bottom() + vec2(16.0, -16.0),
        egui::Align2::LEFT_BOTTOM,
        format!("Population {}   ·   {}", fmt_pop(pop), fmt_date(sim.t)),
        egui::FontId::proportional(18.0),
        TEXT,
    );
    // Progress to a million.
    let frac = (pop.max(1.0).log10() / WIN_POP.log10()).clamp(0.0, 1.0) as f32;
    let bar = egui::Rect::from_min_size(rect.left_bottom() + vec2(16.0, -48.0), vec2((rect.width() - 32.0).min(420.0), 8.0));
    painter.rect_filled(bar, 3.0, Color32::from_rgb(30, 38, 58));
    painter.rect_filled(egui::Rect::from_min_size(bar.min, vec2(bar.width() * frac, bar.height())), 3.0, GOOD);
    painter.text(bar.right_top() + vec2(6.0, -4.0), egui::Align2::LEFT_TOP, "→ 1,000,000", egui::FontId::proportional(12.0), DIM);
    if sim.ship_dormant {
        painter.text(rect.right_top() + vec2(-16.0, 14.0), egui::Align2::RIGHT_TOP, "The ship is lost.", egui::FontId::proportional(15.0), BAD);
    }
}

/// Lights of the settlements, on the night side; more and brighter as the
/// colony grows. Domes show on hostile worlds.
fn city_lights(painter: &egui::Painter, c: Pos2, r: f32, col: &PlayerColony, light: egui::Vec2, spin: f32) {
    let pop = col.population().max(1.0);
    let n = ((pop.log10() * 14.0) as usize).clamp(3, 120);
    let l = {
        let v = [light.x, -light.y, 0.65];
        let m = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / m, v[1] / m, v[2] / m]
    };
    let (ss, cs) = spin.sin_cos();
    let seed = col.census.seed;
    let domes = col.hab.needs_domes();
    for i in 0..n {
        let (u, v) = crate::rngx::unit2(seed, i as u64);
        // Cities cluster near the first landing site, then spread.
        let spread = (i as f32 / n as f32).sqrt();
        let lon = (u as f32 - 0.5) * std::f32::consts::TAU * spread;
        let lat = (v as f32 - 0.5) * 2.4 * spread.max(0.15);
        let s = [lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos()];
        // Surface → screen (inverse of the sphere's spin).
        let x = s[0] * cs - s[2] * ss;
        let z = s[0] * ss + s[2] * cs;
        let y = s[1];
        if z <= 0.05 {
            continue;
        }
        let lambert = x * l[0] + y * l[1] + z * l[2];
        let p = c + vec2(x, -y) * r;
        let size = (r * 0.012 * (1.0 + (pop.log10() as f32) * 0.15)).max(1.0);
        if lambert < 0.12 {
            let glow = Color32::from_rgba_unmultiplied(255, 200, 110, 150);
            painter.circle_filled(p, size * 1.8, glow.gamma_multiply(0.35));
            painter.circle_filled(p, size * 0.8, Color32::from_rgb(255, 225, 150));
        } else if domes {
            painter.circle_stroke(p, size * 1.4, Stroke::new(1.0, Color32::from_rgba_unmultiplied(220, 240, 255, 160)));
        } else if i % 3 == 0 {
            painter.circle_filled(p, size * 0.5, Color32::from_rgba_unmultiplied(200, 200, 200, 90));
        }
    }
}

/// The tabbed colony panel.
pub fn panel(ui: &mut Ui, app: &App, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let Some(col) = &sim.colony else {
        ui.label("You have no colony.");
        return;
    };
    // Mode controls.
    ui.horizontal_wrapped(|ui| {
        if sim.in_colony {
            ui.label(RichText::new("COLONY MODE").strong().color(GOOD));
            let b = ui.add_enabled(!sim.ship_dormant, egui::Button::new("Fly the ship (leave the colony)"));
            if b.on_disabled_hover_text("The ship is lost.").clicked() {
                acts.push(Action::LeaveColony);
            }
        } else {
            ui.label(RichText::new("You are away with the ship").color(WARN));
            let here = sim.sys_id == col.sys_id && sim.loc == col.loc && sim.trip.is_none() && sim.shuttle.is_none();
            if ui.add_enabled(here, egui::Button::new("Return to the colony")).on_disabled_hover_text(format!("Dock at {} first.", col.world)).clicked() {
                acts.push(Action::EnterColony);
            }
        }
    });
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for (t, label) in [
            (ColonyTab::Overview, "Overview"),
            (ColonyTab::Supplies, "Supplies"),
            (ColonyTab::Build, "Build"),
            (ColonyTab::Census, "Census"),
            (ColonyTab::Family, "Family"),
            (ColonyTab::Market, "Space dock"),
        ] {
            if ui.add(egui::Button::selectable(app.colony_tab == t, RichText::new(label).strong())).clicked() {
                acts.push(Action::SetColonyTab(t));
            }
        }
    });
    ui.separator();
    match app.colony_tab {
        ColonyTab::Overview => overview(ui, sim, col, acts),
        ColonyTab::Supplies => supplies(ui, sim, col, acts),
        ColonyTab::Build => build(ui, col, acts),
        ColonyTab::Census => census(ui, sim, col, acts),
        ColonyTab::Family => family(ui, app, sim, col, acts),
        ColonyTab::Market => market(ui, app, sim, col, acts),
    }
}

fn stat(ui: &mut Ui, k: &str, frac: f64, label: String, tip: &str) {
    ui.label(dim(k));
    bar(ui, frac, label, 230.0).on_hover_text(tip);
    ui.end_row();
}

fn overview(ui: &mut Ui, sim: &Sim, col: &PlayerColony, acts: &mut Vec<Action>) {
    let pop = col.population();
    let now = years(sim.t);
    egui::Grid::new("col_over").num_columns(2).spacing([14.0, 4.0]).show(ui, |ui| {
        ui.label(dim("Population"));
        ui.label(RichText::new(fmt_pop(pop)).strong().size(17.0));
        ui.end_row();
        ui.label(dim("Founded"));
        ui.label(format!("{} by {} colonists ({:.0} years ago)", fmt_date(col.founded * crate::sim::people::SECS_PER_YEAR), col.founders, now - col.founded));
        ui.end_row();
        ui.label(dim("World"));
        ui.label(format!("{} — {}", col.world, col.hab.name())).on_hover_text(col.hab.describe());
        ui.end_row();
        ui.label(dim("Species"));
        ui.label(col.species());
        ui.end_row();
        stat(ui, "Tech level", col.tech / 8.0, format!("{:.2} (galactic standard 5.0)", col.tech), "AI clusters keep the colony's knowledge. Without them the tech level collapses fast; with enough it slowly rises.");
        stat(ui, "Health", col.health, format!("{:.0}% — {}", col.health * 100.0, col.health_label()), "Medical kits, food, shelter, tech and wealth. Low health shortens lives and kills babies.");
        stat(ui, "Happiness", col.happiness, format!("{:.0}% — {}", col.happiness * 100.0, col.happiness_label()), "World quality, the economy, health and supplies. Captives make everyone miserable.");
        stat(ui, "Wealth", col.wealth, format!("{:.0}% — {}", col.wealth * 100.0, col.wealth_label()), "Raw resources per head, tech and buildings. A small colony on a rich world is wealthy; a big one needs buildings too.");
        if col.ind.genetic > 0.001 {
            stat(ui, "Genetic penalty", col.ind.genetic, format!("{:.0}%", col.ind.genetic * 100.0), "A small founding group: health suffers from the second generation, fading as the colony grows.");
        }
        ui.label(dim("Workers"));
        ui.label(format!("{} (labour {:.0}%)", fmt_pop(col.ind.workers), col.ind.labour_eff * col.ind.power_eff * 100.0));
        ui.end_row();
        ui.label(dim("Materials"));
        let net = col.ind.materials_net_y;
        ui.label(RichText::new(format!("{:.0} t  ({}{:.0} t/yr)", col.materials, if net >= 0.0 { "+" } else { "" }, net)).color(if net >= 0.0 { TEXT } else { BAD }));
        ui.end_row();
        let y = &col.census.last_year;
        ui.label(dim("Last year"));
        ui.label(format!("{} born · {} died", fmt_pop(y.births), fmt_pop(y.deaths)));
        ui.end_row();
        if let Some(l) = col.lifespan() {
            ui.label(dim("Mean age at death"));
            ui.label(format!("{l:.0} years"));
            ui.end_row();
        }
        if y.births >= 1.0 {
            ui.label(dim("Infant mortality"));
            ui.label(format!("{:.1}%  · deaths in childbirth {:.2}%", y.infant_deaths / y.births * 100.0, y.maternal_deaths / y.births * 100.0));
            ui.end_row();
        }
    });
    ui.add_space(6.0);
    history_chart(ui, col);
    let warnings = warnings(col);
    if !warnings.is_empty() {
        ui.add_space(6.0);
        section(ui, "PROBLEMS");
        for w in warnings {
            ui.label(RichText::new(w).color(WARN));
        }
    }
    ui.add_space(10.0);
    section(ui, "GIVE IT UP");
    ui.label(dim("Abandon the colony: it carries on as an independent colony and you fly on with the ship. You may found another."));
    if ui.add_enabled(!sim.ship_dormant, egui::Button::new(RichText::new("Abandon colony").color(BAD))).clicked() {
        acts.push(Action::AbandonColony);
    }
}

fn warnings(col: &PlayerColony) -> Vec<String> {
    let mut w = Vec::new();
    for (g, c) in &col.ind.coverage {
        if *c < 0.95 {
            w.push(format!("{}: only {:.0}% covered.", g.name(), c * 100.0));
        }
    }
    if col.hab.needs_domes() && col.ind.sheltered < 0.999 {
        w.push(format!("{:.0}% of colonists have no shelter.", (1.0 - col.ind.sheltered) * 100.0));
    }
    if col.ind.shortage {
        w.push("Raw materials have run out: construction and the economy stall.".into());
    }
    if col.hybrid && !col.gene_kit {
        w.push("A Genetic Modification Kit is needed before humans and natives can have children.".into());
    }
    w
}

fn history_chart(ui: &mut Ui, col: &PlayerColony) {
    if col.history.len() < 2 {
        return;
    }
    let (resp, painter) = ui.allocate_painter(vec2(ui.available_width().min(440.0), 90.0), Sense::hover());
    let r = resp.rect;
    painter.rect_filled(r, 4.0, BG_CARD);
    let first = col.history[0][0];
    let last = col.history.last().unwrap()[0];
    let span = (last - first).max(1.0);
    let max = col.history.iter().map(|h| h[1]).fold(1.0f32, f32::max).max(1.0).log10();
    let pts: Vec<Pos2> = col
        .history
        .iter()
        .map(|h| pos2(r.left() + 4.0 + (h[0] - first) / span * (r.width() - 8.0), r.bottom() - 4.0 - h[1].max(1.0).log10() / max * (r.height() - 18.0)))
        .collect();
    painter.add(egui::Shape::line(pts, Stroke::new(1.5, GOOD)));
    painter.text(r.left_top() + vec2(6.0, 3.0), egui::Align2::LEFT_TOP, "Population (log scale)", egui::FontId::proportional(11.0), DIM);
}

fn supplies(ui: &mut Ui, sim: &Sim, col: &PlayerColony, acts: &mut Vec<Action>) {
    let pop = col.population();
    let now = years(sim.t);
    ui.label(dim("Each supply serves a number of colonists and wears out with age. Power drives everything else. A manufacturing plant replaces them from raw materials."));
    ui.add_space(4.0);
    egui::Grid::new("col_sup").num_columns(4).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
        for h in ["Supply", "Units / needed", "Coverage", "Next wears out"] {
            ui.label(dim(h));
        }
        ui.end_row();
        for g in Good::SUPPLIES {
            if g == Good::HazardDome && !col.hab.needs_domes() {
                continue;
            }
            let have = col.units(g);
            let need = g.units_for(pop);
            ui.label(RichText::new(g.name()).color(ACCENT)).on_hover_text(g.desc());
            ui.label(format!("{have} / {need}"));
            let cov = col.ind.coverage.get(&g).copied().unwrap_or(0.0);
            bar(ui, cov, format!("{:.0}%", cov * 100.0), 110.0);
            let next = col.supplies.get(&g).and_then(|v| v.iter().map(|b| b.until).min_by(|a, b| a.total_cmp(b)));
            ui.label(dim(next.map(|y| format!("in {:.0} y", (y - now).max(0.0))).unwrap_or_else(|| "—".into())));
            ui.end_row();
        }
    });
    if col.hab.needs_domes() {
        ui.label(dim(format!("Shelter: {} Bio-Domes (10,000 each) + temporary domes (25 each). {:.0}% sheltered.", col.biodomes, col.ind.sheltered * 100.0)));
    }
    if col.hybrid {
        ui.label(if col.gene_kit { dim("Genetic Modification Kit: installed.") } else { RichText::new("Genetic Modification Kit: missing.").color(WARN) });
    }
    ui.add_space(6.0);
    section(ui, "FROM THE SHIP");
    let docked = sim.sys_id == col.sys_id && sim.loc == col.loc && sim.trip.is_none() && sim.shuttle.is_none() && !sim.ship_dormant;
    let has = sim.hold.units.keys().any(|g| g.is_supply() || *g == Good::Slaves);
    super::panels::cargo_hold(ui, sim);
    if ui
        .add_enabled(docked, egui::Button::new("Unload supplies, captives and spare raw materials"))
        .on_disabled_hover_text("The ship must be docked at the colony.")
        .clicked()
    {
        acts.push(Action::UnloadCargo);
    }
    if !has && docked {
        ui.label(dim("Buy colony supplies at a market's Cargo tab and bring them here."));
    }
}

fn build(ui: &mut Ui, col: &PlayerColony, acts: &mut Vec<Action>) {
    ui.label(dim("Projects use materials and labour. More workers build faster; hunger (no hydroponics) and power cuts slow them."));
    ui.horizontal(|ui| {
        ui.label(format!("Plants {} · Bio-Domes {} · Space dock {} · Business Center {}", col.pmp, col.biodomes, if col.dock { "yes" } else { "no" }, if col.business { "yes" } else { "no" }));
    });
    if ui.checkbox(&mut col.auto_build.clone(), "Auto-build plants and domes as the colony grows").clicked() {
        acts.push(Action::ToggleAutoBuild);
    }
    ui.add_space(4.0);
    for p in Project::ALL {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (t, ly) = p.cost();
            ui.horizontal(|ui| {
                ui.label(RichText::new(p.name()).strong());
                ui.label(dim(format!("{t:.0} t · {ly:.0} labour-years")));
            });
            ui.label(dim(p.desc()));
            match col.can_build(p) {
                Ok(()) => {
                    if ui.button("Build").clicked() {
                        acts.push(Action::Build(p));
                    }
                }
                Err(e) => {
                    ui.label(dim(e));
                }
            }
        });
    }
    ui.add_space(6.0);
    section(ui, "UNDER CONSTRUCTION");
    if col.queue.is_empty() {
        ui.label(dim("Nothing."));
    }
    for (i, c) in col.queue.iter().enumerate() {
        ui.horizontal(|ui| {
            bar(ui, c.done, format!("{} {:.0}%", c.kind.name(), c.done * 100.0), 280.0);
            if i == 0 {
                ui.label(dim("building"));
            }
            if ui.small_button("Cancel").clicked() {
                acts.push(Action::CancelBuild(i));
            }
        });
    }
}

fn census(ui: &mut Ui, sim: &Sim, col: &PlayerColony, acts: &mut Vec<Action>) {
    let now = years(sim.t);
    let c = &col.census;
    let pop = c.population();
    ui.horizontal(|ui| {
        if ui.button("Random parent").clicked() {
            acts.push(Action::RandomParent);
        }
        if ui.button("Oldest colonist").clicked() {
            acts.push(Action::OldestColonist);
        }
        if ui.button("Your family").clicked() {
            acts.push(Action::ViewPerson(col.player));
        }
    });
    let captives = c.living.iter().filter(|&&i| c.people[i as usize].origin == Origin::Captive).count();
    egui::Grid::new("census_tot").num_columns(2).spacing([14.0, 2.0]).show(ui, |ui| {
        ui.label(dim("Colonists"));
        ui.label(fmt_pop(pop));
        ui.end_row();
        ui.label(dim("Individual records"));
        ui.label(format!("{} living · {} ever", fmt_pop(c.living.len() as f64), fmt_pop(c.people.len() as f64)));
        ui.end_row();
        if captives > 0 {
            ui.label(dim("Captives"));
            ui.label(RichText::new(format!("{captives}")).color(WARN));
            ui.end_row();
        }
        ui.label(dim("Women of child-bearing age"));
        let pyr = c.pyramid(now);
        let fertile: f64 = pyr.iter().enumerate().filter(|(b, _)| (4..9).contains(b)).map(|(_, v)| v[1]).sum();
        ui.label(fmt_pop(fertile));
        ui.end_row();
    });
    if c.demographic {
        ui.label(dim("The colony is large: most people are counted by age group. Anyone you look at is found and recorded with their family."));
    }
    ui.add_space(6.0);
    pyramid(ui, &c.pyramid(now));
}

fn pyramid(ui: &mut Ui, bands: &[[f64; 2]]) {
    let max = bands.iter().map(|b| b[0].max(b[1])).fold(1.0, f64::max);
    let rows = bands.iter().rposition(|b| b[0] + b[1] >= 0.5).unwrap_or(0) + 1;
    let row_h = 11.0;
    let w = ui.available_width().min(440.0);
    let (resp, painter) = ui.allocate_painter(vec2(w, rows as f32 * row_h + 20.0), Sense::hover());
    let r = resp.rect;
    let mid = r.center().x;
    let half = w / 2.0 - 34.0;
    painter.text(pos2(mid - half, r.top()), egui::Align2::LEFT_TOP, "♂ men", egui::FontId::proportional(11.0), DIM);
    painter.text(pos2(mid + half, r.top()), egui::Align2::RIGHT_TOP, "women ♀", egui::FontId::proportional(11.0), DIM);
    for (i, b) in bands.iter().take(rows).enumerate() {
        let y = r.bottom() - (i as f32 + 1.0) * row_h;
        let lm = (b[0] / max) as f32 * half;
        let lf = (b[1] / max) as f32 * half;
        painter.rect_filled(egui::Rect::from_min_max(pos2(mid - 16.0 - lm, y), pos2(mid - 16.0, y + row_h - 2.0)), 1.0, Color32::from_rgb(110, 160, 230));
        painter.rect_filled(egui::Rect::from_min_max(pos2(mid + 16.0, y), pos2(mid + 16.0 + lf, y + row_h - 2.0)), 1.0, Color32::from_rgb(230, 130, 170));
        if i % 2 == 0 {
            painter.text(pos2(mid, y + row_h / 2.0 - 1.0), egui::Align2::CENTER_CENTER, format!("{}", i * 5), egui::FontId::proportional(9.0), DIM);
        }
    }
}

fn person_button(ui: &mut Ui, col: &PlayerColony, id: u32, now: f64, acts: &mut Vec<Action>) {
    let c = &col.census;
    let Some(p) = c.get(id) else { return };
    let sym = if p.female { "♀" } else { "♂" };
    let status = match p.died {
        Some(d) => format!("†{:.0}", (d - p.born).floor()),
        None => format!("{:.0}", p.age(now).floor()),
    };
    let me = if id == col.player { " (you)" } else { "" };
    let text = RichText::new(format!("{sym} {}{me}  {status}", c.name(id))).color(if p.alive() { TEXT } else { DIM });
    if ui.button(text).clicked() {
        acts.push(Action::ViewPerson(id));
    }
}

fn family(ui: &mut Ui, app: &App, sim: &Sim, col: &PlayerColony, acts: &mut Vec<Action>) {
    let now = years(sim.t);
    let c = &col.census;
    let id = app.person.filter(|i| c.get(*i).is_some()).unwrap_or(col.player);
    let Some(p) = c.get(id) else { return };
    ui.horizontal(|ui| {
        if ui.button("You").clicked() {
            acts.push(Action::ViewPerson(col.player));
        }
        if ui.button("Random parent").clicked() {
            acts.push(Action::RandomParent);
        }
        if ui.button("Oldest colonist").clicked() {
            acts.push(Action::OldestColonist);
        }
    });
    ui.add_space(4.0);
    ui.label(RichText::new(c.name(id)).strong().size(19.0).color(if id == col.player { KEY } else { ACCENT }));
    let born = fmt_date(p.born as f64 * crate::sim::people::SECS_PER_YEAR);
    let life = match p.died {
        Some(d) => format!("born {born}, died {} aged {:.0}", fmt_date(d as f64 * crate::sim::people::SECS_PER_YEAR), (d - p.born).floor()),
        None => format!("born {born}, age {:.0}", p.age(now).floor()),
    };
    ui.label(format!("{} · {life}", if p.female { "Female" } else { "Male" }));
    let origin = match p.origin {
        Origin::Founder => "One of the founders",
        Origin::Born => "Born in the colony",
        Origin::Captive => "A captive",
        Origin::Settler => "Colonist",
    };
    ui.label(dim(format!("{origin}{}", if p.alien { " · native species" } else { "" })));
    if p.due.is_some() {
        ui.label(RichText::new("Expecting a child.").color(GOOD));
    }
    ui.add_space(6.0);
    section(ui, "PARENTS");
    match (p.mother, p.father) {
        (None, None) => {
            ui.label(dim(if p.born < c.founded + 1.0 || p.origin == Origin::Captive { "Born before the colony." } else { "Unknown." }));
        }
        (m, f) => {
            ui.horizontal_wrapped(|ui| {
                for x in [f, m].into_iter().flatten() {
                    person_button(ui, col, x, now, acts);
                }
            });
        }
    }
    section(ui, "PARTNER");
    match p.spouse {
        Some(s) => {
            person_button(ui, col, s, now, acts);
        }
        None => {
            ui.label(dim("None."));
        }
    }
    section(ui, "CHILDREN");
    let kids = c.children(id);
    if kids.is_empty() {
        ui.label(dim("None."));
    } else {
        ui.horizontal_wrapped(|ui| {
            for k in kids {
                person_button(ui, col, k, now, acts);
            }
        });
    }
}

fn market(ui: &mut Ui, app: &App, sim: &Sim, col: &PlayerColony, acts: &mut Vec<Action>) {
    if !col.dock {
        ui.label(dim("No space dock yet. Build one (it needs 100,000 colonists) and merchants will come to you: sell the colony's materials and buy supplies without leaving."));
        return;
    }
    ui.label(RichText::new(format!("Credits: {:.0} cr", sim.credits)).color(GOOD).strong());
    section(ui, "SELL MATERIALS");
    ui.label(format!("Stockpile {:.0} t at {:.0} cr/t", col.materials, col.materials_price()));
    ui.horizontal(|ui| {
        for t in [100.0, 1_000.0, 10_000.0] {
            if ui.add_enabled(col.materials >= 1.0, egui::Button::new(format!("Sell {t:.0} t"))).clicked() {
                acts.push(Action::DockSellMaterials(t));
            }
        }
    });
    section(ui, "VISITING MERCHANTS");
    ui.label(dim(format!("Buys {} units per click (set on the Trade screen's Cargo tab). Deliveries go straight into the colony.", app.cargo_qty)));
    let Some(m) = &col.market else { return };
    egui::Grid::new("dock_mkt").num_columns(4).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
        for (g, o) in &m.offers {
            ui.label(RichText::new(g.name()).color(ACCENT)).on_hover_text(g.desc());
            ui.label(if o.sells && o.stock > 0 { format!("{:.0} cr", o.price) } else { "—".into() });
            ui.label(dim(format!("{} in stock", if o.sells { o.stock } else { 0 })));
            if ui.add_enabled(o.sells && o.stock > 0, egui::Button::new("Buy").small()).clicked() {
                acts.push(Action::DockBuy(*g));
            }
            ui.end_row();
        }
    });
}

/// Founding: shown on the Planets screen for the body the ship is docked at.
pub fn founding(ui: &mut Ui, app: &App, acts: &mut Vec<Action>) {
    let sim = app.sim();
    if sim.colony.is_some() {
        return;
    }
    let sys = app.gal.system_ref(sim.sys_id);
    let Some(p) = sys.site(sim.loc) else { return };
    if crate::sim::colony::world::colonizable(p).is_err() {
        return;
    }
    ui.add_space(8.0);
    section(ui, "FOUND A COLONY HERE");
    match sim.founding_plan(&app.gal) {
        Err(e) => {
            ui.label(dim(e));
        }
        Ok(plan) => {
            let hab = plan.hab;
            ui.label(format!("{} colonists · {} world{}", plan.colonists, hab.name().to_lowercase(), if plan.hybrid { " · Hybrid colony" } else { "" })).on_hover_text(hab.describe());
            egui::Grid::new("found_kit").num_columns(3).spacing([12.0, 2.0]).show(ui, |ui| {
                for (g, have, need) in &plan.kit {
                    ui.label(dim(g.name()));
                    ui.label(format!("{have} / {need}"));
                    let ok = have >= need;
                    ui.label(RichText::new(if ok { "✔" } else { "short" }).color(if ok { GOOD } else { WARN }));
                    ui.end_row();
                }
            });
            for w in &plan.warnings {
                ui.label(RichText::new(w).color(if hab == Hab::Lethal { BAD } else { WARN }));
            }
            ui.label(dim("Your crew settle for good and the ship is mothballed. You can leave to fly for supplies, but you can only ever keep one colony."));
            let label = if app.confirm_found { "Confirm: found the colony" } else { "Found colony" };
            if ui.add(egui::Button::new(RichText::new(label).strong().color(GOOD))).clicked() {
                acts.push(Action::FoundColony);
            }
        }
    }
}

/// The victory screen.
pub fn victory(ui: &mut Ui, app: &App, view: &mut View, acts: &mut Vec<Action>) {
    egui::CentralPanel::default().frame(egui::Frame::new().fill(BG_DEEP)).show(ui, |ui| {
        let rect = ui.max_rect();
        paint::starfield(ui.painter(), rect, 0x51c, 2.0, view.anim_t as f32);
        let Some(sim) = app.sim.as_ref() else { return };
        let Some(col) = &sim.colony else { return };
        ui.vertical_centered(|ui| {
            ui.add_space((rect.height() * 0.15).min(110.0));
            ui.label(RichText::new("COLONY TRIUMPHANT").size(48.0).strong().color(GOOD));
            ui.add_space(8.0);
            ui.label(RichText::new(format!("{} has reached a million people.", col.name)).size(22.0));
            ui.label(dim(format!("{} years after {} founders landed on {}.", (years(sim.t) - col.founded).round(), col.founders, col.world)));
            ui.add_space(20.0);
            if ui.add(egui::Button::new(RichText::new("End the game in victory").size(18.0)).min_size(vec2(300.0, 40.0))).clicked() {
                acts.push(Action::EndInVictory);
            }
            ui.add_space(6.0);
            if ui
                .add_enabled(!sim.ship_dormant, egui::Button::new(RichText::new("Abandon the colony and fly on").size(16.0)).min_size(vec2(300.0, 34.0)))
                .on_disabled_hover_text("The ship is lost: the colony is all there is.")
                .clicked()
            {
                acts.push(Action::VictoryAbandon);
            }
        });
    });
}
