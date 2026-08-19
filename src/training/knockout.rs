//! Continuous seeded knockout: 1v16 brackets, 2-game matches, period Glicko.

use crate::training::tournament::{
    apply_passive_rd_tick, assign_a_is_black, ensure_ratings, glicko_update_period, inflight_count,
    matchup_games, next_slot_id, rating_of, GlickoRating, SlotStartMode, SlotStatus, TourneySlot,
    TourneyState, RD_MIN,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const PAIR_GAME_CAP: usize = 10;
pub const SINGLES_DRAW_CAP: usize = 4;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum KnockoutStage {
    PlayIn,
    Round { size: usize },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchPhase {
    Pairs,
    Singles,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnockoutMatch {
    pub id: usize,
    pub stage: KnockoutStage,
    /// 0-based index among matches of this stage (bracket order).
    pub bracket_slot: usize,
    pub model_a: String,
    pub model_b: String,
    /// `r` at match creation (fallback winner uses the lower of these).
    pub rating_a: f64,
    pub rating_b: f64,
    pub slot_ids: Vec<usize>,
    pub phase: MatchPhase,
    pub consecutive_draws: usize,
    pub winner: Option<String>,
    pub pair_start_seed: u64,
    pub pair_start_mode: SlotStartMode,
    pub appearances_counted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnockoutTree {
    pub id: usize,
    /// Seed 1 at index 0.
    pub seeds: Vec<String>,
    pub bracket_size: usize,
    pub matches: Vec<KnockoutMatch>,
    /// Stages whose rating period has been applied (labels).
    #[serde(default)]
    pub closed_periods: Vec<String>,
    #[serde(default)]
    pub complete: bool,
}

pub fn stage_label(stage: KnockoutStage) -> String {
    match stage {
        KnockoutStage::PlayIn => "PlayIn".into(),
        KnockoutStage::Round { size: 2 } => "Final".into(),
        KnockoutStage::Round { size } => format!("R{size}"),
    }
}

/// NCAA-style seed order for a power-of-two round: adjacent pairs play.
pub fn seed_bracket(n: usize) -> Vec<usize> {
    assert!(n.is_power_of_two() && n >= 2);
    let mut b = vec![1usize];
    while b.len() < n {
        let m = b.len() * 2;
        let mut next = Vec::with_capacity(m);
        for s in b {
            next.push(s);
            next.push(m + 1 - s);
        }
        b = next;
    }
    b
}

/// `N = 2^n + k` with `k < 2^n`. Returns `(bracket_size, k)`.
pub fn play_in_params(n: usize) -> (usize, usize) {
    assert!(n >= 2);
    if n.is_power_of_two() {
        (n, 0)
    } else {
        let bracket = n.next_power_of_two() / 2;
        (bracket, n - bracket)
    }
}

pub fn seed_order(
    ids: &[String],
    ratings: &BTreeMap<String, GlickoRating>,
    seed_base: u64,
    tourney_id: usize,
) -> Vec<String> {
    let mut rows: Vec<(f64, String)> = ids
        .iter()
        .map(|id| (ratings.get(id).map(|g| g.r).unwrap_or(1500.0), id.clone()))
        .collect();
    rows.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });
    let mut i = 0usize;
    while i < rows.len() {
        let r0 = rows[i].0;
        let mut j = i + 1;
        while j < rows.len() && (rows[j].0 - r0).abs() < 1e-9 {
            j += 1;
        }
        if j - i > 1 {
            fisher_yates(&mut rows[i..j], seed_base, tourney_id, i);
        }
        i = j;
    }
    rows.into_iter().map(|(_, id)| id).collect()
}

fn fisher_yates(slice: &mut [(f64, String)], seed_base: u64, tourney_id: usize, salt: usize) {
    for k in (1..slice.len()).rev() {
        let x = pairing_u64(seed_base, tourney_id, salt as u64, k as u64);
        let j = (x as usize) % (k + 1);
        slice.swap(k, j);
    }
}

fn pairing_u64(seed_base: u64, tourney_id: usize, salt: u64, k: u64) -> u64 {
    let mut x = seed_base
        .wrapping_add(tourney_id as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(salt)
        .wrapping_add(k.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn bump_stat(map: &mut BTreeMap<String, BTreeMap<String, usize>>, id: &str, stage: KnockoutStage) {
    *map.entry(id.to_string())
        .or_default()
        .entry(stage_label(stage))
        .or_insert(0) += 1;
}

/// Fill pending games up to `jobs` inflight, spawning extra brackets if needed.
pub fn fill_knockout_queue(state: &mut TourneyState, jobs: usize) {
    ensure_ratings(state);
    if state.entrants.len() < 2 {
        return;
    }
    let cap = jobs.max(1);
    loop {
        process_all_matches(state);
        enqueue_needed_games(state);
        if inflight_count(state) >= cap {
            return;
        }
        let before = state.slots.len();
        if !spawn_knockout(state) {
            return;
        }
        enqueue_needed_games(state);
        if state.slots.len() == before {
            return;
        }
    }
}

/// Vector index of the next Pending slot: oldest tree, then earlier round.
/// A new tree's R16 / a bye-bye QF must not jump an older final or a live play-in.
pub(crate) fn pending_slot_claim_index(state: &TourneyState) -> Option<usize> {
    let mut best: Option<(usize, u32, usize, usize)> = None;
    for (idx, slot) in state.slots.iter().enumerate() {
        if slot.status != SlotStatus::Pending {
            continue;
        }
        let (tree, round) = slot_tree_and_round(state, slot.id);
        let key = (tree, round, slot.id, idx);
        if best.map_or(true, |b| key < b) {
            best = Some(key);
        }
    }
    best.map(|(_, _, _, idx)| idx)
}

fn slot_tree_and_round(state: &TourneyState, slot_id: usize) -> (usize, u32) {
    for t in &state.knockouts {
        for m in &t.matches {
            if m.slot_ids.contains(&slot_id) {
                return (t.id, stage_claim_rank(m.stage));
            }
        }
    }
    (usize::MAX, u32::MAX)
}

/// Play-in first, then larger rounds (R16 before R8 before Final).
fn stage_claim_rank(stage: KnockoutStage) -> u32 {
    match stage {
        KnockoutStage::PlayIn => 0,
        KnockoutStage::Round { size } => 1000 - size as u32,
    }
}

pub fn on_knockout_slot_finished(state: &mut TourneyState, _slot_id: usize) {
    process_all_matches(state);
    enqueue_needed_games(state);
}

pub fn spawn_knockout(state: &mut TourneyState) -> bool {
    ensure_ratings(state);
    let ids: Vec<String> = state.entrants.iter().map(|e| e.id.clone()).collect();
    if ids.len() < 2 {
        return false;
    }
    let tourney_id = state.knockouts.iter().map(|t| t.id).max().unwrap_or(0) + 1;
    let seeds = seed_order(&ids, &state.ratings, state.seed_base, tourney_id);
    let n = seeds.len();
    let (bracket_size, k) = play_in_params(n);
    let mut tree = KnockoutTree {
        id: tourney_id,
        seeds: seeds.clone(),
        bracket_size,
        matches: Vec::new(),
        closed_periods: Vec::new(),
        complete: false,
    };
    let mut next_match_id = 1usize;

    if k > 0 {
        let first_play = bracket_size - k + 1; // 1-indexed seed
        for i in 0..k {
            let hi = first_play + i; // 1-indexed
            let lo = n - i;
            let a = seeds[hi - 1].clone();
            let b = seeds[lo - 1].clone();
            tree.matches.push(new_match(
                state,
                next_match_id,
                KnockoutStage::PlayIn,
                i,
                a,
                b,
            ));
            next_match_id += 1;
        }
    }

    let order = seed_bracket(bracket_size);
    let n_matches = bracket_size / 2;
    for slot in 0..n_matches {
        let s_hi = order[slot * 2];
        let s_lo = order[slot * 2 + 1];
        let a = if s_hi <= bracket_size - k {
            Some(seeds[s_hi - 1].clone())
        } else {
            None
        };
        let b = if s_lo <= bracket_size - k {
            Some(seeds[s_lo - 1].clone())
        } else {
            None
        };
        if let (Some(a), Some(b)) = (a, b) {
            tree.matches.push(new_match(
                state,
                next_match_id,
                KnockoutStage::Round { size: bracket_size },
                slot,
                a,
                b,
            ));
            next_match_id += 1;
        }
    }

    state.knockouts.push(tree);
    true
}

fn new_match(
    state: &TourneyState,
    id: usize,
    stage: KnockoutStage,
    bracket_slot: usize,
    a: String,
    b: String,
) -> KnockoutMatch {
    let ra = rating_of(state, &a).r;
    let rb = rating_of(state, &b).r;
    KnockoutMatch {
        id,
        stage,
        bracket_slot,
        model_a: a,
        model_b: b,
        rating_a: ra,
        rating_b: rb,
        slot_ids: Vec::new(),
        phase: MatchPhase::Pairs,
        consecutive_draws: 0,
        winner: None,
        pair_start_seed: 0,
        pair_start_mode: SlotStartMode::Opening,
        appearances_counted: false,
    }
}

fn process_all_matches(state: &mut TourneyState) {
    replace_aborted_games(state);
    let n_trees = state.knockouts.len();
    for t in 0..n_trees {
        let n_matches = state.knockouts[t].matches.len();
        for m in 0..n_matches {
            process_match(state, t, m);
        }
        try_close_periods(state, t);
        try_advance_winners(state, t);
    }
}

fn replace_aborted_games(state: &mut TourneyState) {
    let aborted: BTreeSet<usize> = state
        .slots
        .iter()
        .filter(|s| s.status == SlotStatus::Aborted)
        .map(|s| s.id)
        .collect();
    if aborted.is_empty() {
        return;
    }
    for t in 0..state.knockouts.len() {
        let n_m = state.knockouts[t].matches.len();
        for mi in 0..n_m {
            let ids = state.knockouts[t].matches[mi].slot_ids.clone();
            for (idx, sid) in ids.iter().enumerate() {
                if !aborted.contains(sid) {
                    continue;
                }
                let Some(old) = state.slots.iter().find(|s| s.id == *sid).cloned() else {
                    continue;
                };
                let new_id = push_slot(
                    state,
                    old.model_a,
                    old.model_b,
                    old.start_seed,
                    old.a_is_black,
                    old.start_mode,
                );
                state.knockouts[t].matches[mi].slot_ids[idx] = new_id;
            }
        }
    }
}

fn process_match(state: &mut TourneyState, tree_idx: usize, match_idx: usize) {
    let winner_now = {
        let m = &state.knockouts[tree_idx].matches[match_idx];
        if m.winner.is_some() {
            return;
        }
        if m.slot_ids.is_empty() {
            return;
        }
        if match_has_inflight(state, m) {
            return;
        }
        let scores = match_game_scores(state, m);
        if scores.is_empty() {
            return;
        }
        decide_match(m, &scores)
    };
    if let Some(w) = winner_now {
        let stage = state.knockouts[tree_idx].matches[match_idx].stage;
        state.knockouts[tree_idx].matches[match_idx].winner = Some(w.clone());
        bump_stat(&mut state.knockout_stage_wins, &w, stage);
        if matches!(stage, KnockoutStage::Round { size: 2 }) {
            *state.knockout_titles.entry(w).or_insert(0) += 1;
            state.knockouts[tree_idx].complete = true;
        }
        return;
    }
    let m = &state.knockouts[tree_idx].matches[match_idx];
    let n_done = match_game_scores(state, m).len();
    if m.phase == MatchPhase::Pairs && n_done >= PAIR_GAME_CAP {
        state.knockouts[tree_idx].matches[match_idx].phase = MatchPhase::Singles;
        state.knockouts[tree_idx].matches[match_idx].consecutive_draws = 0;
    }
}

enum MatchDecision {
    Wait,
    Winner(String),
    Continue,
}

fn decide_match(m: &KnockoutMatch, scores: &[f64]) -> Option<String> {
    match decide_match_inner(m, scores) {
        MatchDecision::Winner(w) => Some(w),
        _ => None,
    }
}

fn decide_match_inner(m: &KnockoutMatch, scores: &[f64]) -> MatchDecision {
    match m.phase {
        MatchPhase::Pairs => {
            if scores.len() % 2 == 1 {
                return MatchDecision::Wait;
            }
            let (sa, sb) = match_points(scores);
            if (sa - sb).abs() > 1e-9 {
                return MatchDecision::Winner(if sa > sb {
                    m.model_a.clone()
                } else {
                    m.model_b.clone()
                });
            }
            MatchDecision::Continue
        }
        MatchPhase::Singles => {
            let singles = &scores[PAIR_GAME_CAP.min(scores.len())..];
            if let Some(&last) = singles.last() {
                if (last - 0.5).abs() > 1e-9 {
                    return MatchDecision::Winner(if last > 0.5 {
                        m.model_a.clone()
                    } else {
                        m.model_b.clone()
                    });
                }
            }
            let mut run = 0usize;
            for &s in singles.iter().rev() {
                if (s - 0.5).abs() < 1e-9 {
                    run += 1;
                } else {
                    break;
                }
            }
            if run >= SINGLES_DRAW_CAP {
                MatchDecision::Winner(lower_rated(m).to_string())
            } else {
                MatchDecision::Continue
            }
        }
    }
}

fn lower_rated(m: &KnockoutMatch) -> &str {
    if m.rating_a < m.rating_b - 1e-9 {
        &m.model_a
    } else if m.rating_b < m.rating_a - 1e-9 {
        &m.model_b
    } else if m.model_a <= m.model_b {
        &m.model_a
    } else {
        &m.model_b
    }
}

fn match_points(scores: &[f64]) -> (f64, f64) {
    let sa: f64 = scores.iter().sum();
    let sb = scores.len() as f64 - sa;
    (sa, sb)
}

fn match_has_inflight(state: &TourneyState, m: &KnockoutMatch) -> bool {
    m.slot_ids.iter().any(|id| {
        state
            .slots
            .iter()
            .find(|s| s.id == *id)
            .is_some_and(|s| matches!(s.status, SlotStatus::Pending | SlotStatus::Running))
    })
}

fn match_game_scores(state: &TourneyState, m: &KnockoutMatch) -> Vec<f64> {
    let mut out = Vec::new();
    for id in &m.slot_ids {
        let Some(s) = state.slots.iter().find(|s| s.id == *id) else {
            continue;
        };
        if s.status != SlotStatus::Done {
            continue;
        }
        if let Some(sa) = s.score_a {
            out.push(sa);
        }
    }
    out
}

fn enqueue_needed_games(state: &mut TourneyState) {
    let n_trees = state.knockouts.len();
    for t in 0..n_trees {
        let n_m = state.knockouts[t].matches.len();
        for mi in 0..n_m {
            maybe_enqueue_match_games(state, t, mi);
        }
    }
}

fn maybe_enqueue_match_games(state: &mut TourneyState, tree_idx: usize, match_idx: usize) {
    {
        let m = &state.knockouts[tree_idx].matches[match_idx];
        if m.winner.is_some() {
            return;
        }
        if match_has_inflight(state, m) {
            return;
        }
    }
    let scores = {
        let m = &state.knockouts[tree_idx].matches[match_idx];
        match_game_scores(state, m)
    };
    let m = &state.knockouts[tree_idx].matches[match_idx];
    match decide_match_inner(m, &scores) {
        MatchDecision::Winner(_) | MatchDecision::Wait => return,
        MatchDecision::Continue => {}
    }
    if !m.appearances_counted {
        let stage = m.stage;
        let a = m.model_a.clone();
        let b = m.model_b.clone();
        bump_stat(&mut state.knockout_stage_appearances, &a, stage);
        bump_stat(&mut state.knockout_stage_appearances, &b, stage);
        state.knockouts[tree_idx].matches[match_idx].appearances_counted = true;
    }
    let phase = state.knockouts[tree_idx].matches[match_idx].phase;
    let a = state.knockouts[tree_idx].matches[match_idx].model_a.clone();
    let b = state.knockouts[tree_idx].matches[match_idx].model_b.clone();
    let n_done = scores.len();
    match phase {
        MatchPhase::Pairs => enqueue_pair(state, tree_idx, match_idx, &a, &b),
        MatchPhase::Singles => enqueue_single(state, tree_idx, match_idx, &a, &b, n_done),
    }
}

fn enqueue_pair(state: &mut TourneyState, tree_idx: usize, match_idx: usize, a: &str, b: &str) {
    let prior = matchup_games(state, a, b);
    let start_mode = if prior < 2 {
        SlotStartMode::Opening
    } else {
        SlotStartMode::Light
    };
    let m = &state.knockouts[tree_idx].matches[match_idx];
    let start_seed = state
        .seed_base
        .wrapping_add((state.knockouts[tree_idx].id as u64).wrapping_mul(1_000_003))
        .wrapping_add((m.id as u64).wrapping_mul(97))
        .wrapping_add((m.slot_ids.len() as u64).wrapping_mul(13));
    let id0 = next_slot_id(state);
    let a_black = assign_a_is_black(state, a, b, id0);
    let s0 = push_slot(
        state,
        a.to_string(),
        b.to_string(),
        start_seed,
        a_black,
        start_mode,
    );
    let s1 = push_slot(
        state,
        a.to_string(),
        b.to_string(),
        start_seed,
        !a_black,
        start_mode,
    );
    let m = &mut state.knockouts[tree_idx].matches[match_idx];
    m.pair_start_seed = start_seed;
    m.pair_start_mode = start_mode;
    m.slot_ids.push(s0);
    m.slot_ids.push(s1);
}

fn enqueue_single(
    state: &mut TourneyState,
    tree_idx: usize,
    match_idx: usize,
    a: &str,
    b: &str,
    n_done: usize,
) {
    let singles_idx = n_done.saturating_sub(PAIR_GAME_CAP);
    let (start_seed, start_mode) = if singles_idx % 2 == 0 {
        let seed = state
            .seed_base
            .wrapping_add((state.knockouts[tree_idx].id as u64).wrapping_mul(1_000_003))
            .wrapping_add((state.knockouts[tree_idx].matches[match_idx].id as u64) * 91)
            .wrapping_add((singles_idx as u64 / 2) * 17)
            .wrapping_add(0xA11CE);
        (seed, SlotStartMode::Light)
    } else {
        let m = &state.knockouts[tree_idx].matches[match_idx];
        (m.pair_start_seed, m.pair_start_mode)
    };
    let a_black = singles_idx % 2 == 0;
    let sid = push_slot(
        state,
        a.to_string(),
        b.to_string(),
        start_seed,
        a_black,
        start_mode,
    );
    let m = &mut state.knockouts[tree_idx].matches[match_idx];
    m.pair_start_seed = start_seed;
    m.pair_start_mode = start_mode;
    m.slot_ids.push(sid);
}

fn push_slot(
    state: &mut TourneyState,
    model_a: String,
    model_b: String,
    start_seed: u64,
    a_is_black: bool,
    start_mode: SlotStartMode,
) -> usize {
    let id = next_slot_id(state);
    state.slots.push(TourneySlot {
        id,
        model_a,
        model_b,
        start_seed,
        a_is_black,
        status: SlotStatus::Pending,
        game_path: None,
        score_a: None,
        round: 0,
        start_mode,
    });
    id
}

fn try_advance_winners(state: &mut TourneyState, tree_idx: usize) {
    let bracket_size = state.knockouts[tree_idx].bracket_size;
    let k = {
        let n = state.knockouts[tree_idx].seeds.len();
        play_in_params(n).1
    };
    // Play-in winners fill R{bracket} matches that were not created at spawn.
    if k > 0 {
        let play_in: Vec<(usize, Option<String>)> = state.knockouts[tree_idx]
            .matches
            .iter()
            .filter(|m| m.stage == KnockoutStage::PlayIn)
            .map(|m| (m.bracket_slot, m.winner.clone()))
            .collect();
        let order = seed_bracket(bracket_size);
        let first_play = bracket_size - k + 1;
        for (pi_slot, winner) in play_in {
            let Some(w) = winner else { continue };
            let inherited_seed = first_play + pi_slot; // 1-indexed
            let pos = order
                .iter()
                .position(|&s| s == inherited_seed)
                .expect("play-in seed in bracket");
            let r16_slot = pos / 2;
            let as_a = pos % 2 == 0;
            let partner_seed = if as_a { order[pos + 1] } else { order[pos - 1] };
            let partner = if partner_seed <= bracket_size - k {
                Some(state.knockouts[tree_idx].seeds[partner_seed - 1].clone())
            } else {
                // Partner is also a play-in slot.
                let other_pi = partner_seed - first_play;
                state.knockouts[tree_idx]
                    .matches
                    .iter()
                    .find(|m| m.stage == KnockoutStage::PlayIn && m.bracket_slot == other_pi)
                    .and_then(|m| m.winner.clone())
            };
            let Some(partner) = partner else { continue };
            let exists = state.knockouts[tree_idx].matches.iter().any(|m| {
                m.stage == KnockoutStage::Round { size: bracket_size } && m.bracket_slot == r16_slot
            });
            if exists {
                continue;
            }
            let (a, b) = if as_a { (w, partner) } else { (partner, w) };
            let id = state.knockouts[tree_idx]
                .matches
                .iter()
                .map(|m| m.id)
                .max()
                .unwrap_or(0)
                + 1;
            let nm = new_match(
                state,
                id,
                KnockoutStage::Round { size: bracket_size },
                r16_slot,
                a,
                b,
            );
            state.knockouts[tree_idx].matches.push(nm);
        }
    }

    // Later rounds: pair winners of adjacent bracket slots.
    let mut size = bracket_size;
    while size >= 4 {
        let cur = KnockoutStage::Round { size };
        let next_size = size / 2;
        let next = KnockoutStage::Round { size: next_size };
        let winners: Vec<(usize, String)> = state.knockouts[tree_idx]
            .matches
            .iter()
            .filter(|m| m.stage == cur)
            .filter_map(|m| m.winner.clone().map(|w| (m.bracket_slot, w)))
            .collect();
        let n_expected = size / 2;
        if winners.len() < n_expected && winners.is_empty() {
            size = next_size;
            continue;
        }
        for slot in 0..(next_size / 2).max(1).min(next_size) {
            // next round has next_size/2 matches; slot i takes winners of cur 2i and 2i+1
            let need = next_size / 2;
            if slot >= need {
                break;
            }
            let w0 = winners
                .iter()
                .find(|(s, _)| *s == slot * 2)
                .map(|(_, w)| w.clone());
            let w1 = winners
                .iter()
                .find(|(s, _)| *s == slot * 2 + 1)
                .map(|(_, w)| w.clone());
            let (Some(a), Some(b)) = (w0, w1) else {
                continue;
            };
            let exists = state.knockouts[tree_idx]
                .matches
                .iter()
                .any(|m| m.stage == next && m.bracket_slot == slot);
            if exists {
                continue;
            }
            let id = state.knockouts[tree_idx]
                .matches
                .iter()
                .map(|m| m.id)
                .max()
                .unwrap_or(0)
                + 1;
            let nm = new_match(state, id, next, slot, a, b);
            state.knockouts[tree_idx].matches.push(nm);
        }
        size = next_size;
    }
}

fn try_close_periods(state: &mut TourneyState, tree_idx: usize) {
    let bracket_size = state.knockouts[tree_idx].bracket_size;
    let k = play_in_params(state.knockouts[tree_idx].seeds.len()).1;
    let first_full = KnockoutStage::Round { size: bracket_size };
    let first_label = if k > 0 {
        format!("PlayIn+{}", stage_label(first_full))
    } else {
        stage_label(first_full)
    };

    if !state.knockouts[tree_idx]
        .closed_periods
        .iter()
        .any(|p| p == &first_label)
    {
        let play_in_done = k == 0
            || state.knockouts[tree_idx]
                .matches
                .iter()
                .filter(|m| m.stage == KnockoutStage::PlayIn)
                .all(|m| m.winner.is_some());
        let r_done = round_complete(state, tree_idx, first_full);
        if play_in_done && r_done {
            let mut stages = vec![first_full];
            if k > 0 {
                stages.insert(0, KnockoutStage::PlayIn);
            }
            apply_period(state, tree_idx, &stages);
            state.knockouts[tree_idx].closed_periods.push(first_label);
        }
    }

    let mut size = bracket_size / 2;
    while size >= 2 {
        let stage = KnockoutStage::Round { size };
        let label = stage_label(stage);
        if !state.knockouts[tree_idx]
            .closed_periods
            .iter()
            .any(|p| p == &label)
            && round_complete(state, tree_idx, stage)
        {
            apply_period(state, tree_idx, &[stage]);
            state.knockouts[tree_idx].closed_periods.push(label);
        }
        size /= 2;
    }
}

fn round_complete(state: &TourneyState, tree_idx: usize, stage: KnockoutStage) -> bool {
    let matches: Vec<_> = state.knockouts[tree_idx]
        .matches
        .iter()
        .filter(|m| m.stage == stage)
        .collect();
    if matches.is_empty() {
        return false;
    }
    let expected = match stage {
        KnockoutStage::PlayIn => play_in_params(state.knockouts[tree_idx].seeds.len()).1,
        KnockoutStage::Round { size } => size / 2,
    };
    matches.len() == expected && matches.iter().all(|m| m.winner.is_some())
}

fn apply_period(state: &mut TourneyState, tree_idx: usize, stages: &[KnockoutStage]) {
    // Published ratings are frozen during a period (no per-game Glicko), so the
    // value at close equals the snapshot at open unless another tree closed first.
    // Compose from current published ratings so a parallel tree cannot clobber.
    let snapshot = state.ratings.clone();
    let mut slot_ids: Vec<usize> = Vec::new();
    for m in &state.knockouts[tree_idx].matches {
        if stages.contains(&m.stage) {
            slot_ids.extend_from_slice(&m.slot_ids);
        }
    }
    let period_slots: BTreeSet<usize> = slot_ids.iter().copied().collect();
    let mut games_by_player: BTreeMap<String, Vec<(GlickoRating, f64)>> = BTreeMap::new();
    for e in &state.entrants {
        games_by_player.insert(e.id.clone(), Vec::new());
    }
    for sid in slot_ids {
        let Some(slot) = state.slots.iter().find(|s| s.id == sid) else {
            continue;
        };
        if slot.status != SlotStatus::Done {
            continue;
        }
        let Some(sa) = slot.score_a else { continue };
        let ra = snapshot.get(&slot.model_a).copied().unwrap_or_default();
        let rb = snapshot.get(&slot.model_b).copied().unwrap_or_default();
        games_by_player
            .entry(slot.model_a.clone())
            .or_default()
            .push((rb, sa));
        games_by_player
            .entry(slot.model_b.clone())
            .or_default()
            .push((ra, 1.0 - sa));
    }
    for e in state.entrants.clone() {
        let cur = snapshot.get(&e.id).copied().unwrap_or_default();
        let results = games_by_player.get(&e.id).cloned().unwrap_or_default();
        let ng = if !results.is_empty() {
            glicko_update_period(cur, &results)
        } else if agent_has_foreign_inflight(state, &e.id, &period_slots) {
            cur
        } else {
            apply_passive_rd_tick(cur)
        };
        let ng = GlickoRating {
            r: ng.r,
            rd: ng.rd.max(RD_MIN),
        };
        state.ratings.insert(e.id.clone(), ng);
        state.elo.insert(e.id, ng.r);
    }
}

fn agent_has_foreign_inflight(
    state: &TourneyState,
    id: &str,
    period_slots: &BTreeSet<usize>,
) -> bool {
    state.slots.iter().any(|s| {
        !period_slots.contains(&s.id)
            && matches!(s.status, SlotStatus::Pending | SlotStatus::Running)
            && (s.model_a == id || s.model_b == id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::training::tournament::{
        build_schedule, TourneyConfig, TourneyEntrant, TourneyFormat,
    };

    fn ids(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("p{i}")).collect()
    }

    fn cfg_n(n: usize) -> TourneyConfig {
        TourneyConfig {
            entrants: (0..n)
                .map(|i| TourneyEntrant {
                    id: format!("p{i}"),
                    model: format!("p{i}.json"),
                    engine: None,
                })
                .collect(),
            format: TourneyFormat::Knockout,
            seed_base: 1,
            jobs: 4,
            ..TourneyConfig::default()
        }
    }

    fn finish_slot(st: &mut TourneyState, id: usize, score_a: f64) {
        if let Some(s) = st.slots.iter_mut().find(|s| s.id == id) {
            s.status = SlotStatus::Done;
            s.score_a = Some(score_a);
        }
        on_knockout_slot_finished(st, id);
    }

    #[test]
    fn seed_bracket_16_is_ncaa() {
        assert_eq!(seed_bracket(2), vec![1, 2]);
        assert_eq!(seed_bracket(4), vec![1, 4, 2, 3]);
        assert_eq!(
            seed_bracket(16),
            vec![1, 16, 8, 9, 4, 13, 5, 12, 2, 15, 7, 10, 3, 14, 6, 11]
        );
    }

    #[test]
    fn play_in_19_is_six_bottom() {
        assert_eq!(play_in_params(16), (16, 0));
        assert_eq!(play_in_params(17), (16, 1));
        assert_eq!(play_in_params(19), (16, 3));
    }

    #[test]
    fn seed_order_sorts_by_r() {
        let ids = ids(4);
        let mut ratings = BTreeMap::new();
        ratings.insert(
            "p0".into(),
            GlickoRating {
                r: 1600.0,
                rd: 50.0,
            },
        );
        ratings.insert(
            "p1".into(),
            GlickoRating {
                r: 1800.0,
                rd: 50.0,
            },
        );
        ratings.insert(
            "p2".into(),
            GlickoRating {
                r: 1700.0,
                rd: 50.0,
            },
        );
        ratings.insert(
            "p3".into(),
            GlickoRating {
                r: 1500.0,
                rd: 50.0,
            },
        );
        let s = seed_order(&ids, &ratings, 1, 1);
        assert_eq!(s, vec!["p1", "p2", "p0", "p3"]);
    }

    #[test]
    fn seed_order_tie_shuffle_is_deterministic() {
        let ids = ids(4);
        let mut ratings = BTreeMap::new();
        for id in &ids {
            ratings.insert(
                id.clone(),
                GlickoRating {
                    r: 1500.0,
                    rd: 50.0,
                },
            );
        }
        let a = seed_order(&ids, &ratings, 7, 1);
        let b = seed_order(&ids, &ratings, 7, 1);
        assert_eq!(a, b);
        let c = seed_order(&ids, &ratings, 8, 1);
        assert_ne!(a, c);
    }

    #[test]
    fn nineteen_play_in_and_known_r16() {
        let mut st = build_schedule(&cfg_n(19));
        for (i, e) in st.entrants.clone().iter().enumerate() {
            st.ratings.insert(
                e.id.clone(),
                GlickoRating {
                    r: 1900.0 - i as f64 * 10.0,
                    rd: 50.0,
                },
            );
        }
        spawn_knockout(&mut st);
        fill_knockout_queue(&mut st, 4);
        let tree = &st.knockouts[0];
        assert_eq!(tree.bracket_size, 16);
        let play = tree
            .matches
            .iter()
            .filter(|m| m.stage == KnockoutStage::PlayIn)
            .count();
        assert_eq!(play, 3);
        let r16 = tree
            .matches
            .iter()
            .filter(|m| m.stage == KnockoutStage::Round { size: 16 })
            .count();
        // 5 bye-bye matches known immediately (4v13 .. 8v9 in NCAA layout).
        assert_eq!(r16, 5);
        assert!(st.slots.len() >= 3 * 2 + 5 * 2);
    }

    #[test]
    fn match_1_1_enqueues_another_pair() {
        let mut st = build_schedule(&cfg_n(2));
        spawn_knockout(&mut st);
        enqueue_needed_games(&mut st);
        assert_eq!(st.slots.len(), 2);
        let ids: Vec<usize> = st.slots.iter().map(|s| s.id).collect();
        finish_slot(&mut st, ids[0], 1.0);
        finish_slot(&mut st, ids[1], 0.0);
        assert_eq!(st.slots.len(), 4);
        assert!(st.knockouts[0].matches[0].winner.is_none());
    }

    #[test]
    fn match_2_0_has_winner() {
        let mut st = build_schedule(&cfg_n(2));
        spawn_knockout(&mut st);
        enqueue_needed_games(&mut st);
        let ids: Vec<usize> = st.slots.iter().map(|s| s.id).collect();
        finish_slot(&mut st, ids[0], 1.0);
        finish_slot(&mut st, ids[1], 1.0);
        assert_eq!(st.knockouts[0].matches[0].winner.as_deref(), Some("p0"));
        assert_eq!(st.knockout_titles.get("p0").copied().unwrap_or(0), 1);
    }

    #[test]
    fn ten_all_then_four_draws_picks_lower_r() {
        let mut st = build_schedule(&cfg_n(2));
        st.ratings.insert(
            "p0".into(),
            GlickoRating {
                r: 1600.0,
                rd: 40.0,
            },
        );
        st.ratings.insert(
            "p1".into(),
            GlickoRating {
                r: 1400.0,
                rd: 40.0,
            },
        );
        spawn_knockout(&mut st);
        enqueue_needed_games(&mut st);
        for _ in 0..5 {
            let pending: Vec<usize> = st
                .slots
                .iter()
                .filter(|s| s.status == SlotStatus::Pending)
                .map(|s| s.id)
                .collect();
            assert_eq!(pending.len(), 2);
            finish_slot(&mut st, pending[0], 0.5);
            finish_slot(&mut st, pending[1], 0.5);
        }
        assert_eq!(st.knockouts[0].matches[0].phase, MatchPhase::Singles);
        for _ in 0..4 {
            let pending: Vec<usize> = st
                .slots
                .iter()
                .filter(|s| s.status == SlotStatus::Pending)
                .map(|s| s.id)
                .collect();
            assert_eq!(pending.len(), 1);
            finish_slot(&mut st, pending[0], 0.5);
        }
        assert_eq!(st.knockouts[0].matches[0].winner.as_deref(), Some("p1"));
    }

    #[test]
    fn play_in_period_waits_for_r16() {
        let mut st = build_schedule(&cfg_n(3));
        for (i, e) in st.entrants.clone().iter().enumerate() {
            st.ratings.insert(
                e.id.clone(),
                GlickoRating {
                    r: 1700.0 - i as f64 * 50.0,
                    rd: 80.0,
                },
            );
        }
        let r_before = st.ratings["p2"].r;
        let rd_before = st.ratings["p2"].rd;
        spawn_knockout(&mut st);
        enqueue_needed_games(&mut st);
        // 3 players: bracket 2, k=1, play-in p1 vs p2, p0 bye to final.
        let play: Vec<usize> = st.knockouts[0]
            .matches
            .iter()
            .find(|m| m.stage == KnockoutStage::PlayIn)
            .unwrap()
            .slot_ids
            .clone();
        finish_slot(&mut st, play[0], 1.0);
        finish_slot(&mut st, play[1], 1.0);
        assert!(st.knockouts[0].closed_periods.is_empty());
        assert!((st.ratings["p2"].r - r_before).abs() < 1e-9);
        assert!((st.ratings["p2"].rd - rd_before).abs() < 1e-9);
        // Final (R2) games.
        let final_ids: Vec<usize> = st.knockouts[0]
            .matches
            .iter()
            .find(|m| m.stage == KnockoutStage::Round { size: 2 })
            .unwrap()
            .slot_ids
            .clone();
        finish_slot(&mut st, final_ids[0], 1.0);
        finish_slot(&mut st, final_ids[1], 1.0);
        assert!(!st.knockouts[0].closed_periods.is_empty());
        assert!(st.ratings["p2"].rd > rd_before || st.ratings["p2"].r != r_before);
    }

    #[test]
    fn fill_spawns_second_tree_when_final_inflight() {
        let mut st = build_schedule(&cfg_n(2));
        fill_knockout_queue(&mut st, 4);
        assert!(st.knockouts.len() >= 2, "got {} trees", st.knockouts.len());
    }

    #[test]
    fn claim_prefers_older_tree_over_newer_pending() {
        let mut st = build_schedule(&cfg_n(2));
        fill_knockout_queue(&mut st, 4);
        assert!(st.knockouts.len() >= 2);
        let t1 = st.knockouts[0].matches[0].slot_ids.clone();
        assert_eq!(t1.len(), 2);
        finish_slot(&mut st, t1[0], 1.0);
        finish_slot(&mut st, t1[1], 0.0);
        let t1_next = st.knockouts[0].matches[0].slot_ids[2];
        let t2_pending: Vec<usize> = st.knockouts[1].matches[0]
            .slot_ids
            .iter()
            .copied()
            .filter(|&id| {
                st.slots
                    .iter()
                    .find(|s| s.id == id)
                    .is_some_and(|s| s.status == SlotStatus::Pending)
            })
            .collect();
        assert!(!t2_pending.is_empty());
        assert!(t2_pending.iter().all(|&id| id < t1_next));
        let idx = pending_slot_claim_index(&st).unwrap();
        assert_eq!(st.slots[idx].id, t1_next);
    }

    fn match_stage_for_slot(st: &TourneyState, slot_id: usize) -> KnockoutStage {
        for t in &st.knockouts {
            for m in &t.matches {
                if m.slot_ids.contains(&slot_id) {
                    return m.stage;
                }
            }
        }
        panic!("slot {slot_id} not in a match");
    }

    #[test]
    fn claim_prefers_earlier_round_in_same_tree() {
        let mut st = build_schedule(&cfg_n(19));
        for (i, e) in st.entrants.clone().iter().enumerate() {
            st.ratings.insert(
                e.id.clone(),
                GlickoRating {
                    r: 1900.0 - i as f64 * 10.0,
                    rd: 50.0,
                },
            );
        }
        spawn_knockout(&mut st);
        enqueue_needed_games(&mut st);
        let r16: Vec<KnockoutMatch> = st.knockouts[0]
            .matches
            .iter()
            .filter(|m| m.stage == KnockoutStage::Round { size: 16 })
            .cloned()
            .collect();
        let a = r16.iter().find(|m| m.bracket_slot == 2).expect("4v13");
        let b = r16.iter().find(|m| m.bracket_slot == 3).expect("5v12");
        for id in a.slot_ids.iter().chain(b.slot_ids.iter()) {
            finish_slot(&mut st, *id, 1.0);
        }
        assert!(st.knockouts[0]
            .matches
            .iter()
            .any(|m| m.stage == KnockoutStage::Round { size: 8 }));
        let play = st.knockouts[0]
            .matches
            .iter()
            .find(|m| m.stage == KnockoutStage::PlayIn)
            .unwrap()
            .slot_ids
            .clone();
        finish_slot(&mut st, play[0], 1.0);
        finish_slot(&mut st, play[1], 0.0);
        let idx = pending_slot_claim_index(&st).unwrap();
        let stage = match_stage_for_slot(&st, st.slots[idx].id);
        assert_eq!(stage, KnockoutStage::PlayIn);
        assert!(st.slots.iter().any(|s| {
            s.status == SlotStatus::Pending
                && match_stage_for_slot(&st, s.id) == KnockoutStage::Round { size: 8 }
        }));
    }

    #[test]
    fn sixteen_is_1v16_no_play_in() {
        let mut st = build_schedule(&cfg_n(16));
        for (i, e) in st.entrants.clone().iter().enumerate() {
            st.ratings.insert(
                e.id.clone(),
                GlickoRating {
                    r: 2000.0 - i as f64,
                    rd: 50.0,
                },
            );
        }
        spawn_knockout(&mut st);
        enqueue_needed_games(&mut st);
        let tree = &st.knockouts[0];
        assert_eq!(tree.bracket_size, 16);
        assert!(!tree
            .matches
            .iter()
            .any(|m| m.stage == KnockoutStage::PlayIn));
        let r16: Vec<_> = tree
            .matches
            .iter()
            .filter(|m| m.stage == KnockoutStage::Round { size: 16 })
            .collect();
        assert_eq!(r16.len(), 8);
        let first = r16.iter().find(|m| m.bracket_slot == 0).unwrap();
        assert_eq!(first.model_a, "p0");
        assert_eq!(first.model_b, "p15");
    }

    #[test]
    fn seventeen_play_in_is_bottom_two() {
        let mut st = build_schedule(&cfg_n(17));
        for (i, e) in st.entrants.clone().iter().enumerate() {
            st.ratings.insert(
                e.id.clone(),
                GlickoRating {
                    r: 2000.0 - i as f64,
                    rd: 50.0,
                },
            );
        }
        spawn_knockout(&mut st);
        let tree = &st.knockouts[0];
        let play: Vec<_> = tree
            .matches
            .iter()
            .filter(|m| m.stage == KnockoutStage::PlayIn)
            .collect();
        assert_eq!(play.len(), 1);
        assert_eq!(play[0].model_a, "p15");
        assert_eq!(play[0].model_b, "p16");
        let r16 = tree
            .matches
            .iter()
            .filter(|m| m.stage == KnockoutStage::Round { size: 16 })
            .count();
        assert_eq!(r16, 7);
    }

    #[test]
    fn bye_is_not_a_play_in_appearance() {
        let mut st = build_schedule(&cfg_n(3));
        for (i, e) in st.entrants.clone().iter().enumerate() {
            st.ratings.insert(
                e.id.clone(),
                GlickoRating {
                    r: 1700.0 - i as f64 * 50.0,
                    rd: 80.0,
                },
            );
        }
        spawn_knockout(&mut st);
        enqueue_needed_games(&mut st);
        let play = st
            .knockout_stage_appearances
            .get("p0")
            .and_then(|m| m.get("PlayIn"))
            .copied()
            .unwrap_or(0);
        assert_eq!(play, 0);
        assert_eq!(st.knockout_stage_appearances["p1"]["PlayIn"], 1);
        assert_eq!(st.knockout_stage_appearances["p2"]["PlayIn"], 1);
    }

    #[test]
    fn final_period_sit_out_gets_one_rd_tick() {
        let mut st = build_schedule(&cfg_n(4));
        for (i, e) in st.entrants.clone().iter().enumerate() {
            st.ratings.insert(
                e.id.clone(),
                GlickoRating {
                    r: 1800.0 - i as f64 * 50.0,
                    rd: 80.0,
                },
            );
        }
        spawn_knockout(&mut st);
        enqueue_needed_games(&mut st);
        // Two semis, 4 pending games.
        let pending: Vec<usize> = st
            .slots
            .iter()
            .filter(|s| s.status == SlotStatus::Pending)
            .map(|s| s.id)
            .collect();
        assert_eq!(pending.len(), 4);
        for id in pending {
            finish_slot(&mut st, id, 1.0);
        }
        assert!(st.knockouts[0].closed_periods.iter().any(|p| p == "R4"));
        let r2 = st.ratings["p2"].r;
        let rd2 = st.ratings["p2"].rd;
        let final_ids: Vec<usize> = st.knockouts[0]
            .matches
            .iter()
            .find(|m| m.stage == KnockoutStage::Round { size: 2 })
            .unwrap()
            .slot_ids
            .clone();
        finish_slot(&mut st, final_ids[0], 1.0);
        finish_slot(&mut st, final_ids[1], 1.0);
        assert!(st.knockouts[0].closed_periods.iter().any(|p| p == "Final"));
        assert!((st.ratings["p2"].r - r2).abs() < 1e-9);
        assert!(st.ratings["p2"].rd > rd2);
    }
}
