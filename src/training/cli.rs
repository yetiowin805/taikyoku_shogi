//! CLI handlers for the local training pipeline.

use crate::board_position::BoardPosition;
use crate::training::eval_trace::{run_eval_trace, EvalTraceConfig};
use crate::training::featurize::{featurize_dir, FeaturizeConfig};
use crate::training::match_harness::{run_matches, MatchConfig};
use crate::training::mobility_seed::{run_mobility_seed, MobilitySeedConfig};
use crate::training::paths::{self, ensure_data_dirs};
use crate::training::pool::{generate_pool, load_starts_dir, parse_starts_spec, PoolGenerateConfig};
use crate::training::record::{AgentSpec, GameStart};
use crate::training::run_status::{
    disk_free_gb, utc_now_iso, RunStatus, WorkerDaemonConfig,
};
use crate::training::file_pst_grid::{run_file_pst_grid, FilePstGridConfig};
use crate::training::loud_grid::{run_loud_grid, LoudGridConfig};
use crate::training::pst_grid::{run_pst_grid, PstGridConfig};
use crate::training::scale_sample::{run_scale_sample, ScaleSampleConfig};
use crate::training::texel::{fit_texel, TexelFitConfig, TexelInit};
use crate::training::tournament::{
    load_manifest, new_run_id, run_tournament, standings_summary, TourneyConfig, TourneyFormat,
    DEFAULT_GAMES_PER_PAIR,
};
use crate::training::worker::{
    play_one_game, run_batch, BatchConfig, WorkerConfig, DEFAULT_MAX_MOVES,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Depth ceiling when `--time-ms` is set but `--depth` is omitted.
const DEFAULT_TIMED_DEPTH_CEILING: u32 = 8;

/// Resolve AB depth / time from CLI flags.
/// Time is a soft budget (ID returns the last completed depth); depth is a hard ceiling.
/// When `time_ms` is set and depth was not explicit, use [`DEFAULT_TIMED_DEPTH_CEILING`].
fn resolve_ab_depth(
    depth: Option<u32>,
    depth_explicit: bool,
    time_ms: Option<u64>,
) -> Option<u32> {
    if time_ms.is_some() && !depth_explicit {
        Some(DEFAULT_TIMED_DEPTH_CEILING)
    } else {
        depth
    }
}

pub fn print_training_usage() {
    println!("Training / Texel pipeline:");
    println!("  worker run   --black AGENT --white AGENT [--model PATH] [--depth N] [--time-ms MS]");
    println!("               [--start opening|PATH] [--seed S] [--out PATH] [--verbose]");
    println!("  worker batch --games N [--starts DIR|opening|random] [--outdir DIR] [--seed-base S]");
    println!("               [--black AGENT] [--white AGENT] [--model PATH] [--depth N] [--time-ms MS]");
    println!("               [--jobs J]");
    println!("  worker daemon [--batch N] [--jobs J] [--starts DIR|opening|random] [--outdir DIR] [--seed-base S]");
    println!("                [--black AGENT] [--white AGENT] [--model PATH] [--depth N] [--time-ms MS]");
    println!("                [--status PATH] [--sleep-secs N]   (SIGTERM drains current batch)");
    println!("  (--seed-base 0 = per-game OS entropy; N>0 = deterministic N+index)");
    println!("  (--time-ms = soft AB budget; ID keeps last completed depth. Omit --depth → ceiling 8)");
    println!("  pool generate [--count K] [--seed-base S] [--outdir DIR]");
    println!("                [--from-play] [--agent AGENT] [--until-move N] [--noise F]");
    println!("                (default: Fischer shuffle+ablations; --from-play = legacy midgame)");
    println!("  featurize [--games-dir DIR] [--out DIR] [--target-per-game N] [--quiet-stride N]");
    println!("            (event-driven sampling; default target 150/game)");
    println!("  eval-trace [--games-dir DIR] [--model PATH] [--out DIR] [--quiet-stride N]");
    println!("             [--top K] [--max-games N] [--clip F] [--exclude-tail N]");
    println!("             [--search-depth D] [--search-stride S] [--no-search]");
    println!("             (uses saved AB evals when present; clip±10000; drop last 8 plies)");
    println!("  mobility-seed [--samples N] [--seed S] [--starts DIR] [--out PATH]");
    println!("  scale-sample [--seed PATH] [--out DIR] [--n N] [--rng-seed S]");
    println!("            (copy seed + all_m10/all_p10 + random ±10% big-param models)");
    println!("  loud-grid [--seed PATH] [--out DIR]");
    println!("            (3×3×3: Hook H90/100/110 × Capricorn C80/100/120 × other O80/100/110)");
    println!("  pst-grid [--seed PATH] [--out DIR]");
    println!("            (3×3×3 fast PST: promo P110/120/130 × opp-half H25/50/75 × back B25/50/75)");
    println!("  file-pst-grid [--seed PATH] [--out DIR]");
    println!("            (5×3×3: file F×C × back B60/75/90 × tropism T10/15/20)");
    println!("  texel-fit [--features DIR] [--out PATH] [--iters N] [--lr F] [--k F]");
    println!("            [--init seed|mobility|PATH] [--late-frac F] [--keep-draws]");
    println!("            [--no-log-space] [--no-lr-scale-k] [--no-renorm-pawn]");
    println!("            (default: seed init, log-space, all plies+draws,");
    println!("             2500 iters, lr=0.05 scaled by 1/K, Pawn→1)");
    println!("  match --a AGENT --b AGENT [--starts SPEC] [--games N] [--jobs J] [--outdir DIR]");
    println!("  tournament --manifest PATH [--run-id ID] [--resume] [--games-per-pair N] [--jobs J]");
    println!("             [--starts light] [--depth N] [--time-ms MS] [--outdir DIR]");
    println!("             [--format round_robin|swiss]");
    println!("             (Swiss = continuous Glicko until TOURNEY_STOP / Ctrl-C;");
    println!("              RR default games-per-pair=24;");
    println!("              --time-ms soft budget, last completed ID depth; omit --depth → 8)");
    println!();
    println!("  Agents: mi, random, royal, ab");
    println!("  Starts: opening | random | light | DIR of pool JSON");
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

fn agent_spec(
    name: &str,
    depth: Option<u32>,
    model: Option<String>,
    qdepth: Option<u32>,
    max_time_ms: Option<u64>,
) -> AgentSpec {
    let mut a = AgentSpec::new(name);
    a.depth = depth;
    a.model = model;
    a.quiescence_depth = qdepth;
    a.max_time_ms = max_time_ms;
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
    let mut depth_explicit = false;
    let mut time_ms = None;
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
            depth_explicit = true;
            continue;
        }
        if let Some(v) = take_u64(args, &mut i, "--time-ms")? {
            time_ms = Some(v);
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

    let depth = resolve_ab_depth(depth, depth_explicit, time_ms);
    let cfg = WorkerConfig {
        black: agent_spec(&black, depth, model.clone(), qdepth, time_ms),
        white: agent_spec(&white, depth, model, qdepth, time_ms),
        start,
        seed,
        max_moves,
        verbose,
        stop: None,
    };
    let record = play_one_game(&cfg).map_err(|e| e.message)?;
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
    let mut depth_explicit = false;
    let mut time_ms = None;
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
            depth_explicit = true;
            continue;
        }
        if let Some(v) = take_u64(args, &mut i, "--time-ms")? {
            time_ms = Some(v);
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

    let depth = resolve_ab_depth(depth, depth_explicit, time_ms);
    let starts = parse_starts_spec(&starts_spec)?;
    let outdir_label = outdir.clone();
    let outcome = run_batch(&BatchConfig {
        games,
        starts,
        outdir,
        seed_base,
        black: agent_spec(&black, depth, model.clone(), qdepth, time_ms),
        white: agent_spec(&white, depth, model, qdepth, time_ms),
        jobs,
        max_moves,
        verbose,
    })?;
    for e in &outcome.errors {
        eprintln!("{}", e);
    }
    outcome.summary.print();
    if outcome.partials_saved > 0 {
        println!(
            "Saved {} partial game(s) under {}/partial/",
            outcome.partials_saved, outdir_label
        );
    }
    Ok(())
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

fn env_u64_opt(name: &str) -> Result<Option<u64>, String> {
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
    let mut seed_base = env_u64("SEED_BASE", 0)?;
    let mut black = env_or("BLACK", "ab");
    let mut white = env_or("WHITE", "ab");
    let mut depth = env_u32_opt("DEPTH")?;
    let mut depth_explicit = depth.is_some();
    let mut time_ms = env_u64_opt("TIME_MS")?;
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
            depth_explicit = true;
            continue;
        }
        if let Some(v) = take_u64(args, &mut i, "--time-ms")? {
            time_ms = Some(v);
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

    let depth = resolve_ab_depth(depth, depth_explicit, time_ms);

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

    let starts = parse_starts_spec(&starts_spec)?;
    let status_path = PathBuf::from(&status_file);
    let daemon_cfg = WorkerDaemonConfig {
        black: black.clone(),
        white: white.clone(),
        depth,
        model: model.clone(),
        qdepth,
        max_time_ms: time_ms,
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
        "worker daemon: batch={} jobs={} starts={} outdir={} status={} seeds={}",
        batch,
        jobs,
        starts_spec,
        outdir,
        status_path.display(),
        if seed_base == 0 {
            "os-random".to_string()
        } else {
            format!("base={}", seed_base)
        }
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
            black: agent_spec(&black, depth, model.clone(), qdepth, time_ms),
            white: agent_spec(&white, depth, model.clone(), qdepth, time_ms),
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
        next_seed = if seed_base == 0 {
            0
        } else {
            next_seed.wrapping_add(batch as u64)
        };
        outcome.summary.print();
        if outcome.partials_saved > 0 {
            println!(
                "Saved {} partial(s) under {}/partial/",
                outcome.partials_saved, outdir
            );
        }

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
        if args[i] == "--from-play" {
            cfg.from_play = true;
            i += 1;
            continue;
        }
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
    if cfg.from_play {
        println!("Wrote {} from-play starts to {}", ids.len(), cfg.outdir);
    } else {
        println!(
            "Wrote {} fischer starts (+ *.recipe.json) to {}",
            ids.len(),
            cfg.outdir
        );
    }
    Ok(())
}

pub fn cmd_featurize(args: &[String]) -> Result<(), String> {
    let mut cfg = FeaturizeConfig::default();
    let mut i = 2;
    while i < args.len() {
        if let Some(v) = take_flag_value(args, &mut i, "--games-dir")? {
            cfg.games_dir = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--out")? {
            cfg.out_dir = v;
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--target-per-game")? {
            cfg.target_per_game = v;
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--quiet-stride")? {
            cfg.quiet_stride = v.max(1);
            continue;
        }
        // Legacy aliases
        if let Some(v) = take_usize(args, &mut i, "--max-per-game")? {
            cfg.target_per_game = v;
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--stride")? {
            cfg.quiet_stride = v.max(1);
            continue;
        }
        if args[i] == "--all-positions" {
            // Keep all event candidates (no even subsample).
            cfg.target_per_game = 0;
            i += 1;
            continue;
        }
        return Err(format!("Unknown flag {}", args[i]));
    }
    let n = featurize_dir(&cfg)?;
    println!("Wrote {} labeled positions to {}", n, cfg.out_dir);
    Ok(())
}

pub fn cmd_eval_trace(args: &[String]) -> Result<(), String> {
    let mut cfg = EvalTraceConfig::default();
    let mut i = 2;
    while i < args.len() {
        if let Some(v) = take_flag_value(args, &mut i, "--games-dir")? {
            cfg.games_dir = PathBuf::from(v);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--model")? {
            cfg.model_path = PathBuf::from(v);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--out")? {
            cfg.out_dir = PathBuf::from(v);
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--quiet-stride")? {
            cfg.quiet_stride = v.max(1);
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--top")? {
            cfg.top_n = v.max(1);
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--max-games")? {
            cfg.max_games = Some(v);
            continue;
        }
        if let Some(v) = take_f32(args, &mut i, "--clip")? {
            cfg.eval_clip = v;
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--exclude-tail")? {
            cfg.exclude_tail_plies = v;
            continue;
        }
        if let Some(v) = take_u32(args, &mut i, "--search-depth")? {
            cfg.search_depth = v;
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--search-stride")? {
            cfg.search_stride = v.max(1);
            continue;
        }
        if args.get(i).map(|s| s.as_str()) == Some("--no-search") {
            cfg.skip_search = true;
            i += 1;
            continue;
        }
        if args.get(i).map(|s| s.as_str()) == Some("--keep-noise") {
            cfg.zero_noise = false;
            i += 1;
            continue;
        }
        return Err(format!("Unknown flag {}", args[i]));
    }
    let (n, md) = run_eval_trace(&cfg)?;
    println!(
        "Traced {n} games → {} (ranked list: {}; search depth={} stride={} on top {})",
        cfg.out_dir.display(),
        md.display(),
        cfg.search_depth,
        cfg.search_stride,
        if cfg.skip_search { 0 } else { cfg.top_n }
    );
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

pub fn cmd_scale_sample(args: &[String]) -> Result<(), String> {
    let mut cfg = ScaleSampleConfig::default();
    let mut i = 2;
    while i < args.len() {
        if let Some(v) = take_flag_value(args, &mut i, "--seed")? {
            cfg.seed_model = PathBuf::from(v);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--out")? {
            cfg.out_dir = PathBuf::from(v);
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--n")? {
            cfg.n_samples = v;
            continue;
        }
        if let Some(v) = take_u64(args, &mut i, "--rng-seed")? {
            cfg.rng_seed = v;
            continue;
        }
        return Err(format!("Unknown flag {}", args[i]));
    }
    let (man, samples) = run_scale_sample(&cfg)?;
    println!(
        "Wrote {} entrants under {} ({} samples: all_m10, all_p10, + {} random; rng_seed={})",
        man.entrants.len(),
        cfg.out_dir.display(),
        samples.samples.len(),
        samples.samples.len().saturating_sub(2),
        cfg.rng_seed
    );
    Ok(())
}

pub fn cmd_loud_grid(args: &[String]) -> Result<(), String> {
    let mut cfg = LoudGridConfig::default();
    let mut i = 2;
    while i < args.len() {
        if let Some(v) = take_flag_value(args, &mut i, "--seed")? {
            cfg.seed_model = PathBuf::from(v);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--out")? {
            cfg.out_dir = PathBuf::from(v);
            continue;
        }
        return Err(format!("Unknown flag {}", args[i]));
    }
    let (man, grid) = run_loud_grid(&cfg)?;
    println!(
        "Wrote {} entrants under {} (hook={:?}, capricorn={:?}, other={:?})",
        man.entrants.len(),
        cfg.out_dir.display(),
        grid.hook_mover_pieces,
        grid.capricorn_pieces,
        grid.other_two_mover_pieces
    );
    Ok(())
}

pub fn cmd_pst_grid(args: &[String]) -> Result<(), String> {
    let mut cfg = PstGridConfig::default();
    let mut i = 2;
    while i < args.len() {
        if let Some(v) = take_flag_value(args, &mut i, "--seed")? {
            cfg.seed_model = PathBuf::from(v);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--out")? {
            cfg.out_dir = PathBuf::from(v);
            continue;
        }
        return Err(format!("Unknown flag {}", args[i]));
    }
    let (man, grid) = run_pst_grid(&cfg)?;
    println!(
        "Wrote {} entrants under {} (seed cell P120H50B75; sample opp_half={:.4})",
        man.entrants.len(),
        cfg.out_dir.display(),
        grid.cells
            .iter()
            .find(|c| c.id == "P130H25B50")
            .map(|c| c.opp_half_factor)
            .unwrap_or(0.0)
    );
    Ok(())
}

pub fn cmd_file_pst_grid(args: &[String]) -> Result<(), String> {
    let mut cfg = FilePstGridConfig::default();
    let mut i = 2;
    while i < args.len() {
        if let Some(v) = take_flag_value(args, &mut i, "--seed")? {
            cfg.seed_model = PathBuf::from(v);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--out")? {
            cfg.out_dir = PathBuf::from(v);
            continue;
        }
        return Err(format!("Unknown flag {}", args[i]));
    }
    let (man, _grid) = run_file_pst_grid(&cfg)?;
    println!(
        "Wrote {} entrants under {} (seed cell F100C100B75T15)",
        man.entrants.len(),
        cfg.out_dir.display()
    );
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
        if let Some(v) = take_flag_value(args, &mut i, "--init")? {
            cfg.init = match v.as_str() {
                "seed" => TexelInit::Seed,
                "mobility" => TexelInit::Mobility,
                path => TexelInit::Path(path.to_string()),
            };
            continue;
        }
        if let Some(v) = take_f32(args, &mut i, "--late-frac")? {
            cfg.late_frac = v;
            continue;
        }
        if args.get(i).map(|s| s.as_str()) == Some("--keep-draws") {
            cfg.drop_draws = false;
            i += 1;
            continue;
        }
        if args.get(i).map(|s| s.as_str()) == Some("--no-log-space") {
            cfg.log_space = false;
            i += 1;
            continue;
        }
        if args.get(i).map(|s| s.as_str()) == Some("--no-lr-scale-k") {
            cfg.lr_scale_by_k = false;
            i += 1;
            continue;
        }
        if args.get(i).map(|s| s.as_str()) == Some("--no-renorm-pawn") {
            cfg.renormalize_pawn = false;
            i += 1;
            continue;
        }
        return Err(format!("Unknown flag {}", args[i]));
    }
    let (_cp, stats) = fit_texel(&cfg)?;
    println!(
        "Wrote {} (CE {:.6} → {:.6}, k={:.4}, max|Δw|={:.4}, mean|Δw|={:.4}, max%Δ={:.1}, mean%Δ={:.1}, used={}/{}, agree={:.3})",
        cfg.out_model,
        stats.loss_before,
        stats.loss_after,
        stats.k,
        stats.max_abs_delta,
        stats.mean_abs_delta,
        stats.max_pct_delta,
        stats.mean_pct_delta,
        stats.n_used,
        stats.n_raw,
        stats.sign_agreement
    );
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
            cfg.starts_spec = v;
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
        if let Some(v) = take_usize(args, &mut i, "--jobs")? {
            cfg.jobs = v;
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

pub fn cmd_tournament(args: &[String]) -> Result<(), String> {
    let mut cfg = TourneyConfig::default();
    let mut manifest: Option<String> = None;
    let mut depth_explicit = false;
    let mut i = 2;
    while i < args.len() {
        if args[i] == "--verbose" {
            cfg.verbose = true;
            i += 1;
            continue;
        }
        if args[i] == "--resume" {
            cfg.resume = true;
            i += 1;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--manifest")? {
            manifest = Some(v);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--run-id")? {
            cfg.run_id = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--outdir")? {
            cfg.outdir = PathBuf::from(v);
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--starts")? {
            cfg.starts_spec = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--stop-file")? {
            cfg.stop_file = PathBuf::from(v);
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--games-per-pair")? {
            cfg.games_per_pair = v;
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--jobs")? {
            cfg.jobs = v;
            continue;
        }
        if let Some(v) = take_u64(args, &mut i, "--seed-base")? {
            cfg.seed_base = v;
            continue;
        }
        if let Some(v) = take_u32(args, &mut i, "--depth")? {
            cfg.depth = v;
            depth_explicit = true;
            continue;
        }
        if let Some(v) = take_u64(args, &mut i, "--time-ms")? {
            cfg.max_time_ms = Some(v);
            continue;
        }
        if let Some(v) = take_usize(args, &mut i, "--max-moves")? {
            cfg.max_moves = v;
            continue;
        }
        if let Some(v) = take_flag_value(args, &mut i, "--format")? {
            cfg.format = match v.as_str() {
                "round_robin" | "rr" => TourneyFormat::RoundRobin,
                "swiss" => TourneyFormat::Swiss,
                other => return Err(format!("Unknown --format {other} (round_robin|swiss)")),
            };
            continue;
        }
        if let Some(_v) = take_usize(args, &mut i, "--swiss-rounds")? {
            // Legacy flag: Swiss is continuous until stop; ignore finite rounds.
            continue;
        }
        return Err(format!("Unknown flag {}", args[i]));
    }
    if let Some(d) = resolve_ab_depth(Some(cfg.depth), depth_explicit, cfg.max_time_ms) {
        cfg.depth = d;
    }
    let manifest_path = manifest.ok_or("tournament requires --manifest PATH")?;
    let man = load_manifest(Path::new(&manifest_path))?;
    cfg.entrants = man.entrants;
    if cfg.run_id.is_empty() {
        cfg.run_id = new_run_id();
    }
    if cfg.format == TourneyFormat::Swiss {
        // Continuous until stop; games_per_pair unused for Swiss scheduling.
        if cfg.games_per_pair == DEFAULT_GAMES_PER_PAIR {
            cfg.games_per_pair = 1;
        }
    } else if cfg.games_per_pair == 0 {
        cfg.games_per_pair = DEFAULT_GAMES_PER_PAIR;
    }
    let state = run_tournament(&cfg)?;
    println!("{}", standings_summary(&state));
    Ok(())
}

#[cfg(test)]
mod timed_id_cli_tests {
    use super::*;

    #[test]
    fn time_without_depth_defaults_ceiling_8() {
        assert_eq!(
            resolve_ab_depth(None, false, Some(1000)),
            Some(DEFAULT_TIMED_DEPTH_CEILING)
        );
        assert_eq!(
            resolve_ab_depth(Some(2), false, Some(1000)),
            Some(DEFAULT_TIMED_DEPTH_CEILING)
        );
    }

    #[test]
    fn explicit_depth_kept_with_time() {
        assert_eq!(resolve_ab_depth(Some(4), true, Some(1000)), Some(4));
    }

    #[test]
    fn depth_only_leaves_time_none_and_depth() {
        assert_eq!(resolve_ab_depth(Some(2), true, None), Some(2));
        assert_eq!(resolve_ab_depth(None, false, None), None);
    }

    #[test]
    fn agent_spec_carries_time_ms() {
        let a = agent_spec("ab", Some(8), None, None, Some(1000));
        assert_eq!(a.depth, Some(8));
        assert_eq!(a.max_time_ms, Some(1000));
    }
}
