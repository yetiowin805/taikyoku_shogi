//! Local data layout: raw is regenerable source of truth; derived is disposable.

use std::fs;
use std::path::{Path, PathBuf};

pub const DATA_ROOT: &str = "data";
pub const RAW_GAMES: &str = "data/raw/games";
pub const RAW_STARTS: &str = "data/raw/starts";
pub const DERIVED_POSITIONS: &str = "data/derived/positions";

pub fn ensure_data_dirs() -> Result<(), String> {
    for dir in [RAW_GAMES, RAW_STARTS, DERIVED_POSITIONS] {
        fs::create_dir_all(dir).map_err(|e| format!("Failed to create {}: {}", dir, e))?;
    }
    Ok(())
}

pub fn raw_games_dir() -> PathBuf {
    PathBuf::from(RAW_GAMES)
}

pub fn raw_starts_dir() -> PathBuf {
    PathBuf::from(RAW_STARTS)
}

pub fn derived_positions_dir() -> PathBuf {
    PathBuf::from(DERIVED_POSITIONS)
}

pub fn game_path(game_id: &str) -> PathBuf {
    raw_games_dir().join(format!("{}.json", game_id))
}

pub fn start_path(start_id: &str) -> PathBuf {
    raw_starts_dir().join(format!("{}.json", start_id))
}

/// List `*.json` files in a directory (sorted).
pub fn list_json_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| format!("read_dir {}: {}", dir.display(), e))? {
        let entry = entry.map_err(|e| format!("dir entry: {}", e))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}
