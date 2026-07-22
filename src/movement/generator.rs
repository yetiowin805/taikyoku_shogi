use crate::move_simulation::BoardLike;
use crate::piece::{Piece, Color};
use crate::position::Position;
use crate::movement::types::{MovementCapability, BlockingMode};
use crate::movement::direction::{Direction, DirectionSet, direction_set_to_directions};
use crate::path_utils;

pub struct MovementGenerator;

impl MovementGenerator {
    /// Generate all possible target positions for a piece given its movement capabilities
    pub fn generate_targets<B: BoardLike>(
        piece: &Piece,
        board: &B,
        capabilities: &[MovementCapability],
    ) -> Vec<Position> {
        Self::generate_targets_filtered(piece, board, capabilities, false)
    }

    /// Like [`generate_targets`], but when `captures_only` is set only returns
    /// landings that capture an enemy (or capturing-range path clears).
    pub fn generate_targets_filtered<B: BoardLike>(
        piece: &Piece,
        board: &B,
        capabilities: &[MovementCapability],
        captures_only: bool,
    ) -> Vec<Position> {
        let mut targets = Vec::new();

        for capability in capabilities {
            let mut new_targets =
                Self::generate_for_capability(piece, board, capability, captures_only);
            targets.append(&mut new_targets);
        }

        // Remove duplicates
        targets.sort();
        targets.dedup();
        targets
    }

    /// Generate targets for a single movement capability
    fn generate_for_capability<B: BoardLike>(
        piece: &Piece,
        board: &B,
        capability: &MovementCapability,
        captures_only: bool,
    ) -> Vec<Position> {
        match capability {
            MovementCapability::Simple {
                directions,
                max_distance,
            } => Self::generate_simple(piece, board, *directions, *max_distance, captures_only),
            MovementCapability::Range {
                directions,
                blocking,
                cannot_jump_over,
            } => Self::generate_range(
                piece,
                board,
                *directions,
                *blocking,
                cannot_jump_over,
                captures_only,
            ),
            MovementCapability::Jumping { offsets } => {
                Self::generate_jumping(piece, board, offsets, captures_only)
            }
            MovementCapability::TwoStep { first, second } => {
                Self::generate_two_step(piece, board, first, second, captures_only)
            }
            MovementCapability::ConditionalDiagonalJump {
                directions,
                base_jump,
                conditional_jumps,
                required_jump_positions,
                empty_after_jump,
            } => Self::generate_conditional_diagonal_jump(
                piece,
                board,
                *directions,
                *base_jump,
                conditional_jumps,
                *required_jump_positions,
                *empty_after_jump,
                captures_only,
            ),
            MovementCapability::FreeEagleMultiMove {
                max_distance_forward_diagonal,
                max_distance_other,
            } => Self::generate_free_eagle_multi_move(
                piece,
                board,
                *max_distance_forward_diagonal,
                *max_distance_other,
                captures_only,
            ),
        }
    }

    /// Adjust directions for piece color (for pawn and gold general)
    /// Black moves "up" (increasing rank), White moves "down" (decreasing rank)
    fn adjust_directions_for_color(directions: DirectionSet, color: Color) -> DirectionSet {
        if color == Color::Black {
            // No adjustment needed for Black (N is forward)
            directions
        } else {
            // For White, flip directions: N<->S, NE<->SW, E<->W, SE<->NW
            let mut adjusted = 0u8;
            if directions & Direction::N.to_bit() != 0 { adjusted |= Direction::S.to_bit(); }
            if directions & Direction::S.to_bit() != 0 { adjusted |= Direction::N.to_bit(); }
            if directions & Direction::NE.to_bit() != 0 { adjusted |= Direction::SW.to_bit(); }
            if directions & Direction::SW.to_bit() != 0 { adjusted |= Direction::NE.to_bit(); }
            if directions & Direction::E.to_bit() != 0 { adjusted |= Direction::E.to_bit(); } // E stays E
            if directions & Direction::W.to_bit() != 0 { adjusted |= Direction::W.to_bit(); } // W stays W
            if directions & Direction::SE.to_bit() != 0 { adjusted |= Direction::NW.to_bit(); }
            if directions & Direction::NW.to_bit() != 0 { adjusted |= Direction::SE.to_bit(); }
            adjusted
        }
    }

    /// Generate targets for simple movement
    fn generate_simple<B: BoardLike>(
        piece: &Piece,
        board: &B,
        directions: DirectionSet,
        max_distance: u8,
        captures_only: bool,
    ) -> Vec<Position> {
        let mut targets = Vec::new();
        
        // Adjust directions for color (for pawn and gold general)
        let adjusted_directions = Self::adjust_directions_for_color(directions, piece.color);
        
        for direction in direction_set_to_directions(adjusted_directions) {
            let (file_delta, rank_delta) = direction.to_offset();
            
            for distance in 1..=max_distance {
                let file_offset = file_delta * distance as i8;
                let rank_offset = rank_delta * distance as i8;
                
                let Some(target) = piece.position.offset(file_offset, rank_offset) else {
                    break; // Out of bounds, stop in this direction
                };
                
                // For simple movement with max_distance > 1, check if path is clear
                if max_distance > 1 {
                    if !path_utils::is_path_clear_for_boardlike(board, piece.position, target) {
                        // Path is blocked - find the first blocking piece along the path
                        let path_positions = path_utils::get_path_positions(piece.position, target);
                        for path_pos in path_positions {
                            if let Some(blocking_piece) = board.get_piece(path_pos) {
                                // Found the first blocking piece
                                if blocking_piece.color != piece.color {
                                    targets.push(path_pos); // Can capture enemy blocking piece
                                }
                                break; // Stop in this direction
                            }
                        }
                        break; // Stop in this direction
                    }
                }
                
                // Check if target has a friendly piece - cannot land on friendly
                if let Some(target_piece) = board.get_piece(target) {
                    if target_piece.color == piece.color {
                        // Can't land on friendly piece - stop in this direction
                        break;
                    }
                    targets.push(target); // enemy
                } else if !captures_only {
                    targets.push(target);
                }
            }
        }
        
        targets
    }


    /// Generate targets for range movement
    fn generate_range<B: BoardLike>(
        piece: &Piece,
        board: &B,
        directions: DirectionSet,
        blocking: BlockingMode,
        cannot_jump_over: &std::collections::HashSet<crate::piece::PieceType>,
        captures_only: bool,
    ) -> Vec<Position> {
        let mut targets = Vec::new();
        
        // Adjust directions for color (for pieces that need it)
        let adjusted_directions = Self::adjust_directions_for_color(directions, piece.color);
        
        for direction in direction_set_to_directions(adjusted_directions) {
            let (file_delta, rank_delta) = direction.to_offset();
            let mut distance = 1;
            let mut ray_has_enemy_capture = false;
            
            loop {
                let file_offset = file_delta * distance as i8;
                let rank_offset = rank_delta * distance as i8;
                
                let Some(target) = piece.position.offset(file_offset, rank_offset) else {
                    break; // Out of bounds
                };
                
                let target_piece = board.get_piece(target);
                let is_empty = target_piece.is_none();
                let is_friendly = target_piece.map(|p| p.color == piece.color).unwrap_or(false);
                let is_enemy = target_piece.map(|p| p.color != piece.color).unwrap_or(false);
                
                match blocking {
                    BlockingMode::NoJump => {
                        if is_enemy {
                            targets.push(target);
                        } else if is_empty && !captures_only {
                            targets.push(target);
                        }
                        // Stop at first piece (whether we can capture it or not)
                        if !is_empty {
                            break;
                        }
                    }
                    BlockingMode::Jump => {
                        if is_enemy {
                            targets.push(target);
                        } else if !is_friendly && !captures_only {
                            targets.push(target);
                        }
                    }
                    BlockingMode::Capturing => {
                        if let Some(piece_in_path) = target_piece {
                            if cannot_jump_over.contains(&piece_in_path.piece_type) {
                                break;
                            }
                        }
                        if is_enemy {
                            ray_has_enemy_capture = true;
                        }
                        if !is_friendly {
                            if !captures_only || is_enemy || (is_empty && ray_has_enemy_capture) {
                                targets.push(target);
                            }
                        }
                    }
                }
                
                distance += 1;
            }
        }
        
        targets
    }

    /// Generate targets for jumping movement
    /// Offsets are relative to Black's perspective (N is up, S is down)
    /// For White pieces, offsets are automatically flipped (rank_delta is negated)
    fn generate_jumping<B: BoardLike>(
        piece: &Piece,
        board: &B,
        offsets: &[(i8, i8)],
        captures_only: bool,
    ) -> Vec<Position> {
        let mut targets = Vec::new();
        
        for &(file_delta, rank_delta) in offsets {
            // Adjust rank_delta for White (flip vertically)
            let adjusted_rank_delta = if piece.color == Color::White {
                -rank_delta
            } else {
                rank_delta
            };
            
            if let Some(target) = piece.position.offset(file_delta, adjusted_rank_delta) {
                // Jumping moves are never blocked, but can't land on friendly pieces
                if let Some(target_piece) = board.get_piece(target) {
                    if target_piece.color != piece.color {
                        targets.push(target);
                    }
                } else if !captures_only {
                    targets.push(target);
                }
            }
        }
        
        targets
    }

    /// Generate targets for two-step movement
    fn generate_two_step<B: BoardLike>(
        piece: &Piece,
        board: &B,
        first: &MovementCapability,
        second: &MovementCapability,
        captures_only: bool,
    ) -> Vec<Position> {
        let mut targets = Vec::new();
        
        // Intermediates need full first-leg targets even for captures_only.
        let first_targets = Self::generate_for_capability(piece, board, first, false);
        
        if captures_only {
            targets.extend(first_targets.iter().copied().filter(|pos| {
                board
                    .get_piece(*pos)
                    .is_some_and(|p| p.color != piece.color)
            }));
        } else {
            targets.extend(first_targets.iter().copied());
        }
        
        for intermediate_pos in first_targets {
            let mut temp_piece = *piece;
            temp_piece.position = intermediate_pos;
            let second_targets =
                Self::generate_for_capability(&temp_piece, board, second, captures_only);
            if captures_only {
                let inter_captures = board
                    .get_piece(intermediate_pos)
                    .is_some_and(|p| p.color != piece.color);
                for t in second_targets {
                    let dest_captures = board
                        .get_piece(t)
                        .is_some_and(|p| p.color != piece.color);
                    if inter_captures || dest_captures {
                        targets.push(t);
                    }
                }
            } else {
                targets.extend(second_targets);
            }
        }
        
        targets
    }

    /// Generate targets for conditional diagonal jump movement
    /// This handles patterns like the Wooden Dove where:
    /// - Can jump base_jump spaces normally (jumping over anything)
    /// - Can jump conditional_jumps distances if the first required_jump_positions have pieces
    ///   and the next empty_after_jump positions are empty
    fn generate_conditional_diagonal_jump<B: BoardLike>(
        piece: &Piece,
        board: &B,
        directions: DirectionSet,
        base_jump: u8,
        conditional_jumps: &[u8],
        required_jump_positions: u8,
        empty_after_jump: u8,
        captures_only: bool,
    ) -> Vec<Position> {
        let mut targets = Vec::new();
        
        // Adjust directions for color (though diagonal directions shouldn't need adjustment)
        let adjusted_directions = Self::adjust_directions_for_color(directions, piece.color);
        
        // Only process diagonal directions
        let diagonal_directions = vec![
            Direction::NE, Direction::SE, Direction::SW, Direction::NW
        ];
        
        for direction in diagonal_directions {
            // Check if this direction is allowed
            if (adjusted_directions & direction.to_bit()) == 0 {
                continue;
            }
            
            let (file_delta, rank_delta) = direction.to_offset();
            
            // Generate base jump (normal jump, can jump over anything)
            if let Some(target) = piece.position.offset(
                file_delta * base_jump as i8,
                rank_delta * base_jump as i8,
            ) {
                // Can land on empty or enemy square
                if let Some(target_piece) = board.get_piece(target) {
                    if target_piece.color != piece.color {
                        targets.push(target); // Can capture enemy
                    }
                } else {
                    targets.push(target); // Empty square
                }
            }
            
            // Check each conditional jump distance individually
            // The rule is: can only jump over pieces that would be jumped in a 3-space jump (positions 1-2)
            // But positions 1-2 don't need to have pieces - they just can't have pieces beyond position 2
            // For 4-space jump: position 3 must be empty
            // For 5-space jump: positions 3 and 4 must be empty
            for &jump_distance in conditional_jumps {
                // Check that position 3 is empty (required for both 4 and 5 space jumps)
                let pos3 = 3;
                let pos3_empty = if let Some(pos) = piece.position.offset(
                    file_delta * pos3 as i8,
                    rank_delta * pos3 as i8,
                ) {
                    board.get_piece(pos).is_none()
                } else {
                    false // Out of bounds
                };
                
                if !pos3_empty {
                    continue; // Position 3 has a piece, can't do this jump distance
                }
                
                // For 5-space jump, also check that position 4 is empty
                if jump_distance == 5 {
                    let pos4 = 4;
                    let pos4_empty = if let Some(pos) = piece.position.offset(
                        file_delta * pos4 as i8,
                        rank_delta * pos4 as i8,
                    ) {
                        board.get_piece(pos).is_none()
                    } else {
                        false // Out of bounds
                    };
                    
                    if !pos4_empty {
                        continue; // Position 4 has a piece, can't do 5-space jump
                    }
                }
                
                // Check if we can land at the target position
                if let Some(target) = piece.position.offset(
                    file_delta * jump_distance as i8,
                    rank_delta * jump_distance as i8,
                ) {
                    if let Some(target_piece) = board.get_piece(target) {
                        if target_piece.color != piece.color {
                            targets.push(target); // Can capture enemy
                        }
                        // Can't land on friendly piece
                    } else if !captures_only {
                        targets.push(target); // Empty square
                    }
                }
            }
        }
        
        targets
    }
    
    /// Generate Free Eagle multi-move patterns
    /// Returns positions that can be reached via multi-move patterns
    /// Note: The actual Move objects with paths will be created in game_state.rs
    fn generate_free_eagle_multi_move<B: BoardLike>(
        piece: &Piece,
        board: &B,
        max_distance_forward_diagonal: u8,
        max_distance_other: u8,
        captures_only: bool,
    ) -> Vec<Position> {
        let mut targets = Vec::new();
        
        // Forward diagonals for this color
        let forward_diagonals = match piece.color {
            Color::Black => vec![Direction::NE, Direction::NW],
            Color::White => vec![Direction::SE, Direction::SW],
        };
        
        // Other directions (orthogonal and backward diagonals)
        let other_directions = match piece.color {
            Color::Black => vec![Direction::N, Direction::S, Direction::E, Direction::W, Direction::SE, Direction::SW],
            Color::White => vec![Direction::N, Direction::S, Direction::E, Direction::W, Direction::NE, Direction::NW],
        };
        
        // Pattern 1 & 2: Multi-move patterns
        // Generate all possible final destinations for forward diagonals (up to max_distance_forward_diagonal)
        for direction in &forward_diagonals {
            let (file_delta, rank_delta) = direction.to_offset();
            let mut path = vec![piece.position];
            let mut current = piece.position;
            
            for distance in 1..=max_distance_forward_diagonal {
                let Some(next) = current.offset(file_delta, rank_delta) else {
                    break; // Out of bounds
                };
                
                if let Some(p) = board.get_piece(next) {
                    if p.color == piece.color {
                        break; // Blocked by friendly piece
                    }
                    // Enemy piece - can capture and continue
                    path.push(next);
                    current = next;
                    targets.push(next);
                } else {
                    path.push(next);
                    current = next;
                    if !captures_only {
                        targets.push(next);
                    }
                }
            }
        }
        
        // Generate all possible final destinations for other directions (up to max_distance_other)
        for direction in other_directions {
            let (file_delta, rank_delta) = direction.to_offset();
            let mut current = piece.position;
            
            for distance in 1..=max_distance_other {
                let Some(next) = current.offset(file_delta, rank_delta) else {
                    break; // Out of bounds
                };
                
                if let Some(p) = board.get_piece(next) {
                    if p.color == piece.color {
                        break; // Blocked by friendly piece
                    }
                    // Enemy piece - can capture and continue
                    current = next;
                    targets.push(next);
                } else {
                    // Empty square
                    current = next;
                    if !captures_only {
                        targets.push(next);
                    }
                }
            }
        }
        
        // Pattern 3: Special forward diagonal (3 forward + 1 back) - only if capture on 3rd space
        for direction in &forward_diagonals {
            let (file_delta, rank_delta) = direction.to_offset();
            let back_delta = (-file_delta, -rank_delta);
            
            // Move 3 spaces forward
            let mut pos3 = piece.position;
            let mut valid = true;
            for _ in 0..3 {
                let Some(next) = pos3.offset(file_delta, rank_delta) else {
                    valid = false;
                    break;
                };
                if let Some(p) = board.get_piece(next) {
                    if p.color == piece.color {
                        valid = false;
                        break;
                    }
                }
                pos3 = next;
            }
            
            if !valid {
                continue;
            }
            
            // Check if there's a capture on the 3rd space
            if let Some(p) = board.get_piece(pos3) {
                if p.color != piece.color {
                    // There's an enemy piece on the 3rd space - pattern is valid
                    // Move 1 space back
                    if let Some(final_pos) = pos3.offset(back_delta.0, back_delta.1) {
                        if let Some(p) = board.get_piece(final_pos) {
                            if p.color != piece.color {
                                targets.push(final_pos); // Can capture on backward step
                            }
                        } else {
                            targets.push(final_pos); // Empty square
                        }
                    }
                }
            }
        }
        
        // Pattern 4: Special any direction (2 forward + 1 back) - only if capture on 2nd space
        let all_directions = Direction::all();
        for direction in all_directions {
            let (file_delta, rank_delta) = direction.to_offset();
            let back_delta = (-file_delta, -rank_delta);
            
            // Move 2 spaces forward
            let mut pos2 = piece.position;
            let mut valid = true;
            for _ in 0..2 {
                let Some(next) = pos2.offset(file_delta, rank_delta) else {
                    valid = false;
                    break;
                };
                if let Some(p) = board.get_piece(next) {
                    if p.color == piece.color {
                        valid = false;
                        break;
                    }
                }
                pos2 = next;
            }
            
            if !valid {
                continue;
            }
            
            // Check if there's a capture on the 2nd space
            if let Some(p) = board.get_piece(pos2) {
                if p.color != piece.color {
                    // There's an enemy piece on the 2nd space - pattern is valid
                    // Move 1 space back
                    if let Some(final_pos) = pos2.offset(back_delta.0, back_delta.1) {
                        if let Some(p) = board.get_piece(final_pos) {
                            if p.color != piece.color {
                                targets.push(final_pos); // Can capture on backward step
                            }
                        } else {
                            targets.push(final_pos); // Empty square
                        }
                    }
                }
            }
        }
        
        // Pattern 5: Stay in place while capturing enemy 1 space away in any direction
        if Self::check_pattern5(piece, board).is_some() {
            targets.push(piece.position);
        }
        
        // Pattern 6: Stay in place while capturing on 2nd space along forward diagonal
        if Self::check_pattern6(piece, board).is_some() {
            targets.push(piece.position);
        }
        
        // Remove duplicates
        targets.sort();
        targets.dedup();
        targets
    }
    
    /// Check if Pattern 5 is valid (enemy 1 space away in any direction)
    /// Returns Some(capture_pos) if valid, None otherwise
    pub fn check_pattern5<B: BoardLike>(piece: &Piece, board: &B) -> Option<Position> {
        use crate::movement::direction::Direction;
        
        for direction in Direction::all() {
            let (file_delta, rank_delta) = direction.to_offset();
            
            if let Some(capture_pos) = piece.position.offset(file_delta, rank_delta) {
                if let Some(target_piece) = board.get_piece(capture_pos) {
                    if target_piece.color != piece.color {
                        return Some(capture_pos);
                    }
                }
            }
        }
        None
    }
    
    /// Check if Pattern 6 is valid (capture on 2nd space along forward diagonal)
    /// Returns Some((pos1, pos2)) if valid, None otherwise
    /// pos1 may be None if the first position is empty
    pub fn check_pattern6<B: BoardLike>(piece: &Piece, board: &B) -> Option<(Option<Position>, Position)> {
        use crate::movement::direction::Direction;
        
        let forward_diagonals = match piece.color {
            Color::Black => vec![Direction::NE, Direction::NW],
            Color::White => vec![Direction::SE, Direction::SW],
        };
        
        for direction in &forward_diagonals {
            let (file_delta, rank_delta) = direction.to_offset();
            
            // Check position 1 space away
            let pos1 = piece.position.offset(file_delta, rank_delta);
            
            // Check position 2 spaces away (only if we can reach it)
            if let Some(pos1) = pos1 {
                // Check if position 1 is empty or has an enemy (can jump over enemy)
                let can_reach_pos2 = if let Some(p) = board.get_piece(pos1) {
                    p.color != piece.color // Can jump over enemy at pos1
                } else {
                    true // Empty at pos1, can continue
                };
                
                if can_reach_pos2 {
                    if let Some(pos2) = pos1.offset(file_delta, rank_delta) {
                        if let Some(p) = board.get_piece(pos2) {
                            if p.color != piece.color {
                                return Some((Some(pos1), pos2));
                            }
                        }
                    }
                }
            }
        }
        None
    }
}

