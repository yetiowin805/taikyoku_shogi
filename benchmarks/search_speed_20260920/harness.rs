//! JSON-lines benchmark server; model loading and replay are outside search timing.
use std::{collections::HashMap, io::{self, BufRead, Write}, path::PathBuf, time::Instant};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use taikyoku_shogi::{alphabeta_player::AlphaBetaPlayer, eval::EvalCheckpoint, game_state::GameState,
    search::search, training::{record::load_game_json, worker::replay_to_ply}};
#[derive(Deserialize)]
struct Position { name: String, game: PathBuf, ply: usize }
#[derive(Deserialize)]
struct Model { name: String, path: PathBuf }
#[derive(Deserialize)]
struct Corpus { positions: Vec<Position>, models: Vec<Model> }
#[derive(Deserialize)]
struct Request { position: usize, model: usize, depth: u32, #[serde(default)] budget_ms: u64 }
fn cpu_us() -> i64 {
    let mut r: libc::rusage = unsafe {std::mem::zeroed()};
    assert_eq!(unsafe {libc::getrusage(libc::RUSAGE_SELF,&mut r)},0);
    (r.ru_utime.tv_sec+r.ru_stime.tv_sec)*1_000_000+r.ru_utime.tv_usec+r.ru_stime.tv_usec
}
fn main() {
    let path=std::env::args().nth(1).expect("corpus.json");
    let corpus:Corpus=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let states:Vec<GameState>=corpus.positions.iter().map(|p| {
        let r=load_game_json(&std::fs::read_to_string(&p.game).unwrap()).unwrap();
        assert!(p.ply<=r.moves.len()); replay_to_ply(&r,p.ply).unwrap()
    }).collect();
    let mut players=HashMap::new();
    for line in io::stdin().lock().lines() {
        let q:Request=serde_json::from_str(&line.unwrap()).unwrap();
        let player=players.entry(q.model).or_insert_with(|| {
            AlphaBetaPlayer::from_checkpoint(EvalCheckpoint::load_path(&corpus.models[q.model].path).unwrap())
        });
        let state=&states[q.position];
        let mut cfg=player.config().clone();
        cfg.depth=q.depth.max(1); cfg.max_time_ms=if q.budget_ms==0 {None} else {Some(q.budget_ms)};
        cfg.collect_trace=false;
        let before=cpu_us(); let start=Instant::now();
        let result=search(state,player.weights(),&cfg);
        let ms=start.elapsed().as_secs_f64()*1000.; let cpu_ms=(cpu_us()-before) as f64/1000.;
        let legal=state.generate_legal_moves();
        assert!(result.best_move.as_ref().is_some_and(|m|legal.contains(m)),"chosen full route is illegal");
        let signature=json!({"best":result.best_move,"score":result.score,"nodes":result.nodes,"q_nodes":result.q_nodes,
            "depth":result.completed_depth,"static_eval":result.static_eval,"lines":result.root_lines});
        let signature_sha256=format!("{:x}",Sha256::digest(serde_json::to_vec(&signature).unwrap()));
        let board=state.get_board();
        let pieces=board.pieces_by_color(taikyoku_shogi::piece::Color::Black).len()+board.pieces_by_color(taikyoku_shogi::piece::Color::White).len();
        println!("{}",json!({"position":q.position,"name":corpus.positions[q.position].name,"model":q.model,"agent":corpus.models[q.model].name,
            "ms":ms,"cpu_ms":cpu_ms,"depth":q.depth,"completed_depth":result.completed_depth,"aborted":result.aborted,
            "nodes":result.nodes,"q_nodes":result.q_nodes,"score":result.score,"best":result.best_move,"signature":signature_sha256,
            "pieces":pieces,"legal":legal.len(),"ply":corpus.positions[q.position].ply,"hash":state.hash(),"nnue":player.weights().nnue_runtime.is_some()}));
        io::stdout().flush().unwrap();
    }
}
