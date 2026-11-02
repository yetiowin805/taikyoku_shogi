//! Isolated movement primitives; this is not an integrated engine benchmark.
//! Build with `rustc -O examples/speed_movement_spec.rs -o /tmp/movement-spec`.
//! Pin the executable to the same CPU as the engine screen; pass `geometry`
//! to run the full mixed-delta geometry pair, or `geometry-diagonal` for the
//! earlier diagonal-only diagnostic. Each JSON row is a batch mean.
use std::hint::black_box;
use std::time::{Duration, Instant};
const OFFSETS: [(i16, i16); 8] = [
    (0, 1),
    (1, 1),
    (1, 0),
    (1, -1),
    (0, -1),
    (-1, -1),
    (-1, 0),
    (-1, 1),
];
fn measure(name: &str, mut run: impl FnMut() -> u64) {
    let check = run();
    for rep in 0..5 {
        let start = Instant::now();
        let mut n = 0;
        while start.elapsed() < Duration::from_millis(12) {
            for _ in 0..64 {
                black_box(run());
            }
            n += 64;
        }
        println!(
            "{{\"case\":\"{name}\",\"rep\":{rep},\"ns\":{},\"checksum\":{check}}}",
            start.elapsed().as_nanos() as f64 / n as f64
        );
    }
}
#[inline(never)]
fn geometry(df: i16, dr: i16) -> u16 {
    let d = if df == 0 && dr == 0 {
        0
    } else if df == 0 {
        if dr > 0 {
            1
        } else {
            5
        }
    } else if dr == 0 {
        if df > 0 {
            3
        } else {
            7
        }
    } else if df.abs() == dr.abs() {
        match (df > 0, dr > 0) {
            (true, true) => 2,
            (true, false) => 4,
            (false, false) => 6,
            (false, true) => 8,
        }
    } else {
        0
    };
    if d == 0 {
        0
    } else {
        d | ((df.abs().max(dr.abs()) as u16) << 8)
    }
}
#[inline(never)]
fn table_geometry(table: &[u16; 5041], df: i16, dr: i16) -> u16 {
    table[((df + 35) * 71 + dr + 35) as usize]
}
#[inline(never)]
fn walk_runtime(board: &[u8; 1296], captures: bool) -> u64 {
    let mut out = 0;
    for (df, dr) in OFFSETS {
        for n in 1..36 {
            let f = 18 + df * n;
            let r = 18 + dr * n;
            if !(0..36).contains(&f) || !(0..36).contains(&r) {
                break;
            }
            let p = board[r as usize * 36 + f as usize];
            if p == 2 || (p == 0 && !captures) {
                out += 1;
            }
            if p != 0 {
                break;
            }
        }
    }
    out
}
#[inline(never)]
fn walk_const<const C: bool>(board: &[u8; 1296]) -> u64 {
    let mut out = 0;
    for (df, dr) in OFFSETS {
        for n in 1..36 {
            let f = 18 + df * n;
            let r = 18 + dr * n;
            if !(0..36).contains(&f) || !(0..36).contains(&r) {
                break;
            }
            let p = board[r as usize * 36 + f as usize];
            if p == 2 || (p == 0 && !C) {
                out += 1;
            }
            if p != 0 {
                break;
            }
        }
    }
    out
}
#[inline(never)]
fn simple_mask(board: &[u8; 1296], mut dirs: u8) -> u64 {
    let mut out = 0;
    while dirs != 0 {
        let i = dirs.trailing_zeros() as usize;
        dirs &= dirs - 1;
        let (df, dr) = OFFSETS[i];
        let square = (18 + dr) as usize * 36 + (18 + df) as usize;
        if board[square] != 1 {
            out += square as u64 + 1
        }
    }
    out
}
#[inline(never)]
fn simple_compiled(board: &[u8; 1296], offsets: &[(i16, i16)]) -> u64 {
    let mut out = 0;
    for &(df, dr) in offsets {
        let square = (18 + dr) as usize * 36 + (18 + df) as usize;
        if board[square] != 1 {
            out += square as u64 + 1
        }
    }
    out
}
fn main() {
    let mut table = [0u16; 5041];
    for df in -35..=35 {
        for dr in -35..=35 {
            table[((df + 35) * 71 + dr + 35) as usize] = geometry(df, dr)
        }
    }
    let mode = std::env::args().nth(1);
    let diagonal_diagnostic = mode.as_deref() == Some("geometry-diagonal");
    let count = if diagonal_diagnostic { 1000 } else { 5041 };
    let diffs: Vec<_> = (0..count)
        .map(|i| {
            if diagonal_diagnostic {
                (((i * 173) % 71 - 35) as i16, ((i * 31) % 71 - 35) as i16)
            } else {
                let index = (i * 173) % 5041;
                ((index % 71 - 35) as i16, (index / 71 - 35) as i16)
            }
        })
        .collect();
    let shape = if diagonal_diagnostic {
        "1000"
    } else {
        "5041-mixed"
    };
    measure(&format!("geometry-algebra-{shape}"), || {
        black_box(&diffs)
            .iter()
            .map(|&(df, dr)| geometry(black_box(df), black_box(dr)) as u64)
            .sum()
    });
    measure(&format!("geometry-table-{shape}"), || {
        black_box(&diffs)
            .iter()
            .map(|&(df, dr)| table_geometry(black_box(&table), black_box(df), black_box(dr)) as u64)
            .sum()
    });
    if matches!(mode.as_deref(), Some("geometry" | "geometry-diagonal")) {
        return;
    }
    for density in [0, 10, 40] {
        let mut board = [0u8; 1296];
        for (i, square) in board.iter_mut().enumerate() {
            if (i * 173 + 3612) % 100 < density {
                *square = if i % 2 == 0 { 1 } else { 2 }
            }
        }
        for cap in [false, true] {
            assert_eq!(
                walk_runtime(&board, cap),
                if cap {
                    walk_const::<true>(&board)
                } else {
                    walk_const::<false>(&board)
                }
            );
            measure(&format!("capture-runtime-{density}-{cap}"), || {
                walk_runtime(black_box(&board), black_box(cap))
            });
            measure(&format!("capture-const-{density}-{cap}"), || {
                if cap {
                    walk_const::<true>(black_box(&board))
                } else {
                    walk_const::<false>(black_box(&board))
                }
            });
        }
        for dirs in [1u8, 0x1f, 0xff] {
            let offsets: Vec<_> = OFFSETS
                .iter()
                .enumerate()
                .filter(|(i, _)| dirs & (1 << i) != 0)
                .map(|(_, p)| *p)
                .collect();
            assert_eq!(simple_mask(&board, dirs), simple_compiled(&board, &offsets));
            measure(&format!("signature-mask-{density}-{dirs}"), || {
                simple_mask(black_box(&board), black_box(dirs))
            });
            measure(&format!("signature-offsets-{density}-{dirs}"), || {
                simple_compiled(black_box(&board), black_box(&offsets))
            });
        }
    }
    for n in [1usize, 8, 16, 32, 64] {
        let values: Vec<u16> = (0..n).map(|i| ((i * 173) % 1296) as u16).collect();
        measure(&format!("union-sort-dedup-{n}"), || {
            let mut v = black_box(&values).clone();
            v.sort_unstable();
            v.dedup();
            black_box(v).len() as u64
        });
        measure(&format!("union-sort-only-{n}"), || {
            let mut v = black_box(&values).clone();
            v.sort_unstable();
            black_box(v).len() as u64
        });
        if n <= 16 {
            measure(&format!("scratch-heap-{n}"), || {
                let mut v = Vec::new();
                for &p in black_box(&values) {
                    v.push(p)
                }
                black_box(v).len() as u64
            });
            measure(&format!("scratch-inline16-{n}"), || {
                let mut v = [0u16; 16];
                let mut len = 0;
                for &p in black_box(&values) {
                    v[len] = p;
                    len += 1;
                }
                black_box(v);
                len as u64
            });
        }
    }
    for n in [2usize, 16, 36, 96] {
        let pieces: Vec<usize> = (0..n).map(|i| (i * 173 + 3) % 1260).collect();
        let mut mask = [0u64; 21];
        for &p in &pieces {
            mask[p / 64] |= 1u64 << (p % 64);
        }
        let mut friendly = [0u64; 21];
        for i in 0..1296 {
            if (i * 173 + 3612) % 100 < 10 {
                friendly[i / 64] |= 1u64 << (i % 64);
            }
        }
        let list = || {
            let mut out = 0;
            for &p in black_box(&pieces) {
                let to = p + 36;
                if friendly[to / 64] & (1u64 << (to % 64)) == 0 {
                    out += to as u64 + 1
                }
            }
            out
        };
        let batch = || {
            let mask = black_box(&mask);
            let friendly = black_box(&friendly);
            let mut out = 0;
            let mut carry = 0;
            for i in 0..21 {
                let next = mask[i] >> 28;
                let mut word = ((mask[i] << 36) | carry) & !friendly[i];
                if i == 20 {
                    word &= (1u64 << 16) - 1;
                }
                carry = next;
                while word != 0 {
                    let to = i * 64 + word.trailing_zeros() as usize;
                    out += to as u64 + 1;
                    word &= word - 1;
                }
            }
            out
        };
        assert_eq!(list(), batch());
        measure(&format!("pawn-list-{n}"), list);
        measure(&format!("pawn-batch21-{n}"), batch);
    }
}
