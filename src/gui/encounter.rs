//! The pirate-encounter dialog: drawn over any in-game screen while pirates
//! are hailing (game time is stopped until it is answered).

use super::theme::*;
use crate::game::{Action, App};
use crate::sim::combat::{capitalize, Choice, Stage};
use crate::sim::opts::Risk;
use eframe::egui::{self, vec2, Align2, RichText};

pub fn draw(ctx: &egui::Context, app: &App, acts: &mut Vec<Action>) {
    let Some(sim) = app.sim.as_ref() else { return };
    if sim.over.is_some() {
        return;
    }
    let Some(e) = &sim.encounter else { return };
    egui::Window::new(RichText::new("PIRATES").strong().color(CRIT))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, -40.0))
        .min_width(420.0)
        .show(ctx, |ui| {
            let (a, d) = sim.combat_stats();
            egui::Grid::new("pirate_odds").num_columns(3).spacing([24.0, 4.0]).show(ui, |ui| {
                ui.label("");
                ui.label(dim("Attack"));
                ui.label(dim("Defense"));
                ui.end_row();
                ui.label(RichText::new("You").strong());
                ui.label(format!("{a:.1}"));
                ui.label(format!("{d:.1}"));
                ui.end_row();
                ui.label(RichText::new(capitalize(&e.band)).strong().color(BAD));
                ui.label(format!("{:.1}", e.attack));
                ui.label(format!("{:.1}", e.defense));
                ui.end_row();
            });
            ui.add_space(6.0);
            match e.stage {
                Stage::Approach => {
                    ui.label(format!("{} have locked on and demand your cargo.", capitalize(&e.band)));
                    let cap = match sim.opts.risk {
                        Risk::Tourist => "at most half your cargo, never fuel",
                        Risk::Normal => "up to three quarters of your cargo, never fuel",
                        Risk::Risky if sim.battles_lost >= 5 => "up to 95% of everything, and they might not leave witnesses",
                        Risk::Risky => "up to three quarters of everything, fuel included",
                    };
                    ui.label(dim(format!("Losing a fight costs {cap}. Life support is never taken.")));
                    ui.add_space(6.0);
                    let big = |s: String| egui::Button::new(RichText::new(s).strong()).min_size(vec2(400.0, 30.0));
                    if ui.add(big(format!("[1] Fight  ({:.0}% to win)", sim.win_chance() * 100.0))).clicked() {
                        acts.push(Action::Encounter(Choice::Fight));
                    }
                    if ui.add(big(format!("[2] Flee  ({:.0}% to escape; caught means a fight, damaged)", sim.flee_chance() * 100.0))).clicked() {
                        acts.push(Action::Encounter(Choice::Flee));
                    }
                    if ui.add(big(format!("[3] Pay tribute  ({:.0}% of cargo and credits)", sim.tribute_share() * 100.0))).clicked() {
                        acts.push(Action::Encounter(Choice::Pay));
                    }
                }
                Stage::Won => {
                    ui.label(RichText::new("You won. Their ship is crippled.").color(GOOD).strong());
                    ui.add_space(6.0);
                    let big = |s: String| egui::Button::new(RichText::new(s).strong()).min_size(vec2(400.0, 30.0));
                    if ui.add(big(format!("[1] Destroy it  ({:.0} cr bounty at the next port, extra salvage)", e.bounty))).clicked() {
                        acts.push(Action::Encounter(Choice::Destroy));
                    }
                    if ui.add(big("[2] Let them go".to_string())).clicked() {
                        acts.push(Action::Encounter(Choice::Release));
                    }
                }
            }
            ui.add_space(4.0);
            ui.label(dim(format!("Battles won {} · lost {}", sim.battles_won, sim.battles_lost)));
        });
}
