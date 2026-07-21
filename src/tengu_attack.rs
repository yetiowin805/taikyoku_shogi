use crate::board::Board;
use crate::piece::{Piece, Color};
use crate::position::Position;
use crate::path_utils;
use crate::move_simulation::BoardLike;

/// Check if a Tengu (or promoted Peacock) can attack a target position
/// This is optimized to only check the 2 potential two-step paths instead of generating all moves
pub fn can_tengu_attack_target<B: BoardLike>(tengu: &Piece, target: Position, board: &B) -> bool {
    // First check: can reach via single diagonal range move (path is clear)?
    if can_reach_via_diagonal_range(tengu, tengu.position, target, board) {
        return true;
    }
    
    // Special case: If Tengu and target are on the same diagonal with exactly one enemy piece
    // (same color as target) between them, Tengu can attack via two-step:
    // First move: capture the intervening enemy piece
    // Second move: capture the target/royal piece
    if is_on_same_diagonal(tengu.position, target) {
        if find_single_enemy_blocker(tengu, target, board).is_some() {
            // The blocking piece is already verified to be:
            // - An enemy of the Tengu (different color)
            // - Same color as the target (if target has a piece)
            // So this is a valid two-step attack: capture blocker, then capture target
            return true;
        }
    }
    
    // Third check: can reach via two-step move with calculated intermediate positions?
    // Find the 2 potential intermediate positions
    let intermediates = find_potential_intermediates(tengu.position, target);
    
    for intermediate in intermediates {
        // Check if first step (Tengu -> intermediate) is legal
        if !can_reach_via_diagonal_range(tengu, tengu.position, intermediate, board) {
            continue;
        }
        
        // Check if second step (intermediate -> target) is legal
        // Create a temporary piece at intermediate position
        let mut temp_piece = *tengu;
        temp_piece.position = intermediate;
        
        if can_reach_via_diagonal_range(&temp_piece, intermediate, target, board) {
            // Check if target has a friendly piece (can't capture friendly)
            if let Some(target_piece) = BoardLike::get_piece(board, target) {
                if target_piece.color == tengu.color {
                    continue; // Can't capture friendly piece
                }
            }
            return true; // Found a valid two-step path
        }
    }
    
    false
}

/// Check if two positions are on the same diagonal
fn is_on_same_diagonal(from: Position, to: Position) -> bool {
    let file_diff = (to.file as i16 - from.file as i16).abs();
    let rank_diff = (to.rank as i16 - from.rank as i16).abs();
    file_diff == rank_diff && file_diff > 0
}

/// Find the 2 potential intermediate positions for a two-step diagonal move
/// Returns 0, 1, or 2 valid intermediate positions
fn find_potential_intermediates(tengu_pos: Position, target: Position) -> Vec<Position> {
    let mut intermediates = Vec::new();
    
    let f_t = tengu_pos.file as i16;
    let r_t = tengu_pos.rank as i16;
    let f_g = target.file as i16;
    let r_g = target.rank as i16;
    
    // Case 1: f_i - f_t = r_i - r_t AND f_g - f_i = r_g - r_i
    // f_i = (f_t - r_t + f_g + r_g) / 2
    // r_i = (f_g + r_g - f_t + r_t) / 2
    let sum1 = f_t - r_t + f_g + r_g;
    let sum2 = f_g + r_g - f_t + r_t;
    if sum1 % 2 == 0 && sum2 % 2 == 0 {
        let f_i = sum1 / 2;
        let r_i = sum2 / 2;
        if f_i >= 0 && f_i < 36 && r_i >= 0 && r_i < 36 {
            if let Some(intermediate) = Position::new(f_i as u8, r_i as u8) {
                if intermediate != tengu_pos && intermediate != target {
                    intermediates.push(intermediate);
                }
            }
        }
    }
    
    // Case 2: f_i - f_t = -(r_i - r_t) AND f_g - f_i = -(r_g - r_i)
    // f_i = (f_t + r_t + f_g - r_g) / 2
    // r_i = (f_t + r_t - f_g + r_g) / 2
    let sum1 = f_t + r_t + f_g - r_g;
    let sum2 = f_t + r_t - f_g + r_g;
    if sum1 % 2 == 0 && sum2 % 2 == 0 {
        let f_i = sum1 / 2;
        let r_i = sum2 / 2;
        if f_i >= 0 && f_i < 36 && r_i >= 0 && r_i < 36 {
            if let Some(intermediate) = Position::new(f_i as u8, r_i as u8) {
                if intermediate != tengu_pos && intermediate != target {
                    // Avoid duplicates (in case both cases yield the same result)
                    if !intermediates.contains(&intermediate) {
                        intermediates.push(intermediate);
                    }
                }
            }
        }
    }
    
    intermediates
}


/// Find exactly one enemy piece (same color as target) blocking the diagonal path
/// Returns Some(piece) if exactly one enemy piece is found, None otherwise
/// The blocker must be:
/// - An enemy of the Tengu (different color from Tengu)
/// - Same color as the target piece (if target has a piece)
fn find_single_enemy_blocker<B: BoardLike>(tengu: &Piece, target: Position, board: &B) -> Option<Piece> {
    let file_diff = target.file as i8 - tengu.position.file as i8;
    let rank_diff = target.rank as i8 - tengu.position.rank as i8;
    
    // Must be diagonal
    if file_diff.abs() != rank_diff.abs() {
        return None;
    }
    
    // Normalize direction (step size 1 in both file and rank)
    let file_step = if file_diff == 0 { 0 } else if file_diff > 0 { 1 } else { -1 };
    let rank_step = if rank_diff == 0 { 0 } else if rank_diff > 0 { 1 } else { -1 };
    
    // Get target piece color (if any) - blocker should be same color as target
    let target_piece_color = board.get_piece(target).map(|p| p.color);
    
    // Check each square along the path (excluding the destination)
    let mut current_file = tengu.position.file as i8 + file_step;
    let mut current_rank = tengu.position.rank as i8 + rank_step;
    let mut blocker_count = 0;
    let mut found_blocker: Option<Piece> = None;
    
    while current_file != target.file as i8 || current_rank != target.rank as i8 {
        if let Some(pos) = Position::new(current_file as u8, current_rank as u8) {
            if let Some(piece) = board.get_piece(pos) {
                // Found a piece in the path
                blocker_count += 1;

                if blocker_count > 1 {
                    return None;
                }
                
                // Check if it's an enemy piece (different color from Tengu)
                if piece.color == tengu.color {
                    // Friendly piece blocks the path - invalid
                    return None;
                }
                
                // Check if blocker is same color as target (if target has a piece)
                if let Some(target_color) = target_piece_color {
                    if piece.color == target_color {
                        found_blocker = Some(piece);
                    } else {
                        // Enemy piece but wrong color - invalid
                        return None;
                    }
                } else {
                    // Target is empty, so any enemy piece works
                    found_blocker = Some(piece);
                }
            }
        } else {
            // Out of bounds
            return None;
        }
        current_file += file_step;
        current_rank += rank_step;
    }
    
    // Must have exactly one blocker
    if blocker_count == 1 {
        found_blocker
    } else {
        None
    }
}

/// Check if a piece can reach a target via single diagonal range move
fn can_reach_via_diagonal_range<B: BoardLike>(piece: &Piece, from: Position, to: Position, board: &B) -> bool {
    // Must be on the same diagonal
    if !is_on_same_diagonal(from, to) {
        return false;
    }
    
    // Path must be clear (excluding destination)
    if !path_utils::is_diagonal_path_clear_for_boardlike(board, from, to) {
        return false;
    }
    
    // Can land on target (empty or enemy piece, not friendly)
    if let Some(target_piece) = BoardLike::get_piece(board, to) {
        if target_piece.color == piece.color {
            return false; // Can't land on friendly piece
        }
    }
    
    true
}

/// Check if an unpromoted Peacock can attack a target position
/// This is optimized to only check the 2 potential two-step paths instead of generating all moves
pub fn can_peacock_attack_target<B: BoardLike>(peacock: &Piece, target: Position, board: &B) -> bool {
    // First check: can reach via single backwards diagonal simple move (up to 2 spaces)?
    if can_reach_via_backwards_diagonal_simple(peacock, peacock.position, target, board) {
        return true;
    }
    
    // Special case: If Peacock and target are on the same diagonal with exactly one enemy piece
    // (same color as target) between them, Peacock can attack via two-step:
    // First move: capture the intervening enemy piece
    // Second move: capture the target/royal piece
    if is_on_same_diagonal(peacock.position, target) {
        if is_forward_diagonal(peacock, peacock.position, target) {
            if find_single_enemy_blocker(peacock, target, board).is_some() {
                // The blocking piece is already verified to be:
                // - An enemy of the Peacock (different color)
                // - Same color as the target (if target has a piece)
                // So this is a valid two-step attack: capture blocker, then capture target
                return true;
            }
        }
    }
    
    // Third check: can reach via two-step move?
    // Find the 2 potential intermediate positions
    let intermediates = find_potential_intermediates(peacock.position, target);
    
    for intermediate in intermediates {
        // Check if first step (Peacock -> intermediate) is forward diagonal range
        if !can_reach_via_forward_diagonal_range(peacock, peacock.position, intermediate, board) {
            continue;
        }
        
        // Check if second step (intermediate -> target) is any diagonal range
        // Create a temporary piece at intermediate position
        let mut temp_piece = *peacock;
        temp_piece.position = intermediate;
        
        if can_reach_via_diagonal_range(&temp_piece, intermediate, target, board) {
            // Check if target has a friendly piece (can't capture friendly)
            if let Some(target_piece) = BoardLike::get_piece(board, target) {
                if target_piece.color == peacock.color {
                    continue; // Can't capture friendly piece
                }
            }
            return true; // Found a valid two-step path
        }
    }
    
    false
}

/// Check if a move is in a forward diagonal direction relative to piece color
/// For Black: forward diagonals are NE (file+, rank+) and NW (file-, rank+)
/// For White: forward diagonals are SE (file+, rank-) and SW (file-, rank-)
fn is_forward_diagonal(piece: &Piece, from: Position, to: Position) -> bool {
    let file_diff = to.file as i8 - from.file as i8;
    let rank_diff = to.rank as i8 - from.rank as i8;
    
    // Must be diagonal
    if file_diff.abs() != rank_diff.abs() {
        return false;
    }
    
    // Check if it's forward relative to piece color
    match piece.color {
        Color::Black => {
            // Forward for Black means rank increases (rank_diff > 0)
            rank_diff > 0
        }
        Color::White => {
            // Forward for White means rank decreases (rank_diff < 0)
            rank_diff < 0
        }
    }
}

/// Check if a move is in a backwards diagonal direction relative to piece color
/// For Black: backwards diagonals are SE (file+, rank-) and SW (file-, rank-)
/// For White: backwards diagonals are NE (file+, rank+) and NW (file-, rank+)
fn is_backwards_diagonal(piece: &Piece, from: Position, to: Position) -> bool {
    let file_diff = to.file as i8 - from.file as i8;
    let rank_diff = to.rank as i8 - from.rank as i8;
    
    // Must be diagonal
    if file_diff.abs() != rank_diff.abs() {
        return false;
    }
    
    // Check if it's backwards relative to piece color
    match piece.color {
        Color::Black => {
            // Backwards for Black means rank decreases (rank_diff < 0)
            rank_diff < 0
        }
        Color::White => {
            // Backwards for White means rank increases (rank_diff > 0)
            rank_diff > 0
        }
    }
}

/// Check if a piece can reach a target via backwards diagonal simple move (up to 2 spaces)
fn can_reach_via_backwards_diagonal_simple<B: BoardLike>(piece: &Piece, from: Position, to: Position, board: &B) -> bool {
    // Must be backwards diagonal
    if !is_backwards_diagonal(piece, from, to) {
        return false;
    }
    
    // Check distance (must be <= 2)
    let file_diff = (to.file as i16 - from.file as i16).abs();
    if file_diff > 2 {
        return false;
    }
    
    // Path must be clear (excluding destination)
    if !path_utils::is_diagonal_path_clear_for_boardlike(board, from, to) {
        return false;
    }
    
    // Can land on target (empty or enemy piece, not friendly)
    if let Some(target_piece) = BoardLike::get_piece(board, to) {
        if target_piece.color == piece.color {
            return false; // Can't land on friendly piece
        }
    }
    
    true
}

/// Check if a piece can reach a target via forward diagonal range move
fn can_reach_via_forward_diagonal_range<B: BoardLike>(piece: &Piece, from: Position, to: Position, board: &B) -> bool {
    // Must be forward diagonal
    if !is_forward_diagonal(piece, from, to) {
        return false;
    }
    
    // Path must be clear (excluding destination)
    if !path_utils::is_diagonal_path_clear_for_boardlike(board, from, to) {
        return false;
    }
    
    // Can land on target (empty or enemy piece, not friendly)
    if let Some(target_piece) = BoardLike::get_piece(board, to) {
        if target_piece.color == piece.color {
            return false; // Can't land on friendly piece
        }
    }
    
    true
}

/// Check if a Hook Mover (or promoted Capricorn/PoisonousSerpent) can attack a target position
/// This is optimized to only check the 2 potential two-step paths instead of generating all moves
pub fn can_hook_mover_attack_target<B: BoardLike>(hook_mover: &Piece, target: Position, board: &B) -> bool {
    // First check: can reach via single orthogonal range move (path is clear)?
    if can_reach_via_orthogonal_range(hook_mover, hook_mover.position, target, board) {
        return true;
    }
    
    // Special case: If Hook Mover and target are on the same orthogonal line with exactly one enemy piece
    // (same color as target) between them, Hook Mover can attack via two-step:
    // First move: capture the intervening enemy piece
    // Second move: capture the target/royal piece
    if is_on_same_orthogonal(hook_mover.position, target) {
        if find_single_enemy_blocker_orthogonal(hook_mover, target, board).is_some() {
            // The blocking piece is already verified to be:
            // - An enemy of the Hook Mover (different color)
            // - Same color as the target (if target has a piece)
            // So this is a valid two-step attack: capture blocker, then capture target
            return true;
        }
    }
    
    // Third check: can reach via two-step move with calculated intermediate positions?
    // Find the 2 potential intermediate positions
    let intermediates = find_potential_intermediates_orthogonal(hook_mover.position, target);
    
    for intermediate in intermediates {
        // Check if first step (Hook Mover -> intermediate) is legal
        if !can_reach_via_orthogonal_range(hook_mover, hook_mover.position, intermediate, board) {
            continue;
        }
        
        // Check if second step (intermediate -> target) is legal
        // Create a temporary piece at intermediate position
        let mut temp_piece = *hook_mover;
        temp_piece.position = intermediate;
        
        if can_reach_via_orthogonal_range(&temp_piece, intermediate, target, board) {
            // Check if target has a friendly piece (can't capture friendly)
            if let Some(target_piece) = BoardLike::get_piece(board, target) {
                if target_piece.color == hook_mover.color {
                    continue; // Can't capture friendly piece
                }
            }
            return true; // Found a valid two-step path
        }
    }
    
    false
}

/// Check if two positions are on the same orthogonal line (same file or same rank)
fn is_on_same_orthogonal(from: Position, to: Position) -> bool {
    from.file == to.file || from.rank == to.rank
}

/// Find the 2 potential intermediate positions for a two-step orthogonal move
/// Returns 0, 1, or 2 valid intermediate positions
/// Only calculates intermediates for L-shaped paths (different file and rank)
/// Same file/rank cases are handled by single-step or blocker-capture checks
fn find_potential_intermediates_orthogonal(hook_mover_pos: Position, target: Position) -> Vec<Position> {
    let mut intermediates = Vec::new();
    
    let f_h = hook_mover_pos.file as i16;
    let r_h = hook_mover_pos.rank as i16;
    let f_t = target.file as i16;
    let r_t = target.rank as i16;
    
    // Only handle L-shaped paths (different file and different rank)
    // Same file/rank cases are already handled by:
    // 1. Single-step orthogonal range (if path is clear)
    // 2. Two-step via blocker capture (if exactly one enemy blocker exists)
    // For orthogonal moves, we can go: (f_h, r_h) -> (f_h, r_t) -> (f_t, r_t) OR (f_h, r_h) -> (f_t, r_h) -> (f_t, r_t)
    // So we have two potential intermediates: (f_h, r_t) and (f_t, r_h)
    if f_h != f_t && r_h != r_t {
        // Intermediate 1: (f_h, r_t) - same file as hook_mover, same rank as target
        if let Some(intermediate1) = Position::new(f_h as u8, r_t as u8) {
            if intermediate1 != hook_mover_pos && intermediate1 != target {
                intermediates.push(intermediate1);
            }
        }
        
        // Intermediate 2: (f_t, r_h) - same file as target, same rank as hook_mover
        if let Some(intermediate2) = Position::new(f_t as u8, r_h as u8) {
            if intermediate2 != hook_mover_pos && intermediate2 != target {
                if !intermediates.contains(&intermediate2) {
                    intermediates.push(intermediate2);
                }
            }
        }
    }
    
    intermediates
}


/// Find exactly one enemy piece (same color as target) blocking the orthogonal path
/// Returns Some(piece) if exactly one enemy piece is found, None otherwise
/// The blocker must be:
/// - An enemy of the Hook Mover (different color from Hook Mover)
/// - Same color as the target piece (if target has a piece)
fn find_single_enemy_blocker_orthogonal<B: BoardLike>(hook_mover: &Piece, target: Position, board: &B) -> Option<Piece> {
    let file_diff = target.file as i8 - hook_mover.position.file as i8;
    let rank_diff = target.rank as i8 - hook_mover.position.rank as i8;
    
    // Must be orthogonal
    if file_diff != 0 && rank_diff != 0 {
        return None;
    }
    if file_diff == 0 && rank_diff == 0 {
        return None; // Same position
    }
    
    // Normalize direction (step size 1 in either file or rank)
    let file_step = if file_diff == 0 { 0 } else if file_diff > 0 { 1 } else { -1 };
    let rank_step = if rank_diff == 0 { 0 } else if rank_diff > 0 { 1 } else { -1 };
    
    // Get target piece color (if any) - blocker should be same color as target
    let target_piece_color = BoardLike::get_piece(board, target).map(|p| p.color);
    
    // Check each square along the path (excluding the destination)
    let mut current_file = hook_mover.position.file as i8 + file_step;
    let mut current_rank = hook_mover.position.rank as i8 + rank_step;
    let mut blocker_count = 0;
    let mut found_blocker: Option<Piece> = None;
    
    while current_file != target.file as i8 || current_rank != target.rank as i8 {
        if let Some(pos) = Position::new(current_file as u8, current_rank as u8) {
            if let Some(piece) = BoardLike::get_piece(board, pos) {
                // Found a piece in the path
                blocker_count += 1;

                if blocker_count > 1 {
                    return None;
                }
                
                // Check if it's an enemy piece (different color from Hook Mover)
                if piece.color == hook_mover.color {
                    // Friendly piece blocks the path - invalid
                    return None;
                }
                
                // Check if blocker is same color as target (if target has a piece)
                if let Some(target_color) = target_piece_color {
                    if piece.color == target_color {
                        found_blocker = Some(piece);
                    } else {
                        // Enemy piece but wrong color - invalid
                        return None;
                    }
                } else {
                    // Target is empty, so any enemy piece works
                    found_blocker = Some(piece);
                }
            }
        } else {
            // Out of bounds
            return None;
        }
        current_file += file_step;
        current_rank += rank_step;
    }
    
    // Must have exactly one blocker
    if blocker_count == 1 {
        found_blocker
    } else {
        None
    }
}

/// Check if a piece can reach a target via single orthogonal range move
fn can_reach_via_orthogonal_range<B: BoardLike>(piece: &Piece, from: Position, to: Position, board: &B) -> bool {
    // Must be on the same orthogonal line
    if !is_on_same_orthogonal(from, to) {
        return false;
    }
    
    // Path must be clear (excluding destination)
    if !path_utils::is_orthogonal_path_clear_for_boardlike(board, from, to) {
        return false;
    }
    
    // Can land on target (empty or enemy piece, not friendly)
    if let Some(target_piece) = BoardLike::get_piece(board, to) {
        if target_piece.color == piece.color {
            return false; // Can't land on friendly piece
        }
    }
    
    true
}

/// Check if a Cannon Soldier can attack a target position
/// This is optimized to check the 7-move forward simple movement capability directly
/// Cannon Soldier has unique 7-move forward simple movement (max is otherwise 5)
pub fn can_cannon_soldier_attack_target<B: BoardLike>(cannon_soldier: &Piece, target: Position, board: &B) -> bool {
    use crate::movement::direction::Direction;
    use crate::movement::types::MovementCapability;
    
    // Get movement config for Cannon Soldier
    let config = crate::movement::MovementConfig::for_piece(cannon_soldier);
    
    // Find the 7-move forward simple movement capability
    for capability in &config.capabilities {
        if let MovementCapability::Simple { directions, max_distance } = capability {
            // Check if this is the forward direction (N for Black, S for White)
            // and has max_distance 7
            if *max_distance == 7 {
                // Adjust directions for color
                let adjusted_directions = crate::attack_utils::adjust_directions_for_color(*directions, cannon_soldier.color);
                
                // Check if target is reachable in the forward direction
                let forward_direction = if cannon_soldier.color == Color::Black {
                    Direction::N
                } else {
                    Direction::S
                };
                
                // Check if the adjusted directions include forward
                if (adjusted_directions & forward_direction.to_bit()) != 0 {
                    // Check if target is on the forward line from cannon_soldier position
                    let file_diff = target.file as i8 - cannon_soldier.position.file as i8;
                    let rank_diff = target.rank as i8 - cannon_soldier.position.rank as i8;
                    
                    // Must be on same file (orthogonal forward)
                    if file_diff == 0 {
                        // Check if rank_diff is in the correct direction and within range
                        let correct_direction = if cannon_soldier.color == Color::Black {
                            rank_diff > 0 && rank_diff <= 7
                        } else {
                            rank_diff < 0 && rank_diff >= -7
                        };
                        
                        if correct_direction {
                            // Check if path is clear (excluding destination)
                            if path_utils::is_orthogonal_path_clear_for_boardlike(board, cannon_soldier.position, target) {
                                // Check if target has a friendly piece (can't capture friendly)
                                if let Some(target_piece) = BoardLike::get_piece(board, target) {
                                    if target_piece.color == cannon_soldier.color {
                                        return false; // Can't capture friendly piece
                                    }
                                }
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Also check other movement capabilities (backwards, sideways, diagonals)
    // Use can_reach_boardlike for these (works with BoardLike)
    cannon_soldier.can_reach_boardlike(target, board)
}

/// Check if a Lion Hawk can attack a target position
/// Lion Hawk has two two-step options:
/// 1. Simple 1 space orthogonally → Simple 1 space any direction
/// 2. Range diagonal → Simple 1 space any direction
pub fn can_lion_hawk_attack_target<B: BoardLike>(lion_hawk: &Piece, target: Position, board: &B) -> bool {
    if lion_hawk.position == target {
        return false;
    }
    // Can't capture friendly
    if let Some(target_piece) = BoardLike::get_piece(board, target) {
        if target_piece.color == lion_hawk.color {
            return false;
        }
    }

    let file_diff = (target.file as i8 - lion_hawk.position.file as i8).abs();
    let rank_diff = (target.rank as i8 - lion_hawk.position.rank as i8).abs();

    // Jump: all squares at Chebyshev distance 2 (same as Lion)
    if file_diff.max(rank_diff) == 2 {
        return true;
    }

    // TwoStep includes first-step-only destinations:
    // - orthogonal simple 1
    if (file_diff == 1 && rank_diff == 0) || (file_diff == 0 && rank_diff == 1) {
        return true;
    }
    // - diagonal range any distance
    if can_reach_via_diagonal_range(lion_hawk, lion_hawk.position, target, board) {
        return true;
    }

    // Option 1 completed: orthogonal 1 → any 1
    for (file_delta, rank_delta) in [(0i8, 1i8), (0, -1), (1, 0), (-1, 0)] {
        if let Some(intermediate) = lion_hawk.position.offset(file_delta, rank_delta) {
            if let Some(piece_at_intermediate) = BoardLike::get_piece(board, intermediate) {
                if piece_at_intermediate.color == lion_hawk.color {
                    continue;
                }
            }
            if can_reach_via_simple_1_any_direction(lion_hawk, intermediate, target, board) {
                return true;
            }
        }
    }

    // Option 2 completed: diagonal range → any 1
    // Intermediate must be adjacent to the target (second step is only 1 square).
    // Tengu-style find_potential_intermediates is wrong here: when LI and target share a
    // diagonal it returns no intermediates, and second-step distance is not a diagonal range.
    for (file_delta, rank_delta) in [
        (0i8, 1i8),
        (0, -1),
        (1, 0),
        (-1, 0),
        (1, 1),
        (1, -1),
        (-1, 1),
        (-1, -1),
    ] {
        let Some(intermediate) = target.offset(file_delta, rank_delta) else {
            continue;
        };
        if intermediate == lion_hawk.position {
            continue;
        }
        if can_reach_via_diagonal_range(lion_hawk, lion_hawk.position, intermediate, board) {
            return true;
        }
    }

    false
}

/// Check if a piece can reach a target via simple 1 space any direction
/// This is used for the second step of Lion Hawk's two-step move
fn can_reach_via_simple_1_any_direction<B: BoardLike>(piece: &Piece, from: Position, to: Position, board: &B) -> bool {
    let file_diff = to.file as i8 - from.file as i8;
    let rank_diff = to.rank as i8 - from.rank as i8;
    
    // Must be exactly 1 space away (any of the 8 directions)
    if file_diff.abs() > 1 || rank_diff.abs() > 1 {
        return false;
    }
    
    // Can't be the same position
    if file_diff == 0 && rank_diff == 0 {
        return false;
    }
    
    // Can land on target (empty or enemy piece, not friendly)
    if let Some(target_piece) = BoardLike::get_piece(board, to) {
        if target_piece.color == piece.color {
            return false; // Can't land on friendly piece
        }
    }
    
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;
    use crate::piece::{Color, Piece, PieceType};

    #[test]
    fn lion_hawk_attacks_clear_long_diagonal() {
        let mut board = Board::new();
        let li_pos = Position::new(5, 5).unwrap();
        let target = Position::new(10, 10).unwrap();
        let li = Piece::new(PieceType::LionHawk, Color::White, li_pos);
        board.place_piece(li);
        board.place_piece(Piece::new(PieceType::King, Color::Black, target));

        assert!(can_lion_hawk_attack_target(&li, target, &board));
        assert!(board.is_position_attacked_by_color_for_check(target, Color::White));
    }

    #[test]
    fn lion_hawk_blocked_on_diagonal() {
        let mut board = Board::new();
        let li_pos = Position::new(5, 5).unwrap();
        let blocker = Position::new(7, 7).unwrap();
        let target = Position::new(10, 10).unwrap();
        let li = Piece::new(PieceType::LionHawk, Color::White, li_pos);
        board.place_piece(li);
        board.place_piece(Piece::new(PieceType::Pawn, Color::Black, blocker));
        board.place_piece(Piece::new(PieceType::King, Color::Black, target));

        assert!(!can_lion_hawk_attack_target(&li, target, &board));
        assert!(!board.is_position_attacked_by_color_for_check(target, Color::White));
    }

    #[test]
    fn lion_hawk_attacks_via_diagonal_then_orthogonal_step() {
        let mut board = Board::new();
        let li_pos = Position::new(5, 5).unwrap();
        // Not on LI's diagonal; reachable via range to (9,9) then step to (9,10)
        let target = Position::new(9, 10).unwrap();
        let li = Piece::new(PieceType::LionHawk, Color::White, li_pos);
        board.place_piece(li);
        board.place_piece(Piece::new(PieceType::King, Color::Black, target));

        assert!(can_lion_hawk_attack_target(&li, target, &board));
    }

    #[test]
    fn lion_hawk_jumps_chebyshev_two() {
        let mut board = Board::new();
        let li_pos = Position::new(10, 10).unwrap();
        let target = Position::new(12, 11).unwrap();
        let li = Piece::new(PieceType::LionHawk, Color::Black, li_pos);
        board.place_piece(li);

        assert!(can_lion_hawk_attack_target(&li, target, &board));
    }
}
