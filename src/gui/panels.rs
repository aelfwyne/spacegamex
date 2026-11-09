//! Detail screens and the cockpit's control column.

use super::paint;
use super::theme::*;
use super::{power_budget, View};
use crate::game::{Action, App, Screen};
use crate::sim::civ::{self, CivTier};
use crate::sim::state::{FaultKind, Focus};
use crate::sim::{fmt_f1, fmt_hours, fmt_kg, Loc, Res, ShipSpec, Sim};
use eframe::egui::{self, vec2, Color32, RichText, Sense, Ui};

fn faulted(sim: &Sim, k: FaultKind) -> bool {
    sim.faults.iter().any(|f| f.kind == k)
}

/// On/off toggle bound to an action.
fn toggle(ui: &mut Ui, on: bool, label: &str, key: &str, act: Action, acts: &mut Vec<Action>) {
    let txt = RichText::new(format!("{label}: {}", if on { "ON" } else { "OFF" })).color(if on { GOOD } else { DIM });
    if ui.selectable_label(on, txt).on_hover_text(format!("toggle  [{key}]")).clicked() {
        acts.push(act);
    }
}

/// Segmented percent control.
fn segmented(ui: &mut Ui, cur: u8, opts: &[u8], mk: impl Fn(u8) -> Action, acts: &mut Vec<Action>) {
    for &o in opts {
        if ui.selectable_label(cur == o, format!("{o}%")).clicked() && cur != o {
            acts.push(mk(o));
        }
    }
}

fn focus_combo(ui: &mut Ui, sim: &Sim, acts: &mut Vec<Action>) {
    egui::ComboBox::from_id_salt("focus")
        .selected_text(sim.focus.name())
        .show_ui(ui, |ui| {
            for f in Focus::ALL {
                if ui.selectable_label(sim.focus == f, f.name()).clicked() && sim.focus != f {
                    acts.push(Action::SetFocus(f));
                }
            }
        })
        .response
        .on_hover_text("Fab bay priority  [3]. Every non-idle focus runs life support first.");
}

// ------------------------------------------------------------ cockpit column

pub fn controls(ui: &mut Ui, app: &App, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let spec = sim.ship();
    ui.add_space(4.0);

    egui::CollapsingHeader::new(RichText::new("POWER & SYSTEMS").strong().color(ACCENT))
        .default_open(true)
        .show(ui, |ui| {
            let (gen_mw, load) = power_budget(sim);
            let short = gen_mw < load;
            bar_colored(ui, (gen_mw / load.max(1e-9)).min(1.0), format!("gen {gen_mw:.0} MW / draw {load:.0} MW"), ui.available_width(), if short { BAD } else { GOOD });
            if short {
                ui.label(RichText::new("BROWNOUT — systems running below spec").color(BAD).strong());
            }
            ui.horizontal(|ui| {
                ui.label("Reactor").on_hover_text("[4] cycles");
                segmented(ui, sim.reactor_pct, &[25, 50, 75, 100], Action::SetReactor, acts);
            });
            ui.horizontal_wrapped(|ui| {
                toggle(ui, sim.scoop_on, "Scoop", "1", Action::ToggleScoop, acts);
                toggle(ui, sim.mining_on, "Drones", "2", Action::ToggleMining, acts);
                toggle(ui, sim.shields, "Shields", "Y in Systems", Action::ToggleShields, acts);
            });
            ui.horizontal_wrapped(|ui| {
                let auto = sim.drone_auto;
                if ui.selectable_label(!auto, if auto { "Drones: auto" } else { "Drones: manual" }).on_hover_text("Automatic allocation by need, or manual target (planet screen)").clicked() {
                    acts.push(Action::ToggleDroneAuto);
                }
                toggle(ui, sim.repair_on, "Repair", "X in Systems", Action::ToggleRepair, acts);
                toggle(ui, sim.lights, "Lights", "O in Systems", Action::ToggleLights, acts);
            });
            ui.horizontal(|ui| {
                ui.label("Fab focus");
                focus_combo(ui, sim, acts);
            });
            for f in &sim.faults {
                ui.label(RichText::new(format!("⚠ {} — {} left", f.kind.name(), fmt_hours(f.until_h - sim.t_h()))).color(BAD))
                    .on_hover_text(format!("Nanobots clear it early if {:.0} MAT is on hand and repair is on.", f.mat_fix));
            }
            if sim.faults.is_empty() {
                ui.label(dim("all systems nominal"));
            }
            ui.label(RichText::new(format!("Credits: {:.0} cr", sim.credits)).color(GOOD));
        });

    egui::CollapsingHeader::new(RichText::new("FTL DRIVE").strong().color(ACCENT))
        .default_open(true)
        .show(ui, |ui| {
            let ch = sim.ftl_charge;
            bar_colored(ui, ch / 100.0, format!("capacitors {ch:.0}%"), ui.available_width(), if ch >= 100.0 { GOOD } else { ACCENT });
            ui.horizontal(|ui| {
                ui.label("Throttle").on_hover_text("[5] cycles. Lower throttle: slower, cheaper.");
                segmented(ui, sim.ftl_throttle, &[50, 75, 100], Action::SetThrottle, acts);
            });
            ui.label(format!(
                "wear {:.0}%   speed {:.1} ly/h",
                sim.ftl_wear,
                spec.ftl_speed * ShipSpec::throttle_factor(sim.ftl_throttle).0
            ));
            let dest = sim
                .destination
                .and_then(|d| app.gal.cache.get(&d))
                .map(|s| s.star.name.clone());
            ui.horizontal(|ui| {
                ui.label("Destination:");
                ui.label(RichText::new(dest.clone().unwrap_or_else(|| "none".into())).color(if dest.is_some() { ACCENT } else { DIM }));
                if ui.small_button("Star map  [M]").clicked() {
                    acts.push(Action::Open(Screen::Map));
                }
            });
            if sim.trip.is_some() {
                if ui.button(RichText::new("Skip to arrival  [X]").strong()).clicked() {
                    acts.push(Action::SkipToArrival);
                }
            } else {
                let ready = sim.can_depart();
                let why = if sim.destination.is_none() {
                    "Pick a destination on the star map."
                } else if sim.shuttle.is_some() {
                    "Finish the in-system shuttle first."
                } else {
                    "Capacitors still charging."
                };
                let b = egui::Button::new(RichText::new("JUMP  [T]").strong().color(if ready { GOOD } else { DIM }))
                    .min_size(vec2(ui.available_width(), 28.0));
                if ui.add_enabled(ready, b).on_disabled_hover_text(why).clicked() {
                    acts.push(Action::Jump);
                }
            }
        });

    egui::CollapsingHeader::new(RichText::new("LIFE SUPPORT").strong().color(ACCENT))
        .default_open(true)
        .show(ui, |ui| {
            let w = ui.available_width();
            for (r, label) in [(Res::Oxygen, "O2"), (Res::Water, "Water"), (Res::Food, "Food")] {
                let f = sim.inv.fraction(r);
                bar(ui, f, format!("{label} {:.0}%  ({} kg)", f * 100.0, fmt_kg(sim.inv.get(r))), w);
            }
            people_rows(ui, sim, w);
        });

    egui::CollapsingHeader::new(RichText::new("RESOURCES").strong().color(ACCENT))
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("res_mini").num_columns(3).spacing([6.0, 2.0]).show(ui, |ui| {
                for r in Res::ALL {
                    ui.label(RichText::new(r.short()).monospace().color(c(r.color()))).on_hover_text(r.name());
                    let f = sim.inv.fraction(r);
                    bar(ui, f, format!("{} kg", fmt_kg(sim.inv.get(r))), 150.0);
                    rate_label(ui, sim.rates.get(&r).copied().unwrap_or(0.0));
                    ui.end_row();
                }
            });
        });
}

fn people_rows(ui: &mut Ui, sim: &Sim, w: f32) {
    ui.label(format!("Crew {}", sim.crew.count));
    ui.horizontal(|ui| {
        bar(ui, sim.crew.health / 100.0, format!("health {:.0}", sim.crew.health), w / 2.0 - 6.0);
        bar_colored(ui, sim.crew.morale / 100.0, format!("morale {:.0}", sim.crew.morale), w / 2.0 - 6.0, if sim.crew.morale < 40.0 { WARN } else { ACCENT });
    });
    if sim.pax.count > 0 {
        ui.label(format!("Passengers {}", sim.pax.count));
        ui.horizontal(|ui| {
            bar(ui, sim.pax.health / 100.0, format!("health {:.0}", sim.pax.health), w / 2.0 - 6.0);
            bar_colored(ui, sim.pax.morale / 100.0, format!("morale {:.0}", sim.pax.morale), w / 2.0 - 6.0, if sim.pax.morale < 40.0 { WARN } else { ACCENT });
        });
    }
}

fn rate_label(ui: &mut Ui, r: f64) {
    if r.abs() < 0.005 {
        ui.label(dim("·"));
    } else if r > 0.0 {
        ui.label(RichText::new(format!("+{r:.1}/h")).color(GOOD).monospace());
    } else {
        ui.label(RichText::new(format!("{r:.1}/h")).color(BAD).monospace());
    }
}

fn back_row(ui: &mut Ui, title: &str, acts: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        if ui.button("← Back  [Esc]").clicked() {
            acts.push(Action::Back);
        }
        ui.heading(RichText::new(title).color(ACCENT));
    });
    ui.separator();
}

fn kv(ui: &mut Ui, k: &str, v: RichText) {
    ui.label(dim(k));
    ui.label(v);
    ui.end_row();
}

// ------------------------------------------------------------ planet

pub fn planet(ui: &mut Ui, app: &App, view: &View, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let sys = app.gal.system_ref(sim.sys_id);
    back_row(ui, "PLANET", acts);
    if sys.planets.is_empty() {
        ui.label("This system has no planets.");
        return;
    }
    let pi = app.sel_planet.min(sys.planets.len() - 1);
    let p = &sys.planets[pi];
    let at = matches!(sim.loc, Loc::Planet(i) if i == pi);

    ui.horizontal(|ui| {
        if ui.button("◀ Prev  [←]").clicked() {
            acts.push(Action::StepPlanet(-1));
        }
        for (i, q) in sys.planets.iter().enumerate() {
            let short = q.name.rsplit(' ').next().unwrap_or(&q.name).to_string();
            if ui.selectable_label(i == pi, short).on_hover_text(&q.name).clicked() {
                acts.push(Action::SelectPlanet(i));
            }
        }
        if ui.button("Next ▶  [→]").clicked() {
            acts.push(Action::StepPlanet(1));
        }
    });
    ui.add_space(6.0);

    ui.horizontal_top(|ui| {
        // Portrait.
        let side = 260.0;
        let (resp, painter) = ui.allocate_painter(vec2(side, side), Sense::hover());
        let r = resp.rect;
        paint::starfield(&painter, r, crate::gui::paint::seed_of(&p.name), 2.0, view.anim_t as f32);
        let pr = side * 0.36;
        let t = view.anim_t as f32;
        paint::planet(&painter, r.center(), pr, p, vec2(-0.8, -0.4), t * 0.08);
        for m in 0..p.moons.min(6) {
            let ang = t * (0.3 + m as f32 * 0.11) + m as f32 * 1.7;
            let orb = pr + 14.0 + m as f32 * 7.0;
            let mp = r.center() + vec2(ang.cos() * orb, ang.sin() * orb * 0.35);
            let behind = ang.sin() < 0.0;
            if !behind || mp.distance(r.center()) > pr {
                painter.circle_filled(mp, 2.5, Color32::from_rgb(190, 190, 200));
            }
        }

        ui.vertical(|ui| {
            ui.label(RichText::new(format!("{} — {}", p.name, p.class.name())).strong().size(19.0));
            egui::Grid::new("pfacts").num_columns(2).spacing([16.0, 3.0]).show(ui, |ui| {
                kv(ui, "Orbit", format!("{} AU  (period {:.1} yr)", fmt_f1(p.au), p.period).into());
                kv(ui, "Mass / radius", format!("{:.2} M⊕ / {:.2} R⊕", p.mass, p.radius).into());
                kv(ui, "Gravity", format!("{:.2} g", p.gravity).into());
                kv(ui, "Temperature", format!("{:.0} K", p.temp).into());
                kv(ui, "Composition", p.bulk.clone().into());
                kv(ui, "Moons", format!("{}", p.moons).into());
                kv(ui, "Atmosphere", p.atmo.as_ref().map(|a| a.describe()).unwrap_or_else(|| "none (hard vacuum)".into()).into());
            });
            let hab = (0.5..2.2).contains(&(p.au / sys.star.habitable_au()));
            if hab && p.class.mineable() {
                ui.label(RichText::new("within the habitable zone").color(GOOD));
            }
            if p.class.scoopable() {
                ui.label(RichText::new("Atmospheric scoop works here (H2 + N2)").color(ACCENT));
            }
            ui.add_space(6.0);
            let dock = if at {
                RichText::new("DOCKED").color(GOOD).strong()
            } else if let Some(leg) = &sim.shuttle {
                if matches!(leg.to, Loc::Planet(i) if i == pi) {
                    RichText::new(format!("shuttling here — {} left", fmt_hours(leg.remaining_h))).color(WARN)
                } else {
                    RichText::new("shuttling elsewhere").color(WARN)
                }
            } else if let Some(trip) = &sim.trip {
                RichText::new(format!("in FTL to {}", trip.dest_name)).color(CIV)
            } else {
                dim("not docked")
            };
            ui.horizontal(|ui| {
                ui.label(dock);
                if !at && ui.button("Shuttle here  [S]").clicked() {
                    acts.push(Action::ShuttleTo(Loc::Planet(pi)));
                }
                if at && ui.button("Return to star").clicked() {
                    acts.push(Action::ShuttleTo(Loc::Star));
                }
            });
            if p.class.mineable() {
                ui.horizontal(|ui| {
                    toggle(ui, sim.mining_on, "Mining", "2", Action::ToggleMining, acts);
                    if ui.selectable_label(!sim.drone_auto, if sim.drone_auto { "Allocation: auto" } else { "Allocation: manual" })
                        .on_hover_text("[A] toggles. Manual: pick a deposit below.")
                        .clicked()
                    {
                        acts.push(Action::ToggleDroneAuto);
                    }
                    ui.label(dim(if at && sim.mining_on { "drones working" } else { "drones idle" }));
                });
            }
        });
    });

    ui.add_space(8.0);
    section(ui, "DEPOSITS");
    if p.deposits.is_empty() {
        ui.label(dim("No solid deposits — scoopable atmosphere only."));
        return;
    }
    egui::Grid::new("deps").num_columns(5).striped(true).spacing([18.0, 4.0]).show(ui, |ui| {
        for h in ["", "Resource", "Remaining", "Status", ""] {
            ui.label(dim(h));
        }
        ui.end_row();
        for (di, d) in p.deposits.iter().enumerate() {
            let targeted = !sim.drone_auto && sim.mine_target == Some((pi, di));
            ui.label(RichText::new(d.res.short()).monospace().color(c(d.res.color())));
            ui.label(d.res.name());
            ui.label(format!("{:.0} t", d.tons));
            let (st, col) = if d.tons < 1.0 {
                ("exhausted", DIM)
            } else if targeted {
                ("◄ targeted", KEY)
            } else if at && sim.mining_on {
                ("mineable", TEXT)
            } else {
                ("available", DIM)
            };
            ui.label(RichText::new(st).color(col));
            if d.tons >= 1.0 && p.class.mineable() && !targeted {
                if ui.small_button("Target").on_hover_text("Switch drones to manual and mine this").clicked() {
                    acts.push(Action::SetMineTarget(pi, di));
                }
            } else {
                ui.label("");
            }
            ui.end_row();
        }
    });
}

// ------------------------------------------------------------ star

pub fn star(ui: &mut Ui, app: &App, view: &View, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let sys = app.gal.system_ref(sim.sys_id);
    let s = &sys.star;
    back_row(ui, "STAR", acts);
    ui.horizontal_top(|ui| {
        let side = 260.0;
        let (resp, painter) = ui.allocate_painter(vec2(side, side), Sense::hover());
        paint::starfield(&painter, resp.rect, sys.id ^ 0x5, 1.5, view.anim_t as f32);
        let r = (40.0 + 18.0 * (1.0 + s.radius as f32).ln()).min(side * 0.3);
        paint::star(&painter, resp.rect.center(), r, s.class.color(), view.anim_t as f32);

        ui.vertical(|ui| {
            ui.label(RichText::new(format!("{}  ({})", s.name, s.catalog)).strong().size(19.0).color(c(s.class.color())));
            let rad = s.class.radiation();
            egui::Grid::new("sfacts").num_columns(2).spacing([16.0, 3.0]).show(ui, |ui| {
                kv(ui, "Class", format!("{}{}", s.class.class_name(), if s.giant { " — giant" } else { "" }).into());
                kv(ui, "Mass", format!("{:.2} M☉", s.mass).into());
                kv(ui, "Radius", format!("{:.2} R☉", s.radius).into());
                kv(ui, "Luminosity", format!("{:.2} L☉", s.lum).into());
                kv(ui, "Surface temp", format!("{:.0} K", s.temp).into());
                kv(ui, "Age", format!("{:.1} Gyr", s.age).into());
                kv(ui, "Metallicity", format!("{:+.2} dex", s.metallicity).into());
                kv(ui, "Habitable zone", format!("~{} AU", fmt_f1(s.habitable_au())).into());
                kv(ui, "Scoop yield", format!("×{:.2} base", s.scoop_base()).into());
                let (lbl, col) = if rad > 0.5 { ("DANGEROUS", BAD) } else if rad > 0.2 { ("elevated", WARN) } else { ("low", GOOD) };
                kv(ui, "Radiation", RichText::new(format!("{lbl} ({rad:.2})")).color(col));
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if matches!(sim.loc, Loc::Star) && sim.shuttle.is_none() {
                    ui.label(RichText::new("DOCKED AT STAR — the fuel scoop works here").color(GOOD));
                } else if ui.button("Shuttle to star  [S]").clicked() {
                    acts.push(Action::ShuttleTo(Loc::Star));
                }
            });
            if let Some(cv) = &sys.civ {
                ui.add_space(6.0);
                ui.label(RichText::new(format!("{} — {}", cv.name, cv.tier.name())).color(CIV).strong());
                if cv.tier != CivTier::T1 && ui.button("Open trade  [K]").clicked() {
                    acts.push(Action::Open(Screen::Trade));
                }
            } else {
                ui.label(dim("No civilization detected."));
            }
        });
    });
    ui.add_space(8.0);
    section(ui, "WORLDS");
    egui::Grid::new("worlds").num_columns(5).striped(true).spacing([18.0, 3.0]).show(ui, |ui| {
        for (i, p) in sys.planets.iter().enumerate() {
            let here = matches!(sim.loc, Loc::Planet(j) if j == i);
            ui.label(if here { RichText::new("◉").color(GOOD) } else { dim("●") });
            if ui.selectable_label(i == app.sel_planet, &p.name).on_hover_text("open planet").clicked() {
                acts.push(Action::SelectPlanet(i));
                acts.push(Action::Open(Screen::Planet));
            }
            ui.label(p.class.name());
            ui.label(format!("{:.2} AU", p.au));
            ui.label(dim(format!("{} deposits", p.deposits.iter().filter(|d| d.tons >= 1.0).count())));
            ui.end_row();
        }
    });
}

// ------------------------------------------------------------ resources

pub fn resources(ui: &mut Ui, app: &App, acts: &mut Vec<Action>) {
    let sim = app.sim();
    back_row(ui, "RESOURCES", acts);
    ui.label(dim("Raw materials are gathered by the scoop and drones; the fab bay processes them with power."));
    ui.add_space(4.0);
    egui::Grid::new("res").num_columns(4).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
        for r in Res::ALL {
            ui.label(RichText::new(format!("{}  {}", r.short(), r.name())).color(c(r.color())).strong());
            let f = sim.inv.fraction(r);
            bar(ui, f, format!("{} / {} kg", fmt_kg(sim.inv.get(r)), fmt_kg(sim.inv.cap(r))), 260.0);
            rate_label(ui, sim.rates.get(&r).copied().unwrap_or(0.0));
            ui.label(dim(r.desc()));
            ui.end_row();
        }
    });
}

// ------------------------------------------------------------ crew

pub fn crew(ui: &mut Ui, app: &App, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let spec = sim.ship();
    back_row(ui, "CREW & PASSENGERS", acts);
    ui.label(RichText::new(format!("{} — crew complement {}", spec.name, spec.crew)).strong());
    egui::Grid::new("crew").num_columns(2).spacing([12.0, 2.0]).show(ui, |ui| {
        for (i, n) in sim.crew.names.iter().enumerate() {
            ui.label(dim(format!("{:>2}.", i + 1)));
            ui.label(n);
            ui.end_row();
        }
    });
    if sim.crew.count == 0 {
        ui.label(RichText::new("(no crew remaining)").color(BAD));
    }
    ui.add_space(4.0);
    let w = 420.0f32.min(ui.available_width());
    people_rows(ui, sim, w);
    if sim.stats.crew_lost > 0 {
        ui.label(RichText::new(format!("{} crew member(s) lost on this voyage.", sim.stats.crew_lost)).color(BAD));
    }
    if spec.pax > 0 {
        ui.label(RichText::new(format!("{} of {} passengers in stasis hibernation.", sim.pax.count, spec.pax)).color(ACCENT));
        if sim.stats.pax_lost > 0 {
            ui.label(RichText::new(format!("{} passengers lost.", sim.stats.pax_lost)).color(BAD));
        }
    }
    ui.add_space(8.0);
    section(ui, "VOYAGE");
    egui::Grid::new("stats").num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
        let st = &sim.stats;
        kv(ui, "Jumps", format!("{}  ({:.1} ly)", st.jumps, st.dist_ly).into());
        kv(ui, "Systems visited", format!("{}", st.systems).into());
        kv(ui, "Scooped / mined", format!("{} / {} kg", fmt_kg(st.scooped), fmt_kg(st.mined)).into());
        kv(ui, "Fabricated", format!("{} kg", fmt_kg(st.fabbed)).into());
        kv(ui, "Trades", format!("{}", st.trades).into());
    });
    ui.add_space(6.0);
    ui.label(dim("Health drops when O2, water or food run out, during brownouts, or under radiation. Medical supplies speed recovery. Morale affects fab and mining efficiency."));
}

// ------------------------------------------------------------ systems

pub fn systems(ui: &mut Ui, app: &App, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let spec = sim.ship();
    back_row(ui, "SHIP SYSTEMS", acts);
    ui.label(dim("Every system is automated, but you set the targets. Watch the power budget — spend too much and everything browns out."));
    ui.add_space(6.0);
    let rf = faulted(sim, FaultKind::Reactor);
    let fault_tag = |ui: &mut Ui, f: bool| {
        if f {
            ui.label(RichText::new("⚠ FAULT").color(BAD));
        } else {
            ui.label("");
        }
    };
    egui::Grid::new("sys").num_columns(4).striped(true).spacing([14.0, 6.0]).show(ui, |ui| {
        ui.label(key_chip("4"));
        ui.label("Reactor output");
        ui.horizontal(|ui| {
            segmented(ui, sim.reactor_pct, &[25, 50, 75, 100], Action::SetReactor, acts);
            let k = sim.reactor_pct as f64 / 100.0 * if rf { 0.55 } else { 1.0 };
            ui.label(dim(format!("{:.0} MW, burn {:.0} kg/h", spec.reactor_mw * k, spec.reactor_burn * k)));
        });
        fault_tag(ui, rf);
        ui.end_row();

        ui.label(key_chip("Y"));
        ui.label("Shields");
        ui.horizontal(|ui| {
            toggle(ui, sim.shields, "Shields", "Y", Action::ToggleShields, acts);
            ui.label(dim(format!("{:.0} MW — blocks stellar radiation", spec.shields_mw)));
        });
        ui.label("");
        ui.end_row();

        ui.label(key_chip("L"));
        ui.label("Life support");
        ui.horizontal(|ui| {
            segmented(ui, sim.ls_eff, &[50, 75, 100], Action::SetLsEff, acts);
            ui.label(dim(format!("{:.0} MW", spec.ls_mw * sim.ls_eff as f64 / 100.0)));
        });
        fault_tag(ui, faulted(sim, FaultKind::LifeSupport));
        ui.end_row();

        ui.label(key_chip("O"));
        ui.label("Deck lights");
        ui.horizontal(|ui| {
            toggle(ui, sim.lights, "Lights", "O", Action::ToggleLights, acts);
            ui.label(dim("off saves nothing but costs morale"));
        });
        ui.label("");
        ui.end_row();

        ui.label(key_chip("1"));
        ui.label("Fuel scoop");
        ui.horizontal(|ui| {
            toggle(ui, sim.scoop_on, "Scoop", "1", Action::ToggleScoop, acts);
            ui.label(dim(format!("{:.0} MW", spec.scoop_mw)));
        });
        fault_tag(ui, faulted(sim, FaultKind::Scoop));
        ui.end_row();

        ui.label(key_chip("2"));
        ui.label("Mining drones");
        ui.horizontal(|ui| {
            toggle(ui, sim.mining_on, "Drones", "2", Action::ToggleMining, acts);
            ui.label(dim(format!("{} drones, {:.0} MW", spec.drones, spec.drones as f64 * spec.drone_mw)));
        });
        fault_tag(ui, faulted(sim, FaultKind::Drones));
        ui.end_row();

        ui.label(key_chip("A"));
        ui.label("Drone allocation");
        ui.horizontal(|ui| {
            if ui.selectable_label(sim.drone_auto, "automatic").clicked() && !sim.drone_auto {
                acts.push(Action::ToggleDroneAuto);
            }
            if ui.selectable_label(!sim.drone_auto, "manual").clicked() && sim.drone_auto {
                acts.push(Action::ToggleDroneAuto);
            }
            if !sim.drone_auto && ui.small_button("Next target  [N]").clicked() {
                acts.push(Action::CycleMineTarget);
            }
        });
        ui.label("");
        ui.end_row();

        ui.label(key_chip("B"));
        ui.label("Fab bay focus");
        focus_combo(ui, sim, acts);
        ui.label("");
        ui.end_row();

        ui.label(key_chip("5"));
        ui.label("FTL throttle");
        ui.horizontal(|ui| {
            segmented(ui, sim.ftl_throttle, &[50, 75, 100], Action::SetThrottle, acts);
            ui.label(dim(format!("{:.1} ly/h", spec.ftl_speed * ShipSpec::throttle_factor(sim.ftl_throttle).0)));
        });
        fault_tag(ui, faulted(sim, FaultKind::FtlCoil));
        ui.end_row();

        ui.label(key_chip("X"));
        ui.label("Nanobot repair");
        ui.horizontal(|ui| {
            toggle(ui, sim.repair_on, "Repair", "X", Action::ToggleRepair, acts);
            ui.label(dim("spends MAT: hull +0.5/h, FTL wear −0.25/h"));
        });
        ui.label("");
        ui.end_row();
    });
    ui.add_space(8.0);
    let (gen_mw, load) = power_budget(sim);
    ui.horizontal(|ui| {
        ui.label("Power budget");
        bar_colored(ui, (gen_mw / load.max(1e-9)).min(1.0), format!("gen {gen_mw:.0} MW / draw {load:.0} MW"), 320.0, if gen_mw < load { BAD } else { GOOD });
        if gen_mw < load {
            ui.label(RichText::new("BROWNOUT").color(BAD).strong());
        }
    });
    ui.add_space(8.0);
    section(ui, "FAB RECIPES (kg in per kg out)");
    ui.label(dim("O2: 1.125 water → 1 O2 (+0.125 H2)    Water: 1 ice → 1"));
    ui.label(dim("Food: 0.5 C + 0.25 N + 0.5 W → 1    Mat: 0.5 metal + 0.5 silicate → 1    Med: 0.4 M + 0.2 C + 0.3 W → 1"));
}

// ------------------------------------------------------------ trade

pub fn trade(ui: &mut Ui, app: &App, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let sys = app.gal.system_ref(sim.sys_id);
    back_row(ui, "TRADE", acts);
    let Some(cv) = &sys.civ else {
        ui.label("Nobody to trade with.");
        return;
    };
    let t3 = cv.tier == CivTier::T3;
    ui.label(RichText::new(format!("{} — {}", cv.name, cv.tier.name())).color(CIV).strong().size(17.0));
    ui.label(dim(if t3 {
        "Full interstellar station: buy and sell anything; shipyard hull repair at 8 cr per hull point."
    } else if cv.tier == CivTier::T2 {
        "Planetary market: they buy raw materials (cheaply) and sell limited processed stock."
    } else {
        "They have not left their world. Observation only."
    }));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("Credits: {:.0} cr", sim.credits)).color(GOOD).strong());
        ui.separator();
        ui.label("Quantity");
        let mut q = app.trade_qty;
        if ui.add(egui::DragValue::new(&mut q).range(1.0..=1_000_000.0).speed(10.0).suffix(" kg")).changed() {
            acts.push(Action::SetTradeQty(q));
        }
        for preset in [10.0, 100.0, 1_000.0, 10_000.0] {
            if ui.selectable_label(app.trade_qty == preset, fmt_kg(preset)).clicked() {
                acts.push(Action::SetTradeQty(preset));
            }
        }
        ui.label(dim("[+]/[-] ×10"));
    });
    if t3 {
        let deficit = (sim.ship().hull - sim.hull).max(0.0);
        ui.horizontal(|ui| {
            let b = ui.add_enabled(deficit >= 1.0, egui::Button::new(format!("Shipyard: repair hull ({:.0} cr)  [Y]", deficit * 8.0)));
            if b.on_disabled_hover_text("Hull is intact.").clicked() {
                acts.push(Action::RepairHull);
            }
        });
    }
    ui.add_space(6.0);
    egui::Grid::new("market").num_columns(7).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
        for h in ["Resource", "Aboard", "Buy at", "Sell at", "Their stock", "", ""] {
            ui.label(dim(h));
        }
        ui.end_row();
        for (i, r) in Res::ALL.iter().enumerate() {
            let price = sys.trade_prices.get(r).copied().unwrap_or_else(|| civ::base_price(*r));
            let can_sell = t3 || (cv.tier == CivTier::T2 && r.is_raw());
            let stock = sys.trade_stock.get(r).copied().unwrap_or(0.0);
            let can_buy = stock > 0.0;
            let sel = i == app.trade_row;
            let name = RichText::new(format!("{}  {}", r.short(), r.name())).color(c(r.color()));
            if ui.selectable_label(sel, name).clicked() {
                acts.push(Action::TradeSelect(i));
            }
            ui.label(format!("{} kg", fmt_kg(sim.inv.get(*r))));
            ui.label(if can_buy { format!("{:.2}", price * if t3 { 1.0 } else { 1.2 }) } else { "—".into() });
            ui.label(if can_sell { format!("{:.2}", price * if t3 { 0.7 } else { 0.5 }) } else { "—".into() });
            ui.label(if stock > 0.0 { fmt_kg(stock) } else { "none".into() });
            if ui.add_enabled(can_buy, egui::Button::new("Buy").small()).on_hover_text("[B] on the selected row").clicked() {
                acts.push(Action::TradeSelect(i));
                acts.push(Action::Buy(*r));
            }
            if ui.add_enabled(can_sell, egui::Button::new("Sell").small()).on_hover_text("[S] on the selected row").clicked() {
                acts.push(Action::TradeSelect(i));
                acts.push(Action::Sell(*r));
            }
            ui.end_row();
        }
    });
}

// ------------------------------------------------------------ help

pub fn help(ui: &mut Ui, app: &App, acts: &mut Vec<Action>) {
    back_row(ui, "HELP", acts);
    let tips = [
        "H2 is life. Scoop it at the star (shuttle there, scoop on). The reactor and FTL drive burn it.",
        "Water comes from ice: dock at an ice world and let the drones mine (automatic allocation works well).",
        "O2 comes from water electrolysis; food from carbon + nitrogen + water. The fab bay runs life support first.",
        "Repair the hull with structural MAT (metal + silicate). FTL wear grows with every jump.",
        "Power budget: too many systems on → brownout → life support suffers.",
        "Jump: open the star map, pick a system, and jump once the capacitors read 100%. Charge depends on distance.",
        "Civilizations are about 1 in 50 systems and mostly untradeable. Be self-sufficient.",
        "Run out of H2 mid-jump and the drive collapses. Run out of O2, water or food and people die.",
    ];
    section(ui, "HOW TO SURVIVE");
    for t in tips {
        ui.label(format!("•  {t}"));
    }
    ui.add_space(8.0);
    section(ui, "MOUSE");
    ui.label("System view: click a world to select it, double-click a world or the star to shuttle there.");
    ui.label("Star map: drag to pan, scroll to zoom, click to select, double-click to set the destination.");
    ui.add_space(8.0);
    section(ui, "KEYS");
    let keys = [
        ("1 / 2 / 3", "scoop · drones · cycle fab focus"),
        ("4 / 5", "cycle reactor output · FTL throttle"),
        ("6", "ship systems"),
        ("S", "shuttle star ↔ selected planet"),
        ("P / I", "planet view (next planet) · star view"),
        ("← / →", "select previous / next planet"),
        ("M", "star map — ↑↓ or 1–9 select, Enter set/clear destination, T jump, C clear"),
        ("T / Enter", "jump (cockpit), once a destination is set and charged"),
        ("K", "trade — ↑↓ select, B buy, S sell, +/- quantity ×10, Y shipyard repair"),
        ("C / R", "crew · resources"),
        ("F", "time compression 1x → 10x → 60x → 600x → pause"),
        ("X", "skip to arrival during FTL (works while paused)"),
        ("G / Q", "save · save and quit"),
        ("H / Esc", "help · back"),
    ];
    egui::Grid::new("keys").num_columns(2).spacing([18.0, 3.0]).show(ui, |ui| {
        for (k, d) in keys {
            ui.label(RichText::new(k).monospace().color(KEY));
            ui.label(d);
            ui.end_row();
        }
    });
    ui.add_space(8.0);
    section(ui, "TIME");
    ui.label("1x is real time (1 game second per real second); fast-forward compresses it. Time keeps flowing on every screen.");
    if let Some(sim) = app.sim.as_ref() {
        ui.add_space(6.0);
        ui.label(dim(format!("Galaxy seed: 0x{:012X} — share it to replay your universe.", sim.seed)));
    }
}
