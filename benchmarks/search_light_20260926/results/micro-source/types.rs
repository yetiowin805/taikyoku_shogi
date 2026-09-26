use crate::movement::direction::DirectionSet;
use crate::piece::PieceType;
use std::collections::HashSet;

/// Blocking mode for range movement
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockingMode {
    /// NoJump: Blocked by any piece in the path (standard range movement)
    NoJump,
    /// Jump: Can jump over pieces without capturing them (pieces remain on board)
    Jump,
    /// Capturing: Jumps over and captures all pieces in path (both enemy and friendly),
    /// but cannot land on a friendly piece
    Capturing,
}

/// Movement capability - defines how a piece can move
#[derive(Debug, Clone)]
pub enum MovementCapability {
    /// Simple movement: Move 1 to max_distance squares in specified directions
    Simple {
        directions: DirectionSet,
        max_distance: u8,
    },
    /// Range movement: Unlimited range in specified directions with blocking mode
    /// For Capturing mode, cannot_jump_over specifies piece types that cannot be jumped over
    Range {
        directions: DirectionSet,
        blocking: BlockingMode,
        /// Set of piece types that cannot be jumped over (only used for Capturing mode)
        /// Empty set means all pieces can be jumped over
        cannot_jump_over: HashSet<PieceType>,
    },
    /// Jumping movement: Jump to relative positions (not direction-based)
    /// Offsets are (file_delta, rank_delta) relative to starting position
    Jumping {
        offsets: Vec<(i8, i8)>,
    },
    /// Two-step movement: First move, then second move from intermediate position
    /// Both moves are full MovementCapability instances
    TwoStep {
        first: Box<MovementCapability>,
        second: Box<MovementCapability>,
    },
    /// Conditional diagonal jump: Can jump base_jump spaces normally,
    /// and can jump conditional_jumps distances if the first required_jump_positions
    /// positions have pieces and the next empty_after_jump positions are empty
    /// Example: Wooden Dove can jump 3 spaces normally, and 4-5 spaces if positions 1-2 have pieces and position 3 is empty
    ConditionalDiagonalJump {
        directions: DirectionSet,  // Diagonal directions only
        base_jump: u8,  // Base jump distance (e.g., 3)
        conditional_jumps: Vec<u8>,  // Conditional jump distances (e.g., [4, 5])
        required_jump_positions: u8,  // Number of positions that must have pieces (e.g., 2 for positions 1-2)
        empty_after_jump: u8,  // Number of positions that must be empty after the jump positions (e.g., 1 for position 3)
    },
    /// Free Eagle multi-move: Can move up to max_distance_forward_diagonal in forward diagonals,
    /// or up to max_distance_other in other directions, capturing all enemy pieces along the path
    FreeEagleMultiMove {
        max_distance_forward_diagonal: u8,  // 4 for forward diagonals
        max_distance_other: u8,  // 3 for orthogonal and backward diagonals
    },
}

