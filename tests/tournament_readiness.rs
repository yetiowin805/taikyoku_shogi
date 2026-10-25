use std::{
    fs,
    path::PathBuf,
    process::{Command, ExitStatus, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use taikyoku_shogi::eval::EvalCheckpoint;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "tournament-ready-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }

    fn run(&self, entrants: usize, ready: &std::path::Path) -> ExitStatus {
        let seed = self.0.join("seed.json");
        EvalCheckpoint::seed("readiness").save_path(&seed).unwrap();
        let manifest = self.0.join("manifest.json");
        fs::write(
            &manifest,
            serde_json::json!({"entrants": (0..entrants).map(|i| {
            serde_json::json!({"id": format!("agent-{i}"), "model": seed})
        }).collect::<Vec<_>>()} )
            .to_string(),
        )
        .unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_taikyoku_shogi"))
            .current_dir(&self.0)
            .env_remove("TAIKYOKU_ENGINE_POINTER")
            .env("TAIKYOKU_TOURNAMENT_READY_FILE", ready)
            .args(["tournament", "--manifest"])
            .arg(manifest)
            .args([
                "--run-id",
                "ready",
                "--format",
                "round_robin",
                "--jobs",
                "1",
                "--games-per-pair",
                "1",
                "--max-moves",
                "0",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                return status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("tournament failed to exit within 30 seconds");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn readiness_is_published_only_after_successful_initialization() {
    let f = Fixture::new();
    let ready = f.0.join("ready");
    assert!(!f.run(1, &ready).success());
    assert!(!ready.exists());
    assert!(f.run(2, &ready).success());
    let marker = fs::read_to_string(ready).unwrap();
    assert!(marker.starts_with("ready "));
    assert!(f.0.join("data/raw/tourney/ready/state.json").exists());
}

#[test]
fn failed_readiness_publication_is_a_startup_error() {
    let f = Fixture::new();
    assert!(!f.run(2, &f.0.join("missing/ready")).success());
}
