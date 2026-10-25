//! Opt-in October 2026 W512-v3 tournament arms. No network or material changes.
//! Keep policy decisions here; remove losing arms after the tournament decision.
use super::*;

const HISTORY_LIMIT: i32 = 16_384;
const HISTORY_ENTRIES: usize = 65_536;

impl SearchExperiment {
    pub(super) fn history(self) -> bool {
        matches!(self, Self::History | Self::HistoryTacticalLmr)
    }
    pub(super) fn tactical_lmr(self) -> bool {
        matches!(self, Self::TacticalLmr | Self::HistoryTacticalLmr)
    }
    pub(super) fn guarded_null(self) -> bool {
        self == Self::GuardedNull
    }
    pub(super) fn sacrifice(self) -> bool {
        matches!(self, Self::Sacrifice | Self::SacrificeThreatQ)
    }
    pub(super) fn threat_q(self) -> bool {
        matches!(self, Self::ThreatQ | Self::SacrificeThreatQ)
    }
}

#[derive(Default)]
pub(super) struct ExperimentState {
    pub policy: SearchExperiment,
    capture_history: HashMap<u64, i32>,
    pub quiet_entry: Option<QuietEntry>,
}

impl ExperimentState {
    pub fn new(policy: SearchExperiment) -> Self {
        Self {
            policy,
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct QuietEntry {
    from: Position,
    to: Position,
    via: Option<Position>,
}

impl QuietEntry {
    pub fn new(mv: &Move) -> Self {
        Self {
            from: mv.from,
            to: mv.to,
            via: mv.intermediate(),
        }
    }

    // A deliberately selective affected-ray/near-square filter. Two-leg routes
    // can change more remote attacks: missing those is allowed, inventing an
    // attack is not. Actual attack/capture checks follow this cheap filter.
    fn affects(self, target: Position) -> bool {
        [Some(self.from), Some(self.to), self.via]
            .into_iter()
            .flatten()
            .any(|p| {
                let dx = p.file.abs_diff(target.file);
                let dy = p.rank.abs_diff(target.rank);
                dx == 0 || dy == 0 || dx == dy || (dx <= 2 && dy <= 2)
            })
    }
}

pub(super) fn history_bonus(depth: u32) -> i32 {
    (depth.min(32) as i32).pow(2).saturating_mul(16).min(2048)
}

fn update_bounded<K: std::hash::Hash + Eq>(table: &mut HashMap<K, i32>, key: K, bonus: i32) {
    if table.len() >= HISTORY_ENTRIES && !table.contains_key(&key) {
        return;
    }
    let bonus = bonus.clamp(-HISTORY_LIMIT, HISTORY_LIMIT);
    let value = table.entry(key).or_default();
    *value += bonus - *value * bonus.abs() / HISTORY_LIMIT;
    *value = (*value).clamp(-HISTORY_LIMIT, HISTORY_LIMIT);
}

pub(super) fn update_quiet_history(ctx: &mut SearchContext, mv: &Move, bonus: i32) {
    update_bounded(&mut ctx.history, history_key(mv), bonus);
}

fn capture_key(state: &GameState, mv: &Move) -> Option<u64> {
    if !move_captures_enemy(state, mv) {
        return None;
    }
    let board = state.get_board();
    let mover = board.get_piece(mv.from)?;
    let victim = board
        .get_piece(mv.to)
        .map_or(0, |p| p.piece_type as u64 + 1);
    // Includes the intermediate square: different multi-leg captures must not
    // alias merely because their endpoints match. Piece types fit ten bits.
    Some(
        mv.to.to_index() as u64
            | ((mv.intermediate().map_or(1296, |p| p.to_index()) as u64) << 11)
            | ((mover.piece_type as u64) << 22)
            | (victim << 32)
            | (u64::from(mover.color == Color::White) << 42)
            | (u64::from(mv.promoted) << 43),
    )
}

pub(super) fn capture_history_score(ctx: &SearchContext, state: &GameState, mv: &Move) -> i32 {
    if !ctx.experiments.policy.history() {
        return 0;
    }
    capture_key(state, mv)
        .and_then(|key| ctx.experiments.capture_history.get(&key).copied())
        .unwrap_or(0)
        / 16
}

pub(super) fn record_cutoff(
    state: &GameState,
    ctx: &mut SearchContext,
    winner: &Move,
    depth: u32,
    searched: &[(Move, i32)],
) {
    if !ctx.experiments.policy.history() || ctx.abort {
        return;
    }
    let bonus = history_bonus(depth);
    for (mv, _) in searched {
        let delta = if mv == winner { bonus } else { -bonus / 2 };
        if let Some(key) = capture_key(state, mv) {
            update_bounded(&mut ctx.experiments.capture_history, key, delta);
        } else {
            update_quiet_history(ctx, mv, delta);
        }
    }
}

#[derive(Default)]
pub(super) struct QuietThreats {
    threatened: Vec<Position>,
    royal_targets: Vec<Position>,
}

pub(super) fn threatened_majors(
    state: &GameState,
    weights: &EvalWeights,
    mv: &Move,
) -> QuietThreats {
    let us = state.get_current_turn();
    let entry = QuietEntry::new(mv);
    let board = state.get_board();
    QuietThreats {
        threatened: board
            .iter_pieces_by_color(us)
            .filter(|p| {
                is_large_hang_victim(p, weights)
                    && entry.affects(p.position)
                    && attacked_memo(state, p.position, us.opposite())
            })
            .map(|p| p.position)
            .collect(),
        royal_targets: board
            .iter_pieces_by_color(us.opposite())
            .filter(|p| {
                p.piece_type.is_royal()
                    && entry.affects(p.position)
                    && !attacked_memo(state, p.position, us)
            })
            .map(|p| p.position)
            .collect(),
    }
}

pub(super) fn protect_quiet(state: &GameState, mv: &Move, before: &QuietThreats) -> bool {
    let us = state.get_current_turn().opposite();
    before
        .royal_targets
        .iter()
        .any(|&target| attacked_memo(state, target, us))
        || before.threatened.iter().any(|&target| {
            let target = if target == mv.from { mv.to } else { target };
            state
                .get_board()
                .get_piece(target)
                .is_some_and(|p| p.color == us)
                && !attacked_memo(state, target, us.opposite())
        })
}

/// Admit tactical q after a quiet move without changing its candidate filters.
/// This detects an available affected-square threat, not a complete delta of all
/// before/after attacks. In particular an already-available equal-value exchange
/// may qualify; unlike the old hang gate the attacker need not be cheaper.
pub(super) fn quiet_leaf_has_major_capture(
    state: &GameState,
    weights: &EvalWeights,
    entry: QuietEntry,
) -> bool {
    let us = state.get_current_turn();
    state
        .get_board()
        .iter_pieces_by_color(us.opposite())
        .any(|p| {
            entry.affects(p.position)
                && (is_large_hang_victim(&p, weights) || p.piece_type.is_royal())
                && (attacked_memo(state, p.position, us)
                    || peacock_might_directly_capture(state.get_board(), us, p.position))
                && generate_captures_hitting_square(state, p.position)
                    .iter()
                    .any(|mv| mv.to == p.position)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::piece::PieceType;

    fn position(file: u8, rank: u8) -> Position {
        Position::new(file, rank).unwrap()
    }
    fn put(state: &mut GameState, kind: PieceType, color: Color, file: u8, rank: u8) {
        state.place_piece(Piece::new(kind, color, position(file, rank)));
    }
    fn kings() -> GameState {
        let mut state = GameState::new();
        put(&mut state, PieceType::King, Color::Black, 1, 1);
        put(&mut state, PieceType::King, Color::White, 34, 34);
        state
    }

    #[test]
    fn checkpoint_policies_reach_players_and_reject_unknown_names() {
        let old: crate::eval::SearchDefaults =
            serde_json::from_str(r#"{"depth":2,"max_time_ms":null}"#).unwrap();
        assert_eq!(old.experiment, SearchExperiment::Baseline);
        for policy in SearchExperiment::ALL {
            let mut cp = crate::eval::EvalCheckpoint::seed("search-test");
            cp.search_defaults.experiment = policy;
            let cp = serde_json::from_str(&serde_json::to_string(&cp).unwrap()).unwrap();
            let player = crate::alphabeta_player::AlphaBetaPlayer::from_checkpoint(cp);
            assert_eq!(player.config().experiment, policy);
        }
        assert!(serde_json::from_str::<crate::eval::SearchDefaults>(
            r#"{"depth":2,"max_time_ms":null,"experiment":"typo"}"#
        )
        .is_err());
    }

    #[test]
    fn bounded_history_learns_successes_and_failures_without_unbounded_storage() {
        let mut table = HashMap::new();
        for _ in 0..1000 {
            update_bounded(&mut table, 1, 2048);
        }
        assert!(table[&1] > 0 && table[&1] <= HISTORY_LIMIT);
        for _ in 0..1000 {
            update_bounded(&mut table, 1, -2048);
        }
        assert!(table[&1] < 0 && table[&1] >= -HISTORY_LIMIT);
        for key in 0..HISTORY_ENTRIES + 10 {
            update_bounded(&mut table, key, 100);
        }
        assert_eq!(table.len(), HISTORY_ENTRIES);
    }

    #[test]
    fn tactical_quiet_protection_requires_rescue_or_new_royal_attack() {
        let mut state = kings();
        let mut weights = EvalWeights::seed();
        weights.piece.insert(PieceType::Rook, 1000.0);
        weights.rebuild_piece_value_table();
        put(&mut state, PieceType::Rook, Color::Black, 10, 10);
        put(&mut state, PieceType::Rook, Color::White, 10, 15);
        let mv = Move::new(position(10, 10), position(11, 10));
        let before = threatened_majors(&state, &weights, &mv);
        assert!(before.threatened.contains(&mv.from));
        let undo = state.make_move_for_search(mv.clone()).unwrap();
        assert!(protect_quiet(&state, &mv, &before));
        state.unmake_move_for_search(undo);
        let not_saved = Move::new(position(10, 10), position(10, 11));
        let before = threatened_majors(&state, &weights, &not_saved);
        let undo = state.make_move_for_search(not_saved.clone()).unwrap();
        assert!(!protect_quiet(&state, &not_saved, &before));
        state.unmake_move_for_search(undo);

        let mut royal = kings();
        put(&mut royal, PieceType::Rook, Color::Black, 12, 10);
        let mv = Move::new(position(12, 10), position(34, 10));
        let before = threatened_majors(&royal, &weights, &mv);
        royal.make_move_for_search(mv.clone()).unwrap();
        assert!(protect_quiet(&royal, &mv, &before));
    }

    #[test]
    fn threat_entry_verifies_capture_and_scout_horizon_is_opt_in() {
        let mut state = kings();
        let mut weights = EvalWeights::seed();
        weights.piece.insert(PieceType::Rook, 1000.0);
        weights.rebuild_piece_value_table();
        put(&mut state, PieceType::Rook, Color::Black, 10, 10);
        put(&mut state, PieceType::Rook, Color::White, 10, 15);
        let entry = QuietEntry::new(&Move::new(position(10, 13), position(11, 13)));
        assert!(quiet_leaf_has_major_capture(&state, &weights, entry));
        put(&mut state, PieceType::Pawn, Color::Black, 10, 12);
        assert!(!quiet_leaf_has_major_capture(&state, &weights, entry));
        with_quiesce_test_context(&state, &weights, 2, None, |_, ctx| {
            assert_eq!(leaf_quiescence_depth(ctx, false), 1);
            ctx.experiments.policy = SearchExperiment::ScoutQ;
            assert_eq!(leaf_quiescence_depth(ctx, false), 2);
            assert_eq!(leaf_quiescence_depth(ctx, true), 2);
            ctx.quiescence_depth = 0;
            assert_eq!(leaf_quiescence_depth(ctx, false), 0);
        });
    }

    #[test]
    fn qfix_resolves_recursive_check_before_zero_budget_stand_pat() {
        let mut state = GameState::new();
        put(&mut state, PieceType::King, Color::Black, 0, 0);
        put(&mut state, PieceType::King, Color::White, 35, 35);
        put(&mut state, PieceType::Pawn, Color::White, 34, 35);
        put(&mut state, PieceType::Lance, Color::White, 35, 34);
        put(&mut state, PieceType::Bishop, Color::White, 34, 34);
        put(&mut state, PieceType::GoldGeneral, Color::White, 10, 10);
        put(&mut state, PieceType::Knight, Color::Black, 34, 33);
        state.set_current_turn(Color::White);
        let weights = EvalWeights::seed();
        assert!(last_royal_evasions(&mut state).unwrap().is_empty());
        with_quiesce_test_context(&state, &weights, 0, None, |pos, ctx| {
            ctx.experiments.policy = SearchExperiment::QFix;
            let score = quiesce(pos, &weights, 0, -900_000, 900_000, None, true, true, ctx);
            assert_eq!(score, -weights.mate_score);
            assert_eq!(pos.hash(), state.hash());
        });
    }

    #[test]
    fn guarded_null_searches_real_moves_on_pv_instead_of_passing() {
        let state = kings();
        let weights = EvalWeights::seed();
        let nodes = |policy| {
            with_quiesce_test_context(&state, &weights, 0, None, |pos, ctx| {
                ctx.experiments.policy = policy;
                alphabeta(pos, &weights, 2, -200_000, -100_000, true, ctx);
                ctx.nodes
            })
        };
        assert!(nodes(SearchExperiment::GuardedNull) > nodes(SearchExperiment::Baseline));
    }

    #[test]
    fn all_policies_return_legal_moves_without_mutating_the_root() {
        let mut state = kings();
        put(&mut state, PieceType::GoldGeneral, Color::Black, 10, 10);
        put(&mut state, PieceType::GoldGeneral, Color::White, 10, 12);
        let weights = EvalWeights::seed();
        let hash = state.hash();
        let legal = state.generate_legal_moves();
        for policy in SearchExperiment::ALL {
            let result = search(
                &state,
                &weights,
                &SearchConfig {
                    depth: 2,
                    collect_trace: false,
                    experiment: policy,
                    ..SearchConfig::default()
                },
            );
            assert_eq!(result.completed_depth, 2, "{policy:?}");
            assert!(legal.contains(&result.best_move.unwrap()), "{policy:?}");
            assert_eq!(state.hash(), hash);
        }
    }

    #[test]
    fn sacrifice_enters_pv_search_but_not_baseline_or_non_pv() {
        let mut state = kings();
        let weights = EvalWeights::seed();
        put(&mut state, PieceType::GreatGeneral, Color::Black, 10, 10);
        put(&mut state, PieceType::Pawn, Color::White, 10, 12);
        put(&mut state, PieceType::GoldGeneral, Color::White, 10, 15);
        let mv = Move::new(position(10, 10), position(10, 14));
        assert!(capture_hangs_high_value_piece(
            &state,
            &weights,
            &mv,
            false,
            &mut LandingAttackCache::new()
        ));
        for (policy, pv, searched) in [
            (SearchExperiment::Baseline, true, false),
            (SearchExperiment::Sacrifice, false, false),
            (SearchExperiment::Sacrifice, true, true),
        ] {
            with_quiesce_test_context(&state, &weights, 0, None, |pos, ctx| {
                ctx.experiments.policy = policy;
                let (score, _, _, _) = search_move_list(
                    pos,
                    &weights,
                    1,
                    -900_000,
                    900_000,
                    pv,
                    ctx,
                    0,
                    std::slice::from_ref(&mv),
                    0,
                );
                assert_eq!(score != i32::MIN + 1, searched);
                assert_eq!(pos.hash(), state.hash());
            });
        }
    }
}
