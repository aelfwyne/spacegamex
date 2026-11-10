//! Detail screens and the cockpit's control column.

use super::paint;
use super::theme::*;
use super::{power_budget, View};
use crate::game::{Action, App, Screen, TradeTab};
use crate::sim::civ::{self, CivTier};
use crate::sim::state::{FaultKind, Focus};
use crate::sim::{fmt_f1, fmt_hours, fmt_kg, Good, Loc, Res, ShipSpec, Sim};
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
            let (gen_mw, load, burn) = power_budget(app);
            let short = gen_mw < load;
            bar_colored(ui, (gen_mw / load.max(1e-9)).min(1.0), format!("gen {gen_mw:.0} MW / draw {load:.0} MW · burn {burn:.1} kg/h"), ui.available_width(), if short { BAD } else { GOOD })
                .on_hover_text("The reactor burns H2 for the power actually drawn, not its rating.");
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
                let sys = app.gal.system_ref(sim.sys_id);
                if sys.site(sim.loc).is_some_and(|p| p.salvage.as_ref().is_some_and(|s| s.remaining >= 1.0)) {
                    toggle(ui, sim.salvage_on, "Salvage", "V", Action::ToggleSalvage, acts);
                }
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
    // Below what "{:.1}" can show: a dot, not a red "-0.0/h".
    if r.abs() < 0.05 {
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

/// Gameplay consequences of a body's traits, in plain words.
fn hazards(sys: &crate::sim::system::StarSystem, loc: Loc, p: &crate::sim::planet::Planet) -> Vec<(String, Color32)> {
    use crate::sim::planet::{BeltFeature, LifeType, PlanetClass};
    let mut v: Vec<(String, Color32)> = Vec::new();
    if p.corrosion() > 0.0 {
        v.push((format!("Corrosive air: hull −{:.1}/h while docked (halved with shields up).", p.corrosion()), BAD));
    }
    if p.atmosphere == Some(crate::sim::planet::AtmoKind::Infiltrating) {
        v.push(("Infiltrating air seeps past seals: life-support faults are likely while docked.".into(), BAD));
    }
    let parent_rad = loc.planet().and_then(|i| sys.planets.get(i)).map(|g| g.radiation()).unwrap_or(0.0);
    if p.radiation() > 0.0 || (p.is_moon && parent_rad > 0.0) {
        v.push(("Inside a giant's radiation belts: hull and crew damage unless shields are up.".into(), WARN));
    }
    if p.shore_leave() {
        v.push(("Breathable air and a livable climate: shore leave lifts crew morale while docked.".into(), GOOD));
    }
    if p.life.is_some_and(|l| l.dangerous()) {
        v.push(("Dangerous wildlife: drone faults are three times as likely while mining here.".into(), WARN));
    }
    if p.life.is_some_and(|l| l.biomass()) {
        v.push(("Living biomass: carbon and nitrogen to harvest.".into(), GOOD));
    }
    if let Some(f) = p.gg_feature
        && f.scoop_mult() < 1.0 {
            v.push((format!("{}: scoop yield ×{:.2}.", f.name(), f.scoop_mult()), WARN));
        }
    if p.life.is_some_and(|l| matches!(l, LifeType::SkyEcosystem | LifeType::SkyLeviathans)) {
        v.push(("Sky life: the scoop also strains out a little carbon.".into(), GOOD));
    }
    if p.class == PlanetClass::AsteroidBelt {
        let k = if p.belt_feature == Some(BeltFeature::Dust) { 5 } else { 3 };
        v.push((format!("Debris: micrometeorite strikes ×{k} while working the belt."), WARN));
    }
    if p.scoopable() {
        v.push(("Scoopable envelope: H2 and N2 for the tanks.".into(), ACCENT));
    }
    v
}

pub fn planet(ui: &mut Ui, app: &App, view: &View, acts: &mut Vec<Action>) {
    use crate::sim::planet::{Host, PlanetClass};
    use crate::sim::worldgen::life::fmt_population;
    let sim = app.sim();
    let sys = app.gal.system_ref(sim.sys_id);
    back_row(ui, "WORLDS", acts);
    if sys.planets.is_empty() {
        ui.label("This system has no planets, moons or belts — only the star.");
        return;
    }
    let pi = app.sel_planet.min(sys.planets.len() - 1);
    let loc = match app.sel_moon {
        Some(m) if sys.planets[pi].satellites.get(m).is_some() => Loc::Moon(pi, m),
        _ => Loc::Planet(pi),
    };
    let Some(p) = sys.site(loc) else { return };
    let at = sim.loc == loc;

    ui.horizontal_wrapped(|ui| {
        if ui.button("◀ Prev  [←]").clicked() {
            acts.push(Action::StepPlanet(-1));
        }
        for (i, q) in sys.planets.iter().enumerate() {
            let short = q.name.rsplit(' ').next().unwrap_or(&q.name).to_string();
            let short = if q.host == Host::B { format!("B-{short}") } else { short };
            if ui.selectable_label(i == pi && app.sel_moon.is_none(), short).on_hover_text(format!("{} — {}", q.name, q.kind_label())).clicked() {
                acts.push(Action::SelectPlanet(i));
            }
        }
        if ui.button("Next ▶  [→]").clicked() {
            acts.push(Action::StepPlanet(1));
        }
    });
    let parent = &sys.planets[pi];
    if !parent.satellites.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.label(dim(format!("Moons of {}:", parent.name)));
            for (m, moon) in parent.satellites.iter().enumerate() {
                let tag = moon.name.rsplit(' ').next().unwrap_or("?").to_string();
                let mut t = RichText::new(tag);
                if moon.natives.is_some() || moon.colony.is_some() {
                    t = t.color(CIV);
                }
                if ui.selectable_label(app.sel_moon == Some(m), t).on_hover_text(format!("{} — {}", moon.name, moon.kind_label())).clicked() {
                    acts.push(Action::SelectBody(Loc::Moon(pi, m)));
                }
            }
        });
    }
    ui.add_space(6.0);

    ui.horizontal_top(|ui| {
        // Portrait.
        let side = 260.0;
        let (resp, painter) = ui.allocate_painter(vec2(side, side), Sense::hover());
        let r = resp.rect;
        paint::starfield(&painter, r, paint::seed_of(&p.name), 2.0, view.anim_t as f32);
        let t = view.anim_t as f32;
        let pr = if p.class == PlanetClass::GasGiant || p.class == PlanetClass::IceGiant { side * 0.3 } else { side * 0.36 };
        paint::planet(&painter, r.center(), pr, p, vec2(-0.8, -0.4), t * 0.08);
        let moon_n = if p.satellites.is_empty() { p.moons } else { p.satellites.len() };
        for m in 0..moon_n.min(6) {
            let ang = t * (0.3 + m as f32 * 0.11) + m as f32 * 1.7;
            let orb = pr * if p.ring { 2.35 } else { 1.0 } + 14.0 + m as f32 * 7.0;
            let mp = r.center() + vec2(ang.cos() * orb, ang.sin() * orb * 0.35);
            let behind = ang.sin() < 0.0;
            if !behind || mp.distance(r.center()) > pr {
                painter.circle_filled(mp, 2.5, Color32::from_rgb(190, 190, 200));
            }
        }

        ui.vertical(|ui| {
            ui.label(RichText::new(format!("{} — {}", p.name, p.kind_label())).strong().size(19.0));
            if let Loc::Moon(i, _) = loc
                && ui.small_button(format!("↑ {}", sys.planets[i].name)).clicked() {
                    acts.push(Action::SelectPlanet(i));
                }
            egui::Grid::new("pfacts").num_columns(2).spacing([16.0, 3.0]).show(ui, |ui| {
                let host = sys.host_star(p.host);
                let orbit = if p.host == Host::AB {
                    format!("{} AU around both stars (period {:.2} yr)", fmt_f1(p.au), p.period)
                } else {
                    format!("{} AU from {} (period {:.2} yr)", fmt_f1(p.au), host.name, p.period)
                };
                kv(ui, "Orbit", orbit.into());
                if p.class != PlanetClass::AsteroidBelt {
                    kv(ui, "Size", format!("{:.0} km  ·  {:.2} M⊕  ·  {:.2} g", p.size_km, p.mass, p.gravity).into());
                    if matches!(p.class, PlanetClass::GasGiant | PlanetClass::IceGiant) {
                        kv(ui, "Temperature", format!("{:.0} K at the cloud tops", p.temp).into());
                    } else {
                        kv(ui, "Temperature", format!("{} ({}) — {:.0} K", p.temp_band.name(), p.temp_band.range(), p.temp).into());
                    }
                    if let Some(a) = p.atmosphere {
                        let pressure = p.atmo.as_ref().map(|x| format!(", {:.2} bar", x.pressure_bar)).unwrap_or_default();
                        kv(ui, "Atmosphere", format!("{}{pressure}", a.describe()).into());
                    } else if !matches!(p.class, PlanetClass::GasGiant | PlanetClass::IceGiant) {
                        kv(ui, "Atmosphere", "none (hard vacuum)".to_string().into());
                    }
                    if let Some(g) = p.geosphere {
                        kv(ui, "Geosphere", g.name().to_string().into());
                    }
                    kv(ui, "Composition", p.bulk.clone().into());
                }
                if !p.surface.is_empty() {
                    kv(ui, "Surface", p.surface.clone().into());
                }
                if let Some(f) = p.gg_feature {
                    kv(ui, "Feature", f.name().to_string().into());
                }
                if let Some(f) = p.belt_feature {
                    kv(ui, "Feature", f.name().to_string().into());
                }
                let mut sat = Vec::new();
                if moon_n > 0 {
                    sat.push(format!("{moon_n} moon{}", if moon_n == 1 { "" } else { "s" }));
                }
                if p.ring {
                    sat.push("ring".into());
                }
                if !sat.is_empty() {
                    kv(ui, "In orbit", sat.join(", ").into());
                }
                if p.tidally_locked {
                    kv(ui, "Rotation", RichText::new("Tidally locked").color(WARN));
                }
            });
            if !p.terrain.is_empty() {
                ui.add_space(4.0);
                ui.label(dim("Terrain"));
                for t in &p.terrain {
                    ui.label(format!("•  {t}"));
                }
            }
            ui.add_space(6.0);
            let dock = if at {
                RichText::new("DOCKED").color(GOOD).strong()
            } else if let Some(leg) = &sim.shuttle {
                if leg.to == loc {
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
                    acts.push(Action::ShuttleTo(loc));
                }
                if at && ui.button("Return to star").clicked() {
                    acts.push(Action::ShuttleTo(Loc::Star));
                }
            });
            if p.mineable() {
                ui.horizontal(|ui| {
                    toggle(ui, sim.mining_on, "Drones", "2", Action::ToggleMining, acts);
                    if ui.selectable_label(!sim.drone_auto, if sim.drone_auto { "Allocation: auto" } else { "Allocation: manual" })
                        .on_hover_text("[A] toggles. Manual: pick a deposit below.")
                        .clicked()
                    {
                        acts.push(Action::ToggleDroneAuto);
                    }
                    ui.label(dim(if at && sim.mining_on { if sim.salvage_on { "drones salvaging" } else { "drones mining" } } else { "drones idle" }));
                });
            }
        });
    });

    // ---- hazards & effects ----
    let hz = hazards(sys, loc, p);
    if !hz.is_empty() {
        ui.add_space(8.0);
        section(ui, "CONDITIONS");
        for (t, col) in hz {
            ui.label(RichText::new(format!("•  {t}")).color(col));
        }
    }

    // ---- life & people ----
    if p.life.is_some() || p.natives.is_some() || p.colony.is_some() || !p.settlers.is_empty() {
        ui.add_space(8.0);
        section(ui, "LIFE & PEOPLE");
        if let Some(l) = p.life {
            ui.label(RichText::new(l.name()).strong().color(GOOD));
            ui.label(dim(l.description()));
        }
        if let Some(n) = &p.natives {
            ui.add_space(4.0);
            if n.tech == crate::sim::planet::NativeTech::ExtinctRuins {
                ui.label(RichText::new(format!("Ruins of the {}", n.species)).strong().color(DIM));
                ui.label(dim("A civilisation evolved here and is gone. Only its ruins remain."));
            } else {
                ui.label(RichText::new(format!("The {} — {}", n.species, n.tech.name())).strong().color(CIV));
                egui::Grid::new("natives").num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
                    if n.species_count > 1 {
                        kv(ui, "Species", format!("{} sentient species evolved here", n.species_count).into());
                    }
                    kv(ui, "Population", fmt_population(n.population).into());
                    kv(ui, "Politics", n.factions.clone().into());
                    kv(ui, "Power", n.dominant_power.clone().into());
                    if !n.in_orbit.is_empty() {
                        kv(ui, "In orbit", n.in_orbit.clone().into());
                    }
                    if !n.relations.is_empty() {
                        kv(ui, "Toward the colony", n.relations.clone().into());
                    }
                });
            }
        }
        if !p.settlers.is_empty() {
            ui.label(RichText::new(format!("Settled by: {}", p.settlers.join(", "))).color(CIV));
        }
        if let Some(cl) = &p.colony {
            ui.add_space(4.0);
            ui.label(RichText::new(format!("{} — {}", cl.name, cl.tech.name())).strong().color(super::sysview::COLONY_COL));
            egui::Grid::new("colony").num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
                kv(ui, "Founded by", format!("the {}", cl.founders).into());
                kv(ui, "Size", cl.size.clone().into());
                kv(ui, "Population", fmt_population(cl.population).into());
                if !cl.missions.is_empty() {
                    kv(ui, "Missions", cl.missions.clone().into());
                }
                if !cl.factions.is_empty() {
                    kv(ui, "Factions", format!("{} — {}", cl.factions, cl.faction_type).into());
                }
                if !cl.in_orbit.is_empty() {
                    kv(ui, "In orbit", cl.in_orbit.clone().into());
                }
            });
        }
    }

    // ---- salvage ----
    if let Some(sv) = &p.salvage {
        ui.add_space(8.0);
        section(ui, "SALVAGE");
        ui.label(&sv.what);
        ui.horizontal(|ui| {
            bar_colored(ui, sv.remaining / sv.initial.max(1.0), format!("{:.0} t left", sv.remaining / 1000.0), 220.0, WARN);
            if sv.remaining >= 1.0 {
                let r = ui.add_enabled(at, egui::Button::selectable(sim.salvage_on, if sim.salvage_on { "Salvaging  [V]" } else { "Salvage  [V]" }));
                if r.on_disabled_hover_text("Dock here first.").clicked() {
                    acts.push(Action::ToggleSalvage);
                }
            }
        });
        ui.label(dim("With salvage on, the drones strip wreckage for structural material and metal instead of mining — and now and then turn up something worth selling."));
    }

    ui.add_space(8.0);
    section(ui, "DEPOSITS");
    if p.deposits.is_empty() {
        ui.label(dim(if p.scoopable() { "No solid deposits — scoopable atmosphere only." } else { "No usable deposits." }));
        return;
    }
    egui::Grid::new("deps").num_columns(5).striped(true).spacing([18.0, 4.0]).show(ui, |ui| {
        for h in ["", "Resource", "Remaining", "Status", ""] {
            ui.label(dim(h));
        }
        ui.end_row();
        for (di, d) in p.deposits.iter().enumerate() {
            let targeted = !sim.drone_auto && sim.mine_target == Some((loc, di));
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
            if d.tons >= 1.0 && p.mineable() && !targeted {
                if ui.small_button("Target").on_hover_text("Switch drones to manual and mine this").clicked() {
                    acts.push(Action::SetMineTarget(loc, di));
                }
            } else {
                ui.label("");
            }
            ui.end_row();
        }
    });
}

// ------------------------------------------------------------ star

fn star_facts(ui: &mut Ui, id: &str, s: &crate::sim::star::Star) {
    let rad = s.radiation();
    egui::Grid::new(id).num_columns(2).spacing([16.0, 3.0]).show(ui, |ui| {
        kv(ui, "Type", format!("{} ({})", s.desc, s.code).into());
        kv(ui, "Mass", format!("{:.2} M☉", s.mass).into());
        kv(ui, "Radius", format!("{:.3} R☉", s.radius).into());
        kv(ui, "Luminosity", format!("{:.4} L☉", s.lum).into());
        kv(ui, "Surface temp", format!("{:.0} K", s.temp).into());
        kv(ui, "Age", format!("{:.1} Gyr", s.age).into());
        kv(ui, "Metallicity", format!("{:+.2} dex", s.metallicity).into());
        kv(ui, "Habitable zone", format!("~{:.2} AU", s.habitable_au()).into());
        kv(ui, "Scoop yield", format!("×{:.2} base", s.scoop_base()).into());
        let (lbl, col) = if rad > 0.5 { ("DANGEROUS", BAD) } else if rad > 0.2 { ("elevated", WARN) } else { ("low", GOOD) };
        kv(ui, "Radiation", RichText::new(format!("{lbl} ({rad:.2})")).color(col));
    });
}

pub fn star(ui: &mut Ui, app: &App, view: &View, acts: &mut Vec<Action>) {
    use crate::sim::planet::Host;
    let sim = app.sim();
    let sys = app.gal.system_ref(sim.sys_id);
    let s = &sys.star;
    back_row(ui, "STAR", acts);
    ui.horizontal_top(|ui| {
        let side = 260.0;
        let (resp, painter) = ui.allocate_painter(vec2(side, side), Sense::hover());
        paint::starfield(&painter, resp.rect, sys.id ^ 0x5, 1.5, view.anim_t as f32);
        let r = paint::star_px(s, 60.0).min(side * 0.32);
        paint::star_of(&painter, resp.rect.center(), r, s, view.anim_t as f32);
        if let Some(cp) = &sys.companion {
            let rb = paint::star_px(&cp.star, 26.0).min(side * 0.14);
            paint::star_of(&painter, resp.rect.right_top() + vec2(-rb - 18.0, rb + 18.0), rb, &cp.star, view.anim_t as f32);
        }
        ui.vertical(|ui| {
            ui.label(RichText::new(format!("{}  ({})", s.name, s.catalog)).strong().size(19.0).color(c(s.class.color())));
            star_facts(ui, "sfacts", s);
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
                ui.label(RichText::new(format!("{} on {} — {}", cv.display(), crate::sim::state::loc_label(cv.seat, sys), cv.tier.name())).color(CIV).strong());
                if cv.tier != CivTier::T1 && ui.button("Open trade  [K]").clicked() {
                    acts.push(Action::Open(Screen::Trade));
                }
            } else {
                ui.label(dim("No civilization detected."));
            }
        });
    });
    if let Some(cp) = &sys.companion {
        ui.add_space(8.0);
        section(ui, &format!("COMPANION — {} BINARY", cp.layout.name().to_uppercase()));
        ui.label(dim(cp.layout.description()));
        ui.horizontal_top(|ui| {
            star_facts(ui, "bfacts", &cp.star);
            egui::Grid::new("binfacts").num_columns(2).spacing([16.0, 3.0]).show(ui, |ui| {
                kv(ui, "Name", cp.star.name.clone().into());
                kv(ui, "Separation", format!("{:.2} AU (e = {:.2})", cp.separation_au, cp.ecc).into());
                kv(ui, "Mass ratio", format!("{:.2}", cp.mass_ratio).into());
                if cp.p_type {
                    kv(ui, "Planets", format!("circle both stars beyond {:.2} AU", cp.circumbinary).into());
                } else {
                    kv(ui, "Stable zone A", format!("out to {:.2} AU", cp.limit_a).into());
                    kv(ui, "Stable zone B", format!("out to {:.2} AU", cp.limit_b).into());
                }
            });
        });
        if sys.companion_dockable() {
            if sim.loc == Loc::Companion {
                ui.label(RichText::new("DOCKED AT THE COMPANION — the fuel scoop works here").color(GOOD));
            } else if ui.button(format!("Shuttle to {} ({:.1} AU)", cp.star.name, sys.dist_au(sim.loc, Loc::Companion))).clicked() {
                acts.push(Action::ShuttleTo(Loc::Companion));
            }
        }
    }
    ui.add_space(8.0);
    section(ui, "WORLDS");
    egui::Grid::new("worlds").num_columns(6).striped(true).spacing([14.0, 3.0]).show(ui, |ui| {
        for (i, p) in sys.planets.iter().enumerate() {
            let here = sim.loc.planet() == Some(i);
            ui.label(if here { RichText::new("◉").color(GOOD) } else { dim("●") });
            if ui.selectable_label(i == app.sel_planet, &p.name).on_hover_text("open").clicked() {
                acts.push(Action::SelectPlanet(i));
                acts.push(Action::Open(Screen::Planet));
            }
            ui.label(p.kind_label());
            ui.label(format!("{:.2} AU{}", p.au, if p.host == Host::B { " (B)" } else { "" }));
            let mut notes: Vec<String> = Vec::new();
            if !p.satellites.is_empty() {
                notes.push(format!("{} moons", p.satellites.len()));
            }
            if let Some(l) = p.life {
                notes.push(l.name().to_lowercase());
            }
            if p.natives.is_some() || p.satellites.iter().any(|m| m.natives.is_some()) {
                notes.push("natives".into());
            }
            if p.colony.is_some() || p.satellites.iter().any(|m| m.colony.is_some()) {
                notes.push("colony".into());
            }
            ui.label(dim(notes.join(", ")));
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
    ui.add_space(8.0);
    cargo_hold(ui, sim);
}

/// The commercial cargo hold, listed.
pub fn cargo_hold(ui: &mut Ui, sim: &Sim) {
    section(ui, "CARGO HOLD");
    let cap = sim.ship().cargo_t;
    bar(ui, sim.hold.mass_t() / cap.max(1e-9), format!("{:.1} / {cap:.0} t", sim.hold.mass_t()), 260.0);
    if sim.hold.is_empty() {
        ui.label(dim("Empty. Buy goods and colony supplies on the Trade screen's Cargo tab."));
        return;
    }
    egui::Grid::new("hold").num_columns(3).striped(true).spacing([14.0, 2.0]).show(ui, |ui| {
        for (g, n) in &sim.hold.units {
            ui.label(RichText::new(g.name()).color(if g.is_supply() { ACCENT } else { TEXT })).on_hover_text(g.desc());
            ui.label(format!("{n}"));
            ui.label(dim(format!("{:.1} t", g.mass_t() * *n as f64)));
            ui.end_row();
        }
    });
}

// ------------------------------------------------------------ crew

pub fn crew(ui: &mut Ui, app: &App, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let spec = sim.ship();
    back_row(ui, "CREW & PASSENGERS", acts);
    section(ui, "CAPTAIN");
    egui::Grid::new("captain").num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
        kv(ui, "Name", RichText::new(sim.player_name()).strong());
        kv(ui, "Sex", sim.player_sex.name().into());
        kv(ui, "Age", format!("{:.0}", sim.player_age().floor()).into());
        kv(ui, "Retires at", format!("about {:.0}", sim.retire_age.round()).into());
        let partner = match &sim.spouse {
            Some(sp) => format!("{} ({}, {})", sp.name, sp.sex.name().to_lowercase(), sp.species_name()),
            None => "none (recruit one at a port)".into(),
        };
        kv(ui, "Partner", partner.into());
        if let Some(p) = &sim.pilot {
            kv(ui, "Pilot", format!("{} (skill {})", p.name, p.skill).into());
        }
    });
    ui.add_space(6.0);
    ui.label(RichText::new(format!("{} — crew complement {}", spec.name, spec.crew)).strong());
    egui::Grid::new("crew").num_columns(2).spacing([12.0, 2.0]).show(ui, |ui| {
        for (i, n) in sim.crew.names.iter().enumerate() {
            ui.label(dim(format!("{:>2}.", i + 1)));
            let role = if i == 0 {
                " (captain)"
            } else if sim.spouse.as_ref().is_some_and(|r| &r.name == n) {
                " (partner)"
            } else if sim.pilot.as_ref().is_some_and(|r| &r.name == n) {
                " (pilot)"
            } else {
                ""
            };
            ui.label(format!("{n}{role}"));
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
            let (gen_mw, _, burn) = power_budget(app);
            let max_burn = spec.burn_at(gen_mw);
            ui.label(dim(format!("{gen_mw:.0} MW · burning {burn:.1} kg/h H2 now (up to {max_burn:.1} at full load)")));
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
    let (gen_mw, load, _) = power_budget(app);
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
    ui.label(RichText::new(format!("{} — {}", cv.display(), cv.tier.name())).color(CIV).strong().size(17.0));
    ui.label(dim(format!("Seat: {}{}", crate::sim::state::loc_label(cv.seat, sys), if cv.small { " (small outpost market)" } else { "" })));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for (t, label) in [(TradeTab::Resources, "Resources"), (TradeTab::Cargo, "Cargo"), (TradeTab::Port, "Port")] {
            if ui.add(egui::Button::selectable(app.trade_tab == t, RichText::new(label).strong())).clicked() {
                acts.push(Action::SetTradeTab(t));
            }
        }
        ui.label(dim("[Tab] next tab"));
        ui.separator();
        ui.label(RichText::new(format!("Credits: {:.0} cr", sim.credits)).color(GOOD).strong());
    });
    ui.separator();
    match app.trade_tab {
        TradeTab::Resources => trade_resources(ui, app, acts),
        TradeTab::Cargo => trade_cargo(ui, app, acts),
        TradeTab::Port => trade_port(ui, app, acts),
    }
}

fn trade_resources(ui: &mut Ui, app: &App, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let sys = app.gal.system_ref(sim.sys_id);
    let Some(cv) = &sys.civ else { return };
    let t3 = cv.tier == CivTier::T3;
    if !app.resource_trade_ok() {
        ui.label(dim("They have not left their world and do not deal in bulk resources. They will barter goods: see the Cargo tab."));
        return;
    }
    ui.label(dim(if t3 {
        "Full interstellar station: buy and sell anything; shipyard hull repair at 8 cr per hull point."
    } else {
        "Planetary market: they buy raw materials (cheaply) and sell limited processed stock."
    }));
    ui.horizontal(|ui| {
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

fn trade_cargo(ui: &mut Ui, app: &App, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let sys = app.gal.system_ref(sim.sys_id);
    let Some(market) = &sys.market else {
        ui.label(dim("No cargo market here."));
        return;
    };
    let cap = sim.ship().cargo_t;
    let used = sim.hold.mass_t();
    ui.horizontal(|ui| {
        bar(ui, used / cap.max(1e-9), format!("Hold {used:.1} / {cap:.0} t"), 220.0);
        ui.separator();
        ui.label("Units");
        let mut q = app.cargo_qty;
        if ui.add(egui::DragValue::new(&mut q).range(1..=10_000).speed(1.0)).changed() {
            acts.push(Action::SetCargoQty(q));
        }
        for preset in [1u32, 5, 10, 50] {
            if ui.selectable_label(app.cargo_qty == preset, preset.to_string()).clicked() {
                acts.push(Action::SetCargoQty(preset));
            }
        }
    });
    ui.label(dim("Prices are per unit. Every market prices goods differently: buy where they are cheap, sell where they are wanted."));
    ui.add_space(4.0);
    egui::Grid::new("cargo_market").num_columns(8).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
        for h in ["Good", "Mass", "Aboard", "Buy at", "Sell at", "Stock", "", ""] {
            ui.label(dim(h));
        }
        ui.end_row();
        for (i, g) in Good::ALL.iter().enumerate() {
            let have = sim.hold.get(*g);
            let buy = market.buy_price(*g);
            let sell = market.sell_price(*g);
            if buy.is_none() && sell.is_none() && have == 0 {
                continue;
            }
            let sel = app.trade_tab == TradeTab::Cargo && i == app.trade_row;
            let col = if g.is_supply() { ACCENT } else { TEXT };
            if ui.selectable_label(sel, RichText::new(g.name()).color(col)).on_hover_text(g.desc()).clicked() {
                acts.push(Action::TradeSelect(i));
            }
            ui.label(dim(format!("{:.1} t", g.mass_t())));
            ui.label(if have > 0 { have.to_string() } else { "—".into() });
            ui.label(buy.map(|p| format!("{p:.0}")).unwrap_or_else(|| "—".into()));
            ui.label(sell.map(|p| format!("{p:.0}")).unwrap_or_else(|| "—".into()));
            let stock = market.offers.get(g).map(|o| o.stock).unwrap_or(0);
            ui.label(if buy.is_some() { stock.to_string() } else { "—".into() });
            if ui.add_enabled(buy.is_some(), egui::Button::new("Buy").small()).on_hover_text("[B] on the selected row").clicked() {
                acts.push(Action::TradeSelect(i));
                acts.push(Action::BuyGood(*g));
            }
            if ui.add_enabled(sell.is_some() && have > 0, egui::Button::new("Sell").small()).on_hover_text("[S] on the selected row").clicked() {
                acts.push(Action::TradeSelect(i));
                acts.push(Action::SellGood(*g));
            }
            ui.end_row();
        }
    });
}

fn trade_port(ui: &mut Ui, app: &App, acts: &mut Vec<Action>) {
    let sim = app.sim();
    let Some(tier) = sim.port_tier(&app.gal) else {
        ui.label(dim("Pre-space peoples have no port: nobody to recruit or hire here."));
        return;
    };
    section(ui, "RECRUIT A PARTNER");
    match &sim.spouse {
        Some(sp) => {
            ui.label(format!("{} ({}, {}) is aboard as your partner.", sp.name, sp.sex.name().to_lowercase(), sp.species_name()));
        }
        None => {
            ui.label(dim(format!(
                "Look for a {} partner willing to leave with you. A partner lets a single captain found a colony, and helps with every job aboard a one-person ship (+10%). A native partner makes the colony Hybrid and needs a Genetic Modification Kit for children.",
                sim.player_sex.opposite().name().to_lowercase()
            )));
            let cost = crate::sim::commerce::recruit_cost(tier);
            let chance = if tier == CivTier::T3 { "about 1 in 2" } else { "about 1 in 3" };
            if ui.add_enabled(sim.credits >= cost, egui::Button::new(format!("Search the port ({cost:.0} cr, 1 h, {chance})"))).clicked() {
                acts.push(Action::RecruitPartner);
            }
        }
    }
    ui.add_space(8.0);
    section(ui, "HIRE A PILOT");
    match &sim.pilot {
        Some(p) => {
            ui.label(format!("Pilot {} (skill {}) flies for you: better odds in battle and when fleeing.", p.name, p.skill));
        }
        None => {
            ui.label(dim("A combat pilot improves your odds against pirates, especially when running."));
            let cost = crate::sim::commerce::pilot_cost(tier);
            let skill = if tier == CivTier::T3 { "skill 2–3" } else { "skill 1–2" };
            if ui.add_enabled(sim.credits >= cost, egui::Button::new(format!("Hire a pilot ({cost:.0} cr, {skill})"))).clicked() {
                acts.push(Action::HirePilot);
            }
        }
    }
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
    ui.label("System view: click a world or moon to select it, double-click to shuttle there. Drag to pan, scroll to zoom; zoom in (or select a giant) to see its moons.");
    ui.label("Binary systems: the view buttons frame star A, star B or both; a wide companion is a second star you can shuttle to and scoop.");
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
        ("V", "drones salvage wreckage instead of mining (where there is any)"),
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
