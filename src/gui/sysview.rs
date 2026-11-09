//! In-system viewport: the star, orbits, shaded planets and the ship.
//! Click a world to select it, double-click it (or the star) to shuttle.

use super::paint;
use super::theme::*;
use super::View;
use crate::game::{Action, App, Screen};
use crate::sim::planet::PlanetClass;
use crate::sim::{fmt_hours, Loc};
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Pos2, RichText, Sense, Stroke, Ui};

pub fn draw(ui: &mut Ui, app: &App, view: &mut View, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let sys = app.gal.system_ref(sim.sys_id);
    let footer_h = 74.0;
    let size = vec2(ui.available_width(), (ui.available_height() - footer_h).max(120.0));
    let (resp, painter) = ui.allocate_painter(size, Sense::click());
    let rect = resp.rect;
    let t = view.anim_t as f32;
    paint::starfield(&painter, rect, sys.id, 1.6, t);

    // ---- geometry ----
    let center = rect.center();
    let radius = (rect.width().min(rect.height()) / 2.0 - 26.0).max(30.0);
    let star_r = ((7.0 + 5.0 * (1.0 + sys.star.radius as f32).ln()) * if sys.star.giant { 1.6 } else { 1.0 }).clamp(7.0, 30.0);
    let max_au = sys.planets.iter().map(|p| p.au).fold(0.1f64, f64::max) * 1.05;
    let inner = star_r + 16.0;
    let orbit_r = |au: f64| inner + (radius - inner) * (au / max_au).sqrt() as f32;
    let years = sim.t / 86_400.0 / 365.25;
    let pos_of = |i: usize| -> Pos2 {
        let p = &sys.planets[i];
        let ang = p.phase + std::f64::consts::TAU * (years / p.period.max(1e-6));
        let r = orbit_r(p.au);
        center + vec2(ang.cos() as f32, -ang.sin() as f32) * r
    };
    let disc_r = |i: usize| -> f32 {
        let p = &sys.planets[i];
        match p.class {
            PlanetClass::Terrestrial => 6.0 + 2.5 * p.radius as f32,
            PlanetClass::SuperEarth => 8.0 + 1.5 * p.radius as f32,
            PlanetClass::IceGiant => 12.0 + 0.5 * p.radius as f32,
            PlanetClass::GasGiant => 14.0 + 0.4 * p.radius as f32,
        }
        .min(22.0)
    };

    // ---- hover / pick ----
    let hover = resp.hover_pos();
    let pick = |p: Pos2| -> Option<Loc> {
        let mut best: Option<(f32, Loc)> = None;
        for i in 0..sys.planets.len() {
            let d = pos_of(i).distance(p);
            if d < disc_r(i) + 8.0 && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, Loc::Planet(i)));
            }
        }
        best.map(|(_, l)| l).or_else(|| (center.distance(p) < star_r + 8.0).then_some(Loc::Star))
    };
    let hovered = hover.and_then(pick);

    // ---- orbits ----
    for (i, p) in sys.planets.iter().enumerate() {
        let sel = i == app.sel_planet;
        let col = if sel { Color32::from_rgb(90, 110, 70) } else { Color32::from_rgb(38, 46, 62) };
        painter.circle_stroke(center, orbit_r(p.au), Stroke::new(if sel { 1.4 } else { 1.0 }, col));
    }
    // Habitable zone band.
    let hz = sys.star.habitable_au();
    if hz < max_au {
        painter.circle_stroke(center, orbit_r(hz), Stroke::new(6.0, Color32::from_rgba_unmultiplied(60, 160, 90, 22)));
    }

    // ---- star ----
    paint::star(&painter, center, star_r, sys.star.class.color(), t);

    // ---- planets ----
    for i in 0..sys.planets.len() {
        let p = &sys.planets[i];
        let at = pos_of(i);
        let r = disc_r(i);
        let light = (center - at).normalized();
        paint::planet(&painter, at, r, p, light, t * 0.05 + i as f32);
        let sel = i == app.sel_planet;
        if sel {
            painter.circle_stroke(at, r + 4.0, Stroke::new(1.5, KEY));
        } else if hovered == Some(Loc::Planet(i)) {
            painter.circle_stroke(at, r + 4.0, Stroke::new(1.0, ACCENT));
        }
        let short = p.name.rsplit(' ').next().unwrap_or(&p.name);
        painter.text(
            at + vec2(0.0, r + 6.0),
            Align2::CENTER_TOP,
            short,
            FontId::proportional(12.0),
            if sel { KEY } else { DIM },
        );
        if p.exhausted() && p.class.mineable() {
            painter.text(at + vec2(0.0, -r - 4.0), Align2::CENTER_BOTTOM, "mined out", FontId::proportional(10.0), DIM);
        }
    }

    // ---- ship ----
    let loc_pos = |l: Loc| -> (Pos2, f32) {
        match l {
            Loc::Star => (center, star_r),
            Loc::Planet(i) if i < sys.planets.len() => (pos_of(i), disc_r(i)),
            Loc::Planet(_) => (center, star_r),
        }
    };
    if let Some(leg) = &sim.shuttle {
        let (a, ar) = loc_pos(sim.loc);
        let (b, br) = loc_pos(leg.to);
        let dir = (b - a).normalized();
        let (a, b) = (a + dir * (ar + 6.0), b - dir * (br + 6.0));
        let prog = (1.0 - leg.remaining_h / leg.total_h.max(1e-9)).clamp(0.0, 1.0) as f32;
        let at = a + (b - a) * prog;
        painter.extend(egui::Shape::dashed_line(&[at, b], Stroke::new(1.2, WARN), 6.0, 5.0));
        paint::ship(&painter, at, dir, 7.0, GOOD);
        painter.text(at + vec2(10.0, -10.0), Align2::LEFT_BOTTOM, format!("ETA {}", fmt_hours(leg.remaining_h)), FontId::proportional(12.0), WARN);
    } else {
        let (p, r) = loc_pos(sim.loc);
        let ang = t * 0.6;
        let off = vec2(ang.cos(), ang.sin()) * (r + 9.0);
        paint::ship(&painter, p + off, vec2(-ang.sin(), ang.cos()), 6.0, GOOD);
    }

    // ---- overlay text ----
    let civ = sys.civ.as_ref().map(|c| format!("{} — {}", c.name, c.tier.name())).unwrap_or_else(|| "No civilization".into());
    painter.text(rect.left_top() + vec2(12.0, 10.0), Align2::LEFT_TOP, format!("{}  ({})", sys.star.name, sys.star.catalog), FontId::proportional(18.0), c(sys.star.class.color()));
    painter.text(rect.left_top() + vec2(12.0, 34.0), Align2::LEFT_TOP, format!("{}  ·  {} worlds", sys.star.class.class_name(), sys.planets.len()), FontId::proportional(13.0), DIM);
    painter.text(rect.left_top() + vec2(12.0, 52.0), Align2::LEFT_TOP, civ, FontId::proportional(13.0), if sys.civ.is_some() { CIV } else { DIM });
    painter.text(rect.right_bottom() + vec2(-10.0, -8.0), Align2::RIGHT_BOTTOM, "click: select   double-click: shuttle there   green band: habitable zone", FontId::proportional(11.5), DIM);

    // ---- interaction ----
    if let Some(l) = hovered {
        let resp = resp.clone().on_hover_ui_at_pointer(|ui| match l {
            Loc::Star => {
                ui.label(RichText::new(&sys.star.name).strong());
                ui.label(sys.star.class.class_name());
                ui.label(dim("double-click to shuttle to the star (fuel scoop)"));
            }
            Loc::Planet(i) => planet_tooltip(ui, app, i),
        });
        if resp.double_clicked() {
            acts.push(Action::ShuttleTo(l));
        } else if resp.clicked()
            && let Loc::Planet(i) = l {
                acts.push(Action::SelectPlanet(i));
            }
    }

    footer(ui, app, acts);
}

fn planet_tooltip(ui: &mut Ui, app: &App, i: usize) {
    let sim = app.sim();
    let p = &app.gal.system_ref(sim.sys_id).planets[i];
    ui.label(RichText::new(&p.name).strong());
    ui.label(format!("{}  ·  {:.2} AU  ·  {:.0} K", p.class.name(), p.au, p.temp));
    if p.class.scoopable() {
        ui.label(RichText::new("scoopable atmosphere (H2 + N2)").color(ACCENT));
    }
    for d in p.deposits.iter().filter(|d| d.tons >= 1.0).take(5) {
        ui.label(RichText::new(format!("{}  {:.0} kt", d.res.name(), d.tons / 1000.0)).color(c(d.res.color())));
    }
    ui.label(dim("double-click to shuttle here"));
}

fn footer(ui: &mut Ui, app: &App, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let sys = app.gal.system_ref(sim.sys_id);
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button("◀").on_hover_text("Previous planet  [←]").clicked() {
            acts.push(Action::StepPlanet(-1));
        }
        if ui.button("▶").on_hover_text("Next planet  [→]").clicked() {
            acts.push(Action::StepPlanet(1));
        }
        if let Some(p) = sys.planets.get(app.sel_planet) {
            let at = matches!(sim.loc, Loc::Planet(i) if i == app.sel_planet);
            ui.label(RichText::new(&p.name).strong().color(KEY));
            ui.label(format!("{} · {:.2} AU · {:.0} K · {:.2} g", p.class.name(), p.au, p.temp, p.gravity));
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
                    acts.push(Action::ShuttleTo(Loc::Planet(app.sel_planet)));
                }
            });
        }
    });
    ui.horizontal(|ui| {
        ui.label(dim(match sim.loc {
            Loc::Star => "Docked at the star: the fuel scoop works here.",
            Loc::Planet(_) => "Docked at a world: drones mine its deposits; giants can be scooped.",
        }));
    });
    let _ = pos2;
}
