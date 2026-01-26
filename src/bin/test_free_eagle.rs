use taikyoku_shogi::game_state::{GameState, Move};
use taikyoku_shogi::piece::{Piece, PieceType, Color};
use taikyoku_shogi::position::Position;
use std::io::{self, Write};

/// Test GameState that disables win conditions
struct TestGameState {
    state: GameState,
}

impl TestGameState {
    fn new() -> Self {
        TestGameState {
            state: GameState::new(),
        }
    }

    /// Place a piece on the board
    fn place_piece(&mut self, piece_type: PieceType, color: Color, file: u8, rank: u8) -> Result<(), String> {
        if file >= 9 || rank >= 9 {
            return Err(format!("Position ({}, {}) is out of bounds for 9x9 board", file, rank));
        }
        
        let pos = Position::new(file, rank)
            .ok_or_else(|| format!("Invalid position: ({}, {})", file, rank))?;
        
        let piece = Piece::new(piece_type, color, pos);
        self.state.place_piece(piece);
        Ok(())
    }

    /// Remove a piece from the board
    fn remove_piece(&mut self, file: u8, rank: u8) -> Result<(), String> {
        if file >= 9 || rank >= 9 {
            return Err(format!("Position ({}, {}) is out of bounds for 9x9 board", file, rank));
        }
        
        let pos = Position::new(file, rank)
            .ok_or_else(|| format!("Invalid position: ({}, {})", file, rank))?;
        
        self.state.get_board_mut().remove_piece(pos);
        Ok(())
    }

    /// Execute a move
    fn make_move(&mut self, from_file: u8, from_rank: u8, to_file: u8, to_rank: u8) -> Result<(), String> {
        if from_file >= 9 || from_rank >= 9 || to_file >= 9 || to_rank >= 9 {
            return Err("Positions out of bounds for 9x9 board".to_string());
        }

        let from = Position::new(from_file, from_rank)
            .ok_or_else(|| format!("Invalid from position: ({}, {})", from_file, from_rank))?;
        let to = Position::new(to_file, to_rank)
            .ok_or_else(|| format!("Invalid to position: ({}, {})", to_file, to_rank))?;

        let mv = Move::new(from, to);
        if self.state.make_move(mv).is_some() || self.state.get_current_turn() != Color::Black {
            // Move succeeded (turn changed or returned Some)
            Ok(())
        } else {
            Err("Move failed".to_string())
        }
    }

    /// Get all legal moves for a piece
    fn get_legal_moves(&mut self, file: u8, rank: u8) -> Vec<Move> {
        if file >= 9 || rank >= 9 {
            return Vec::new();
        }

        if let Some(pos) = Position::new(file, rank) {
            if let Some(piece) = self.state.get_board().get_piece(pos) {
                // Temporarily set the turn to match the piece's color
                // so we can generate moves for any piece, not just current turn
                let original_turn = self.state.get_current_turn();
                self.state.set_current_turn(piece.color);
                
                let all_moves = self.state.generate_legal_moves();
                let filtered: Vec<Move> = all_moves.into_iter()
                    .filter(|m| m.from == pos)
                    .collect();
                
                // Restore original turn
                self.state.set_current_turn(original_turn);
                
                filtered
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    }

    /// Print the board (9x9 area)
    fn print_board(&self) {
        println!("\n  0   1   2   3   4   5   6   7   8");
        for rank in (0..9).rev() {
            print!("{} ", rank);
            for file in 0..9 {
                if let Some(pos) = Position::new(file, rank) {
                    if let Some(piece) = self.state.get_board().get_piece(pos) {
                        let symbol = piece.base_symbol();
                        let display = if piece.color == Color::Black {
                            symbol.to_uppercase()
                        } else {
                            symbol.to_lowercase()
                        };
                        print!("{:3} ", display);
                    } else {
                        print!(" .  ");
                    }
                } else {
                    print!(" ?  ");
                }
            }
            println!();
        }
        println!();
    }

    /// Get the current turn
    fn get_turn(&self) -> Color {
        self.state.get_current_turn()
    }

    /// Switch turn (for testing)
    fn switch_turn(&mut self) {
        self.state.set_current_turn(self.state.get_current_turn().opposite());
    }
}

fn parse_piece_type(s: &str) -> Result<PieceType, String> {
    match s.to_uppercase().as_str() {
        "FE" | "FREE_EAGLE" | "FREEEAGLE" => Ok(PieceType::FreeEagle),
        "P" | "PAWN" => Ok(PieceType::Pawn),
        "K" | "KING" => Ok(PieceType::King),
        "R" | "ROOK" => Ok(PieceType::Rook),
        "B" | "BISHOP" => Ok(PieceType::Bishop),
        "Q" | "QUEEN" | "FREE_KING" => Ok(PieceType::FreeKing),
        _ => Err(format!("Unknown piece type: {}", s)),
    }
}

fn parse_color(s: &str) -> Result<Color, String> {
    match s.to_uppercase().as_str() {
        "B" | "BLACK" => Ok(Color::Black),
        "W" | "WHITE" => Ok(Color::White),
        _ => Err(format!("Unknown color: {}", s)),
    }
}

fn print_help() {
    println!("Commands:");
    println!("  place <piece> <color> <file> <rank>  - Place a piece (e.g., place FE B 2 2)");
    println!("  remove <file> <rank>                  - Remove piece at position");
    println!("  move <from_file> <from_rank> <to_file> <to_rank>  - Execute a move");
    println!("  moves <file> <rank>                   - Show all legal moves for piece");
    println!("  show                                 - Show the board");
    println!("  turn                                 - Show current turn");
    println!("  switch                               - Switch turn");
    println!("  reset                                - Clear the board");
    println!("  help                                 - Show this help");
    println!("  quit                                 - Exit");
    println!();
    println!("Piece types: FE (Free Eagle), P (Pawn), K (King), R (Rook), B (Bishop), Q (Free King)");
    println!("Colors: B (Black), W (White)");
    println!("Coordinates: file and rank are 0-8 (9x9 board)");
}

fn main() {
    let mut state = TestGameState::new();
    
    println!("Free Eagle Test Environment");
    println!("9x9 board, no win conditions");
    println!("Type 'help' for commands\n");

    loop {
        print!("> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            break;
        }

        let parts: Vec<&str> = input.trim().split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "place" => {
                if parts.len() != 5 {
                    println!("Usage: place <piece> <color> <file> <rank>");
                    continue;
                }
                let piece_type = match parse_piece_type(parts[1]) {
                    Ok(pt) => pt,
                    Err(e) => {
                        println!("Error: {}", e);
                        continue;
                    }
                };
                let color = match parse_color(parts[2]) {
                    Ok(c) => c,
                    Err(e) => {
                        println!("Error: {}", e);
                        continue;
                    }
                };
                let file = match parts[3].parse::<u8>() {
                    Ok(f) => f,
                    Err(_) => {
                        println!("Error: Invalid file number");
                        continue;
                    }
                };
                let rank = match parts[4].parse::<u8>() {
                    Ok(r) => r,
                    Err(_) => {
                        println!("Error: Invalid rank number");
                        continue;
                    }
                };
                match state.place_piece(piece_type, color, file, rank) {
                    Ok(_) => {
                        println!("Placed {} {:?} at ({}, {})", parts[1], color, file, rank);
                        state.print_board();
                    }
                    Err(e) => println!("Error: {}", e),
                }
            }
            "remove" => {
                if parts.len() != 3 {
                    println!("Usage: remove <file> <rank>");
                    continue;
                }
                let file = match parts[1].parse::<u8>() {
                    Ok(f) => f,
                    Err(_) => {
                        println!("Error: Invalid file number");
                        continue;
                    }
                };
                let rank = match parts[2].parse::<u8>() {
                    Ok(r) => r,
                    Err(_) => {
                        println!("Error: Invalid rank number");
                        continue;
                    }
                };
                match state.remove_piece(file, rank) {
                    Ok(_) => {
                        println!("Removed piece at ({}, {})", file, rank);
                        state.print_board();
                    }
                    Err(e) => println!("Error: {}", e),
                }
            }
            "move" => {
                if parts.len() != 5 {
                    println!("Usage: move <from_file> <from_rank> <to_file> <to_rank>");
                    continue;
                }
                let from_file = match parts[1].parse::<u8>() {
                    Ok(f) => f,
                    Err(_) => {
                        println!("Error: Invalid from_file number");
                        continue;
                    }
                };
                let from_rank = match parts[2].parse::<u8>() {
                    Ok(r) => r,
                    Err(_) => {
                        println!("Error: Invalid from_rank number");
                        continue;
                    }
                };
                let to_file = match parts[3].parse::<u8>() {
                    Ok(f) => f,
                    Err(_) => {
                        println!("Error: Invalid to_file number");
                        continue;
                    }
                };
                let to_rank = match parts[4].parse::<u8>() {
                    Ok(r) => r,
                    Err(_) => {
                        println!("Error: Invalid to_rank number");
                        continue;
                    }
                };
                match state.make_move(from_file, from_rank, to_file, to_rank) {
                    Ok(_) => {
                        println!("Move executed: ({}, {}) -> ({}, {})", from_file, from_rank, to_file, to_rank);
                        state.print_board();
                    }
                    Err(e) => println!("Error: {}", e),
                }
            }
            "moves" => {
                if parts.len() != 3 {
                    println!("Usage: moves <file> <rank>");
                    continue;
                }
                let file = match parts[1].parse::<u8>() {
                    Ok(f) => f,
                    Err(_) => {
                        println!("Error: Invalid file number");
                        continue;
                    }
                };
                let rank = match parts[2].parse::<u8>() {
                    Ok(r) => r,
                    Err(_) => {
                        println!("Error: Invalid rank number");
                        continue;
                    }
                };
                let moves = state.get_legal_moves(file, rank);
                if moves.is_empty() {
                    println!("No legal moves for piece at ({}, {})", file, rank);
                } else {
                    println!("Legal moves for piece at ({}, {}):", file, rank);
                    for (i, mv) in moves.iter().enumerate() {
                        if let Some(path) = mv.free_eagle_path() {
                            println!("  {}. ({}, {}) -> ({}, {}) [Free Eagle path: {:?}]", 
                                i + 1, mv.from.file, mv.from.rank, mv.to.file, mv.to.rank, path);
                        } else {
                            println!("  {}. ({}, {}) -> ({}, {})", 
                                i + 1, mv.from.file, mv.from.rank, mv.to.file, mv.to.rank);
                        }
                    }
                }
            }
            "show" => {
                state.print_board();
            }
            "turn" => {
                println!("Current turn: {:?}", state.get_turn());
            }
            "switch" => {
                state.switch_turn();
                println!("Turn switched to: {:?}", state.get_turn());
            }
            "reset" => {
                state = TestGameState::new();
                println!("Board reset");
            }
            "help" => {
                print_help();
            }
            "quit" | "exit" => {
                break;
            }
            _ => {
                println!("Unknown command: {}. Type 'help' for commands.", parts[0]);
            }
        }
    }
}

