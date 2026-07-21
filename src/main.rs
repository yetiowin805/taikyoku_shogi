// Modules are now declared in lib.rs
use taikyoku_shogi::debug_tool::DebugTool;
use taikyoku_shogi::game_state::{GameState, Move};
use taikyoku_shogi::game_history::{GameHistory, GameResult};
use taikyoku_shogi::minimal_intelligence_player::MinimalIntelligencePlayer;
use taikyoku_shogi::piece::Color;
use taikyoku_shogi::random_player::RandomPlayer;
use taikyoku_shogi::uci;
use std::env;

#[derive(Clone, Copy)]
enum PlayMode {
    /// Heuristic player (MinimalIntelligencePlayer)
    Heuristic,
    /// Uniform random legal moves
    Random,
}

impl PlayMode {
    fn label(self) -> &'static str {
        match self {
            PlayMode::Heuristic => "heuristic",
            PlayMode::Random => "random",
        }
    }

    fn choose_move(self, game_state: &GameState) -> Option<Move> {
        match self {
            PlayMode::Heuristic => MinimalIntelligencePlayer::make_move(game_state),
            PlayMode::Random => RandomPlayer::make_move(game_state),
        }
    }
}

/// Convert a number to Japanese/Chinese numerals (1-indexed, no zero)
fn to_japanese_numeral(n: u8) -> String {
    if n == 0 {
        return "零".to_string();
    }
    
    let digits = [
        "", "一", "二", "三", "四", "五", "六", "七", "八", "九"
    ];
    
    let mut result = String::new();
    let n = n as usize;
    
    if n >= 30 {
        let tens = n / 10;
        let ones = n % 10;
        result.push_str(digits[tens]);
        result.push_str("十");
        if ones > 0 {
            result.push_str(digits[ones]);
        }
    } else if n >= 20 {
        result.push_str("二");
        result.push_str("十");
        let ones = n % 10;
        if ones > 0 {
            result.push_str(digits[ones]);
        }
    } else if n >= 10 {
        result.push_str("十");
        let ones = n % 10;
        if ones > 0 {
            result.push_str(digits[ones]);
        }
    } else {
        result.push_str(digits[n]);
    }
    
    result
}

/// Flip file number for shogi notation (rightmost is 1, leftmost is 36)
/// Takes 0-indexed file (0-35) and returns 1-indexed flipped file (1-36)
fn flip_file(file: u8) -> u8 {
    36 - file
}

/// Flip rank number for shogi notation (top is 1, bottom is 36)
/// Takes 0-indexed rank (0-35) and returns 1-indexed flipped rank (1-36)
fn flip_rank(rank: u8) -> u8 {
    36 - rank
}

fn print_usage() {
    println!("Usage:");
    println!("  cargo run -- play [mi|random]  - Self-play (default: mi heuristic)");
    println!("  cargo run -- list              - List saved games");
    println!("  cargo run -- view <file>       - View a game");
    println!("  cargo run -- debug             - Start debug REPL");
    println!("  cargo run -- serve [port]      - Start local GUI/API server (default 3000)");
    println!("  cargo run --                   - Start UCI interface (stub)");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.len() > 1 {
        match args[1].as_str() {
            "play" => {
                let mode = match args.get(2).map(|s| s.as_str()) {
                    None | Some("mi") | Some("heuristic") => PlayMode::Heuristic,
                    Some("random") => PlayMode::Random,
                    Some(other) => {
                        println!("Unknown play mode '{}'. Use 'mi' or 'random'.", other);
                        print_usage();
                        return;
                    }
                };
                play_game(mode);
            }
            "view" => {
                if args.len() < 3 {
                    println!("Usage: cargo run -- view <game_file>");
                    return;
                }
                view_game(&args[2]);
            }
            "list" => {
                list_games();
            }
            "debug" => {
                let mut debug_tool = DebugTool::new();
                debug_tool.run();
            }
            "serve" => {
                let port: u16 = args
                    .get(2)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(3000);
                let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
                let static_dir = std::path::PathBuf::from("web/dist");
                let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
                if let Err(e) = rt.block_on(taikyoku_shogi::server::serve(
                    addr,
                    if static_dir.exists() {
                        Some(static_dir)
                    } else {
                        None
                    },
                )) {
                    eprintln!("{}", e);
                }
            }
            _ => {
                print_usage();
            }
        }
    } else {
        uci::run_uci_loop();
    }
}

fn play_game(mode: PlayMode) {
    let mut game_state = GameState::new();
    game_state.setup_initial_position();
    
    let mut history = GameHistory::new("games");
    history.start_game();
    
    let mut move_number = 1;
    let max_moves = 20000; // Prevent infinite games
    
    println!("Starting {} self-play game...", mode.label());
    
    while move_number <= max_moves {
        // Check if game is a draw by 500-move rule (no capture or promotion for 500 turns)
        if game_state.is_draw_by_500_move_rule() {
            if let Ok(filename) = history.end_game(GameResult::Draw) {
                println!("Game ended after {} moves. Draw by 500-move rule (no capture or promotion for 500 turns). Saved to: {}", 
                    move_number - 1, filename);
            }
            return;
        }
        
        // Check if game is a draw by insufficient material (only Kings, Crown Princes, Great Generals remain)
        if game_state.is_draw_by_insufficient_material() {
            if let Ok(filename) = history.end_game(GameResult::Draw) {
                println!("Game ended after {} moves. Draw by insufficient material (only Kings, Crown Princes, and Great Generals remain). Saved to: {}", 
                    move_number - 1, filename);
            }
            return;
        }
        
        // Check if game is over (all royal pieces captured)
        if let Some(winner) = game_state.get_winner() {
            let result = match winner {
                Color::Black => GameResult::BlackWins,
                Color::White => GameResult::WhiteWins,
            };
            
            if let Ok(filename) = history.end_game(result) {
                println!("Game ended after {} moves. {:?} wins (all opponent royal pieces captured). Saved to: {}", 
                    move_number - 1, winner, filename);
            }
            return;
        }
        
        let legal_moves = game_state.generate_legal_moves();
        
        if legal_moves.is_empty() {
            // Game over - no legal moves
            let result = match game_state.get_current_turn() {
                Color::Black => GameResult::WhiteWins,
                Color::White => GameResult::BlackWins,
            };
            
            if let Ok(filename) = history.end_game(result) {
                println!("Game ended after {} moves. Saved to: {}", move_number - 1, filename);
            }
            return;
        }
        
        if let Some(mv) = mode.choose_move(&game_state) {
            let color = game_state.get_current_turn();
            let promoted = mv.promoted;
            
            // Get piece type before making the move (for shogi-style notation)
            let piece_symbol = if let Some(piece) = game_state.get_board().get_piece(mv.from) {
                // Use the base type symbol (what the piece was before promotion, or current type if not promoted)
                let symbol = piece.base_symbol();
                // Add "+" prefix if the piece is already promoted
                if piece.is_promoted {
                    format!("+{}", symbol)
                } else {
                    symbol.to_string()
                }
            } else {
                "?".to_string()
            };
            
            // Check turn before making the move to detect if move succeeded
            let turn_before = game_state.get_current_turn();
            
            // Execute the move
            let _ = game_state.make_move(mv.clone());
            let turn_after = game_state.get_current_turn();
            
            // Move succeeded if turn changed (for both regular and two-step moves)
            if turn_before != turn_after {
                // Record the move (two-step moves are saved as a single move with intermediate position)
                history.record_move(&mv, color, move_number, promoted, None);
                
                let color_str = match color {
                    Color::Black => "B",
                    Color::White => "W",
                };
                
                // Check if this is a two-step move for display
                if let Some(intermediate) = mv.intermediate() {
                    // Display two-step move as two lines with different move numbers
                    let to_file_flipped = flip_file(mv.to.file);
                    let to_rank_flipped = flip_rank(mv.to.rank);
                    let to_rank_jp = to_japanese_numeral(to_rank_flipped);
                    let from_file_flipped = flip_file(mv.from.file);
                    let from_rank_flipped = flip_rank(mv.from.rank);
                    let from_rank_jp = to_japanese_numeral(from_rank_flipped);
                    let inter_file_flipped = flip_file(intermediate.file);
                    let inter_rank_flipped = flip_rank(intermediate.rank);
                    let inter_rank_jp = to_japanese_numeral(inter_rank_flipped);
                    
                    println!("{}. {}: {}{}{}{}{}{}", 
                        move_number, color_str,
                        inter_file_flipped, inter_rank_jp, piece_symbol.clone(),
                        from_file_flipped, from_rank_jp, "");
                    println!("{}. {}: {}{}{}{}{}{}", 
                        move_number + 1, color_str,
                        to_file_flipped, to_rank_jp, piece_symbol,
                        inter_file_flipped, inter_rank_jp,
                        if promoted { "成" } else { "" });
                    
                    // Print board after the second step
                    println!("\n=== After move {} ===\n{}", move_number + 1, history.format_board(&game_state));
                    
                    move_number += 2;
                } else {
                    // Display single move
                    let to_file_flipped = flip_file(mv.to.file);
                    let to_rank_flipped = flip_rank(mv.to.rank);
                    let to_rank_jp = to_japanese_numeral(to_rank_flipped);
                    let from_file_flipped = flip_file(mv.from.file);
                    let from_rank_flipped = flip_rank(mv.from.rank);
                    let from_rank_jp = to_japanese_numeral(from_rank_flipped);
                    
                    println!("{}. {}: {}{}{}{}{}{}", 
                        move_number, color_str,
                        to_file_flipped, to_rank_jp, piece_symbol,
                        from_file_flipped, from_rank_jp,
                        if promoted { "成" } else { "" });
                    
                    // Print board after the move
                    println!("\n=== After move {} ===\n{}", move_number, history.format_board(&game_state));
                    
                    move_number += 1;
                }
            }
            // If move failed (turn didn't change), just continue (don't increment move_number)
        } else {
            break;
        }
    }
    
    // Game ended due to move limit
    if let Ok(filename) = history.end_game(GameResult::Draw) {
        println!("Game ended after {} moves (draw by move limit). Saved to: {}", max_moves, filename);
    }
}

fn view_game(filename: &str) {
    let history = GameHistory::new("games");
    // Remove "games/" prefix if present
    let filename = filename.strip_prefix("games/").unwrap_or(filename);
    match history.load_game(filename) {
        Ok(game) => {
            println!("{}", history.format_game(&game));
        }
        Err(e) => {
            println!("Error loading game: {}", e);
        }
    }
}

fn list_games() {
    let history = GameHistory::new("games");
    match history.list_games() {
        Ok(games) => {
            if games.is_empty() {
                println!("No saved games found.");
            } else {
                println!("Saved games:");
                for game in games {
                    println!("  {}", game);
                }
            }
        }
        Err(e) => {
            println!("Error listing games: {}", e);
        }
    }
}

