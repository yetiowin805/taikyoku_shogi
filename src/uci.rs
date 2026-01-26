use crate::game_state::GameState;
use std::io::{self, BufRead, Write};

pub fn run_uci_loop() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut game_state = GameState::new();

    println!("id name Taikyoku Shogi Engine");
    println!("id author Your Name");
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
                println!("id author Your Name");
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
                
                if parts[1] == "startpos" {
                    game_state = GameState::new();
                    game_state.setup_initial_position();
                } else if parts[1] == "fen" {
                    // TODO: Parse FEN string
                    game_state = GameState::new();
                }

                // Apply moves if any
                if parts.len() > 2 && parts[2] == "moves" {
                    for move_str in parts.iter().skip(3) {
                        // TODO: Parse move string (e.g., "a1b2" format)
                        // For now, just acknowledge
                    }
                }
            }
            "go" => {
                // Generate and return legal moves
                let legal_moves = game_state.generate_legal_moves();
                
                // Format: "info string legal moves: a1b2 a1c3 ..."
                // Using 1-indexed coordinates for human display
                print!("info string legal moves:");
                for mv in &legal_moves {
                    // Simple format: from_file_from_rank_to_file_to_rank (1-indexed)
                    print!(" {}{}{}{}", 
                        mv.from.file + 1, mv.from.rank + 1,
                        mv.to.file + 1, mv.to.rank + 1);
                }
                println!();
                
                // For now, just return the first move as "bestmove" if any exist
                if let Some(first_move) = legal_moves.first() {
                    println!("bestmove {}{}{}{}", 
                        first_move.from.file + 1, first_move.from.rank + 1,
                        first_move.to.file + 1, first_move.to.rank + 1);
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

