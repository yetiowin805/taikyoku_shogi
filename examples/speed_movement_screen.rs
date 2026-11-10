//! Short deterministic movement screen. Pass optional JSON array of TSFEN strings.
use std::hint::black_box;
use std::time::{Duration, Instant};
use taikyoku_shogi::board::Board;
use taikyoku_shogi::eval::ALL_PIECE_TYPES;
use taikyoku_shogi::game_state::LegalMoveGen;
use taikyoku_shogi::movement::{MovementConfig, MovementGenerator};
use taikyoku_shogi::notation::tsfen_decode;
use taikyoku_shogi::piece::{Color, Piece, PieceType};
use taikyoku_shogi::position::Position;

fn random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}
fn board_for(density: u64) -> Board {
    let mut board = Board::new();
    let mut state = 55171;
    for index in 0..1296 {
        if random(&mut state) % 1000 < density {
            let kind = ALL_PIECE_TYPES[random(&mut state) as usize % ALL_PIECE_TYPES.len()];
            let color = if random(&mut state) & 1 == 0 {
                Color::Black
            } else {
                Color::White
            };
            board.place_piece(Piece::new(
                kind,
                color,
                Position::from_index(index).unwrap(),
            ));
        }
    }
    board
}
fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |a, &b| {
        (a ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}
fn timed(name: &str, count: usize, parity: u64, mut run: impl FnMut()) {
    run();
    for rep in 0..5 {
        let start = Instant::now();
        let mut iterations = 0;
        while start.elapsed() < Duration::from_millis(20) {
            for _ in 0..8 {
                run();
            }
            iterations += 8;
        }
        println!(
            "{}",
            serde_json::json!({"case":name,"rep":rep,"iterations":iterations,"ns":start.elapsed().as_nanos() as f64 / iterations as f64,"count":count,"checksum":parity})
        );
    }
}
fn main() {
    for density in [0, 100, 400] {
        for kind in [
            PieceType::Pawn,
            PieceType::Rook,
            PieceType::Tengu,
            PieceType::HookMover,
            PieceType::LionHawk,
            PieceType::FreeEagle,
            PieceType::WoodenDove,
        ] {
            let mut board = board_for(density);
            let piece = Piece::new(kind, Color::Black, Position::new(18, 18).unwrap());
            board.place_piece(piece);
            let config = MovementConfig::for_piece(&piece);
            for captures in [false, true] {
                let result = MovementGenerator::generate_targets_filtered(
                    &piece,
                    &board,
                    &config.capabilities,
                    captures,
                );
                timed(
                    &format!("targets-{kind:?}-{density}-{captures}"),
                    result.len(),
                    checksum(&serde_json::to_vec(&result).unwrap()),
                    || {
                        black_box(MovementGenerator::generate_targets_filtered(
                            black_box(&piece),
                            black_box(&board),
                            &config.capabilities,
                            black_box(captures),
                        ));
                    },
                );
            }
            let targets: Vec<_> = (0..1296)
                .step_by(31)
                .map(|i| Position::from_index(i).unwrap())
                .collect();
            let results: Vec<_> = targets
                .iter()
                .map(|&target| piece.can_reach(target, &board))
                .collect();
            timed(
                &format!("reach-{kind:?}-{density}"),
                results.iter().filter(|&&b| b).count(),
                checksum(&serde_json::to_vec(&results).unwrap()),
                || {
                    for &target in &targets {
                        black_box(piece.can_reach(black_box(target), black_box(&board)));
                    }
                },
            );
        }
    }
    let mut fixtures = vec![("opening".to_owned(), tsfen_decode("startpos").unwrap())];
    if let Some(path) = std::env::args().nth(1) {
        let input = std::fs::read_to_string(path).unwrap();
        let values: Vec<serde_json::Value> = if input.trim_start().starts_with('[') {
            serde_json::from_str(&input).unwrap()
        } else {
            input
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| serde_json::from_str(line).unwrap())
                .collect()
        };
        for (i, value) in values.iter().enumerate() {
            let fen = value
                .as_str()
                .or_else(|| value.get("tsfen").and_then(|v| v.as_str()))
                .unwrap();
            let name = value
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("corpus-{i}"));
            fixtures.push((name, tsfen_decode(fen).unwrap()));
        }
    }
    for (name, position) in fixtures {
        let state = position.to_state().unwrap();
        for mode in [
            LegalMoveGen::All,
            LegalMoveGen::CapturesOnly,
            LegalMoveGen::WithoutQuietMultiLeg,
            LegalMoveGen::QuietMultiLegOnly,
        ] {
            let mut buffer = state.generate_legal_moves_mode(mode);
            let parity = checksum(&serde_json::to_vec(&buffer).unwrap());
            let count = buffer.len();
            timed(&format!("legal-{name}-{mode:?}"), count, parity, || {
                state.generate_legal_moves_mode_into(mode, black_box(&mut buffer));
                black_box(buffer.len());
            });
        }
    }
}
