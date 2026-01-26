# Free Eagle Test Environment Usage

## Running the Test Environment

```bash
cargo run --bin test_free_eagle
```

## Commands

### `place <piece> <color> <file> <rank>`
Place a piece on the board.

**Examples:**
- `place FE B 2 2` - Place a Black Free Eagle at (2, 2)
- `place P W 3 3` - Place a White Pawn at (3, 3)

**Piece Types:**
- `FE` or `FREE_EAGLE` - Free Eagle
- `P` or `PAWN` - Pawn
- `K` or `KING` - King
- `R` or `ROOK` - Rook
- `B` or `BISHOP` - Bishop
- `Q` or `QUEEN` or `FREE_KING` - Free King

**Colors:**
- `B` or `BLACK` - Black
- `W` or `WHITE` - White

**Coordinates:**
- File and rank are 0-8 (9x9 board)
- File is column (left to right)
- Rank is row (bottom to top)

### `remove <file> <rank>`
Remove a piece from the board.

**Example:**
- `remove 2 2` - Remove piece at (2, 2)

### `move <from_file> <from_rank> <to_file> <to_rank>`
Execute a move.

**Example:**
- `move 2 2 3 3` - Move piece from (2, 2) to (3, 3)

### `moves <file> <rank>`
Show all legal moves for a piece at the given position.

**Example:**
- `moves 2 2` - Show all legal moves for piece at (2, 2)

### `show`
Display the current board state.

### `turn`
Show the current player's turn.

### `switch`
Switch the current turn (for testing).

### `reset`
Clear the board and reset to initial state.

### `help`
Show this help message.

### `quit` or `exit`
Exit the test environment.

## Example Session

```
> place FE B 4 4
Placed FE Black at (4, 4)

  0   1   2   3   4   5   6   7   8
8  .   .   .   .   .   .   .   .   .  
7  .   .   .   .   .   .   .   .   .  
6  .   .   .   .   .   .   .   .   .  
5  .   .   .   .   .   .   .   .   .  
4  .   .   .   .  FE  .   .   .   .  
3  .   .   .   .   .   .   .   .   .  
2  .   .   .   .   .   .   .   .   .  
1  .   .   .   .   .   .   .   .   .  
0  .   .   .   .   .   .   .   .   .  

> place P W 7 7
Placed P White at (7, 7)

> moves 4 4
Legal moves for piece at (4, 4):
  1. (4, 4) -> (7, 7) [Free Eagle path: [Position { file: 4, rank: 4 }, Position { file: 5, rank: 5 }, Position { file: 6, rank: 6 }, Position { file: 7, rank: 7 }]]
  ...

> move 4 4 7 7
Move executed: (4, 4) -> (7, 7)
[Board shows Free Eagle at (7, 7), Pawn captured]
```

## Notes

- The board is 9x9 (coordinates 0-8)
- Win conditions are disabled - you can test moves without worrying about check/checkmate
- The board uses the full 36x36 Position system internally, but you only use positions 0-8
- Free Eagle moves will show their paths when you use the `moves` command
- Turn management works normally - moves will switch turns automatically

## Testing Free Eagle Patterns

### Pattern 3 (3 forward + 1 back):
```
> place FE B 1 1
> place P W 4 4  # 3 spaces forward diagonally
> moves 1 1
# Look for a move that goes 3 forward then 1 back
```

### Pattern 4 (2 forward + 1 back):
```
> place FE B 1 1
> place P W 3 3  # 2 spaces away
> moves 1 1
# Look for a move that goes 2 forward then 1 back
```

### Pattern 5 (Stay in place, capture 1 away):
```
> place FE B 2 2
> place P W 3 2  # 1 space to the right
> moves 2 2
# Look for a move that stays at (2, 2) but captures
```

