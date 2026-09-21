//! Bounded model-local feature sums and reusable make/unmake snapshots.
use super::{features, Accumulator};
use crate::piece::{Color, Piece};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) struct Snapshot {
    sums: Vec<i32>,
    net: Arc<super::Network>,
}

const SLOTS: usize = 4096;
#[derive(Debug, Default)]
pub(super) struct Storage {
    // Owned by this accumulator, hence never shared across models or searches.
    // Direct mapping bounds memory; full keys make collisions harmless.
    cache: Vec<Option<(usize, Vec<i32>)>>,
    undo_pool: Vec<Vec<i32>>,
}
impl Clone for Storage {
    fn clone(&self) -> Self {
        Self::default()
    }
}
impl Accumulator {
    pub(super) fn change_cached(&mut self, piece: &Piece, sign: i32) {
        let w = self.net.width;
        if self.storage.cache.is_empty() {
            self.storage.cache.resize_with(SLOTS, || None);
        }
        for (side, color) in [Color::Black, Color::White].into_iter().enumerate() {
            let key = (features::key(piece) * 2 + side) * 1296 + piece.position.to_index();
            let slot = ((key as u64)
                .wrapping_mul(0x9e3779b97f4a7c15)
                .rotate_right(32) as usize)
                % SLOTS;
            let entry = &mut self.storage.cache[slot];
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
        assert!(acc.storage.cache.iter().any(Option::is_some));
        let clone = acc.clone();
        assert!(clone.storage.cache.is_empty() && clone.storage.undo_pool.is_empty());
        assert_eq!(clone.sums, acc.sums);
    }
}
