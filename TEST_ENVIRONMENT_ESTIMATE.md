# Test Environment Estimate

## Option 1: Minimal Test Environment (Recommended)

### What It Would Include
- 5x5 board (instead of 36x36)
- No win conditions
- Simple helper functions to place pieces
- Interactive REPL to test moves
- Board display after each move

### Estimated Work
**Time**: 2-3 hours
**Lines of Code**: ~200-300 lines

### Implementation Approach
Create a new binary: `src/bin/test_free_eagle.rs`

**Key Components**:
1. **Modified Board** (30 lines):
   - Add `size` parameter to `Board::new(size: u8)`
   - Update `Position::to_index()` to use size
   - Or create a `TestBoard` wrapper

2. **Test GameState** (50 lines):
   - Disable win checking
   - Simplified setup
   - Helper methods for piece placement

3. **Helper Functions** (100 lines):
   - `place_piece_at(state, piece_type, color, file, rank)`
   - `print_board(state)`
   - `execute_and_show_move(state, from_file, from_rank, to_file, to_rank)`

4. **Simple REPL** (50-100 lines):
   - Commands: `place`, `move`, `show`, `reset`, `quit`
   - Parse commands and execute

### Example Usage
```
> place FE Black 2 2
> place P White 3 3
> move 2 2 3 3
> show
```

---

## Option 2: Full Test Framework

### What It Would Include
- Configurable board size
- Test case definitions
- Assertion helpers
- Move validation
- Path visualization

### Estimated Work
**Time**: 4-6 hours
**Lines of Code**: ~500-700 lines

### Implementation Approach
Create `src/test_framework.rs` with:
- Test board builder
- Test case runner
- Assertion macros
- Move path visualizer

---

## Recommendation

**Go with Option 1** - it's quick to implement and gives you everything you need to test Free Eagle moves interactively.

### Quick Start Code Structure

```rust
// src/bin/test_free_eagle.rs
use taikyoku_shogi::*;

struct TestBoard {
    size: u8,
    squares: Vec<Option<Piece>>,
}

impl TestBoard {
    fn new(size: u8) -> Self {
        TestBoard {
            size,
            squares: vec![None; (size * size) as usize],
        }
    }
    
    fn place(&mut self, piece: Piece) {
        let idx = piece.position.file as usize * self.size as usize + piece.position.rank as usize;
        self.squares[idx] = Some(piece);
    }
    
    fn get(&self, file: u8, rank: u8) -> Option<Piece> {
        let idx = file as usize * self.size as usize + rank as usize;
        self.squares.get(idx).copied().flatten()
    }
}

fn main() {
    // Simple REPL for testing
    // ...
}
```

### Benefits
- Fast to implement
- Easy to use
- Doesn't affect main codebase
- Can test specific scenarios quickly
- See board state immediately

### Drawbacks
- Separate codebase to maintain
- Need to handle Position validation for smaller board

---

## Alternative: Use Existing Code with Hacks

If you want something even faster:

1. **Temporarily modify `Position::new()`** to accept 5x5 coordinates
2. **Comment out win condition checks** in `has_lost()`
3. **Create a simple test script** that places pieces and executes moves

**Time**: 30 minutes
**Lines**: ~50 lines of modifications

This is the fastest but messier approach.

