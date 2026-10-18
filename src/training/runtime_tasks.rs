//! Durable scheduler commands. The coordinator is the only writer of tournament state.
use super::game_process::file_hash;
use super::tournament::{save_state, TourneyConfig, TourneyEntrant, TourneyState};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

pub const PROTOCOL: u32 = 1;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ControlState {
    pub assignments_paused: bool,
    pub pause_reason: String,
    pub receipts: BTreeMap<String, Receipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub sha256: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol: u32,
    run_id: String,
    command: Command,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Admit { entrants: Vec<ValidatedEntrant> },
    PauseAssignments { reason: String },
    ResumeAssignments,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidatedEntrant {
    entrant: TourneyEntrant,
    /// Includes the descriptor and every content-pinned NNUE dependency.
    files: BTreeMap<String, String>,
}

fn apply(state: &mut TourneyState, request: Request) -> Result<(), String> {
    if request.protocol != PROTOCOL || request.run_id != state.run_id {
        return Err("task protocol or run identity mismatch".into());
    }
    match request.command {
        Command::PauseAssignments { reason } => {
            if reason.trim().is_empty() {
                return Err("pause requires a reason".into());
            }
            state.runtime_control.assignments_paused = true;
            state.runtime_control.pause_reason = reason;
        }
        Command::ResumeAssignments => {
            state.runtime_control.assignments_paused = false;
            state.runtime_control.pause_reason.clear();
        }
        Command::Admit { entrants } => {
            if entrants.is_empty() {
                return Err("admission requires entrants".into());
            }
            for item in entrants {
                let entrant = item.entrant;
                if entrant.id.is_empty()
                    || entrant.engine.is_some()
                    || state.retired.contains(&entrant.id)
                {
                    return Err("invalid, historical, or retired entrant identity".into());
                }
                if !item.files.contains_key(&entrant.model) {
                    return Err("model descriptor is not content pinned".into());
                }
                for (path, expected) in &item.files {
                    if !Path::new(path).is_absolute() || file_hash(Path::new(path))? != *expected {
                        return Err(format!("model content changed: {path}"));
                    }
                }
                // Check that the request also pins the dependency named by the descriptor.
                let descriptor: serde_json::Value =
                    serde_json::from_slice(&fs::read(&entrant.model).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                if let Some(nnue) = descriptor.get("weights").and_then(|w| w.get("nnue")) {
                    let file = nnue["file"].as_str().ok_or("NNUE file missing")?;
                    let sha = nnue["sha256"].as_str().ok_or("NNUE hash missing")?;
                    if item.files.get(file).map(String::as_str) != Some(sha) {
                        return Err("NNUE dependency is not content pinned".into());
                    }
                }
                if let Some(old) = state.entrants.iter().find(|e| e.id == entrant.id) {
                    if old.model != entrant.model || old.engine != entrant.engine {
                        return Err(format!("entrant identity already bound: {}", entrant.id));
                    }
                } else {
                    state.entrants.push(entrant);
                }
            }
        }
    }
    Ok(())
}

fn publish_receipt(
    run: &Path,
    id: &str,
    state: &TourneyState,
    receipt: &Receipt,
) -> Result<(), String> {
    // State is authoritative. Replay repairs these derived outputs after a crash.
    super::durable::write(
        &run.join("analysis/manifest.json"),
        &serde_json::to_vec(&serde_json::json!({"entrants":state.entrants}))
            .map_err(|e| e.to_string())?,
        false,
    )?;
    super::durable::write(
        &run.join("analysis/tasks/receipts")
            .join(format!("{id}.json")),
        &serde_json::to_vec(receipt).map_err(|e| e.to_string())?,
        false,
    )
}

/// Called under the same mutex as slot claims and game-result commits.
pub fn process(cfg: &TourneyConfig, state: &mut TourneyState) -> Result<(), String> {
    let run = cfg.outdir.join(&cfg.run_id);
    let inbox = run.join("analysis/tasks/requests");
    if !inbox.exists() {
        return Ok(());
    }
    let mut paths: Vec<_> = fs::read_dir(inbox)
        .map_err(|e| e.to_string())?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    paths.sort();
    for path in paths {
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("invalid task filename")?;
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        if let Some(receipt) = state.runtime_control.receipts.get(id) {
            if receipt.sha256 != hash {
                return Err(format!("immutable task was modified: {id}"));
            }
            if !run
                .join("analysis/tasks/receipts")
                .join(format!("{id}.json"))
                .exists()
            {
                publish_receipt(&run, id, state, receipt)?;
            }
            continue;
        }
        let mut candidate = state.clone();
        let result = serde_json::from_slice::<Request>(&bytes)
            .map_err(|e| e.to_string())
            .and_then(|req| apply(&mut candidate, req));
        if result.is_err() {
            candidate = state.clone();
        }
        let receipt = Receipt {
            sha256: hash,
            status: if result.is_ok() {
                "applied"
            } else {
                "rejected"
            }
            .into(),
            error: result.err(),
        };
        candidate
            .runtime_control
            .receipts
            .insert(id.into(), receipt.clone());
        // One durable commit includes the mutation and idempotency receipt. Never
        // replace a newer result ledger with an external process's stale snapshot.
        super::durable::write(
            &run.join("state.json"),
            &serde_json::to_vec(&candidate).map_err(|e| e.to_string())?,
            true,
        )?;
        *state = candidate;
        save_state(cfg, state)?;
        publish_receipt(&run, id, state, &receipt)?;
    }
    Ok(())
}

pub struct TaskService {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    ready: PathBuf,
}

impl TaskService {
    pub fn start(
        cfg: TourneyConfig,
        state: Arc<Mutex<TourneyState>>,
        failure: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let dir = cfg.outdir.join(&cfg.run_id).join("analysis/tasks");
        fs::create_dir_all(dir.join("requests")).map_err(|e| e.to_string())?;
        fs::create_dir_all(dir.join("receipts")).map_err(|e| e.to_string())?;
        let ready = dir.join("ready.json");
        // Recover committed requests before advertising readiness. A repaired
        // restart clears a prior fatal error only after recovery succeeds.
        if ready.exists() {
            fs::remove_file(&ready).map_err(|e| e.to_string())?;
        }
        process(&cfg, &mut state.lock().unwrap())?;
        let error_path = dir.join("error.json");
        if error_path.exists() {
            fs::remove_file(&error_path).map_err(|e| e.to_string())?;
        }
        let stat = fs::read_to_string("/proc/self/stat").map_err(|e| e.to_string())?;
        let identity = stat
            .rsplit_once(") ")
            .ok_or("invalid process identity")?
            .1
            .split_whitespace()
            .nth(19)
            .ok_or("missing process identity")?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                let result = process(&cfg, &mut state.lock().unwrap());
                if let Err(error) = result {
                    eprintln!("scheduler task failed: {error}; new game assignments stopped");
                    failure.store(true, Ordering::Relaxed);
                    let _ = super::durable::write(
                        &error_path,
                        &serde_json::to_vec(&serde_json::json!({"error":error})).unwrap(),
                        false,
                    );
                    return;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        });
        let service = Self {
            stop,
            thread: Some(thread),
            ready,
        };
        super::durable::write(&service.ready,&serde_json::to_vec(&serde_json::json!({"protocol":PROTOCOL,"pid":std::process::id(),"identity":identity})).map_err(|e|e.to_string())?,false)?;
        Ok(service)
    }
}
impl Drop for TaskService {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_file(&self.ready);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::training::tournament::{
        build_schedule, claim_for_worker, SlotStatus, TourneyFormat,
    };
    use serde_json::json;
    use std::sync::Barrier;

    struct Fixture {
        cfg: TourneyConfig,
        run: PathBuf,
        state: TourneyState,
    }
    impl Fixture {
        fn new(name: &str) -> Self {
            let out =
                std::env::temp_dir().join(format!("runtime-tasks-{}-{name}", std::process::id()));
            let cfg = TourneyConfig {
                outdir: out,
                run_id: "run".into(),
                format: TourneyFormat::Swiss,
                entrants: (0..3)
                    .map(|i| TourneyEntrant {
                        id: format!("old{i}"),
                        model: format!("/old{i}"),
                        engine: None,
                    })
                    .collect(),
                ..Default::default()
            };
            let run = cfg.outdir.join("run");
            for dir in ["requests", "receipts"] {
                fs::create_dir_all(run.join("analysis/tasks").join(dir)).unwrap();
            }
            let state = build_schedule(&cfg);
            save_state(&cfg, &state).unwrap();
            Self { cfg, run, state }
        }
        fn command(&self, id: &str, command: serde_json::Value) {
            super::super::durable::write(
                &self
                    .run
                    .join("analysis/tasks/requests")
                    .join(format!("{id}.json")),
                &serde_json::to_vec(&json!({"protocol":1,"run_id":"run","command":command}))
                    .unwrap(),
                false,
            )
            .unwrap();
        }
        fn entrant(&self, id: &str) -> serde_json::Value {
            let path = self.run.join(format!("{id}.json"));
            fs::write(&path, b"{}").unwrap();
            json!({"entrant":{"id":id,"model":path},"files":{path.to_str().unwrap():file_hash(&path).unwrap()}})
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.cfg.outdir);
        }
    }

    #[test]
    fn admits_while_games_run_and_recovers_missing_receipt_without_losing_results() {
        let mut f = Fixture::new("recovery");
        claim_for_worker(&mut f.state, &f.cfg, 0, true).unwrap();
        let before = serde_json::to_value(&f.state).unwrap();
        f.command("001", json!({"kind":"admit","entrants":[f.entrant("new")]}));
        process(&f.cfg, &mut f.state).unwrap();
        let after = serde_json::to_value(&f.state).unwrap();
        for field in ["slots", "ratings", "knockouts", "retired", "top_two_pairs"] {
            assert_eq!(before[field], after[field]);
        }
        assert_eq!(f.state.entrants.len(), 4);
        // Crash after the authoritative commit, before the acknowledgement.
        fs::remove_file(f.run.join("analysis/tasks/receipts/001.json")).unwrap();
        f.state = serde_json::from_slice(&fs::read(f.run.join("state.json")).unwrap()).unwrap();
        f.state.slots[0].status = SlotStatus::Done;
        f.state.slots[0].score_a = Some(1.0);
        save_state(&f.cfg, &f.state).unwrap();
        process(&f.cfg, &mut f.state).unwrap();
        assert_eq!(f.state.entrants.len(), 4);
        assert_eq!(f.state.slots[0].score_a, Some(1.0));
        assert!(f.run.join("analysis/tasks/receipts/001.json").exists());
        // The real top-two scheduler can discover a newly disconnected entrant.
        let id = claim_for_worker(&mut f.state, &f.cfg, 2, true).unwrap();
        let slot = f.state.slots.iter().find(|s| s.id == id).unwrap();
        assert!(slot.model_a == "new" || slot.model_b == "new");
    }

    #[test]
    fn rejected_batch_is_atomic_and_modified_task_id_cannot_reapply() {
        let mut f = Fixture::new("rejected");
        let valid = f.entrant("new");
        let mut bad = f.entrant("bad");
        bad["files"] = json!({});
        f.command("001", json!({"kind":"admit","entrants":[valid,bad]}));
        process(&f.cfg, &mut f.state).unwrap();
        assert_eq!(f.state.entrants.len(), 3);
        assert_eq!(f.state.runtime_control.receipts["001"].status, "rejected");
        f.command("001", json!({"kind":"resume_assignments"}));
        assert!(process(&f.cfg, &mut f.state)
            .unwrap_err()
            .contains("modified"));
    }

    #[test]
    fn concurrent_workers_cannot_reclaim_after_pause_receipt_and_admission_needs_no_idle_cpu() {
        let mut f = Fixture::new("concurrent");
        f.cfg.jobs = 8;
        f.cfg.entrants = (0..20)
            .map(|i| TourneyEntrant {
                id: format!("old{i}"),
                model: format!("/old{i}"),
                engine: None,
            })
            .collect();
        f.state = build_schedule(&f.cfg);
        let state = Arc::new(Mutex::new(f.state.clone()));
        // All eight workers have live games. Their results must survive admission.
        for worker in 0..8 {
            claim_for_worker(&mut state.lock().unwrap(), &f.cfg, worker, true).unwrap();
        }
        let barrier = Arc::new(Barrier::new(9));
        std::thread::scope(|scope| {
            for worker in 0..8 {
                let state = state.clone();
                let barrier = barrier.clone();
                let cfg = &f.cfg;
                scope.spawn(move || {
                    barrier.wait();
                    for _ in 0..100 {
                        let mut state = state.lock().unwrap();
                        assert!(claim_for_worker(&mut state, cfg, worker, true).is_none());
                    }
                    let mut state = state.lock().unwrap();
                    let slot = state
                        .slots
                        .iter_mut()
                        .find(|s| s.status == SlotStatus::Running)
                        .unwrap();
                    slot.status = SlotStatus::Done;
                    slot.score_a = Some(0.5);
                });
            }
            f.command(
                "001",
                json!({"kind":"pause_assignments","reason":"maintenance"}),
            );
            f.command("002", json!({"kind":"admit","entrants":[f.entrant("new")]}));
            process(&f.cfg, &mut state.lock().unwrap()).unwrap();
            barrier.wait();
        });
        let mut state = state.lock().unwrap();
        assert_eq!(
            state
                .slots
                .iter()
                .filter(|s| s.status == SlotStatus::Done)
                .count(),
            8
        );
        assert_eq!(state.entrants.len(), 21);
        f.command("003", json!({"kind":"resume_assignments"}));
        process(&f.cfg, &mut state).unwrap();
        assert!(claim_for_worker(&mut state, &f.cfg, 0, true).is_some());
    }

    #[test]
    fn failed_commit_never_becomes_visible_and_can_retry() {
        let mut f = Fixture::new("disk-error");
        f.command("001", json!({"kind":"admit","entrants":[f.entrant("new")]}));
        fs::remove_file(f.run.join("state.json")).unwrap();
        fs::create_dir(f.run.join("state.json")).unwrap();
        assert!(process(&f.cfg, &mut f.state).is_err());
        assert_eq!(f.state.entrants.len(), 3);
        assert!(!f.run.join("analysis/tasks/receipts/001.json").exists());
        fs::remove_dir(f.run.join("state.json")).unwrap();
        process(&f.cfg, &mut f.state).unwrap();
        assert_eq!(f.state.entrants.len(), 4);
    }
}
