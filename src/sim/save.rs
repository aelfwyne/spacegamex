//! Save / load. Persists the whole sim plus every materialised star system,
//! so deposits-mined and trade stock survive across sessions.

use super::galaxy::Galaxy;
use super::state::Sim;
use super::system::StarSystem;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// v2: star positions spread across their grid cell.
/// v3: detailed world generation (moons, binaries, life, colonies).
/// Older galaxies have a different shape, so older saves cannot continue.
pub const VERSION: u32 = 3;

#[derive(Serialize, Deserialize)]
pub struct SaveFile {
    pub version: u32,
    pub sim: Sim,
    pub seed: u64,
    /// Galaxy options; saves from before they existed used the defaults.
    #[serde(default)]
    pub opts: super::opts::GalaxyOpts,
    pub systems: Vec<(u64, StarSystem)>,
}

/// Per-user data directory for the game.
///
/// Linux: `$XDG_DATA_HOME/spacegame` (default `~/.local/share/spacegame`).
/// Windows: `%APPDATA%\spacegame\data`. macOS: `~/Library/Application
/// Support/spacegame`. Tests use a private temp dir so they never touch a
/// player's real save.
pub fn data_dir() -> std::path::PathBuf {
    if cfg!(test) {
        return std::env::temp_dir().join(format!("spacegame-test-{}", std::process::id()));
    }
    directories::ProjectDirs::from("", "", "spacegame")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("spacegame"))
}

pub fn save_path() -> std::path::PathBuf {
    data_dir().join("save.json")
}

pub fn save(sim: &Sim, galaxy: &Galaxy) -> std::io::Result<std::path::PathBuf> {
    let path = save_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Only systems the ship has visited can differ from their generated
    // state (mined deposits, bought-out markets); everything else is a pure
    // function of the seed and regenerates identically on load.
    let systems: Vec<(u64, StarSystem)> = galaxy
        .cache
        .iter()
        .filter(|(k, _)| sim.visited.contains(k) || **k == sim.sys_id)
        .map(|(k, v)| (*k, v.clone()))
        .collect();
    let file = SaveFile {
        version: VERSION,
        sim: sim.clone(),
        seed: galaxy.seed,
        opts: galaxy.opts,
        systems,
    };
    let json = serde_json::to_string(&file).map_err(std::io::Error::other)?;
    // Write-then-rename: a crash mid-write leaves the previous save intact
    // instead of a truncated file that no longer parses.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Is there a save file at all (valid or not)?
pub fn exists() -> bool {
    save_path().is_file()
}

/// Load the save, or say exactly why it could not be loaded.
pub fn load() -> Result<(Sim, Galaxy), String> {
    let path = save_path();
    let data = std::fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "No saved game found.".to_string()
        } else {
            format!("Could not read {}: {e}", path.display())
        }
    })?;
    let file: SaveFile = serde_json::from_str(&data)
        .map_err(|e| format!("Save file is damaged ({}): {e}", path.display()))?;
    if file.version != VERSION {
        return Err(format!(
            "Save is from an incompatible version ({} — this build reads {}).",
            file.version, VERSION
        ));
    }
    let mut galaxy = Galaxy::with_opts(file.seed, file.opts);
    let systems: HashMap<u64, StarSystem> = file.systems.into_iter().collect();
    galaxy.cache = systems;
    // Regenerate what the save left out but the game will look up at once:
    // where we are, where we are going, and the local neighbourhood.
    let sim = file.sim;
    let here = galaxy.system(sim.sys_id).pos;
    galaxy.neighbors(here, 60.0, sim.sys_id);
    if let Some(d) = sim.destination {
        galaxy.system(d);
    }
    if let Some(t) = &sim.trip {
        galaxy.system(t.dest_id);
    }
    Ok((sim, galaxy))
}

pub fn delete() {
    let _ = std::fs::remove_file(save_path());
}
