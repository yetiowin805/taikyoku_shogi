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

        // Priority 5: Weighted random move (excluding moves that leave a royal in check)
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

    /// After simulating `mv`, true if any royal of `moving_piece`'s color is in check.
    /// This is the general safety net for discovered checks (sliders, Lion Hawk, Tengu, etc.).
    fn any_royal_in_check_after(board: &Board, mv: &Move, moving_piece: &Piece) -> bool {
        use crate::move_simulation::BoardLike;
        let virtual_board = crate::move_simulation::simulate_move(board, mv, moving_piece);
        let opponent = moving_piece.color.opposite();

        for royal in BoardLike::get_pieces_by_color(&virtual_board, moving_piece.color) {
            if !royal.piece_type.is_royal() {
                continue;
            }
            if BoardLike::is_position_attacked_by_color_for_check(
                &virtual_board,
                royal.position,
                opponent,
            ) {
                return true;
            }
        }
        false
    }

    /// Check if a move is safe (does not leave any friendly royal in check).
    fn is_move_safe(game_state: &GameState, mv: &Move) -> bool {
        let Some(moving_piece) = game_state.get_board().get_piece(mv.from) else {
            return false;
        };
        !Self::any_royal_in_check_after(game_state.get_board(), mv, &moving_piece)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// White Lion Hawk on a diagonal toward Black's king, blocked by a Black pawn.
    /// Moving the pawn off the diagonal discovers check — mi must reject that move.
    #[test]
    fn rejects_move_that_opens_lion_hawk_discovered_check() {
        let mut state = GameState::new();
        let king_pos = Position::new(10, 10).unwrap();
        let blocker_pos = Position::new(7, 7).unwrap();
        let li_pos = Position::new(5, 5).unwrap();
        let off_diagonal = Position::new(7, 8).unwrap();

        state.place_piece(Piece::new(PieceType::King, Color::Black, king_pos));
        state.place_piece(Piece::new(PieceType::Pawn, Color::Black, blocker_pos));
        state.place_piece(Piece::new(PieceType::LionHawk, Color::White, li_pos));
        // Unrelated piece whose move does not open check
        let free_pos = Position::new(20, 10).unwrap();
        state.place_piece(Piece::new(PieceType::Pawn, Color::Black, free_pos));
        state.set_current_turn(Color::Black);

        assert!(
            !state
                .get_board()
                .is_position_attacked_by_color_for_check(king_pos, Color::White),
            "king should not be in check while the diagonal is blocked"
        );

        let opens_check = Move::new(blocker_pos, off_diagonal);
        assert!(
            !MinimalIntelligencePlayer::is_move_safe(&state, &opens_check),
            "moving the blocker off the LI diagonal must be unsafe"
        );

        let safe = Move::new(free_pos, Position::new(20, 11).unwrap());
        assert!(
            MinimalIntelligencePlayer::is_move_safe(&state, &safe),
            "an unrelated forward pawn move should remain safe"
        );
    }

    #[test]
    fn rejects_move_that_opens_lance_discovered_check() {
        let mut state = GameState::new();
        let king_pos = Position::new(10, 10).unwrap();
        let blocker_pos = Position::new(10, 15).unwrap();
        let lance_pos = Position::new(10, 20).unwrap();

        state.place_piece(Piece::new(PieceType::King, Color::Black, king_pos));
        state.place_piece(Piece::new(PieceType::GoldGeneral, Color::Black, blocker_pos));
        state.place_piece(Piece::new(PieceType::Lance, Color::White, lance_pos));
        state.set_current_turn(Color::Black);

        let opens_check = Move::new(blocker_pos, Position::new(11, 15).unwrap());
        assert!(!MinimalIntelligencePlayer::is_move_safe(&state, &opens_check));
    }
}
