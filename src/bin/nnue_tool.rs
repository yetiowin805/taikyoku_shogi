//! Dataset export and cross-language NNUE verification. Does not train or launch games.
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, BufWriter, Write},
    path::Path,
};
use taikyoku_shogi::{
    eval::{evaluate, EvalCheckpoint},
    game_history::GameHistory,
    nnue::{self, features},
    piece::Color,
    search::search,
    training::{record::load_game_json, worker::replay_to_ply},
};
#[derive(Deserialize)]
struct Job {
    game: String,
    plies: Vec<usize>,
    split: String,
}
fn schema() -> serde_json::Value {
    json!({"version":1,"channels":features::CHANNELS,"features":features::FEATURES,"hash":format!("{:x}",sha2::digest::Output::<Sha256>::from(features::schema().hash)),"names_hash":format!("{:x}",Sha256::digest(serde_json::to_vec(&features::schema().names).unwrap())),"names":features::schema().names})
}
fn run() -> Result<(), String> {
    let a: Vec<_> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("schema") => println!("{}", schema()),
        Some("export") if a.len() == 5 => {
            let cp = EvalCheckpoint::load_path(&a[2])?;
            if cp.weights.nnue.is_some() {
                return Err("export baseline must be handcrafted".into());
            }
            let dir = Path::new(&a[4]);
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            if dir.join("features.bin").exists() {
                return Err("dataset already exists".into());
            }
            let mut out =
                BufWriter::new(File::create(dir.join("features.bin")).map_err(|e| e.to_string())?);
            let mut index =
                BufWriter::new(File::create(dir.join("samples.jsonl")).map_err(|e| e.to_string())?);
            let mut seen = std::collections::HashSet::new();
            let mut offset = 0u64;
            let mut count = 0;
            for (n, line) in BufReader::new(File::open(&a[3]).map_err(|e| e.to_string())?)
                .lines()
                .enumerate()
            {
                let job: Job = serde_json::from_str(&line.map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
                if job.split != "train" && job.split != "validation" {
                    return Err("invalid split".into());
                }
                let rec =
                    load_game_json(&fs::read_to_string(&job.game).map_err(|e| e.to_string())?)?;
                let wanted: std::collections::HashSet<_> = job.plies.iter().copied().collect();
                let last = *job.plies.iter().max().ok_or("empty job")?;
                if last >= rec.moves.len() || rec.result.is_none() || rec.abort_reason.is_some() {
                    return Err(format!("invalid completed game {}", job.game));
                }
                let mut state = replay_to_ply(&rec, 0)?;
                for (ply, m) in rec.moves.iter().enumerate().take(last + 1) {
                    if state.get_current_turn() != m.color {
                        return Err(format!("replay color mismatch {}:{ply}", job.game));
                    }
                    if wanted.contains(&ply)
                        && state.get_winner().is_none()
                        && !state.is_draw_by_progress_rule()
                    {
                        if let Some(score) = m.eval.filter(|s| s.abs() < 900_000) {
                            let stm = state.get_current_turn();
                            let us = features::active(state.get_board(), stm);
                            let them = features::active(state.get_board(), stm.opposite());
                            let mut encoded = Vec::with_capacity((us.len() + them.len()) * 4);
                            for &i in us.iter().chain(&them) {
                                encoded.extend_from_slice(&(i as u32).to_le_bytes());
                            }
                            let hash: [u8; 32] = Sha256::digest(&encoded).into();
                            if seen.insert(hash) {
                                let sign = if stm == Color::Black { 1. } else { -1. };
                                writeln!(index,"{}",json!({"offset":offset,"us":us.len(),"them":them.len(),"score":score as f32*sign,"material":nnue::material(state.get_board(),&cp.weights)*sign,"split":job.split,"game":job.game,"ply":ply,"position_hash":format!("{:x}",sha2::digest::Output::<Sha256>::from(hash))})).map_err(|e|e.to_string())?;
                                out.write_all(&encoded).map_err(|e| e.to_string())?;
                                offset += (us.len() + them.len()) as u64;
                                count += 1;
                            }
                        }
                    }
                    if ply < last {
                        state.make_move(GameHistory::record_to_move(m)?);
                        if state.get_current_turn() == m.color {
                            return Err(format!("replay failed {}:{ply}", job.game));
                        }
                    }
                }
                if n % 100 == 0 {
                    eprintln!("games={} samples={count}", n + 1);
                }
            }
            out.flush().map_err(|e| e.to_string())?;
            index.flush().map_err(|e| e.to_string())?;
            fs::write(
                dir.join("schema.json"),
                serde_json::to_vec_pretty(&schema()).unwrap(),
            )
            .map_err(|e| e.to_string())?;
            let identity = json!({"version":1,"samples":count,"baseline_sha256":nnue::hash_file(Path::new(&a[2]))?,"features_sha256":nnue::hash_file(&dir.join("features.bin"))?,"samples_sha256":nnue::hash_file(&dir.join("samples.jsonl"))?,"schema_sha256":nnue::hash_file(&dir.join("schema.json"))?});
            fs::write(
                dir.join("dataset.json"),
                serde_json::to_vec_pretty(&identity).unwrap(),
            )
            .map_err(|e| e.to_string())?;
            println!("{}", json!({"samples":count,"feature_indices":offset}));
        }
        Some("evaluate") if a.len() >= 5 => {
            let cp = EvalCheckpoint::load_path(&a[2])?;
            let rec = load_game_json(&fs::read_to_string(&a[3]).map_err(|e| e.to_string())?)?;
            let ply: usize = a[4].parse().map_err(|_| "bad ply")?;
            if ply > rec.moves.len() {
                return Err("ply out of bounds".into());
            }
            let mut s = replay_to_ply(&rec, ply)?;
            s.ensure_eval_inc(&cp.weights);
            let static_score = evaluate(&s, &cp.weights);
            let mut value = json!({"score":static_score,"material":nnue::material(s.get_board(),&cp.weights),"turn":s.get_current_turn()});
            if let Some(ms) = a.get(5) {
                let mut cfg =
                    taikyoku_shogi::alphabeta_player::AlphaBetaPlayer::from_checkpoint(cp.clone())
                        .config()
                        .clone();
                cfg.depth = 8;
                cfg.max_time_ms = Some(ms.parse().map_err(|_| "bad time")?);
                let t = std::time::Instant::now();
                let r = search(&s, &cp.weights, &cfg);
                let elapsed_ms = t.elapsed().as_millis();
                let legal = r.best_move.as_ref().is_some_and(|m| s.generate_legal_moves().contains(m));
                value["search"] = json!({"depth":r.completed_depth,"nodes":r.nodes,"score":r.score,"best":r.best_move,"legal":legal,"elapsed_ms":elapsed_ms});
            }
            println!("{value}");
        }
        _ => return Err(
            "nnue_tool schema | export BASE JOBS_JSONL OUTDIR | evaluate MODEL GAME PLY [TIME_MS]"
                .into(),
        ),
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("nnue_tool: {e}");
        std::process::exit(1)
    }
}
