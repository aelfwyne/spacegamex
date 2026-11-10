//! Headless balance simulation (the `colonia-galactica-sim` binary and the
//! regression sweep). Runs the autopilot with no front end at all.

use crate::sim::{Galaxy, Level, Sim};

/// `--flag N` as u8, if present.
pub fn args_flag_u8(flag: &str) -> Option<u8> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == flag
            && let Some(v) = args.next() {
                return v.parse().ok();
            }
    }
    None
}

/// `--seed N` (decimal or 0x-hex), or a fresh random seed.
pub fn args_seed() -> u64 {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--seed"
            && let Some(v) = args.next() {
                if let Ok(n) = v.parse::<u64>() {
                    return n;
                }
                if let Ok(n) = u64::from_str_radix(v.trim_start_matches("0x"), 16) {
                    return n;
                }
            }
    }
    crate::rngx::random_seed()
}

pub fn sim_args() -> Option<u64> {
    // --sim HOURS  → headless balance run
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--sim" {
            if let Some(v) = args.next() {
                return v.parse::<u64>().ok();
            }
            return Some(200);
        }
    }
    None
}

/// Headless balance simulation with the autopilot.
pub fn headless_sim(ship_idx: usize, seed: u64, hours: u64) -> i32 {
    use crate::sim::Res as R;
    if ship_idx >= crate::sim::SHIPS.len() {
        eprintln!("error: --ship must be 0..{}", crate::sim::SHIPS.len() - 1);
        return 2;
    }
    let mut gal = Galaxy::new(seed);
    let mut sim = Sim::new_game(seed, ship_idx, &mut gal);
    let dt = 0.25f64; // h
    let steps = (hours as f64 / dt) as u64;
    let mut next_report = 24.0f64;
    let start_day = 0.0f64;
    println!("=== COLONIA GALACTICA headless balance run ===");
    println!("ship: {}  seed: 0x{seed:012X}  hours: {hours}", crate::sim::SHIPS[ship_idx].name);
    println!("t(day h)  sys      loc         H2%   WAT%  OXY%  FOO%  HULL%  HP  MOR  state");
    let mut last_cause = String::new();
    for _ in 0..steps {
        sim.autopilot(&mut gal);
        sim.advance(dt, &mut gal);
        let th = sim.t_h();
        if th >= next_report {
            next_report += 24.0;
            let s = &sim;
            let sys = gal.system_ref(s.sys_id);
            let state = if s.trip.is_some() {
                "TRIP".to_string()
            } else if s.shuttle.is_some() {
                "SHUTTLE".to_string()
            } else {
                crate::sim::state::loc_label(s.loc, sys).split(' ').next().unwrap_or("").to_string()
            };
            println!(
                "{:>8.1}  {:<9} {:<11} {:>4.0} {:>5.0} {:>5.0} {:>5.0} {:>6.0} {:>3.0} {:>4.0}  faults={} credits={:.0}",
                th / 24.0 + start_day,
                sys.star.name,
                state,
                s.inv.fraction(R::H2) * 100.0,
                s.inv.fraction(R::Water) * 100.0,
                s.inv.fraction(R::Oxygen) * 100.0,
                s.inv.fraction(R::Food) * 100.0,
                s.hull / s.ship().hull * 100.0,
                s.crew.health,
                s.crew.morale,
                s.faults.len(),
                s.credits
            );
        }
        if let Some(over) = &sim.over {
            last_cause = over.cause.clone();
            break;
        }
    }
    println!();
    if let Some(over) = &sim.over {
        println!("GAME OVER @ day {:.1} — {}", over.t / 86400.0, over.cause);
        println!("detail: {}", over.detail);
    } else {
        println!("SURVIVED {hours} h ({:.1} days)", hours as f64 / 24.0);
        let _ = &last_cause;
    }
    println!(
        "stats: jumps={} dist={:.1}ly systems={} scooped={:.0}kg mined={:.0}kg fabbed={:.0}kg trades={} crew_lost={} pax_lost={}",
        sim.stats.jumps, sim.stats.dist_ly, sim.stats.systems, sim.stats.scooped, sim.stats.mined, sim.stats.fabbed, sim.stats.trades, sim.stats.crew_lost, sim.stats.pax_lost
    );
    println!(
        "final: H2 {:.0}% WAT {:.0}% OXY {:.0}% FOO {:.0}% ICE {:.0}% MET {:.0}% SIL {:.0}% CAR {:.0}% NIT {:.0}% MAT {:.0}% MED {:.0}% hull {:.0}% wear {:.0}%",
        sim.inv.fraction(R::H2) * 100.0,
        sim.inv.fraction(R::Water) * 100.0,
        sim.inv.fraction(R::Oxygen) * 100.0,
        sim.inv.fraction(R::Food) * 100.0,
        sim.inv.fraction(R::Ice) * 100.0,
        sim.inv.fraction(R::Metal) * 100.0,
        sim.inv.fraction(R::Silicate) * 100.0,
        sim.inv.fraction(R::Carbon) * 100.0,
        sim.inv.fraction(R::Nitrogen) * 100.0,
        sim.inv.fraction(R::Mat) * 100.0,
        sim.inv.fraction(R::Med) * 100.0,
        sim.hull / sim.ship().hull * 100.0,
        sim.ftl_wear
    );
    println!("final log (last 12):");
    for e in sim.log.last_n(12) {
        let tag = match e.level {
            Level::Info => "  ",
            Level::Good => "++",
            Level::Warn => "!!",
            Level::Bad => "##",
            Level::Critical => "XX",
        };
        println!("  {tag} [day {:.1}] {}", e.t / 86400.0, e.msg);
    }
    i32::from(sim.over.is_some())
}
