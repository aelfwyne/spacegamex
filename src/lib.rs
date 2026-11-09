//! FTL Drift — spaceship survival sim.
//!
//! * [`sim`]   — the deterministic simulation (no UI, no I/O but save files)
//! * [`game`]  — toolkit-agnostic game shell: screens, actions, keys, tick
//! * [`gui`]   — the eframe/egui front end
//! * [`headless`] — autopilot balance runner for `spacegame-sim`

pub mod game;
pub mod gui;
pub mod headless;
pub mod rngx;
pub mod sim;
