//! Interstellar map: a pannable, zoomable chart of every system within
//! 30 ly, plus a sortable list. Click selects, double-click sets the
//! destination. The galaxy is infinite; this is just the local chart.

use super::paint;
use super::theme::*;
use super::View;
use crate::game::{Action, App};
use crate::sim::civ::CivTier;
use crate::sim::galaxy::Neighbor;
use crate::sim::{fmt_hours, ShipSpec, Sim};
use eframe::egui::{self, vec2, Align2, Color32, FontId, Pos2, RichText, Sense, Stroke, Ui, Vec2};

const RADIUS: f32 = 30.0;

pub fn draw(ui: &mut Ui, app: &App, view: &mut View, acts: &mut Vec<Action>) {
    egui::Panel::right("map_list")
        .resizable(true)
        .default_size(440.0)
        .min_size(340.0)
        .show(ui, |ui| side(ui, app, view, acts));
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(BG_DEEP))
        .show(ui, |ui| chart(ui, app, view, acts));
}

fn civ_color(t: Option<CivTier>) -> Option<Color32> {
    match t {
        Some(CivTier::T3) => Some(CIV),
        Some(CivTier::T2) => Some(ACCENT),
        Some(CivTier::T1) => Some(DIM),
        None => None,
    }
}

fn chart(ui: &mut Ui, app: &App, view: &mut View, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let here = app.gal.system_ref(sim.sys_id).pos;
    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let rect = resp.rect;
    paint::starfield(&painter, rect, 0xa11ce ^ sim.sys_id, 0.7, view.anim_t as f32);

    let fit = (rect.width().min(rect.height()) / 2.0 - 30.0).max(20.0) / RADIUS;
    if view.map_zoom <= 0.0 {
        view.map_zoom = fit;
        view.map_pan = Vec2::ZERO;
    }
    // Pan (drag) and zoom (wheel / pinch) about the pointer.
    if resp.dragged() {
        let d = resp.drag_delta();
        view.map_pan -= vec2(d.x, -d.y) / view.map_zoom;
    }
    if resp.hovered() {
        let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta().y, i.zoom_delta()));
        let factor = pinch * (scroll * 0.0018).exp();
        if (factor - 1.0).abs() > 1e-4
            && let Some(p) = resp.hover_pos() {
                let before = to_world(p, rect.center(), view);
                view.map_zoom = (view.map_zoom * factor).clamp(fit * 0.35, fit * 8.0);
                let after = to_world(p, rect.center(), view);
                view.map_pan += before - after;
            }
    }
    let to_screen = |ly: [f64; 2]| -> Pos2 {
        let d = vec2((ly[0] - here[0]) as f32, (ly[1] - here[1]) as f32) - view.map_pan;
        rect.center() + vec2(d.x, -d.y) * view.map_zoom
    };
    let painter = painter.with_clip_rect(rect);

    // Grid: the galaxy's 10-ly cells.
    let cell = crate::sim::galaxy::CELL as f32;
    let tl = to_world(rect.left_top(), rect.center(), view) + vec2(here[0] as f32, here[1] as f32);
    let br = to_world(rect.right_bottom(), rect.center(), view) + vec2(here[0] as f32, here[1] as f32);
    let grid = Stroke::new(1.0, Color32::from_rgba_unmultiplied(70, 90, 130, 30));
    let mut x = (tl.x / cell).floor() * cell;
    while x <= br.x {
        let sx = to_screen([x as f64, 0.0]).x;
        painter.line_segment([egui::pos2(sx, rect.top()), egui::pos2(sx, rect.bottom())], grid);
        x += cell;
    }
    let mut y = (br.y / cell).floor() * cell;
    while y <= tl.y {
        let sy = to_screen([0.0, y as f64]).y;
        painter.line_segment([egui::pos2(rect.left(), sy), egui::pos2(rect.right(), sy)], grid);
        y += cell;
    }
    // Chart range.
    let c0 = to_screen(here);
    painter.circle_stroke(c0, RADIUS * view.map_zoom, Stroke::new(1.0, Color32::from_rgba_unmultiplied(90, 200, 235, 50)));
    painter.text(c0 + vec2(0.0, -RADIUS * view.map_zoom - 4.0), Align2::CENTER_BOTTOM, "30 ly", FontId::proportional(11.0), DIM);

    // Destination line.
    if let Some(did) = sim.destination {
        let dp = crate::sim::Galaxy::pos_of(did);
        let dsp = to_screen(dp);
        painter.extend(egui::Shape::dashed_line(&[c0, dsp], Stroke::new(1.5, ACCENT), 8.0, 6.0));
        let dist = ((dp[0] - here[0]).powi(2) + (dp[1] - here[1]).powi(2)).sqrt();
        painter.text(c0 + (dsp - c0) * 0.5 + vec2(6.0, -6.0), Align2::LEFT_BOTTOM, format!("{dist:.1} ly"), FontId::proportional(12.0), ACCENT);
    }
    // In-flight: the ship between the stars.
    if let Some(trip) = &sim.trip {
        let a = c0;
        let b = to_screen(trip.dest_pos);
        let p = 1.0 - (trip.remaining_h / trip.total_h.max(1e-9)) as f32;
        paint::ship(&painter, a + (b - a) * p, b - a, 8.0, GOOD);
    }

    // Systems.
    let hover = resp.hover_pos();
    let mut hovered: Option<usize> = None;
    let mut hover_d = f32::MAX;
    let label_all = view.map_zoom > fit * 1.6;
    for (i, n) in app.map_list.iter().enumerate() {
        let p = to_screen(n.pos);
        if !rect.expand(20.0).contains(p) {
            continue;
        }
        let base = match n.class.glyph() {
            'O' | 'B' => 5.5,
            'A' | 'F' => 4.5,
            'G' => 4.0,
            'K' => 3.5,
            _ => 3.0,
        } * if n.giant { 1.6 } else { 1.0 };
        let col = c(n.class.color());
        paint::glow(&painter, p, base * 0.6, base * 3.5, col, 0.35);
        painter.circle_filled(p, base, col);
        if n.binary {
            // A companion star: a second, smaller dot alongside.
            painter.circle_filled(p + vec2(base + 2.5, -base * 0.6), (base * 0.55).max(1.6), col.gamma_multiply(0.85));
        }
        if let Some(cc) = civ_color(n.civ) {
            let cc = if n.civ_kind == Some(crate::sim::civ::CivKind::Colony) { super::sysview::COLONY_COL } else { cc };
            painter.circle_stroke(p, base + 4.0, Stroke::new(1.5, cc));
        }
        let visited = sim.visited.contains(&n.id);
        let sel = i == app.map_sel;
        let dest = sim.destination == Some(n.id);
        if sel {
            painter.circle_stroke(p, base + 8.0, Stroke::new(2.0, KEY));
        }
        if dest {
            painter.circle_stroke(p, base + 11.0, Stroke::new(1.5, ACCENT));
        }
        // Quest targets: a gold diamond.
        if sim.quests.iter().any(|q| q.target_sys() == n.id) {
            let d = base + 6.0;
            let pts = vec![p + vec2(0.0, -d), p + vec2(d, 0.0), p + vec2(0.0, d), p + vec2(-d, 0.0), p + vec2(0.0, -d)];
            painter.add(egui::Shape::line(pts, Stroke::new(1.5, Color32::from_rgb(255, 205, 90))));
        }
        if sel || dest || n.civ.is_some() || label_all {
            let col = if sel { KEY } else if visited { DIM } else { TEXT };
            painter.text(p + vec2(base + 6.0, -base - 2.0), Align2::LEFT_BOTTOM, &n.star_name, FontId::proportional(12.0), col);
        }
        if let Some(h) = hover {
            let d = h.distance(p);
            if d < base + 10.0 && d < hover_d {
                hover_d = d;
                hovered = Some(i);
            }
        }
    }
    // Here.
    paint::star(&painter, c0, 5.0, app.gal.system_ref(sim.sys_id).star.class.color(), 0.0);
    painter.circle_stroke(c0, 11.0, Stroke::new(2.0, GOOD));
    painter.text(c0 + vec2(0.0, 14.0), Align2::CENTER_TOP, "YOU", FontId::proportional(11.0), GOOD);

    // Legend.
    let lg = rect.left_bottom() + vec2(10.0, -10.0);
    painter.text(lg, Align2::LEFT_BOTTOM, "drag: pan   wheel: zoom   click: select   double-click: set destination", FontId::proportional(11.5), DIM);
    painter.text(lg + vec2(0.0, -16.0), Align2::LEFT_BOTTOM, "rings: magenta = interstellar natives   cyan = spacefaring natives   grey = pre-space   orange = colony   double dot = binary", FontId::proportional(11.5), DIM);

    if let Some(i) = hovered {
        let n = &app.map_list[i];
        let resp = resp.clone().on_hover_ui_at_pointer(|ui| neighbor_tooltip(ui, sim, n));
        if resp.double_clicked() {
            acts.push(Action::MapSelect(i));
            acts.push(Action::ToggleDestination(n.id));
        } else if resp.clicked() {
            acts.push(Action::MapSelect(i));
        }
    }
}

fn to_world(p: Pos2, center: Pos2, view: &View) -> Vec2 {
    let d = (p - center) / view.map_zoom;
    vec2(d.x, -d.y) + view.map_pan
}

fn neighbor_tooltip(ui: &mut Ui, sim: &Sim, n: &Neighbor) {
    ui.label(RichText::new(&n.star_name).strong().color(c(n.class.color())));
    neighbor_facts(ui, sim, n);
}

fn neighbor_facts(ui: &mut Ui, sim: &Sim, n: &Neighbor) {
    ui.label(format!("{} class{}  ·  {:.1} ly  ·  {} worlds", n.star_class, if n.giant { " giant" } else { "" }, n.dist_ly, n.n_planets));
    if let Some(t) = n.civ {
        ui.label(RichText::new(t.name()).color(civ_color(Some(t)).unwrap_or(TEXT)));
    }
    ui.label(dim(format!("richness {:.0}/100{}", n.richness, if sim.visited.contains(&n.id) { "  ·  visited" } else { "" })));
}

/// Flight estimates for a hop: (precharge h, flight h, H2 kg).
fn hop_cost(sim: &Sim, dist: f64) -> (f64, f64, f64) {
    let spec = sim.ship();
    let (sp, burn) = ShipSpec::throttle_factor(sim.ftl_throttle);
    let flight = dist / (spec.ftl_speed * sp);
    let pre = dist * spec.precharge_mwh_ly * (1.0 + sim.ftl_wear / 100.0) / spec.precharge_mw.max(1e-9);
    let h2 = spec.ftl_burn * burn * (1.0 + sim.ftl_wear / 200.0) * flight + spec.reactor_burn * (pre + flight);
    (pre, flight, h2)
}

fn side(ui: &mut Ui, app: &App, view: &mut View, acts: &mut Vec<Action>) {
    let sim = app.sim();
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        section(ui, "NEARBY SYSTEMS");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Re-center").clicked() {
                view.map_zoom = 0.0;
            }
        });
    });
    // FTL status.
    let charge = sim.ftl_charge;
    bar_colored(ui, charge / 100.0, format!("FTL charge {charge:.0}%"), ui.available_width(), if charge >= 100.0 { GOOD } else { ACCENT });

    // Selected system card.
    if let Some(n) = app.map_list.get(app.map_sel) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(&n.star_name).strong().size(17.0).color(c(n.class.color())));
                if sim.destination == Some(n.id) {
                    ui.label(RichText::new("DESTINATION").color(ACCENT).strong());
                }
            });
            neighbor_facts(ui, sim, n);
            let (pre, flight, h2) = hop_cost(sim, n.dist_ly);
            ui.label(format!("precharge ~{}   flight ~{}   H2 ~{:.0} kg", fmt_hours(pre), fmt_hours(flight), h2));
            ui.horizontal(|ui| {
                let is_dest = sim.destination == Some(n.id);
                let label = if is_dest { "Clear destination  [Enter]" } else { "Set destination  [Enter]" };
                if ui.add_enabled(sim.trip.is_none(), egui::Button::new(label)).clicked() {
                    acts.push(Action::ToggleDestination(n.id));
                }
                // The jump itself re-checks everything and explains a refusal.
                let ready = sim.trip.is_none() && sim.shuttle.is_none() && charge >= 100.0;
                let why = if sim.trip.is_some() {
                    "Already in FTL transit."
                } else if sim.shuttle.is_some() {
                    "Finish the in-system shuttle first."
                } else {
                    "Capacitors are still charging (charge is set by the destination's distance)."
                };
                let r = ui.add_enabled(ready, egui::Button::new(RichText::new("JUMP  [T]").strong().color(if ready { GOOD } else { DIM })));
                if r.on_disabled_hover_text(why).clicked() {
                    acts.push(Action::JumpTo(n.id));
                }
            });
        });
    }
    ui.add_space(4.0);

    let sel_changed = view.last_map_sel != app.map_sel;
    view.last_map_sel = app.map_sel;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("nbrs").num_columns(6).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
            for h in ["#", "System", "Cls", "ly", "Civ", "Worlds"] {
                ui.label(dim(h));
            }
            ui.end_row();
            if app.map_list.is_empty() {
                ui.label(dim("no systems charted within 30 ly"));
                ui.end_row();
            }
            for (i, n) in app.map_list.iter().enumerate() {
                let sel = i == app.map_sel;
                ui.label(dim(if i < 9 { format!("{}", i + 1) } else { String::new() }));
                let mut name = RichText::new(&n.star_name);
                if sim.destination == Some(n.id) {
                    name = name.color(ACCENT).strong();
                } else if sim.visited.contains(&n.id) {
                    name = name.color(DIM);
                }
                let r = ui.selectable_label(sel, name);
                if sel && sel_changed {
                    r.scroll_to_me(Some(egui::Align::Center));
                }
                if r.double_clicked() {
                    acts.push(Action::ToggleDestination(n.id));
                } else if r.clicked() {
                    acts.push(Action::MapSelect(i));
                }
                ui.label(RichText::new(format!("{}{}", n.star_class, if n.giant { "★" } else { "" })).color(c(n.class.color())));
                ui.label(format!("{:.1}", n.dist_ly));
                match n.civ {
                    Some(t) => ui.label(RichText::new(match t {
                        CivTier::T3 => "interstellar",
                        CivTier::T2 => "planetary",
                        CivTier::T1 => "primitive",
                    }).color(civ_color(Some(t)).unwrap_or(TEXT))),
                    None => ui.label(""),
                };
                ui.label(format!("{}", n.n_planets));
                ui.end_row();
            }
        });
    });
}
