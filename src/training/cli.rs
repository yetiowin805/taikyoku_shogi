//! CLI handlers for the local training pipeline.

use crate::board_position::BoardPosition;
use crate::training::featurize::{featurize_dir, FeaturizeConfig};
use crate::training::match_harness::{run_matches, MatchConfig};
use crate::training::mobility_seed::{run_mobility_seed, MobilitySeedConfig};
use crate::training::paths::{self, ensure_data_dirs};
use crate::training::pool::{generate_pool, load_starts_dir, PoolGenerateConfig};
use crate::training::record::{AgentSpec, GameStart};
use crate::training::run_status::{
    disk_free_gb, utc_now_iso, RunStatus, WorkerDaemonConfig,
};
use crate::training::texel::{fit_texel, TexelFitConfig};
use crate::training::worker::{
    play_one_game, run_batch, BatchConfig, WorkerConfig, DEFAULT_MAX_MOVES,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn print_training_usage() {
    println!("Training / Texel pipeline:");
    println!("  worker run   --black AGENT --white AGENT [--model PATH] [--depth N]");
    println!("               [--start opening|PATH] [--seed S] [--out PATH] [--verbose]");
    println!("  worker batch --games N [--starts DIR|opening] [--outdir DIR] [--seed-base S]");
    println!("               [--black AGENT] [--white AGENT] [--model PATH] [--depth N] [--jobs J]");
    println!("  worker daemon [--batch N] [--jobs J] [--starts DIR] [--outdir DIR] [--seed-base S]");
    println!("                [--black AGENT] [--white AGENT] [--model PATH] [--depth N]");
    println!("                [--status PATH] [--sleep-secs N]   (SIGTERM drains current batch)");
    println!("  pool generate [--agent AGENT] [--until-move N] [--count K] [--noise F]");
    println!("                [--seed-base S] [--outdir DIR]");
    println!("  featurize [--games-dir DIR] [--out DIR] [--stride N] [--all-positions]");
    println!("  mobility-seed [--samples N] [--seed S] [--starts DIR] [--out PATH]");
    println!("  texel-fit [--features DIR] [--out PATH] [--iters N] [--lr F]");
    println!("  match --a AGENT --b AGENT [--starts DIR] [--games N] [--outdir DIR] [--seed-base S]");
    println!();
    println!("  Agents: mi, random, royal, ab");
    println!(
        "  Data layout: {} / {} / {} / {}",
        paths::RAW_GAMES,
        paths::RAW_STARTS,
        paths::DERIVED_POSITIONS,
        paths::DATA_RUN
    );
    println!("  Daemon status: {}", paths::RUN_STATUS);
}

fn parse_agent_flag(args: &[String], i: &mut usize, flag: &str) -> Result<Option<String>, String> {
    if args.get(*i).map(|s| s.as_str()) != Some(flag) {
        return Ok(None);
    }
    *i += 1;
    let v = args
        .get(*i)
        .ok_or_else(|| format!("Missing value for {}", flag))?
        .clone();
    *i += 1;
    Ok(Some(v))
}

fn take_flag_value(args: &[String], i: &mut usize, flag: &str) -> Result<Option<String>, String> {
    parse_agent_flag(args, i, flag)
}

fn take_u64(args: &[String], i: &mut usize, flag: &str) -> Result<Option<u64>, String> {
    if let Some(s) = take_flag_value(args, i, flag)? {
        Ok(Some(s.parse().map_err(|_| format!("Invalid {} value", flag))?))
    } else {
        Ok(None)
    }
}

fn take_u32(args: &[String], i: &mut usize, flag: &str) -> Result<Option<u32>, String> {
    if let Some(s) = take_flag_value(args, i, flag)? {
        Ok(Some(s.parse().map_err(|_| format!("Invalid {} value", flag))?))
    } else {
        Ok(None)
    }
}

fn take_usize(args: &[String], i: &mut usize, flag: &str) -> Result<Option<usize>, String> {
    if let Some(s) = take_flag_value(args, i, flag)? {
        Ok(Some(s.parse().map_err(|_| format!("Invalid {} value", flag))?))
    } else {
        Ok(None)
    }
}

fn take_f64(args: &[String], i: &mut usize, flag: &str) -> Result<Option<f64>, String> {
    if let Some(s) = take_flag_value(args, i, flag)? {
        Ok(Some(s.parse().map_err(|_| format!("Invalid {} value", flag))?))
    } else {
        Ok(None)
    }
}

fn take_f32(args: &[String], i: &mut usize, flag: &str) -> Result<Option<f32>, String> {
    if let Some(s) = take_flag_value(args, i, flag)? {
        Ok(Some(s.parse().map_err(|_| format!("Invalid {} value", flag))?))
    } else {
        Ok(None)
    }
}

fn agent_spec(name: &str, depth: Option<u32>, model: Option<String>, qdepth: Option<u32>) -> AgentSpec {
    let mut a = AgentSpec::new(name);
    a.depth = depth;
    a.model = model;
    a.quiescence_depth = qdepth;
    a
}

fn parse_start(s: &str) -> Result<GameStart, String> {
    if s == "opening" {
        return Ok(GameStart::Opening);
    }
    let pos = BoardPosition::load_path(Path::new(s))?;
    Ok(GameStart::Position { position: pos })
}

pub fn cmd_worker(args: &[String]) -> Result<(), String> {
    let sub = args.get(2).map(|s| s.as_str()).unwrap_or("");
    match sub {
        "run" => cmd_worker_run(args),
        "batch" => cmd_worker_batch(args),
        "daemon" => cmd_worker_daemon(args),
        _ => {
            print_training_usage();
            Err("Usage: worker run|batch|daemon ...".into())
        }
    }
}

fn cmd_worker_run(args: &[String]) -> Result<(), String> {
    ensure_data_dirs()?;
    let mut black = "ab".to_string();
    let mut white = "ab".to_string();
    let mut depth = None;
    let mut model = None;
    let mut qdepth = None;
    let mut start = GameStart::Opening;
    let mut seed = 1u64;
    let mut out: Option<PathBuf> = None;
    let mut verbose = false;
    let mut max_moves = DEFAULT_MAX_MOVES;

    let mut i = 3;
    while i < args.len() {
        if args[i] == "--verbose" {
            verbose = true;
            i += 1;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--black")? {
            black = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--white")? {
            white = v;
            continue;
        }
        if let Some(v) = take_u32(args, &mut i, "--depth")? {
            depth = Some(v);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--model")? {
            model = Some(v);
            continue;
        }
        if let Some(v) = take_u32(args, &mut i, "--qdepth")? {
            qdepth = Some(v);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--start")? {
            start = parse_start(&v)?;
            continue;
        }
        if let Some(v) = take_u64(args, &mut i, "--seed")? {
            seed = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--out")? {
            out = Some(PathBuf::from(v));
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--max-moves")? {
            max_moves = v;
            continue;
        }
        return Err(format!("Unknown flag {}", args[i]));
    }

    let cfg = WorkerConfig {
        black: agent_spec(&black, depth, model.clone(), qdepth),
        white: agent_spec(&white, depth, model, qdepth),
        start,
        seed,
        max_moves,
        verbose,
    };
    let record = play_one_game(&cfg)?;
    let path = out.unwrap_or_else(|| paths::game_path(&record.game_id));
    record.save_path(&path)?;
    println!(
        "Saved {} (result={:?}, moves={})",
        path.display(),
        record.result,
        record.stats.move_count
    );
    Ok(())
}

fn cmd_worker_batch(args: &[String]) -> Result<(), String> {
    ensure_data_dirs()?;
    let mut games = 1usize;
    let mut starts_spec = "opening".to_string();
    let mut outdir = paths::RAW_GAMES.to_string();
    let mut seed_base = 1u64;
    let mut black = "random".to_string();
    let mut white = "random".to_string();
    let mut depth = None;
    let mut model = None;
    let mut qdepth = None;
    let mut jobs = 1usize;
    let mut max_moves = DEFAULT_MAX_MOVES;
    let mut verbose = false;

    let mut i = 3;
    while i < args.len() {
        if args[i] == "--verbose" {
            verbose = true;
            i += 1;
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--games")? {
            games = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--starts")? {
            starts_spec = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--outdir")? {
            outdir = v;
            continue;
        }
        if let Some(v) = take_u64(args, &mut i, "--seed-base")? {
            seed_base = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--black")? {
            black = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--white")? {
            white = v;
            continue;
        }
        if let Some(v) = take_u32(args, &mut i, "--depth")? {
            depth = Some(v);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--model")? {
            model = Some(v);
            continue;
        }
        if let Some(v) = take_u32(args, &mut i, "--qdepth")? {
            qdepth = Some(v);
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--jobs")? {
            jobs = v.max(1);
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--max-moves")? {
            max_moves = v;
            continue;
        }
        return Err(format!("Unknown flag {}", args[i]));
    }

    let starts = load_game_starts(&starts_spec)?;
    let outcome = run_batch(&BatchConfig {
        games,
        starts,
        outdir,
        seed_base,
        black: agent_spec(&black, depth, model.clone(), qdepth),
        white: agent_spec(&white, depth, model, qdepth),
        jobs,
        max_moves,
        verbose,
    })?;
    for e in &outcome.errors {
        eprintln!("{}", e);
    }
    outcome.summary.print();
    Ok(())
}

fn load_game_starts(starts_spec: &str) -> Result<Vec<GameStart>, String> {
    if starts_spec == "opening" {
        return Ok(vec![GameStart::Opening]);
    }
    let loaded = load_starts_dir(Path::new(starts_spec))?;
    if loaded.is_empty() {
        return Err(format!("No starts in {}", starts_spec));
    }
    Ok(loaded
        .into_iter()
        .map(|(_, p)| GameStart::Position { position: p })
        .collect())
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn env_usize(name: &str, default: usize) -> Result<usize, String> {
    match std::env::var(name) {
        Ok(s) => s
            .parse()
            .map_err(|_| format!("Invalid env {}={}", name, s)),
        Err(_) => Ok(default),
    }
}

fn env_u64(name: &str, default: u64) -> Result<u64, String> {
    match std::env::var(name) {
        Ok(s) => s
            .parse()
            .map_err(|_| format!("Invalid env {}={}", name, s)),
        Err(_) => Ok(default),
    }
}

fn env_u32_opt(name: &str) -> Result<Option<u32>, String> {
    match std::env::var(name) {
        Ok(s) if s.is_empty() => Ok(None),
        Ok(s) => Ok(Some(
            s.parse()
                .map_err(|_| format!("Invalid env {}={}", name, s))?,
        )),
        Err(_) => Ok(None),
    }
}

fn stop_requested(flag: &AtomicBool) -> bool {
    if flag.load(Ordering::SeqCst) {
        return true;
    }
    Path::new(paths::RUN_STOP_FLAG).exists()
}

fn cmd_worker_daemon(args: &[String]) -> Result<(), String> {
    ensure_data_dirs()?;

    // Env defaults (systemd /etc/taikyoku/worker.env), then CLI overrides.
    let mut batch = env_usize("BATCH", 8)?;
    let mut jobs = env_usize("JOBS", 4)?.max(1);
    let mut starts_spec = env_or("STARTS", paths::RAW_STARTS);
    let mut outdir = env_or("OUTDIR", paths::RAW_GAMES);
    let mut seed_base = env_u64("SEED_BASE", 1)?;
    let mut black = env_or("BLACK", "ab");
    let mut white = env_or("WHITE", "ab");
    let mut depth = env_u32_opt("DEPTH")?;
    let mut model = std::env::var("MODEL").ok().filter(|s| !s.is_empty());
    let mut qdepth = env_u32_opt("QDEPTH")?;
    let mut max_moves = env_usize("MAX_MOVES", DEFAULT_MAX_MOVES)?;
    let mut status_file = env_or("STATUS", paths::RUN_STATUS);
    let mut sleep_secs = env_u64("SLEEP_SECS", 0)?;
    let mut verbose = false;

    let mut i = 3;
    while i < args.len() {
        if args[i] == "--verbose" {
            verbose = true;
            i += 1;
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--batch")? {
            batch = v.max(1);
            continue;
        }
        // Alias used in some docs / muscle memory.
        if let Some(v) = take_usize(args, &mut i, "--games")? {
            batch = v.max(1);
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--jobs")? {
            jobs = v.max(1);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--starts")? {
            starts_spec = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--outdir")? {
            outdir = v;
            continue;
        }
        if let Some(v) = take_u64(args, &mut i, "--seed-base")? {
            seed_base = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--black")? {
            black = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--white")? {
            white = v;
            continue;
        }
        if let Some(v) = take_u32(args, &mut i, "--depth")? {
            depth = Some(v);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--model")? {
            model = Some(v);
            continue;
        }
        if let Some(v) = take_u32(args, &mut i, "--qdepth")? {
            qdepth = Some(v);
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--max-moves")? {
            max_moves = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--status")? {
            status_file = v;
            continue;
        }
        if let Some(v) = take_u64(args, &mut i, "--sleep-secs")? {
            sleep_secs = v;
            continue;
        }
        return Err(format!("Unknown flag {}", args[i]));
    }

    // Clear leftover STOP from a previous stop request.
    let stop_path = Path::new(paths::RUN_STOP_FLAG);
    if stop_path.exists() {
        let _ = std::fs::remove_file(stop_path);
    }

    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = stop.clone();
        ctrlc::set_handler(move || {
            stop.store(true, Ordering::SeqCst);
            eprintln!("worker daemon: stop requested; draining current batch…");
        })
        .map_err(|e| format!("signal handler: {}", e))?;
    }

    let starts = load_game_starts(&starts_spec)?;
    let status_path = PathBuf::from(&status_file);
    let daemon_cfg = WorkerDaemonConfig {
        black: black.clone(),
        white: white.clone(),
        depth,
        model: model.clone(),
        qdepth,
        jobs,
        batch,
        starts: starts_spec.clone(),
        outdir: outdir.clone(),
        seed_base,
        max_moves,
    };

    let started_at = utc_now_iso();
    let mut games_completed = 0usize;
    let mut games_failed = 0usize;
    let mut batches_completed = 0usize;
    let mut last_game_id: Option<String> = None;
    let mut last_error: Option<String> = None;
    let mut next_seed = seed_base;

    let write_status = |running: bool,
                        stop_requested: bool,
                        games_completed: usize,
                        games_failed: usize,
                        batches_completed: usize,
                        last_game_id: &Option<String>,
                        last_error: &Option<String>,
                        next_seed: u64|
     -> Result<(), String> {
        let status = RunStatus {
            running,
            games_completed,
            games_failed,
            batches_completed,
            started_at: started_at.clone(),
            updated_at: utc_now_iso(),
            last_game_id: last_game_id.clone(),
            last_error: last_error.clone(),
            config: daemon_cfg.clone(),
            disk_free_gb: disk_free_gb(Path::new(&outdir)),
            next_seed,
            stop_requested,
        };
        status.write_path(&status_path)
    };

    write_status(
        true,
        false,
        games_completed,
        games_failed,
        batches_completed,
        &last_game_id,
        &last_error,
        next_seed,
    )?;
    println!(
        "worker daemon: batch={} jobs={} starts={} outdir={} status={}",
        batch,
        jobs,
        starts_spec,
        outdir,
        status_path.display()
    );

    loop {
        if stop_requested(&stop) {
            break;
        }

        let outcome = run_batch(&BatchConfig {
            games: batch,
            starts: starts.clone(),
            outdir: outdir.clone(),
            seed_base: next_seed,
            black: agent_spec(&black, depth, model.clone(), qdepth),
            white: agent_spec(&white, depth, model.clone(), qdepth),
            jobs,
            max_moves,
            verbose,
        })?;

        for e in &outcome.errors {
            eprintln!("{}", e);
        }
        if let Some(e) = outcome.errors.last() {
            last_error = Some(e.clone());
        }
        games_completed += outcome.games_ok;
        games_failed += outcome.games_failed;
        batches_completed += 1;
        if outcome.last_game_id.is_some() {
            last_game_id = outcome.last_game_id;
        }
        next_seed = next_seed.wrapping_add(batch as u64);
        outcome.summary.print();

        write_status(
            true,
            stop_requested(&stop),
            games_completed,
            games_failed,
            batches_completed,
            &last_game_id,
            &last_error,
            next_seed,
        )?;

        if stop_requested(&stop) {
            break;
        }
        if sleep_secs > 0 {
            std::thread::sleep(std::time::Duration::from_secs(sleep_secs));
        }
    }

    write_status(
        false,
        true,
        games_completed,
        games_failed,
        batches_completed,
        &last_game_id,
        &last_error,
        next_seed,
    )?;
    if stop_path.exists() {
        let _ = std::fs::remove_file(stop_path);
    }
    println!(
        "worker daemon: stopped after {} games ({} failed, {} batches)",
        games_completed, games_failed, batches_completed
    );
    Ok(())
}

pub fn cmd_pool(args: &[String]) -> Result<(), String> {
    let sub = args.get(2).map(|s| s.as_str()).unwrap_or("");
    if sub != "generate" {
        return Err("Usage: pool generate ...".into());
    }
    let mut cfg = PoolGenerateConfig::default();
    let mut i = 3;
    while i < args.len() {
        if let Some(v) = take_flag_value(args, &mut i, "--agent")? {
            cfg.agent = AgentSpec::new(v);
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--until-move")? {
            cfg.until_move = v;
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--count")? {
            cfg.count = v;
            continue;
        }
        if let Some(v) = take_f64(args, &mut i, "--noise")? {
            cfg.noise = v;
            continue;
        }
        if let Some(v) = take_u64(args, &mut i, "--seed-base")? {
            cfg.seed_base = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--outdir")? {
            cfg.outdir = v;
            continue;
        }
        if let Some(v) = take_u32(args, &mut i, "--depth")? {
            cfg.agent.depth = Some(v);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--model")? {
            cfg.agent.model = Some(v);
            continue;
        }
        return Err(format!("Unknown flag {}", args[i]));
    }
    let ids = generate_pool(&cfg)?;
    println!("Wrote {} starts to {}", ids.len(), cfg.outdir);
    Ok(())
}

pub fn cmd_featurize(args: &[String]) -> Result<(), String> {
    let mut cfg = FeaturizeConfig::default();
    let mut i = 2;
    while i < args.len() {
        if args[i] == "--all-positions" {
            cfg.quiet_only = false;
            i += 1;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--games-dir")? {
            cfg.games_dir = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--out")? {
            cfg.out_dir = v;
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--stride")? {
            cfg.stride = v.max(1);
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--max-per-game")? {
            cfg.max_per_game = v;
            continue;
        }
        return Err(format!("Unknown flag {}", args[i]));
    }
    let n = featurize_dir(&cfg)?;
    println!("Wrote {} labeled positions to {}", n, cfg.out_dir);
    Ok(())
}

pub fn cmd_mobility_seed(args: &[String]) -> Result<(), String> {
    let mut cfg = MobilitySeedConfig::default();
    let mut starts_dir: Option<String> = None;
    let mut i = 2;
    while i < args.len() {
        if let Some(v) = take_usize(args, &mut i, "--samples")? {
            cfg.samples_per_piece = v;
            continue;
        }
        if let Some(v) = take_u64(args, &mut i, "--seed")? {
            cfg.seed = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--starts")? {
            starts_dir = Some(v);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--out")? {
            cfg.out_model = v;
            continue;
        }
        return Err(format!("Unknown flag {}", args[i]));
    }
    if let Some(dir) = starts_dir {
        cfg.starts = load_starts_dir(Path::new(&dir))?
            .into_iter()
            .map(|(_, p)| p)
            .collect();
    }
    let (cp, stats) = run_mobility_seed(&cfg)?;
    let sampled: usize = stats.iter().filter(|s| s.samples > 0).count();
    println!(
        "Wrote {} ({} piece types sampled, {} held at seed)",
        cfg.out_model,
        sampled,
        stats.len() - sampled
    );
    let _ = cp;
    Ok(())
}

pub fn cmd_texel_fit(args: &[String]) -> Result<(), String> {
    let mut cfg = TexelFitConfig::default();
    let mut i = 2;
    while i < args.len() {
        if let Some(v) = take_flag_value(args, &mut i, "--features")? {
            cfg.features_dir = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--out")? {
            cfg.out_model = v;
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--iters")? {
            cfg.iterations = v;
            continue;
        }
        if let Some(v) = take_f32(args, &mut i, "--lr")? {
            cfg.learning_rate = v;
            continue;
        }
        if let Some(v) = take_f32(args, &mut i, "--k")? {
            cfg.k = v;
            cfg.fit_k = false;
            continue;
        }
        return Err(format!("Unknown flag {}", args[i]));
    }
    let (_cp, loss) = fit_texel(&cfg)?;
    println!("Wrote {} (mean CE loss {:.6})", cfg.out_model, loss);
    Ok(())
}

pub fn cmd_match(args: &[String]) -> Result<(), String> {
    let mut cfg = MatchConfig::default();
    let mut i = 2;
    while i < args.len() {
        if args[i] == "--verbose" {
            cfg.verbose = true;
            i += 1;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--a")? {
            cfg.engine_a = AgentSpec::new(v);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--b")? {
            cfg.engine_b = AgentSpec::new(v);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--starts")? {
            cfg.starts_dir = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--outdir")? {
            cfg.outdir = v;
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--games")? {
            cfg.max_games = v;
            continue;
        }
        if let Some(v) = take_u64(args, &mut i, "--seed-base")? {
            cfg.seed_base = v;
            continue;
        }
        if let Some(v) = take_u32(args, &mut i, "--depth")? {
            cfg.engine_a.depth = Some(v);
            cfg.engine_b.depth = Some(v);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--model-a")? {
            cfg.engine_a.model = Some(v);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--model-b")? {
            cfg.engine_b.model = Some(v);
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--max-moves")? {
            cfg.max_moves = v;
            continue;
        }
        return Err(format!("Unknown flag {}", args[i]));
    }
    let _ = run_matches(&cfg)?;
    Ok(())
}
