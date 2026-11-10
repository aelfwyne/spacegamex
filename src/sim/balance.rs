//! Balance harness: parks each ship at each kind of site and measures the
//! real simulation's net resource rates. Run with
//! `cargo test --release balance_table -- --ignored --nocapture`.

#[cfg(test)]
mod tests {
    use crate::sim::galaxy::Galaxy;
    use crate::sim::planet::PlanetClass;
    use crate::sim::star::{StarClass, StarKind};
    use crate::sim::state::Loc;
    use crate::sim::{GalaxyOpts, Res, Sim, SHIPS};

    /// First non-home system whose primary is a main-sequence `class` star.
    fn find_star(gal: &mut Galaxy, class: StarClass) -> Option<u64> {
        for r in 0i64..12 {
            for cx in -r..=r {
                for cy in -r..=r {
                    for id in gal.ids_in_cell(cx, cy) {
                        if id == gal.home_id() {
                            continue;
                        }
                        let s = gal.system(id);
                        let kind_ok = class == StarClass::D || s.star.kind == StarKind::MainSequence;
                        if s.star.class == class && kind_ok && s.companion.is_none() {
                            return Some(id);
                        }
                    }
                }
            }
        }
        None
    }

    fn home_site(gal: &mut Galaxy, f: impl Fn(&crate::sim::planet::Planet) -> bool) -> Option<Loc> {
        let home = gal.home_id();
        gal.system(home).planets.iter().position(f).map(Loc::Planet)
    }

    /// Net kg/h of every resource after parking at (sys, loc).
    fn measure(opts: GalaxyOpts, ship: usize, sys: u64, loc: Loc) -> Vec<(Res, f64)> {
        let mut gal = Galaxy::with_opts(42, opts);
        let mut sim = Sim::new_game(42, ship, &mut gal);
        gal.system(sys);
        sim.sys_id = sys;
        sim.loc = loc;
        sim.scoop_on = true;
        sim.mining_on = true;
        sim.shields = true;
        sim.ftl_charge = 100.0;
        sim.opts.risk = crate::sim::Risk::Tourist;
        let dt = 0.25;
        // Pirates are not part of a rate measurement: wave them off.
        for _ in 0..(24.0 / dt) as usize {
            sim.encounter = None;
            sim.advance(dt, &mut gal);
        }
        // Raw tanks at 20%, so no tank tops out and hides its true rate.
        for r in [Res::H2, Res::Ice, Res::Metal, Res::Silicate, Res::Carbon, Res::Nitrogen] {
            let now = sim.inv.get(r);
            sim.inv.take(r, now);
            sim.inv.add(r, sim.inv.cap(r) * 0.2);
        }
        let before: Vec<f64> = Res::ALL.iter().map(|&r| sim.inv.get(r)).collect();
        let hours = 12.0;
        for _ in 0..(hours / dt) as usize {
            sim.encounter = None;
            sim.advance(dt, &mut gal);
        }
        Res::ALL.iter().zip(before).map(|(&r, b)| (r, (sim.inv.get(r) - b) / hours)).collect()
    }

    #[test]
    #[ignore]
    fn balance_table() {
        let opts = GalaxyOpts::default();
        let mut gal = Galaxy::with_opts(42, opts);
        let home = gal.home_id();
        let mut sites: Vec<(String, u64, Loc)> = vec![("G star (home)".into(), home, Loc::Star)];
        for (name, c) in [("K star", StarClass::K), ("M dwarf", StarClass::M), ("White dwarf", StarClass::D)] {
            if let Some(id) = find_star(&mut gal, c) {
                sites.push((name.into(), id, Loc::Star));
            }
        }
        if let Some(l) = home_site(&mut gal, |p| p.class == PlanetClass::GasGiant) {
            sites.push(("Gas giant".into(), home, l));
        }
        if let Some(l) = home_site(&mut gal, |p| p.class == PlanetClass::IceGiant) {
            sites.push(("Ice giant".into(), home, l));
        }
        if let Some(l) = home_site(&mut gal, |p| matches!(p.class, PlanetClass::Terrestrial | PlanetClass::SuperEarth)) {
            sites.push(("Rocky world".into(), home, l));
        }
        let mut runs: Vec<(usize, GalaxyOpts)> = (0..SHIPS.len()).map(|i| (i, opts)).collect();
        for r in [crate::sim::Abundance::Scarce, crate::sim::Abundance::Abundant] {
            runs.push((0, GalaxyOpts { resources: r, ..opts }));
        }
        for (si, opts) in runs {
            let spec = &SHIPS[si];
            println!("\n== {} — resources {} ==", spec.name, opts.resources.name());
            print!("{:<14}", "site");
            for r in Res::ALL {
                print!("{:>8}", r.short());
            }
            println!();
            for (name, id, loc) in &sites {
                let rates = measure(opts, si, *id, *loc);
                print!("{name:<14}");
                for (_, v) in rates {
                    print!("{v:>8.1}");
                }
                println!();
            }
        }
    }

    /// The smallest ship must never lose fuel parked at an ordinary star,
    /// even on Scarce: red dwarfs are most of the galaxy.
    #[test]
    fn small_ship_refuels_at_dim_stars_even_when_scarce() {
        let opts = GalaxyOpts { resources: crate::sim::Abundance::Scarce, ..GalaxyOpts::default() };
        let mut gal = Galaxy::with_opts(42, opts);
        for class in [StarClass::M, StarClass::D] {
            let id = find_star(&mut gal, class).expect("star of class");
            let h2 = measure(opts, 0, id, Loc::Star).into_iter().find(|(r, _)| *r == Res::H2).unwrap().1;
            assert!(h2 > 0.0, "{class:?}: H2 {h2:.1} kg/h");
        }
    }
}
