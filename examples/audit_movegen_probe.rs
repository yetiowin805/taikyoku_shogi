//! Temporary first-pass, non-search movement audit. Run with --release.
use std::hint::black_box;
use std::time::Instant;
use taikyoku_shogi::attack_utils;
use taikyoku_shogi::board::Board;
use taikyoku_shogi::eval::ALL_PIECE_TYPES;
use taikyoku_shogi::movement::{MovementConfig, MovementGenerator};
use taikyoku_shogi::piece::{Color, Piece, PieceType};
use taikyoku_shogi::position::Position;
use taikyoku_shogi::tengu_attack;

fn pos(index: usize) -> Position {
    Position::from_index(index).unwrap()
}

fn random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn board_for(seed: u64, permille: u64) -> Board {
    let mut board = Board::new();
    let mut state = seed;
    for index in 0..1296 {
        if random(&mut state) % 1000 < permille {
            let kind = ALL_PIECE_TYPES[random(&mut state) as usize % ALL_PIECE_TYPES.len()];
            let color = if random(&mut state) & 1 == 0 {
                Color::Black
            } else {
                Color::White
            };
            board.place_piece(Piece::new(kind, color, pos(index)));
        }
    }
    board
}

fn special(piece: &Piece, target: Position, board: &Board) -> bool {
    if target == piece.position {
        return false;
    }
    if attack_utils::is_tengu_or_promoted_peacock(piece) {
        tengu_attack::can_tengu_attack_target(piece, target, board)
    } else if attack_utils::is_unpromoted_peacock(piece) {
        tengu_attack::can_peacock_attack_target(piece, target, board)
    } else if attack_utils::is_hook_mover_like_piece(piece) {
        tengu_attack::can_hook_mover_attack_target(piece, target, board)
    } else if attack_utils::is_lion_hawk(piece) {
        tengu_attack::can_lion_hawk_attack_target(piece, target, board)
    } else if attack_utils::is_cannon_soldier(piece) {
        tengu_attack::can_cannon_soldier_attack_target(piece, target, board)
    } else {
        piece.can_reach(target, board)
    }
}

fn parity() {
    let mut directed_checks = 0u64;
    let mut filter_checks = 0u64;
    let mut special_checks = 0u64;
    let mut failures = Vec::new();
    for (board_id, (seed, density, start)) in [
        (91671, 0, 666),
        (3612, 100, 0),
        (18273, 400, 1295),
        (55171, 100, 648),
    ]
    .into_iter()
    .enumerate()
    {
        let mut board = board_for(seed, density);
        for &original in ALL_PIECE_TYPES {
            for promoted in [false, true] {
                for color in [Color::Black, Color::White] {
                    let mut piece = Piece::new(original, color, pos(start));
                    if promoted {
                        piece.promote();
                    }
                    board.place_piece(piece);
                    let caps = &MovementConfig::for_piece(&piece).capabilities;
                    let generated = MovementGenerator::generate_targets(&piece, &board, caps);
                    let mut hits = [false; 1296];
                    for &target in &generated {
                        hits[target.to_index()] = target != piece.position;
                        if target != piece.position {
                            filter_checks += 1;
                            if !attack_utils::should_check_piece_for_target_position(
                                &piece, target, false,
                            ) {
                                failures.push(format!("filter board={board_id} {original:?} promoted={promoted} {color:?} -> {target:?}"));
                            }
                        }
                    }
                    let specialized = attack_utils::is_tengu_or_promoted_peacock(&piece)
                        || attack_utils::is_unpromoted_peacock(&piece)
                        || attack_utils::is_hook_mover_like_piece(&piece)
                        || attack_utils::is_lion_hawk(&piece)
                        || attack_utils::is_cannon_soldier(&piece);
                    // All destinations for specialized probes; 32 deterministic
                    // destinations plus all reached targets for generic probes.
                    for (index, &expected) in hits.iter().enumerate() {
                        if !specialized
                            && !expected
                            && !(index * 173 + original as usize * 31).is_multiple_of(41)
                        {
                            continue;
                        }
                        let target = pos(index);
                        let direct = piece.can_reach(target, &board);
                        directed_checks += 1;
                        if direct != expected {
                            failures.push(format!("direct board={board_id} {original:?} promoted={promoted} {color:?} -> {target:?} expected={expected}"));
                        }
                        if specialized {
                            special_checks += 1;
                            if special(&piece, target, &board) != expected {
                                failures.push(format!("special board={board_id} {original:?} now={:?} promoted={promoted} {color:?} -> {target:?} expected={expected}", piece.piece_type));
                            }
                        }
                    }
                }
            }
        }
    }
    println!("PARITY directed={directed_checks} reached_filter={filter_checks} special={special_checks} failures={}", failures.len());
    for failure in failures.iter().take(30) {
        println!("FAIL {failure}");
    }
    assert!(failures.is_empty(), "movement entrypoints disagree");
}

fn dedup_sorted(raw: &[Position]) -> Vec<Position> {
    let mut targets = raw.to_vec();
    targets.sort();
    targets.dedup();
    targets
}

fn dedup_unstable(raw: &[Position]) -> Vec<Position> {
    let mut targets = raw.to_vec();
    targets.sort_unstable();
    targets.dedup();
    targets
}

fn dedup_bitmap(raw: &[Position]) -> Vec<Position> {
    let mut bits = [0u64; 21];
    for &target in raw {
        // Position ordering is file-major; preserve it exactly.
        let index = target.file() as usize * 36 + target.rank() as usize;
        bits[index / 64] |= 1u64 << (index % 64);
    }
    let mut targets = Vec::with_capacity(raw.len().min(1296));
    for (word_index, &word) in bits.iter().enumerate() {
        let mut word = word;
        while word != 0 {
            let index = word_index * 64 + word.trailing_zeros() as usize;
            targets.push(Position::new((index / 36) as u8, (index % 36) as u8).unwrap());
            word &= word - 1;
        }
    }
    targets
}

fn micros() {
    for (name, density) in [("empty", 0), ("sparse", 100), ("dense", 400)] {
        let board = board_for(55171, density);
        for kind in [
            PieceType::Pawn,
            PieceType::Rook,
            PieceType::Lion,
            PieceType::Tengu,
            PieceType::HookMover,
            PieceType::FreeEagle,
        ] {
            let piece = Piece::new(kind, Color::Black, pos(666));
            let caps = &MovementConfig::for_piece(&piece).capabilities;
            let raw: Vec<_> = caps
                .iter()
                .flat_map(|cap| MovementGenerator::capability_landings(&piece, &board, cap))
                .collect();
            assert_eq!(dedup_sorted(&raw), dedup_bitmap(&raw));
            assert_eq!(dedup_sorted(&raw), dedup_unstable(&raw));
            let iterations = 20000;
            for repetition in 0..5 {
                let run = |mode| {
                    let start = Instant::now();
                    let mut count = 0;
                    for _ in 0..iterations {
                        count += match mode {
                            0 => black_box(dedup_sorted(black_box(&raw))).len(),
                            1 => black_box(dedup_unstable(black_box(&raw))).len(),
                            _ => black_box(dedup_bitmap(black_box(&raw))).len(),
                        };
                    }
                    black_box(count);
                    start.elapsed().as_nanos() as f64 / iterations as f64
                };
                let (sort_ns, unstable_ns, bitmap_ns) = if repetition % 2 == 0 {
                    let a = run(0);
                    let b = run(1);
                    (a, b, run(2))
                } else {
                    let c = run(2);
                    let b = run(1);
                    (run(0), b, c)
                };
                println!("DEDUP {name} {kind:?} rep={repetition} raw={} unique={} sort_ns={sort_ns:.2} unstable_ns={unstable_ns:.2} bitmap_ns={bitmap_ns:.2}", raw.len(), dedup_sorted(&raw).len());
            }
        }
    }
    for kind in [
        PieceType::Tengu,
        PieceType::HookMover,
        PieceType::Peacock,
        PieceType::LionHawk,
    ] {
        for (name, density) in [("empty", 0), ("sparse", 100), ("dense", 400)] {
            let mut board = board_for(55171, density);
            let piece = Piece::new(kind, Color::Black, pos(666));
            board.place_piece(piece);
            for repetition in 0..5 {
                let run = |optimized| {
                    let start = Instant::now();
                    let mut hits = 0;
                    for cycle in 0..8 {
                        for index in 0..1296 {
                            let target = pos((index * 173 + cycle * 31) % 1296);
                            hits += if optimized {
                                special(black_box(&piece), black_box(target), black_box(&board))
                            } else {
                                black_box(&piece).can_reach(black_box(target), black_box(&board))
                            } as usize;
                        }
                    }
                    black_box(hits);
                    start.elapsed().as_nanos() as f64 / (8.0 * 1296.0)
                };
                let (generic_ns, special_ns) = if repetition % 2 == 0 {
                    let a = run(false);
                    (a, run(true))
                } else {
                    let b = run(true);
                    (run(false), b)
                };
                println!("REACH {name} {kind:?} rep={repetition} generic_ns={generic_ns:.2} special_ns={special_ns:.2}");
            }
        }
    }
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("parity") => parity(),
        Some("micros") => micros(),
        _ => {
            parity();
            micros();
        }
    }
}
