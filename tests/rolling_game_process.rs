#![cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use taikyoku_shogi::training::{
    game_process::{self, EngineBundle},
    worker::WorkerConfig,
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "rolling-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn bundle(&self, name: &str) -> EngineBundle {
        let p = self.0.join(name);
        fs::write(&p, r#"#!/usr/bin/env python3
import json,sys,time,pathlib,os
r=json.load(open(sys.argv[2])); root=pathlib.Path(sys.argv[0]).parent; name=pathlib.Path(sys.argv[0]).name
(root/(name+'.started')).write_text(str(os.getpid()))
while not (root/(name+'.release')).exists(): time.sleep(.01)
if (root/(name+'.crash')).exists(): sys.exit(7)
c=r['config']
record=dict(format_version=2,game_id=name,seed=c['seed'],black=c['black'],white=c['white'],start=c['start'],moves=[],result='Draw')
json.dump(record,open(sys.argv[3],'w'))
"#).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        let hash = game_process::file_hash(&p).unwrap();
        EngineBundle {
            protocol: 1,
            revision: name.into(),
            engine_bin: p.display().to_string(),
            engine_sha256: hash.clone(),
            analyzer_bin: p.display().to_string(),
            analyzer_sha256: hash,
        }
    }
    fn select(&self, b: &EngineBundle) -> PathBuf {
        let p = self.0.join("active.json");
        let t = self.0.join("next.json");
        fs::write(&t, serde_json::to_vec(b).unwrap()).unwrap();
        fs::rename(t, &p).unwrap();
        p
    }
    fn release(&self, n: &str) {
        fs::write(self.0.join(format!("{n}.release")), "").unwrap();
    }
    fn wait(&self, n: &str) {
        let until = Instant::now() + Duration::from_secs(10);
        while !self.0.join(format!("{n}.started")).exists() {
            assert!(Instant::now() < until);
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
fn update_and_rollback_only_affect_next_game_and_preserve_frozen_agents() {
    let f = Fixture::new();
    let a = f.bundle("a");
    let b = f.bundle("b");
    let pointer = f.select(&a);
    let mut cfg = WorkerConfig::default();
    cfg.seed = 42;
    cfg.white.engine = Some(a.engine_bin.clone());
    let dir = f.0.clone();
    let first = cfg.clone();
    let worker = std::thread::spawn(move || game_process::play(&first, &pointer, &dir, 1));
    f.wait("a");
    f.select(&b);
    f.release("a");
    let old = worker.join().unwrap().unwrap();
    assert_eq!(old.black.engine_build, Some(a.clone()));
    assert_eq!(old.white.engine.as_ref(), Some(&a.engine_bin));
    assert!(old.white.engine_build.is_none());
    assert_eq!(old.white.engine_sha256.as_ref(), Some(&a.engine_sha256));
    f.release("b");
    let new = game_process::play(&cfg, &f.0.join("active.json"), &f.0, 2).unwrap();
    assert_eq!(new.black.engine_build, Some(b));
    assert_eq!(new.white.engine, old.white.engine);
    f.select(&a);
    let back = game_process::play(&cfg, &f.0.join("active.json"), &f.0, 3).unwrap();
    assert_eq!(back.black.engine_build, Some(a));
}

#[test]
fn stop_kills_child_and_crash_cannot_reuse_stale_result() {
    let f = Fixture::new();
    let a = f.bundle("a");
    let pointer = f.select(&a);
    let stop = Arc::new(AtomicBool::new(false));
    let cfg = WorkerConfig {
        stop: Some(stop.clone()),
        ..Default::default()
    };
    let dir = f.0.clone();
    let worker = std::thread::spawn(move || game_process::play(&cfg, &pointer, &dir, 1));
    f.wait("a");
    let pid: i32 = fs::read_to_string(f.0.join("a.started"))
        .unwrap()
        .parse()
        .unwrap();
    stop.store(true, Ordering::Relaxed);
    assert_eq!(worker.join().unwrap().unwrap_err(), "stopped");
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    fs::write(f.0.join("slot-1.result.json"), "stale result").unwrap();
    f.release("a");
    fs::write(f.0.join("a.crash"), "").unwrap();
    let error = game_process::play(&WorkerConfig::default(), &f.0.join("active.json"), &f.0, 1)
        .unwrap_err();
    assert!(error.contains("game worker exited"));
    assert!(!f.0.join("slot-1.result.json").exists());
}

#[test]
fn real_game_child_matches_in_process_and_records_build() {
    let f = Fixture::new();
    let engine = Path::new(env!("CARGO_BIN_EXE_taikyoku_shogi"));
    let hash = game_process::file_hash(engine).unwrap();
    let b = EngineBundle {
        protocol: 1,
        revision: "test".into(),
        engine_bin: engine.display().to_string(),
        engine_sha256: hash.clone(),
        analyzer_bin: engine.display().to_string(),
        analyzer_sha256: hash,
    };
    let pointer = f.select(&b);
    let mut cfg = WorkerConfig::default();
    cfg.max_moves = 2;
    cfg.seed = 17;
    use taikyoku_shogi::{
        board_position::BoardPosition,
        piece::{Color, Piece, PieceType},
        position::Position,
        training::record::GameStart,
    };
    cfg.start = GameStart::Position {
        position: BoardPosition {
            pieces: vec![
                Piece::new(PieceType::King, Color::Black, Position::new(0, 0).unwrap()),
                Piece::new(
                    PieceType::King,
                    Color::White,
                    Position::new(35, 35).unwrap(),
                ),
                Piece::new(
                    PieceType::Rook,
                    Color::Black,
                    Position::new(10, 10).unwrap(),
                ),
            ],
            turn: Color::Black,
            draw_counter: 0,
        },
    };
    cfg.black.depth = Some(1);
    cfg.white.depth = Some(1);
    cfg.black.quiescence_depth = Some(0);
    cfg.white.quiescence_depth = Some(0);
    let direct = taikyoku_shogi::training::worker::play_one_game(&cfg).unwrap();
    let separate = game_process::play(&cfg, &pointer, &f.0, 0).unwrap();
    assert_eq!(
        serde_json::to_value(&direct.moves).unwrap(),
        serde_json::to_value(&separate.moves).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&direct.result).unwrap(),
        serde_json::to_value(&separate.result).unwrap()
    );
    assert_eq!(separate.black.engine_build, Some(b));
}

#[test]
fn coordinator_resume_keeps_completed_games_and_ratings() {
    use taikyoku_shogi::training::tournament::{
        run_tournament, SlotStatus, TourneyConfig, TourneyEntrant, TourneyFormat,
    };
    struct Environment;
    impl Drop for Environment {
        fn drop(&mut self) {
            std::env::remove_var("TAIKYOKU_ENGINE_POINTER");
        }
    }
    let f = Fixture::new();
    let a = f.bundle("a");
    let pointer = f.select(&a);
    f.release("a");
    std::env::set_var("TAIKYOKU_ENGINE_POINTER", &pointer);
    let _env = Environment;
    let mut cfg = TourneyConfig {
        run_id: "fixture".into(),
        outdir: f.0.clone(),
        starts_spec: "opening".into(),
        games_per_pair: 1,
        jobs: 1,
        max_moves: 1,
        format: TourneyFormat::RoundRobin,
        stop_file: f.0.join("stop"),
        entrants: vec![
            TourneyEntrant {
                id: "a".into(),
                model: "dummy-a".into(),
                engine: None,
            },
            TourneyEntrant {
                id: "b".into(),
                model: "dummy-b".into(),
                engine: None,
            },
        ],
        ..Default::default()
    };
    let first = run_tournament(&cfg).unwrap();
    assert!(first.slots.iter().all(|s| s.status == SlotStatus::Done));
    let saved: Vec<_> = first
        .slots
        .iter()
        .map(|s| fs::read(s.game_path.as_ref().unwrap()).unwrap())
        .collect();
    cfg.resume = true;
    let resumed = run_tournament(&cfg).unwrap();
    assert_eq!(
        serde_json::to_value(&first.ratings).unwrap(),
        serde_json::to_value(&resumed.ratings).unwrap()
    );
    assert_eq!(first.slots.len(), resumed.slots.len());
    for (slot, bytes) in resumed.slots.iter().zip(saved) {
        assert_eq!(fs::read(slot.game_path.as_ref().unwrap()).unwrap(), bytes);
    }
}
