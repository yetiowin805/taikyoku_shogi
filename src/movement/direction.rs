/// Represents one of the 8 main directions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    N,   // North (up, increasing rank)
    NE,  // Northeast
    E,   // East (right, increasing file)
    SE,  // Southeast
    S,   // South (down, decreasing rank)
    SW,  // Southwest
    W,   // West (left, decreasing file)
    NW,  // Northwest
}

/// Bitmask for direction sets (u8: 8 bits for 8 directions)
/// Bit order: N=1, NE=2, E=4, SE=8, S=16, SW=32, W=64, NW=128
pub type DirectionSet = u8;

impl Direction {
    /// Convert direction to (file_delta, rank_delta) offset
    pub fn to_offset(&self) -> (i8, i8) {
        match self {
            Direction::N => (0, 1),
            Direction::NE => (1, 1),
            Direction::E => (1, 0),
            Direction::SE => (1, -1),
            Direction::S => (0, -1),
            Direction::SW => (-1, -1),
            Direction::W => (-1, 0),
            Direction::NW => (-1, 1),
        }
    }

    /// Convert direction to bitmask bit
    pub fn to_bit(&self) -> u8 {
        match self {
            Direction::N => 1,
            Direction::NE => 2,
            Direction::E => 4,
            Direction::SE => 8,
            Direction::S => 16,
            Direction::SW => 32,
            Direction::W => 64,
            Direction::NW => 128,
        }
    }

    /// Get direction from bitmask bit
    pub fn from_bit(bit: u8) -> Option<Direction> {
        match bit {
            1 => Some(Direction::N),
            2 => Some(Direction::NE),
            4 => Some(Direction::E),
            8 => Some(Direction::SE),
            16 => Some(Direction::S),
            32 => Some(Direction::SW),
            64 => Some(Direction::W),
            128 => Some(Direction::NW),
            _ => None,
        }
    }

    /// Get all 8 directions
    pub fn all() -> Vec<Direction> {
        vec![
            Direction::N, Direction::NE, Direction::E, Direction::SE,
            Direction::S, Direction::SW, Direction::W, Direction::NW,
        ]
    }
}

/// DirectionSet constants
/// All 8 directions (0xFF = 255)
pub const DIRECTION_SET_ALL: DirectionSet = 0xFF;

/// Orthogonal directions only (N, S, E, W) = 0x55 = 85
pub const DIRECTION_SET_ORTHOGONAL: DirectionSet = 0x55;

/// Diagonal directions only (NE, SE, SW, NW) = 0xAA = 170
pub const DIRECTION_SET_DIAGONAL: DirectionSet = 0xAA;

/// Helper functions for working with DirectionSet
pub fn direction_set_contains(set: DirectionSet, direction: Direction) -> bool {
    (set & direction.to_bit()) != 0
}

pub fn direction_set_add(set: &mut DirectionSet, direction: Direction) {
    *set |= direction.to_bit();
}

pub fn direction_set_remove(set: &mut DirectionSet, direction: Direction) {
    *set &= !direction.to_bit();
}

pub fn direction_set_to_directions(set: DirectionSet) -> Vec<Direction> {
    Direction::all()
        .into_iter()
        .filter(|&dir| direction_set_contains(set, dir))
        .collect()
}

pub fn direction_set_is_empty(set: DirectionSet) -> bool {
    set == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direction_offsets() {
        assert_eq!(Direction::N.to_offset(), (0, 1));
        assert_eq!(Direction::E.to_offset(), (1, 0));
        assert_eq!(Direction::SE.to_offset(), (1, -1));
    }

    #[test]
    fn test_direction_set() {
        use super::{direction_set_contains, direction_set_add, direction_set_remove, DIRECTION_SET_ORTHOGONAL};
        let mut set = DIRECTION_SET_ORTHOGONAL;
        assert!(direction_set_contains(set, Direction::N));
        assert!(direction_set_contains(set, Direction::E));
        assert!(!direction_set_contains(set, Direction::NE));
        
        direction_set_add(&mut set, Direction::NE);
        assert!(direction_set_contains(set, Direction::NE));
        
        direction_set_remove(&mut set, Direction::N);
        assert!(!direction_set_contains(set, Direction::N));
    }

    #[test]
    fn test_all_directions() {
        use super::{direction_set_contains, DIRECTION_SET_ALL};
        let all = DIRECTION_SET_ALL;
        for dir in Direction::all() {
            assert!(direction_set_contains(all, dir));
        }
    }
}

