//! Experimental cache of range two-mover first-leg counts. No weighted sums are
//! cached: evaluation still adds each piece's term in the original list order.
use crate::position::Position;
use std::sync::atomic::{AtomicU32, Ordering};
pub(crate) struct MobilityCache(Box<[AtomicU32]>);
impl MobilityCache {
    pub(crate) fn new() -> Self {
        Self((0..1296).map(|_| AtomicU32::new(u32::MAX)).collect())
    }
    pub(crate) fn get(&self, p: Position) -> Option<u32> {
        let v = self.0[p.to_index()].load(Ordering::Relaxed);
        (v != u32::MAX).then_some(v)
    }
    pub(crate) fn put(&self, p: Position, value: u32) {
        self.0[p.to_index()].store(value, Ordering::Relaxed);
    }
    /// Conservatively invalidate every possible first-leg ray through a changed
    /// square. All board edits call this, including intermediate captures and
    /// piece replacement during promotion/unmake. It never depends on a ray's
    /// old blockers, so a removed blocker cannot hide an affected piece.
    pub(crate) fn invalidate(&mut self, p: Position) {
        *self.0[p.to_index()].get_mut() = u32::MAX;
        for (df, dr) in [
            (-1, -1),
            (-1, 0),
            (-1, 1),
            (0, -1),
            (0, 1),
            (1, -1),
            (1, 0),
            (1, 1),
        ] {
            let mut next = p;
            while let Some(sq) = next.offset(df, dr) {
                *self.0[sq.to_index()].get_mut() = u32::MAX;
                next = sq;
            }
        }
    }
}
impl Clone for MobilityCache {
    fn clone(&self) -> Self {
        Self(
            self.0
                .iter()
                .map(|v| AtomicU32::new(v.load(Ordering::Relaxed)))
                .collect(),
        )
    }
}
