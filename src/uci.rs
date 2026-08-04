use crate::game_state::GameState;
use crate::notation::{move_decode, move_encode, tsfen_decode};
use std::io::{self, BufRead, Write};

pub fn run_uci_loop() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut game_state = GameState::new();

    println!("id name Taikyoku Shogi Engine");
    println!("id author Taikyoku");
    println!("uciok");

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "uci" => {
                println!("id name Taikyoku Shogi Engine");
                println!("id author Taikyoku");
                println!("uciok");
            }
            "isready" => {
                println!("readyok");
            }
            "ucinewgame" => {
                game_state = GameState::new();
                game_state.setup_initial_position();
            }
            "position" => {
                if parts.len() < 2 {
                    continue;
                }

                let moves_idx = parts.iter().position(|p| *p == "moves");

                if parts[1] == "startpos" {
                    game_state = GameState::new();
                    game_state.setup_initial_position();
                } else if parts[1] == "fen" {
                    let fen_end = moves_idx.unwrap_or(parts.len());
                    if fen_end <= 2 {
                        eprintln!("info string missing TSFEN after position fen");
                        continue;
                    }
                    let fen = parts[2..fen_end].join(" ");
                    match tsfen_decode(&fen) {
                        Ok(pos) => {
                            game_state = pos.to_state();
                        }
                        Err(e) => {
                            eprintln!("info string TSFEN error: {}", e);
                            continue;
                        }
                    }
                } else {
                    continue;
                }

                if let Some(i) = moves_idx {
                    for move_str in parts.iter().skip(i + 1) {
                        match move_decode(move_str) {
                            Ok(mv) => {
                                let turn_before = game_state.get_current_turn();
                                let result = game_state.make_move(mv);
                                let turn_after = game_state.get_current_turn();
                                if result.is_none() && turn_before == turn_after {
                                    eprintln!("info string illegal move {}", move_str);
                                    break;
                                }
                            }
                            Err(e) => {
                                eprintln!("info string move parse error ({}): {}", move_str, e);
                                break;
                            }
                        }
                    }
                }
            }
            "go" => {
                let legal_moves = game_state.generate_legal_moves();

                print!("info string legal moves:");
                for mv in &legal_moves {
                    print!(" {}", move_encode(mv));
                }
                println!();

                if let Some(first_move) = legal_moves.first() {
                    println!("bestmove {}", move_encode(first_move));
                } else {
                    println!("bestmove (none)");
                }
            }
            "quit" => {
                break;
            }
            _ => {
                // Unknown command, ignore
            }
        }

        stdout.flush().unwrap();
    }
}
