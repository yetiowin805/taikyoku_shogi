//! CLI handlers for the local training pipeline.

use crate::board_position::BoardPosition;
use crate::training::featurize::{featurize_dir, FeaturizeConfig};
use crate::training::match_harness::{run_matches, MatchConfig};
use crate::training::mobility_seed::{run_mobility_seed, MobilitySeedConfig};
use crate::training::paths::{self, ensure_data_dirs};
use crate::training::pool::{generate_pool, load_starts_dir, PoolGenerateConfig};
use crate::training::record::{AgentSpec, GameStart};
use crate::training::texel::{fit_texel, TexelFitConfig};
use crate::training::worker::{play_one_game, BatchSummary, WorkerConfig, DEFAULT_MAX_MOVES};
use std::path::{Path, PathBuf};

pub fn print_training_usage() {
    println!("Training / Texel pipeline:");
    println!("  worker run   --black AGENT --white AGENT [--model PATH] [--depth N]");
    println!("               [--start opening|PATH] [--seed S] [--out PATH] [--verbose]");
    println!("  worker batch --games N [--starts DIR|opening] [--outdir DIR] [--seed-base S]");
    println!("               [--black AGENT] [--white AGENT] [--model PATH] [--depth N] [--jobs J]");
    println!("  pool generate [--count K] [--seed-base S] [--outdir DIR]");
    println!("                [--from-play] [--agent AGENT] [--until-move N] [--noise F]");
    println!("                (default: Fischer mirrored shuffle + ablations; --from-play = legacy)");
    println!("  featurize [--games-dir DIR] [--out DIR] [--stride N] [--all-positions]");
    println!("  mobility-seed [--samples N] [--seed S] [--starts DIR] [--out PATH]");
    println!("  texel-fit [--features DIR] [--out PATH] [--iters N] [--lr F]");
    println!("  match --a AGENT --b AGENT [--starts DIR] [--games N] [--outdir DIR] [--seed-base S]");
    println!();
    println!("  Agents: mi, random, royal, ab");
    println!("  Data layout: {} / {} / {}", paths::RAW_GAMES, paths::RAW_STARTS, paths::DERIVED_POSITIONS);
    println!("  Typical: pool generate --count 64 && worker batch --games 64 --starts {} --jobs 8 --black ab --white ab", paths::RAW_STARTS);
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
        _ => {
            print_training_usage();
            Err("Usage: worker run|batch ...".into())
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

    std::fs::create_dir_all(&outdir).map_err(|e| format!("outdir: {}", e))?;

    let starts: Vec<GameStart> = if starts_spec == "opening" {
        vec![GameStart::Opening]
    } else {
        let loaded = load_starts_dir(Path::new(&starts_spec))?;
        if loaded.is_empty() {
            return Err(format!("No starts in {}", starts_spec));
        }
        loaded
            .into_iter()
            .map(|(_, p)| GameStart::Position { position: p })
            .collect()
    };

    let black_spec = agent_spec(&black, depth, model.clone(), qdepth);
    let white_spec = agent_spec(&white, depth, model, qdepth);

    let summary_mu = std::sync::Mutex::new(BatchSummary::default());
    let next = std::sync::Mutex::new(0usize);
    let errors = std::sync::Mutex::new(Vec::<String>::new());

    std::thread::scope(|scope| {
        for _ in 0..jobs {
            let summary_mu = &summary_mu;
            let next = &next;
            let errors = &errors;
            let starts = &starts;
            let outdir = &outdir;
            let black_spec = &black_spec;
            let white_spec = &white_spec;
            scope.spawn(move || loop {
                let g = {
                    let mut n = next.lock().unwrap();
                    if *n >= games {
                        return;
                    }
                    let g = *n;
                    *n += 1;
                    g
                };
                let start = starts[g % starts.len()].clone();
                let cfg = WorkerConfig {
                    black: black_spec.clone(),
                    white: white_spec.clone(),
                    start,
                    seed: seed_base.wrapping_add(g as u64),
                    max_moves,
                    verbose: verbose && jobs == 1,
                };
                match play_one_game(&cfg) {
                    Ok(record) => {
                        let path = Path::new(outdir).join(format!("{}.json", record.game_id));
                        if let Err(e) = record.save_path(&path) {
                            errors.lock().unwrap().push(e);
                            continue;
                        }
                        summary_mu
                            .lock()
                            .unwrap()
                            .record(&record.result, record.stats.move_count);
                        if verbose && jobs == 1 {
                            println!("Game {} -> {}", g + 1, path.display());
                        }
                    }
                    Err(e) => errors.lock().unwrap().push(format!("game {}: {}", g, e)),
                }
            });
        }
    });

    for e in errors.into_inner().unwrap() {
        eprintln!("{}", e);
    }
    let summary = summary_mu.into_inner().unwrap();

    summary.print();
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
