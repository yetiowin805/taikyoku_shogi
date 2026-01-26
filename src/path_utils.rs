use crate::board::Board;
use crate::position::Position;

/// Calculate normalized direction steps from one position to another
/// Returns Some((file_step, rank_step)) if positions are aligned, None otherwise
/// Steps are normalized to -1, 0, or 1
fn calculate_direction_steps(from: Position, to: Position) -> Option<(i8, i8)> {
    let file_diff = to.file as i8 - from.file as i8;
    let rank_diff = to.rank as i8 - from.rank as i8;
    
    if file_diff == 0 && rank_diff == 0 {
        return None; // Same position
    }
    
    let file_step = if file_diff == 0 { 0 } else if file_diff > 0 { 1 } else { -1 };
    let rank_step = if rank_diff == 0 { 0 } else if rank_diff > 0 { 1 } else { -1 };
    
    Some((file_step, rank_step))
}

/// Get all positions along a path between two positions
/// Returns positions excluding start, including end
pub fn get_path_positions(from: Position, to: Position) -> Vec<Position> {
    let mut positions = Vec::new();
    
    let file_diff = to.file as i8 - from.file as i8;
    let rank_diff = to.rank as i8 - from.rank as i8;
    
    // Normalize direction
    let file_step = if file_diff == 0 { 0 } else if file_diff > 0 { 1 } else { -1 };
    let rank_step = if rank_diff == 0 { 0 } else if rank_diff > 0 { 1 } else { -1 };
    
    // Get all positions along the path (including destination)
    let mut current_file = from.file as i8 + file_step;
    let mut current_rank = from.rank as i8 + rank_step;
    
    loop {
        // Check if we've reached or passed the destination
        if file_step != 0 && (current_file - to.file as i8) * file_step > 0 {
            break;
        }
        if rank_step != 0 && (current_rank - to.rank as i8) * rank_step > 0 {
            break;
        }
        if file_step == 0 && rank_step == 0 {
            break;
        }
        
        if let Some(pos) = Position::new(current_file as u8, current_rank as u8) {
            positions.push(pos);
            if pos == to {
                break;
            }
        } else {
            break;
        }
        
        current_file += file_step;
        current_rank += rank_step;
    }
    
    positions
}

/// Check if a path between two positions is clear using BoardLike (excluding destination)
/// Works for any direction (orthogonal, diagonal, or combination)
pub fn is_path_clear_for_boardlike<B: crate::move_simulation::BoardLike>(
    board: &B,
    from: Position,
    to: Position,
) -> bool {
    let file_diff = to.file as i8 - from.file as i8;
    let rank_diff = to.rank as i8 - from.rank as i8;
    
    // Normalize direction
    let file_step = if file_diff == 0 { 0 } else if file_diff > 0 { 1 } else { -1 };
    let rank_step = if rank_diff == 0 { 0 } else if rank_diff > 0 { 1 } else { -1 };
    
    // Check each square along the path (excluding the destination)
    let mut current_file = from.file as i8 + file_step;
    let mut current_rank = from.rank as i8 + rank_step;
    
    while current_file != to.file as i8 || current_rank != to.rank as i8 {
        if let Some(pos) = Position::new(current_file as u8, current_rank as u8) {
            if !board.is_empty(pos) {
                // Path is blocked
                return false;
            }
        } else {
            // Out of bounds
            return false;
        }
        current_file += file_step;
        current_rank += rank_step;
    }
    
    true
}

/// Check if a diagonal path is clear using BoardLike (excluding destination)
/// Returns false if positions are not on the same diagonal
pub fn is_diagonal_path_clear_for_boardlike<B: crate::move_simulation::BoardLike>(
    board: &B,
    from: Position,
    to: Position,
) -> bool {
    let file_diff = to.file as i8 - from.file as i8;
    let rank_diff = to.rank as i8 - from.rank as i8;
    
    // Must be diagonal
    if file_diff.abs() != rank_diff.abs() {
        return false;
    }
    
    // Normalize direction (step size 1 in both file and rank)
    let file_step = if file_diff == 0 { 0 } else if file_diff > 0 { 1 } else { -1 };
    let rank_step = if rank_diff == 0 { 0 } else if rank_diff > 0 { 1 } else { -1 };
    
    // Check each square along the path (excluding the destination)
    let mut current_file = from.file as i8 + file_step;
    let mut current_rank = from.rank as i8 + rank_step;
    
    while current_file != to.file as i8 || current_rank != to.rank as i8 {
        if let Some(pos) = Position::new(current_file as u8, current_rank as u8) {
            if !board.is_empty(pos) {
                // Path is blocked
                return false;
            }
        } else {
            // Out of bounds
            return false;
        }
        current_file += file_step;
        current_rank += rank_step;
    }
    
    true
}

/// Check if an orthogonal path is clear using BoardLike (excluding destination)
/// Returns false if positions are not on the same file or rank
pub fn is_orthogonal_path_clear_for_boardlike<B: crate::move_simulation::BoardLike>(
    board: &B,
    from: Position,
    to: Position,
) -> bool {
    let file_diff = to.file as i8 - from.file as i8;
    let rank_diff = to.rank as i8 - from.rank as i8;
    
    // Must be orthogonal (either file_diff == 0 or rank_diff == 0, but not both)
    if file_diff != 0 && rank_diff != 0 {
        return false;
    }
    if file_diff == 0 && rank_diff == 0 {
        return false; // Same position
    }
    
    // Normalize direction (step size 1 in either file or rank)
    let file_step = if file_diff == 0 { 0 } else if file_diff > 0 { 1 } else { -1 };
    let rank_step = if rank_diff == 0 { 0 } else if rank_diff > 0 { 1 } else { -1 };
    
    // Check each square along the path (excluding the destination)
    let mut current_file = from.file as i8 + file_step;
    let mut current_rank = from.rank as i8 + rank_step;
    
    while current_file != to.file as i8 || current_rank != to.rank as i8 {
        if let Some(pos) = Position::new(current_file as u8, current_rank as u8) {
            if !board.is_empty(pos) {
                // Path is blocked
                return false;
            }
        } else {
            // Out of bounds
            return false;
        }
        current_file += file_step;
        current_rank += rank_step;
    }
    
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::piece::{Piece, PieceType, Color};

    #[test]
    fn test_get_path_positions_horizontal() {
        let from = Position::new(5, 10).unwrap();
        let to = Position::new(8, 10).unwrap();
        let path = get_path_positions(from, to);
        
        assert_eq!(path.len(), 3);
        assert_eq!(path[0], Position::new(6, 10).unwrap());
        assert_eq!(path[1], Position::new(7, 10).unwrap());
        assert_eq!(path[2], Position::new(8, 10).unwrap());
    }

    #[test]
    fn test_get_path_positions_vertical() {
        let from = Position::new(10, 5).unwrap();
        let to = Position::new(10, 8).unwrap();
        let path = get_path_positions(from, to);
        
        assert_eq!(path.len(), 3);
        assert_eq!(path[0], Position::new(10, 6).unwrap());
        assert_eq!(path[1], Position::new(10, 7).unwrap());
        assert_eq!(path[2], Position::new(10, 8).unwrap());
    }

    #[test]
    fn test_get_path_positions_diagonal() {
        let from = Position::new(5, 5).unwrap();
        let to = Position::new(8, 8).unwrap();
        let path = get_path_positions(from, to);
        
        assert_eq!(path.len(), 3);
        assert_eq!(path[0], Position::new(6, 6).unwrap());
        assert_eq!(path[1], Position::new(7, 7).unwrap());
        assert_eq!(path[2], Position::new(8, 8).unwrap());
    }

    #[test]
    fn test_get_path_positions_same_position() {
        let pos = Position::new(10, 10).unwrap();
        let path = get_path_positions(pos, pos);
        assert_eq!(path.len(), 0); // No positions between same position
    }

    #[test]
    fn test_get_path_positions_adjacent() {
        let from = Position::new(10, 10).unwrap();
        let to = Position::new(11, 10).unwrap();
        let path = get_path_positions(from, to);
        
        assert_eq!(path.len(), 1);
        assert_eq!(path[0], to);
    }

    #[test]
    fn test_is_path_clear_empty() {
        let board = Board::new();
        let from = Position::new(5, 10).unwrap();
        let to = Position::new(8, 10).unwrap();
        
        assert!(is_path_clear_for_boardlike(&board, from, to));
    }

    #[test]
    fn test_is_path_clear_blocked() {
        let mut board = Board::new();
        let from = Position::new(5, 10).unwrap();
        let blocker = Position::new(7, 10).unwrap();
        let to = Position::new(8, 10).unwrap();
        
        let piece = Piece::new(PieceType::Pawn, Color::Black, blocker);
        board.place_piece(piece);
        
        assert!(!is_path_clear_for_boardlike(&board, from, to));
    }

    #[test]
    fn test_is_diagonal_path_clear_valid() {
        let board = Board::new();
        let from = Position::new(5, 5).unwrap();
        let to = Position::new(8, 8).unwrap();
        
        assert!(is_diagonal_path_clear_for_boardlike(&board, from, to));
    }

    #[test]
    fn test_is_diagonal_path_clear_not_diagonal() {
        let board = Board::new();
        let from = Position::new(5, 5).unwrap();
        let to = Position::new(8, 6).unwrap(); // Not diagonal
        
        assert!(!is_diagonal_path_clear_for_boardlike(&board, from, to));
    }

    #[test]
    fn test_is_orthogonal_path_clear_valid() {
        let board = Board::new();
        let from = Position::new(5, 10).unwrap();
        let to = Position::new(8, 10).unwrap();
        
        assert!(is_orthogonal_path_clear_for_boardlike(&board, from, to));
    }

    #[test]
    fn test_is_orthogonal_path_clear_not_orthogonal() {
        let board = Board::new();
        let from = Position::new(5, 5).unwrap();
        let to = Position::new(8, 8).unwrap(); // Diagonal, not orthogonal
        
        assert!(!is_orthogonal_path_clear_for_boardlike(&board, from, to));
    }

    #[test]
    fn test_is_orthogonal_path_clear_same_position() {
        let board = Board::new();
        let pos = Position::new(10, 10).unwrap();
        
        assert!(!is_orthogonal_path_clear_for_boardlike(&board, pos, pos));
    }
}

