//! Opt-in measurement prototypes. Never enabled in production builds.
use super::{features, Accumulator};
use crate::piece::{Color, Piece};
use std::sync::OnceLock;

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
    pub(crate) fn save_sums(&mut self) -> Vec<i32> {
        let mut saved = self.probe.undo_pool.pop().unwrap_or_default();
        saved.clone_from(&self.sums);
        saved
    }
    pub(crate) fn restore_sums(&mut self, mut saved: Vec<i32>) {
        std::mem::swap(&mut saved, &mut self.sums);
        self.probe.undo_pool.push(saved);
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
