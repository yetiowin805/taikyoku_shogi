//! Opt-in measurement prototypes. Never enabled in production builds.
use super::{features, Accumulator};
use crate::piece::{Color, Piece};
use std::{cell::Cell, sync::OnceLock};

fn mode() -> &'static str {
    static MODE: OnceLock<String> = OnceLock::new();
    MODE.get_or_init(|| {
        let value = std::env::var("NNUE_SPEED_PROBE").unwrap_or_else(|_| "baseline".into());
        assert!(matches!(
            value.as_str(),
            "baseline" | "fused" | "snapshot" | "both"
        ));
        value
    })
}
pub fn fused() -> bool {
    matches!(mode(), "fused" | "both")
}
pub fn snapshot() -> bool {
    matches!(mode(), "snapshot" | "both")
}

const SLOTS: usize = 4096;
#[derive(Debug, Default)]
pub(super) struct Storage {
    // Owned by this accumulator, hence never shared across models or searches.
    // Direct mapping bounds memory; full keys make collisions harmless.
    cache: Vec<Option<(usize, Vec<i32>)>>,
    undo_pool: Vec<Vec<i32>>,
    pub(super) residual: Cell<[Option<i32>; 2]>,
}
impl Clone for Storage {
    fn clone(&self) -> Self {
        Self::default()
    }
}
impl Accumulator {
    pub(super) fn change_fused(&mut self, piece: &Piece, sign: i32) {
        let w = self.net.width;
        if self.probe.cache.is_empty() {
            self.probe.cache.resize_with(SLOTS, || None);
        }
        for (side, color) in [Color::Black, Color::White].into_iter().enumerate() {
            let key = (features::key(piece) * 2 + side) * 1296 + piece.position.to_index();
            let slot = key.wrapping_mul(0x9e3779b97f4a7c15usize).rotate_right(32) % SLOTS;
            let entry = &mut self.probe.cache[slot];
            if !entry.as_ref().is_some_and(|(k, _)| *k == key) {
                let mut sum = entry.take().map(|(_, v)| v).unwrap_or_else(|| vec![0; w]);
                sum.fill(0);
                features::visit(piece, color, |i| {
                    for (a, &b) in sum.iter_mut().zip(&self.net.weights[i * w..(i + 1) * w]) {
                        *a += i32::from(b);
                    }
                });
                *entry = Some((key, sum));
            }
            for (a, &b) in self.sums[side * w..(side + 1) * w]
                .iter_mut()
                .zip(&entry.as_ref().unwrap().1)
            {
                *a += sign * b;
            }
        }
    }
    pub(crate) fn save_sums(&mut self) -> Snapshot {
        let mut saved = self.probe.undo_pool.pop().unwrap_or_default();
        saved.clone_from(&self.sums);
        Snapshot { sums: saved, residual: self.probe.residual.get() }
    }
    pub(crate) fn restore_sums(&mut self, mut saved: Snapshot) {
        std::mem::swap(&mut saved.sums, &mut self.sums);
        self.probe.residual.set(saved.residual);
        self.probe.undo_pool.push(saved.sums);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{board::Board, eval::ALL_PIECE_TYPES, position::Position};
    #[test]
    fn fused_rows_and_snapshots_match_scalar_with_collisions() {
        let net = super::super::tests::net(32, "fused-parity");
        let mut acc = Accumulator::new(net, &Board::new());
        // Exercise every piece, promotion state and color over more keys than slots.
        for square in [0, 35, 36, 629, 1295] {
            for &pt in ALL_PIECE_TYPES {
                for color in [Color::Black, Color::White] {
                    for promoted in [false, true] {
                        let mut p = Piece::new(pt, color, Position::from_index(square).unwrap());
                        if promoted {
                            p.promote();
                        }
                        let before = acc.sums.clone();
                        let saved = acc.save_sums();
                        acc.change_fused(&p, 1);
                        let mut expected = before.clone();
                        for (side, c) in [Color::Black, Color::White].into_iter().enumerate() {
                            features::visit(&p, c, |i| {
                                for (a, &b) in expected[side * 32..(side + 1) * 32]
                                    .iter_mut()
                                    .zip(&acc.net.weights[i * 32..(i + 1) * 32])
                                {
                                    *a += i32::from(b);
                                }
                            });
                        }
                        assert_eq!(acc.sums, expected);
                        acc.change_fused(&p, -1);
                        assert_eq!(acc.sums, before);
                        acc.restore_sums(saved);
                        assert_eq!(acc.sums, before);
                    }
                }
            }
        }
    }
}

/// Process-constant opt-in additions, always compared with native + `both`.
#[derive(Default)]
pub struct Followup {
    pub quant: bool,
    pub packed: bool,
    pub royal: bool,
    pub memo: bool,
    pub stats: bool,
    pub head: usize,
}
pub fn followup() -> &'static Followup {
    static FLAGS: OnceLock<Followup> = OnceLock::new();
    FLAGS.get_or_init(|| {
        let mut f = Followup::default();
        for part in std::env::var("NNUE_FOLLOWUP").unwrap_or_default().split(',').filter(|p| !p.is_empty()) {
            match part {
                "quant" => f.quant = true,
                "packed" => f.packed = true,
                "royal" => f.royal = true,
                "memo" => f.memo = true,
                "stats" => f.stats = true,
                "head24" => f.head = 24,
                "head16" => f.head = 16,
                _ => panic!("unknown NNUE_FOLLOWUP option: {part}"),
            }
        }
        f
    })
}
#[derive(Debug, Clone)]
pub(crate) struct Snapshot {
    sums: Vec<i32>,
    residual: [Option<i32>; 2],
}
thread_local! { static COUNTERS: Cell<[u64; 4]> = const {Cell::new([0; 4])}; }
// residual calls, residual cache hits, change calls, royal probes.
pub fn count(index: usize) {
    if followup().stats { COUNTERS.with(|c| {let mut v=c.get(); v[index]+=1; c.set(v)}); }
}
pub fn reset_counters() { COUNTERS.with(|c| c.set([0; 4])); }
pub fn counters() -> [u64; 4] { COUNTERS.with(Cell::get) }

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HeadPlan {
    sha256: String,
    width: usize,
    pub keep: Vec<usize>,
    pub fill: [i32; 32],
}
pub(super) fn head_plan(sha: &str, width: usize) -> Option<HeadPlan> {
    let n = followup().head;
    if n == 0 { return None; }
    let path=std::env::var("NNUE_HEAD_PLAN").expect("NNUE_HEAD_PLAN required for pruning");
    let p: HeadPlan=serde_json::from_slice(&std::fs::read(path).expect("head plan missing")).expect("bad head plan");
    assert_eq!(p.sha256, sha, "head plan model mismatch");
    assert_eq!(p.width, width);
    assert_eq!(p.keep.len(), n);
    assert!(p.keep.windows(2).all(|w| w[0] < w[1]) && p.keep.iter().all(|&j| j<32));
    assert!(p.fill.iter().all(|&v| (0..=127).contains(&v)));
    Some(p)
}
pub fn quantize(a: i32) -> u8 { ((a.clamp(0,4096)*127+2048)>>12) as u8 }

// Unsigned inputs are <=127, so each pair summed by maddubs is in
// [-32512,32258]: no saturating intermediate loses information. The entire
// dot product (at most 8192 terms) also fits i32.
#[cfg(target_arch="x86_64")]
#[target_feature(enable="avx2")]
unsafe fn dot_avx2(x: &[u8], w: &[i8]) -> i32 {
    use std::arch::x86_64::*;
    let mut acc=_mm256_setzero_si256();
    let ones=_mm256_set1_epi16(1);
    for i in (0..x.len()).step_by(32) {
        let a=_mm256_loadu_si256(x.as_ptr().add(i).cast());
        let b=_mm256_loadu_si256(w.as_ptr().add(i).cast());
        let pairs=_mm256_maddubs_epi16(a,b);
        acc=_mm256_add_epi32(acc,_mm256_madd_epi16(pairs,ones));
    }
    let mut lanes=[0i32;8];
    _mm256_storeu_si256(lanes.as_mut_ptr().cast(),acc);
    lanes.into_iter().sum()
}
pub(super) fn dot(x: &[u8], w: &[i8]) -> i32 {
    assert_eq!(x.len(), w.len());
    assert_eq!(x.len()%32, 0);
    #[cfg(target_arch="x86_64")]
    if is_x86_feature_detected!("avx2") { return unsafe {dot_avx2(x,w)}; }
    panic!("packed experiment requires AVX2");
}

#[cfg(test)]
mod followup_tests {
    use super::*;
    #[test]
    fn quantizer_extremes_and_packed_dot_are_exact() {
        for a in (-100_000..=100_000).chain([i32::MIN,i32::MAX,4095,4096,4097]) {
            assert_eq!(quantize(a), ((i64::from(a)*127+2048)/4096).clamp(0,127) as u8);
        }
        #[cfg(target_arch="x86_64")]
        if is_x86_feature_detected!("avx2") {
            for n in [32,64,1024,4096,8192] {
                for mode in 0..4 {
                    let x:Vec<_>=(0..n).map(|i| if mode==0 {(i%128) as u8} else {127}).collect();
                    let w:Vec<_>=(0..n).map(|i| match mode {0=>((i*73)%256) as u8 as i8,1=>-128,2=>127,_=>if i%2==0 {-128} else {127}}).collect();
                    assert_eq!(dot(&x,&w), x.iter().zip(&w).map(|(&a,&b)|i32::from(a)*i32::from(b)).sum::<i32>());
                }
            }
        }
    }
    #[test]
    fn memoized_results_follow_changes_and_restore_both_perspectives() {
        use crate::{board::Board, position::Position, piece::PieceType};
        let mut acc=Accumulator::new(super::super::tests::net(32,"memo"),&Board::new());
        let scores=[acc.residual(Color::Black),acc.residual(Color::White)];
        let saved=acc.save_sums();
        let p=Piece::new(PieceType::King,Color::Black,Position::new(3,4).unwrap());
        acc.change(&p,1);
        for c in [Color::Black,Color::White] { assert_eq!(acc.residual(c),acc.residual_uncached(c)); }
        acc.restore_sums(saved);
        assert_eq!(scores,[acc.residual(Color::Black),acc.residual(Color::White)]);
        assert_eq!(acc.clone().residual(Color::Black),scores[0]);
    }
}
