//! Historical seed / logic-engine registry for Swiss baselines.

use crate::eval::EvalCheckpoint;
use crate::training::tournament::TourneyEntrant;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const DEFAULT_MANIFEST: &str = "models/history/manifest.json";
pub const DEFAULT_BIN_DIR: &str = "models/history/bin";
pub const DEFAULT_MODEL_DIR: &str = "models/history/models";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub git: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryManifest {
    #[serde(default)]
    pub weights: Vec<HistoryEntry>,
    #[serde(default)]
    pub engines: Vec<HistoryEntry>,
}

impl HistoryManifest {
    pub fn load_path(path: impl AsRef<Path>) -> Result<Self, String> {
        let text = fs::read_to_string(path.as_ref()).map_err(|e| {
            format!(
                "read history manifest {}: {e}",
                path.as_ref().display()
            )
        })?;
        serde_json::from_str(&text).map_err(|e| format!("parse history manifest: {e}"))
    }
}

/// `git show {rev}:models/ab-seed.json` → dest, then set checkpoint name to `id`.
pub fn extract_seed_at(rev: &str, id: &str, dest: &Path) -> Result<(), String> {
    let spec = format!("{rev}:models/ab-seed.json");
    let out = Command::new("git")
        .args(["show", &spec])
        .output()
        .map_err(|e| format!("git show {spec}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git show {spec} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let mut cp: EvalCheckpoint = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("parse seed at {rev}: {e}"))?;
    cp.name = id.to_string();
    cp.weights.rebuild_piece_value_table();
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    cp.save_path(dest)
        .map_err(|e| format!("save {}: {e}", dest.display()))?;
    Ok(())
}

pub fn logic_binary_path(id: &str) -> PathBuf {
    PathBuf::from(DEFAULT_BIN_DIR).join(id)
}

pub fn history_model_path(id: &str) -> PathBuf {
    PathBuf::from(DEFAULT_MODEL_DIR).join(format!("{id}.json"))
}

fn ensure_history_model(entry: &HistoryEntry) -> Result<PathBuf, String> {
    let dest = history_model_path(&entry.id);
    if !dest.is_file() {
        extract_seed_at(&entry.git, &entry.id, &dest)?;
    }
    Ok(dest)
}

/// One selectable GUI agent (current checkpoint or history BASE_/LOGIC_).
#[derive(Debug, Clone, Serialize)]
pub struct GuiAgent {
    pub id: String,
    pub label: String,
    pub path: String,
    /// `current`, `weights`, or `logic`.
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// Current `models/*.json` plus history manifest entries (extracting missing seeds).
pub fn list_gui_agents() -> Result<Vec<GuiAgent>, String> {
    let mut out = Vec::new();
    for name in crate::eval::list_model_files("models")? {
        out.push(GuiAgent {
            id: name.clone(),
            label: name.clone(),
            path: format!("models/{name}"),
            kind: "current".into(),
            engine: None,
            summary: None,
        });
    }
    let man = match HistoryManifest::load_path(DEFAULT_MANIFEST) {
        Ok(m) => m,
        Err(_) => return Ok(out),
    };
    for w in &man.weights {
        let dest = match ensure_history_model(w) {
            Ok(p) => p,
            Err(_) => continue,
        };
        out.push(GuiAgent {
            id: w.id.clone(),
            label: w.id.clone(),
            path: dest.display().to_string(),
            kind: "weights".into(),
            engine: None,
            summary: w.summary.clone(),
        });
    }
    for e in &man.engines {
        let dest = match ensure_history_model(e) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let bin = logic_binary_path(&e.id);
        out.push(GuiAgent {
            id: e.id.clone(),
            label: e.id.clone(),
            path: dest.display().to_string(),
            kind: "logic".into(),
            engine: Some(bin.display().to_string()),
            summary: e.summary.clone(),
        });
    }
    Ok(out)
}

/// Append BASE_* weight and LOGIC_* engine entrants under `out_dir`.
pub fn append_history_entrants(
    manifest_path: &Path,
    out_dir: &Path,
    entrants: &mut Vec<TourneyEntrant>,
) -> Result<Vec<String>, String> {
    let man = HistoryManifest::load_path(manifest_path)?;
    let mut ids = Vec::new();
    for w in &man.weights {
        let dest = out_dir.join(format!("{}.json", w.id));
        extract_seed_at(&w.git, &w.id, &dest)?;
        ids.push(w.id.clone());
        entrants.push(TourneyEntrant {
            id: w.id.clone(),
            model: dest.display().to_string(),
            engine: None,
        });
    }
    for e in &man.engines {
        let dest = out_dir.join(format!("{}.json", e.id));
        extract_seed_at(&e.git, &e.id, &dest)?;
        ids.push(e.id.clone());
        entrants.push(TourneyEntrant {
            id: e.id.clone(),
            model: dest.display().to_string(),
            engine: Some(logic_binary_path(&e.id).display().to_string()),
        });
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_manifest_lists_expected_ids() {
        let man = HistoryManifest::load_path(DEFAULT_MANIFEST).expect("manifest");
        let w: Vec<_> = man.weights.iter().map(|e| e.id.as_str()).collect();
        assert!(w.contains(&"BASE_PRELOUD"));
        assert!(w.contains(&"BASE_T150C50"));
        assert!(w.contains(&"BASE_T150C120"));
        assert!(w.contains(&"BASE_H120O80"));
        assert!(w.contains(&"BASE_H105O105"));
        assert!(w.contains(&"BASE_P120H50B75"));
        assert!(w.contains(&"BASE_P120H75B60"));
        let e: Vec<_> = man.engines.iter().map(|e| e.id.as_str()).collect();
        assert!(e.contains(&"LOGIC_TROPISM"));
        assert!(e.contains(&"LOGIC_H105"));
        assert!(e.contains(&"LOGIC_B65T12"));
    }

    #[test]
    fn list_gui_agents_includes_current_and_history_ids() {
        let agents = list_gui_agents().expect("gui agents");
        let ids: Vec<_> = agents.iter().map(|a| a.id.as_str()).collect();
        assert!(ids.iter().any(|id| id.ends_with(".json")));
        assert!(ids.contains(&"BASE_H105O105"));
        assert!(ids.contains(&"LOGIC_H105"));
        let logic = agents.iter().find(|a| a.id == "LOGIC_H105").unwrap();
        assert_eq!(logic.kind, "logic");
        assert!(logic.engine.as_ref().unwrap().contains("LOGIC_H105"));
        let base = agents.iter().find(|a| a.id == "BASE_H105O105").unwrap();
        assert_eq!(base.kind, "weights");
        assert!(base.engine.is_none());
    }

    #[test]
    fn h105_seed_loads_even_file_tables() {
        let tmp = std::env::temp_dir().join(format!(
            "tk-h105-{}.json",
            std::process::id()
        ));
        extract_seed_at("12791cb", "BASE_H105O105", &tmp).expect("git show H105");
        let cp = EvalCheckpoint::load_path(&tmp).unwrap();
        let _ = fs::remove_file(&tmp);
        assert!(
            cp.weights
                .file_factor
                .iter()
                .all(|f| (*f - 1.0).abs() < 1e-5),
            "missing file_factor must serde-default to even 1.0"
        );
        assert!(cp.weights.eg_tropism_scale > 0.0);
        assert!((cp.weights.two_mover_mob_k - 0.0).abs() < 1e-6);
    }
}
