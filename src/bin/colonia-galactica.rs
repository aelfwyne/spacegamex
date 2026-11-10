//! Colonia Galactica — windowed game.
//!
//! Release builds on Windows are GUI-subsystem apps (no console window);
//! the headless simulator is a separate console binary, `colonia-galactica-sim`.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use eframe::egui;
use colonia_galactica::gui::GuiApp;

/// Must match the `.desktop` file name (and StartupWMClass) so KDE/Wayland
/// shows the right icon and groups the window in the task manager.
const APP_ID: &str = "colonia-galactica";

fn main() -> eframe::Result {
    let seed = colonia_galactica::headless::args_seed();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Colonia Galactica")
        .with_app_id(APP_ID)
        .with_inner_size([1360.0, 860.0])
        .with_min_inner_size([960.0, 600.0]);
    match eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon.png")) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(e) => eprintln!("warning: window icon: {e}"),
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(APP_ID, options, Box::new(move |cc| Ok(Box::new(GuiApp::new(cc, seed)))))
}
