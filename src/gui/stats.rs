//! Career statistics: everything counted, the extremes seen, and the
//! kinds of things found.

use super::colony::fmt_pop;
use super::panels::back_row;
use super::theme::*;
use crate::game::{Action, App};
use crate::sim::stats::{CATEGORIES, EXTREMES};
use crate::sim::{fmt_kg, Sim};
use eframe::egui::{self, RichText, Ui};

fn grid(ui: &mut Ui, id: &str, rows: Vec<(&str, String)>) {
    egui::Grid::new(id).num_columns(2).striped(true).spacing([24.0, 3.0]).show(ui, |ui| {
        for (k, v) in rows {
            ui.label(dim(k));
            ui.label(v);
            ui.end_row();
        }
    });
}

fn n(x: u32) -> String {
    fmt_pop(x as f64)
}

fn career(sim: &Sim) -> Vec<(&'static str, String)> {
    let card = sim.score();
    vec![
        ("Captain", sim.player_name().to_string()),
        ("Background", sim.background.name().to_string()),
        ("Ship", sim.ship().name.to_string()),
        ("Years in command", format!("{:.1}", sim.years_played())),
        ("Captain's age", format!("{:.0}", sim.player_age().floor())),
        ("Successions", n(sim.stats.successions)),
        ("Score so far", format!("{} (if you retired now)", fmt_pop(card.total as f64))),
    ]
}

pub fn page(ui: &mut Ui, app: &App, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let st = &sim.stats;
    back_row(ui, "STATISTICS", acts);
    ui.columns(2, |cols| {
        let ui = &mut cols[0];
        section(ui, "CAREER");
        grid(ui, "st_career", career(sim));
        ui.add_space(8.0);
        section(ui, "EXPLORATION");
        grid(ui, "st_explore", vec![
            ("Jumps", n(st.jumps)),
            ("Light-years flown", format!("{:.1}", st.dist_ly)),
            ("Systems visited", n(st.systems)),
            ("Farthest from home", format!("{:.1} ly", st.farthest_ly)),
            ("Landings (worlds and moons)", n(st.landings)),
        ]);
        ui.add_space(8.0);
        section(ui, "WORK");
        grid(ui, "st_work", vec![
            ("Fuel scooped", format!("{} kg", fmt_kg(st.scooped))),
            ("Ore mined", format!("{} kg", fmt_kg(st.mined))),
            ("Fabricated", format!("{} kg", fmt_kg(st.fabbed))),
            ("Salvage recovered", format!("{:.0} cr", st.salvage_cr)),
        ]);
        ui.add_space(8.0);
        section(ui, "TRADE");
        grid(ui, "st_trade", vec![
            ("Credits now", format!("{:.0} cr", sim.credits)),
            ("Credits earned", format!("{:.0} cr", st.credits_earned)),
            ("Credits spent", format!("{:.0} cr", st.credits_spent)),
            ("Trades", n(st.trades)),
            ("Cargo units bought / sold", format!("{} / {}", n(st.goods_bought), n(st.goods_sold))),
            ("Partners recruited", n(st.partners)),
            ("Pilots hired", n(st.pilots)),
        ]);
        ui.add_space(8.0);
        section(ui, "PIRATES AND JOBS");
        grid(ui, "st_combat", vec![
            ("Pirate encounters", n(st.pirates_met)),
            ("Fought / fled / paid off", format!("{} / {} / {}", n(st.pirates_fought), n(st.pirates_fled), n(st.pirates_paid))),
            ("Battles won / lost", format!("{} / {}", n(sim.battles_won), n(sim.battles_lost))),
            ("Hull damage taken", format!("{:.0}", st.hull_damage)),
            ("Jobs completed / dropped", format!("{} / {}", n(st.quests_done), n(st.quests_dropped))),
            ("Artifacts kept / delivered", format!("{} / {}", n(st.artifacts_kept), n(st.artifacts_delivered))),
            ("Relics found / sold", format!("{} / {}", n(st.relics_found), n(st.relics_sold))),
            ("Crew lost", n(st.crew_lost)),
            ("Passengers lost", n(st.pax_lost)),
        ]);
        ui.add_space(8.0);
        section(ui, "COLONY");
        grid(ui, "st_colony", vec![
            ("Colonies founded / abandoned", format!("{} / {}", n(st.colonies_founded), n(st.colonies_abandoned))),
            ("Population now", sim.colony.as_ref().map(|c| fmt_pop(c.population())).unwrap_or_else(|| "—".into())),
            ("Peak population", fmt_pop(st.colony_peak)),
            ("Births", fmt_pop(st.colony_births)),
            ("Deaths", fmt_pop(st.colony_deaths)),
            ("Buildings completed", n(st.buildings)),
            ("Laws enacted", n(st.laws_enacted)),
        ]);

        let ui = &mut cols[1];
        section(ui, "RECORDS");
        egui::Grid::new("st_ext").num_columns(3).striped(true).spacing([18.0, 3.0]).show(ui, |ui| {
            for (key, _, unit) in EXTREMES {
                ui.label(dim(key));
                match st.extremes.get(key) {
                    Some(e) => {
                        let v = match unit {
                            "g" => format!("{:.2} g", e.value),
                            "L☉" => format!("{:.3} L☉", e.value),
                            "" => format!("{:.0}", e.value),
                            u => format!("{} {u}", fmt_pop(e.value)),
                        };
                        ui.label(RichText::new(v).strong());
                        ui.label(&e.what);
                    }
                    None => {
                        ui.label("—");
                        ui.label("");
                    }
                }
                ui.end_row();
            }
        });
        ui.add_space(8.0);
        section(ui, "DISCOVERIES");
        ui.label(dim("Counted once per system visited, moons included."));
        for cat in CATEGORIES {
            ui.add_space(4.0);
            ui.label(RichText::new(cat).strong().color(ACCENT));
            match st.found.get(cat) {
                Some(kinds) if !kinds.is_empty() => {
                    let mut v: Vec<(&String, &u32)> = kinds.iter().collect();
                    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
                    egui::Grid::new(("st_found", cat)).num_columns(2).spacing([24.0, 1.0]).show(ui, |ui| {
                        for (k, c) in v {
                            ui.label(k.as_str());
                            ui.label(dim(c.to_string()));
                            ui.end_row();
                        }
                    });
                }
                _ => {
                    ui.label(dim("none yet"));
                }
            }
        }
    });
}
