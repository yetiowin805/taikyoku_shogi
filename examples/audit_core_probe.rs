//! First-pass non-search audit: line occupancy and API allocation census.
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use taikyoku_shogi::board::Board;
use taikyoku_shogi::debug_tool::DebugTool;
use taikyoku_shogi::game_state::{GameState, LegalMoveGen};
use taikyoku_shogi::move_simulation::{move_to_delta, simulate_move, BoardLike};
use taikyoku_shogi::piece::{Color, Piece, PieceType};
use taikyoku_shogi::position::Position;

struct Counter;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(new, Ordering::Relaxed);
        System.realloc(ptr, layout, new)
    }
}
#[global_allocator]
static COUNTER: Counter = Counter;

fn census(name: &str, f: impl FnOnce() -> usize) {
    ALLOCS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    let result = black_box(f());
    let allocations = ALLOCS.load(Ordering::Relaxed);
    let bytes = BYTES.load(Ordering::Relaxed);
    println!(
        "{}",
        serde_json::json!({"kind":"allocations","case":name,"alloc_or_realloc_calls":allocations,"requested_bytes":bytes,"result":result})
    );
}

fn random(s: &mut u64) -> u64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    *s
}

struct Lines {
    rows: [[u64; 36]; 2],
    cols: [[u64; 36]; 2],
}
impl Lines {
    fn new(board: &Board) -> Self {
        let mut result = Self {
            rows: [[0; 36]; 2],
            cols: [[0; 36]; 2],
        };
        for color in [Color::Black, Color::White] {
            let c = usize::from(color == Color::White);
            for p in board.pieces_by_color(color) {
                let f = p.position.file() as usize;
                let r = p.position.rank() as usize;
                result.rows[c][r] |= 1 << f;
                result.cols[c][f] |= 1 << r;
            }
        }
        result
    }
    fn rook(&self, from: Position, color: Color) -> u32 {
        let c = usize::from(color == Color::White);
        let f = from.file() as usize;
        let r = from.rank() as usize;
        line_targets(f, self.rows[0][r] | self.rows[1][r], self.rows[c][r]).count_ones()
            + line_targets(r, self.cols[0][f] | self.cols[1][f], self.cols[c][f]).count_ones()
    }
}

fn line_targets(i: usize, occupied: u64, friendly: u64) -> u64 {
    let before = (1u64 << i) - 1;
    let low_blockers = occupied & before;
    let low = if low_blockers == 0 {
        0
    } else {
        63 - low_blockers.leading_zeros() as usize
    };
    let after = ((1u64 << 36) - 1) & !((1u64 << (i + 1)) - 1);
    let high_blockers = occupied & after;
    let high = if high_blockers == 0 {
        35
    } else {
        high_blockers.trailing_zeros() as usize
    };
    ((before & !((1u64 << low) - 1)) | (after & ((1u64 << (high + 1)) - 1))) & !friendly
}

fn scalar_rook(board: &Board, from: Position, color: Color) -> u32 {
    let mut result = 0;
    for (df, dr) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
        let mut next = from.offset(df, dr);
        while let Some(square) = next {
            if let Some(p) = board.get_piece(square) {
                result += u32::from(p.color != color);
                break;
            }
            result += 1;
            next = square.offset(df, dr);
        }
    }
    result
}

fn main() {
    let mut opening = GameState::new();
    opening.setup_initial_position();
    let all = opening.generate_legal_moves();
    let mut buffer = all.clone();
    let debug = DebugTool::new();
    // Warm OnceLocks and borrowed/cached entry points before counting allocations.
    let _ = debug.snapshot();
    census("opening_all_moves", || opening.generate_legal_moves().len());
    census("opening_all_into_reused_buffer", || {
        opening.generate_legal_moves_mode_into(LegalMoveGen::All, &mut buffer);
        buffer.len()
    });
    census("opening_captures", || {
        opening
            .generate_legal_moves_mode(LegalMoveGen::CapturesOnly)
            .len()
    });
    census("opening_debug_snapshot", || debug.snapshot().pieces.len());
    println!(
        "{}",
        serde_json::json!({"kind":"sample_move", "move":all[0]})
    );
    census("opening_sample_make_unmake", || {
        let undo = opening.make_move_for_search(all[0].clone()).unwrap();
        opening.unmake_move_for_search(undo);
        1
    });
    let mv = &all[0];
    let piece = opening.get_board().get_piece(mv.from).unwrap();
    census("opening_simulate_sample", || {
        let virtual_board = simulate_move(opening.get_board(), mv, &piece);
        usize::from(virtual_board.get_piece(mv.to).is_some())
    });
    census("opening_simulate_sample_attack_query", || {
        let virtual_board = simulate_move(opening.get_board(), mv, &piece);
        usize::from(virtual_board.is_position_attacked_by_color(mv.to, Color::White))
    });
    let quiet = all
        .iter()
        .find(|mv| {
            !mv.is_two_step()
                && !mv.is_free_eagle()
                && !mv.promoted
                && move_to_delta(
                    opening.get_board(),
                    mv,
                    &opening.get_board().get_piece(mv.from).unwrap(),
                )
                .pieces_removed
                .is_empty()
        })
        .unwrap();
    let quiet_piece = opening.get_board().get_piece(quiet.from).unwrap();
    println!("{}", serde_json::json!({"kind":"quiet_move", "move":quiet}));
    census("opening_quiet_make_unmake", || {
        let undo = opening.make_move_for_search(quiet.clone()).unwrap();
        opening.unmake_move_for_search(undo);
        1
    });
    census("opening_simulate_quiet", || {
        let virtual_board = simulate_move(opening.get_board(), quiet, &quiet_piece);
        usize::from(virtual_board.get_piece(quiet.to).is_some())
    });
    census("opening_simulate_quiet_attack_query", || {
        let virtual_board = simulate_move(opening.get_board(), quiet, &quiet_piece);
        usize::from(virtual_board.is_position_attacked_by_color(quiet.to, Color::White))
    });
    if std::env::args().nth(1).as_deref() == Some("allocations") {
        return;
    }

    let mut cases = Vec::new();
    let mut rng = 0x1987a52u64;
    for density in [0, 30, 100, 400] {
        for _ in 0..3 {
            let mut board = Board::new();
            for i in 0..1296 {
                if random(&mut rng) % 1000 < density {
                    let color = if random(&mut rng) & 1 == 0 {
                        Color::Black
                    } else {
                        Color::White
                    };
                    board.place_piece(Piece::new(
                        PieceType::Pawn,
                        color,
                        Position::from_index(i).unwrap(),
                    ));
                }
            }
            let lines = Lines::new(&board);
            for i in 0..1296 {
                for color in [Color::Black, Color::White] {
                    let square = Position::from_index(i).unwrap();
                    assert_eq!(
                        scalar_rook(&board, square, color),
                        lines.rook(square, color)
                    );
                }
            }
            cases.push((density, board, lines));
        }
    }
    println!(
        "{}",
        serde_json::json!({"kind":"parity","rook_count_checks":12*1296*2,"line_index_bytes":std::mem::size_of::<Lines>(),"scope":"count-only ordinary rook kernel; index construction/mutation and Move emission excluded"})
    );
    for density in [0, 30, 100, 400] {
        for rep in 0..7 {
            for arm in if rep % 2 == 0 {
                ["scalar", "lines"]
            } else {
                ["lines", "scalar"]
            } {
                let now = Instant::now();
                let mut checksum = 0u64;
                let mut queries = 0;
                for _ in 0..100 {
                    for (_, board, lines) in cases.iter().filter(|(d, _, _)| *d == density) {
                        for i in (0..1296).step_by(7) {
                            let p = black_box(Position::from_index(i).unwrap());
                            checksum += u64::from(if arm == "scalar" {
                                scalar_rook(black_box(board), p, Color::Black)
                            } else {
                                black_box(lines).rook(p, Color::Black)
                            });
                            queries += 1;
                        }
                    }
                }
                println!(
                    "{}",
                    serde_json::json!({"kind":"rook_micro","density_permille":density,"repetition":rep,"arm":arm,"queries":queries,"ns_per_query":now.elapsed().as_nanos() as f64 / queries as f64,"checksum":black_box(checksum)})
                );
            }
        }
    }
}
