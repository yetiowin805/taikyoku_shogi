use std::{collections::HashSet, hint::black_box};
use movement::{config::MovementConfig, types::{MovementCapability as Cap, BlockingMode}};
use piece::{Piece, PieceType};

#[derive(Clone, Copy)]
struct Bits([u64; 5]);
impl Bits {
    fn new(set: &HashSet<PieceType>) -> Self {
        let mut bits = [0; 5];
        for &p in set { bits[p as usize / 64] |= 1 << (p as usize % 64); }
        Self(bits)
    }
    fn contains(&self, p: PieceType) -> bool {
        self.0[p as usize / 64] & (1 << (p as usize % 64)) != 0
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Flags { two: bool, capture: bool, only_capture: bool, dirs: u8 }
fn flags(c: &MovementConfig) -> Flags {
    let mut f = Flags { two: false, capture: false, only_capture: false, dirs: 0 };
    let mut other = false;
    for cap in &c.capabilities {
        match cap {
            Cap::TwoStep { .. } => f.two = true,
            Cap::Range { directions, blocking, .. } => {
                f.dirs |= *directions;
                if *blocking == BlockingMode::Capturing { f.capture = true; } else { other = true; }
            }
            _ => (),
        }
    }
    f.only_capture = f.capture && !other;
    f
}
fn sets<'a>(cap: &'a Cap, out: &mut Vec<&'a HashSet<PieceType>>) {
    match cap {
        Cap::Range { cannot_jump_over, .. } => out.push(cannot_jump_over),
        Cap::TwoStep { first, second } => { sets(first, out); sets(second, out); }
        _ => (),
    }
}
#[repr(C)]
struct Timespec { sec: i64, ns: i64 }
unsafe extern "C" { fn clock_gettime(clock: i32, t: *mut Timespec) -> i32; }
fn cpu_ns() -> u64 {
    let mut t = Timespec { sec: 0, ns: 0 };
    assert_eq!(unsafe { clock_gettime(2, &mut t) }, 0);
    t.sec as u64 * 1_000_000_000 + t.ns as u64
}
fn measure(f: impl Fn() -> usize) -> (u64, usize) {
    let start = cpu_ns();
    let checksum = black_box(f());
    (cpu_ns() - start, checksum)
}
fn pair(name: &str, operations: usize, before: impl Fn() -> usize, after: impl Fn() -> usize) {
    assert_eq!(before(), after());
    for rep in 0..7 {
        let (a, b) = if rep % 2 == 0 { (measure(&before), measure(&after)) }
                     else { let b = measure(&after); let a = measure(&before); (a,b) };
        assert_eq!(a.1, b.1);
        println!("{{\"case\":\"{name}\",\"rep\":{rep},\"operations\":{operations},\"baseline_cpu_ns\":{},\"candidate_cpu_ns\":{},\"checksum\":{}}}", a.0, b.0, a.1);
    }
}
fn main() {
    let mut pieces: Vec<_> = eval::ALL_PIECE_TYPES.iter().map(|&t| Piece {
        piece_type: t, is_promoted: false, base_piece_type: None }).collect();
    pieces.push(Piece { piece_type: PieceType::RainDragon, is_promoted: true, base_piece_type: Some(PieceType::EarthDragon) });
    pieces.push(Piece { piece_type: PieceType::Whale, is_promoted: true, base_piece_type: Some(PieceType::ReverseChariot) });
    let configs: Vec<_> = pieces.iter().map(MovementConfig::for_piece).collect();
    let metadata: Vec<_> = configs.iter().map(|c| flags(c)).collect();
    let mut blocker_sets = Vec::new();
    for c in &configs { for cap in &c.capabilities { sets(cap, &mut blocker_sets); } }
    let bits: Vec<_> = blocker_sets.iter().map(|s| Bits::new(s)).collect();
    for (set, bit) in blocker_sets.iter().zip(&bits) {
        for &t in eval::ALL_PIECE_TYPES { assert_eq!(set.contains(&t), bit.contains(t)); }
    }
    eprintln!("validated {} configs, {} range sets against {} piece types", configs.len(), blocker_sets.len(), eval::ALL_PIECE_TYPES.len());
    let mut seed = 20260926u64;
    let mut next = || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed as usize };
    const ROUNDS: usize = 128;
    const N: usize = 4096;
    for (name, empty, positive) in [("blockers-uniform", false, false), ("blockers-half-hits", false, true), ("blockers-empty", true, false)] {
        let ids: Vec<_> = blocker_sets.iter().enumerate().filter(|(_,s)| s.is_empty() == empty).map(|(i,_)|i).collect();
        let queries: Vec<_> = (0..N).map(|i| {
            let set = ids[next() % ids.len()];
            // Sort randomized HashSet iteration to freeze the query stream.
            let piece = if positive && i%2==0 {
                let mut members: Vec<_> = blocker_sets[set].iter().copied().collect();
                members.sort_by_key(|p|*p as usize);
                members[next() % members.len()]
            } else { eval::ALL_PIECE_TYPES[next() % eval::ALL_PIECE_TYPES.len()] };
            (set, piece)
        }).collect();
        pair(name, N*ROUNDS, || {
            let mut count = 0;
            for _ in 0..ROUNDS { for &(i,p) in black_box(&queries) { count += blocker_sets[i].contains(&p) as usize; } }
            count
        }, || {
            let mut count = 0;
            for _ in 0..ROUNDS { for &(i,p) in black_box(&queries) { count += bits[i].contains(p) as usize; } }
            count
        });
    }
    let queries: Vec<_> = (0..N).map(|_|next()%configs.len()).collect();
    pair("metadata-two-step", N*ROUNDS, || {
        let mut count=0;
        for _ in 0..ROUNDS { for &i in black_box(&queries) {
            count += configs[i].capabilities.iter().any(|c|matches!(c, Cap::TwoStep{..})) as usize;
        } } count
    }, || {
        let mut count=0;
        for _ in 0..ROUNDS { for &i in black_box(&queries) { count += metadata[i].two as usize; } } count
    });
    pair("metadata-capturing", N*ROUNDS, || {
        let mut count=0;
        for _ in 0..ROUNDS { for &i in black_box(&queries) {
            count += configs[i].capabilities.iter().any(|c|matches!(c, Cap::Range{blocking:BlockingMode::Capturing,..})) as usize;
        } } count
    }, || {
        let mut count=0;
        for _ in 0..ROUNDS { for &i in black_box(&queries) { count += metadata[i].capture as usize; } } count
    });
    pair("metadata-range-mask", N*ROUNDS, || {
        let mut count=0;
        for _ in 0..ROUNDS { for &i in black_box(&queries) {
            let mut dirs=0;
            for cap in &configs[i].capabilities { if let Cap::Range{directions,..}=cap { dirs |= *directions; } }
            count += dirs as usize;
        } } count
    }, || {
        let mut count=0;
        for _ in 0..ROUNDS { for &i in black_box(&queries) { count += metadata[i].dirs as usize; } } count
    });
    for (name, density) in [("ray-dense", 3), ("ray-sparse", 16), ("ray-empty", 0)] {
        let mut lines = Vec::new();
        let mut masks = Vec::new();
        for _ in 0..64 {
            let mut line = [false; 36];
            let mut mask = 0u64;
            for (i, square) in line.iter_mut().enumerate() {
                *square = density > 0 && next() % density == 0;
                if *square { mask |= 1 << i; }
            }
            for from in 0..36 { for to in 0..36 {
                let lo = from.min(to); let hi = from.max(to);
                let clear = (lo+1..hi).all(|i| !line[i]);
                let segment = if hi <= lo+1 { 0 } else { ((1u64 << hi)-1) & !((1u64 << (lo+1))-1) };
                assert_eq!(clear, mask & segment == 0);
            } }
            lines.push(line); masks.push(mask);
        }
        let queries: Vec<_> = (0..N).map(|_|(next()%64, next()%36, next()%36)).collect();
        pair(name, N*ROUNDS, || {
            let mut count = 0;
            for _ in 0..ROUNDS { for &(i, from, to) in black_box(&queries) {
                let lo = from.min(to); let hi = from.max(to);
                count += (lo+1..hi).all(|sq| !lines[i][sq]) as usize;
            } } count
        }, || {
            let mut count = 0;
            for _ in 0..ROUNDS { for &(i, from, to) in black_box(&queries) {
                let lo = from.min(to); let hi = from.max(to);
                let segment = if hi <= lo+1 { 0 } else { ((1u64 << hi)-1) & !((1u64 << (lo+1))-1) };
                count += (masks[i] & segment == 0) as usize;
            } } count
        });
    }
}
