//! Non-default search experiments. Conservative fallbacks are deliberate.
use super::*;

/// Search an unambiguous TT route before generating the other pieces. Restricted
/// mode accepts only Stage A routes: exactly the move existing TT ordering puts
/// first. Stage-promoting mode also accepts a quiet multi-leg Stage B route.
///
/// Search make/unmake changes piece-list order, and searching can update ordering
/// heuristics. Freeze those inputs before the probe so delayed generation uses
/// the original sequence. This copy is part of the measured cost.
#[allow(clippy::too_many_arguments)]
pub(super) fn tt_first(
    state: &mut GameState,
    weights: &EvalWeights,
    depth: u32,
    alpha: i32,
    beta: i32,
    alpha_orig: i32,
    is_pv: bool,
    ctx: &mut SearchContext,
    key: u64,
    hint: Option<MoveKey>,
) -> Option<i32> {
    // Splitting a sibling reduction group would need shared representative state.
    if ctx.sibling_mode != 0 {
        return None;
    }
    let hint = hint?;
    let from = Position::new(hint.0, hint.1)?;
    let piece = state.get_board().get_piece(from)?;
    if piece.color != state.get_current_turn() {
        return None;
    }
    let full = state.generate_legal_moves_for_pieces(&[piece]);
    let mut matches = full.iter().filter(|m| same_tt_move(m, hint));
    let first = matches.next()?.clone();
    if matches.next().is_some() {
        return None;
    }
    let mut piece_a = Vec::new();
    state.generate_legal_moves_for_pieces_mode(
        &[piece],
        LegalMoveGen::WithoutQuietMultiLeg,
        &mut piece_a,
    );
    let in_a = piece_a.iter().any(|m| same_tt_move(m, hint));
    if !in_a && crate::optimization::options().tt_first == 1 {
        return None;
    }

    let reuse_context = context(state, ctx, depth, alpha, beta, is_pv, None);
    let frozen = state.clone();
    let mut killers = ctx.killers.clone();
    let mut history = ctx.history.clone();
    let parent = ctx.ply;
    crate::optimization::count(|c| c.tt_first_probes += 1);
    let (mut best, mut best_key, mut a, mut cut) = search_move_list_offset(
        state,
        weights,
        depth,
        alpha,
        beta,
        is_pv,
        ctx,
        parent,
        std::slice::from_ref(&first),
        0,
        0,
    );
    if cut {
        crate::optimization::count(|c| c.tt_first_cutoffs += 1);
    }

    if !cut && !ctx.abort {
        let mut moves = ctx.buffers.moves.take();
        frozen.generate_legal_moves_mode_into(LegalMoveGen::WithoutQuietMultiLeg, &mut moves);
        let mut only_b = false;
        if moves.is_empty() {
            frozen.generate_legal_moves_mode_into(LegalMoveGen::QuietMultiLegOnly, &mut moves);
            only_b = true;
        }
        let original_a_len = moves.len();
        // Restore original ordering inputs just for sorting, then put the live
        // heuristics back. No search runs while these fields are swapped.
        std::mem::swap(&mut ctx.killers, &mut killers);
        std::mem::swap(&mut ctx.history, &mut history);
        order_moves_with_heuristics(&frozen, weights, &mut moves, ctx, parent, only_b, false);
        std::mem::swap(&mut ctx.killers, &mut killers);
        std::mem::swap(&mut ctx.history, &mut history);
        prefer_tt_move(&mut moves, Some(hint));
        if in_a || only_b {
            debug_assert!(moves.first().is_some_and(|m| same_tt_move(m, hint)));
            moves.remove(0);
        }
        let (score, best2, next_a, next_cut) = search_move_list_offset(
            state, weights, depth, a, beta, is_pv, ctx, parent, &moves, 1, 1,
        );
        if score > best {
            best = score;
            best_key = best2;
        }
        a = next_a;
        cut = next_cut;
        if !cut && !ctx.abort && !only_b {
            let mut stage_b = ctx.buffers.moves.take();
            state.generate_legal_moves_mode_into(LegalMoveGen::QuietMultiLegOnly, &mut stage_b);
            order_moves_with_heuristics(state, weights, &mut stage_b, ctx, parent, true, false);
            prefer_tt_move(&mut stage_b, Some(hint));
            if !in_a {
                stage_b.retain(|m| !same_tt_move(m, hint));
            }
            let (score, best2, _, _) = search_move_list(
                state,
                weights,
                depth,
                a,
                beta,
                is_pv,
                ctx,
                parent,
                &stage_b,
                original_a_len,
            );
            if score > best {
                best = score;
                best_key = best2;
            }
        }
    }
    if best == i32::MIN + 1 {
        return Some(evaluate_with_ply(state, weights, ctx.ply));
    }
    if !ctx.abort {
        let bound = if best <= alpha_orig {
            TtBound::Upper
        } else if best >= beta {
            TtBound::Lower
        } else {
            TtBound::Exact
        };
        let entry = TtEntry {
            key,
            depth,
            score: best,
            bound,
            best: best_key,
        };
        ctx.reuse.store(entry, reuse_context, false);
        ctx.tt.store(entry);
    }
    Some(best)
}

use std::hash::{Hash, Hasher};
const MAX_RETAINED: usize = 65_536;
#[derive(Clone, Copy)]
struct RetainedEntry {
    entry: TtEntry,
    context: u64,
}
#[derive(Default)]
struct Retained {
    identity: Vec<u8>,
    main: HashMap<u64, RetainedEntry>,
    q: HashMap<u64, RetainedEntry>,
}
thread_local! { static RETAINED: std::cell::RefCell<Option<Retained>> = const { std::cell::RefCell::new(None) }; }
#[derive(Default)]
pub(super) struct ReuseSession {
    mode: u8,
    previous: Retained,
    next: Retained,
}
impl ReuseSession {
    pub(super) fn acquire(weights: &EvalWeights, config: &SearchConfig) -> Self {
        let mode = crate::optimization::options().retain_tt;
        if mode == 0 {
            return Self::default();
        }
        let mut policy = config.clone();
        // Budgets change how much is completed, not the meaning of an entry.
        policy.depth = 1;
        policy.max_time_ms = None;
        let mut identity = serde_json::to_vec(weights).expect("serializable weights");
        identity.extend_from_slice(
            format!("{policy:?} {:?}", crate::optimization::options()).as_bytes(),
        );
        let previous = RETAINED
            .with(|p| p.borrow_mut().take())
            .filter(|p| p.identity == identity)
            .unwrap_or_default();
        Self {
            mode,
            previous,
            next: Retained {
                identity,
                ..Default::default()
            },
        }
    }
    pub(super) fn probe(
        &self,
        state: &GameState,
        key: u64,
        context: u64,
        q: bool,
    ) -> Option<(TtEntry, bool)> {
        if self.mode == 0 {
            return None;
        }
        let saved = (if q {
            &self.previous.q
        } else {
            &self.previous.main
        })
        .get(&key)?;
        let can_cut = self.mode == 2 && saved.context == context;
        if self.mode == 2 {
            crate::optimization::count(|c| {
                if can_cut {
                    c.retained_bounds += 1
                } else {
                    c.rejected_bounds += 1
                }
            });
        }
        let mut entry = saved.entry;
        // Stored endpoint identities do not identify a multi-leg route uniquely.
        // Reuse a move hint only if a single complete current legal route matches.
        entry.best = entry.best.filter(|hint| {
            let Some(from) = Position::new(hint.0, hint.1) else {
                return false;
            };
            let Some(piece) = state.get_board().get_piece(from) else {
                return false;
            };
            piece.color == state.get_current_turn()
                && state
                    .generate_legal_moves_for_pieces(&[piece])
                    .iter()
                    .filter(|m| same_tt_move(m, *hint))
                    .count()
                    == 1
        });
        if entry.best.is_some() {
            crate::optimization::count(|c| c.retained_hints += 1);
        }
        (can_cut || entry.best.is_some()).then_some((entry, can_cut))
    }
    pub(super) fn store(&mut self, entry: TtEntry, context: u64, q: bool) {
        if self.mode == 0 {
            return;
        }
        let map = if q {
            &mut self.next.q
        } else {
            &mut self.next.main
        };
        if map.len() < MAX_RETAINED || map.contains_key(&entry.key) {
            map.insert(entry.key, RetainedEntry { entry, context });
        }
    }
}
impl Drop for ReuseSession {
    fn drop(&mut self) {
        if self.mode != 0 {
            RETAINED.with(|p| {
                let mut p = p.borrow_mut();
                // A nested search owns fresh storage and may have returned first.
                if p.is_none() {
                    *p = Some(std::mem::take(&mut self.next));
                }
            });
        }
    }
}

/// Conservative bound context, captured BEFORE the node searches children.
/// Includes full repetition history, draw state, traversal order, incremental
/// floating-point state, evaluation ply, windows, PV/null eligibility, ordering
/// heuristics and q-entry policy. A mismatch permits at most a validated hint.
/// Hashes have the same collision limitation as the existing Zobrist TT.
#[allow(clippy::too_many_arguments)]
pub(super) fn context(
    state: &GameState,
    ctx: &SearchContext,
    depth: u32,
    alpha: i32,
    beta: i32,
    is_pv: bool,
    q: Option<(Option<Position>, bool, bool)>,
) -> u64 {
    if crate::optimization::options().retain_tt != 2 {
        return 0;
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    state.hash_search_context(&mut h);
    (ctx.ply, depth, alpha, beta, is_pv, ctx.allow_null, q).hash(&mut h);
    (
        ctx.quiescence_depth,
        ctx.quiesce_entry_depth,
        ctx.last_ab_capture_enemy.to_bits(),
        ctx.last_ab_to,
        ctx.last_ab_wipe,
        ctx.last_ab_mover_large,
    )
        .hash(&mut h);
    ctx.killers.hash(&mut h);
    let mut history: Vec<_> = ctx.history.iter().collect();
    history.sort_unstable();
    history.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{optimization, piece::PieceType};
    struct OptionsGuard(optimization::Options);
    impl OptionsGuard {
        fn set(options: optimization::Options) -> Self {
            let old = optimization::options();
            optimization::configure(options);
            Self(old)
        }
    }
    impl Drop for OptionsGuard {
        fn drop(&mut self) {
            optimization::configure(self.0);
        }
    }
    fn fixture() -> GameState {
        let mut state = GameState::new();
        for (kind, color, f, r) in [
            (PieceType::King, Color::Black, 0, 0),
            (PieceType::King, Color::White, 35, 35),
            (PieceType::HookMover, Color::Black, 12, 12),
            (PieceType::Tengu, Color::White, 18, 18),
            (PieceType::FreeEagle, Color::Black, 16, 16),
            (PieceType::Pawn, Color::White, 17, 17),
            (PieceType::Pawn, Color::White, 18, 16),
            (PieceType::Pawn, Color::Black, 12, 14),
        ] {
            state.place_piece(Piece::new(kind, color, Position::new(f, r).unwrap()));
        }
        state
    }
    fn assert_mobility(state: &GameState) {
        for color in [Color::Black, Color::White] {
            for p in state.get_board().iter_pieces_by_color(color) {
                let mut expected = 0;
                if is_range_two_mover(p.piece_type) {
                    for cap in &MovementConfig::for_piece_type(p.piece_type).capabilities {
                        if let MovementCapability::TwoStep { first, second } = cap {
                            if matches!(**first, MovementCapability::Range { .. })
                                && matches!(**second, MovementCapability::Range { .. })
                            {
                                expected = MovementGenerator::capability_landings(
                                    &p,
                                    state.get_board(),
                                    first,
                                )
                                .len() as u32;
                                break;
                            }
                        }
                    }
                }
                assert_eq!(
                    crate::eval::first_leg_landing_count(&p, state.get_board()),
                    expected,
                    "{p:?}"
                );
            }
        }
    }
    #[test]
    fn mobility_invalidates_intermediate_captures_unmake_and_clone() {
        let _guard = OptionsGuard::set(optimization::Options {
            incremental_mobility: true,
            ..Default::default()
        });
        let mut state = fixture();
        assert_mobility(&state);
        let moves = state.generate_legal_moves();
        let mut found_path = false;
        for mv in moves
            .into_iter()
            .filter(|m| m.is_two_step() || m.is_free_eagle())
            .take(48)
        {
            found_path = true;
            if let Some(undo) = state.make_move_for_search(mv) {
                assert_mobility(&state);
                assert_mobility(&state.clone());
                state.unmake_move_for_search(undo);
                assert_mobility(&state);
            }
        }
        assert!(found_path);
        let mut changed = state
            .get_board()
            .get_piece(Position::new(12, 12).unwrap())
            .unwrap();
        changed.piece_type = PieceType::Tengu;
        changed.is_promoted = true;
        state.place_piece(changed);
        assert_mobility(&state);
    }
    #[test]
    fn retained_bounds_reject_context_and_model_changes() {
        let _guard = OptionsGuard::set(optimization::Options {
            retain_tt: 2,
            ..Default::default()
        });
        RETAINED.with(|p| *p.borrow_mut() = None);
        let weights = EvalWeights::seed();
        let config = SearchConfig::default();
        let state = fixture();
        let key = state.hash();
        let mut first = ReuseSession::acquire(&weights, &config);
        first.store(
            TtEntry {
                key,
                depth: 3,
                score: 17,
                bound: TtBound::Exact,
                best: None,
            },
            42,
            false,
        );
        drop(first);
        let second = ReuseSession::acquire(&weights, &config);
        assert!(second.probe(&state, key, 41, false).is_none());
        assert!(second.probe(&state, key, 42, false).unwrap().1);
        drop(second);
        let mut changed = weights.clone();
        changed.mate_score += 1;
        assert!(ReuseSession::acquire(&changed, &config)
            .previous
            .main
            .is_empty());
        RETAINED.with(|p| *p.borrow_mut() = None);
    }
    #[test]
    fn restricted_tt_first_keeps_depth_results_and_route_order() {
        let _guard = OptionsGuard::set(optimization::Options::default());
        let mut state = GameState::new();
        for (kind, color, f, r) in [
            (PieceType::King, Color::Black, 0, 0),
            (PieceType::King, Color::White, 35, 35),
            (PieceType::GoldGeneral, Color::Black, 2, 2),
            (PieceType::GoldGeneral, Color::White, 33, 33),
            (PieceType::Pawn, Color::Black, 4, 4),
            (PieceType::Pawn, Color::White, 31, 31),
        ] {
            state.place_piece(Piece::new(kind, color, Position::new(f, r).unwrap()));
        }
        let weights = EvalWeights::seed();
        let config = SearchConfig {
            depth: 3,
            quiescence_depth: 0,
            max_time_ms: None,
            ..Default::default()
        };
        let original = search(&state, &weights, &config);
        optimization::configure(optimization::Options {
            tt_first: 1,
            ..Default::default()
        });
        let variant = search(&state, &weights, &config);
        assert_eq!(
            (original.score, original.nodes, original.q_nodes),
            (variant.score, variant.nodes, variant.q_nodes)
        );
        assert_eq!(
            serde_json::to_value(original.root_lines).unwrap(),
            serde_json::to_value(variant.root_lines).unwrap()
        );
        assert!(optimization::counters().tt_first_probes > 0);
    }
}
