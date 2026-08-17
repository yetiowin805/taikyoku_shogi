//! Persistent one-move search I/O for historical / current binaries.
//!
//! Protocol (one command per line):
//! ```text
//! position fen <TSFEN1 ...>
//! position startpos
//! go depth N time_ms M model PATH
//! quit
//! ```
//! Replies `bestmove <encoded>` or `bestmove (none)`.

use crate::alphabeta_player::AlphaBetaPlayer;
use crate::game_state::GameState;
use crate::notation::{move_encode, tsfen_decode};
use crate::player::AgentOptions;
use std::io::{self, BufRead, Write};

pub fn run_think_loop() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut state = GameState::new();
    state.setup_initial_position();
    let mut depth: Option<u32> = None;
    let mut time_ms: Option<u64> = None;
    let mut model: Option<String> = None;

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts[0] {
            "quit" => break,
            "isready" => {
                println!("readyok");
            }
            "position" => {
                if parts.len() >= 2 && parts[1] == "startpos" {
                    state = GameState::new();
                    state.setup_initial_position();
                } else if parts.len() >= 3 && parts[1] == "fen" {
                    let fen = parts[2..].join(" ");
                    match tsfen_decode(&fen) {
                        Ok(pos) => state = pos.to_state(),
                        Err(e) => {
                            eprintln!("info string TSFEN error: {e}");
                        }
                    }
                }
            }
            "go" => {
                let mut i = 1;
                while i < parts.len() {
                    match parts[i] {
                        "depth" => {
                            if let Some(v) = parts.get(i + 1).and_then(|s| s.parse().ok()) {
                                depth = Some(v);
                            }
                            i += 2;
                        }
                        "time_ms" => {
                            if let Some(v) = parts.get(i + 1).and_then(|s| s.parse().ok()) {
                                time_ms = Some(v);
                            }
                            i += 2;
                        }
                        "model" => {
                            if let Some(v) = parts.get(i + 1) {
                                model = Some((*v).to_string());
                            }
                            i += 2;
                        }
                        _ => i += 1,
                    }
                }
                match search_bestmove(&state, depth, time_ms, model.as_deref()) {
                    Ok(tok) => println!("bestmove {tok}"),
                    Err(e) => {
                        eprintln!("info string search error: {e}");
                        println!("bestmove (none)");
                    }
                }
            }
            _ => {}
        }
        let _ = stdout.flush();
    }
}

/// Search `state` and return an encoded best move (or `"(none)"`).
pub fn search_bestmove(
    state: &GameState,
    depth: Option<u32>,
    time_ms: Option<u64>,
    model: Option<&str>,
) -> Result<String, String> {
    let opts = AgentOptions {
        depth,
        model: model.map(|s| s.to_string()),
        max_time_ms: time_ms,
        quiescence_depth: None,
        engine: None,
    };
    let player = AlphaBetaPlayer::from_options(&opts);
    match player.choose_move_inner(state) {
        Some(mv) => Ok(move_encode(&mv)),
        None => Ok("(none)".into()),
    }
}

/// One-shot helper used by tests.
pub fn search_startpos(depth: u32) -> Result<String, String> {
    let mut state = GameState::new();
    state.setup_initial_position();
    search_bestmove(&state, Some(depth), None, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board_position::BoardPosition;
    use crate::notation::{move_decode, move_encode};

    #[test]
    fn think_loop_startpos_returns_legal_move() {
        let tok = search_startpos(1).expect("search");
        assert_ne!(tok, "(none)");
        let _mv = move_decode(&tok).expect("parse bestmove");
        let mut state = GameState::new();
        state.setup_initial_position();
        let legal = state.generate_legal_moves();
        assert!(
            legal.iter().any(|m| move_encode(m) == tok),
            "bestmove {tok} not legal"
        );
        let _ = BoardPosition::from_state(&state);
    }
}
