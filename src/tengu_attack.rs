use crate::move_simulation::{BoardAfterStep, BoardLike};
use crate::path_utils;
use crate::piece::{Color, Piece};
use crate::position::Position;

/// Check if a Tengu (or promoted Peacock) can attack a target position
/// This is optimized to only check the 2 potential two-step paths instead of generating all moves
pub fn can_tengu_attack_target<B: BoardLike>(tengu: &Piece, target: Position, board: &B) -> bool {
    // First check: can reach via single diagonal range move (path is clear)?
    if can_reach_via_diagonal_range(tengu, tengu.position, target, board) {
        return true;
    }

    // Otherwise try a perpendicular turn through an empty corner.
    // Find the 2 potential intermediate positions
    let intermediates = find_potential_intermediates(tengu.position, target);

    for intermediate in intermediates.into_iter().flatten() {
        if !board.is_empty(intermediate)
            || !path_utils::is_perpendicular_turn(tengu.position, intermediate, target)
        {
            continue;
        }
        // Check if first step (Tengu -> intermediate) is legal
        if !can_reach_via_diagonal_range(tengu, tengu.position, intermediate, board) {
            continue;
        }

        // Check if second step (intermediate -> target) is legal
        // Create a temporary piece at intermediate position
        let mut temp_piece = *tengu;
        temp_piece.position = intermediate;
        let after_first = BoardAfterStep::new(board, *tengu, intermediate);

        if can_reach_via_diagonal_range(&temp_piece, intermediate, target, &after_first) {
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
    let file_diff = (to.file() as i16 - from.file() as i16).abs();
    let rank_diff = (to.rank() as i16 - from.rank() as i16).abs();
    file_diff == rank_diff && file_diff > 0
}

/// Find the 2 potential intermediate positions for a two-step diagonal move
/// Returns 0, 1, or 2 valid intermediate positions
fn find_potential_intermediates(tengu_pos: Position, target: Position) -> [Option<Position>; 2] {
    let mut intermediates = [None; 2];

    let f_t = tengu_pos.file() as i16;
    let r_t = tengu_pos.rank() as i16;
    let f_g = target.file() as i16;
    let r_g = target.rank() as i16;

    // Case 1: f_i - f_t = r_i - r_t AND f_g - f_i = r_g - r_i
    // f_i = (f_t - r_t + f_g + r_g) / 2
    // r_i = (f_g + r_g - f_t + r_t) / 2
    let sum1 = f_t - r_t + f_g + r_g;
    let sum2 = f_g + r_g - f_t + r_t;
    if sum1 % 2 == 0 && sum2 % 2 == 0 {
        let f_i = sum1 / 2;
        let r_i = sum2 / 2;
        if (0..36).contains(&f_i) && (0..36).contains(&r_i) {
            if let Some(intermediate) = Position::new(f_i as u8, r_i as u8) {
                if intermediate != tengu_pos && intermediate != target {
                    intermediates[0] = Some(intermediate);
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
        if (0..36).contains(&f_i) && (0..36).contains(&r_i) {
            if let Some(intermediate) = Position::new(f_i as u8, r_i as u8) {
                if intermediate != tengu_pos && intermediate != target {
                    // Avoid duplicates (in case both cases yield the same result)
                    if !intermediates.contains(&Some(intermediate)) {
                        intermediates[1] = Some(intermediate);
                    }
                }
            }
        }
    }

    intermediates
}

/// Check if a piece can reach a target via single diagonal range move
fn can_reach_via_diagonal_range<B: BoardLike>(
    piece: &Piece,
    from: Position,
    to: Position,
    board: &B,
) -> bool {
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
pub fn can_peacock_attack_target<B: BoardLike>(
    peacock: &Piece,
    target: Position,
    board: &B,
) -> bool {
    // First check: can reach via single backwards diagonal simple move (up to 2 spaces)?
    if can_reach_via_backwards_diagonal_simple(peacock, peacock.position, target, board) {
        return true;
    }
    // The first leg of a TwoStep is also a complete legal move. Same-diagonal
    // targets have no turning intermediates, so the fallback below cannot find it.
    if can_reach_via_forward_diagonal_range(peacock, peacock.position, target, board) {
        return true;
    }

    // Otherwise try a perpendicular turn through an empty corner.
    // Find the 2 potential intermediate positions
    let intermediates = find_potential_intermediates(peacock.position, target);

    for intermediate in intermediates.into_iter().flatten() {
        if !board.is_empty(intermediate)
            || !path_utils::is_perpendicular_turn(peacock.position, intermediate, target)
        {
            continue;
        }
        // Check if first step (Peacock -> intermediate) is forward diagonal range
        if !can_reach_via_forward_diagonal_range(peacock, peacock.position, intermediate, board) {
            continue;
        }

        // Check if second step (intermediate -> target) is any diagonal range
        // Create a temporary piece at intermediate position
        let mut temp_piece = *peacock;
        temp_piece.position = intermediate;
        let after_first = BoardAfterStep::new(board, *peacock, intermediate);

        if can_reach_via_diagonal_range(&temp_piece, intermediate, target, &after_first) {
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
    let file_diff = to.file() as i8 - from.file() as i8;
    let rank_diff = to.rank() as i8 - from.rank() as i8;

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
    let file_diff = to.file() as i8 - from.file() as i8;
    let rank_diff = to.rank() as i8 - from.rank() as i8;

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
fn can_reach_via_backwards_diagonal_simple<B: BoardLike>(
    piece: &Piece,
    from: Position,
    to: Position,
    board: &B,
) -> bool {
    // Must be backwards diagonal
    if !is_backwards_diagonal(piece, from, to) {
        return false;
    }

    // Check distance (must be <= 2)
    let file_diff = (to.file() as i16 - from.file() as i16).abs();
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
fn can_reach_via_forward_diagonal_range<B: BoardLike>(
    piece: &Piece,
    from: Position,
    to: Position,
    board: &B,
) -> bool {
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
pub fn can_hook_mover_attack_target<B: BoardLike>(
    hook_mover: &Piece,
    target: Position,
    board: &B,
) -> bool {
    // First check: can reach via single orthogonal range move (path is clear)?
    if can_reach_via_orthogonal_range(hook_mover, hook_mover.position, target, board) {
        return true;
    }

    // Otherwise try a perpendicular turn through an empty corner.
    // Find the 2 potential intermediate positions
    let intermediates = find_potential_intermediates_orthogonal(hook_mover.position, target);

    for intermediate in intermediates.into_iter().flatten() {
        if !board.is_empty(intermediate)
            || !path_utils::is_perpendicular_turn(hook_mover.position, intermediate, target)
        {
            continue;
        }
        // Check if first step (Hook Mover -> intermediate) is legal
        if !can_reach_via_orthogonal_range(hook_mover, hook_mover.position, intermediate, board) {
            continue;
        }

        // Check if second step (intermediate -> target) is legal
        // Create a temporary piece at intermediate position
        let mut temp_piece = *hook_mover;
        temp_piece.position = intermediate;
        let after_first = BoardAfterStep::new(board, *hook_mover, intermediate);

        if can_reach_via_orthogonal_range(&temp_piece, intermediate, target, &after_first) {
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
    from.file() == to.file() || from.rank() == to.rank()
}

/// Find the 2 potential intermediate positions for a two-step orthogonal move
/// Returns 0, 1, or 2 valid intermediate positions
/// Only calculates intermediates for L-shaped paths (different file and rank)
/// Same file/rank cases can only use a single slide.
fn find_potential_intermediates_orthogonal(
    hook_mover_pos: Position,
    target: Position,
) -> [Option<Position>; 2] {
    let mut intermediates = [None; 2];

    let f_h = hook_mover_pos.file() as i16;
    let r_h = hook_mover_pos.rank() as i16;
    let f_t = target.file() as i16;
    let r_t = target.rank() as i16;

    // Only handle L-shaped paths (different file and different rank)
    // Same file/rank cases can only use the single-step orthogonal range.
    // For orthogonal moves, we can go: (f_h, r_h) -> (f_h, r_t) -> (f_t, r_t) OR (f_h, r_h) -> (f_t, r_h) -> (f_t, r_t)
    // So we have two potential intermediates: (f_h, r_t) and (f_t, r_h)
    if f_h != f_t && r_h != r_t {
        // Intermediate 1: (f_h, r_t) - same file as hook_mover, same rank as target
        if let Some(intermediate1) = Position::new(f_h as u8, r_t as u8) {
            if intermediate1 != hook_mover_pos && intermediate1 != target {
                intermediates[0] = Some(intermediate1);
            }
        }

        // Intermediate 2: (f_t, r_h) - same file as target, same rank as hook_mover
        if let Some(intermediate2) = Position::new(f_t as u8, r_h as u8) {
            if intermediate2 != hook_mover_pos
                && intermediate2 != target
                && !intermediates.contains(&Some(intermediate2))
            {
                intermediates[1] = Some(intermediate2);
            }
        }
    }

    intermediates
}

/// Check if a piece can reach a target via single orthogonal range move
fn can_reach_via_orthogonal_range<B: BoardLike>(
    piece: &Piece,
    from: Position,
    to: Position,
    board: &B,
) -> bool {
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
pub fn can_cannon_soldier_attack_target<B: BoardLike>(
    cannon_soldier: &Piece,
    target: Position,
    board: &B,
) -> bool {
    use crate::movement::direction::Direction;
    use crate::movement::types::MovementCapability;

    // Get movement config for Cannon Soldier
    let config = crate::movement::MovementConfig::for_piece(cannon_soldier);

    // Find the 7-move forward simple movement capability
    for capability in &config.capabilities {
        if let MovementCapability::Simple {
            directions,
            max_distance,
        } = capability
        {
            // Check if this is the forward direction (N for Black, S for White)
            // and has max_distance 7
            if *max_distance == 7 {
                // Adjust directions for color
                let adjusted_directions = crate::attack_utils::adjust_directions_for_color(
                    *directions,
                    cannon_soldier.color,
                );

                // Check if target is reachable in the forward direction
                let forward_direction = if cannon_soldier.color == Color::Black {
                    Direction::N
                } else {
                    Direction::S
                };

                // Check if the adjusted directions include forward
                if (adjusted_directions & forward_direction.to_bit()) != 0 {
                    // Check if target is on the forward line from cannon_soldier position
                    let file_diff = target.file() as i8 - cannon_soldier.position.file() as i8;
                    let rank_diff = target.rank() as i8 - cannon_soldier.position.rank() as i8;

                    // Must be on same file (orthogonal forward)
                    if file_diff == 0 {
                        // Check if rank_diff is in the correct direction and within range
                        let correct_direction = if cannon_soldier.color == Color::Black {
                            rank_diff > 0 && rank_diff <= 7
                        } else {
                            (-7..0).contains(&rank_diff)
                        };

                        if correct_direction {
                            // Check if path is clear (excluding destination)
                            if path_utils::is_orthogonal_path_clear_for_boardlike(
                                board,
                                cannon_soldier.position,
                                target,
                            ) {
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
pub fn can_lion_hawk_attack_target<B: BoardLike>(
    lion_hawk: &Piece,
    target: Position,
    board: &B,
) -> bool {
    if lion_hawk.position == target {
        return false;
    }
    // Can't capture friendly
    if let Some(target_piece) = BoardLike::get_piece(board, target) {
        if target_piece.color == lion_hawk.color {
            return false;
        }
    }

    let file_diff = (target.file() as i8 - lion_hawk.position.file() as i8).abs();
    let rank_diff = (target.rank() as i8 - lion_hawk.position.rank() as i8).abs();

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
fn can_reach_via_simple_1_any_direction<B: BoardLike>(
    piece: &Piece,
    from: Position,
    to: Position,
    board: &B,
) -> bool {
    let file_diff = to.file() as i8 - from.file() as i8;
    let rank_diff = to.rank() as i8 - from.rank() as i8;

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
    fn intermediate_order_matches_geometry_for_every_square_pair() {
        for from_index in 0..1296 {
            let from = Position::new((from_index % 36) as u8, (from_index / 36) as u8).unwrap();
            for to_index in 0..1296 {
                let to = Position::new((to_index % 36) as u8, (to_index / 36) as u8).unwrap();
                let (x, y, u, v) = (
                    from.file() as i16,
                    from.rank() as i16,
                    to.file() as i16,
                    to.rank() as i16,
                );
                let mut diagonal = Vec::new();
                for (a, b) in [
                    (x - y + u + v, u + v - x + y),
                    (x + y + u - v, x + y - u + v),
                ] {
                    if a % 2 == 0 && b % 2 == 0 && (0..72).contains(&a) && (0..72).contains(&b) {
                        let mid = Position::new((a / 2) as u8, (b / 2) as u8).unwrap();
                        if mid != from && mid != to && !diagonal.contains(&mid) {
                            diagonal.push(mid);
                        }
                    }
                }
                let orthogonal = if x != u && y != v {
                    vec![
                        Position::new(from.file(), to.rank()).unwrap(),
                        Position::new(to.file(), from.rank()).unwrap(),
                    ]
                } else {
                    Vec::new()
                };
                assert!(
                    find_potential_intermediates(from, to)
                        .into_iter()
                        .flatten()
                        .eq(diagonal),
                    "diagonal intermediates: {from:?} -> {to:?}"
                );
                assert!(
                    find_potential_intermediates_orthogonal(from, to)
                        .into_iter()
                        .flatten()
                        .eq(orthogonal),
                    "orthogonal intermediates: {from:?} -> {to:?}"
                );
            }
        }
    }

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
