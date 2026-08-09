//! On-disk status for continuous `worker daemon` runs.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Snapshot written to `data/run/status.json` (and optional `--status` path).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerDaemonConfig {
    pub black: String,
    pub white: String,
    pub depth: Option<u32>,
    pub model: Option<String>,
    pub qdepth: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_time_ms: Option<u64>,
    pub jobs: usize,
    pub batch: usize,
    pub starts: String,
    pub outdir: String,
    pub seed_base: u64,
    pub max_moves: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStatus {
    pub running: bool,
    pub games_completed: usize,
    pub games_failed: usize,
    pub batches_completed: usize,
    /// ISO-8601 UTC when the daemon started.
    pub started_at: String,
    /// ISO-8601 UTC of the last status write.
    pub updated_at: String,
    pub last_game_id: Option<String>,
    pub last_error: Option<String>,
    pub config: WorkerDaemonConfig,
    pub disk_free_gb: Option<f64>,
    /// Next seed offset that will be used for the following game index.
    pub next_seed: u64,
    pub stop_requested: bool,
}

impl RunStatus {
    pub fn write_path(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("status parent: {}", e))?;
        }
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(self).map_err(|e| format!("status json: {}", e))?;
        fs::write(&tmp, json).map_err(|e| format!("write {}: {}", tmp.display(), e))?;
        fs::rename(&tmp, path).map_err(|e| format!("rename status: {}", e))?;
        Ok(())
    }

    pub fn load_path(path: &Path) -> Result<Self, String> {
        let data = fs::read_to_string(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
        serde_json::from_str(&data).map_err(|e| format!("parse {}: {}", path.display(), e))
    }
}

/// Approximate free space on the filesystem containing `path` (via `df -k`).
pub fn disk_free_gb(path: &Path) -> Option<f64> {
    let target = if path.exists() {
        path.to_path_buf()
    } else {
        path.parent()
            .filter(|p| p.as_os_str().len() > 0)
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    };
    // GNU df
    if let Ok(out) = std::process::Command::new("df")
        .args(["-k", "--output=avail"])
        .arg(&target)
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(avail_kb) = text
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty() && *l != "Avail" && l.chars().all(|c| c.is_ascii_digit()))
            {
                if let Ok(kb) = avail_kb.parse::<u64>() {
                    return Some(kb as f64 / 1_024.0 / 1_024.0);
                }
            }
        }
    }
    // POSIX-ish: Filesystem 1024-blocks Used Available ...
    let out = std::process::Command::new("df")
        .args(["-k"])
        .arg(&target)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().nth(1)?;
    let avail_kb: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    Some(avail_kb as f64 / 1_024.0 / 1_024.0)
}

pub fn utc_now_iso() -> String {
    // Prefer chronologically sortable wall time without adding a time crate:
    // format via `date -u` when available, else a coarse fallback.
    if let Ok(out) = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    format!(
        "unix:{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrip_json() {
        let dir = std::env::temp_dir().join(format!(
            "taikyoku-status-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("status.json");
        let status = RunStatus {
            running: true,
            games_completed: 3,
            games_failed: 1,
            batches_completed: 1,
            started_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:01:00Z".into(),
            last_game_id: Some("g1".into()),
            last_error: None,
            config: WorkerDaemonConfig {
                black: "ab".into(),
                white: "ab".into(),
                depth: Some(2),
                model: None,
                qdepth: None,
                max_time_ms: None,
                jobs: 2,
                batch: 4,
                starts: "opening".into(),
                outdir: "data/raw/games".into(),
                seed_base: 1,
                max_moves: 100,
            },
            disk_free_gb: Some(12.5),
            next_seed: 5,
            stop_requested: false,
        };
        status.write_path(&path).unwrap();
        let loaded = RunStatus::load_path(&path).unwrap();
        assert_eq!(loaded.games_completed, 3);
        assert_eq!(loaded.last_game_id.as_deref(), Some("g1"));
        assert!(loaded.running);
        let _ = fs::remove_dir_all(&dir);
    }
}
