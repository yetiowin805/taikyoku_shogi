# Taikyoku Shogi Engine

A game engine for Taikyoku (Ultimate) Shogi, a large variant of Shogi played on a 36x36 board.

## Features

- Game engine with legal move generation
- King and Pawn pieces (more pieces to be added)
- Promotion rules (pieces promote when entering opponent's 11th rank)
- Random player for testing
- Game history saving and viewing
- UCI protocol interface

## Building

```bash
cargo build
```

## Running

### UCI Interface
```bash
cargo run
```
This starts the UCI protocol interface for use with chess GUIs or AI players.

### Play Random Game
```bash
cargo run -- play
```
Plays a random game between two random players and saves it to the `games/` directory.

### List Saved Games
```bash
cargo run -- list
```
Lists all saved game files.

### View a Game
```bash
cargo run -- view games/game_1234567890.json
```
Displays the move history and result of a saved game.

## Testing

```bash
cargo test
```

## Current Implementation

- **Board**: 36x36 board representation
- **Pieces**: King (moves up to 2 squares in any direction) and Pawn (standard shogi movement)
- **Promotion**: Pawns promote when entering opponent's 11th rank (rank 26 for Black, rank 11 for White when displayed as 1-indexed)
- **Game History**: Games are saved as JSON files in the `games/` directory

## Initial Setup

- **Kings**: File 17 (17th from left, displayed as 17), on back ranks (rank 1 for Black, rank 36 for White when displayed)
- **Pawns**: Rank 11 for Black, rank 26 for White (when displayed), all files (1-36 when displayed)

Note: Coordinates are displayed as 1-indexed (1-36) for human readability, but stored internally as 0-indexed (0-35).

