use std::hint::black_box;
use std::time::Instant;
use taikyoku_shogi::board::Board;
use taikyoku_shogi::eval::ALL_PIECE_TYPES;
use taikyoku_shogi::movement::{MovementCapability, MovementConfig, MovementGenerator};
use taikyoku_shogi::piece::{Color, Piece, PieceType};
use taikyoku_shogi::position::Position;

fn next(s: &mut u64) -> u64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    *s
}
fn target(index: usize) -> Position {
    Position::from_index(index).unwrap()
}
fn board(seed: u64, density: u64) -> Board {
    let mut board = Board::new();
    let mut state = seed;
    for i in 0..1296 {
        if next(&mut state) % 1000 < density {
            let kind = ALL_PIECE_TYPES[next(&mut state) as usize % ALL_PIECE_TYPES.len()];
            let color = if next(&mut state) & 1 == 0 {
                Color::Black
            } else {
                Color::White
            };
            board.place_piece(Piece::new(kind, color, target(i)));
        }
    }
    board
}
fn restricted(piece: &Piece, board: &Board) -> Vec<Position> {
    let config = MovementConfig::for_piece(piece);
    let mut out = Vec::new();
    for cap in &config.capabilities {
        match cap {
            MovementCapability::TwoStep { first, second }
                if matches!(first.as_ref(), MovementCapability::Range { .. })
                    && matches!(second.as_ref(), MovementCapability::Range { .. }) =>
            {
                let MovementCapability::Range {
                    directions,
                    blocking,
                    cannot_jump_over,
                } = second.as_ref()
                else {
                    panic!()
                };
                let firsts = MovementGenerator::capability_landings(piece, board, first);
                out.extend(firsts.iter().copied());
                for via in firsts {
                    if board.get_piece(via).is_some() {
                        continue;
                    }
                    let df = via.file() as i16 - piece.position.file() as i16;
                    let dr = via.rank() as i16 - piece.position.rank() as i16;
                    let perp = if df == 0 {
                        0x44
                    } else if dr == 0 {
                        0x11
                    } else if df * dr > 0 {
                        0x88
                    } else {
                        0x22
                    };
                    let second = MovementCapability::Range {
                        directions: directions & perp,
                        blocking: *blocking,
                        cannot_jump_over: *cannot_jump_over,
                    };
                    let mut at_via = *piece;
                    at_via.position = via;
                    out.extend(MovementGenerator::capability_landings(
                        &at_via, board, &second,
                    ));
                }
            }
            _ => out.extend(MovementGenerator::capability_landings(piece, board, cap)),
        }
    }
    out.sort();
    out.dedup();
    out
}
fn main() {
    let mut checks = 0;
    for seed in [55171, 18273, 3612, 91671] {
        for density in [0, 100, 400] {
            let mut board = board(seed, density);
            for index in [0, 35, 666, 1295] {
                for kind in [
                    PieceType::Tengu,
                    PieceType::HookMover,
                    PieceType::Peacock,
                    PieceType::Capricorn,
                    PieceType::OldKite,
                    PieceType::PoisonousSerpent,
                ] {
                    for color in [Color::Black, Color::White] {
                        for promoted in [false, true] {
                            let mut piece = Piece::new(kind, color, target(index));
                            if promoted {
                                piece.promote();
                            }
                            board.place_piece(piece);
                            assert_eq!(
                                piece.get_potential_targets(&board),
                                restricted(&piece, &board),
                                "{seed} {density} {index} {kind:?} {color:?}"
                            );
                            checks += 1;
                        }
                    }
                }
            }
        }
    }
    println!("TURN_PARITY cases={checks}");
    for density in [0, 100, 400] {
        for kind in [PieceType::Tengu, PieceType::HookMover, PieceType::Peacock] {
            let mut board = board(55171, density);
            let piece = Piece::new(kind, Color::Black, target(666));
            board.place_piece(piece);
            for rep in 0..5 {
                let run = |restricted_mode| {
                    let start = Instant::now();
                    for _ in 0..2000 {
                        black_box(if restricted_mode {
                            restricted(black_box(&piece), black_box(&board))
                        } else {
                            black_box(&piece).get_potential_targets(black_box(&board))
                        });
                    }
                    start.elapsed().as_nanos() as f64 / 2000.0
                };
                let (base, candidate) = if rep % 2 == 0 {
                    let a = run(false);
                    (a, run(true))
                } else {
                    let a = run(true);
                    (run(false), a)
                };
                println!("TURN density={density} {kind:?} rep={rep} base_ns={base:.2} candidate_ns={candidate:.2}");
            }
        }
    }
}
