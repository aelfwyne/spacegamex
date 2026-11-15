//! The colony: a turning view of the world with its cities, and every
//! colony stat, in tabs.

use super::paint;
use super::theme::*;
use super::View;
use crate::game::{Action, App, ColonyTab};
use crate::sim::colony::census::{Origin, Species};
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
/// The colony on screen (the capital unless another is selected).
pub fn viewed(app: &App) -> Option<&PlayerColony> {
    let sim = app.sim();
    sim.colony_at(app.sel_colony.min(sim.outposts.len())).or(sim.colony.as_ref())
}

pub fn panel(ui: &mut Ui, app: &App, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let Some(col) = viewed(app) else {
        ui.label("You have no colony.");
        return;
    };
    let sel = app.sel_colony.min(sim.outposts.len());
    if sim.colony_count() > 1 {
        ui.horizontal_wrapped(|ui| {
            ui.label(dim("Colonies:"));
            for (i, c) in sim.all_colonies().enumerate() {
                let label = format!("{}{}{}", if i == sel { "▶ " } else { "" }, c.name, if i == 0 { " (capital)" } else { "" });
                let text = RichText::new(label).strong().color(if i == sel { KEY } else { TEXT });
                if ui
                    .add(egui::Button::selectable(i == sel, text))
                    .on_hover_text(format!("{} on {} · pop {}", c.name, c.world, fmt_pop(c.population())))
                    .clicked()
                {
                    acts.push(Action::SelectColony(i));
                }
            }
        });
    }
    // Mode controls.
    ui.horizontal_wrapped(|ui| {
        if sim.in_colony {
            ui.label(RichText::new("COLONY MODE").strong().color(GOOD));
            let b = ui.add_enabled(!sim.ship_dormant, egui::Button::new("Fly the ship (leave the colony)"));
            if b.on_disabled_hover_text("The ship is lost. Take command of another from the Fleet tab.").clicked() {
                acts.push(Action::LeaveColony);
            }
        } else {
            ui.label(RichText::new("You are away with the ship").color(WARN));
            let here = sim.sys_id == col.sys_id && sim.loc == col.loc && sim.trip.is_none() && sim.shuttle.is_none();
            if col.outpost {
                ui.label(dim(format!("{} is run by its settlers: your family lives at the capital.", col.name)));
            } else if ui.add_enabled(here, egui::Button::new("Return to the colony")).on_disabled_hover_text(format!("Dock at {} first.", col.world)).clicked() {
                acts.push(Action::EnterColony);
            }
        }
    });
    materials_strip(ui, col);
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for (t, label) in [
            (ColonyTab::Overview, "Overview"),
            (ColonyTab::Supplies, "Supplies"),
            (ColonyTab::Build, "Build"),
            (ColonyTab::Census, "Census"),
            (ColonyTab::Family, "Family"),
            (ColonyTab::Laws, "Laws"),
            (ColonyTab::Market, "Space dock"),
            (ColonyTab::Fleet, "Fleet"),
        ] {
            if ui.add(egui::Button::selectable(app.colony_tab == t, RichText::new(label).strong())).clicked() {
                acts.push(Action::SetColonyTab(t));
            }
        }
    });
    ui.separator();
    match app.colony_tab {
        ColonyTab::Overview => overview(ui, app, sim, col, acts),
        ColonyTab::Laws => laws(ui, app, sim, col, acts),
        ColonyTab::Supplies => supplies(ui, sim, col, acts),
        ColonyTab::Build => build(ui, col, acts),
        ColonyTab::Census => census(ui, sim, col, acts),
        ColonyTab::Family => family(ui, app, sim, col, acts),
        ColonyTab::Market => market(ui, app, sim, col, acts),
        ColonyTab::Fleet => fleet(ui, sim, col, acts),
    }
}

/// "+12 t/yr" with a sign, coloured by direction.
fn rate_text(v: f64) -> RichText {
    let s = format!("{}{:.0} t/yr", if v >= 0.0 { "+" } else { "−" }, v.abs());
    RichText::new(s).color(if v >= 0.5 { GOOD } else if v <= -0.5 { BAD } else { TEXT })
}

/// The materials stockpile and where it is heading, always in view.
fn materials_strip(ui: &mut Ui, col: &PlayerColony) {
    let i = &col.ind;
    let tip = format!(
        "Raw materials: building, laws, the manufacturing plant and trade all draw on them.\n\nMined and hauled  +{:.0} t/yr\nUpkeep  −{:.0} t/yr\nConstruction  −{:.0} t/yr\nManufacturing plant  −{:.0} t (last year)\n\nAdd more by unloading spare raw materials from the ship (Supplies tab), or with Resource Drones.",
        i.materials_made_y, i.materials_upkeep_y, i.materials_build_y, col.plant_spent_t
    );
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("MATERIALS").small().color(DIM));
        ui.label(RichText::new(format!("{:.0} t", col.materials)).strong().color(if col.materials < 1.0 { BAD } else { KEY }));
        ui.label(rate_text(col.materials_trend_y()));
        if !col.queue.is_empty() {
            ui.label(dim(format!("· building {}", col.queue[0].kind.name())));
        }
    })
    .response
    .on_hover_text(tip);
}

/// One line of a hull's numbers.
fn ship_line(s: &crate::sim::ShipSpec) -> String {
    format!(
        "{} · crew {} · hull {:.0} · hold {:.0} t · FTL {:.0} ly/h · attack {:.1} / defense {:.1}",
        s.role.name(),
        s.crew,
        s.hull,
        s.cargo_t,
        s.ftl_speed,
        s.attack,
        s.defense
    )
}

fn fleet(ui: &mut Ui, sim: &Sim, col: &PlayerColony, acts: &mut Vec<Action>) {
    use crate::sim::{SHIPS, STARTERS};
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        section(ui, "YOUR FLEET");
        ui.label(dim("Every ship keeps its own stores, cargo, crew and fittings. You, your partner and your pilot move between them here at the colony."));
        if sim.ship_dormant {
            ui.label(RichText::new("Flagship: none — your ship was lost.").color(BAD));
        } else {
            let s = sim.ship();
            ui.label(RichText::new(format!("Flagship: {} (hull {:.0}%, hold {:.0}/{:.0} t)", s.name, sim.hull / s.hull * 100.0, sim.hold.mass_t(), s.cargo_t)).strong().color(GOOD));
        }
        let switch_ok = sim.at_fleet();
        if sim.fleet.is_empty() {
            ui.label(dim("No ships laid up. Buy one at an interstellar shipyard (Trade › Port) and add it to the fleet, or build one here."));
        }
        for (i, b) in sim.fleet.iter().enumerate() {
            let s = b.spec();
            card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(s.name).strong());
                    ui.label(dim(format!("hull {:.0}% · hold {:.0}/{:.0} t · H2 {:.0} kg · {} crew", b.hull / s.hull * 100.0, b.hold.mass_t(), s.cargo_t, b.inv.get(crate::sim::Res::H2), b.crew.len())));
                });
                ui.label(dim(ship_line(s)));
                if b.pax.count > 0 {
                    let royal = b.settlers.iter().take(b.pax.count).filter(|e| e.royal).count();
                    let kin = if royal > 0 { format!(", {royal} of the royal line") } else { String::new() };
                    ui.label(RichText::new(format!("{} settlers aboard{kin}: take command and found a colony with them.", b.pax.count)).color(GOOD));
                }
                ui.horizontal(|ui| {
                    let mut r = ui.add_enabled(switch_ok.is_ok(), egui::Button::new("Take command"));
                    if let Err(e) = &switch_ok {
                        r = r.on_disabled_hover_text(e.clone());
                    }
                    if r.clicked() {
                        acts.push(Action::SwitchShip(i));
                    }
                    if ui.small_button(RichText::new("Break up").color(BAD)).on_hover_text("Scrap it for colony materials. Its cargo is lost.").clicked() {
                        acts.push(Action::ScrapShip(i));
                    }
                });
            });
        }
        ui.add_space(6.0);
        section(ui, "SHIPYARD");
        if !col.shipyard {
            ui.label(dim("The colony has no shipyard. It is the crowning building (Build tab): it needs a space dock, a Business Center and 250,000 colonists."));
            return;
        }
        ui.label(dim(format!("Ships are built from the stockpile ({:.0} t) with colony labour, and join the fleet when launched.", col.materials)));
        ui.label(dim("An ark built here takes 500 of this colony's people as settlers, with a share of the royal line if it lives here (never your heirs)."));
        let designs: Vec<usize> = (0..STARTERS).chain(col.blueprints.iter().copied()).collect();
        for idx in designs {
            let s = &SHIPS[idx];
            let p = Project::Ship(idx);
            let (t, ly) = p.cost();
            card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(s.name).strong());
                    ui.label(dim(format!("{t:.0} t · {ly:.0} labour-years")));
                });
                ui.label(dim(ship_line(s)));
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
        section(ui, "BLUEPRINTS FOR SALE");
        let mut any = false;
        for (idx, s) in SHIPS.iter().enumerate().skip(STARTERS) {
            if col.blueprints.contains(&idx) {
                continue;
            }
            any = true;
            let price = s.blueprint_price() * sim.background.buy_mult();
            card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(s.name).strong());
                ui.label(dim(s.blurb));
                ui.label(dim(ship_line(s)));
                if ui.add_enabled(sim.credits >= price, egui::Button::new(format!("Buy blueprints ({price:.0} cr)"))).clicked() {
                    acts.push(Action::BuyBlueprint(idx));
                }
            });
        }
        if !any {
            ui.label(dim("You own every design on the market."));
        }
    });
}

fn stat(ui: &mut Ui, k: &str, frac: f64, label: String, tip: &str) {
    ui.label(dim(k));
    bar(ui, frac, label, 230.0).on_hover_text(tip);
    ui.end_row();
}

fn overview(ui: &mut Ui, app: &App, sim: &Sim, col: &PlayerColony, acts: &mut Vec<Action>) {
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
    section(ui, "MATERIALS");
    egui::Grid::new("col_mat").num_columns(2).spacing([14.0, 3.0]).show(ui, |ui| {
        let i = &col.ind;
        ui.label(dim("Stockpile"));
        ui.label(RichText::new(format!("{:.0} t", col.materials)).strong().color(KEY));
        ui.end_row();
        ui.label(dim("Mined and hauled"))
            .on_hover_text("Colonists dig by hand; Resource Drones dig far more. A rich world and higher tech yield more.");
        ui.label(rate_text(i.materials_made_y));
        ui.end_row();
        ui.label(dim("Upkeep")).on_hover_text("Everyday wear and use, per colonist. Short recyclers make it worse; Rationing makes it lighter.");
        ui.label(rate_text(-i.materials_upkeep_y));
        ui.end_row();
        ui.label(dim("Construction")).on_hover_text("What the project being built draws (Build tab).");
        ui.label(rate_text(-i.materials_build_y));
        ui.end_row();
        if col.pmp > 0 {
            ui.label(dim("Manufacturing plant")).on_hover_text("Supplies the plant made from raw materials last year.");
            ui.label(rate_text(-col.plant_spent_t));
            ui.end_row();
        }
        ui.label(RichText::new("Net").strong());
        ui.label(rate_text(col.materials_trend_y()).strong());
        ui.end_row();
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
    ui.add_space(6.0);
    super::panels::retire_button(ui, app, acts);
}

fn laws(ui: &mut Ui, app: &App, sim: &Sim, col: &PlayerColony, acts: &mut Vec<Action>) {
    use crate::sim::colony::law::{self, Law};
    let cost = Law::cost_t(col.population());
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("Enacting costs {cost:.0} t")).strong());
        ui.label(dim("· stockpile"));
        ui.label(RichText::new(format!("{:.0} t", col.materials)).strong().color(if col.materials >= cost { GOOD } else { BAD }));
    });
    ui.label(dim("Laws trade one thing for another. A law stands for at least a year before it can be repealed. Repealing is free."));
    ui.add_space(4.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for l in Law::ALL {
            let enacted = col.laws.iter().find(|e| e.law == l);
            // Hegemony rules from the capital only (still shown where it
            // was enacted, so it can be repealed).
            if l == Law::Hegemony && col.outpost && enacted.is_none() {
                continue;
            }
            let blocked = l.conflicts().filter(|x| col.laws.iter().any(|e| e.law == *x));
            card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(l.name()).strong().color(if enacted.is_some() { GOOD } else { TEXT }));
                    if enacted.is_some() {
                        ui.label(RichText::new("IN FORCE").small().color(GOOD));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match enacted {
                        Some(e) => {
                            let left = law::MIN_TERM_Y - (col.clock - e.since);
                            let grave = l == Law::Hegemony && !col.outpost;
                            let label = if grave && app.confirm_repeal {
                                RichText::new("Confirm repeal").color(BAD).strong()
                            } else {
                                RichText::new("Repeal")
                            };
                            let b = ui.add_enabled(left <= 0.0, egui::Button::new(label));
                            if b.on_disabled_hover_text(format!("Can be repealed in {:.0} months.", (left * 12.0).ceil())).clicked() {
                                acts.push(Action::RepealLaw(l));
                            }
                        }
                        None => {
                            let ok = blocked.is_none() && col.materials >= cost;
                            let why = match blocked {
                                Some(x) => format!("Cannot stand alongside {}.", x.name()),
                                None => format!("Needs {cost:.0} t of materials."),
                            };
                            if ui.add_enabled(ok, egui::Button::new(format!("Enact ({cost:.0} t)"))).on_disabled_hover_text(why).clicked() {
                                acts.push(Action::EnactLaw(l));
                            }
                        }
                    });
                });
                ui.label(dim(l.blurb()));
                ui.label(RichText::new(format!("+ {}", l.pros())).color(GOOD));
                ui.label(RichText::new(format!("− {}", l.cons())).color(BAD));
                if l == Law::Hegemony && enacted.is_some() && !col.outpost {
                    hegemony_repeal_warning(ui, sim, col);
                }
            });
            ui.add_space(3.0);
        }
    });
}

/// What repealing Hegemony at the capital would cost, spelled out.
fn hegemony_repeal_warning(ui: &mut Ui, sim: &Sim, col: &PlayerColony) {
    let freed: Vec<&str> = sim.outposts.iter().map(|o| o.name.as_str()).collect();
    let cast_out = col.population() >= WIN_POP;
    ui.add_space(4.0);
    ui.label(RichText::new("⚠ REPEALING HEGEMONY CANNOT BE UNDONE").color(BAD).strong().size(15.0));
    if freed.is_empty() {
        ui.label(RichText::new("You have no other colonies to lose, but you can found no more.").color(WARN));
    } else {
        ui.label(RichText::new(format!("Every other colony declares independence and is lost to you: {}.", freed.join(", "))).color(BAD));
    }
    if cast_out {
        let fleet = if sim.fleet.is_empty() { String::new() } else { format!(" The {} ship(s) laid up here stay behind.", sim.fleet.len()) };
        ui.label(
            RichText::new(format!(
                "{} is past a million: it no longer needs a sovereign. You are cast out, and it goes on as an independent colony. You fly on with the {} as a roaming captain, without a colony.{fleet}",
                col.name,
                sim.ship().name
            ))
            .color(BAD)
            .strong(),
        );
        if sim.ship_dormant {
            ui.label(RichText::new("You have no ship to leave in: Hegemony cannot be repealed.").color(WARN));
        }
    } else {
        ui.label(dim(format!("You keep {} (a million people would send you away).", col.name)));
    }
}

fn warnings(col: &PlayerColony) -> Vec<String> {
    let mut w = Vec::new();
    for (g, c) in &col.ind.coverage {
        if *g == Good::HydroKit && *c < 0.95 && col.ind.forage >= 0.95 {
            w.push(format!("Hydroponics: only {:.0}% covered. The rest live off the land: hard, slow labour.", c * 100.0));
            continue;
        }
        let c = if *g == Good::HydroKit { c.max(col.ind.forage) } else { *c };
        if c < 0.95 {
            w.push(format!("{}: only {:.0}% covered.", g.name(), c * 100.0));
        }
    }
    if col.hab.needs_domes() && col.ind.sheltered < 0.999 {
        w.push(format!("{:.0}% of colonists have no shelter.", (1.0 - col.ind.sheltered) * 100.0));
    }
    if col.ind.shortage {
        w.push("Raw materials have run out: construction and the economy stall.".into());
    }
    if col.hybrid && col.gene_kits == 0 && col.census.get(col.player).is_some_and(|p| p.spouse.is_some() && !p.licensed) {
        w.push("A Genetic Modification Kit is needed before you and your native partner can have children.".into());
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
            let need = col.units_for(g, pop);
            ui.label(RichText::new(g.name()).color(ACCENT)).on_hover_text(g.desc());
            ui.label(format!("{have} / {need}"));
            let cov = col.ind.coverage.get(&g).copied().unwrap_or(0.0);
            bar(ui, cov, format!("{:.0}%", cov * 100.0), 110.0);
            let next = col.supplies.get(&g).and_then(|v| v.iter().map(|b| b.until).min_by(|a, b| a.total_cmp(b)));
            ui.label(dim(next.map(|y| format!("in {:.0} y", (y - now).max(0.0))).unwrap_or_else(|| "—".into())));
            ui.end_row();
        }
        // Genetic Modification Kits: a stock, not a coverage.
        let couples = col.census.living.iter().filter(|&&i| {
            let p = &col.census.people[i as usize];
            p.female && p.licensed
        }).count();
        ui.label(RichText::new(Good::GeneKit.name()).color(ACCENT)).on_hover_text(
            "Each kit lets one human and one native have children together; their children are Hybrids. Kits are used as soon as there is a single native and a single human to marry (checked on delivery and every year), or a married human and native without one.",
        );
        ui.label(format!("{} in store", col.gene_kits));
        ui.label(dim(format!("{couples} licensed couple{}", if couples == 1 { "" } else { "s" })));
        ui.label(dim("—"));
        ui.end_row();
    });
    if col.ind.forage >= 0.01 {
        ui.label(dim(format!("Without hydroponics, hunting, gathering and farming can feed {:.0}% of the colony, at the cost of hard labour.", col.ind.forage * 100.0)));
    }
    if col.hab.needs_domes() {
        ui.label(dim(format!("Shelter: {} Bio-Domes (10,000 each) + temporary domes (25 each). {:.0}% sheltered.", col.biodomes, col.ind.sheltered * 100.0)));
    }
    if col.drone_lvl > 0 {
        ui.label(dim(format!("Resource Drones refitted: each unit serves {:.0} colonists (+{}%).", col.per(Good::ResDrones), 10 * col.drone_lvl)));
    }
    let boons = col.boons.lines();
    if !boons.is_empty() {
        ui.label(dim(format!("Gifts from artifact deliveries: {}.", boons.join(", "))));
    }
    ui.add_space(6.0);
    section(ui, "FROM THE SHIP");
    let docked = sim.sys_id == col.sys_id && sim.loc == col.loc && sim.trip.is_none() && sim.shuttle.is_none() && !sim.ship_dormant;
    let has = sim.hold.units.keys().any(|g| g.is_supply() || *g == Good::Slaves || *g == Good::GeneKit);
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
        if ui.button(if col.outpost { "The leader" } else { "Your family" }).clicked() {
            acts.push(Action::ViewPerson(col.player));
        }
    });
    let captives = c.living.iter().filter(|&&i| c.people[i as usize].origin == Origin::Captive).count();
    let (natives, hybrids) = (c.count_species(Species::Native), c.count_species(Species::Hybrid));
    egui::Grid::new("census_tot").num_columns(2).spacing([14.0, 2.0]).show(ui, |ui| {
        ui.label(dim("Colonists"));
        ui.label(fmt_pop(pop));
        ui.end_row();
        ui.label(dim("Individual records"));
        ui.label(format!("{} living · {} ever", fmt_pop(c.living.len() as f64), fmt_pop(c.people.len() as f64)));
        ui.end_row();
        // Captives are natives brought in as slaves; their children are
        // natives born free. One line, with the captives called out.
        if natives > 0 || captives > 0 {
            ui.label(dim("Natives")).on_hover_text("A native species. Captives are those bought as slaves: only they weigh on the colony's happiness and health. Their children are born free.");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label(format!("{natives} ({:.1}%)", natives as f64 / pop.max(1.0) * 100.0));
                if captives > 0 {
                    ui.label(", ");
                    let text = match (captives == natives, captives) {
                        (true, 1) => "a captive".to_string(),
                        (true, _) => "all captives".to_string(),
                        (false, 1) => "1 captive".to_string(),
                        (false, k) => format!("{k} captives"),
                    };
                    ui.label(RichText::new(text).color(WARN));
                }
            });
            ui.end_row();
        }
        if hybrids > 0 {
            ui.label(dim("Hybrids"));
            ui.label(format!("{hybrids} ({:.1}%)", hybrids as f64 / pop.max(1.0) * 100.0));
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
    let gone = c.emigrated.get(&id).copied();
    let status = match (p.died, gone) {
        (Some(d), _) => format!("†{:.0}", (d - p.born).floor()),
        (None, Some(_)) => "emigrated".to_string(),
        (None, None) => format!("{:.0}", p.age(now).floor()),
    };
    let me = if id == col.player { if col.outpost { " (leader)" } else { " (you)" } } else { "" };
    let text = RichText::new(format!("{sym} {}{me}  {status}", c.name(id))).color(if c.here(id) { TEXT } else { DIM });
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
    let life = match (p.died, c.emigrated.get(&id)) {
        (Some(d), _) => format!("born {born}, died {} aged {:.0}", fmt_date(d as f64 * crate::sim::people::SECS_PER_YEAR), (d - p.born).floor()),
        (None, Some(&left)) => format!("born {born}, left on a colony ark {} aged {:.0}", fmt_date(left as f64 * crate::sim::people::SECS_PER_YEAR), (left - p.born).floor()),
        (None, None) => format!("born {born}, age {:.0}", p.age(now).floor()),
    };
    ui.label(format!("{} · {life}", if p.female { "Female" } else { "Male" }));
    let origin = match p.origin {
        Origin::Founder => "One of the founders",
        Origin::Born => "Born in the colony",
        Origin::Captive => "A captive",
        Origin::Settler => "Colonist",
    };
    let licensed = if p.licensed { " · may have children with a native or human partner (Genetic Modification Kit)" } else { "" };
    ui.label(dim(format!("{origin} · {}{licensed}", p.species().name())));
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
    ui.label(dim(format!("Buys {} units per click (set on the Trade screen's Cargo tab). Stock comes back over the weeks, like any port.", app.cargo_qty)));
    if col.pmp > 0 {
        ui.label(dim(format!(
            "Your {} manufacturing {} the dock from the stockpile (1.5 t of materials per tonne made), and sell for less.",
            col.pmp,
            if col.pmp == 1 { "plant restocks" } else { "plants restock" }
        )));
    }
    // Docked at the colony shown (the GUI does not swap it in).
    let docked = !sim.ship_dormant && sim.trip.is_none() && sim.shuttle.is_none() && sim.sys_id == col.sys_id && sim.loc == col.loc;
    let to_ship = app.dock_to_ship;
    ui.horizontal(|ui| {
        ui.label("Deliver to:");
        if ui.selectable_label(!to_ship, "Colony").on_hover_text("Supplies go straight into the colony's stores.").clicked() {
            acts.push(Action::DockToShip(false));
        }
        let ship = sim.ship().name;
        let r = ui
            .add(egui::Button::selectable(to_ship, format!("{ship} hold")))
            .on_hover_text("Load supplies into the ship's cargo hold, to carry them to another world.");
        if r.clicked() {
            acts.push(Action::DockToShip(true));
        }
    });
    if to_ship && docked {
        ui.label(dim(format!("Hold: {:.0} / {:.0} t", sim.hold.mass_t(), sim.ship().cargo_t)));
    } else if to_ship {
        ui.label(
            RichText::new(format!(
                "The {ship} is not docked at {} ({}): nothing can be bought for its hold here. Fly to {} first, or choose Colony.",
                col.name,
                col.world,
                col.world,
                ship = sim.ship().name
            ))
            .color(WARN)
            .strong(),
        );
    }
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
    // With a colony already, only Hegemony lets you found more (the plan
    // explains why not, once the law is in force).
    if sim.colony.is_some() && !sim.hegemony() {
        return;
    }
    if sim.all_colonies().any(|c| c.sys_id == sim.sys_id && c.loc == sim.loc) {
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
            if sim.colony.is_some() {
                ui.label(dim("Under Hegemony: the ship's crew and passengers settle here for good. You, your partner and your pilot stay aboard and fly on."));
            } else {
                ui.label(dim("Your crew settle for good and the ship is mothballed. You can leave to fly for supplies. Only under the Hegemony law can you found more colonies."));
            }
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
            ui.add_space(6.0);
            let hegemony = col.laws.iter().any(|e| e.law == crate::sim::colony::law::Law::Hegemony);
            if ui
                .add_enabled(hegemony, egui::Button::new(RichText::new("Continue the Royal Lineage").size(16.0).color(KEY)).min_size(vec2(300.0, 34.0)))
                .on_hover_text("Keep your colony and play on: your house rules, generation after generation.")
                .on_disabled_hover_text("Only a colony under the Hegemony law can rule on past a million.")
                .clicked()
            {
                acts.push(Action::ContinueLineage);
            }
        });
    });
}
