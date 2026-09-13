//! One-position CLI and supervised v1 JSONL worker. Every iteration is flushed.
use serde::Deserialize;
use std::io::{self, BufRead, Write};
use std::time::Instant;
use taikyoku_shogi::{
    alphabeta_player::AlphaBetaPlayer,
    eval::EvalCheckpoint,
    game_history::GameHistory,
    game_state::GameState,
    notation::move_encode,
    piece::Color,
    player::AgentOptions,
    search::search_with_progress,
    training::record::{GameRecordV2, GameStart},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    id: String,
    game: String,
    /// Content identity checked by the supervising controller before dispatch.
    game_sha256: String,
    ply: usize,
    model: String,
    depth: u32,
    time_ms: u64,
}

struct Replay {
    path: String,
    identity: String,
    record: GameRecordV2,
    original: GameState,
    state: GameState,
    applied: usize,
}
impl Replay {
    fn load(path: &str, identity: &str) -> Result<Self, String> {
        let record: GameRecordV2 =
            serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let original = match &record.start {
            GameStart::Opening => {
                let mut s = GameState::new();
                s.setup_initial_position();
                s
            }
            GameStart::Position { position } => position.to_state(),
        };
        Ok(Self {
            path: path.into(),
            identity: identity.into(),
            state: original.clone(),
            original,
            record,
            applied: 0,
        })
    }
    fn seek(&mut self, ply: usize) -> Result<(), String> {
        if ply == 0 || ply > self.record.moves.len() {
            return Err("ply out of bounds".into());
        }
        if ply - 1 < self.applied {
            self.state = self.original.clone();
            self.applied = 0;
        }
        while self.applied < ply - 1 {
            let mv = &self.record.moves[self.applied];
            if self.state.get_current_turn() != mv.color {
                return Err("replay color mismatch".into());
            }
            self.state.make_move(GameHistory::record_to_move(mv)?);
            if self.state.get_current_turn() == mv.color {
                return Err("replay failed".into());
            }
            self.applied += 1;
        }
        if self.state.get_current_turn() != self.record.moves[ply - 1].color {
            return Err("target color mismatch".into());
        }
        Ok(())
    }
    fn analyze(&mut self, request: &Request, batch: bool) -> Result<(), String> {
        self.seek(request.ply)?;
        let color = self.state.get_current_turn();
        let agent = if color == Color::Black {
            &self.record.black
        } else {
            &self.record.white
        };
        if agent.name != "ab" || agent.engine.is_some() {
            return Err(
                "analysis requires current in-process ab agent; historical engines are unsupported"
                    .into(),
            );
        }
        // A fresh binding and empty logical search tables for every position/model.
        let checkpoint = EvalCheckpoint::load_path(&request.model)?;
        let player = AlphaBetaPlayer::from_checkpoint_with_overrides(
            checkpoint,
            &AgentOptions {
                model: Some(request.model.clone()),
                depth: Some(request.depth),
                max_time_ms: Some(request.time_ms),
                quiescence_depth: agent.quiescence_depth,
            },
        );
        let sign = if color == Color::Black { 1 } else { -1 };
        let started = Instant::now();
        let mut progress = |depth, score, mv: &taikyoku_shogi::game_state::Move, nodes| {
            emit(
                request,
                batch,
                "iteration",
                serde_json::json!({
                    "completed_depth": depth, "score": score * sign, "best_move": move_encode(mv),
                    "nodes": nodes, "elapsed_ms": started.elapsed().as_millis()
                }),
            );
        };
        let result = search_with_progress(
            &self.state,
            player.weights(),
            player.config(),
            &mut progress,
        );
        let terminal = result.best_move.is_none() && !result.aborted;
        if terminal {
            emit(
                request,
                batch,
                "iteration",
                serde_json::json!({
                    "completed_depth": 0, "terminal": true, "score": result.score * sign,
                    "best_move": null, "nodes": result.nodes, "elapsed_ms": started.elapsed().as_millis()
                }),
            );
        } else if result.completed_depth == 0 {
            return Err("no search iteration completed".into());
        }
        if batch {
            emit(
                request,
                true,
                "complete",
                serde_json::json!({
                    "completed_depth": result.completed_depth, "aborted": result.aborted,
                    "terminal": terminal, "elapsed_ms": started.elapsed().as_millis()
                }),
            );
        }
        Ok(())
    }
}
fn emit(request: &Request, batch: bool, event: &str, mut value: serde_json::Value) {
    if batch {
        value["version"] = 1.into();
        value["id"] = request.id.clone().into();
        value["event"] = event.into();
    }
    println!("{value}");
    let _ = io::stdout().flush();
}
fn batch() -> Result<(), String> {
    let mut replay: Option<Replay> = None;
    for line in io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        // Invalid framing is fatal; a supervisor must never attribute an uncorrelated event.
        let request: Request = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        let result = (|| {
            if request.version != 1 {
                return Err("unsupported protocol version".into());
            }
            if !replay
                .as_ref()
                .is_some_and(|r| r.path == request.game && r.identity == request.game_sha256)
            {
                replay = Some(Replay::load(&request.game, &request.game_sha256)?);
            }
            replay.as_mut().unwrap().analyze(&request, true)
        })();
        if let Err(error) = result {
            emit(&request, true, "error", serde_json::json!({"error": error}));
            // A failed replay may have partially advanced. Start clean next time.
            replay = None;
        }
    }
    Ok(())
}
fn run() -> Result<(), String> {
    let a: Vec<String> = std::env::args().collect();
    if a.len() == 2 && a[1] == "--batch-v1" {
        return batch();
    }
    if a.len() == 3 && a[1] == "--validate-model" {
        EvalCheckpoint::load_path(&a[2])?;
        return Ok(());
    }
    if a.len() != 6 {
        return Err("usage: analyze_position GAME PLY MODEL DEPTH TIME_MS | --batch-v1".into());
    }
    let request = Request {
        version: 1,
        id: String::new(),
        game: a[1].clone(),
        game_sha256: String::new(),
        ply: a[2].parse().map_err(|_| "invalid ply")?,
        model: a[3].clone(),
        depth: a[4].parse().map_err(|_| "invalid depth")?,
        time_ms: a[5].parse().map_err(|_| "invalid time")?,
    };
    Replay::load(&request.game, "")?.analyze(&request, false)
}
fn main() {
    if let Err(e) = run() {
        eprintln!("analyze_position: {e}");
        std::process::exit(1);
    }
}
