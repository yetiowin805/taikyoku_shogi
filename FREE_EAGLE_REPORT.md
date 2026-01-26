# Free Eagle Implementation Report

## Overview
This document provides a comprehensive overview of the Free Eagle piece implementation, including move generation, execution, notation, and game viewing functionality.

## Table of Contents
1. [Free Eagle Move Patterns](#free-eagle-move-patterns)
2. [Move Generation](#move-generation)
3. [Move Execution](#move-execution)
4. [Move Notation](#move-notation)
5. [Game Viewing/Replay](#game-viewingreplay)
6. [Key Data Structures](#key-data-structures)
7. [Known Issues](#known-issues)

---

## Free Eagle Move Patterns

The Free Eagle has 7 movement patterns:

### Pattern 0: Standard Range Moves
- Uses standard range movement (like a Queen in chess)
- Generated via `MovementConfig::for_piece()` 
- No special path - just regular range moves
- **Location**: `src/game_state.rs:1524-1538`

### Pattern 1: Forward Diagonal Multi-Move (up to 4 spaces)
- Forward diagonals: NE/NW for Black, SE/SW for White
- Can move up to 4 spaces in these directions
- Can capture and continue
- **Location**: `src/game_state.rs:1273-1305`

### Pattern 2: Other Directions Multi-Move (up to 3 spaces)
- Other directions: N, S, E, W, and backward diagonals
- Can move up to 3 spaces in these directions
- Can capture and continue
- **Location**: `src/game_state.rs:1307-1339`

### Pattern 3: Forward Diagonal Special (3 forward + 1 back)
- **Condition**: Only if there's a capture on the 3rd space
- Moves 3 spaces forward diagonally, then 1 space back
- **Location**: `src/game_state.rs:1341-1390`

### Pattern 4: Any Direction Special (2 forward + 1 back)
- **Condition**: Only if there's a capture on the 2nd space
- Moves 2 spaces in any direction, then 1 space back
- **Location**: `src/game_state.rs:1392-1442`

### Pattern 5: Stay in Place (capture 1 space away)
- Captures an enemy piece 1 space away in any direction
- Returns to starting position
- Path: `[start, capture_pos, start]`
- **Location**: `src/game_state.rs:1444-1460`

### Pattern 6: Stay in Place (capture on 2nd space along forward diagonal)
- **Condition**: Only if there's a capture on the 2nd space (Pattern 5 covers 1st space)
- Captures one or two enemies on forward diagonals
- Returns to starting position
- Path: `[start, pos1, pos2, start]`
- **Location**: `src/game_state.rs:1462-1522`

---

## Move Generation

### Entry Point
- **Function**: `generate_free_eagle_moves(&self, piece: &Piece) -> Vec<Move>`
- **Location**: `src/game_state.rs:1255-1541`
- **Called from**: `generate_legal_moves_for_pieces()` at line 561

### Key Logic
1. Defines forward diagonals and other directions based on piece color
2. Generates moves for each pattern (1-6)
3. Also generates Pattern 0 (standard range moves)
4. Each multi-move pattern creates a `Move` with `free_eagle_path` set
5. Standard range moves create regular `Move` objects (no path)

### Important Notes
- All patterns call `is_legal_move()` to validate before adding to moves list
- Patterns 3 and 4 require captures at specific positions to be valid
- Patterns 5 and 6 generate moves that end at the starting position

---

## Move Execution

### Entry Point
- **Function**: `make_move(&mut self, mut mv: Move) -> Option<Position>`
- **Location**: `src/game_state.rs:812-990`

### Free Eagle Path Reconstruction (Lines 818-837)
When a Free Eagle move is loaded from JSON (without `free_eagle_path`):
1. Gets the piece at `mv.from`
2. Checks if it's a Free Eagle and path is missing
3. Generates all legal Free Eagle moves
4. Finds a matching move (same destination)
5. If multi-move pattern: copies the path
6. If standard range move: proceeds without path (regular execution)

### Free Eagle Path Execution (Lines 899-975)
When `mv.free_eagle_path` is `Some(path)`:
1. Executes each step in the path sequentially:
   - For each `i` from 1 to `path.len()`:
     - Gets piece at `path[i-1]` (current position)
     - Checks for capture at `path[i]` (destination)
     - Removes captured piece if enemy
     - Moves piece from `path[i-1]` to `path[i]`
     - Handles promotion on final step
2. Records moves in `move_history`:
   - Records intermediate steps (lines 941-954)
   - Records final destination if different (lines 955-964)
   - **NOTE**: Does NOT record the main move with path!
3. Updates capture/promotion counter
4. Changes turn
5. Returns `None` (not a two-step move)

### Current Issues
- **Problem**: The main move with `free_eagle_path` is NOT recorded in `move_history`
- **Impact**: When reconstructing game state, the path information is lost
- **Location**: Lines 941-964 only record individual steps, not the composite move

---

## Move Notation

### Move Structure
```rust
pub struct Move {
    pub from: Position,
    pub to: Position,
    pub promoted: bool,
    pub intermediate: Option<Position>,  // For two-step moves
    pub free_eagle_path: Option<Vec<Position>>,  // For Free Eagle multi-moves
}
```

### Notation Format
The notation uses Japanese-style format:
- Format: `{move_number}. {color}: {to_file}{to_rank}{piece_symbol}{from_file}{from_rank}{promotion}`
- Example: `143. Black: 27二十五FE24二十二`
  - Move 143
  - Black's turn
  - To: file 27, rank 25 (displayed as flipped: 10, 12)
  - Piece: FE (Free Eagle)
  - From: file 24, rank 22

### Notation Generation
- **Location**: `src/main.rs:234-238` (single moves)
- **Location**: `src/game_history.rs:297-494` (game replay)

### Free Eagle Notation Issues
- Currently, Free Eagle multi-moves are displayed as single moves
- The path information is not shown in notation
- Intermediate steps are recorded in `move_history` but not displayed properly

---

## Game Viewing/Replay

### Entry Point
- **Command**: `cargo run -- view <game_file>`
- **Function**: `view_game(filename: &str)` in `src/main.rs:102-112`
- **Main Logic**: `GameHistory::format_game()` in `src/game_history.rs:298-494`

### How It Works
1. Loads game JSON file
2. Creates initial game state with `setup_initial_position()`
3. For each move in the game:
   - Formats the move notation
   - Applies the move to the game state
   - Shows board periodically
4. Displays final result

### Move Application (Lines 455-491)
```rust
// Create Move object from MoveRecord
let move_obj = if let (Some(inter_file), Some(inter_rank)) = (mv.intermediate_file, mv.intermediate_rank) {
    Move::new_two_step_with_promotion(from, intermediate, to, mv.promoted)
} else {
    Move::new_with_promotion(from, to, mv.promoted)
};

// Apply move
let move_result = current_state.make_move(move_obj);
```

### Current Issues with Free Eagle
- **Problem**: When replaying, Free Eagle moves loaded from JSON don't have `free_eagle_path`
- **Solution**: The path reconstruction logic (lines 818-837) handles this
- **Problem**: Free Eagle multi-moves are not displayed as multiple moves
- **Impact**: The notation shows a single move instead of showing each step

---

## Key Data Structures

### Move Record (JSON Format)
```rust
pub struct MoveRecord {
    pub move_number: usize,
    pub color: Color,
    pub from_file: u8,
    pub from_rank: u8,
    pub to_file: u8,
    pub to_rank: u8,
    pub promoted: bool,
    pub intermediate_file: Option<u8>,  // For two-step moves
    pub intermediate_rank: Option<u8>,
    // NOTE: No field for free_eagle_path!
}
```

### Move (In-Memory)
```rust
pub struct Move {
    pub from: Position,
    pub to: Position,
    pub promoted: bool,
    pub intermediate: Option<Position>,
    pub free_eagle_path: Option<Vec<Position>>,  // Only in memory, not in JSON
}
```

### GameState
```rust
pub struct GameState {
    board: Board,
    current_turn: Color,
    move_history: Vec<Move>,  // Contains Move objects with paths
    turns_without_capture_or_promotion: u32,
}
```

---

## Known Issues

### Issue 1: Free Eagle Multi-Moves Not Recorded Properly
- **Location**: `src/game_state.rs:899-975`
- **Problem**: The main move with `free_eagle_path` is not recorded in `move_history`
- **Impact**: Game state reconstruction may not work correctly
- **Fix Needed**: Record the main move with its path after executing steps

### Issue 2: Free Eagle Multi-Moves Not Displayed as Multiple Moves
- **Location**: `src/main.rs:180-245`, `src/game_history.rs:297-494`
- **Problem**: Multi-move patterns are shown as single moves in notation
- **Impact**: Hard to see what actually happened (e.g., "3 forward + 1 back")
- **Fix Needed**: Display each step as a separate move with sequential numbers

### Issue 3: Path Reconstruction May Not Match Original
- **Location**: `src/game_state.rs:818-837`
- **Problem**: When reconstructing path, it finds "a" matching move, not necessarily "the" original move
- **Impact**: If multiple moves go to the same destination, wrong path might be used
- **Fix Needed**: Store path in JSON or use more specific matching

### Issue 4: Move History Contains Intermediate Steps
- **Location**: `src/game_state.rs:941-964`
- **Problem**: Intermediate steps are recorded as separate moves in `move_history`
- **Impact**: `move_history` may contain moves that shouldn't be there
- **Fix Needed**: Either don't record intermediates, or record them properly with context

---

## Code Locations Summary

### Free Eagle Move Generation
- **Main Function**: `generate_free_eagle_moves()` - `src/game_state.rs:1255-1541`
- **Pattern 0**: Lines 1524-1538
- **Pattern 1**: Lines 1273-1305
- **Pattern 2**: Lines 1307-1339
- **Pattern 3**: Lines 1341-1390
- **Pattern 4**: Lines 1392-1442
- **Pattern 5**: Lines 1444-1460
- **Pattern 6**: Lines 1462-1522

### Free Eagle Move Execution
- **Path Reconstruction**: `src/game_state.rs:818-837`
- **Path Execution**: `src/game_state.rs:899-975`
- **Special Handling**: `src/game_state.rs:560-562` (in `generate_legal_moves_for_pieces`)

### Move Notation
- **Main Loop**: `src/main.rs:180-245`
- **Game Replay**: `src/game_history.rs:297-494`
- **Helper Functions**: `src/main.rs:19-70` (Japanese numeral conversion, file/rank flipping)

### Game Viewing
- **Entry Point**: `src/main.rs:80-85`
- **Main Function**: `GameHistory::format_game()` - `src/game_history.rs:298-494`
- **Move Application**: `src/game_history.rs:455-491`

---

## Testing Recommendations

1. **Test Pattern 3** (3 forward + 1 back):
   - Set up Free Eagle with enemy piece 3 spaces forward diagonally
   - Verify move is generated
   - Execute move and verify path is correct
   - Check notation shows all steps

2. **Test Pattern 4** (2 forward + 1 back):
   - Set up Free Eagle with enemy piece 2 spaces away
   - Verify move is generated
   - Execute and verify path

3. **Test Path Reconstruction**:
   - Load a game with Free Eagle moves
   - Verify paths are reconstructed correctly
   - Check that moves execute properly

4. **Test Notation**:
   - Execute a Free Eagle multi-move
   - Verify notation shows all steps
   - Check move numbers increment correctly

---

## Estimated Work for Test Environment

Creating a 5x5 test board without win conditions would require:

1. **New Board Size** (Low effort):
   - Modify `Board::new()` to accept size parameter
   - Update position validation
   - ~50 lines of code

2. **Disable Win Conditions** (Low effort):
   - Add flag to `GameState` to disable win checking
   - Modify `has_lost()` and `get_winner()` to return None when disabled
   - ~20 lines of code

3. **Test Helper Functions** (Medium effort):
   - Function to place pieces easily
   - Function to print board state
   - Function to execute and display moves
   - ~100-150 lines of code

4. **Test Main Function** (Low effort):
   - Simple REPL or script to test moves
   - ~50-100 lines of code

**Total Estimate**: ~220-320 lines of code, 2-3 hours of work

**Alternative**: Create a separate test binary (`src/bin/test_free_eagle.rs`) that:
- Creates a 5x5 board
- Has helper functions for easy piece placement
- Allows interactive testing
- Shows board state after each move

This would be cleaner and wouldn't affect the main codebase.

