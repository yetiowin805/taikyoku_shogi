//! Disposable audit kernels; run with `cargo run --release --example audit_representation_probe`.
//! Candidate representations are deliberately outside production code.
use std::hint::black_box;
use std::mem::size_of;
use std::time::Instant;
use taikyoku_shogi::board::Board;
use taikyoku_shogi::board_position::BoardPosition;
use taikyoku_shogi::eval::ALL_PIECE_TYPES;
use taikyoku_shogi::game_state::{GameState, Move, MoveData};
use taikyoku_shogi::movement::types::MovementCapability;
use taikyoku_shogi::piece::{Color, Piece, PieceType};
use taikyoku_shogi::position::Position;

#[derive(Clone)]
enum InlineData {
    Standard,
    TwoStep(Position),
    FreeEagle { path: [Position; 5], len: u8 },
}

#[derive(Clone)]
struct InlineMove {
    from: Position,
    to: Position,
    promoted: bool,
    data: InlineData,
}

#[derive(Clone, Copy)]
enum CopyInlineData {
    Standard,
    TwoStep(Position),
    FreeEagle { path: [Position; 5], len: u8 },
}

#[derive(Clone, Copy)]
struct CopyInlineMove {
    from: Position,
    to: Position,
    promoted: bool,
    data: CopyInlineData,
}

impl CopyInlineMove {
    fn from_inline(mv: &InlineMove) -> Self {
        Self {
            from: mv.from,
            to: mv.to,
            promoted: mv.promoted,
            data: match &mv.data {
                InlineData::Standard => CopyInlineData::Standard,
                InlineData::TwoStep(via) => CopyInlineData::TwoStep(*via),
                InlineData::FreeEagle { path, len } => CopyInlineData::FreeEagle {
                    path: *path,
                    len: *len,
                },
            },
        }
    }

    fn to_move(self) -> Move {
        Move {
            from: self.from,
            to: self.to,
            promoted: self.promoted,
            data: match self.data {
                CopyInlineData::Standard => MoveData::Standard,
                CopyInlineData::TwoStep(intermediate) => MoveData::TwoStep { intermediate },
                CopyInlineData::FreeEagle { path, len } => MoveData::FreeEagle {
                    path: path[..len as usize].to_vec(),
                },
            },
        }
    }
}

impl InlineMove {
    fn from_move(mv: &Move) -> Self {
        let data = match &mv.data {
            MoveData::Standard => InlineData::Standard,
            MoveData::TwoStep { intermediate } => InlineData::TwoStep(*intermediate),
            MoveData::FreeEagle { path } => {
                assert!(path.len() <= 5);
                let mut buf = [mv.from; 5];
                buf[..path.len()].copy_from_slice(path);
                InlineData::FreeEagle {
                    path: buf,
                    len: path.len() as u8,
                }
            }
        };
        Self {
            from: mv.from,
            to: mv.to,
            promoted: mv.promoted,
            data,
        }
    }

    fn to_move(&self) -> Move {
        Move {
            from: self.from,
            to: self.to,
            promoted: self.promoted,
            data: match &self.data {
                InlineData::Standard => MoveData::Standard,
                InlineData::TwoStep(intermediate) => MoveData::TwoStep {
                    intermediate: *intermediate,
                },
                InlineData::FreeEagle { path, len } => MoveData::FreeEagle {
                    path: path[..*len as usize].to_vec(),
                },
            },
        }
    }
}

#[derive(Clone, Copy)]
struct PackedMove(u64);

impl PackedMove {
    fn from_move(mv: &Move) -> Self {
        let mut bits = mv.from.to_index() as u64 | ((mv.promoted as u64) << 60);
        match &mv.data {
            MoveData::Standard => bits |= (mv.to.to_index() as u64) << 11,
            MoveData::TwoStep { intermediate } => {
                bits |= (intermediate.to_index() as u64) << 11;
                bits |= (mv.to.to_index() as u64) << 22;
                bits |= 1 << 55;
            }
            MoveData::FreeEagle { path } => {
                assert!((2..=5).contains(&path.len()));
                assert_eq!(path.first(), Some(&mv.from));
                assert_eq!(path.last(), Some(&mv.to));
                for (i, pos) in path[1..].iter().enumerate() {
                    bits |= (pos.to_index() as u64) << (11 * (i + 1));
                }
                bits |= 2 << 55;
                bits |= (path.len() as u64) << 57;
            }
        }
        Self(bits)
    }

    fn square(self, n: usize) -> Position {
        Position::from_index(((self.0 >> (11 * n)) & 2047) as usize).unwrap()
    }

    fn to_move(self) -> Move {
        let (to, data) = match (self.0 >> 55) & 3 {
            0 => (self.square(1), MoveData::Standard),
            1 => (
                self.square(2),
                MoveData::TwoStep {
                    intermediate: self.square(1),
                },
            ),
            2 => {
                let len = ((self.0 >> 57) & 7) as usize;
                let path: Vec<_> = (0..len).map(|n| self.square(n)).collect();
                (self.square(len - 1), MoveData::FreeEagle { path })
            }
            _ => unreachable!(),
        };
        Move {
            from: self.square(0),
            to,
            promoted: self.0 & (1 << 60) != 0,
            data,
        }
    }
}

fn encode_piece(p: Piece) -> u32 {
    let base = p.base_piece_type.map_or(511, |pt| pt as u32);
    p.piece_type as u32
        | ((p.color == Color::White) as u32) << 9
        | (p.is_promoted as u32) << 10
        | base << 11
        | (p.position.to_index() as u32) << 20
}

fn decode_piece(bits: u32) -> Piece {
    let base = (bits >> 11) & 511;
    Piece {
        piece_type: ALL_PIECE_TYPES[(bits & 511) as usize],
        color: if bits & 512 != 0 {
            Color::White
        } else {
            Color::Black
        },
        is_promoted: bits & 1024 != 0,
        base_piece_type: if base == 511 {
            None
        } else {
            Some(ALL_PIECE_TYPES[base as usize])
        },
        position: Position::from_index((bits >> 20) as usize).unwrap(),
    }
}

fn piece_sum(p: Option<Piece>) -> u64 {
    p.map_or(0, |p| {
        1 + p.piece_type as u64
            + ((p.color == Color::White) as u64) * 313
            + p.position.to_index() as u64 * 619
            + p.is_promoted as u64 * 887
            + p.base_piece_type.map_or(511, |base| base as u64) * 997
    })
}

fn insufficient_borrowed(board: &Board) -> bool {
    let black = board.pieces_by_color(Color::Black);
    let white = board.pieces_by_color(Color::White);
    !black.is_empty()
        && !white.is_empty()
        && black.iter().chain(white).all(|p| {
            matches!(
                p.piece_type,
                PieceType::King
                    | PieceType::CrownPrince
                    | PieceType::GreatGeneral
                    | PieceType::DrunkenElephant
                    | PieceType::FreeKing
            )
        })
}

fn snapshot_borrowed(state: &GameState) -> BoardPosition {
    let board = state.get_board();
    let black = board.pieces_by_color(Color::Black);
    let white = board.pieces_by_color(Color::White);
    let mut pieces = Vec::with_capacity(black.len() + white.len());
    pieces.extend_from_slice(black);
    pieces.extend_from_slice(white);
    BoardPosition {
        pieces,
        turn: state.get_current_turn(),
        draw_counter: state.get_turns_without_capture_or_promotion(),
    }
}

fn timed(mut f: impl FnMut() -> u64, n: usize) -> f64 {
    let start = Instant::now();
    let mut sum = 0u64;
    for _ in 0..n {
        sum = sum.wrapping_add(black_box(f()));
    }
    black_box(sum);
    start.elapsed().as_nanos() as f64 / n as f64
}

fn pair(label: &str, n: usize, mut base: impl FnMut() -> u64, mut candidate: impl FnMut() -> u64) {
    assert_eq!(base(), candidate());
    for _ in 0..100 {
        black_box(base());
        black_box(candidate());
    }
    for rep in 0..9 {
        let (b, c) = if rep % 2 == 0 {
            (timed(&mut base, n), timed(&mut candidate, n))
        } else {
            let c = timed(&mut candidate, n);
            (timed(&mut base, n), c)
        };
        println!(
            "{label},rep={rep},n={n},baseline_ns={b:.3},candidate_ns={c:.3},ratio={:.4}",
            c / b
        );
    }
}

fn clone_pair<T: Clone>(label: &str, original: &[Move], compact: &[T], n: usize) {
    pair(
        label,
        n,
        || {
            let copy = black_box(original).to_vec();
            black_box(&copy);
            copy.len() as u64
        },
        || {
            let copy = black_box(compact).to_vec();
            black_box(&copy);
            copy.len() as u64
        },
    );
}

fn main() {
    println!("size Position={} Option<Position>={} PieceType={} Option<PieceType>={} Piece={} Option<Piece>={} MoveData={} Move={} InlineMove={} CopyInlineMove={} PackedMove={} Board={} MovementCapability={}",
        size_of::<Position>(), size_of::<Option<Position>>(), size_of::<PieceType>(),
        size_of::<Option<PieceType>>(), size_of::<Piece>(), size_of::<Option<Piece>>(),
        size_of::<MoveData>(), size_of::<Move>(), size_of::<InlineMove>(), size_of::<CopyInlineMove>(),
        size_of::<PackedMove>(), size_of::<Board>(), size_of::<MovementCapability>());
    let mut opening = GameState::new();
    opening.setup_initial_position();
    let mut rich = GameState::new();
    let fe = Piece::new(
        PieceType::FreeEagle,
        Color::Black,
        Position::new(18, 18).unwrap(),
    );
    rich.place_piece(fe);
    for (file, rank) in [(19, 19), (21, 21), (18, 20), (16, 16), (15, 18)] {
        rich.place_piece(Piece::new(
            PieceType::Pawn,
            Color::White,
            Position::new(file, rank).unwrap(),
        ));
    }
    let opening_moves = opening.generate_legal_moves();
    let rich_moves = rich.generate_free_eagle_moves(&fe);
    println!("workload opening_black={} opening_white={} opening_moves={} rich_moves={} rich_paths={} rich_max_path={}",
        opening.get_board().pieces_by_color(Color::Black).len(),
        opening.get_board().pieces_by_color(Color::White).len(), opening_moves.len(), rich_moves.len(),
        rich_moves.iter().filter(|m| m.is_free_eagle()).count(),
        rich_moves.iter().filter_map(|m| m.free_eagle_path().map(Vec::len)).max().unwrap());
    for (name, moves) in [("opening", &opening_moves), ("free_eagle", &rich_moves)] {
        let inline: Vec<_> = moves.iter().map(InlineMove::from_move).collect();
        let copy_inline: Vec<_> = inline.iter().map(CopyInlineMove::from_inline).collect();
        let packed: Vec<_> = moves.iter().map(PackedMove::from_move).collect();
        for ((original, inline), packed) in moves.iter().zip(&inline).zip(&packed) {
            assert_eq!(original, &inline.to_move());
            assert_eq!(original, &packed.to_move());
        }
        for (original, copy_inline) in moves.iter().zip(&copy_inline) {
            assert_eq!(original, &copy_inline.to_move());
        }
        clone_pair(&format!("{name}_inline_clone"), moves, &inline, 20_000);
        clone_pair(
            &format!("{name}_copy_inline_clone"),
            moves,
            &copy_inline,
            20_000,
        );
        clone_pair(&format!("{name}_packed_clone"), moves, &packed, 20_000);
    }
    for pt in ALL_PIECE_TYPES {
        for color in [Color::Black, Color::White] {
            for promoted in [false, true] {
                for base in [None, Some(*pt), Some(PieceType::ReverseChariot)] {
                    for index in [0, 35, 36, 647, 1295] {
                        let p = Piece {
                            piece_type: *pt,
                            color,
                            is_promoted: promoted,
                            base_piece_type: base,
                            position: Position::from_index(index).unwrap(),
                        };
                        assert_eq!(p, decode_piece(encode_piece(p)));
                    }
                }
            }
        }
    }
    let board = opening.get_board();
    let compact: Vec<_> = (0..1296)
        .map(|i| {
            board
                .get_piece(Position::from_index(i).unwrap())
                .map_or(u32::MAX, encode_piece)
        })
        .collect();
    let positions: Vec<_> = (0..4096)
        .map(|i| Position::from_index((i * 817 + 29) % 1296).unwrap())
        .collect();
    for pos in &positions {
        let raw = compact[pos.to_index()];
        assert_eq!(
            board.get_piece(*pos),
            (raw != u32::MAX).then(|| decode_piece(raw))
        );
    }
    pair(
        "packed_square_full_decode_4096_probes",
        3000,
        || {
            black_box(&positions)
                .iter()
                .map(|p| piece_sum(black_box(board).get_piece(*p)))
                .sum()
        },
        || {
            black_box(&positions)
                .iter()
                .map(|p| {
                    let raw = black_box(&compact)[p.to_index()];
                    piece_sum((raw != u32::MAX).then(|| decode_piece(raw)))
                })
                .sum()
        },
    );
    pair(
        "insufficient_material_opening",
        100_000,
        || black_box(&opening).is_draw_by_insufficient_material() as u64,
        || insufficient_borrowed(black_box(board)) as u64,
    );
    pair(
        "snapshot_opening",
        40_000,
        || {
            let value = BoardPosition::from_state(black_box(&opening));
            black_box(&value);
            value.pieces.len() as u64
        },
        || {
            let value = snapshot_borrowed(black_box(&opening));
            black_box(&value);
            value.pieces.len() as u64
        },
    );
    assert_eq!(
        BoardPosition::from_state(&opening),
        snapshot_borrowed(&opening)
    );
    for subset in [
        Vec::new(),
        vec![PieceType::King],
        vec![PieceType::Pawn, PieceType::King],
        vec![
            PieceType::DrunkenElephant,
            PieceType::CrownPrince,
            PieceType::GreatGeneral,
            PieceType::FreeKing,
        ],
    ] {
        for color_mask in 0..4 {
            let mut state = GameState::new();
            for (ci, color) in [Color::Black, Color::White].into_iter().enumerate() {
                if color_mask & (1 << ci) != 0 {
                    for (index, pt) in subset.iter().enumerate() {
                        state.place_piece(Piece::new(
                            *pt,
                            color,
                            Position::from_index(ci * 36 + index).unwrap(),
                        ));
                    }
                }
            }
            assert_eq!(
                state.is_draw_by_insufficient_material(),
                insufficient_borrowed(state.get_board())
            );
            assert_eq!(BoardPosition::from_state(&state), snapshot_borrowed(&state));
        }
    }
    println!("parity=PASS generated_move_roundtrips={} piece_roundtrips={} square_probes={} draw_snapshot_cases=16",
        opening_moves.len()+rich_moves.len(), ALL_PIECE_TYPES.len()*2*2*3*5,positions.len());
}
