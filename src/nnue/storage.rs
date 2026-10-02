//! Bounded model-local feature sums and reusable make/unmake snapshots.
use super::{features, Accumulator};
use crate::piece::{Color, Piece};
use std::sync::{Arc, Weak};

#[derive(Debug, Clone)]
pub(crate) struct Snapshot {
    sums: Vec<i32>,
    net: Arc<super::Network>,
}

// At width 2048 the row and delta payloads total 128 MiB, allocated lazily.
const SLOTS: usize = 16384;
// Quiescence repeats the same multi-piece capture sweeps across many sibling
// and cousin nodes. Caching a move's whole accumulator delta turns its 4-20
// row reads per perspective into one. Keys are the exact changed-piece list.
const DELTA_SLOTS: usize = 4096;
const DELTA_MIN_CHANGES: usize = 4;
const EMPTY: usize = usize::MAX;
fn piece_code(p: &Piece) -> u32 {
    (features::key(p) as u32) << 11 | p.position.to_index() as u32
}
#[derive(Debug, Default)]
pub(super) struct Storage {
    // Owned by this accumulator, reusable across searches with the same model.
    // Direct mapping bounds memory; full keys make collisions harmless. Rows
    // live in flat arrays (slot * width), allocated on first use.
    keys: Vec<usize>,
    rows: Vec<i16>,
    delta_keys: Vec<Vec<u32>>,
    deltas: Vec<i32>,
    undo_pool: Vec<Vec<i32>>,
    scratch: Vec<(usize, usize, bool)>,
    wide: Vec<i32>,
    codes: Vec<u32>,
}
impl Clone for Storage {
    fn clone(&self) -> Self {
        Self::default()
    }
}
/// Row and delta caches of the most recently dropped accumulator on this
/// thread. Successive searches of one game share most pieces and capture
/// sweeps, so a new search adopts them instead of starting cold. A weak network
/// identity avoids retaining an otherwise unused large network on every thread.
type Parked = (
    Weak<super::Network>,
    Vec<usize>,
    Vec<i16>,
    Vec<Vec<u32>>,
    Vec<i32>,
);
thread_local! {
    static PARKED: std::cell::RefCell<Option<Parked>> = const { std::cell::RefCell::new(None) };
}
impl Drop for Accumulator {
    fn drop(&mut self) {
        if self.storage.keys.is_empty() {
            return;
        }
        let st = &mut self.storage;
        let parked = (
            Arc::downgrade(&self.net),
            std::mem::take(&mut st.keys),
            std::mem::take(&mut st.rows),
            std::mem::take(&mut st.delta_keys),
            std::mem::take(&mut st.deltas),
        );
        let _ = PARKED.try_with(|p| *p.borrow_mut() = Some(parked));
    }
}
impl Accumulator {
    /// Offset in `rows` of the summed ability rows of `piece` from one
    /// perspective, with its cache key, or `None` if that sum does not fit
    /// i16 (then callers add the weight rows directly, which is exact).
    fn ensure_row(&mut self, piece: &Piece, side: usize) -> Option<(usize, usize)> {
        let w = self.net.width;
        self.ensure_storage();
        // The cache identity is the exact selected feature set, including the
        // original ability list (White jumping pieces may differ from Black).
        let ability = features::schema().row_classes[features::key(piece)];
        let relative_color = usize::from(piece.color == Color::White) ^ side;
        let square = if side == 1 { 1295 - piece.position.to_index() } else { piece.position.to_index() };
        let key = (ability * 2 + relative_color) * 1296 + square;
        let slot = ((key as u64)
            .wrapping_mul(0x9e3779b97f4a7c15)
            .rotate_right(32) as usize)
            % SLOTS;
        if self.storage.keys[slot] == key {
            return Some((slot * w, key));
        }
        let mut wide = std::mem::take(&mut self.storage.wide);
        wide.clear();
        wide.resize(w, 0);
        self.add_direct(&mut wide, piece, side, 1);
        let fits = wide.iter().all(|&v| i16::try_from(v).is_ok());
        if fits {
            for (d, &v) in self.storage.rows[slot * w..(slot + 1) * w]
                .iter_mut()
                .zip(&wide)
            {
                *d = v as i16;
            }
            self.storage.keys[slot] = key;
        }
        self.storage.wide = wide;
        fits.then_some((slot * w, key))
    }
    fn ensure_storage(&mut self) {
        if !self.storage.keys.is_empty() {
            return;
        }
        let net = &self.net;
        let parked = PARKED
            .try_with(|p| p.borrow_mut().take())
            .ok()
            .flatten()
            .filter(|(n, ..)| n.upgrade().is_some_and(|n| Arc::ptr_eq(&n, net)));
        let st = &mut self.storage;
        if let Some((_, keys, rows, delta_keys, deltas)) = parked {
            (st.keys, st.rows, st.delta_keys, st.deltas) = (keys, rows, delta_keys, deltas);
        } else {
            st.keys = vec![EMPTY; SLOTS];
            st.rows = vec![0; SLOTS * self.net.width];
        }
    }
    fn add_direct(&self, dst: &mut [i32], piece: &Piece, side: usize, sign: i32) {
        let w = self.net.width;
        features::visit(piece, [Color::Black, Color::White][side], |i| {
            for (a, &b) in dst.iter_mut().zip(&self.net.weights[i * w..(i + 1) * w]) {
                *a += sign * i32::from(b);
            }
        });
    }
    pub(super) fn change_cached(&mut self, piece: &Piece, sign: i32) {
        let w = self.net.width;
        for side in 0..2 {
            let mut sums = std::mem::take(&mut self.sums);
            let dst = &mut sums[side * w..(side + 1) * w];
            match self.ensure_row(piece, side) {
                Some((off, _)) => {
                    for (a, &b) in dst.iter_mut().zip(&self.storage.rows[off..off + w]) {
                        *a += sign * i32::from(b);
                    }
                }
                None => self.add_direct(dst, piece, side, sign),
            }
            self.sums = sums;
        }
    }
    /// Save the current sums and apply every piece change of one move in a
    /// single tiled pass per perspective. The new sums are written into the
    /// recycled undo buffer, so no separate snapshot copy is needed.
    pub(crate) fn save_and_apply(&mut self, subs: &[Piece], add: Option<&Piece>) -> Snapshot {
        let w = self.net.width;
        let mut out = self.storage.undo_pool.pop().unwrap_or_default();
        out.resize(2 * w, 0);
        self.ensure_storage();
        let mut codes = std::mem::take(&mut self.storage.codes);
        let delta_slot = if subs.len() + usize::from(add.is_some()) >= DELTA_MIN_CHANGES {
            codes.clear();
            codes.extend(subs.iter().map(piece_code));
            codes.extend(add.map(|p| piece_code(p) | 1 << 31));
            let mut h = 0u64;
            for &c in &codes {
                h = (h ^ u64::from(c))
                    .wrapping_mul(0x9e37_79b9_7f4a_7c15)
                    .rotate_left(29);
            }
            let slot = (h >> 32) as usize % DELTA_SLOTS;
            if self.storage.delta_keys.is_empty() {
                self.storage.delta_keys = vec![Vec::new(); DELTA_SLOTS];
                self.storage.deltas = vec![0; DELTA_SLOTS * 2 * w];
            }
            if self.storage.delta_keys[slot] == codes {
                let d = &self.storage.deltas[slot * 2 * w..(slot + 1) * 2 * w];
                for (o, (&a, &b)) in out.iter_mut().zip(self.sums.iter().zip(d)) {
                    *o = a + b;
                }
                self.storage.codes = codes;
                std::mem::swap(&mut out, &mut self.sums);
                return Snapshot {
                    sums: out,
                    net: self.net.clone(),
                };
            }
            Some(slot)
        } else {
            None
        };
        let mut slots = std::mem::take(&mut self.storage.scratch);
        for side in 0..2 {
            slots.clear();
            let mut cached = true;
            for (p, plus) in subs
                .iter()
                .map(|p| (p, false))
                .chain(add.map(|p| (p, true)))
            {
                match self.ensure_row(p, side) {
                    Some((off, key)) => slots.push((off, key, plus)),
                    None => cached = false,
                }
            }
            let (keys, rows) = (&self.storage.keys, &self.storage.rows);
            // A later row can evict an earlier one from the direct-mapped cache.
            cached &= slots.iter().all(|&(off, key, _)| keys[off / w] == key);
            let src = &self.sums[side * w..(side + 1) * w];
            let dst = &mut out[side * w..(side + 1) * w];
            if !cached {
                dst.copy_from_slice(src);
                for p in subs {
                    self.add_direct(dst, p, side, -1);
                }
                if let Some(p) = add {
                    self.add_direct(dst, p, side, 1);
                }
            } else if w % 64 == 0 {
                tiled::<64>(src, dst, rows, &slots);
            } else {
                tiled::<32>(src, dst, rows, &slots);
            }
        }
        self.storage.scratch = slots;
        if let Some(slot) = delta_slot {
            let d = &mut self.storage.deltas[slot * 2 * w..(slot + 1) * 2 * w];
            for (d, (&new, &old)) in d.iter_mut().zip(out.iter().zip(&self.sums)) {
                *d = new - old;
            }
            self.storage.delta_keys[slot].clone_from(&codes);
        }
        self.storage.codes = codes;
        std::mem::swap(&mut out, &mut self.sums);
        Snapshot {
            sums: out,
            net: self.net.clone(),
        }
    }
    #[cfg(test)]
    pub(crate) fn save_sums(&mut self) -> Snapshot {
        let mut saved = self.storage.undo_pool.pop().unwrap_or_default();
        saved.clone_from(&self.sums);
        Snapshot {
            sums: saved,
            net: self.net.clone(),
        }
    }
    pub(crate) fn restore_sums(&mut self, mut saved: Snapshot) -> bool {
        // Ordinary search keeps one model bound throughout. Reject a snapshot
        // if a caller explicitly rebound the GameState between make and undo.
        if !Arc::ptr_eq(&saved.net, &self.net) {
            return false;
        }
        debug_assert_eq!(saved.sums.len(), self.sums.len());
        std::mem::swap(&mut saved.sums, &mut self.sums);
        self.storage.undo_pool.push(saved.sums);
        true
    }
}

#[inline(always)]
fn tiled<const T: usize>(
    src: &[i32],
    dst: &mut [i32],
    rows: &[i16],
    slots: &[(usize, usize, bool)],
) {
    for base in (0..src.len()).step_by(T) {
        let mut blk = [0i32; T];
        blk.copy_from_slice(&src[base..base + T]);
        for &(off, _, plus) in slots {
            let row = &rows[off + base..off + base + T];
            if plus {
                for k in 0..T {
                    blk[k] += i32::from(row[k]);
                }
            } else {
                for k in 0..T {
                    blk[k] -= i32::from(row[k]);
                }
            }
        }
        dst[base..base + T].copy_from_slice(&blk);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{board::Board, eval::ALL_PIECE_TYPES, position::Position};
    #[test]
    fn canonical_keys_share_exact_features_across_perspectives() {
        let net = super::super::tests::net(32, "canonical-rows");
        let mut acc = Accumulator::new(net, &Board::new());
        for &pt in ALL_PIECE_TYPES {
            for promoted in [false, true] {
                for reverse in [false, true] {
                    let mut black = Piece::new(pt, Color::Black, Position::from_index(37).unwrap());
                    black.is_promoted = promoted;
                    black.base_piece_type = reverse.then_some(crate::piece::PieceType::ReverseChariot);
                    let mut white = black;
                    white.color = Color::White;
                    white.position = Position::from_index(1295 - 37).unwrap();
                    let mut a = Vec::new();
                    let mut b = Vec::new();
                    features::visit(&black, Color::Black, |i| a.push(i));
                    features::visit(&white, Color::White, |i| b.push(i));
                    let ka = acc.ensure_row(&black, 0).unwrap();
                    let kb = acc.ensure_row(&white, 1).unwrap();
                    assert_eq!(ka == kb, a == b);
                }
            }
        }
    }
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
                        acc.change_cached(&p, 1);
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
                        acc.change_cached(&p, -1);
                        assert_eq!(acc.sums, before);
                        assert!(acc.restore_sums(saved));
                        assert_eq!(acc.sums, before);
                    }
                }
            }
        }
        assert!(acc.storage.keys.iter().any(|&k| k != EMPTY));
        let clone = acc.clone();
        assert!(clone.storage.keys.is_empty() && clone.storage.undo_pool.is_empty());
        assert_eq!(clone.sums, acc.sums);
    }

    /// Independent scalar sums: bias plus every active weight row.
    fn reference(net: &Arc<super::super::Network>, board: &crate::board::Board) -> Vec<i32> {
        let w = net.width;
        let mut out = [net.bias.clone(), net.bias.clone()].concat();
        for (side, persp) in [Color::Black, Color::White].into_iter().enumerate() {
            for c in [Color::Black, Color::White] {
                for p in board.pieces_by_color(c) {
                    features::visit(p, persp, |i| {
                        for (a, &b) in out[side * w..(side + 1) * w]
                            .iter_mut()
                            .zip(&net.weights[i * w..(i + 1) * w])
                        {
                            *a += i32::from(b);
                        }
                    });
                }
            }
        }
        out
    }
    #[test]
    fn batched_moves_match_rebuild_including_i16_overflow_rows() {
        use crate::game_state::GameState;
        for big in [false, true] {
            let mut net = (*super::super::tests::net(32, "batched")).clone_for_test();
            if big {
                // Three or more channels at 16000 overflow i16, forcing the direct path
                // (loadable nets cap weights at 2048, where 17+ channels overflow).
                net.weights.iter_mut().for_each(|x| *x = 16000);
            }
            let net = Arc::new(net);
            let mut s = GameState::new();
            s.setup_initial_position();
            s.nnue = Some(Accumulator::new(net.clone(), s.get_board()));
            assert_eq!(
                s.nnue.as_ref().unwrap().sums,
                reference(&net, s.get_board())
            );
            let before = s.nnue.as_ref().unwrap().sums.clone();
            let mut undos = Vec::new();
            for mv in s.generate_legal_moves().into_iter().take(40) {
                if let Some(u) = s.make_move_for_search(mv) {
                    assert_eq!(
                        s.nnue.as_ref().unwrap().sums,
                        reference(&net, s.get_board())
                    );
                    undos.push(u);
                    if undos.len() == 3 {
                        while let Some(u) = undos.pop() {
                            s.unmake_move_for_search(u);
                        }
                    }
                }
            }
            while let Some(u) = undos.pop() {
                s.unmake_move_for_search(u);
            }
            assert_eq!(s.nnue.as_ref().unwrap().sums, before);
            if big {
                // The initial army must contain pieces whose summed row overflows i16.
                let wide = |p: &Piece| features::schema().pieces[features::key(p)].len() >= 3;
                let mut s = GameState::new();
                s.setup_initial_position();
                assert!(s.get_board().pieces_by_color(Color::Black).iter().any(wide));
            }
        }
    }

    #[test]
    fn parked_caches_are_adopted_only_by_the_same_network() {
        use crate::game_state::GameState;
        let a = super::super::tests::net(32, "park");
        let mut different = a.clone_for_test();
        different.weights.iter_mut().for_each(|v| *v += 1);
        let b = Arc::new(different);
        let mut s = GameState::new();
        s.setup_initial_position();
        let board = s.get_board().clone();
        let filled = |acc: &Accumulator| acc.storage.keys.iter().filter(|&&k| k != EMPTY).count();
        let first = Accumulator::new(a.clone(), &board);
        let warm = filled(&first);
        assert!(warm > 0);
        drop(first);
        // Same network: the new accumulator starts from the parked rows.
        let second = Accumulator::new(a.clone(), &board);
        assert_eq!(filled(&second), warm);
        assert_eq!(second.sums, reference(&a, &board));
        drop(second);
        // Different network object/weights: never adopt stale cached rows.
        let third = Accumulator::new(b.clone(), &board);
        assert_eq!(third.sums, reference(&b, &board));
        assert!(PARKED.with(|p| p.borrow().is_none()));
    }

    #[test]
    fn delta_hits_apply_to_different_accumulator_sums() {
        let net = super::super::tests::net(32, "delta-base-independent");
        let pieces: Vec<_> = (0..5)
            .map(|i| {
                Piece::new(
                    crate::piece::PieceType::Pawn,
                    Color::Black,
                    Position::new(i, 12).unwrap(),
                )
            })
            .collect();
        let mut board = Board::new();
        for &piece in &pieces {
            board.place_piece(piece);
        }
        let mut acc = Accumulator::new(net.clone(), &board);
        let before = acc.sums.clone();
        let saved = acc.save_and_apply(&pieces, None);
        assert_eq!(acc.sums, reference(&net, &Board::new()));
        assert!(acc.storage.delta_keys.iter().any(|k| !k.is_empty()));
        assert!(acc.restore_sums(saved));
        assert_eq!(acc.sums, before);
        // Same sweep, but a different unrelated piece changes the starting sums.
        let extra = Piece::new(
            crate::piece::PieceType::Rook,
            Color::White,
            Position::new(30, 30).unwrap(),
        );
        acc.change_cached(&extra, 1);
        let saved = acc.save_and_apply(&pieces, None);
        let mut expected = Board::new();
        expected.place_piece(extra);
        assert_eq!(acc.sums, reference(&net, &expected));
        assert!(acc.restore_sums(saved));
        board.place_piece(extra);
        assert_eq!(acc.sums, reference(&net, &board));
        drop(acc);
        // Parked rows must not keep a no-longer-active weight blob alive.
        assert_eq!(Arc::strong_count(&net), 1);
    }
}
