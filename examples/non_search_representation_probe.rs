//! Short representation kernels; these are subsystem measurements, not search gains.
use std::{hint::black_box, mem::size_of, time::Instant};
use taikyoku_shogi::{
    board::Board,
    game_state::{GameState, Move, MoveData},
    move_simulation::MoveDelta,
    movement::MovementConfig,
    piece::{Color, Piece, PieceType},
    position::Position,
};
const N: usize = 1296;
const DIR: [(i8, i8); 8] = [
    (0, 1),
    (1, 1),
    (1, 0),
    (1, -1),
    (0, -1),
    (-1, -1),
    (-1, 0),
    (-1, 1),
];
fn measure(name: &str, mut f: impl FnMut() -> u64) {
    let mut times = Vec::new();
    let expected = f();
    for _ in 0..7 {
        let start = Instant::now();
        let mut sum = 0;
        for _ in 0..128 {
            sum += black_box(f());
        }
        let ns = start.elapsed().as_nanos() as f64 / 128.;
        assert_eq!(sum, expected * 128);
        times.push(ns);
    }
    times.sort_by(f64::total_cmp);
    println!(
        "{{\"name\":\"{name}\",\"ns\":{},\"min_ns\":{},\"max_ns\":{},\"checksum\":{expected}}}",
        times[3], times[0], times[6]
    );
}
// Layout-only alternative. This mirrors every descriptor variant, keeping all
// offsets and nested descriptors; only the Range blocker payload moves to heap.
enum CompactCapability {
    Simple(u8, u8),
    Range(
        u8,
        taikyoku_shogi::movement::types::BlockingMode,
        Box<taikyoku_shogi::movement::types::PieceTypeSet>,
    ),
    Jumping(Vec<(i8, i8)>),
    TwoStep(Box<Self>, Box<Self>),
    Conditional(u8, u8, Vec<u8>),
    FreeEagle(u8, u8),
}
impl CompactCapability {
    fn from_cap(cap: &taikyoku_shogi::movement::MovementCapability) -> Self {
        use taikyoku_shogi::movement::MovementCapability as C;
        match cap {
            C::Simple {
                directions,
                max_distance,
            } => Self::Simple(*directions, *max_distance),
            C::Range {
                directions,
                blocking,
                cannot_jump_over,
            } => Self::Range(*directions, *blocking, Box::new(*cannot_jump_over)),
            C::Jumping { offsets } => Self::Jumping(offsets.clone()),
            C::TwoStep { first, second } => Self::TwoStep(
                Box::new(Self::from_cap(first)),
                Box::new(Self::from_cap(second)),
            ),
            C::ConditionalDiagonalJump {
                directions,
                base_jump,
                conditional_jumps,
            } => Self::Conditional(*directions, *base_jump, conditional_jumps.clone()),
            C::FreeEagleMultiMove {
                max_distance_forward_diagonal,
                max_distance_other,
            } => Self::FreeEagle(*max_distance_forward_diagonal, *max_distance_other),
        }
    }
    fn checksum(&self) -> u64 {
        match self {
            Self::Simple(d, m) => 1 + u64::from(*d) + u64::from(*m),
            Self::Range(d, b, s) => {
                2 + u64::from(*d) + *b as u64 + u64::from(s.contains(&PieceType::King))
            }
            Self::Jumping(o) => {
                3 + o
                    .iter()
                    .map(|(x, y)| (*x as i64 + *y as i64 + 256) as u64)
                    .sum::<u64>()
            }
            Self::TwoStep(a, b) => 4 + a.checksum() + b.checksum(),
            Self::Conditional(d, b, j) => {
                5 + u64::from(*d) + u64::from(*b) + j.iter().map(|&j| u64::from(j)).sum::<u64>()
            }
            Self::FreeEagle(a, b) => 6 + u64::from(*a) + u64::from(*b),
        }
    }
}
fn capability_checksum(cap: &taikyoku_shogi::movement::MovementCapability) -> u64 {
    use taikyoku_shogi::movement::MovementCapability as C;
    match cap {
        C::Simple {
            directions,
            max_distance,
        } => 1 + u64::from(*directions) + u64::from(*max_distance),
        C::Range {
            directions,
            blocking,
            cannot_jump_over,
        } => {
            2 + u64::from(*directions)
                + *blocking as u64
                + u64::from(cannot_jump_over.contains(&PieceType::King))
        }
        C::Jumping { offsets } => {
            3 + offsets
                .iter()
                .map(|(x, y)| (*x as i64 + *y as i64 + 256) as u64)
                .sum::<u64>()
        }
        C::TwoStep { first, second } => {
            4 + capability_checksum(first) + capability_checksum(second)
        }
        C::ConditionalDiagonalJump {
            directions,
            base_jump,
            conditional_jumps,
        } => {
            5 + u64::from(*directions)
                + u64::from(*base_jump)
                + conditional_jumps.iter().map(|&j| u64::from(j)).sum::<u64>()
        }
        C::FreeEagleMultiMove {
            max_distance_forward_diagonal,
            max_distance_other,
        } => 6 + u64::from(*max_distance_forward_diagonal) + u64::from(*max_distance_other),
    }
}

fn ordinary(board: &Board) -> u64 {
    let mut n = 0;
    for i in 0..N {
        let from = Position::from_index(i).unwrap();
        for &(df, dr) in &DIR {
            let mut p = from;
            while let Some(q) = p.offset(df, dr) {
                n += 1;
                if board.get_piece(q).is_some() {
                    break;
                }
                p = q;
            }
        }
    }
    n
}
fn main() {
    let mut state = GameState::new();
    if let Some(path) = std::env::args().nth(1) {
        let text = std::fs::read_to_string(path).unwrap();
        let pos = taikyoku_shogi::notation::tsfen_decode(text.trim()).unwrap();
        state = pos.to_state().unwrap();
    } else {
        state.setup_initial_position();
    }
    let board = state.get_board();
    let squares: Vec<_> = (0..N).map(|i| Position::from_index(i).unwrap()).collect();
    println!("{{\"layout\":{{\"Board\":{},\"Move\":{},\"MoveData\":{},\"MoveDelta\":{},\"Piece\":{},\"Position\":{},\"MovementCapability\":{}}}}}",size_of::<Board>(),size_of::<Move>(),size_of::<MoveData>(),size_of::<MoveDelta>(),size_of::<Piece>(),size_of::<Position>(),size_of::<taikyoku_shogi::movement::MovementCapability>());
    let mut slots = [u16::MAX; N];
    let armies = [
        board.pieces_by_color(Color::Black),
        board.pieces_by_color(Color::White),
    ];
    for (color, army) in armies.iter().enumerate() {
        for (slot, p) in army.iter().enumerate() {
            slots[p.position.to_index()] = (slot as u16) | ((color as u16) << 15);
        }
    }
    measure("R06_direct_square_reads", || {
        squares
            .iter()
            .map(|p| {
                black_box(board)
                    .get_piece(*p)
                    .map_or(0, |q| q.piece_type as u64 + 1)
            })
            .sum()
    });
    measure("R06_slot_only_reads", || {
        squares
            .iter()
            .map(|p| {
                let s = black_box(&slots)[p.to_index()];
                if s == u16::MAX {
                    0
                } else {
                    armies[(s >> 15) as usize][(s & 0x7fff) as usize].piece_type as u64 + 1
                }
            })
            .sum()
    });
    let indexes: Vec<_> = squares.iter().map(|p| p.to_index() as u16).collect();
    measure("R07_coordinate_offsets", || {
        squares
            .iter()
            .filter_map(|p| p.offset(1, -1))
            .map(|p| p.to_index() as u64)
            .sum()
    });
    measure("R07_linear_offsets", || {
        indexes
            .iter()
            .filter_map(|&i| {
                let x = i % 36;
                let y = i / 36;
                if x < 35 && y > 0 {
                    Some((i - 35) as u64)
                } else {
                    None
                }
            })
            .sum()
    });
    let pieces: Vec<_> = armies.iter().flat_map(|a| a.iter().copied()).collect();
    let positions: Vec<_> = pieces.iter().map(|p| p.position).collect();
    let types: Vec<_> = pieces.iter().map(|p| p.piece_type).collect();
    measure("R09_AoS_position_type", || {
        black_box(&pieces)
            .iter()
            .map(|p| p.position.to_index() as u64 + p.piece_type as u64)
            .sum()
    });
    measure("R09_SoA_position_type", || {
        black_box(&positions)
            .iter()
            .zip(black_box(&types))
            .map(|(p, t)| p.to_index() as u64 + *t as u64)
            .sum()
    });
    measure("R09_AoS_positions_only", || {
        black_box(&pieces)
            .iter()
            .map(|p| p.position.to_index() as u64)
            .sum()
    });
    measure("R09_SoA_positions_only", || {
        black_box(&positions)
            .iter()
            .map(|p| p.to_index() as u64)
            .sum()
    });
    let mut next = vec![[u16::MAX; 8]; N];
    let mut rays: Vec<[Vec<u16>; 8]> = (0..N)
        .map(|_| std::array::from_fn(|_| Vec::new()))
        .collect();
    for i in 0..N {
        for (d, &(df, dr)) in DIR.iter().enumerate() {
            let mut p = squares[i];
            next[i][d] = p.offset(df, dr).map_or(u16::MAX, |q| q.to_index() as u16);
            while let Some(q) = p.offset(df, dr) {
                rays[i][d].push(q.to_index() as u16);
                p = q;
            }
        }
    }
    let expected = ordinary(board);
    measure("R10_arithmetic_rays", || ordinary(black_box(board)));
    measure("R10_next_square_table", || {
        let mut n = 0;
        for first_steps in &next {
            for (d, &first) in first_steps.iter().enumerate() {
                let mut j = first;
                while j != u16::MAX {
                    n += 1;
                    if board.get_piece(squares[j as usize]).is_some() {
                        break;
                    }
                    j = next[j as usize][d];
                }
            }
        }
        assert_eq!(n, expected);
        n
    });
    measure("R10_full_ray_table", || {
        let mut n = 0;
        for dirs in &rays {
            for ray in dirs {
                for &j in ray {
                    n += 1;
                    if board.get_piece(squares[j as usize]).is_some() {
                        break;
                    }
                }
            }
        }
        assert_eq!(n, expected);
        n
    });
    let mut mailbox = [2u8; 38 * 38];
    for i in 0..N {
        mailbox[(i / 36 + 1) * 38 + i % 36 + 1] = u8::from(board.get_piece(squares[i]).is_some());
    }
    measure("R11_padded_mailbox", || {
        let mut n = 0;
        for i in 0..N {
            let from = ((i / 36 + 1) * 38 + i % 36 + 1) as isize;
            for &(df, dr) in &DIR {
                let delta = dr as isize * 38 + df as isize;
                let mut j = from + delta;
                loop {
                    let occupied = mailbox[j as usize];
                    if occupied == 2 {
                        break;
                    }
                    n += 1;
                    if occupied == 1 {
                        break;
                    }
                    j += delta;
                }
            }
        }
        assert_eq!(n, expected);
        n
    });
    let mut rows = [0u64; 36];
    let mut cols = [0u64; 36];
    for i in 0..N {
        if board.get_piece(squares[i]).is_some() {
            rows[i / 36] |= 1u64 << (i % 36);
            cols[i % 36] |= 1u64 << (i / 36);
        }
    }
    measure("R12_arithmetic_rook", || {
        let mut n = 0;
        for &p in &squares {
            for &(df, dr) in &[(0, 1), (0, -1), (1, 0), (-1, 0)] {
                let mut q = p;
                while let Some(r) = q.offset(df, dr) {
                    n += 1;
                    if board.get_piece(r).is_some() {
                        break;
                    }
                    q = r;
                }
            }
        }
        n
    });
    measure("R12_line_occupancy_rook", || {
        let mut n = 0;
        for i in 0..N {
            let x = i % 36;
            let y = i / 36;
            for (word, p) in [(rows[y], x), (cols[x], y)] {
                let low = word & ((1u64 << p) - 1);
                let high = word & (!0u64 << (p + 1));
                n += if low == 0 {
                    p
                } else {
                    p - (63 - low.leading_zeros() as usize)
                };
                n += if high == 0 {
                    35 - p
                } else {
                    high.trailing_zeros() as usize - p
                };
            }
        }
        n as u64
    });
    let configs: Vec<_> = taikyoku_shogi::eval::ALL_PIECE_TYPES
        .iter()
        .map(|&t| MovementConfig::for_piece_type(t))
        .collect();
    let compact: Vec<Vec<_>> = configs
        .iter()
        .map(|config| {
            config
                .capabilities
                .iter()
                .map(CompactCapability::from_cap)
                .collect()
        })
        .collect();
    println!(
        "{{\"R15_compact_descriptor_bytes\":{}}}",
        size_of::<CompactCapability>()
    );
    let wide_checksum: u64 = pieces
        .iter()
        .flat_map(|p| configs[p.piece_type as usize].capabilities.iter())
        .map(capability_checksum)
        .sum();
    let compact_checksum: u64 = pieces
        .iter()
        .flat_map(|p| compact[p.piece_type as usize].iter())
        .map(CompactCapability::checksum)
        .sum();
    assert_eq!(wide_checksum, compact_checksum);
    measure("R15_wide_config_walk", || {
        pieces
            .iter()
            .flat_map(|p| configs[p.piece_type as usize].capabilities.iter())
            .map(capability_checksum)
            .sum()
    });
    measure("R15_compact_config_walk", || {
        pieces
            .iter()
            .flat_map(|p| compact[p.piece_type as usize].iter())
            .map(CompactCapability::checksum)
            .sum()
    });
    measure("R16_warm_OnceLock_configs", || {
        pieces
            .iter()
            .map(|p| {
                black_box(MovementConfig::for_piece_type(p.piece_type))
                    .capabilities
                    .len() as u64
            })
            .sum()
    });
    measure("R16_flat_config_references", || {
        pieces
            .iter()
            .map(|p| black_box(configs[p.piece_type as usize]).capabilities.len() as u64)
            .sum()
    });
    let moves = state.generate_legal_moves();
    let packed: Vec<_> = moves
        .iter()
        .map(|m| {
            let mut v = (m.from.to_index() as u64)
                | ((m.to.to_index() as u64) << 11)
                | ((m.promoted as u64) << 22);
            if let Some(via) = m.intermediate() {
                v |= (via.to_index() as u64) << 23;
            }
            v
        })
        .collect();
    measure("R04_Move_list_clone", || {
        black_box(moves.clone()).len() as u64
    });
    measure("R04_packed_list_clone", || {
        black_box(packed.clone()).len() as u64
    });
    measure("R04_Move_endpoint_decode", || {
        black_box(&moves)
            .iter()
            .map(|m| m.from.to_index() as u64 + m.to.to_index() as u64)
            .sum()
    });
    measure("R04_packed_endpoint_decode", || {
        black_box(&packed)
            .iter()
            .map(|&m| (m & 2047) + ((m >> 11) & 2047))
            .sum()
    });
    let piece = Piece::new(PieceType::Pawn, Color::White, squares[10]);
    measure("R18_Vec_one_capture", || {
        let mut n = 0;
        for &square in &squares {
            let captures = vec![(square, piece)];
            n += black_box(captures).len();
        }
        n as u64
    });
    measure("R18_inline_one_capture", || {
        let mut n = 0;
        for &square in &squares {
            let captures = Some((square, piece));
            n += black_box(captures).is_some() as usize;
        }
        n as u64
    });
}
