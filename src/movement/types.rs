use crate::movement::direction::DirectionSet;
use crate::piece::PieceType;

/// Words needed to cover every current `PieceType` discriminant, including
/// `SwordGeneral`. Spare bits in the last word absorb a few later variants;
/// a discriminant past this array panics on insert.
const PIECE_TYPE_SET_WORDS: usize = (PieceType::SwordGeneral as usize + 64) / 64;

/// Capturing-ray blocker membership. Replaces `HashSet<PieceType>` on the
/// per-square path check. `Debug` stays set-shaped so nested two-step NNUE
/// feature names keep the empty-set text `{}`.
#[derive(Clone, Copy)]
pub struct PieceTypeSet {
    bits: [u64; PIECE_TYPE_SET_WORDS],
}

impl std::fmt::Debug for PieceTypeSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

impl PieceTypeSet {
    pub const fn new() -> Self {
        Self {
            bits: [0; PIECE_TYPE_SET_WORDS],
        }
    }

    pub fn insert(&mut self, piece: PieceType) -> bool {
        let i = piece as usize;
        let word = i / 64;
        let mask = 1u64 << (i % 64);
        let fresh = self.bits[word] & mask == 0;
        self.bits[word] |= mask;
        fresh
    }

    pub fn contains(&self, piece: &PieceType) -> bool {
        let i = *piece as usize;
        self.bits[i / 64] & (1u64 << (i % 64)) != 0
    }

    pub fn iter(&self) -> impl Iterator<Item = &'static PieceType> + '_ {
        crate::eval::ALL_PIECE_TYPES.iter().filter(move |p| self.contains(p))
    }
}

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
        cannot_jump_over: PieceTypeSet,
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

#[cfg(test)]
mod piece_type_set_tests {
    use super::*;

    #[test]
    fn discriminants_match_the_piece_list() {
        assert_eq!(
            crate::eval::ALL_PIECE_TYPES.len(),
            PieceType::SwordGeneral as usize + 1
        );
        let mut seen = vec![false; PIECE_TYPE_SET_WORDS * 64];
        for (n, &piece) in crate::eval::ALL_PIECE_TYPES.iter().enumerate() {
            let i = piece as usize;
            assert_eq!(i, n);
            assert!(!seen[i]);
            seen[i] = true;
        }
    }

    #[test]
    fn membership_and_debug_match_a_hash_set() {
        assert_eq!(format!("{:?}", PieceTypeSet::new()), "{}");
        for modulus in 1..=11 {
            let mut fast = PieceTypeSet::new();
            let mut old = std::collections::HashSet::new();
            for &piece in crate::eval::ALL_PIECE_TYPES {
                if piece as usize % modulus == 0 {
                    assert_eq!(fast.insert(piece), old.insert(piece));
                    assert_eq!(fast.insert(piece), old.insert(piece));
                }
            }
            for piece in crate::eval::ALL_PIECE_TYPES {
                assert_eq!(fast.contains(piece), old.contains(piece));
            }
            let names: Vec<_> = fast.iter().map(|p| format!("{p:?}")).collect();
            let mut expected: Vec<_> = old.iter().map(|p| format!("{p:?}")).collect();
            expected.sort();
            let mut from_bits = names.clone();
            from_bits.sort();
            assert_eq!(from_bits, expected);
            let rendered = format!("{fast:?}");
            assert!(rendered.starts_with('{') && rendered.ends_with('}'));
            assert!(!rendered.contains("bits"));
        }
    }
}
