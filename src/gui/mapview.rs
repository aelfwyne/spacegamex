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
/// The tagged navigation target (`Sim::waypoint`).
pub const TARGET_COL: Color32 = Color32::from_rgb(110, 235, 200);

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
    // Every colony you hold is marked where it is on the chart; the
    // selected one (Colony screen) also gets a pointer, or an arrow at the
    // chart's edge when it is off it (click that to pan there).
    let here_pos = app.gal.system_ref(sim.sys_id).pos;
    let sel_sys = colony_fix(app).map(|_| selected(app).map_or(0, |c| c.sys_id));
    for c in sim.all_colonies() {
        let pos = crate::sim::Galaxy::pos_of(c.sys_id);
        let cp = to_screen(pos);
        if c.sys_id == sim.sys_id || !rect.shrink(16.0).contains(cp) {
            continue;
        }
        let col = super::sysview::COLONY_COL;
        let dist = ((pos[0] - here_pos[0]).powi(2) + (pos[1] - here_pos[1]).powi(2)).sqrt();
        let d = 9.0;
        let pts = vec![cp + vec2(-d, d * 0.2), cp + vec2(0.0, -d), cp + vec2(d, d * 0.2), cp + vec2(-d, d * 0.2)];
        painter.add(egui::Shape::line(pts, Stroke::new(1.8, col)));
        painter.text(cp + vec2(0.0, 10.0), Align2::CENTER_TOP, format!("{} · {dist:.1} ly", c.name), FontId::proportional(11.0), col);
        if Some(c.sys_id) == sel_sys {
            let tip = cp + vec2(0.0, -13.0);
            painter.add(egui::Shape::convex_polygon(vec![tip, tip + vec2(-7.0, -11.0), tip + vec2(7.0, -11.0)], col, Stroke::NONE));
        }
    }
    // Edge arrows toward the selected colony and the tagged target when
    // they are off the chart (click one to pan there).
    let mut guides = vec![];
    if let Some((cpos, dist, name)) = colony_fix(app) {
        guides.push((cpos, format!("{name} · {dist:.1} ly"), super::sysview::COLONY_COL));
    }
    if let Some((tpos, dist, name, tid)) = waypoint_fix(app) {
        let tp = to_screen(tpos);
        if tid != sim.sys_id && rect.shrink(16.0).contains(tp) {
            // On the chart: a crosshair ring.
            painter.circle_stroke(tp, 15.0, Stroke::new(1.8, TARGET_COL));
            for d in [vec2(1.0, 0.0), vec2(-1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, -1.0)] {
                painter.line_segment([tp + d * 11.0, tp + d * 19.0], Stroke::new(1.8, TARGET_COL));
            }
            painter.text(tp + vec2(0.0, -21.0), Align2::CENTER_BOTTOM, format!("TARGET {name} · {dist:.1} ly"), FontId::proportional(11.0), TARGET_COL);
        }
        guides.push((tpos, format!("target {name} · {dist:.1} ly"), TARGET_COL));
    }
    let mut pan_to = None;
    for (gpos, label, col) in guides {
        let gp = to_screen(gpos);
        if rect.shrink(16.0).contains(gp) {
            continue;
        }
        let dir = (gp - rect.center()).normalized();
        // Where the ray from the centre leaves the (inset) chart.
        let half = rect.shrink(34.0).size() / 2.0;
        let t = (half.x / dir.x.abs().max(1e-6)).min(half.y / dir.y.abs().max(1e-6));
        let tip = rect.center() + dir * t;
        let side = vec2(-dir.y, dir.x);
        let pts = vec![tip + dir * 12.0, tip - dir * 6.0 + side * 9.0, tip - dir * 6.0 - side * 9.0];
        painter.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
        let label_at = tip - dir * 18.0;
        let align = Align2([
            if dir.x > 0.3 { egui::Align::Max } else if dir.x < -0.3 { egui::Align::Min } else { egui::Align::Center },
            if dir.y > 0.3 { egui::Align::Max } else if dir.y < -0.3 { egui::Align::Min } else { egui::Align::Center },
        ]);
        painter.text(label_at, align, label, FontId::proportional(12.0), col);
        let hit = egui::Rect::from_center_size(tip, vec2(36.0, 36.0));
        if resp.clicked() && resp.interact_pointer_pos().is_some_and(|p| hit.contains(p)) {
            pan_to = Some(vec2((gpos[0] - here[0]) as f32, (gpos[1] - here[1]) as f32));
        }
    }

    // Here.
    if let Some(p) = pan_to {
        view.map_pan = p;
    }
    paint::star(&painter, c0, 5.0, app.gal.system_ref(sim.sys_id).star.class.color(), 0.0);
    painter.circle_stroke(c0, 11.0, Stroke::new(2.0, GOOD));
    painter.text(c0 + vec2(0.0, 14.0), Align2::CENTER_TOP, "YOU", FontId::proportional(11.0), GOOD);

    // Legend.
    let lg = rect.left_bottom() + vec2(10.0, -10.0);
    painter.text(lg, Align2::LEFT_BOTTOM, "drag: pan   wheel: zoom   click: select   double-click: set destination   arrows: orange = selected colony, green = target (click to pan there)", FontId::proportional(11.5), DIM);
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

/// The colony selected on the Colony screen (the capital by default).
fn selected(app: &App) -> Option<&crate::sim::colony::PlayerColony> {
    let sim = app.sim();
    sim.colony_at(app.sel_colony.min(sim.outposts.len()))
}

/// The selected colony's position, distance from here (ly) and name, if
/// you have one.
fn colony_fix(app: &App) -> Option<([f64; 2], f64, String)> {
    let sim = app.sim();
    let c = selected(app)?;
    let here = app.gal.system_ref(sim.sys_id).pos;
    let pos = crate::sim::Galaxy::pos_of(c.sys_id);
    let dist = ((pos[0] - here[0]).powi(2) + (pos[1] - here[1]).powi(2)).sqrt();
    Some((pos, dist, c.name.clone()))
}

/// The tagged target's position, distance from here (ly), name and id.
fn waypoint_fix(app: &App) -> Option<([f64; 2], f64, String, u64)> {
    let sim = app.sim();
    let id = sim.waypoint?;
    let here = app.gal.system_ref(sim.sys_id).pos;
    let pos = crate::sim::Galaxy::pos_of(id);
    let dist = ((pos[0] - here[0]).powi(2) + (pos[1] - here[1]).powi(2)).sqrt();
    let name = app
        .visited_list
        .iter()
        .chain(app.map_list.iter())
        .find(|n| n.id == id)
        .map(|n| n.star_name.clone())
        .or_else(|| (id == sim.sys_id).then(|| app.gal.system_ref(id).star.name.clone()))
        .unwrap_or_else(|| "?".into());
    Some((pos, dist, name, id))
}

/// "Show" (pan there) and "Select" / "Best hop" buttons toward a far
/// system `id` at `pos`.
fn guide_buttons(ui: &mut Ui, app: &App, view: &mut View, acts: &mut Vec<Action>, what: &str, (id, pos): (u64, [f64; 2])) {
    let sim = app.sim();
    let here = app.gal.system_ref(sim.sys_id).pos;
    let dist = ((pos[0] - here[0]).powi(2) + (pos[1] - here[1]).powi(2)).sqrt();
    if ui.small_button("Show").on_hover_text(format!("Pan the chart to your {what}.")).clicked() {
        view.map_pan = vec2((pos[0] - here[0]) as f32, (pos[1] - here[1]) as f32);
    }
    if let Some(i) = app.map_list.iter().position(|n| n.id == id) {
        if ui.small_button("Select").clicked() {
            acts.push(Action::MapSelect(i));
        }
    } else if let Some((i, rest)) = app
        .map_list
        .iter()
        .enumerate()
        .map(|(i, n)| (i, ((n.pos[0] - pos[0]).powi(2) + (n.pos[1] - pos[1]).powi(2)).sqrt()))
        .filter(|&(_, d)| d < dist)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        && ui.small_button("Best hop").on_hover_text(format!("Select the charted system closest to the {what} ({rest:.1} ly from it).")).clicked()
    {
        acts.push(Action::MapSelect(i));
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
    let spec = &sim.ship();
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
        section(ui, if view.map_visited { "VISITED SYSTEMS" } else { "NEARBY SYSTEMS" });
        ui.selectable_value(&mut view.map_visited, false, format!("Nearby ({})", app.map_list.len()));
        ui.selectable_value(&mut view.map_visited, true, format!("Visited ({})", app.visited_list.len()))
            .on_hover_text("Every system you have been to, at any range. Tag one as your target and the chart points the way.");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Re-center").clicked() {
                view.map_zoom = 0.0;
            }
        });
    });
    // FTL status.
    let charge = sim.ftl_charge;
    bar_colored(ui, charge / 100.0, format!("FTL charge {charge:.0}%"), ui.available_width(), if charge >= 100.0 { GOOD } else { ACCENT });
    // The way home.
    if let Some((cpos, dist, name)) = colony_fix(app) {
        let col_sys = selected(app).map(|c| c.sys_id);
        ui.horizontal(|ui| {
            let col = super::sysview::COLONY_COL;
            let n = sim.colony_count();
            if n > 1 && ui.small_button("▶ Next").on_hover_text("Point the chart at your next colony.").clicked() {
                acts.push(Action::SelectColony((app.sel_colony.min(n - 1) + 1) % n));
            }
            if col_sys == Some(sim.sys_id) {
                ui.label(RichText::new(format!("Colony {name}: in this system")).color(col));
                return;
            }
            ui.label(RichText::new(format!("Colony {name}: {dist:.1} ly")).color(col));
            if let Some(id) = col_sys {
                guide_buttons(ui, app, view, acts, "colony", (id, cpos));
            }
        });
    }
    // The tagged target.
    if let Some((tpos, dist, name, tid)) = waypoint_fix(app) {
        ui.horizontal(|ui| {
            if ui.small_button("✕").on_hover_text("Clear the target.").clicked() {
                acts.push(Action::ToggleWaypoint(tid));
            }
            if tid == sim.sys_id {
                ui.label(RichText::new(format!("Target {name}: in this system")).color(TARGET_COL));
                return;
            }
            ui.label(RichText::new(format!("Target {name}: {dist:.1} ly")).color(TARGET_COL));
            guide_buttons(ui, app, view, acts, "target", (tid, tpos));
        });
    }

    // Selected system card.
    if let Some(n) = app.map_list.get(app.map_sel) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(&n.star_name).strong().size(17.0).color(c(n.class.color())));
                if sim.destination == Some(n.id) {
                    ui.label(RichText::new("DESTINATION").color(ACCENT).strong());
                }
                if sim.waypoint == Some(n.id) {
                    ui.label(RichText::new("TARGET").color(TARGET_COL).strong());
                }
            });
            neighbor_facts(ui, sim, n);
            if let Some((cpos, _, _)) = colony_fix(app)
                && selected(app).is_some_and(|c| c.sys_id != n.id)
            {
                let d = ((n.pos[0] - cpos[0]).powi(2) + (n.pos[1] - cpos[1]).powi(2)).sqrt();
                ui.label(RichText::new(format!("{d:.1} ly from your colony")).color(super::sysview::COLONY_COL));
            }
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
                let tagged = sim.waypoint == Some(n.id);
                if ui
                    .button(RichText::new(if tagged { "Untarget" } else { "Target" }).color(TARGET_COL))
                    .on_hover_text("Tag as your navigation target: the chart points the way to it from anywhere.")
                    .clicked()
                {
                    acts.push(Action::ToggleWaypoint(n.id));
                }
            });
        });
    }
    ui.add_space(4.0);

    if view.map_visited {
        visited_list(ui, app, view, acts);
        return;
    }
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
                civ_cell(ui, n.civ);
                ui.label(format!("{}", n.n_planets));
                ui.end_row();
            }
        });
    });
}

fn civ_cell(ui: &mut Ui, civ: Option<CivTier>) {
    match civ {
        Some(t) => ui.label(RichText::new(match t {
            CivTier::T3 => "interstellar",
            CivTier::T2 => "planetary",
            CivTier::T1 => "primitive",
        }).color(civ_color(Some(t)).unwrap_or(TEXT))),
        None => ui.label(""),
    };
}

/// Every system visited, at any range, nearest first. Click to select it
/// (or pan the chart to it, beyond 30 ly); ◎ or double-click tags it as
/// the target.
fn visited_list(ui: &mut Ui, app: &App, view: &mut View, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let here = app.gal.system_ref(sim.sys_id).pos;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("visited").num_columns(6).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
            for h in ["", "System", "Cls", "ly", "Civ", "Worlds"] {
                ui.label(dim(h));
            }
            ui.end_row();
            if app.visited_list.is_empty() {
                ui.label("");
                ui.label(dim("no other systems visited yet"));
                ui.end_row();
            }
            for n in &app.visited_list {
                let tagged = sim.waypoint == Some(n.id);
                let mark = RichText::new(if tagged { "◉" } else { "◎" }).color(if tagged { TARGET_COL } else { DIM });
                if ui
                    .add(egui::Button::new(mark).small().frame(false))
                    .on_hover_text(if tagged { "Clear the target." } else { "Tag as your target: the chart points the way." })
                    .clicked()
                {
                    acts.push(Action::ToggleWaypoint(n.id));
                }
                let charted = app.map_list.iter().position(|m| m.id == n.id);
                let sel = charted == Some(app.map_sel);
                let mut name = RichText::new(&n.star_name);
                if tagged {
                    name = name.color(TARGET_COL).strong();
                } else if sim.destination == Some(n.id) {
                    name = name.color(ACCENT).strong();
                }
                let r = ui.selectable_label(sel, name).on_hover_ui(|ui| neighbor_facts(ui, sim, n));
                if r.double_clicked() {
                    acts.push(Action::ToggleWaypoint(n.id));
                } else if r.clicked() {
                    view.map_pan = vec2((n.pos[0] - here[0]) as f32, (n.pos[1] - here[1]) as f32);
                    if let Some(i) = charted {
                        acts.push(Action::MapSelect(i));
                    }
                }
                ui.label(RichText::new(format!("{}{}", n.star_class, if n.giant { "★" } else { "" })).color(c(n.class.color())));
                ui.label(RichText::new(format!("{:.1}", n.dist_ly)).color(if charted.is_some() { TEXT } else { DIM }));
                civ_cell(ui, n.civ);
                ui.label(format!("{}", n.n_planets));
                ui.end_row();
            }
        });
    });
}
