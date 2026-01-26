use crate::game_state::{GameState, Move};
use crate::piece::{Piece, PieceType, Color};
use crate::position::Position;
use crate::board::Board;
use crate::movement::MovementConfig;
use crate::tengu_attack;
use crate::path_utils;
use crate::attack_utils;
use rand::Rng;

pub struct MinimalIntelligencePlayer;

impl MinimalIntelligencePlayer {
    // Strategic weight constants (easy to modify)
    const FORWARD_RANK_WEIGHT_MULTIPLIER: f64 = 1.1;  // 10% bonus for advancing forward
    const CENTER_FILE_WEIGHT_MULTIPLIER: f64 = 1.05;  // 5% bonus for moving toward center
    const PROMOTION_WEIGHT_MULTIPLIER: f64 = 2.0;       // 2x weight for promotion
    const BASE_WEIGHT: f64 = 1.0;                     // Default weight
    const CENTER_FILE: f64 = 17.5;                    // Middle of 0-35 range
    
    /// Make a move following the strategic priority order
    pub fn make_move(game_state: &GameState) -> Option<Move> {
        let legal_moves = game_state.generate_legal_moves();
        if legal_moves.is_empty() {
            return None;
        }

        let current_color = game_state.get_current_turn();
        let opponent_color = current_color.opposite();
        let mut rng = rand::thread_rng();

        // Priority 1: Capture opponent's final royal piece
        if Self::is_final_royal_piece(game_state, opponent_color) {
            let final_royal_capture_moves: Vec<&Move> = legal_moves
                .iter()
                .filter(|mv| {
                    if let Some(piece) = game_state.get_board().get_piece(mv.to) {
                        piece.color == opponent_color && piece.piece_type.is_royal()
                    } else {
                        false
                    }
                })
                .collect();
            
            if !final_royal_capture_moves.is_empty() {
                let index = rng.gen_range(0..final_royal_capture_moves.len());
                return Some(final_royal_capture_moves[index].clone());
            }
        }

        // Priority 2: Resolve check on own final royal piece
        if Self::is_final_royal_piece(game_state, current_color) {
            let royal_pieces = Self::get_royal_pieces(game_state, current_color);
            if let Some(royal_piece) = royal_pieces.first() {
                if Self::is_under_attack(game_state, royal_piece.position, current_color) {
                    if let Some(mv) = Self::resolve_check(game_state, royal_piece) {
                        return Some(mv);
                    }
                }
            }
        }

        // Priority 3: Capture any opponent royal piece
        let royal_capture_moves: Vec<&Move> = legal_moves
            .iter()
            .filter(|mv| {
                if let Some(piece) = game_state.get_board().get_piece(mv.to) {
                    piece.color == opponent_color && piece.piece_type.is_royal()
                } else {
                    false
                }
            })
            .collect();
        
        if !royal_capture_moves.is_empty() {
            let index = rng.gen_range(0..royal_capture_moves.len());
            return Some(royal_capture_moves[index].clone());
        }

        // Priority 4: Resolve check on own royal pieces (prioritized)
        let mut royal_pieces = Self::get_royal_pieces(game_state, current_color);
        // Filter those under attack
        royal_pieces.retain(|piece| {
            Self::is_under_attack(game_state, piece.position, current_color)
        });
        
        if !royal_pieces.is_empty() {
            // Sort by priority (highest first)
            royal_pieces.sort_by(|a, b| {
                Self::royal_piece_priority(b).cmp(&Self::royal_piece_priority(a))
            });
            
            for royal_piece in royal_pieces {
                if let Some(mv) = Self::resolve_check(game_state, &royal_piece) {
                    return Some(mv);
                }
            }
        }

        // Priority 5: Weighted random move (excluding unsafe moves: walking into check, unpinning, Tengu exposure)
        // We calculate weights upfront, but check moves lazily - only when we're about to choose them
        let mut available_moves = legal_moves;
        let mut attempts = 0;
        let max_attempts = available_moves.len(); // Prevent infinite loop
        
        // Calculate weights for all moves upfront (before safety checks to maintain lazy evaluation)
        let mut move_weights: Vec<f64> = Vec::with_capacity(available_moves.len());
        for mv in &available_moves {
            if let Some(moving_piece) = game_state.get_board().get_piece(mv.from) {
                move_weights.push(Self::calculate_move_weight(mv, &moving_piece));
            } else {
                move_weights.push(Self::BASE_WEIGHT); // Fallback if piece not found
            }
        }
        
        while !available_moves.is_empty() && attempts < max_attempts {
            // Choose a move using weighted random selection
            if let Some(index) = Self::select_weighted_random_move(&available_moves, &move_weights, &mut rng) {
                let chosen_move = available_moves[index].clone();
                
                // Check if this move promotes a Fowl Cadet (exclude these moves as Fowl Officer is weaker)
                if let Some(moving_piece) = game_state.get_board().get_piece(chosen_move.from) {
                    if moving_piece.piece_type == PieceType::FowlCadet && chosen_move.promoted {
                        // This move promotes Fowl Cadet, exclude it
                        available_moves.remove(index);
                        move_weights.remove(index);
                        attempts += 1;
                        continue;
                    }
                    
                    // Check if range capturing move would be worthwhile (captures at least 2 more enemy than friendly)
                    if !Self::would_range_capture_be_worthwhile(game_state.get_board(), &chosen_move, &moving_piece) {
                        // This move captures too many friendly pieces relative to enemy pieces, exclude it
                        available_moves.remove(index);
                        move_weights.remove(index);
                        attempts += 1;
                        continue;
                    }
                }
                
                // Check if this move is safe (combines all safety checks)
                if !Self::is_move_safe(game_state, &chosen_move) {
                    // This move is unsafe, remove it and try again
                    available_moves.remove(index);
                    move_weights.remove(index);
                    attempts += 1;
                    continue;
                }
                
                // This move is safe, return it
                return Some(chosen_move);
            } else {
                // Weighted selection failed (shouldn't happen, but break to avoid infinite loop)
                break;
            }
        }
        
        // Fallback: If all moves are unsafe (zugzwang position),
        // or we've exhausted all attempts, choose a move entirely at random
        // Regenerate legal moves for the fallback since we consumed the original vector
        let legal_moves_fallback = game_state.generate_legal_moves();
        if !legal_moves_fallback.is_empty() {
            let index = rng.gen_range(0..legal_moves_fallback.len());
            Some(legal_moves_fallback[index].clone())
        } else {
            None
        }
    }



    /// Determine if a piece should be checked for attack potential
    /// Returns true if piece is Tengu/promoted Peacock/Capricorn, unpromoted Peacock, Hook Mover/promoted forms, within 9x9 square of any royal piece, or has range movement toward any royal piece
    fn should_check_piece_for_attack(piece: &Piece, royal_pieces: &[Piece]) -> bool {
        // Always check Tengu/promoted Peacock/Capricorn and unpromoted Peacock due to highly mobile two-step movement
        if attack_utils::is_tengu_or_promoted_peacock(piece) || attack_utils::is_unpromoted_peacock(piece) {
            return true;
        }
        
        // Always check Hook Mover and its promoted forms due to highly mobile two-step movement
        if attack_utils::is_hook_mover_like_piece(piece) {
            return true;
        }
        
        // Check if piece is within 9x9 square of any royal piece
        for royal in royal_pieces {
            if attack_utils::is_within_9x9_square(piece.position, royal.position) {
                return true;
            }
        }
        
        // Check if piece has range movement in direction toward any royal piece
        for royal in royal_pieces {
            if let Some(direction) = attack_utils::get_direction_toward(piece.position, royal.position) {
                if attack_utils::has_range_movement_in_direction(piece, direction) {
                    return true;
                }
            }
        }
        
        false
    }

    /// Check if a position is under attack by opponent pieces
    /// Uses optimized Board-level attack detection
    /// Optimized for check detection on royal pieces
    fn is_under_attack(game_state: &GameState, position: Position, defender_color: Color) -> bool {
        let board = game_state.get_board();
        let attacker_color = defender_color.opposite();
        board.is_position_attacked_by_color_for_check(position, attacker_color)
    }

    /// Get all opponent pieces that can attack a given position
    /// Uses the same attack detection logic as is_under_attack for consistency
    fn get_attacking_pieces(game_state: &GameState, position: Position, defender_color: Color) -> Vec<Piece> {
        let board = game_state.get_board();
        let attacker_color = defender_color.opposite();
        let all_attacker_pieces = board.get_pieces_by_color(attacker_color);
        
        // Use the same filtering and attack detection logic as is_position_attacked_by_color_for_check
        use crate::attack_utils;
        
        // Filter pieces based on proximity and movement capabilities
        let filtered_pieces: Vec<Piece> = all_attacker_pieces
            .into_iter()
            .filter(|piece| attack_utils::should_check_piece_for_target_position(piece, position, true))
            .collect();
        
        // Get pieces that can actually attack the position
        let mut attacking = Vec::new();
        
        for piece in &filtered_pieces {
            // Check specialized pieces with optimized functions
            if attack_utils::is_tengu_or_promoted_peacock(piece) {
                if tengu_attack::can_tengu_attack_target(piece, position, board) {
                    attacking.push(*piece);
                }
            } else if attack_utils::is_unpromoted_peacock(piece) {
                if tengu_attack::can_peacock_attack_target(piece, position, board) {
                    attacking.push(*piece);
                }
            } else if attack_utils::is_hook_mover_like_piece(piece) {
                if tengu_attack::can_hook_mover_attack_target(piece, position, board) {
                    attacking.push(*piece);
                }
            } else if attack_utils::is_cannon_soldier(piece) {
                if tengu_attack::can_cannon_soldier_attack_target(piece, position, board) {
                    attacking.push(*piece);
                }
            } else {
                // For non-specialized pieces, use can_reach_boardlike
                if piece.can_reach_boardlike(position, board) {
                    attacking.push(*piece);
                }
            }
        }
        
        attacking
    }
    
    /// Check if a piece is a two-step piece (Tengu, Hook Mover, Peacock, Capricorn, promoted Poisonous Serpent, or promoted Old Kite)
    fn is_two_step_piece(piece: &Piece) -> bool {
        attack_utils::is_tengu_or_promoted_peacock(piece) || 
        attack_utils::is_unpromoted_peacock(piece) || 
        attack_utils::is_hook_mover_like_piece(piece)
    }

    /// Get all royal pieces for a given color
    fn get_royal_pieces(game_state: &GameState, color: Color) -> Vec<Piece> {
        let board = game_state.get_board();
        board.get_pieces_by_color(color)
            .into_iter()
            .filter(|piece| piece.piece_type.is_royal())
            .collect()
    }

    /// Assign priority value to royal pieces
    fn royal_piece_priority(piece: &Piece) -> u8 {
        match piece.piece_type {
            PieceType::King => 3,  // Highest priority
            PieceType::CrownPrince => {
                if piece.is_promoted {
                    1  // Promoted crown prince (lowest)
                } else {
                    2  // Unpromoted crown prince
                }
            }
            _ => 0,
        }
    }

    /// Check if a player has only one royal piece remaining
    fn is_final_royal_piece(game_state: &GameState, color: Color) -> bool {
        Self::get_royal_pieces(game_state, color).len() == 1
    }

    /// Simulate a move and check if a royal piece is still under attack
    fn simulate_move_and_check(
        game_state: &GameState,
        mv: &Move,
        royal_piece_pos: Position,
        defender_color: Color,
    ) -> bool {
        // Create a temporary game state by cloning the board
        let mut temp_state = GameState::new();
        // Copy all pieces from the current board
        let board = game_state.get_board();
        let all_pieces = board.get_pieces_by_color(Color::Black);
        for piece in &all_pieces {
            temp_state.place_piece(*piece);
        }
        let all_pieces = board.get_pieces_by_color(Color::White);
        for piece in &all_pieces {
            temp_state.place_piece(*piece);
        }
        
        // Set the current turn
        // We need to manually set the turn - but GameState doesn't expose this
        // Instead, we'll use a different approach: create a temporary board and manually apply the move
        
        // Actually, let's use a simpler approach: manually check if the move would resolve the check
        // by checking if after the move, the royal piece would still be under attack
        
        // Get the piece making the move
        let Some(moving_piece) = board.get_piece(mv.from) else {
            return false;
        };
        
        use crate::move_simulation::BoardLike;
        let virtual_board = crate::move_simulation::simulate_move(board, mv, &moving_piece);

        // Check if the royal piece is still under attack
        // Note: We need to find where the royal piece is now (it might have moved)
        let royal_piece_final_pos = if mv.from == royal_piece_pos {
            // The royal piece itself moved
            mv.to
        } else {
            // The royal piece didn't move
            royal_piece_pos
        };

        // Check if attacked by opponent (defender_color.opposite()), not by defender's own color
        !BoardLike::is_position_attacked_by_color_for_check(&virtual_board, royal_piece_final_pos, defender_color.opposite())
    }

    /// Check if a position is under attack on a specific board
    /// Uses optimized Board-level attack detection
    /// Optimized for check detection on royal pieces
    fn is_under_attack_on_board(board: &Board, position: Position, defender_color: Color) -> bool {
        let attacker_color = defender_color.opposite();
        board.is_position_attacked_by_color_for_check(position, attacker_color)
    }

    /// Check if a position is under attack on a specific board
    /// Uses optimized Board-level attack detection
    fn is_position_under_attack_on_board(board: &Board, position: Position, defender_color: Color) -> bool {
        let attacker_color = defender_color.opposite();
        board.is_position_attacked_by_color_for_check(position, attacker_color)
    }

    /// Apply a move to a board (handles capturing range movements, piece movement, and promotion)
    /// This is a helper function to factor out move application logic
    fn apply_move_to_board(board: &mut Board, mv: &Move, moving_piece: &Piece) {
        // First, handle capturing range movements
        let config = MovementConfig::for_piece(moving_piece);
        let uses_capturing = config.capabilities.iter().any(|cap| {
            if let crate::movement::types::MovementCapability::Range { blocking, .. } = cap {
                *blocking == crate::movement::BlockingMode::Capturing
            } else {
                false
            }
        });

        if uses_capturing {
            let path_positions = path_utils::get_path_positions(mv.from, mv.to);
            for pos in path_positions {
                if pos != mv.from && pos != mv.to {
                    board.remove_piece(pos);
                }
            }
        }

        // Move the piece
        board.move_piece(mv.from, mv.to);

        // Handle promotion
        if mv.promoted {
            if let Some(mut piece) = board.get_piece(mv.to) {
                piece.promote();
                board.remove_piece(mv.to);
                board.place_piece(piece);
            }
        }
    }

    /// Check if a royal piece would be under attack after making a move
    /// This simulates the move and checks if the destination square is under attack
    fn would_royal_be_under_attack_after_move(game_state: &GameState, mv: &Move, moving_piece: &Piece) -> bool {
        use crate::move_simulation::BoardLike;
        let base_board = game_state.get_board();
        let virtual_board = crate::move_simulation::simulate_move(base_board, mv, moving_piece);
        BoardLike::is_position_attacked_by_color_for_check(&virtual_board, mv.to, moving_piece.color.opposite())
    }

    /// Check if moving a piece would unpin it from a royal piece, exposing the royal to attack
    /// Returns true if the move would unpin and expose a royal piece to attack
    /// Note: This accounts for pieces that can jump over or capture through other pieces
    fn would_move_unpin_royal_piece(board: &Board, mv: &Move, moving_piece: &Piece) -> bool {
        // Get all royal pieces of the same color
        let royal_pieces: Vec<Piece> = board.get_pieces_by_color(moving_piece.color)
            .into_iter()
            .filter(|piece| piece.piece_type.is_royal())
            .collect();

        // For each royal piece, check if the moving piece is pinned to it
        for royal in &royal_pieces {
            // Check if the moving piece is in the same line as the royal piece
            if let Some(direction) = attack_utils::get_direction_toward(royal.position, moving_piece.position) {
                // The moving piece is aligned with the royal piece
                // Check if there's an opponent piece beyond the moving piece that could attack the royal
                
                // Get the direction offset
                let (file_step, rank_step) = direction.to_offset();
                
                // Start from the royal piece and iterate in the direction
                let mut current_pos = royal.position;
                let mut found_moving_piece = false;
                let mut opponent_pieces_beyond = Vec::new();
                
                loop {
                    // Move to the next position in the direction
                    if let Some(next_pos) = current_pos.offset(file_step, rank_step) {
                        current_pos = next_pos;
                        
                        // If we've found the moving piece, continue beyond it
                        if current_pos == moving_piece.position {
                            found_moving_piece = true;
                            continue;
                        }
                        
                        // If we've passed the moving piece, look for opponent pieces
                        if found_moving_piece {
                            if let Some(piece) = board.get_piece(current_pos) {
                                if piece.color != moving_piece.color {
                                    // Found an opponent piece beyond the moving piece
                                    // We don't stop here because pieces might be able to jump over or capture through
                                    opponent_pieces_beyond.push(piece);
                                }
                            }
                            // Continue scanning even through empty squares, because pieces with jumping
                            // or capturing range moves might be able to attack through them
                        }
                    } else {
                        // Out of bounds
                        break;
                    }
                }
                
                // If we found opponent pieces beyond the moving piece, check if they can attack the royal
                // after the move is simulated
                if !opponent_pieces_beyond.is_empty() {
                    use crate::move_simulation::BoardLike;
                    let virtual_board = crate::move_simulation::simulate_move(board, mv, moving_piece);
                    
                    // Check if the royal piece is now under attack by the opponent
                    if BoardLike::is_position_attacked_by_color_for_check(&virtual_board, royal.position, moving_piece.color.opposite()) {
                        return true; // This move would unpin and expose the royal
                    }
                }
            }
        }
        
        false
    }

    /// Check if capturing a piece on the intermediate space of a two-step move would open a line of attack
    /// Returns true if capturing on the intermediate space would expose a royal piece to attack
    fn would_capture_on_intermediate_expose_royal(board: &Board, mv: &Move, moving_piece: &Piece) -> bool {
        // Only check for two-step moving pieces (Tengu or Peacock)
        if !attack_utils::is_tengu_or_promoted_peacock(moving_piece) && !attack_utils::is_unpromoted_peacock(moving_piece) {
            return false;
        }
        
        // Must have an intermediate position
        let Some(intermediate_pos) = mv.intermediate() else {
            return false;
        };
        
        // Check if there's an enemy piece at the intermediate position (before the move)
        let Some(captured_piece) = board.get_piece(intermediate_pos) else {
            return false; // No piece to capture at intermediate
        };
        
        // Must be an enemy piece
        if captured_piece.color == moving_piece.color {
            return false;
        }
        
        // Get all royal pieces of the moving piece's color
        let royal_pieces: Vec<Piece> = board.get_pieces_by_color(moving_piece.color)
            .into_iter()
            .filter(|piece| piece.piece_type.is_royal())
            .collect();
        
        // For each royal piece, check if the intermediate position is on a line with it
        for royal in &royal_pieces {
            // Check if intermediate position is on a line with the royal piece
            if let Some(direction) = attack_utils::get_direction_toward(royal.position, intermediate_pos) {
                // The intermediate position is aligned with the royal piece
                // Check if there's an opponent piece beyond the intermediate position that could attack the royal
                
                // Get the direction offset
                let (file_step, rank_step) = direction.to_offset();
                
                // Start from the royal piece and iterate in the direction
                let mut current_pos = royal.position;
                let mut found_intermediate = false;
                let mut opponent_pieces_beyond = Vec::new();
                
                loop {
                    // Move to the next position in the direction
                    if let Some(next_pos) = current_pos.offset(file_step, rank_step) {
                        current_pos = next_pos;
                        
                        // If we've found the intermediate position, continue beyond it
                        if current_pos == intermediate_pos {
                            found_intermediate = true;
                            continue;
                        }
                        
                        // If we've passed the intermediate position, look for opponent pieces
                        if found_intermediate {
                            if let Some(piece) = board.get_piece(current_pos) {
                                if piece.color != moving_piece.color {
                                    // Found an opponent piece beyond the intermediate position
                                    // Check if it has range movement in the direction toward the royal
                                    if attack_utils::has_range_movement_in_direction(&piece, direction) {
                                        opponent_pieces_beyond.push(piece);
                                    }
                                }
                            }
                            // Continue scanning even through empty squares, because pieces with jumping
                            // or capturing range moves might be able to attack through them
                        }
                    } else {
                        // Out of bounds
                        break;
                    }
                }
                
                // If we found opponent pieces beyond the intermediate position with range movement,
                // check if they can attack the royal after the capture
                if !opponent_pieces_beyond.is_empty() {
                    use crate::move_simulation::{BoardLike, MoveDelta, VirtualBoard};
                    // Create a delta that only removes the captured piece at intermediate
                    let mut delta = MoveDelta::new();
                    if let Some(captured) = board.get_piece(intermediate_pos) {
                        delta.pieces_removed.push((intermediate_pos, captured));
                    }
                    let virtual_board = VirtualBoard::new(board, delta);
                    
                    // Check if the royal piece is now under attack by the opponent
                    if BoardLike::is_position_attacked_by_color_for_check(&virtual_board, royal.position, moving_piece.color.opposite()) {
                        return true; // Capturing on intermediate would expose the royal
                    }
                }
            }
        }
        
        false
    }

    /// Check if any opponent Tengu/Capricorn or Hook Mover can attack a royal piece after a move
    /// Returns true if any such piece can attack any royal piece after the move
    fn would_tengu_attack_royal_after_move(board: &Board, mv: &Move, moving_piece: &Piece) -> bool {
        use crate::move_simulation::BoardLike;
        let virtual_board = crate::move_simulation::simulate_move(board, mv, moving_piece);
        
        // Get all royal pieces of the moving piece's color
        let royal_pieces: Vec<Piece> = BoardLike::get_pieces_by_color(&virtual_board, moving_piece.color)
            .into_iter()
            .filter(|piece| piece.piece_type.is_royal())
            .collect();
        
        // Get all opponent Tengu/Capricorn/promoted Peacock pieces
        let opponent_color = moving_piece.color.opposite();
        let opponent_tengu: Vec<Piece> = BoardLike::get_pieces_by_color(&virtual_board, opponent_color)
            .into_iter()
            .filter(|piece| attack_utils::is_tengu_or_promoted_peacock(piece))
            .collect();
        
        // Get all opponent Hook Mover/promoted forms
        let opponent_hook_mover: Vec<Piece> = BoardLike::get_pieces_by_color(&virtual_board, opponent_color)
            .into_iter()
            .filter(|piece| attack_utils::is_hook_mover_like_piece(piece))
            .collect();
        
        // For each royal piece, check if any Tengu/Capricorn/promoted Peacock can attack it
        // Note: tengu_attack functions require &Board, so we'll use can_reach_boardlike as fallback
        for royal in &royal_pieces {
            for tengu in &opponent_tengu {
                // Use BoardLike's attack detection which will use can_reach for VirtualBoard
                if BoardLike::is_position_attacked_by_color_for_check(&virtual_board, royal.position, opponent_color) {
                    // Check if this specific tengu is the attacker
                    if tengu.can_reach_boardlike(royal.position, &virtual_board) {
                        return true;
                    }
                }
            }
            
            // Check if any Hook Mover/promoted forms can attack it
            for hook_mover in &opponent_hook_mover {
                if hook_mover.can_reach_boardlike(royal.position, &virtual_board) {
                    return true;
                }
            }
        }
        
        false
    }

    /// Check if a move is safe (doesn't put royal pieces into check, doesn't unpin, and doesn't expose to Tengu)
    /// Returns true if the move is safe, false otherwise
    fn is_move_safe(game_state: &GameState, mv: &Move) -> bool {
        // Get the moving piece
        let Some(moving_piece) = game_state.get_board().get_piece(mv.from) else {
            return false; // Piece not found, consider unsafe
        };
        
        use crate::move_simulation::BoardLike;
        let base_board = game_state.get_board();
        let virtual_board = crate::move_simulation::simulate_move(base_board, mv, &moving_piece);
        
        // Check 1: Walking into check (if moving piece is royal)
        if moving_piece.piece_type.is_royal() {
            if BoardLike::is_position_attacked_by_color_for_check(&virtual_board, mv.to, moving_piece.color.opposite()) {
                return false; // Royal piece would be in check at destination
            }
        }
        
        // Check 2: Unpinning detection
        if Self::would_move_unpin_royal_piece(base_board, mv, &moving_piece) {
            return false; // Move would unpin and expose a royal piece
        }
        
        // Check 3: Two-step capture on intermediate exposing royal
        if Self::would_capture_on_intermediate_expose_royal(base_board, mv, &moving_piece) {
            return false; // Capturing on intermediate would expose royal piece to attack
        }
        
        // Check 4: Tengu attack detection
        if Self::would_tengu_attack_royal_after_move(base_board, mv, &moving_piece) {
            return false; // Move would expose royal piece to Tengu attack
        }
        
        // All checks passed
        true
    }

    /// Check if a range capturing move would be safe
    /// Returns true if the destination square would not be under attack after the move
    /// Returns true for non-capturing range moves (no restriction)
    /// Uses lazy evaluation (short-circuits on first attacker found) similar to check detection
    fn would_range_capture_be_worthwhile(board: &Board, mv: &Move, moving_piece: &Piece) -> bool {
        // Check if piece has capturing range movement capability
        let config = MovementConfig::for_piece(moving_piece);
        let uses_capturing = config.capabilities.iter().any(|cap| {
            if let crate::movement::types::MovementCapability::Range { blocking, .. } = cap {
                *blocking == crate::movement::types::BlockingMode::Capturing
            } else {
                false
            }
        });

        // If not a capturing range move, no restriction
        if !uses_capturing {
            return true;
        }

        // Fast check: Count friendly and enemy pieces in the path
        // If capturing at least 1 friendly piece, must capture at least 1 more enemy piece
        let path_positions = path_utils::get_path_positions(mv.from, mv.to);
        let mut friendly_count = 0;
        let mut enemy_count = 0;
        let mut captured_two_step_piece = false;

        // Count pieces in the path (excluding both start and destination)
        for pos in &path_positions {
            if pos == &mv.from || pos == &mv.to {
                continue; // Skip start and destination positions
            }
            
            if let Some(piece) = board.get_piece(*pos) {
                if piece.color == moving_piece.color {
                    friendly_count += 1;
                } else {
                    enemy_count += 1;
                    // Check if this enemy piece is a two-step piece
                    if Self::is_two_step_piece(&piece) {
                        captured_two_step_piece = true;
                    }
                }
            }
        }

        // Also check the destination square - if it has an enemy piece, count it
        if let Some(piece) = board.get_piece(mv.to) {
            if piece.color != moving_piece.color {
                enemy_count += 1;
                // Check if this enemy piece is a two-step piece
                if Self::is_two_step_piece(&piece) {
                    captured_two_step_piece = true;
                }
            }
            // Note: We can't land on friendly pieces, so we don't need to check for that
        }

        // If capturing any friendly pieces, must capture at least 1 more enemy piece
        if friendly_count > 0 {
            if enemy_count < friendly_count + 1 {
                return false; // Fast rejection: not enough enemy pieces captured
            }
        }

        // Exception: If we captured a two-step piece, allow ending in an attacked space
        // (as long as the piece count check passed)
        if captured_two_step_piece {
            return true; // Skip attack detection check
        }

        // Use VirtualBoard instead of cloning
        use crate::move_simulation::BoardLike;
        let virtual_board = crate::move_simulation::simulate_move(board, mv, moving_piece);

        // Check if the destination square would be under attack by the opponent
        // Use regular attack detection (not _for_check) since we're checking if a non-royal piece
        // would be safe - we want to include capturing-only pieces at range
        let attacker_color = moving_piece.color.opposite();
        !BoardLike::is_position_attacked_by_color(&virtual_board, mv.to, attacker_color)
    }

    /// Calculate strategic weight for a move
    /// Weights are stackable (multiply together)
    /// Returns weight value (base = 1.0, can be increased by bonuses)
    fn calculate_move_weight(mv: &Move, moving_piece: &Piece) -> f64 {
        let mut weight = Self::BASE_WEIGHT;
        
        // Forward rank check (stackable)
        // Black: rank increases = forward (rank 0 is back, rank 35 is front)
        // White: rank decreases = forward (rank 35 is back, rank 0 is front)
        // Note: Bonus does NOT apply if piece is already in promotion zone (opponent's first 11 ranks)
        // Black promotion zone: ranks 25-35, White promotion zone: ranks 0-10
        let is_in_promotion_zone = match moving_piece.color {
            Color::Black => mv.from.rank >= 25,  // Black in opponent's first 11 ranks
            Color::White => mv.from.rank <= 10,  // White in opponent's first 11 ranks
        };
        
        if !is_in_promotion_zone {
            if (moving_piece.color == Color::Black && mv.to.rank > mv.from.rank) ||
               (moving_piece.color == Color::White && mv.to.rank < mv.from.rank) {
                weight *= Self::FORWARD_RANK_WEIGHT_MULTIPLIER;
            }
        }
        
        // Center file check (stackable)
        // Center is at file 17.5 (middle of 0-35 range)
        let from_dist = (mv.from.file as f64 - Self::CENTER_FILE).abs();
        let to_dist = (mv.to.file as f64 - Self::CENTER_FILE).abs();
        if to_dist < from_dist {
            weight *= Self::CENTER_FILE_WEIGHT_MULTIPLIER;
        }
        
        // Promotion check (stackable)
        if mv.promoted {
            weight *= Self::PROMOTION_WEIGHT_MULTIPLIER;
        }
        
        weight
    }
    
    /// Select a move using weighted random selection
    /// Returns the index of the selected move, or None if invalid
    fn select_weighted_random_move(moves: &[Move], weights: &[f64], rng: &mut rand::rngs::ThreadRng) -> Option<usize> {
        if moves.is_empty() || weights.is_empty() || moves.len() != weights.len() {
            return None;
        }
        
        let total_weight: f64 = weights.iter().sum();
        if total_weight <= 0.0 {
            return None;
        }
        
        let random = rng.gen_range(0.0..total_weight);
        let mut cumulative = 0.0;
        
        for (i, &weight) in weights.iter().enumerate() {
            cumulative += weight;
            if random < cumulative {
                return Some(i);
            }
        }
        
        // Fallback to last move (shouldn't happen due to floating point, but safety)
        Some(moves.len() - 1)
    }


    /// Find a move that resolves check on a specific royal piece
    fn resolve_check(game_state: &GameState, royal_piece: &Piece) -> Option<Move> {
        let legal_moves = game_state.generate_legal_moves();
        let mut rng = rand::thread_rng();
        let defender_color = royal_piece.color;

        // Step i: Find moves that capture attacking pieces
        let attacking_pieces = Self::get_attacking_pieces(game_state, royal_piece.position, defender_color);
        let attacking_positions: Vec<Position> = if !attacking_pieces.is_empty() {
            attacking_pieces.iter().map(|p| p.position).collect()
        } else {
            Vec::new()
        };

        if !attacking_positions.is_empty() {
            let capture_moves: Vec<&Move> = legal_moves
                .iter()
                .filter(|mv| {
                    // Check if this move captures any attacking piece
                    attacking_positions.contains(&mv.to) ||
                    // For capturing range movements, check if any attacking piece is in the path
                    Self::move_captures_attacker(game_state, mv, &attacking_positions)
                })
                .filter(|mv| {
                    // Verify the move resolves check
                    Self::simulate_move_and_check(game_state, mv, royal_piece.position, defender_color)
                })
                .collect();
            
            if !capture_moves.is_empty() {
                let index = rng.gen_range(0..capture_moves.len());
                return Some(capture_moves[index].clone());
            }
        }

        // Step ii: Try moving the royal piece itself
        let royal_moves: Vec<&Move> = legal_moves
            .iter()
            .filter(|mv| mv.from == royal_piece.position)
            .filter(|mv| {
                Self::simulate_move_and_check(game_state, mv, royal_piece.position, defender_color)
            })
            .collect();
        
        if !royal_moves.is_empty() {
            let index = rng.gen_range(0..royal_moves.len());
            return Some(royal_moves[index].clone());
        }

        // Step iii: Try any other move that resolves check
        let resolving_moves: Vec<&Move> = legal_moves
            .iter()
            .filter(|mv| {
                // Exclude moves we've already tried
                !attacking_positions.contains(&mv.to) && mv.from != royal_piece.position
            })
            .filter(|mv| {
                Self::simulate_move_and_check(game_state, mv, royal_piece.position, defender_color)
            })
            .collect();
        
        if !resolving_moves.is_empty() {
            let index = rng.gen_range(0..resolving_moves.len());
            return Some(resolving_moves[index].clone());
        }

        // Step iv: If no move resolves check, return a random legal move
        if !legal_moves.is_empty() {
            let index = rng.gen_range(0..legal_moves.len());
            return Some(legal_moves[index].clone());
        }

        None
    }

    /// Check if a move captures an attacker (for capturing range movements)
    fn move_captures_attacker(game_state: &GameState, mv: &Move, attacking_positions: &[Position]) -> bool {
        let board = game_state.get_board();
        let Some(moving_piece) = board.get_piece(mv.from) else {
            return false;
        };

        let config = crate::movement::MovementConfig::for_piece(&moving_piece);
        let uses_capturing = config.capabilities.iter().any(|cap| {
            if let crate::movement::MovementCapability::Range { blocking, .. } = cap {
                *blocking == crate::movement::BlockingMode::Capturing
            } else {
                false
            }
        });

        if uses_capturing {
            let path_positions = path_utils::get_path_positions(mv.from, mv.to);
            for pos in path_positions {
                if attacking_positions.contains(&pos) {
                    return true;
                }
            }
        }

        false
    }
}

