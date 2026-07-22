use crate::board::Board;
use crate::piece::{Piece, PieceType, Color};
use crate::position::Position;
use crate::movement::types::BlockingMode;
use crate::movement::MovementConfig;
use crate::path_utils;
use crate::game_state::Move;

/// Represents changes made by a single move
/// This structured representation enables efficient move simulation without cloning the board
#[derive(Clone, Debug)]
pub struct MoveDelta {
    /// Piece that moved: (from_position, to_position, original_piece)
    /// None if no piece moved (e.g., only captures)
    pub piece_moved: Option<(Position, Position, Piece)>,
    
    /// Pieces removed along path (for capturing range moves)
    /// Vec of (position, removed_piece)
    pub pieces_removed: Vec<(Position, Piece)>,
    
    /// Promotion: (position, old_piece_type, new_piece_type)
    /// None if no promotion occurred
    pub piece_promoted: Option<(Position, PieceType, PieceType)>,
}

impl MoveDelta {
    /// Create empty delta (no changes)
    pub fn new() -> Self {
        MoveDelta {
            piece_moved: None,
            pieces_removed: Vec::new(),
            piece_promoted: None,
        }
    }
    
    /// Check if delta is empty (no changes)
    pub fn is_empty(&self) -> bool {
        self.piece_moved.is_none() 
            && self.pieces_removed.is_empty() 
            && self.piece_promoted.is_none()
    }
    
    /// Get all positions affected by this delta
    pub fn affected_positions(&self) -> Vec<Position> {
        let mut positions = Vec::new();
        
        if let Some((from, to, _)) = &self.piece_moved {
            positions.push(*from);
            positions.push(*to);
        }
        
        for (pos, _) in &self.pieces_removed {
            positions.push(*pos);
        }
        
        if let Some((pos, _, _)) = &self.piece_promoted {
            positions.push(*pos);
        }
        
        positions
    }
    
    /// Get pieces that moved (for incremental attack detection)
    /// Returns Vec of (from_position, to_position, piece)
    pub fn moved_pieces(&self) -> Vec<(Position, Position, Piece)> {
        if let Some((from, to, piece)) = &self.piece_moved {
            vec![(*from, *to, *piece)]
        } else {
            Vec::new()
        }
    }
    
    /// Get pieces that were removed (for incremental attack detection)
    pub fn removed_pieces(&self) -> &[(Position, Piece)] {
        &self.pieces_removed
    }
    
    /// Get affected pieces for incremental attack detection (Approach 3)
    /// Future: Can be used to only re-check attacks from affected pieces
    /// Returns information about which pieces moved, were removed, and paths that were cleared
    pub fn affected_pieces_for_attack_detection(&self) -> AttackAffectedPieces {
        AttackAffectedPieces {
            moved_pieces: self.moved_pieces(),
            removed_pieces: self.removed_pieces().to_vec(),
            cleared_paths: self.cleared_paths(),
        }
    }
    
    /// Get paths that were cleared (for range moves)
    /// Future: Used for incremental attack detection to know which paths to re-check
    fn cleared_paths(&self) -> Vec<(Position, Position)> {
        let mut paths = Vec::new();
        if let Some((from, to, _)) = &self.piece_moved {
            // Check if this was a range move that cleared a path
            if !self.pieces_removed.is_empty() {
                // This was a capturing range move, so the path from from to to was cleared
                paths.push((*from, *to));
            }
        }
        paths
    }
    
    /// Convert to UndoDelta for Approach 1 (Undo/Redo).
    /// Reverses piece_moved, restores pieces_removed, and reverses promotion.
    pub fn to_undo_delta(&self) -> Option<UndoDelta> {
        let (from, to, original_mover) = self.piece_moved?;
        let _ = self.piece_promoted;
        Some(UndoDelta {
            from,
            to,
            original_mover,
            removed: self.pieces_removed.clone(),
        })
    }
}

/// Information about pieces affected by a move for incremental attack detection
/// Future: Used by Approach 3 (Incremental Attack Detection)
#[derive(Debug, Clone)]
pub struct AttackAffectedPieces {
    pub moved_pieces: Vec<(Position, Position, Piece)>,
    pub removed_pieces: Vec<(Position, Piece)>,
    pub cleared_paths: Vec<(Position, Position)>,
}

/// Board-level undo for a single move (Approach 1: Undo/Redo).
/// Game-level fields (turn, draw counter) are handled by [`crate::game_state::SearchUndo`].
#[derive(Debug, Clone)]
pub struct UndoDelta {
    pub from: Position,
    pub to: Position,
    pub original_mover: Piece,
    pub removed: Vec<(Position, Piece)>,
}

impl UndoDelta {
    /// Apply this undo to a board (mover back to `from`, restore captures).
    pub fn apply_to_board(&self, board: &mut Board) {
        board.remove_piece(self.to);
        board.place_piece(self.original_mover);
        for (_pos, piece) in &self.removed {
            board.place_piece(*piece);
        }
    }
}

/// Virtual board that applies a delta to a base board
/// This allows simulating moves without cloning the entire board
pub struct VirtualBoard<'a> {
    base_board: &'a Board,
    delta: MoveDelta,
}

impl<'a> VirtualBoard<'a> {
    /// Create a new virtual board from a base board and delta
    pub fn new(base_board: &'a Board, delta: MoveDelta) -> Self {
        VirtualBoard {
            base_board,
            delta,
        }
    }
    
    /// Compose with another delta (for nested simulations)
    /// Returns a new VirtualBoard with both deltas applied
    /// Note: The other delta is applied on top of this one
    pub fn compose(&self, other_delta: MoveDelta) -> VirtualBoard<'a> {
        // For now, we'll create a combined delta
        // This is a simplified composition - in practice, we might want more sophisticated merging
        let mut combined = self.delta.clone();
        
        // If other delta moves a piece, update our delta
        if let Some((from, to, piece)) = other_delta.piece_moved {
            // Check if this conflicts with our delta
            if let Some((our_from, our_to, _)) = &combined.piece_moved {
                // If other move is from our destination, chain the moves
                if *our_to == from {
                    combined.piece_moved = Some((*our_from, to, piece));
                } else {
                    // Different piece or conflicting move - other takes precedence
                    combined.piece_moved = Some((from, to, piece));
                }
            } else {
                combined.piece_moved = Some((from, to, piece));
            }
        }
        
        // Merge removed pieces (avoid duplicates)
        for (pos, piece) in other_delta.pieces_removed {
            if !combined.pieces_removed.iter().any(|(p, _)| *p == pos) {
                combined.pieces_removed.push((pos, piece));
            }
        }
        
        // Other promotion takes precedence
        if other_delta.piece_promoted.is_some() {
            combined.piece_promoted = other_delta.piece_promoted;
        }
        
        VirtualBoard::new(self.base_board, combined)
    }
    
    /// Get the underlying delta (for future undo/redo)
    pub fn delta(&self) -> &MoveDelta {
        &self.delta
    }
    
    /// Get the base board (for debugging/inspection)
    pub fn base_board(&self) -> &'a Board {
        self.base_board
    }
}

/// Trait for board-like structures that can be queried
/// Allows Board, VirtualBoard, and future implementations to be used interchangeably
pub trait BoardLike {
    /// Get piece at position, if any
    fn get_piece(&self, pos: Position) -> Option<Piece>;
    
    /// Check if position is empty
    fn is_empty(&self, pos: Position) -> bool;
    
    /// Check if position has a piece of given color
    fn has_piece_of_color(&self, pos: Position, color: Color) -> bool;
    
    /// Get all pieces of a given color
    /// Note: This is less efficient for VirtualBoard, but needed for compatibility
    fn get_pieces_by_color(&self, color: Color) -> Vec<Piece>;
    
    /// Check if a position is attacked by pieces of a given color
    /// This is the primary query used in move simulation
    fn is_position_attacked_by_color(&self, pos: Position, attacker_color: Color) -> bool;
    
    /// Check if a position is attacked, optimized for check detection (treats capturing-only pieces as short-range)
    fn is_position_attacked_by_color_for_check(&self, pos: Position, attacker_color: Color) -> bool;
}

// Implement for Board
impl BoardLike for Board {
    fn get_piece(&self, pos: Position) -> Option<Piece> {
        Board::get_piece(self, pos)
    }
    
    fn is_empty(&self, pos: Position) -> bool {
        Board::is_empty(self, pos)
    }
    
    fn has_piece_of_color(&self, pos: Position, color: Color) -> bool {
        Board::has_piece_of_color(self, pos, color)
    }
    
    fn get_pieces_by_color(&self, color: Color) -> Vec<Piece> {
        Board::get_pieces_by_color(self, color)
    }
    
    fn is_position_attacked_by_color(&self, pos: Position, attacker_color: Color) -> bool {
        Board::is_position_attacked_by_color(self, pos, attacker_color)
    }
    
    fn is_position_attacked_by_color_for_check(&self, pos: Position, attacker_color: Color) -> bool {
        Board::is_position_attacked_by_color_for_check(self, pos, attacker_color)
    }
}

// Implement for VirtualBoard
impl<'a> BoardLike for VirtualBoard<'a> {
    fn get_piece(&self, pos: Position) -> Option<Piece> {
        // 1. Check if piece moved here
        if let Some((from, to, original_piece)) = &self.delta.piece_moved {
            if pos == *to {
                let mut piece = *original_piece;
                piece.position = *to;
                
                // Apply promotion if this position was promoted
                if let Some((promo_pos, _, new_type)) = &self.delta.piece_promoted {
                    if *promo_pos == *to {
                        piece.piece_type = *new_type;
                        piece.is_promoted = true;
                    }
                }
                return Some(piece);
            }
            if pos == *from {
                return None; // Piece moved away
            }
        }
        
        // 2. Check if piece was removed (capturing range move)
        for (removed_pos, _) in &self.delta.pieces_removed {
            if pos == *removed_pos {
                return None;
            }
        }
        
        // 3. Fall back to base board
        self.base_board.get_piece(pos)
    }
    
    fn is_empty(&self, pos: Position) -> bool {
        self.get_piece(pos).is_none()
    }
    
    fn has_piece_of_color(&self, pos: Position, color: Color) -> bool {
        self.get_piece(pos)
            .map(|p| p.color == color)
            .unwrap_or(false)
    }
    
    fn get_pieces_by_color(&self, color: Color) -> Vec<Piece> {
        // Get pieces from base board
        let mut pieces = self.base_board.get_pieces_by_color(color);
        
        // Remove pieces that were moved or removed
        if let Some((from, to, original_piece)) = &self.delta.piece_moved {
            if original_piece.color == color {
                // Remove piece from old position
                pieces.retain(|p| p.position != *from);
                // Add piece at new position (with promotion if applicable)
                let mut moved_piece = *original_piece;
                moved_piece.position = *to;
                if let Some((promo_pos, _, new_type)) = &self.delta.piece_promoted {
                    if *promo_pos == *to {
                        moved_piece.piece_type = *new_type;
                        moved_piece.is_promoted = true;
                    }
                }
                pieces.push(moved_piece);
            }
        }
        
        // Remove pieces that were captured
        for (removed_pos, removed_piece) in &self.delta.pieces_removed {
            if removed_piece.color == color {
                pieces.retain(|p| p.position != *removed_pos);
            }
        }
        
        pieces
    }
    
    fn is_position_attacked_by_color(&self, pos: Position, attacker_color: Color) -> bool {
        // Use the shared attack detection implementation
        // Note: For VirtualBoard, specialized tengu_attack functions won't be used
        // but can_reach will work correctly
        crate::board::is_position_attacked_by_color_impl(self, pos, attacker_color, false)
    }
    
    fn is_position_attacked_by_color_for_check(&self, pos: Position, attacker_color: Color) -> bool {
        // Use the shared attack detection implementation optimized for check detection
        crate::board::is_position_attacked_by_color_impl(self, pos, attacker_color, true)
    }
}

/// Convert a Move to a MoveDelta by analyzing what would change
/// This is the core function that enables move simulation
pub fn move_to_delta(board: &Board, mv: &Move, moving_piece: &Piece) -> MoveDelta {
    let mut delta = MoveDelta::new();
    
    // 1. Track piece movement
    delta.piece_moved = Some((mv.from, mv.to, *moving_piece));
    
    // 2. Handle capturing range movements (pieces cleared along the path, not endpoints)
    let config = MovementConfig::for_piece(moving_piece);
    let uses_capturing = config.capabilities.iter().any(|cap| {
        if let crate::movement::types::MovementCapability::Range { blocking, .. } = cap {
            *blocking == BlockingMode::Capturing
        } else {
            false
        }
    });
    
    if uses_capturing {
        let path_positions = path_utils::get_path_positions(mv.from, mv.to);
        for pos in path_positions {
            if pos != mv.from && pos != mv.to {
                if let Some(piece) = board.get_piece(pos) {
                    delta.pieces_removed.push((pos, piece));
                }
            }
        }
    }

    // 3. Capture at destination — must be recorded so VirtualBoard piece-lists
    // drop the captured piece (get_piece alone is not enough for attack checks).
    if let Some(captured) = board.get_piece(mv.to) {
        if captured.color != moving_piece.color {
            delta.pieces_removed.push((mv.to, captured));
        }
    }

    // 4. Two-step intermediate capture (if present)
    if let Some(intermediate) = mv.intermediate() {
        if let Some(captured) = board.get_piece(intermediate) {
            if captured.color != moving_piece.color {
                delta.pieces_removed.push((intermediate, captured));
            }
        }
    }
    
    // 5. Handle promotion
    if mv.promoted {
        if let Some(new_type) = moving_piece.piece_type.promotes_to() {
            delta.piece_promoted = Some((
                mv.to,
                moving_piece.piece_type,
                new_type,
            ));
        }
    }
    
    delta
}

/// Create a VirtualBoard by simulating a move
pub fn simulate_move<'a>(base_board: &'a Board, mv: &Move, moving_piece: &Piece) -> VirtualBoard<'a> {
    let delta = move_to_delta(base_board, mv, moving_piece);
    VirtualBoard::new(base_board, delta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::piece::PieceType;

    #[test]
    fn test_move_delta_new() {
        let delta = MoveDelta::new();
        assert!(delta.is_empty());
        assert!(delta.affected_positions().is_empty());
    }

    #[test]
    fn test_move_delta_piece_moved() {
        let mut delta = MoveDelta::new();
        let from = Position::new(0, 0).unwrap();
        let to = Position::new(1, 1).unwrap();
        let piece = Piece::new(PieceType::King, Color::Black, from);
        delta.piece_moved = Some((from, to, piece));
        
        assert!(!delta.is_empty());
        let positions = delta.affected_positions();
        assert_eq!(positions.len(), 2);
        assert!(positions.contains(&from));
        assert!(positions.contains(&to));
    }

    #[test]
    fn test_virtual_board_get_piece() {
        let mut board = Board::new();
        let pos = Position::new(10, 10).unwrap();
        let piece = Piece::new(PieceType::King, Color::Black, pos);
        board.place_piece(piece);
        
        // Create delta that moves the piece
        let mut delta = MoveDelta::new();
        let new_pos = Position::new(11, 11).unwrap();
        delta.piece_moved = Some((pos, new_pos, piece));
        
        let virtual_board = VirtualBoard::new(&board, delta);
        
        // Old position should be empty
        assert!(virtual_board.is_empty(pos));
        // New position should have the piece
        let moved_piece = virtual_board.get_piece(new_pos);
        assert!(moved_piece.is_some());
        assert_eq!(moved_piece.unwrap().position, new_pos);
    }
    
    #[test]
    fn test_virtual_board_promotion() {
        let mut board = Board::new();
        let pos = Position::new(10, 10).unwrap();
        let piece = Piece::new(PieceType::Pawn, Color::Black, pos);
        board.place_piece(piece);
        
        // Create delta that moves and promotes the piece
        let mut delta = MoveDelta::new();
        let new_pos = Position::new(11, 11).unwrap();
        delta.piece_moved = Some((pos, new_pos, piece));
        delta.piece_promoted = Some((new_pos, PieceType::Pawn, PieceType::GoldGeneral));
        
        let virtual_board = VirtualBoard::new(&board, delta);
        let moved_piece = virtual_board.get_piece(new_pos);
        assert!(moved_piece.is_some());
        assert_eq!(moved_piece.unwrap().piece_type, PieceType::GoldGeneral);
        assert!(moved_piece.unwrap().is_promoted);
    }
    
    #[test]
    fn test_virtual_board_capturing_range() {
        let mut board = Board::new();
        let from = Position::new(10, 10).unwrap();
        let to = Position::new(15, 15).unwrap();
        let moving_piece = Piece::new(PieceType::King, Color::Black, from);
        board.place_piece(moving_piece);
        
        // Place pieces along the path
        let captured1 = Piece::new(PieceType::Pawn, Color::White, Position::new(12, 12).unwrap());
        let captured2 = Piece::new(PieceType::Pawn, Color::Black, Position::new(13, 13).unwrap());
        board.place_piece(captured1);
        board.place_piece(captured2);
        
        // Create delta with capturing range move
        let mut delta = MoveDelta::new();
        delta.piece_moved = Some((from, to, moving_piece));
        delta.pieces_removed.push((Position::new(12, 12).unwrap(), captured1));
        delta.pieces_removed.push((Position::new(13, 13).unwrap(), captured2));
        
        let virtual_board = VirtualBoard::new(&board, delta);
        
        // Check that captured pieces are gone
        assert!(virtual_board.is_empty(Position::new(12, 12).unwrap()));
        assert!(virtual_board.is_empty(Position::new(13, 13).unwrap()));
        // Check that moving piece is at destination
        assert!(!virtual_board.is_empty(to));
    }
    
    #[test]
    fn test_move_to_delta() {
        let mut board = Board::new();
        let from = Position::new(10, 10).unwrap();
        let to = Position::new(11, 11).unwrap();
        let piece = Piece::new(PieceType::King, Color::Black, from);
        board.place_piece(piece);
        
        let mv = Move::new(from, to);
        let delta = move_to_delta(&board, &mv, &piece);
        
        assert!(!delta.is_empty());
        assert!(delta.piece_moved.is_some());
        assert_eq!(delta.pieces_removed.len(), 0);
        assert!(delta.piece_promoted.is_none());
    }
    
    #[test]
    fn test_virtual_board_get_pieces_by_color() {
        let mut board = Board::new();
        let pos1 = Position::new(10, 10).unwrap();
        let pos2 = Position::new(11, 11).unwrap();
        let piece1 = Piece::new(PieceType::King, Color::Black, pos1);
        let piece2 = Piece::new(PieceType::Pawn, Color::Black, pos2);
        board.place_piece(piece1);
        board.place_piece(piece2);
        
        // Move piece1
        let mut delta = MoveDelta::new();
        let new_pos = Position::new(12, 12).unwrap();
        delta.piece_moved = Some((pos1, new_pos, piece1));
        
        let virtual_board = VirtualBoard::new(&board, delta);
        let black_pieces = virtual_board.get_pieces_by_color(Color::Black);
        
        assert_eq!(black_pieces.len(), 2);
        assert!(black_pieces.iter().any(|p| p.position == new_pos));
        assert!(black_pieces.iter().any(|p| p.position == pos2));
        assert!(!black_pieces.iter().any(|p| p.position == pos1));
    }

    #[test]
    fn test_move_to_delta_records_destination_capture() {
        let mut board = Board::new();
        let from = Position::new(10, 10).unwrap();
        let to = Position::new(11, 10).unwrap();
        let mover = Piece::new(PieceType::GoldGeneral, Color::White, from);
        let victim = Piece::new(PieceType::Lance, Color::Black, to);
        board.place_piece(mover);
        board.place_piece(victim);

        let mv = Move::new(from, to);
        let delta = move_to_delta(&board, &mv, &mover);

        assert!(
            delta
                .pieces_removed
                .iter()
                .any(|(pos, p)| *pos == to && p.piece_type == PieceType::Lance),
            "destination capture must be in pieces_removed"
        );

        let vb = simulate_move(&board, &mv, &mover);
        let black = vb.get_pieces_by_color(Color::Black);
        assert!(
            !black.iter().any(|p| p.position == to),
            "captured piece must leave the color piece-list"
        );
        assert_eq!(vb.get_piece(to).map(|p| p.color), Some(Color::White));
    }

    #[test]
    fn test_capturing_checker_clears_check_in_simulation() {
        // White king checked vertically by a Black lance; adjacent White gold
        // captures the lance. Simulation must report the king safe afterward.
        let mut board = Board::new();
        let king_pos = Position::new(10, 20).unwrap();
        let checker_pos = Position::new(10, 10).unwrap();
        let capturer_pos = Position::new(11, 10).unwrap();

        board.place_piece(Piece::new(PieceType::King, Color::White, king_pos));
        board.place_piece(Piece::new(PieceType::Lance, Color::Black, checker_pos));
        let capturer = Piece::new(PieceType::GoldGeneral, Color::White, capturer_pos);
        board.place_piece(capturer);

        assert!(
            board.is_position_attacked_by_color_for_check(king_pos, Color::Black),
            "precondition: king is in check from lance"
        );
        assert!(
            board.is_position_attacked_by_color(king_pos, Color::Black),
            "precondition: also attacked under full attack rules"
        );

        let mv = Move::new(capturer_pos, checker_pos);
        let vb = simulate_move(&board, &mv, &capturer);

        assert!(
            !vb.is_position_attacked_by_color_for_check(king_pos, Color::Black),
            "after capturing the checker, king must not be in check"
        );
        assert!(
            !vb
                .get_pieces_by_color(Color::Black)
                .iter()
                .any(|p| p.piece_type == PieceType::Lance),
            "checker must be gone from black piece list"
        );
    }
}

