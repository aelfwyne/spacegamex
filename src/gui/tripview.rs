//! FTL transit viewport: a warp starfield rushing past, the destination
//! star swelling ahead, and journey telemetry.

use super::paint;
use super::theme::*;
use super::View;
use crate::game::{Action, App};
use crate::rngx::mix64;
use crate::sim::{fmt_hours, ShipSpec};
use eframe::egui::{self, vec2, Align2, Color32, FontId, RichText, Sense, Stroke, Ui};

pub fn draw(ui: &mut Ui, app: &App, view: &mut View, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let Some(trip) = sim.trip.as_ref() else { return };
    let spec = &sim.ship();
    let tele_h = 120.0;
    let size = vec2(ui.available_width(), (ui.available_height() - tele_h).max(120.0));
    let (resp, painter) = ui.allocate_painter(size, Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, 0.0, Color32::from_rgb(2, 3, 8));
    let center = rect.center();
    let progress = (1.0 - trip.remaining_h / trip.total_h.max(1e-9)).clamp(0.0, 1.0) as f32;
    let (thr_speed, _) = ShipSpec::throttle_factor(sim.ftl_throttle);

    // ---- warp streaks: stars on a tunnel, flying toward the viewer ----
    let t = view.warp_t as f32;
    let v = 0.35 * thr_speed as f32;
    let reach = rect.width().max(rect.height()) * 0.75;
    for i in 0..360u64 {
        let h = mix64(trip.dest_id ^ i.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        let ang = (h & 0xffff) as f32 / 65535.0 * std::f32::consts::TAU;
        let rad = 0.15 + ((h >> 16) & 0xffff) as f32 / 65535.0 * 0.85;
        let z0 = ((h >> 32) & 0xffff) as f32 / 65535.0;
        let z = 1.0 - (z0 + t * v).fract(); // 1 = far, 0 = passing
        let z = z.max(0.02);
        let tail = (z + 0.06 * thr_speed as f32).min(1.2);
        let dir = vec2(ang.cos(), ang.sin());
        let p1 = center + dir * (rad * reach * 0.12 / z);
        let p0 = center + dir * (rad * reach * 0.12 / tail);
        if !rect.expand(40.0).contains(p1) {
            continue;
        }
        let a = ((1.0 - z) * 1.4).clamp(0.05, 1.0);
        let tint = if h >> 60 & 1 == 0 { Color32::from_rgb(170, 200, 255) } else { Color32::from_rgb(235, 235, 255) };
        painter.line_segment([p0, p1], Stroke::new(0.6 + 1.6 * (1.0 - z), paint::with_alpha(tint, a)));
    }

    // ---- destination star, growing as we close in ----
    let dest_rgb = app
        .gal
        .cache
        .get(&trip.dest_id)
        .map(|s| s.star.class.color())
        .unwrap_or(crate::sim::Rgb(255, 240, 200));
    let r = 2.0 + 22.0 * progress.powf(2.5);
    paint::glow(&painter, center, r, r * 6.0 + 10.0, c(dest_rgb), 0.35);
    paint::star(&painter, center, r, dest_rgb, t);

    // ---- overlay ----
    painter.text(rect.left_top() + vec2(12.0, 10.0), Align2::LEFT_TOP, format!("FTL TRANSIT → {}", trip.dest_name), FontId::proportional(18.0), CIV);
    if sim.ff == 0 {
        painter.text(rect.center_top() + vec2(0.0, 40.0), Align2::CENTER_TOP, "PAUSED", FontId::proportional(28.0), BAD);
    }
    // Journey bar.
    let bar_rect = egui::Rect::from_min_max(rect.left_bottom() + vec2(60.0, -28.0), rect.right_bottom() + vec2(-60.0, -18.0));
    painter.rect_filled(bar_rect, 4.0, Color32::from_rgb(25, 30, 45));
    let mut done = bar_rect;
    done.set_right(bar_rect.left() + bar_rect.width() * progress);
    painter.rect_filled(done, 4.0, CIV.gamma_multiply(0.8));
    painter.text(bar_rect.left_center() + vec2(-8.0, 0.0), Align2::RIGHT_CENTER, "origin", FontId::proportional(12.0), DIM);
    painter.text(bar_rect.right_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, "dest", FontId::proportional(12.0), DIM);
    paint::ship(&painter, egui::pos2(done.right(), bar_rect.center().y), vec2(1.0, 0.0), 8.0, GOOD);

    // ---- telemetry ----
    let (_, tf_burn) = ShipSpec::throttle_factor(sim.ftl_throttle);
    let burn = spec.ftl_burn * tf_burn * (1.0 + sim.ftl_wear / 200.0);
    let h2_h = sim.inv.get(crate::sim::Res::H2) / burn.max(1e-9);
    ui.add_space(6.0);
    egui::Grid::new("tele").num_columns(4).spacing([24.0, 4.0]).show(ui, |ui| {
        ui.label(dim("Progress"));
        ui.label(format!("{:.1} / {:.1} ly  ({:.0}%)", trip.dist_ly * progress as f64, trip.dist_ly, progress * 100.0));
        ui.label(dim("ETA"));
        ui.label(if sim.ff == 0 {
            format!("{} game time (paused)", fmt_hours(trip.remaining_h))
        } else {
            format!("{} game  (~{} real at x{})", fmt_hours(trip.remaining_h), fmt_hours(trip.remaining_h / sim.ff as f64), sim.ff)
        });
        ui.end_row();
        ui.label(dim("FTL burn"));
        ui.label(format!("{burn:.0} kg/h H2 + {:.0} MW   throttle {}%", spec.ftl_mw, sim.ftl_throttle));
        ui.label(dim("H2 lasts"));
        let ok = h2_h > trip.remaining_h;
        ui.label(RichText::new(fmt_hours(h2_h)).color(if ok { GOOD } else { BAD }))
            .on_hover_text("Run dry mid-jump and the drive collapses: you are flung back, hull badly damaged.");
        ui.end_row();
        ui.label(dim("Hull"));
        ui.label(RichText::new(format!("{:.0}/{:.0}  (dust {:.2}/h)", sim.hull, spec.hull, trip.dust_rate)).color(frac_color(sim.hull / spec.hull)));
        ui.label(dim("Crew"));
        ui.label(format!(
            "health {:.0} · morale {:.0}{}",
            sim.crew.health,
            sim.crew.morale,
            if sim.pax.count > 0 { format!(" · {} pax (health {:.0})", sim.pax.count, sim.pax.health) } else { String::new() }
        ));
        ui.end_row();
    });
    ui.horizontal(|ui| {
        if ui.button(RichText::new("Skip to arrival  [X]").strong()).clicked() {
            acts.push(Action::SkipToArrival);
        }
        ui.label(dim("The fab keeps running off the raw tanks in flight. Nothing can be scooped or mined."));
    });
}
