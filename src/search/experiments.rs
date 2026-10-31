//! Opt-in October 2026 W512-v3 tournament arms. No network or material changes.
//! Keep policy decisions here; remove losing arms after the tournament decision.
use super::*;

const HISTORY_LIMIT: i32 = 16_384;
const HISTORY_ENTRIES: usize = 65_536;

impl SearchExperiment {
    pub(super) fn history(self) -> bool {
        matches!(self, Self::History | Self::HistoryTacticalLmr) || self.second_wave_hl()
    }
    pub(super) fn tactical_lmr(self) -> bool {
        matches!(self, Self::TacticalLmr | Self::HistoryTacticalLmr) || self.second_wave_hl()
    }
    pub(super) fn guarded_null(self) -> bool {
        matches!(
            self,
            Self::GuardedNull
                | Self::QFixNull
                | Self::HistoryTacticalLmrQFixNull
                | Self::HistoryTacticalLmrQFixNullVerify
        )
    }
    pub(super) fn sacrifice(self) -> bool {
        matches!(
            self,
            Self::Sacrifice | Self::SacrificeThreatQ | Self::HistoryTacticalLmrQFixSacrifice
        )
    }
    pub(super) fn threat_q(self) -> bool {
        matches!(self, Self::ThreatQ | Self::SacrificeThreatQ)
    }
    fn second_wave_hl(self) -> bool {
        matches!(
            self,
            Self::HistoryTacticalLmrQFix
                | Self::HistoryTacticalLmrQFixNull
                | Self::HistoryTacticalLmrQFixContinuation
                | Self::HistoryTacticalLmrQFixHistoryReduction
                | Self::HistoryTacticalLmrQFixRecapture
                | Self::HistoryTacticalLmrQFixSacrifice
                | Self::HistoryTacticalLmrQFixNullVerify
        )
    }
    pub(super) fn continuation(self) -> bool {
        self == Self::HistoryTacticalLmrQFixContinuation
    }
    pub(super) fn history_reduction(self) -> bool {
        self == Self::HistoryTacticalLmrQFixHistoryReduction
    }
    pub(super) fn recapture_extension(self) -> bool {
        self == Self::HistoryTacticalLmrQFixRecapture
    }
    pub(super) fn verify_null(self) -> bool {
        self == Self::HistoryTacticalLmrQFixNullVerify
    }
}

impl SearchTrial {
    pub(super) fn active(self) -> bool {
        self != Self::None
    }
    pub(super) fn q_refill(self) -> bool {
        matches!(self, Self::QRefill | Self::QRefillHistory)
    }
    pub(super) fn q_history(self) -> bool {
        matches!(self, Self::QHistory | Self::QRefillHistory)
    }
    pub(super) fn internal_pvs(self) -> bool {
        matches!(self, Self::InternalPvs | Self::PvsPromote)
    }
    pub(super) fn promote_pv(self) -> bool {
        matches!(self, Self::PromotePv | Self::PvsPromote)
    }
}

/// Candidate incoming leaf policy. Descendant AB searches overwrite these fields;
/// third-wave trials restore them at the recursive boundary before a retry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct IncomingLeaf {
    enemy: f32,
    to: Option<Position>,
    wipe: bool,
    mover_large: bool,
    entry_depth: u32,
}
impl IncomingLeaf {
    pub fn read(ctx: &SearchContext) -> Self {
        Self {
            enemy: ctx.last_ab_capture_enemy,
            to: ctx.last_ab_to,
            wipe: ctx.last_ab_wipe,
            mover_large: ctx.last_ab_mover_large,
            entry_depth: ctx.quiesce_entry_depth,
        }
    }
    pub fn restore(self, ctx: &mut SearchContext) {
        ctx.last_ab_capture_enemy = self.enemy;
        ctx.last_ab_to = self.to;
        ctx.last_ab_wipe = self.wipe;
        ctx.last_ab_mover_large = self.mover_large;
        ctx.quiesce_entry_depth = self.entry_depth;
    }
}

#[derive(Default)]
pub(super) struct ExperimentState {
    pub policy: SearchExperiment,
    pub trial: SearchTrial,
    capture_history: HashMap<u64, i32>,
    pub quiet_entry: Option<QuietEntry>,
    // Second-wave CH: one tagged, direct-mapped 1 MiB table; never dense board².
    continuation: Vec<ContinuationEntry>,
    pub previous_move: Option<u64>,
    // Second-wave HR: support is bounded by the existing quiet-history key cap.
    quiet_support: HashMap<u32, u8>,
    pub recapture_spent: bool,
    pub verifying_null: bool,
    pub recapture_extensions: u64,
    pub null_verifications: u64,
}

impl ExperimentState {
    pub fn with_trial(mut self, trial: SearchTrial) -> Self {
        self.trial = trial;
        self
    }
    pub fn new(policy: SearchExperiment) -> Self {
        Self {
            policy,
            continuation: if policy.continuation() {
                vec![ContinuationEntry::default(); HISTORY_ENTRIES]
            } else {
                Vec::new()
            },
            ..Self::default()
        }
    }
}

// Hash collisions can affect ordering, never legality or TT scores. Tags reject
// index collisions; all full-route coordinates/promotion and mover identity enter
// the fingerprint. A different two-step/FreeEagle route gets its own hint.
#[derive(Clone, Copy, Default)]
struct ContinuationEntry {
    key: u64,
    value: i32,
}

fn mix(key: u64, value: u64) -> u64 {
    let mut x = key.wrapping_add(value).wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

fn route_key(state: &GameState, mv: &Move) -> u64 {
    let mut key = mix(mv.from.to_index() as u64, mv.to.to_index() as u64);
    key = mix(key, u64::from(mv.promoted));
    if let Some(piece) = state.get_board().get_piece(mv.from) {
        key = mix(key, piece.piece_type as u64);
        key = mix(key, u64::from(piece.color == Color::White));
    }
    match &mv.data {
        crate::game_state::MoveData::Standard => mix(key, 0),
        crate::game_state::MoveData::TwoStep { intermediate } => {
            mix(mix(key, 1), intermediate.to_index() as u64)
        }
        crate::game_state::MoveData::FreeEagle { path } => {
            key = mix(mix(key, 2), path.len() as u64);
            for square in path {
                key = mix(key, square.to_index() as u64);
            }
            key
        }
    }
}

pub(super) fn incoming_move(state: &GameState, mv: &Move, ctx: &SearchContext) -> Option<u64> {
    ctx.experiments
        .policy
        .continuation()
        .then(|| route_key(state, mv))
}

fn continuation_key(state: &GameState, mv: &Move, ctx: &SearchContext) -> Option<u64> {
    ctx.experiments
        .previous_move
        .map(|previous| mix(previous, route_key(state, mv)))
}

pub(super) fn continuation_score(ctx: &SearchContext, state: &GameState, mv: &Move) -> i32 {
    if !ctx.experiments.policy.continuation() || move_captures_enemy(state, mv) {
        return 0;
    }
    let Some(key) = continuation_key(state, mv, ctx) else {
        return 0;
    };
    let Some(entry) = ctx
        .experiments
        .continuation
        .get(key as usize & (HISTORY_ENTRIES - 1))
    else {
        return 0;
    };
    if entry.key == key {
        entry.value
    } else {
        0
    }
}

fn update_continuation(ctx: &mut SearchContext, state: &GameState, mv: &Move, bonus: i32) {
    let Some(key) = continuation_key(state, mv, ctx) else {
        return;
    };
    let entry = &mut ctx.experiments.continuation[key as usize & (HISTORY_ENTRIES - 1)];
    if entry.key != key {
        *entry = ContinuationEntry { key, value: 0 };
    }
    entry.value += bonus - entry.value * bonus.abs() / HISTORY_LIMIT;
    entry.value = entry.value.clamp(-HISTORY_LIMIT, HISTORY_LIMIT);
}

pub(super) fn age_continuation(ctx: &mut SearchContext) {
    for entry in &mut ctx.experiments.continuation {
        entry.value /= 2;
    }
}

pub(super) fn protect_history(ctx: &SearchContext, mv: &Move) -> bool {
    ctx.experiments.policy.history_reduction()
        && ctx
            .experiments
            .quiet_support
            .get(&history_key(mv))
            .copied()
            .unwrap_or(0)
            >= 2
        && history_score(ctx, mv) >= 512
}

/// Isolate real-move null verification from scores obtained with passing enabled.
pub(super) fn verification_key(ctx: &SearchContext) -> u64 {
    if ctx.experiments.verifying_null {
        0x6e75_6c6c_7665_7269
    } else {
        0
    }
}

pub(super) fn has_major_recapture(
    state: &GameState,
    weights: &EvalWeights,
    prev_to: Option<Position>,
) -> bool {
    prev_to_is_major_enemy(state, weights, prev_to)
        && prev_to.is_some_and(|target| {
            generate_captures_hitting_square(state, target)
                .iter()
                .any(|mv| mv.to == target)
        })
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
                let dx = p.file().abs_diff(target.file());
                let dy = p.rank().abs_diff(target.rank());
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
            if ctx.experiments.policy.continuation() {
                update_continuation(ctx, state, mv, delta);
            }
            if ctx.experiments.policy.history_reduction() && mv == winner {
                let key = history_key(mv);
                if ctx.experiments.quiet_support.len() < HISTORY_ENTRIES
                    || ctx.experiments.quiet_support.contains_key(&key)
                {
                    let support = ctx.experiments.quiet_support.entry(key).or_default();
                    *support = support.saturating_add(1);
                }
            }
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

/// Third-wave ATK: only previously unattacked major targets on affected geometry.
/// Geometry is a prescreen, never evidence of an attack; the post-move oracle verifies it.
pub(super) fn new_major_targets(
    state: &GameState,
    weights: &EvalWeights,
    mv: &Move,
) -> Vec<Position> {
    let us = state.get_current_turn();
    let entry = QuietEntry::new(mv);
    state
        .get_board()
        .iter_pieces_by_color(us.opposite())
        .filter(|piece| {
            is_large_hang_victim(piece, weights)
                && !piece.piece_type.is_royal()
                && entry.affects(piece.position)
                && !attacked_memo(state, piece.position, us)
        })
        .map(|piece| piece.position)
        .collect()
}

pub(super) fn creates_major_threat(state: &GameState, targets: &[Position]) -> bool {
    let us = state.get_current_turn().opposite();
    targets.iter().any(|&target| {
        state
            .get_board()
            .get_piece(target)
            .is_some_and(|piece| piece.color == us.opposite())
            && attacked_memo(state, target, us)
    })
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
    fn third_wave_trials_round_trip_without_replacing_hlf_or_hlfs() {
        let old: crate::eval::SearchDefaults =
            serde_json::from_str(r#"{"depth":2,"max_time_ms":null}"#).unwrap();
        assert_eq!(old.trial, SearchTrial::None);
        for foundation in [
            SearchExperiment::HistoryTacticalLmrQFix,
            SearchExperiment::HistoryTacticalLmrQFixSacrifice,
        ] {
            for trial in SearchTrial::ALL {
                let mut checkpoint = crate::eval::EvalCheckpoint::seed("third-wave-test");
                checkpoint.search_defaults.experiment = foundation;
                checkpoint.search_defaults.trial = trial;
                let checkpoint =
                    serde_json::from_str(&serde_json::to_string(&checkpoint).unwrap()).unwrap();
                let player = crate::alphabeta_player::AlphaBetaPlayer::from_checkpoint(checkpoint);
                assert_eq!(player.config().experiment, foundation);
                assert_eq!(player.config().trial, trial);
            }
        }
        assert!(serde_json::from_str::<crate::eval::SearchDefaults>(
            r#"{"depth":2,"max_time_ms":null,"trial":"typo"}"#
        )
        .is_err());
    }

    #[test]
    fn context_trial_restores_real_descendant_overwrite_before_retries() {
        let mut state = kings();
        put(&mut state, PieceType::Pawn, Color::Black, 10, 10);
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        let run = |trial| {
            with_quiesce_test_context(&state, &weights, 0, None, |pos, ctx| {
                ctx.experiments.policy = SearchExperiment::HistoryTacticalLmrQFix;
                ctx.experiments.trial = trial;
                ctx.allow_null = false;
                ctx.last_ab_capture_enemy = 77.0;
                ctx.last_ab_to = Some(position(20, 20));
                ctx.last_ab_wipe = true;
                ctx.last_ab_mover_large = true;
                ctx.quiesce_entry_depth = 7;
                let incoming = IncomingLeaf::read(ctx);
                alphabeta(pos, &weights, 1, -900_000, 900_000, true, ctx);
                assert_eq!(pos.hash(), state.hash());
                (incoming, IncomingLeaf::read(ctx))
            })
        };
        let legacy = run(SearchTrial::None);
        assert_ne!(
            legacy.0, legacy.1,
            "ordinary descendant search really changes incoming leaf policy"
        );
        let restored = run(SearchTrial::ContextRetry);
        assert_eq!(restored.0, restored.1);
    }

    fn refill_position() -> (GameState, EvalWeights) {
        let mut state = kings();
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.piece.insert(PieceType::Rook, 2000.0);
        weights.piece.insert(PieceType::Bishop, 1000.0);
        weights.rebuild_piece_value_table();
        for file in [5, 15, 25] {
            put(&mut state, PieceType::Rook, Color::Black, file, 5);
            put(&mut state, PieceType::Bishop, Color::White, file, 8);
        }
        for file in [5, 15] {
            put(&mut state, PieceType::Rook, Color::White, file, 10);
        }
        (state, weights)
    }

    #[test]
    fn refill_reaches_second_path_after_post_fire_hang_rejects_first() {
        let mut state = GameState::new();
        for (kind, color, file, rank) in [
            (PieceType::King, Color::Black, 1, 2),
            (PieceType::CrownPrince, Color::Black, 0, 2),
            (PieceType::King, Color::White, 34, 33),
            (PieceType::CrownPrince, Color::White, 35, 33),
        ] {
            put(&mut state, kind, color, file, rank);
        }
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        for (kind, value) in [
            (PieceType::GreatGeneral, 5000.0),
            (PieceType::Peacock, 1000.0),
            (PieceType::Bishop, 900.0),
            (PieceType::Rook, 2000.0),
            (PieceType::Pawn, 2.0),
        ] {
            weights.piece.insert(kind, value);
        }
        weights.rebuild_piece_value_table();
        for file in [5, 16] {
            put(&mut state, PieceType::GreatGeneral, Color::Black, file, 5);
            put(&mut state, PieceType::Pawn, Color::Black, file, 6);
            put(&mut state, PieceType::Pawn, Color::White, file, 7);
            put(&mut state, PieceType::Peacock, Color::White, file, 8);
        }
        put(&mut state, PieceType::Rook, Color::White, 5, 10);
        put(&mut state, PieceType::Rook, Color::Black, 25, 5);
        put(&mut state, PieceType::Bishop, Color::White, 25, 8);
        // Different square colors prevent the first Peacock from defending the second.
        for (file, attacked) in [(5, true), (16, false)] {
            let mv = Move::new(position(file, 5), position(file, 8));
            let undo = state
                .make_move_for_search_with_weights(mv, &weights)
                .unwrap();
            assert_eq!(
                state
                    .get_board()
                    .is_position_attacked_by_color(position(file, 8), Color::White),
                attacked
            );
            state.unmake_move_for_search(undo);
        }
        let run = |trial| {
            with_quiesce_test_context(&state, &weights, 1, None, |pos, ctx| {
                ctx.experiments.policy = SearchExperiment::HistoryTacticalLmrQFix;
                ctx.experiments.trial = trial;
                let score = quiesce(pos, &weights, 1, -900_000, 900_000, None, true, true, ctx);
                assert_eq!(pos.hash(), state.hash());
                (score, ctx.q_caps_searched)
            })
        };
        let legacy = run(SearchTrial::None);
        let refill = run(SearchTrial::QRefill);
        assert!(legacy.1 > 0);
        assert!(
            refill.0 > legacy.0,
            "refill should search the larger surviving path take: {legacy:?} vs {refill:?}"
        );
    }

    #[test]
    fn q_history_promotes_equal_victim_capture_without_expanding_quota() {
        let (state, weights) = refill_position();
        with_quiesce_test_context(&state, &weights, 1, None, |pos, ctx| {
            ctx.experiments.policy = SearchExperiment::HistoryTacticalLmrQFix;
            ctx.experiments.trial = SearchTrial::QHistory;
            let wanted = Move::new(position(25, 5), position(25, 8));
            ctx.experiments
                .capture_history
                .insert(capture_key(pos, &wanted).unwrap(), HISTORY_LIMIT);
            let key = q_tt_key(pos, None, 1, true, true, ctx);
            let stand = evaluate_with_ply(pos, &weights, ctx.ply);
            let score = quiesce(pos, &weights, 1, -900_000, 900_000, None, true, true, ctx);
            assert!(score > stand);
            assert_eq!(
                ctx.q_tt.probe(key).unwrap().best,
                Some(move_tt_key(&wanted))
            );
            assert!(ctx.q_caps_searched <= QUIESCE_TOP_N_PATH_AWARE_ROOT as u64);
            assert_eq!(pos.hash(), state.hash());
        });
    }

    #[test]
    fn pre_move_attack_oracle_does_not_count_a_defender_of_friendly_occupied_destination() {
        let (mut state, weights) = refill_position();
        let target = position(5, 8);
        assert!(!state
            .get_board()
            .is_position_attacked_by_color(target, Color::White));
        let undo = state
            .make_move_for_search_with_weights(Move::new(position(5, 5), target), &weights)
            .unwrap();
        assert!(state
            .get_board()
            .is_position_attacked_by_color(target, Color::White));
        state.unmake_move_for_search(undo);
    }

    #[test]
    fn post_capture_safety_rejects_real_defended_takes_and_restores_board() {
        let (state, weights) = refill_position();
        let run = |trial, prev_to| {
            with_quiesce_test_context(&state, &weights, 1, prev_to, |pos, ctx| {
                ctx.experiments.policy = SearchExperiment::HistoryTacticalLmrQFix;
                ctx.experiments.trial = trial;
                let stand = evaluate_with_ply(pos, &weights, 0);
                let score = quiesce(
                    pos, &weights, 1, -900_000, 900_000, prev_to, true, true, ctx,
                );
                assert_eq!(pos.hash(), state.hash());
                assert_eq!(
                    pos.get_board()
                        .get_piece(position(5, 8))
                        .unwrap()
                        .piece_type,
                    PieceType::Bishop
                );
                (score, stand, ctx.q_caps_searched)
            })
        };
        let old = run(SearchTrial::None, None);
        let safety = run(SearchTrial::PostCaptureSafety, None);
        assert_eq!(old.2, 2);
        assert!(old.0 > old.1);
        assert_eq!(
            safety.2, 0,
            "ordinary top-two defended takes must be rejected after make"
        );
        assert_eq!(safety.0, safety.1);
        let mandatory = run(SearchTrial::PostCaptureSafety, Some(position(5, 8)));
        assert!(
            mandatory.2 > 0,
            "mandatory major recapture must remain exempt even when defended"
        );
    }

    #[test]
    fn post_capture_safety_never_prunes_a_defended_royal_take() {
        let mut state = GameState::new();
        put(&mut state, PieceType::King, Color::Black, 1, 1);
        put(&mut state, PieceType::King, Color::White, 5, 8);
        put(&mut state, PieceType::Rook, Color::Black, 5, 5);
        put(&mut state, PieceType::Rook, Color::White, 5, 10);
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.piece.insert(PieceType::Rook, 2000.0);
        weights.rebuild_piece_value_table();
        with_quiesce_test_context(&state, &weights, 1, None, |pos, ctx| {
            ctx.experiments.policy = SearchExperiment::HistoryTacticalLmrQFix;
            ctx.experiments.trial = SearchTrial::PostCaptureSafety;
            let score = quiesce(pos, &weights, 1, -900_000, 900_000, None, true, true, ctx);
            assert!(score >= MATE_SCORE_BAND);
            assert_eq!(ctx.q_caps_searched, 1);
            assert_eq!(pos.hash(), state.hash());
        });
    }

    #[test]
    fn shallow_futility_saves_nodes_but_never_hides_a_royal_capture() {
        let mut state = kings();
        put(&mut state, PieceType::Rook, Color::Black, 10, 10);
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        let run = |state: &GameState, trial| {
            with_quiesce_test_context(state, &weights, 0, None, |pos, ctx| {
                ctx.experiments.policy = SearchExperiment::HistoryTacticalLmrQFix;
                ctx.experiments.trial = trial;
                ctx.allow_null = false;
                ctx.last_ab_capture_enemy = 0.0;
                let beta = evaluate_with_ply(pos, &weights, 0) - 1000;
                let score = alphabeta(pos, &weights, 1, beta - 1, beta, false, ctx);
                (score, beta, ctx.nodes)
            })
        };
        let legacy = run(&state, SearchTrial::None);
        let trial = run(&state, SearchTrial::ShallowFutility);
        assert_eq!(trial.0, trial.1);
        assert!(trial.2 < legacy.2);
        put(&mut state, PieceType::Rook, Color::Black, 34, 10);
        let winning = run(&state, SearchTrial::ShallowFutility);
        assert!(
            winning.0 >= MATE_SCORE_BAND,
            "royal capture must be searched"
        );
        assert!(winning.2 > 1);
    }

    #[test]
    fn promoted_pv_does_not_reuse_a_scout_exact_bound() {
        let state = kings();
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        let run = |trial, pv| {
            with_quiesce_test_context(&state, &weights, 0, None, |pos, ctx| {
                ctx.experiments.policy = SearchExperiment::HistoryTacticalLmrQFixSacrifice;
                ctx.experiments.trial = trial;
                ctx.allow_null = false;
                ctx.tt.store(TtEntry {
                    key: position_hash(pos),
                    depth: 1,
                    score: 123_456,
                    bound: TtBound::Exact,
                    best: None,
                });
                alphabeta(pos, &weights, 1, -900_000, 900_000, pv, ctx)
            })
        };
        assert_eq!(run(SearchTrial::ContextRetry, true), 123_456);
        assert_eq!(run(SearchTrial::PromotePv, false), 123_456);
        assert_ne!(run(SearchTrial::PromotePv, true), 123_456);
    }

    #[test]
    fn interior_pvs_preserves_unpruned_small_tree_value() {
        let mut state = kings();
        put(&mut state, PieceType::Rook, Color::Black, 10, 10);
        put(&mut state, PieceType::Pawn, Color::White, 10, 12);
        put(&mut state, PieceType::Rook, Color::White, 15, 12);
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        let run = |trial| {
            with_quiesce_test_context(&state, &weights, 0, None, |pos, ctx| {
                ctx.experiments.policy = SearchExperiment::HistoryTacticalLmrQFix;
                ctx.experiments.trial = trial;
                ctx.allow_null = false;
                let score = alphabeta(pos, &weights, 2, -900_000, 900_000, true, ctx);
                assert_eq!(pos.hash(), state.hash());
                score
            })
        };
        assert_eq!(
            run(SearchTrial::InternalPvs),
            run(SearchTrial::ContextRetry)
        );
    }

    #[test]
    fn major_threat_protection_requires_new_actual_attack() {
        let mut state = kings();
        let mut weights = EvalWeights::seed();
        weights.piece.insert(PieceType::Rook, 1000.0);
        weights.rebuild_piece_value_table();
        put(&mut state, PieceType::Rook, Color::Black, 10, 10);
        put(&mut state, PieceType::Rook, Color::White, 20, 15);
        let checking_line = Move::new(position(10, 10), position(20, 10));
        let targets = new_major_targets(&state, &weights, &checking_line);
        assert!(targets.contains(&position(20, 15)));
        let undo = state
            .make_move_for_search_with_weights(checking_line, &weights)
            .unwrap();
        assert!(creates_major_threat(&state, &targets));
        state.unmake_move_for_search(undo);
        let harmless = Move::new(position(10, 10), position(11, 10));
        let targets = new_major_targets(&state, &weights, &harmless);
        let undo = state
            .make_move_for_search_with_weights(harmless, &weights)
            .unwrap();
        assert!(!creates_major_threat(&state, &targets));
        state.unmake_move_for_search(undo);
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
                let (score, _, _, _, _) = search_move_list(
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
    #[test]
    fn second_wave_compositions_keep_their_components_explicit() {
        for policy in [
            SearchExperiment::HistoryTacticalLmrQFix,
            SearchExperiment::HistoryTacticalLmrQFixNull,
            SearchExperiment::HistoryTacticalLmrQFixContinuation,
            SearchExperiment::HistoryTacticalLmrQFixHistoryReduction,
            SearchExperiment::HistoryTacticalLmrQFixRecapture,
            SearchExperiment::HistoryTacticalLmrQFixSacrifice,
            SearchExperiment::HistoryTacticalLmrQFixNullVerify,
        ] {
            assert!(policy.history());
            assert!(policy.tactical_lmr());
            assert!(!policy.threat_q());
        }
        assert!(SearchExperiment::QFixNull.guarded_null());
        assert!(!SearchExperiment::QFixNull.history());
        assert!(!SearchExperiment::QFixNull.tactical_lmr());
        assert!(!SearchExperiment::HistoryTacticalLmrQFix.guarded_null());
        assert!(SearchExperiment::HistoryTacticalLmrQFixNull.guarded_null());
        assert!(SearchExperiment::HistoryTacticalLmrQFixNullVerify.guarded_null());
    }

    #[test]
    fn continuation_is_contextual_route_aware_bounded_and_aged() {
        let mut state = kings();
        let weights = EvalWeights::seed();
        put(&mut state, PieceType::GoldGeneral, Color::Black, 10, 10);
        let a = Move::new_two_step(position(10, 10), position(10, 11), position(11, 11));
        let b = Move::new_two_step(position(10, 10), position(11, 10), position(11, 11));
        let c = Move::new_free_eagle(position(10, 10), position(11, 11), vec![position(10, 11)]);
        let d = Move::new_free_eagle(position(10, 10), position(11, 11), vec![position(11, 10)]);
        assert_ne!(route_key(&state, &a), route_key(&state, &b));
        assert_ne!(route_key(&state, &c), route_key(&state, &d));
        with_quiesce_test_context(&state, &weights, 0, None, |pos, ctx| {
            ctx.experiments =
                ExperimentState::new(SearchExperiment::HistoryTacticalLmrQFixContinuation);
            assert_eq!(
                ctx.experiments.continuation.len() * std::mem::size_of::<ContinuationEntry>(),
                1024 * 1024
            );
            ctx.experiments.previous_move = Some(123);
            update_continuation(ctx, pos, &a, 1024);
            assert_eq!(continuation_score(ctx, pos, &a), 1024);
            assert_eq!(continuation_score(ctx, pos, &b), 0);
            ctx.experiments.previous_move = Some(124);
            assert_eq!(continuation_score(ctx, pos, &a), 0);
            ctx.experiments.previous_move = Some(123);
            age_continuation(ctx);
            assert_eq!(continuation_score(ctx, pos, &a), 512);
            let incoming = ctx.experiments.previous_move;
            alphabeta(pos, &weights, 2, -900_000, 900_000, true, ctx);
            assert_eq!(ctx.experiments.previous_move, incoming);
            assert_eq!(pos.hash(), state.hash());
            assert_eq!(ctx.experiments.continuation.len(), HISTORY_ENTRIES);
        });
        assert!(
            ExperimentState::new(SearchExperiment::HistoryTacticalLmrQFix)
                .continuation
                .is_empty()
        );
    }

    #[test]
    fn history_depth_protection_requires_repeated_positive_evidence() {
        let state = kings();
        let weights = EvalWeights::seed();
        let mv = Move::new(position(1, 1), position(1, 2));
        with_quiesce_test_context(&state, &weights, 0, None, |pos, ctx| {
            ctx.experiments.policy = SearchExperiment::HistoryTacticalLmrQFixHistoryReduction;
            assert!(!protect_history(ctx, &mv));
            record_cutoff(pos, ctx, &mv, 6, &[(mv.clone(), 1)]);
            assert!(!protect_history(ctx, &mv));
            record_cutoff(pos, ctx, &mv, 6, &[(mv.clone(), 1)]);
            assert!(protect_history(ctx, &mv));
            ctx.experiments.policy = SearchExperiment::HistoryTacticalLmrQFix;
            assert!(!protect_history(ctx, &mv));
        });
    }

    #[test]
    fn recapture_extra_ply_is_selective_bounded_and_context_keyed() {
        let mut state = kings();
        let weights = EvalWeights::seed();
        put(&mut state, PieceType::GoldGeneral, Color::Black, 10, 10);
        put(&mut state, PieceType::GreatGeneral, Color::White, 10, 11);
        let target = Some(position(10, 11));
        assert!(has_major_recapture(&state, &weights, target));
        with_quiesce_test_context(&state, &weights, 2, target, |pos, ctx| {
            ctx.experiments.policy = SearchExperiment::HistoryTacticalLmrQFixRecapture;
            ctx.quiesce_entry_depth = 1;
            let stand = evaluate_with_ply(pos, &weights, ctx.ply);
            let ordinary = q_tt_key(pos, target, 1, true, true, ctx);
            ctx.experiments.recapture_spent = true;
            assert_ne!(ordinary, q_tt_key(pos, target, 1, true, true, ctx));
            assert_eq!(
                quiesce(pos, &weights, 0, -900_000, 900_000, target, true, true, ctx),
                stand
            );
            assert_eq!(ctx.experiments.recapture_extensions, 0);
            ctx.experiments.recapture_spent = false;
            let resolved = quiesce(pos, &weights, 0, -900_000, 900_000, target, true, true, ctx);
            assert!(resolved > stand);
            assert_eq!(ctx.experiments.recapture_extensions, 1);
            assert!(!ctx.experiments.recapture_spent);
            assert_eq!(ctx.quiesce_entry_depth, 1);
            ctx.quiesce_entry_depth = 2;
            quiesce(pos, &weights, 0, -900_000, 900_000, target, true, true, ctx);
            assert_eq!(ctx.experiments.recapture_extensions, 1);
            assert_eq!(pos.hash(), state.hash());
        });
    }

    #[test]
    fn shallow_null_verification_uses_real_search_and_separate_tt_namespace() {
        let state = kings();
        let weights = EvalWeights::seed();
        with_quiesce_test_context(&state, &weights, 0, None, |pos, ctx| {
            ctx.experiments.policy = SearchExperiment::HistoryTacticalLmrQFixNullVerify;
            ctx.last_ab_capture_enemy = min_quiescence_enemy_material();
            let incoming = (
                ctx.last_ab_capture_enemy,
                ctx.last_ab_to,
                ctx.last_ab_wipe,
                ctx.last_ab_mover_large,
                ctx.quiesce_entry_depth,
            );
            // Too shallow for the depth-two parent, but an unisolated depth-one
            // verification would incorrectly accept this planted optimistic bound.
            ctx.tt.store(TtEntry {
                key: position_hash(pos),
                depth: 1,
                score: 900_000,
                bound: TtBound::Lower,
                best: None,
            });
            alphabeta(pos, &weights, 2, -200_000, -100_000, false, ctx);
            assert_eq!(ctx.experiments.null_verifications, 1);
            assert!(ctx.nodes >= 3);
            assert!(ctx.allow_null);
            assert!(!ctx.experiments.verifying_null);
            assert_eq!(
                (
                    ctx.last_ab_capture_enemy,
                    ctx.last_ab_to,
                    ctx.last_ab_wipe,
                    ctx.last_ab_mover_large,
                    ctx.quiesce_entry_depth
                ),
                incoming
            );
            assert_eq!(pos.hash(), state.hash());
        });
    }
    #[test]
    fn risky_null_verification_rejects_a_static_cutoff_losing_a_trapped_hook() {
        let mut state = GameState::new();
        put(&mut state, PieceType::King, Color::Black, 2, 1);
        put(&mut state, PieceType::King, Color::White, 33, 34);
        put(&mut state, PieceType::HookMover, Color::Black, 18, 15);
        for (file, rank) in [(17, 15), (19, 15), (18, 14), (18, 16)] {
            put(&mut state, PieceType::Pawn, Color::Black, file, rank);
        }
        put(&mut state, PieceType::Knight, Color::White, 19, 17);
        let weights = EvalWeights::seed();
        let beta = evaluate_with_ply(&state, &weights, 0) - 1000;
        let run = |policy| {
            with_quiesce_test_context(&state, &weights, 2, None, |pos, ctx| {
                ctx.experiments.policy = policy;
                ctx.last_ab_capture_enemy = min_quiescence_enemy_material();
                let score = alphabeta(pos, &weights, 2, beta - 1, beta, false, ctx);
                (score, ctx.experiments.null_verifications)
            })
        };
        let ordinary = run(SearchExperiment::HistoryTacticalLmrQFixNull);
        let verified = run(SearchExperiment::HistoryTacticalLmrQFixNullVerify);
        assert!(ordinary.0 >= beta);
        assert_eq!(verified.1, 1);
        assert!(verified.0 < beta, "{verified:?}, beta={beta}");
    }
}
