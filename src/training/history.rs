//! Historical seed / logic-engine registry for Swiss baselines.

use crate::eval::EvalCheckpoint;
use crate::training::tournament::TourneyEntrant;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const DEFAULT_MANIFEST: &str = "models/history/manifest.json";
pub const DEFAULT_BIN_DIR: &str = "models/history/bin";

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
    pub fn git_for(&self, id: &str) -> Result<&str, String> {
        self.weights
            .iter()
            .chain(self.engines.iter())
            .find(|e| e.id == id)
            .map(|e| e.git.as_str())
            .ok_or_else(|| format!("history id {id} not in manifest"))
    }

    pub fn load_path(path: impl AsRef<Path>) -> Result<Self, String> {
        let text = fs::read_to_string(path.as_ref())
            .map_err(|e| format!("read history manifest {}: {e}", path.as_ref().display()))?;
        serde_json::from_str(&text).map_err(|e| format!("parse history manifest: {e}"))
    }
}

/// `git show {rev}:models/ab-seed.json`, then set checkpoint name to `id`.
pub fn load_seed_at(rev: &str, id: &str) -> Result<EvalCheckpoint, String> {
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
    let mut cp: EvalCheckpoint =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("parse seed at {rev}: {e}"))?;
    cp.name = id.to_string();
    cp.weights.rebuild_piece_value_table();
    Ok(cp)
}

/// `git show {rev}:models/ab-seed.json` → dest, then set checkpoint name to `id`.
pub fn extract_seed_at(rev: &str, id: &str, dest: &Path) -> Result<(), String> {
    let cp = load_seed_at(rev, id)?;
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

/// Append BASE_* weight and LOGIC_* engine entrants under `out_dir`.
pub fn append_history_entrants(
    manifest_path: &Path,
    out_dir: &Path,
    entrants: &mut Vec<TourneyEntrant>,
) -> Result<Vec<String>, String> {
    append_history_entrants_except(manifest_path, out_dir, entrants, &[])
}

/// Like [`append_history_entrants`], skipping ids already in the field (or dropped).
pub fn append_history_entrants_except(
    manifest_path: &Path,
    out_dir: &Path,
    entrants: &mut Vec<TourneyEntrant>,
    skip: &[&str],
) -> Result<Vec<String>, String> {
    let skip: HashSet<&str> = skip.iter().copied().collect();
    let man = HistoryManifest::load_path(manifest_path)?;
    let mut ids = Vec::new();
    for w in &man.weights {
        if skip.contains(w.id.as_str()) {
            continue;
        }
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
        if skip.contains(e.id.as_str()) {
            continue;
        }
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
        assert!(e.contains(&"LOGIC_HANGQ_ST"));
        assert!(e.contains(&"LOGIC_HANGQ_AB"));
        assert!(e.contains(&"LOGIC_HANGQ_ANY"));
    }

    #[test]
    fn h105_seed_loads_even_file_tables() {
        let tmp = std::env::temp_dir().join(format!("tk-h105-{}.json", std::process::id()));
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

    #[test]
    fn append_except_skips_listed_ids() {
        let tmp = std::env::temp_dir().join(format!("tk-hist-skip-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let mut entrants = Vec::new();
        let ids = append_history_entrants_except(
            Path::new(DEFAULT_MANIFEST),
            &tmp,
            &mut entrants,
            &["LOGIC_B65T12", "LOGIC_TROPISM", "BASE_T150C50"],
        )
        .expect("append");
        assert!(!ids.iter().any(|id| id == "LOGIC_B65T12"));
        assert!(!ids.iter().any(|id| id == "LOGIC_TROPISM"));
        assert!(!ids.iter().any(|id| id == "BASE_T150C50"));
        assert!(ids.iter().any(|id| id == "LOGIC_H105"));
        assert!(ids.iter().any(|id| id == "BASE_PRELOUD"));
        let _ = fs::remove_dir_all(&tmp);
    }
}
