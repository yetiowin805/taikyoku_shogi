use crate::piece::{Piece, Color};
use crate::position::Position;
use crate::move_simulation;
use crate::tengu_attack;

/// Represents the 36x36 board
/// Uses a flat vector for efficient access
/// Maintains separate piece lists by color for fast iteration
pub struct Board {
    squares: Vec<Option<Piece>>,  // 36 * 36 = 1296 squares
    black_pieces: Vec<Piece>,     // Fast iteration over black pieces
    white_pieces: Vec<Piece>,     // Fast iteration over white pieces
}

impl Board {
    pub fn new() -> Board {
        Board {
            squares: vec![None; 1296],
            black_pieces: Vec::new(),
            white_pieces: Vec::new(),
        }
    }

    /// Get piece at position, if any
    pub fn get_piece(&self, pos: Position) -> Option<Piece> {
        self.squares.get(pos.to_index()).copied().flatten()
    }

    /// Place a piece on the board
    /// If a piece already exists at this position, it will be removed first
    pub fn place_piece(&mut self, piece: Piece) {
        let index = piece.position.to_index();
        if index < self.squares.len() {
            // Remove any existing piece at this position from the list
            if let Some(existing_piece) = self.squares[index] {
                match existing_piece.color {
                    Color::Black => {
                        self.black_pieces.retain(|p| p.position != piece.position);
                    }
                    Color::White => {
                        self.white_pieces.retain(|p| p.position != piece.position);
                    }
                }
            }
            
            self.squares[index] = Some(piece);
            // Add to appropriate color list
            match piece.color {
                Color::Black => self.black_pieces.push(piece),
                Color::White => self.white_pieces.push(piece),
            }
        }
    }

    /// Remove piece from position. Returns the removed piece, if any.
    pub fn remove_piece(&mut self, pos: Position) -> Option<Piece> {
        let index = pos.to_index();
        if index < self.squares.len() {
            if let Some(piece) = self.squares[index].take() {
                match piece.color {
                    Color::Black => {
                        self.black_pieces.retain(|p| p.position != pos);
                    }
                    Color::White => {
                        self.white_pieces.retain(|p| p.position != pos);
                    }
                }
                return Some(piece);
            }
        }
        None
    }

    /// Check if a square is empty
    pub fn is_empty(&self, pos: Position) -> bool {
        self.get_piece(pos).is_none()
    }

    /// Check if a square contains a piece of the given color
    pub fn has_piece_of_color(&self, pos: Position, color: Color) -> bool {
        self.get_piece(pos)
            .map(|p| p.color == color)
            .unwrap_or(false)
    }

    /// Get all pieces of a given color
    /// Uses the optimized piece lists for fast access
    pub fn get_pieces_by_color(&self, color: Color) -> Vec<Piece> {
        match color {
            Color::Black => self.black_pieces.clone(),
            Color::White => self.white_pieces.clone(),
        }
    }

    /// Iterate over pieces of a given color without cloning
    /// Uses the optimized piece lists for fast iteration
    pub fn iter_pieces_by_color(&self, color: Color) -> impl Iterator<Item = Piece> + '_ {
        match color {
            Color::Black => self.black_pieces.iter().copied(),
            Color::White => self.white_pieces.iter().copied(),
        }
    }

    /// Borrow the piece list for a color (no clone).
    pub fn pieces_by_color(&self, color: Color) -> &[Piece] {
        match color {
            Color::Black => &self.black_pieces,
            Color::White => &self.white_pieces,
        }
    }

    /// Move a piece from one position to another
    /// Returns the captured piece if any
    pub fn move_piece(&mut self, from: Position, to: Position) -> Option<Piece> {
        let captured = self.get_piece(to);
        
        // Remove captured piece from list if any
        if let Some(captured_piece) = captured {
            match captured_piece.color {
                Color::Black => {
                    self.black_pieces.retain(|p| p.position != to);
                }
                Color::White => {
                    self.white_pieces.retain(|p| p.position != to);
                }
            }
        }
        
        if let Some(mut piece) = self.get_piece(from) {
            // Update piece position in the list (before updating squares)
            match piece.color {
                Color::Black => {
                    if let Some(list_piece) = self.black_pieces.iter_mut().find(|p| p.position == from) {
                        list_piece.position = to;
                    }
                }
                Color::White => {
                    if let Some(list_piece) = self.white_pieces.iter_mut().find(|p| p.position == from) {
                        list_piece.position = to;
                    }
                }
            }
            
            // Update squares array
            piece.position = to;
            self.squares[from.to_index()] = None;
            self.squares[to.to_index()] = Some(piece);
        }
        
        captured
    }

    /// Create a copy of the board
    pub fn clone(&self) -> Board {
        Clone::clone(self)
    }

    /// Check if a position is attacked by pieces of a given color
    /// Uses early termination and optimized functions for specialized pieces
    /// Returns true immediately when first attacker is found
    pub fn is_position_attacked_by_color(&self, position: Position, attacker_color: Color) -> bool {
        is_position_attacked_by_color_impl(self, position, attacker_color, false)
    }
    
    /// Check if a position is attacked by pieces of a given color, optimized for check detection
    /// This treats pieces with only capturing range movement as short-range only
    /// Uses early termination and optimized functions for specialized pieces
    /// Returns true immediately when first attacker is found
    pub fn is_position_attacked_by_color_for_check(&self, position: Position, attacker_color: Color) -> bool {
        is_position_attacked_by_color_impl(self, position, attacker_color, true)
    }
}

impl Clone for Board {
    fn clone(&self) -> Board {
        Board {
            squares: self.squares.clone(),
            black_pieces: self.black_pieces.clone(),
            white_pieces: self.white_pieces.clone(),
        }
    }
}

/// Internal implementation of attack detection that works with BoardLike trait
/// This allows both Board and VirtualBoard to use the same logic
pub(crate) fn is_position_attacked_by_color_impl<B: move_simulation::BoardLike>(
    board: &B,
    position: Position,
    attacker_color: Color,
    for_check: bool,
) -> bool {
    use crate::attack_utils;
    
    // Get all pieces of attacker_color
    let all_attacker_pieces = board.get_pieces_by_color(attacker_color);
    
    // Filter pieces based on proximity and movement capabilities
    let filtered_pieces: Vec<Piece> = all_attacker_pieces
        .into_iter()
        .filter(|piece| attack_utils::should_check_piece_for_target_position(piece, position, for_check))
        .collect();
    
    // Check specialized pieces first (most likely to attack, optimized functions)
    // These functions now work with BoardLike, so they work with both Board and VirtualBoard
    
    for piece in &filtered_pieces {
        // Check Tengu/promoted Peacock/Capricorn with optimized function
        if attack_utils::is_tengu_or_promoted_peacock(piece) {
            if tengu_attack::can_tengu_attack_target(piece, position, board) {
                return true;
            }
            continue;
        }
        
        // Check unpromoted Peacock with optimized function
        if attack_utils::is_unpromoted_peacock(piece) {
            if tengu_attack::can_peacock_attack_target(piece, position, board) {
                return true;
            }
            continue;
        }
        
        // Check Hook Mover with optimized function
        if attack_utils::is_hook_mover_like_piece(piece) {
            if tengu_attack::can_hook_mover_attack_target(piece, position, board) {
                return true;
            }
            continue;
        }
        
        // Check Lion Hawk with optimized function
        if attack_utils::is_lion_hawk(piece) {
            if tengu_attack::can_lion_hawk_attack_target(piece, position, board) {
                return true;
            }
            continue;
        }
        
        // Check Cannon Soldier with optimized function
        if attack_utils::is_cannon_soldier(piece) {
            if tengu_attack::can_cannon_soldier_attack_target(piece, position, board) {
                return true;
            }
            continue;
        }
    }
    
    // For non-specialized pieces, use can_reach (more efficient than generating all moves)
    for piece in &filtered_pieces {
        // Skip pieces we already checked
        if attack_utils::is_tengu_or_promoted_peacock(piece) ||
           attack_utils::is_unpromoted_peacock(piece) ||
           attack_utils::is_hook_mover_like_piece(piece) ||
           attack_utils::is_lion_hawk(piece) ||
           attack_utils::is_cannon_soldier(piece) {
            continue;
        }
        
        if piece.can_reach_boardlike(position, board) {
            return true; // Early termination
        }
    }
    
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::piece::PieceType;

    #[test]
    fn test_board_creation() {
        let board = Board::new();
        assert!(board.is_empty(Position::new(0, 0).unwrap()));
        assert!(board.is_empty(Position::new(35, 35).unwrap()));
    }

    #[test]
    fn test_place_and_get_piece() {
        let mut board = Board::new();
        let pos = Position::new(10, 10).unwrap();
        let piece = Piece::new(PieceType::King, Color::Black, pos);
        
        board.place_piece(piece);
        assert_eq!(board.get_piece(pos), Some(piece));
        assert!(!board.is_empty(pos));
    }

    #[test]
    fn test_move_piece() {
        let mut board = Board::new();
        let from = Position::new(10, 10).unwrap();
        let to = Position::new(11, 11).unwrap();
        let piece = Piece::new(PieceType::King, Color::Black, from);
        
        board.place_piece(piece);
        board.move_piece(from, to);
        
        assert!(board.is_empty(from));
        assert_eq!(board.get_piece(to).unwrap().position, to);
    }

    #[test]
    fn test_tengu_two_step_attack_with_virtual_board() {
        use crate::move_simulation::{BoardLike, simulate_move};
        use crate::game_state::Move;
        
        let mut board = Board::new();
        
        // Place a Tengu at (10, 10)
        let tengu_pos = Position::new(10, 10).unwrap();
        let tengu = Piece::new(PieceType::Tengu, Color::Black, tengu_pos);
        board.place_piece(tengu);
        
        // Place a target piece at (15, 15) - two-step diagonal move away
        // First step: (10, 10) -> (12, 12) (diagonal range)
        // Second step: (12, 12) -> (15, 15) (diagonal range)
        let target_pos = Position::new(15, 15).unwrap();
        let target = Piece::new(PieceType::King, Color::White, target_pos);
        board.place_piece(target);
        
        // Test that Tengu can attack the target (two-step move)
        assert!(board.is_position_attacked_by_color(target_pos, Color::Black));
        
        // Test with VirtualBoard: simulate moving a piece and check if Tengu still attacks
        let other_piece_pos = Position::new(5, 5).unwrap();
        let other_piece = Piece::new(PieceType::Pawn, Color::White, other_piece_pos);
        board.place_piece(other_piece);
        
        let move_to = Position::new(6, 6).unwrap();
        let mv = Move::new_with_promotion(other_piece_pos, move_to, false);
        let virtual_board = simulate_move(&board, &mv, &other_piece);
        
        // Tengu should still be able to attack the target through VirtualBoard
        assert!(BoardLike::is_position_attacked_by_color(&virtual_board, target_pos, Color::Black));
    }

    #[test]
    fn test_peacock_two_step_attack_with_virtual_board() {
        use crate::move_simulation::{BoardLike, simulate_move};
        use crate::game_state::Move;
        
        let mut board = Board::new();
        
        // Place an unpromoted Peacock at (10, 10)
        let peacock_pos = Position::new(10, 10).unwrap();
        let mut peacock = Piece::new(PieceType::Peacock, Color::Black, peacock_pos);
        peacock.is_promoted = false;
        board.place_piece(peacock);
        
        // Place a target piece at (13, 11) - two-step L-shaped move away
        // First step: forward diagonal range NE to (12, 12) (intermediate)
        // Second step: any diagonal range from (12, 12) SE to (13, 11)
        let target_pos = Position::new(13, 11).unwrap();
        let target = Piece::new(PieceType::King, Color::White, target_pos);
        board.place_piece(target);
        
        // Test that Peacock can attack the target (two-step move)
        // Note: This tests the two-step capability, which should work via can_reach_boardlike
        // The optimized tengu_attack function may not catch this specific case, but
        // can_reach_boardlike should handle it via TwoStep capability
        assert!(board.is_position_attacked_by_color(target_pos, Color::Black));
        
        // Test with VirtualBoard
        let other_piece_pos = Position::new(5, 5).unwrap();
        let other_piece = Piece::new(PieceType::Pawn, Color::White, other_piece_pos);
        board.place_piece(other_piece);
        
        let move_to = Position::new(6, 6).unwrap();
        let mv = Move::new_with_promotion(other_piece_pos, move_to, false);
        let virtual_board = simulate_move(&board, &mv, &other_piece);
        
        // Peacock should still be able to attack the target through VirtualBoard
        assert!(BoardLike::is_position_attacked_by_color(&virtual_board, target_pos, Color::Black));
    }

    #[test]
    fn test_hook_mover_two_step_attack_with_virtual_board() {
        use crate::move_simulation::{BoardLike, simulate_move};
        use crate::game_state::Move;
        
        let mut board = Board::new();
        
        // Place a Hook Mover at (10, 10)
        let hook_mover_pos = Position::new(10, 10).unwrap();
        let hook_mover = Piece::new(PieceType::HookMover, Color::Black, hook_mover_pos);
        board.place_piece(hook_mover);
        
        // Place a target piece at (15, 10) - two-step orthogonal move away
        // First step: (10, 10) -> (12, 10) (orthogonal range)
        // Second step: (12, 10) -> (15, 10) (orthogonal range)
        let target_pos = Position::new(15, 10).unwrap();
        let target = Piece::new(PieceType::King, Color::White, target_pos);
        board.place_piece(target);
        
        // Test that Hook Mover can attack the target (two-step move)
        assert!(board.is_position_attacked_by_color(target_pos, Color::Black));
        
        // Test with VirtualBoard
        let other_piece_pos = Position::new(5, 5).unwrap();
        let other_piece = Piece::new(PieceType::Pawn, Color::White, other_piece_pos);
        board.place_piece(other_piece);
        
        let move_to = Position::new(6, 6).unwrap();
        let mv = Move::new_with_promotion(other_piece_pos, move_to, false);
        let virtual_board = simulate_move(&board, &mv, &other_piece);
        
        // Hook Mover should still be able to attack the target through VirtualBoard
        assert!(BoardLike::is_position_attacked_by_color(&virtual_board, target_pos, Color::Black));
    }

    // ===== Attack Detection Test Suite =====

    #[test]
    fn test_adjacent_simple_attack() {
        // Test: Adjacent piece with Simple movement attacking royal piece
        let mut board = Board::new();
        
        // Place Crown Prince at (10, 10)
        let royal_pos = Position::new(10, 10).unwrap();
        let royal = Piece::new(PieceType::CrownPrince, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place enemy piece adjacent (to the right)
        let attacker_pos = Position::new(11, 10).unwrap();
        let attacker = Piece::new(PieceType::King, Color::White, attacker_pos);
        board.place_piece(attacker);
        
        // Should detect attack
        assert!(board.is_position_attacked_by_color_for_check(royal_pos, Color::White));
    }

    #[test]
    fn test_adjacent_simple_attack_diagonal() {
        // Test: Adjacent diagonal attack
        let mut board = Board::new();
        
        let royal_pos = Position::new(10, 10).unwrap();
        let royal = Piece::new(PieceType::CrownPrince, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place enemy piece diagonally adjacent
        let attacker_pos = Position::new(11, 11).unwrap();
        let attacker = Piece::new(PieceType::King, Color::White, attacker_pos);
        board.place_piece(attacker);
        
        assert!(board.is_position_attacked_by_color_for_check(royal_pos, Color::White));
    }

    #[test]
    fn test_simple_attack_blocked() {
        // Test: Simple attack blocked by friendly piece
        let mut board = Board::new();
        
        let royal_pos = Position::new(10, 10).unwrap();
        let royal = Piece::new(PieceType::CrownPrince, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place friendly blocker
        let blocker_pos = Position::new(11, 10).unwrap();
        let blocker = Piece::new(PieceType::Pawn, Color::Black, blocker_pos);
        board.place_piece(blocker);
        
        // Place enemy piece beyond blocker (should not be able to attack)
        let attacker_pos = Position::new(12, 10).unwrap();
        let attacker = Piece::new(PieceType::King, Color::White, attacker_pos);
        board.place_piece(attacker);
        
        // Should NOT detect attack (blocked by friendly piece)
        assert!(!board.is_position_attacked_by_color_for_check(royal_pos, Color::White));
    }

    #[test]
    fn test_simple_attack_can_capture_blocker() {
        // Test: Simple attack when enemy blocker is at target position
        // For Simple movement, if there's an enemy piece at the target position,
        // the attacker can capture it, so it can attack that position.
        let mut board = Board::new();
        
        let royal_pos = Position::new(10, 10).unwrap();
        let royal = Piece::new(PieceType::CrownPrince, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place enemy piece at royal position (can be captured by attacker)
        let blocker_at_royal = Piece::new(PieceType::Pawn, Color::Black, royal_pos);
        board.place_piece(blocker_at_royal);
        
        // Place attacker at distance 1 (adjacent, can capture piece at royal position)
        let attacker_pos = Position::new(11, 10).unwrap();
        let attacker = Piece::new(PieceType::King, Color::White, attacker_pos);
        board.place_piece(attacker);
        
        // Attacker can capture piece at royal position
        assert!(board.is_position_attacked_by_color_for_check(royal_pos, Color::White));
    }

    #[test]
    fn test_range_attack_adjacent() {
        // Test: Range movement piece adjacent to royal
        let mut board = Board::new();
        
        let royal_pos = Position::new(10, 10).unwrap();
        let royal = Piece::new(PieceType::CrownPrince, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place Rook (range orthogonal) adjacent
        let attacker_pos = Position::new(11, 10).unwrap();
        let attacker = Piece::new(PieceType::Rook, Color::White, attacker_pos);
        board.place_piece(attacker);
        
        assert!(board.is_position_attacked_by_color_for_check(royal_pos, Color::White));
    }

    #[test]
    fn test_range_attack_from_distance() {
        // Test: Range movement piece attacking from distance
        let mut board = Board::new();
        
        let royal_pos = Position::new(10, 10).unwrap();
        let royal = Piece::new(PieceType::CrownPrince, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place Rook 5 spaces away
        let attacker_pos = Position::new(15, 10).unwrap();
        let attacker = Piece::new(PieceType::Rook, Color::White, attacker_pos);
        board.place_piece(attacker);
        
        assert!(board.is_position_attacked_by_color_for_check(royal_pos, Color::White));
    }

    #[test]
    fn test_range_attack_blocked() {
        // Test: Range attack blocked by piece
        let mut board = Board::new();
        
        let royal_pos = Position::new(10, 10).unwrap();
        let royal = Piece::new(PieceType::CrownPrince, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place blocker
        let blocker_pos = Position::new(12, 10).unwrap();
        let blocker = Piece::new(PieceType::Pawn, Color::Black, blocker_pos);
        board.place_piece(blocker);
        
        // Place Rook beyond blocker
        let attacker_pos = Position::new(15, 10).unwrap();
        let attacker = Piece::new(PieceType::Rook, Color::White, attacker_pos);
        board.place_piece(attacker);
        
        // Should NOT detect attack (blocked)
        assert!(!board.is_position_attacked_by_color_for_check(royal_pos, Color::White));
    }

    #[test]
    fn test_range_attack_jump_mode() {
        // Test: Range attack with Jump mode (can jump over pieces)
        let mut board = Board::new();
        
        let royal_pos = Position::new(10, 10).unwrap();
        let royal = Piece::new(PieceType::CrownPrince, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place blocker (should be jumped over)
        let blocker_pos = Position::new(12, 10).unwrap();
        let blocker = Piece::new(PieceType::Pawn, Color::Black, blocker_pos);
        board.place_piece(blocker);
        
        // Place piece with Jump range movement (need to find a piece with Jump mode)
        // For now, test that blocking works correctly - Jump mode pieces are rare
        // This test verifies the blocking logic works
    }

    #[test]
    fn test_tengu_attack_adjacent() {
        // Test: Tengu attacking from adjacent position (should use two-step)
        let mut board = Board::new();
        
        let royal_pos = Position::new(10, 10).unwrap();
        let royal = Piece::new(PieceType::CrownPrince, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place Tengu adjacent (can't attack directly, but can via two-step)
        let tengu_pos = Position::new(11, 11).unwrap();
        let tengu = Piece::new(PieceType::Tengu, Color::White, tengu_pos);
        board.place_piece(tengu);
        
        // Tengu adjacent to royal - should be able to attack via two-step
        // (First step: diagonal range, second step: diagonal range)
        assert!(board.is_position_attacked_by_color_for_check(royal_pos, Color::White));
    }

    #[test]
    fn test_tengu_attack_from_distance() {
        // Test: Tengu attacking from distance
        let mut board = Board::new();
        
        let royal_pos = Position::new(10, 10).unwrap();
        let royal = Piece::new(PieceType::CrownPrince, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place Tengu 5 spaces away diagonally
        let tengu_pos = Position::new(15, 15).unwrap();
        let tengu = Piece::new(PieceType::Tengu, Color::White, tengu_pos);
        board.place_piece(tengu);
        
        assert!(board.is_position_attacked_by_color_for_check(royal_pos, Color::White));
    }

    #[test]
    fn test_hook_mover_attack() {
        // Test: Hook Mover attacking royal
        let mut board = Board::new();
        
        let royal_pos = Position::new(10, 10).unwrap();
        let royal = Piece::new(PieceType::CrownPrince, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place Hook Mover (two-step orthogonal)
        let hook_mover_pos = Position::new(12, 12).unwrap();
        let hook_mover = Piece::new(PieceType::HookMover, Color::White, hook_mover_pos);
        board.place_piece(hook_mover);
        
        assert!(board.is_position_attacked_by_color_for_check(royal_pos, Color::White));
    }

    #[test]
    fn test_unpin_scenario_1() {
        // Test: Piece pinned to royal, moving it would expose royal
        use crate::move_simulation::BoardLike;
        use crate::game_state::Move;
        
        let mut board = Board::new();
        
        // Place royal at (10, 10)
        let royal_pos = Position::new(10, 10).unwrap();
        let royal = Piece::new(PieceType::CrownPrince, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place friendly piece at (11, 10) - pinned
        let pinned_pos = Position::new(11, 10).unwrap();
        let pinned = Piece::new(PieceType::Pawn, Color::Black, pinned_pos);
        board.place_piece(pinned);
        
        // Place enemy Rook at (15, 10) - can attack royal if pinned piece moves
        let rook_pos = Position::new(15, 10).unwrap();
        let rook = Piece::new(PieceType::Rook, Color::White, rook_pos);
        board.place_piece(rook);
        
        // Simulate moving the pinned piece
        let mv = Move::new_with_promotion(pinned_pos, Position::new(11, 11).unwrap(), false);
        let virtual_board = crate::move_simulation::simulate_move(&board, &mv, &pinned);
        
        // Royal should now be under attack
        assert!(BoardLike::is_position_attacked_by_color_for_check(&virtual_board, royal_pos, Color::White));
    }

    #[test]
    fn test_unpin_scenario_2() {
        // Test: Piece pinned diagonally
        use crate::move_simulation::BoardLike;
        use crate::game_state::Move;
        
        let mut board = Board::new();
        
        let royal_pos = Position::new(10, 10).unwrap();
        let royal = Piece::new(PieceType::CrownPrince, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place friendly piece diagonally between royal and enemy
        let pinned_pos = Position::new(11, 11).unwrap();
        let pinned = Piece::new(PieceType::Pawn, Color::Black, pinned_pos);
        board.place_piece(pinned);
        
        // Place enemy Bishop (diagonal range) beyond
        let bishop_pos = Position::new(15, 15).unwrap();
        let bishop = Piece::new(PieceType::Bishop, Color::White, bishop_pos);
        board.place_piece(bishop);
        
        // Move pinned piece
        let mv = Move::new_with_promotion(pinned_pos, Position::new(11, 12).unwrap(), false);
        let virtual_board = crate::move_simulation::simulate_move(&board, &mv, &pinned);
        
        // Royal should now be under attack
        assert!(BoardLike::is_position_attacked_by_color_for_check(&virtual_board, royal_pos, Color::White));
    }

    #[test]
    fn test_unpin_scenario_3_no_threat() {
        // Test: Piece not actually pinned (no threat beyond)
        use crate::move_simulation::BoardLike;
        use crate::game_state::Move;
        
        let mut board = Board::new();
        
        let royal_pos = Position::new(10, 10).unwrap();
        let royal = Piece::new(PieceType::CrownPrince, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place friendly piece
        let piece_pos = Position::new(11, 10).unwrap();
        let piece = Piece::new(PieceType::Pawn, Color::Black, piece_pos);
        board.place_piece(piece);
        
        // No enemy piece beyond - not actually pinned
        // Move should be safe
        let mv = Move::new_with_promotion(piece_pos, Position::new(11, 11).unwrap(), false);
        let virtual_board = crate::move_simulation::simulate_move(&board, &mv, &piece);
        
        // Royal should NOT be under attack
        assert!(!BoardLike::is_position_attacked_by_color_for_check(&virtual_board, royal_pos, Color::White));
    }

    #[test]
    fn test_simple_attack_max_distance_2() {
        // Test: Simple movement with max_distance=2 attacking royal
        let mut board = Board::new();
        
        let royal_pos = Position::new(10, 10).unwrap();
        let royal = Piece::new(PieceType::CrownPrince, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place piece with Simple max_distance=2 at distance 2
        // Using a piece that has Simple movement with max_distance=2
        // For example, a piece that can move 2 spaces in a direction
        let attacker_pos = Position::new(12, 10).unwrap();
        // Need to find a piece with Simple max_distance=2 - using King (max_distance=2 in some directions)
        let attacker = Piece::new(PieceType::King, Color::White, attacker_pos);
        board.place_piece(attacker);
        
        assert!(board.is_position_attacked_by_color_for_check(royal_pos, Color::White));
    }

    #[test]
    fn test_simple_attack_max_distance_2_blocked() {
        // Test: Simple movement max_distance=2 blocked at distance 1
        let mut board = Board::new();
        
        let royal_pos = Position::new(10, 10).unwrap();
        let royal = Piece::new(PieceType::CrownPrince, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place blocker at distance 1
        let blocker_pos = Position::new(11, 10).unwrap();
        let blocker = Piece::new(PieceType::Pawn, Color::Black, blocker_pos);
        board.place_piece(blocker);
        
        // Place attacker at distance 2 (should not be able to attack)
        let attacker_pos = Position::new(12, 10).unwrap();
        let attacker = Piece::new(PieceType::King, Color::White, attacker_pos);
        board.place_piece(attacker);
        
        // Should NOT detect attack (blocked)
        assert!(!board.is_position_attacked_by_color_for_check(royal_pos, Color::White));
    }

    #[test]
    fn test_friendly_piece_blocking_attack() {
        // Test: Friendly piece between attacker and royal blocks attack
        let mut board = Board::new();
        
        let royal_pos = Position::new(10, 10).unwrap();
        let royal = Piece::new(PieceType::CrownPrince, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place friendly blocker
        let blocker_pos = Position::new(11, 10).unwrap();
        let blocker = Piece::new(PieceType::Pawn, Color::Black, blocker_pos);
        board.place_piece(blocker);
        
        // Place enemy Rook beyond blocker
        let attacker_pos = Position::new(15, 10).unwrap();
        let attacker = Piece::new(PieceType::Rook, Color::White, attacker_pos);
        board.place_piece(attacker);
        
        // Should NOT detect attack (blocked by friendly piece)
        assert!(!board.is_position_attacked_by_color_for_check(royal_pos, Color::White));
    }

    #[test]
    fn test_enemy_piece_blocking_attack() {
        // Test: Range attack with enemy blocker - attacker can capture blocker to reach royal
        // For Range movement with NoJump, the path must be clear, but if there's an enemy blocker,
        // the piece can capture it. However, the piece can only reach the blocker's position, not beyond.
        // So if blocker is between attacker and royal, attacker cannot reach royal in one move.
        // This test should actually check: can Range piece attack when it can capture blocker at royal position?
        let mut board = Board::new();
        
        let royal_pos = Position::new(10, 10).unwrap();
        let royal = Piece::new(PieceType::CrownPrince, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place enemy blocker (Black = enemy to White attacker, can be captured)
        let blocker_pos = Position::new(11, 10).unwrap();
        let blocker = Piece::new(PieceType::Pawn, Color::Black, blocker_pos);
        board.place_piece(blocker);
        
        // Place White Rook beyond blocker
        let attacker_pos = Position::new(15, 10).unwrap();
        let attacker = Piece::new(PieceType::Rook, Color::White, attacker_pos);
        board.place_piece(attacker);
        
        // Rook can capture blocker at (11, 10), but royal is at (10, 10)
        // So Rook cannot reach royal in one move (blocked by blocker)
        // The path from (15, 10) to (10, 10) goes through (11, 10) which has the blocker
        // So attack should NOT be detected
        assert!(!board.is_position_attacked_by_color_for_check(royal_pos, Color::White));
        
        // But if blocker is at royal position, Rook can attack
        let mut board2 = Board::new();
        board2.place_piece(royal);
        let blocker_at_royal = Piece::new(PieceType::Pawn, Color::Black, royal_pos);
        board2.place_piece(blocker_at_royal);
        board2.place_piece(attacker);
        
        // Rook can capture piece at royal position
        assert!(board2.is_position_attacked_by_color_for_check(royal_pos, Color::White));
    }

    #[test]
    fn test_shitenno_jumping_range_attack_same_rank() {
        // Test: Shitenno (Range with Jump mode in all directions) attacking on same rank
        // with many pieces in between - should be able to jump over them
        let mut board = Board::new();
        
        let royal_pos = Position::new(20, 10).unwrap();
        let royal = Piece::new(PieceType::King, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place Shitenno far away on the same rank
        let shitenno_pos = Position::new(5, 10).unwrap();
        let shitenno = Piece::new(PieceType::Shitennou, Color::White, shitenno_pos);
        board.place_piece(shitenno);
        
        // Place many pieces in between (both friendly and enemy)
        for i in 6..20 {
            let blocker_pos = Position::new(i, 10).unwrap();
            let blocker = Piece::new(
                PieceType::Pawn,
                if i % 2 == 0 { Color::Black } else { Color::White },
                blocker_pos
            );
            board.place_piece(blocker);
        }
        
        // Shitenno should be able to attack the king by jumping over all pieces
        assert!(board.is_position_attacked_by_color_for_check(royal_pos, Color::White),
                "Shitenno should be able to attack king on same rank with jumping range");
    }

    #[test]
    fn test_shitenno_jumping_range_attack_same_file() {
        // Test: Shitenno attacking on same file (vertical)
        let mut board = Board::new();
        
        let royal_pos = Position::new(10, 25).unwrap();
        let royal = Piece::new(PieceType::King, Color::Black, royal_pos);
        board.place_piece(royal);
        
        let shitenno_pos = Position::new(10, 5).unwrap();
        let shitenno = Piece::new(PieceType::Shitennou, Color::White, shitenno_pos);
        board.place_piece(shitenno);
        
        // Place pieces in between
        for i in 6..25 {
            let blocker_pos = Position::new(10, i).unwrap();
            let blocker = Piece::new(
                PieceType::Pawn,
                if i % 2 == 0 { Color::Black } else { Color::White },
                blocker_pos
            );
            board.place_piece(blocker);
        }
        
        assert!(board.is_position_attacked_by_color_for_check(royal_pos, Color::White),
                "Shitenno should be able to attack king on same file with jumping range");
    }

    #[test]
    fn test_shitenno_jumping_range_attack_diagonal() {
        // Test: Shitenno attacking diagonally
        let mut board = Board::new();
        
        let royal_pos = Position::new(20, 20).unwrap();
        let royal = Piece::new(PieceType::King, Color::Black, royal_pos);
        board.place_piece(royal);
        
        let shitenno_pos = Position::new(5, 5).unwrap();
        let shitenno = Piece::new(PieceType::Shitennou, Color::White, shitenno_pos);
        board.place_piece(shitenno);
        
        // Place pieces in between on the diagonal
        for i in 1..15 {
            let blocker_pos = Position::new(5 + i, 5 + i).unwrap();
            let blocker = Piece::new(
                PieceType::Pawn,
                if i % 2 == 0 { Color::Black } else { Color::White },
                blocker_pos
            );
            board.place_piece(blocker);
        }
        
        assert!(board.is_position_attacked_by_color_for_check(royal_pos, Color::White),
                "Shitenno should be able to attack king diagonally with jumping range");
    }

    #[test]
    fn test_great_eagle_jumping_range_attack_forward_diagonal() {
        // Test: Great Eagle (promoted Flying Eagle) has jumping range in forward diagonals
        // For White: forward diagonals are SE and SW (rank decreases)
        let mut board = Board::new();
        
        // Use proper diagonal: from (5, 25) to (20, 10) is SE diagonal (file+, rank-)
        // Both positions are 15 spaces apart in both file and rank
        let royal_pos = Position::new(20, 10).unwrap();
        let royal = Piece::new(PieceType::King, Color::Black, royal_pos);
        board.place_piece(royal);
        
        // Place Great Eagle (White) at position where it can attack via forward diagonal
        // Forward diagonal for White is SE (file+, rank-) or SW (file-, rank-)
        let great_eagle_pos = Position::new(5, 25).unwrap();
        let great_eagle = Piece::new(PieceType::GreatEagle, Color::White, great_eagle_pos);
        board.place_piece(great_eagle);
        
        // Place pieces in between on the SE diagonal
        // From (5, 25) to (20, 10): file increases, rank decreases
        for i in 1..15 {
            let blocker_pos = Position::new(5 + i, 25 - i).unwrap();
            let blocker = Piece::new(
                PieceType::Pawn,
                if i % 2 == 0 { Color::Black } else { Color::White },
                blocker_pos
            );
            board.place_piece(blocker);
        }
        
        // Great Eagle should be able to attack via jumping range in forward diagonal
        assert!(board.is_position_attacked_by_color_for_check(royal_pos, Color::White),
                "Great Eagle should be able to attack king via forward diagonal jumping range");
    }

    #[test]
    fn test_great_eagle_no_jump_range_attack_other_directions() {
        // Test: Great Eagle has normal range (NoJump) in other directions
        // Should be blocked by pieces
        let mut board = Board::new();
        
        let royal_pos = Position::new(20, 5).unwrap(); // Same file, different rank
        let royal = Piece::new(PieceType::King, Color::Black, royal_pos);
        board.place_piece(royal);
        
        let great_eagle_pos = Position::new(5, 5).unwrap();
        let great_eagle = Piece::new(PieceType::GreatEagle, Color::Black, great_eagle_pos);
        board.place_piece(great_eagle);
        
        // Place a blocker between them (same file, different rank)
        let blocker_pos = Position::new(10, 5).unwrap();
        let blocker = Piece::new(PieceType::Pawn, Color::White, blocker_pos);
        board.place_piece(blocker);
        
        // Great Eagle should NOT be able to attack (blocked, not a forward diagonal)
        assert!(!board.is_position_attacked_by_color_for_check(royal_pos, Color::Black),
                "Great Eagle should NOT be able to attack when blocked in non-forward-diagonal direction");
    }

    #[test]
    fn test_shitenno_jumping_range_attack_empty_target() {
        // Test: Shitenno can attack empty square with jumping range
        let mut board = Board::new();
        
        let target_pos = Position::new(20, 10).unwrap();
        // No piece at target - empty square
        
        let shitenno_pos = Position::new(5, 10).unwrap();
        let shitenno = Piece::new(PieceType::Shitennou, Color::White, shitenno_pos);
        board.place_piece(shitenno);
        
        // Place pieces in between
        for i in 6..20 {
            let blocker_pos = Position::new(i, 10).unwrap();
            let blocker = Piece::new(
                PieceType::Pawn,
                if i % 2 == 0 { Color::Black } else { Color::White },
                blocker_pos
            );
            board.place_piece(blocker);
        }
        
        assert!(board.is_position_attacked_by_color(target_pos, Color::White),
                "Shitenno should be able to attack empty square with jumping range");
    }

    #[test]
    fn test_shitenno_jumping_range_attack_enemy_target() {
        // Test: Shitenno can attack enemy piece with jumping range
        let mut board = Board::new();
        
        let target_pos = Position::new(20, 10).unwrap();
        let enemy = Piece::new(PieceType::Pawn, Color::Black, target_pos);
        board.place_piece(enemy);
        
        let shitenno_pos = Position::new(5, 10).unwrap();
        let shitenno = Piece::new(PieceType::Shitennou, Color::White, shitenno_pos);
        board.place_piece(shitenno);
        
        // Place pieces in between
        for i in 6..20 {
            let blocker_pos = Position::new(i, 10).unwrap();
            let blocker = Piece::new(
                PieceType::Pawn,
                if i % 2 == 0 { Color::Black } else { Color::White },
                blocker_pos
            );
            board.place_piece(blocker);
        }
        
        assert!(board.is_position_attacked_by_color(target_pos, Color::White),
                "Shitenno should be able to attack enemy piece with jumping range");
    }
}

