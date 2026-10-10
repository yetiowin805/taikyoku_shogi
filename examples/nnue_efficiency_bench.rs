//! Serial, strict-NNUE search benchmark. Replay, loading and warmup excluded.
use serde::Deserialize;
use serde_json::json;
use std::{io::{self, BufRead, Write}, path::PathBuf, time::Instant};
use taikyoku_shogi::{alphabeta_player::AlphaBetaPlayer, eval::EvalCheckpoint,
 search::search, training::{record::load_game_json, worker::replay_to_ply}};
#[derive(Deserialize)] struct Position { name:String, game:PathBuf, ply:usize }
#[derive(Deserialize)] struct Model { name:String, path:PathBuf }
#[derive(Deserialize)] struct Corpus { positions:Vec<Position>, models:Vec<Model> }
#[derive(Deserialize)] struct Request {
 position:usize, depth:u32, #[serde(default)] budget_ms:u64,
 #[serde(default)] variant:String, #[serde(default)] repeat:usize,
 #[cfg(feature="search-study")] #[serde(default)] study:taikyoku_shogi::search::StudyConfig,
}
fn usage()->(i64,i64){let mut r:libc::rusage=unsafe{std::mem::zeroed()};
 assert_eq!(unsafe{libc::getrusage(libc::RUSAGE_SELF,&mut r)},0);
 ((r.ru_utime.tv_sec+r.ru_stime.tv_sec)*1_000_000+r.ru_utime.tv_usec+r.ru_stime.tv_usec,r.ru_maxrss)}
fn main(){
 let corpus:Corpus=serde_json::from_slice(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
 let cp=EvalCheckpoint::load_path(&corpus.models[0].path).unwrap();
 assert!(cp.weights.nnue_runtime.is_some(),"NNUE required; no fallback");
 let pl=AlphaBetaPlayer::from_checkpoint(cp);
 let mut warmed=false;
 for line in io::stdin().lock().lines(){
  let q:Request=serde_json::from_str(&line.unwrap()).unwrap();let p=&corpus.positions[q.position];
  let record=load_game_json(&std::fs::read_to_string(&p.game).unwrap()).unwrap();
  assert!(p.ply<record.moves.len());let mut state=replay_to_ply(&record,p.ply).unwrap();
  state.ensure_eval_inc(pl.weights());let legal=state.generate_legal_moves();
  let mut cfg=pl.config().clone();cfg.depth=q.depth;cfg.collect_trace=false;
  cfg.max_time_ms=if q.budget_ms==0{None}else{Some(q.budget_ms)};
  if !warmed {let mut w=cfg.clone();w.depth=1;w.max_time_ms=None;search(&state,pl.weights(),&w);warmed=true;}
  #[cfg(feature="search-study")] {cfg.study=q.study;}
  #[cfg(feature="search-profile")] taikyoku_shogi::profile_timers::reset();
  let before=usage().0;let start=Instant::now();let r=search(&state,pl.weights(),&cfg);
  let ms=start.elapsed().as_secs_f64()*1000.;let cpu_ms=(usage().0-before) as f64/1000.;
  assert!(r.best_move.as_ref().is_some_and(|m|legal.contains(m)),"illegal full route");
  let mut v=json!({"position":q.position,"name":p.name,"agent":corpus.models[0].name,
   "variant":q.variant,"repeat":q.repeat,"depth":q.depth,"budget_ms":q.budget_ms,"nnue":true,
   "hash":state.hash(),"ply":p.ply,"legal":legal.len(),"ms":ms,"cpu_ms":cpu_ms,"peak_rss_kib":usage().1,
   "best":r.best_move,"score":r.score,"completed_depth":r.completed_depth,"aborted":r.aborted,
   "nodes":r.nodes,"q_nodes":r.q_nodes,"iterations":r.iteration_timings,
   "q_generated":r.q_caps_generated,"q_searched":r.q_caps_searched,"q_tt_hits":r.q_tt_hits,
   "lines":r.root_lines.iter().take(8).collect::<Vec<_>>()});
  #[cfg(feature="search-study")] {v["study"]=json!(q.study);v["diag"]=json!(r.study_stats);}
  #[cfg(feature="search-profile")] {v["profile"]=json!(taikyoku_shogi::profile_timers::report());}
  println!("{}",v);io::stdout().flush().unwrap();
 }
}
