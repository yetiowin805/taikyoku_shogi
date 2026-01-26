/// Represents a position on the 36x36 board
/// Coordinates are (file, rank) where file is column (0-35) and rank is row (0-35)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct Position {
    pub file: u8,  // column (0-35)
    pub rank: u8,  // row (0-35)
}

impl Position {
    pub fn new(file: u8, rank: u8) -> Option<Position> {
        if file < 36 && rank < 36 {
            Some(Position { file, rank })
        } else {
            None
        }
    }

    /// Convert to a linear index (0-1295)
    pub fn to_index(&self) -> usize {
        (self.rank as usize) * 36 + (self.file as usize)
    }

    /// Create from a linear index
    pub fn from_index(index: usize) -> Option<Position> {
        if index < 1296 {
            Some(Position {
                file: (index % 36) as u8,
                rank: (index / 36) as u8,
            })
        } else {
            None
        }
    }

    /// Check if position is within board bounds
    pub fn is_valid(&self) -> bool {
        self.file < 36 && self.rank < 36
    }

    /// Add a file/rank offset, returning None if out of bounds
    pub fn offset(&self, file_delta: i8, rank_delta: i8) -> Option<Position> {
        let new_file = self.file as i16 + file_delta as i16;
        let new_rank = self.rank as i16 + rank_delta as i16;
        
        if new_file >= 0 && new_file < 36 && new_rank >= 0 && new_rank < 36 {
            Some(Position {
                file: new_file as u8,
                rank: new_rank as u8,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_creation() {
        assert!(Position::new(0, 0).is_some());
        assert!(Position::new(35, 35).is_some());
        assert!(Position::new(36, 0).is_none());
        assert!(Position::new(0, 36).is_none());
    }

    #[test]
    fn test_position_index_conversion() {
        let pos = Position::new(0, 0).unwrap();
        assert_eq!(pos.to_index(), 0);
        
        let pos = Position::new(35, 35).unwrap();
        assert_eq!(pos.to_index(), 1295);
        
        let pos = Position::new(10, 20).unwrap();
        assert_eq!(pos.to_index(), 20 * 36 + 10);
        
        // Test round-trip
        for i in 0..1296 {
            let pos = Position::from_index(i).unwrap();
            assert_eq!(pos.to_index(), i);
        }
    }

    #[test]
    fn test_position_offset() {
        let pos = Position::new(10, 10).unwrap();
        
        assert_eq!(pos.offset(1, 0), Position::new(11, 10));
        assert_eq!(pos.offset(-1, 0), Position::new(9, 10));
        assert_eq!(pos.offset(0, 1), Position::new(10, 11));
        assert_eq!(pos.offset(0, -1), Position::new(10, 9));
        
        // Out of bounds
        assert_eq!(pos.offset(-11, 0), None);
        assert_eq!(pos.offset(26, 0), None);
    }
}

