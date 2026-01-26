use crate::piece::{Piece, PieceType, Color};
use crate::position::Position;
use crate::movement::{MovementConfig, direction::{Direction, direction_set_contains}};

/// Check if a piece is Tengu, promoted Peacock, promoted Capricorn, or promoted Old Kite
pub fn is_tengu_or_promoted_peacock(piece: &Piece) -> bool {
    piece.piece_type == PieceType::Tengu || 
    piece.piece_type == PieceType::Capricorn ||
    (piece.piece_type == PieceType::Peacock && piece.is_promoted) ||
    (piece.piece_type == PieceType::OldKite && piece.is_promoted)
}

/// Check if a piece is Hook Mover, promoted Capricorn, or promoted Poisonous Serpent (all have Hook Mover movement)
pub fn is_hook_mover_like_piece(piece: &Piece) -> bool {
    piece.piece_type == PieceType::HookMover ||
    (piece.piece_type == PieceType::Capricorn && piece.is_promoted) ||
    (piece.piece_type == PieceType::PoisonousSerpent && piece.is_promoted)
}

/// Check if a piece is unpromoted Peacock
pub fn is_unpromoted_peacock(piece: &Piece) -> bool {
    piece.piece_type == PieceType::Peacock && !piece.is_promoted
}

/// Check if a piece is Cannon Soldier
pub fn is_cannon_soldier(piece: &Piece) -> bool {
    piece.piece_type == PieceType::CannonSoldier
}

/// Check if a piece is Lion Hawk
pub fn is_lion_hawk(piece: &Piece) -> bool {
    piece.piece_type == PieceType::LionHawk
}

/// Check if a Cannon Soldier is in the 7-space forward path toward a target position
/// Returns true if the Cannon Soldier is on the same file as the target and within 7 spaces forward
pub fn is_cannon_soldier_in_forward_path(cannon_soldier: &Piece, target: Position) -> bool {
    // Must be on same file (orthogonal forward)
    let file_diff = target.file as i8 - cannon_soldier.position.file as i8;
    let rank_diff = target.rank as i8 - cannon_soldier.position.rank as i8;
    
    if file_diff != 0 {
        return false; // Not on same file
    }
    
    // Check if rank_diff is in the correct forward direction and within 7 spaces
    if cannon_soldier.color == Color::Black {
        // Black moves forward (increasing rank), so target should be forward
        rank_diff > 0 && rank_diff <= 7
    } else {
        // White moves forward (decreasing rank), so target should be forward
        rank_diff < 0 && rank_diff >= -7
    }
}

/// Check if two positions are within a 9x9 square (5 squares in each direction)
pub fn is_within_9x9_square(piece_pos: Position, target_pos: Position) -> bool {
    let file_diff = (piece_pos.file as i8 - target_pos.file as i8).abs();
    let rank_diff = (piece_pos.rank as i8 - target_pos.rank as i8).abs();
    file_diff <= 5 && rank_diff <= 5
}

/// Get the direction from one position to another
/// Returns Some(Direction) if positions are aligned in one of the 8 directions
/// Returns None if not aligned
pub fn get_direction_toward(from: Position, to: Position) -> Option<Direction> {
    let file_diff = to.file as i8 - from.file as i8;
    let rank_diff = to.rank as i8 - from.rank as i8;
    
    // Same position
    if file_diff == 0 && rank_diff == 0 {
        return None;
    }
    
    // Orthogonal directions
    if file_diff == 0 {
        return if rank_diff > 0 {
            Some(Direction::N)
        } else {
            Some(Direction::S)
        };
    }
    
    if rank_diff == 0 {
        return if file_diff > 0 {
            Some(Direction::E)
        } else {
            Some(Direction::W)
        };
    }
    
    // Diagonal directions - must have equal absolute values
    if file_diff.abs() == rank_diff.abs() {
        return match (file_diff > 0, rank_diff > 0) {
            (true, true) => Some(Direction::NE),
            (true, false) => Some(Direction::SE),
            (false, false) => Some(Direction::SW),
            (false, true) => Some(Direction::NW),
        };
    }
    
    // Not aligned in any of the 8 directions
    None
}

/// Adjust directions for piece color (180-degree rotation for White)
/// Black moves "up" (increasing rank), White moves "down" (decreasing rank)
/// Configs store directions from Black's perspective, so we flip for White
pub fn adjust_directions_for_color(directions: crate::movement::direction::DirectionSet, color: Color) -> crate::movement::direction::DirectionSet {
    use crate::movement::direction::Direction;
    
    if color == Color::Black {
        // No adjustment needed for Black (N is forward)
        directions
    } else {
        // For White, flip all directions in 180-degree rotation: N<->S, E<->W, NE<->SW, SE<->NW
        let mut adjusted = 0u8;
        if directions & Direction::N.to_bit() != 0 { adjusted |= Direction::S.to_bit(); }
        if directions & Direction::S.to_bit() != 0 { adjusted |= Direction::N.to_bit(); }
        if directions & Direction::E.to_bit() != 0 { adjusted |= Direction::W.to_bit(); }
        if directions & Direction::W.to_bit() != 0 { adjusted |= Direction::E.to_bit(); }
        if directions & Direction::NE.to_bit() != 0 { adjusted |= Direction::SW.to_bit(); }
        if directions & Direction::SW.to_bit() != 0 { adjusted |= Direction::NE.to_bit(); }
        if directions & Direction::SE.to_bit() != 0 { adjusted |= Direction::NW.to_bit(); }
        if directions & Direction::NW.to_bit() != 0 { adjusted |= Direction::SE.to_bit(); }
        adjusted
    }
}

/// Check if a piece has range movement capability in a given direction
/// The direction is absolute (from get_direction_toward), and we need to check against
/// the color-adjusted direction set from the config
pub fn has_range_movement_in_direction(piece: &Piece, direction: Direction) -> bool {
    let config = MovementConfig::for_piece(piece);
    
    for capability in &config.capabilities {
        if let crate::movement::types::MovementCapability::Range { directions, .. } = capability {
            // Adjust directions for color (same as movement generation does)
            let adjusted_directions = adjust_directions_for_color(*directions, piece.color);
            
            // Check if the adjusted direction set contains the given direction
            if direction_set_contains(adjusted_directions, direction) {
                return true;
            }
        }
    }
    
    false
}

/// Check if a piece has ONLY capturing range movement (no other range movement types)
/// Such pieces cannot attack royal pieces at range since royal pieces are in their cannot_jump_over set
pub fn has_only_capturing_range_movement(piece: &Piece) -> bool {
    use crate::movement::types::BlockingMode;
    
    let config = MovementConfig::for_piece(piece);
    
    let mut has_capturing_range = false;
    let mut has_other_range = false;
    
    for capability in &config.capabilities {
        match capability {
            crate::movement::types::MovementCapability::Range { blocking, .. } => {
                if *blocking == BlockingMode::Capturing {
                    has_capturing_range = true;
                } else {
                    has_other_range = true; // Has NoJump or Jump range movement
                }
            }
            _ => {
                // Has other movement types (Simple, Jumping, TwoStep, etc.)
                // These are fine, we only care about range movement types
            }
        }
    }
    
    // Has capturing range but no other range movement types
    has_capturing_range && !has_other_range
}

/// Returns true if piece should be checked for attacking a specific target position
/// Filters based on proximity to target position and movement capabilities
/// 
/// `for_check`: If true, optimizes for check detection on royal pieces by treating
///              pieces with only capturing range movement as short-range only
pub fn should_check_piece_for_target_position(
    piece: &Piece, 
    target_position: Position,
    for_check: bool,
) -> bool {
    // Always check Tengu/promoted Peacock/Capricorn and unpromoted Peacock due to highly mobile two-step movement
    if is_tengu_or_promoted_peacock(piece) || is_unpromoted_peacock(piece) {
        return true;
    }
    
    // Always check Hook Mover and its promoted forms due to highly mobile two-step movement
    if is_hook_mover_like_piece(piece) {
        return true;
    }
    
    // Check Lion Hawk: can reach via diagonal range + 1 space any direction
    // Optimized: check if coordinate changes differ by at most 1
    if is_lion_hawk(piece) {
        let file_diff = (target_position.file as i8 - piece.position.file as i8).abs();
        let rank_diff = (target_position.rank as i8 - piece.position.rank as i8).abs();
        // For diagonal range + 1 space: |file_diff| and |rank_diff| differ by at most 1
        if (file_diff as i16 - rank_diff as i16).abs() <= 1 {
            return true;
        }
        // Also check if within 9x9 square (covers orthogonal simple + 1 space option)
        if is_within_9x9_square(piece.position, target_position) {
            return true;
        }
    }
    
    // Check Cannon Soldier only if it's in the 7-space forward path
    // (Otherwise it will be caught by 9x9 square filter or can't reach anyway)
    if is_cannon_soldier(piece) {
        if is_cannon_soldier_in_forward_path(piece, target_position) {
            return true;
        }
    }
    
    // Check if piece is within 9x9 square of target position
    if is_within_9x9_square(piece.position, target_position) {
        return true;
    }
    
    // For check detection, skip long-range check for pieces with only capturing range movement
    // (They can't attack royal pieces at range anyway)
    if for_check && has_only_capturing_range_movement(piece) {
        return false; // Only short-range check (9x9 square) already done above
    }
    
    // Check if piece has range movement in direction toward target position
    if let Some(direction) = get_direction_toward(piece.position, target_position) {
        if has_range_movement_in_direction(piece, direction) {
            return true;
        }
    }
    
    false
}

