//! Headless balance simulator.
//!
//! `colonia-galactica-sim --sim 500 [--ship 0|1|2] [--seed N]` runs the autopilot
//! for N in-game hours and exits 1 if the ship was lost.

use colonia_galactica::headless;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(hours) = headless::sim_args().or_else(|| {
        // Bare invocation: default length, so `colonia-galactica-sim --seed 7` works.
        std::env::args().len().gt(&1).then_some(200)
    }) else {
        eprintln!("usage: colonia-galactica-sim --sim HOURS [--ship 0|1|2] [--seed N]");
        return ExitCode::from(2);
    };
    let ship = headless::args_flag_u8("--ship").unwrap_or(0) as usize;
    let seed = headless::args_seed();
    match headless::headless_sim(ship, seed, hours) {
        0 => ExitCode::SUCCESS,
        1 => ExitCode::FAILURE,
        _ => ExitCode::from(2),
    }
}
