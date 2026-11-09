//! In-system viewport: stars (binary pairs and companions), orbits, shaded
//! planets, gas-giant moon systems, asteroid belts and the ship.
//! Click selects, double-click shuttles there; drag pans, scroll zooms.

use super::paint;
use super::theme::*;
use super::View;
use crate::game::{Action, App, Screen};
use crate::sim::civ::CivKind;
use crate::sim::planet::{Host, Planet, PlanetClass};
use crate::sim::system::StarSystem;
use crate::sim::{fmt_hours, Loc};
use eframe::egui::{self, vec2, Align2, Color32, FontId, Pos2, RichText, Sense, Stroke, Ui, Vec2};

/// Which part of a binary system the view frames.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    A,
    B,
    Both,
}

/// Screen layout for one frame.
struct Layout {
    a: Pos2,
    b: Option<Pos2>,
    k: f32,
    star_r: f32,
    star_b_r: f32,
    years: f64,
    days: f64,
}

impl Layout {
    fn origin(&self, host: Host) -> Pos2 {
        match host {
            Host::B => self.b.unwrap_or(self.a),
            _ => self.a,
        }
    }

    fn orbit_r(&self, host: Host, au: f64) -> f32 {
        let inner = if host == Host::B { self.star_b_r } else { self.star_r } + 12.0;
        inner + self.k * (au as f32).sqrt()
    }

    fn planet_pos(&self, p: &Planet) -> Pos2 {
        let ang = p.phase + std::f64::consts::TAU * (self.years / p.period.max(1e-6));
        self.origin(p.host) + vec2(ang.cos() as f32, -ang.sin() as f32) * self.orbit_r(p.host, p.au)
    }

    fn moon_pos(&self, parent: Pos2, parent_r: f32, m: usize, moon: &Planet) -> Pos2 {
        let r = parent_r + 7.0 + m as f32 * 5.0;
        let ang = moon.phase + std::f64::consts::TAU * self.days / (1.5 + m as f64 * 1.3);
        parent + vec2(ang.cos() as f32, ang.sin() as f32) * r
    }
}

fn disc_r(p: &Planet) -> f32 {
    match p.class {
        PlanetClass::Terrestrial => 4.0 + 2.5 * p.radius as f32,
        PlanetClass::SuperEarth => 6.0 + 1.5 * p.radius as f32,
        PlanetClass::IceWorld => 3.0 + 3.0 * p.radius as f32,
        PlanetClass::IceGiant => 10.0 + 0.5 * p.radius as f32,
        PlanetClass::GasGiant => 13.0 + 0.4 * p.radius as f32,
        PlanetClass::AsteroidBelt => 0.0,
    }
    .clamp(2.5, 20.0)
}

fn moon_r(m: &Planet) -> f32 {
    (1.8 + 2.6 * m.radius as f32).clamp(1.8, 4.5)
}

fn layout(sys: &StarSystem, rect: egui::Rect, view: &View, sim_t: f64) -> Layout {
    let radius = (rect.width().min(rect.height()) / 2.0 - 30.0).max(40.0);
    let star_r = paint::star_px(&sys.star, 12.0).clamp(4.0, 30.0);
    let star_b_r = sys.companion.as_ref().map(|c| paint::star_px(&c.star, 12.0).clamp(4.0, 24.0)).unwrap_or(0.0);
    let max_a = sys
        .planets
        .iter()
        .filter(|p| p.host != Host::B)
        .map(|p| p.au)
        .fold(0.0f64, f64::max)
        .max(sys.star.habitable_au() * 1.5);
    let max_b = sys.planets.iter().filter(|p| p.host == Host::B).map(|p| p.au).fold(0.0f64, f64::max);
    let s_type = sys.companion.as_ref().filter(|c| !c.p_type);
    let sep = s_type.map(|c| c.separation_au).unwrap_or(0.0) as f32;
    let dir_b = vec2(0.8, -0.6);
    let fit = |extent_au: f64, inner: f32| ((radius - inner - 16.0) / (extent_au.max(1e-6) as f32).sqrt()).max(0.01);
    let (k, center_a) = match (view.sys_focus, s_type) {
        (Focus::B, Some(c)) => {
            let k = fit(max_b.max(c.star.habitable_au() * 1.5), star_b_r);
            (k, rect.center() - dir_b * (k * sep.sqrt() + star_r + star_b_r))
        }
        (Focus::Both, Some(_)) => {
            let k = (radius * 1.6) / (sep.sqrt() + (max_b as f32).sqrt() + (max_a as f32).sqrt() * 0.5).max(0.01);
            (k, rect.center() - dir_b * k * sep.sqrt() * 0.5)
        }
        _ => (fit(max_a, star_r), rect.center()),
    };
    let k = k * view.sys_zoom;
    let a = center_a + view.sys_pan;
    let b = s_type.map(|_| a + dir_b * (k * sep.sqrt() + star_r + star_b_r));
    Layout { a, b, k, star_r, star_b_r, years: sim_t / 86_400.0 / 365.25, days: sim_t / 86_400.0 }
}

pub fn draw(ui: &mut Ui, app: &App, view: &mut View, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let sys = app.gal.system_ref(sim.sys_id);
    if view.sys_last != sys.id {
        view.sys_last = sys.id;
        view.sys_zoom = 1.0;
        view.sys_pan = Vec2::ZERO;
        let at_b = matches!(sim.loc, Loc::Companion) || sys.site(sim.loc).is_some_and(|p| p.host == Host::B);
        view.sys_focus = if at_b { Focus::B } else { Focus::A };
    }
    let footer_h = 78.0;
    let size = vec2(ui.available_width(), (ui.available_height() - footer_h).max(120.0));
    let (resp, painter) = ui.allocate_painter(size, Sense::click_and_drag());
    let rect = resp.rect;
    let painter = painter.with_clip_rect(rect);
    let t = view.anim_t as f32;
    paint::starfield(&painter, rect, sys.id, 1.6, t);

    // ---- camera ----
    if resp.dragged() {
        view.sys_pan += resp.drag_delta();
    }
    if resp.hovered() {
        let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta().y, i.zoom_delta()));
        let f = pinch * (scroll * 0.0018).exp();
        if (f - 1.0).abs() > 1e-4 {
            let old = view.sys_zoom;
            view.sys_zoom = (view.sys_zoom * f).clamp(0.3, 12.0);
            if let Some(p) = resp.hover_pos() {
                // Zoom about the pointer.
                let k = view.sys_zoom / old;
                let anchor = rect.center() + view.sys_pan;
                view.sys_pan += (p - anchor) * (1.0 - k);
            }
        }
    }
    let lay = layout(sys, rect, view, sim.t);
    let show_moons_for = |i: usize| view.sys_zoom >= 2.2 || app.sel_planet == i || sim.loc.planet() == Some(i);

    // ---- orbits & belts ----
    for (i, p) in sys.planets.iter().enumerate() {
        let o = lay.origin(p.host);
        let r = lay.orbit_r(p.host, p.au);
        if p.class == PlanetClass::AsteroidBelt {
            paint::belt(&painter, o, r, p, t);
            if i == app.sel_planet {
                painter.circle_stroke(o, r, Stroke::new(1.0, KEY.gamma_multiply(0.6)));
            }
            continue;
        }
        let sel = i == app.sel_planet;
        let col = if sel { Color32::from_rgb(90, 110, 70) } else { Color32::from_rgb(38, 46, 62) };
        painter.circle_stroke(o, r, Stroke::new(if sel { 1.4 } else { 1.0 }, col));
    }
    // Habitable zones and stability limits.
    let hz_ring = |o: Pos2, host: Host, au: f64| {
        painter.circle_stroke(o, lay.orbit_r(host, au), Stroke::new(6.0, Color32::from_rgba_unmultiplied(60, 160, 90, 22)));
    };
    let limit_ring = |o: Pos2, r: f32| {
        painter.extend(egui::Shape::dashed_line(
            &circle_pts(o, r, 96),
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(220, 120, 120, 70)),
            4.0,
            6.0,
        ));
    };
    match &sys.companion {
        Some(c) if c.p_type => {
            hz_ring(lay.a, Host::AB, (sys.star.lum + c.star.lum).sqrt());
            limit_ring(lay.a, lay.orbit_r(Host::AB, c.circumbinary));
        }
        Some(c) => {
            hz_ring(lay.a, Host::A, sys.star.habitable_au());
            limit_ring(lay.a, lay.orbit_r(Host::A, c.limit_a));
            if let Some(b) = lay.b {
                hz_ring(b, Host::B, c.star.habitable_au());
            }
        }
        None => hz_ring(lay.a, Host::A, sys.star.habitable_au()),
    }

    // ---- stars ----
    let mut star_pos = lay.a;
    match &sys.companion {
        Some(cp) if cp.p_type => {
            // A close pair circling their common centre.
            let ang = t * 0.4;
            let d = (lay.star_r * 1.4).max(6.0);
            let q = cp.mass_ratio as f32;
            let off = vec2(ang.cos(), ang.sin()) * d;
            star_pos = lay.a - off * (q / (1.0 + q));
            let b = lay.a + off * (1.0 / (1.0 + q));
            paint::star_of(&painter, star_pos, lay.star_r * 0.8, &sys.star, t);
            paint::star_of(&painter, b, lay.star_r * 0.8 * (0.5 + 0.5 * q.sqrt()), &cp.star, t);
        }
        _ => paint::star_of(&painter, lay.a, lay.star_r, &sys.star, t),
    }
    if let (Some(cp), Some(b)) = (&sys.companion, lay.b) {
        paint::star_of(&painter, b, lay.star_b_r, &cp.star, t);
        painter.text(b + vec2(0.0, lay.star_b_r + 6.0), Align2::CENTER_TOP, &cp.star.name, FontId::proportional(12.0), c(cp.star.class.color()));
    }

    // ---- bodies ----
    let mut hits: Vec<(Loc, Pos2, f32)> = vec![(Loc::Star, lay.a, lay.star_r + 6.0)];
    if let Some(b) = lay.b {
        hits.push((Loc::Companion, b, lay.star_b_r + 6.0));
    }
    for (i, p) in sys.planets.iter().enumerate() {
        if p.class == PlanetClass::AsteroidBelt {
            continue;
        }
        let at = lay.planet_pos(p);
        let r = disc_r(p);
        let light = (lay.origin(p.host) - at).normalized();
        paint::planet(&painter, at, r, p, light, t * 0.05 + i as f32);
        hits.push((Loc::Planet(i), at, r + 6.0));
        let sel = i == app.sel_planet && app.sel_moon.is_none();
        if sel {
            painter.circle_stroke(at, r + 4.0, Stroke::new(1.5, KEY));
        }
        civ_marks(&painter, at, r, p);
        let short = p.name.rsplit(' ').next().unwrap_or(&p.name);
        painter.text(at + vec2(0.0, r + 6.0), Align2::CENTER_TOP, short, FontId::proportional(12.0), if sel { KEY } else { DIM });
        if show_moons_for(i) {
            for (m, moon) in p.satellites.iter().enumerate() {
                let mp = lay.moon_pos(at, r, m, moon);
                let mr = moon_r(moon);
                paint::planet(&painter, mp, mr, moon, light, t * 0.1);
                civ_marks(&painter, mp, mr, moon);
                hits.push((Loc::Moon(i, m), mp, mr + 3.0));
                if app.sel_planet == i && app.sel_moon == Some(m) {
                    painter.circle_stroke(mp, mr + 3.0, Stroke::new(1.2, KEY));
                }
            }
        } else if !p.satellites.is_empty() {
            painter.text(at + vec2(r + 3.0, r), Align2::LEFT_TOP, format!("{} moons", p.satellites.len()), FontId::proportional(10.0), DIM);
        }
    }

    // ---- ship ----
    let loc_at = |l: Loc| -> Option<(Pos2, f32)> {
        match l {
            Loc::Star => Some((star_pos, lay.star_r)),
            Loc::Companion => lay.b.map(|b| (b, lay.star_b_r)),
            Loc::Planet(i) => sys.planets.get(i).map(|p| {
                if p.class == PlanetClass::AsteroidBelt {
                    (lay.origin(p.host) + vec2(0.7, -0.7) * lay.orbit_r(p.host, p.au), 4.0)
                } else {
                    (lay.planet_pos(p), disc_r(p))
                }
            }),
            Loc::Moon(i, m) => sys.planets.get(i).and_then(|p| {
                let at = lay.planet_pos(p);
                p.satellites.get(m).map(|moon| (lay.moon_pos(at, disc_r(p), m, moon), moon_r(moon)))
            }),
        }
    };
    if let Some(leg) = &sim.shuttle {
        if let (Some((a, ar)), Some((b, br))) = (loc_at(sim.loc), loc_at(leg.to)) {
            let dir = (b - a).normalized();
            let (a, b) = (a + dir * (ar + 6.0), b - dir * (br + 6.0));
            let prog = (1.0 - leg.remaining_h / leg.total_h.max(1e-9)).clamp(0.0, 1.0) as f32;
            let at = a + (b - a) * prog;
            painter.extend(egui::Shape::dashed_line(&[at, b], Stroke::new(1.2, WARN), 6.0, 5.0));
            paint::ship(&painter, at, dir, 7.0, GOOD);
            painter.text(at + vec2(10.0, -10.0), Align2::LEFT_BOTTOM, format!("ETA {}", fmt_hours(leg.remaining_h)), FontId::proportional(12.0), WARN);
        }
    } else if let Some((p, r)) = loc_at(sim.loc) {
        let ang = t * 0.6;
        let off = vec2(ang.cos(), ang.sin()) * (r + 8.0);
        paint::ship(&painter, p + off, vec2(-ang.sin(), ang.cos()), 6.0, GOOD);
    }

    // ---- overlay text ----
    let civ = sys
        .civ
        .as_ref()
        .map(|cv| format!("{} — {}", cv.display(), cv.tier.name()))
        .unwrap_or_else(|| "No civilization".into());
    let tl = rect.left_top();
    painter.text(tl + vec2(12.0, 10.0), Align2::LEFT_TOP, format!("{}  ({})", sys.star.name, sys.star.catalog), FontId::proportional(18.0), c(sys.star.class.color()));
    let mut line2 = format!("{} {}  ·  {} worlds", sys.star.desc, sys.star.code, sys.planets.len());
    if let Some(cp) = &sys.companion {
        line2.push_str(&format!("  ·  {} binary with {} ({})", cp.layout.name().to_lowercase(), cp.star.desc.to_lowercase(), cp.star.code));
    }
    painter.text(tl + vec2(12.0, 34.0), Align2::LEFT_TOP, line2, FontId::proportional(13.0), DIM);
    painter.text(tl + vec2(12.0, 52.0), Align2::LEFT_TOP, civ, FontId::proportional(13.0), if sys.civ.is_some() { CIV } else { DIM });
    painter.text(
        rect.right_bottom() + vec2(-10.0, -8.0),
        Align2::RIGHT_BOTTOM,
        "click: select   double-click: shuttle   drag: pan   wheel: zoom   ◆ natives  ■ colony  ✕ salvage",
        FontId::proportional(11.5),
        DIM,
    );

    // ---- interaction ----
    let hovered: Option<Loc> = resp.hover_pos().and_then(|h| {
        // Smallest target under the pointer wins (moons over their giant).
        let mut best: Option<(f32, Loc)> = None;
        for (l, p, r) in &hits {
            if h.distance(*p) < *r && best.is_none_or(|(br, _)| *r < br) {
                best = Some((*r, *l));
            }
        }
        best.map(|(_, l)| l).or_else(|| {
            sys.planets.iter().enumerate().find_map(|(i, p)| {
                if p.class != PlanetClass::AsteroidBelt {
                    return None;
                }
                let d = h.distance(lay.origin(p.host));
                ((d - lay.orbit_r(p.host, p.au)).abs() < 8.0).then_some(Loc::Planet(i))
            })
        })
    });
    if let Some(l) = hovered {
        if let Some((at, r)) = loc_at(l) {
            painter.circle_stroke(at, r + 4.0, Stroke::new(1.0, ACCENT));
        }
        let resp = resp.clone().on_hover_ui_at_pointer(|ui| match l {
            Loc::Star | Loc::Companion => {
                let s = if l == Loc::Star { &sys.star } else { &sys.companion.as_ref().unwrap().star };
                ui.label(RichText::new(&s.name).strong());
                ui.label(format!("{} ({})", s.desc, s.code));
                ui.label(dim("double-click to shuttle here (fuel scoop)"));
            }
            _ => {
                if let Some(p) = sys.site(l) {
                    body_tooltip(ui, sys, p);
                }
            }
        });
        if resp.double_clicked() {
            acts.push(Action::ShuttleTo(l));
        } else if resp.clicked() && l.is_body() {
            acts.push(Action::SelectBody(l));
        }
    }

    footer(ui, app, view, acts);
}

fn circle_pts(c: Pos2, r: f32, n: usize) -> Vec<Pos2> {
    (0..=n)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / n as f32;
            c + vec2(a.cos(), a.sin()) * r
        })
        .collect()
}

pub const COLONY_COL: Color32 = Color32::from_rgb(240, 160, 70);

/// Little markers: natives, colony, salvage.
fn civ_marks(painter: &egui::Painter, at: Pos2, r: f32, p: &Planet) {
    let mut x = at.x + r + 3.0;
    let y = at.y - r - 2.0;
    if p.natives.as_ref().is_some_and(|n| n.species_count > 0) {
        painter.text(egui::pos2(x, y), Align2::LEFT_CENTER, "◆", FontId::proportional(10.0), CIV);
        x += 9.0;
    }
    if p.colony.as_ref().is_some_and(|cl| cl.tech.active()) {
        painter.text(egui::pos2(x, y), Align2::LEFT_CENTER, "■", FontId::proportional(10.0), COLONY_COL);
        x += 9.0;
    }
    if p.salvage.as_ref().is_some_and(|s| s.remaining >= 1.0) {
        painter.text(egui::pos2(x, y), Align2::LEFT_CENTER, "✕", FontId::proportional(10.0), DIM);
    }
}

pub fn body_tooltip(ui: &mut Ui, sys: &StarSystem, p: &Planet) {
    ui.label(RichText::new(&p.name).strong());
    let mut line = format!("{}  ·  {:.2} AU", p.kind_label(), p.au);
    if p.host == Host::B {
        line.push_str(&format!(" from {}", sys.host_star(p.host).name));
    }
    ui.label(line);
    if p.class != PlanetClass::AsteroidBelt {
        let mut s = format!("{} ({:.0} K)", p.temp_band.name(), p.temp);
        if let Some(a) = p.atmosphere {
            s.push_str(&format!("  ·  {} air", a.name().to_lowercase()));
        }
        if let Some(g) = p.geosphere {
            s.push_str(&format!("  ·  {}", g.name()));
        }
        ui.label(s);
    }
    if let Some(f) = p.gg_feature {
        ui.label(f.name());
    }
    if let Some(f) = p.belt_feature {
        ui.label(f.name());
    }
    if let Some(l) = p.life {
        ui.label(RichText::new(l.name()).color(GOOD));
    }
    if let Some(n) = &p.natives {
        ui.label(RichText::new(format!("{} — {}", n.species, n.tech.name())).color(CIV));
    }
    if let Some(cl) = &p.colony {
        ui.label(RichText::new(format!("{} — {}", cl.name, cl.tech.name())).color(COLONY_COL));
    }
    if p.salvage.as_ref().is_some_and(|s| s.remaining >= 1.0) {
        ui.label(RichText::new("salvage on site").color(WARN));
    }
    if p.corrosion() > 0.0 {
        ui.label(RichText::new("corrosive: damages the hull while docked").color(BAD));
    }
    for d in p.deposits.iter().filter(|d| d.tons >= 1.0).take(4) {
        ui.label(RichText::new(format!("{}  {:.0} kt", d.res.name(), d.tons / 1000.0)).color(c(d.res.color())));
    }
    if p.scoopable() {
        ui.label(RichText::new("scoopable atmosphere (H2 + N2)").color(ACCENT));
    }
    ui.label(dim("double-click to shuttle here"));
}

fn footer(ui: &mut Ui, app: &App, view: &mut View, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let sys = app.gal.system_ref(sim.sys_id);
    let sel = app.selected_body();
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button("◀").on_hover_text("Previous planet  [←]").clicked() {
            acts.push(Action::StepPlanet(-1));
        }
        if ui.button("▶").on_hover_text("Next planet  [→]").clicked() {
            acts.push(Action::StepPlanet(1));
        }
        if let Some(p) = sys.site(sel) {
            let at = sim.loc == sel;
            ui.label(RichText::new(&p.name).strong().color(KEY));
            let mut info = format!("{} · {:.2} AU", p.kind_label(), p.au);
            if p.class != PlanetClass::AsteroidBelt {
                info.push_str(&format!(" · {} · {:.2} g", p.temp_band.name(), p.gravity));
            }
            ui.label(info);
            let deps = p
                .deposits
                .iter()
                .filter(|d| d.tons >= 1.0)
                .take(3)
                .map(|d| format!("{} {:.0}kt", d.res.short(), d.tons / 1000.0))
                .collect::<Vec<_>>()
                .join("  ");
            ui.label(dim(if deps.is_empty() { "no solid deposits".to_string() } else { deps }));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Details  [P]").clicked() {
                    acts.push(Action::Open(Screen::Planet));
                }
                if at {
                    if ui.button("Return to star  [S]").clicked() {
                        acts.push(Action::ShuttleTo(Loc::Star));
                    }
                    ui.label(RichText::new("DOCKED").color(GOOD).strong());
                } else if ui.button("Shuttle here  [S]").clicked() {
                    acts.push(Action::ShuttleTo(sel));
                }
            });
        }
    });
    ui.horizontal(|ui| {
        if sys.companion_dockable() {
            ui.label(dim("View:"));
            for (f, label) in [(Focus::A, "Star A"), (Focus::B, "Star B"), (Focus::Both, "Both")] {
                if ui.selectable_label(view.sys_focus == f, label).clicked() {
                    view.sys_focus = f;
                    view.sys_zoom = 1.0;
                    view.sys_pan = Vec2::ZERO;
                }
            }
            if sim.loc != Loc::Companion && ui.small_button("Shuttle to star B").clicked() {
                acts.push(Action::ShuttleTo(Loc::Companion));
            }
            ui.separator();
        }
        if ((view.sys_zoom - 1.0).abs() > 0.01 || view.sys_pan != Vec2::ZERO) && ui.small_button("Re-center").clicked() {
            view.sys_zoom = 1.0;
            view.sys_pan = Vec2::ZERO;
        }
        ui.label(dim(match sim.loc {
            Loc::Star => "Docked at the star: the fuel scoop works here.",
            Loc::Companion => "Docked at the companion star: the fuel scoop works here.",
            _ => "Docked at a world: drones mine its deposits; giants can be scooped.",
        }));
        if sys.civ.as_ref().is_some_and(|cv| cv.kind == CivKind::Colony) {
            ui.label(RichText::new("■ colony in system").color(COLONY_COL));
        }
    });
}
