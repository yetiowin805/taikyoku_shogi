//! Isolated fixtures shared by unit tests; never write repository checkpoints.
use crate::eval::EvalCheckpoint;
use crate::training::{history::HistoryManifest, tournament::TourneyEntrant};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "taikyoku-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("create {}: {e}", path.display()),
            }
        }
    }

    pub fn path(&self) -> PathBuf {
        self.0.clone()
    }

    pub fn seed(&self) -> PathBuf {
        let path = self.0.join("input-seed.json");
        EvalCheckpoint::seed("ab-seed").save_path(&path).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Check the complete roster, including future history entries and duplicates.
pub fn assert_grid_roster<'a>(
    entrants: &[TourneyEntrant],
    cell_ids: impl Iterator<Item = &'a str>,
    manifest: &Path,
    skipped: &[&str],
) {
    let history = HistoryManifest::load_path(manifest).unwrap();
    let expected: BTreeSet<_> = cell_ids
        .map(str::to_owned)
        .chain(
            history
                .weights
                .iter()
                .chain(&history.engines)
                .filter(|entry| !skipped.contains(&entry.id.as_str()))
                .map(|entry| entry.id.clone()),
        )
        .collect();
    let actual: BTreeSet<_> = entrants.iter().map(|entry| entry.id.clone()).collect();
    assert_eq!(actual.len(), entrants.len(), "duplicate entrant IDs");
    assert_eq!(
        actual, expected,
        "grid cells plus eligible history must form the roster"
    );
}
