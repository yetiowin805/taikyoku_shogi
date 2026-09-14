//! Opt-in smoke experiments. No checkpoint schema or production defaults change.
//! The mate probe returns Unknown on budget exhaustion; generation is not interruptible.
use super::*;
use crate::game_state::{GameState, Move};
use crate::move_simulation::{simulate_move, BoardLike};
use std::cell::Cell;
use std::time::{Duration, Instant};

thread_local! {
    static MODES: Cell<(bool, bool)> = const { Cell::new((false, false)) };
}
pub struct ModeGuard((bool, bool));
impl Drop for ModeGuard {
    fn drop(&mut self) {
        MODES.with(|m| m.set(self.0));
    }
}
pub fn set_modes(blocked_alignment: bool, verified_flights: bool) -> ModeGuard {
    ModeGuard(MODES.with(|m| m.replace((blocked_alignment, verified_flights))))
}
pub(super) fn blocked_alignment_enabled() -> bool {
    MODES.with(|m| m.get().0)
}
pub(super) fn verified_flights_enabled() -> bool {
    MODES.with(|m| m.get().1)
}

/// Conservative clear-first-leg heuristic; not a complete two-step attack detector.
/// It deliberately gives no bonus through either color's intervening pieces.
pub fn clear_alignment_ray(board: &Board, from: Position, to: Position) -> bool {
    let Some(d) = ortho_diag_dir(from, to) else {
        return false;
    };
    let (df, dr) = d.to_offset();
    let mut sq = from;
    while let Some(next) = sq.offset(df, dr) {
        if next == to {
            return true;
        }
        if board.get_piece(next).is_some() {
            return false;
        }
        sq = next;
    }
    false
}

/// Actual King/Crown Prince destinations, including two-square King moves and captures.
/// No board clones; targets use the existing movement generator and are deduplicated.
/// Capped at two: larger counts do not alter this experimental penalty.
pub fn verified_flights(board: &Board, color: Color) -> Option<u8> {
    let royal = last_royal_of(board.pieces_by_color(color))?;
    let piece = board.get_piece(royal)?;
    let mut flights = 0;
    let cfg = MovementConfig::for_piece_type(piece.piece_type);
    let targets = MovementGenerator::generate_targets(&piece, board, &cfg.capabilities);
    let last_enemy = last_royal_of(board.pieces_by_color(color.opposite()));
    for to in targets {
        let mv = Move::new(royal, to);
        let vb = simulate_move(board, &mv, &piece);
        if last_enemy == Some(to)
            || !vb.is_position_attacked_by_color_for_check(to, color.opposite())
        {
            flights += 1;
            if flights == 2 {
                break;
            }
        }
    }
    Some(flights)
}

/// Cheap gate: only actual check activates this first prototype. It does not
/// attempt to enumerate potential checking moves during static evaluation.
pub fn verified_flight_penalty(board: &Board, color: Color, k: f32) -> f32 {
    if k == 0.0 {
        return 0.0;
    }
    let Some(royal) = last_royal_of(board.pieces_by_color(color)) else {
        return 0.0;
    };
    if !board.is_position_attacked_by_color_for_check(royal, color.opposite()) {
        return 0.0;
    }
    k * lr_flight_unit(true, verified_flights(board, color).unwrap())
}

/// Consume an already-generated list, never generate evasions for static eval.
/// Zero is left to terminal search; nonzero terms are exploratory evaluation units.
pub fn defense_penalty(evasions: &[Move]) -> i32 {
    match evasions.len() {
        1 => 1000,
        2 => 400,
        _ => 0,
    }
}

fn safe_for(state: &GameState, color: Color) -> bool {
    if state.get_winner() == Some(color) {
        return true;
    }
    let b = state.get_board();
    let royals: Vec<_> = b
        .pieces_by_color(color)
        .iter()
        .filter(|p| p.piece_type.is_royal())
        .collect();
    match royals.as_slice() {
        [] => false,
        [p] => !b.is_position_attacked_by_color_for_check(p.position, color.opposite()),
        _ => true,
    }
}

#[derive(Debug, serde::Serialize, PartialEq)]
pub enum ProbeStatus {
    Found,
    NoMate,
    Unknown,
    NotApplicable,
}
#[derive(Debug, serde::Serialize)]
pub struct ProbeResult {
    pub status: ProbeStatus,
    pub winning_check: Option<Move>,
    pub candidates: usize,
    pub defenses: usize,
    pub generation_calls: usize,
    pub elapsed_us: u128,
}

/// Searches for a checking move with no evasion *by the side to move*, under existing
/// last-royal/check semantics. Not proof of an unavoidable threat against an
/// opponent who gets to move first. Direct royal captures are outside this probe.
/// Full-history make/unmake is retained.
/// At most 64 attacker candidates and 256 defense candidates by default.
pub fn mate_probe(original: &GameState, budget: Duration, cap: usize) -> ProbeResult {
    let start = Instant::now();
    let deadline = start + budget;
    let mut r = ProbeResult {
        status: ProbeStatus::Unknown,
        winning_check: None,
        candidates: 0,
        defenses: 0,
        generation_calls: 0,
        elapsed_us: 0,
    };
    if cap == 0 || budget.is_zero() {
        return r;
    }
    let us = original.get_current_turn();
    let them = us.opposite();
    if original.get_winner().is_some()
        || original.is_repetition_draw_for_search()
        || original.is_draw_by_progress_rule()
        || last_royal_of(original.get_board().pieces_by_color(them)).is_none()
    {
        r.status = ProbeStatus::NotApplicable;
        r.elapsed_us = start.elapsed().as_micros();
        return r;
    }
    // Production undo restores positions/history but can reorder captured pieces.
    // Keep the probe's entire traversal private, including list-order changes.
    let mut working = original.clone();
    let state = &mut working;
    let attackers = state.get_board().pieces_by_color(us).to_vec();
    'attack: for p in attackers {
        if Instant::now() >= deadline || r.candidates >= cap {
            break;
        }
        r.generation_calls += 1;
        let moves = state.generate_legal_moves_for_pieces(&[p]);
        for mv in moves {
            if Instant::now() >= deadline || r.candidates >= cap {
                break 'attack;
            }
            r.candidates += 1;
            let Some(undo) = state.make_move_for_search(mv.clone()) else {
                continue;
            };
            let mut found = false;
            let mut unknown = false;
            if state.get_winner().is_none()
                && safe_for(state, us)
                && !safe_for(state, them)
                && !state.is_repetition_draw_for_search()
                && !state.is_draw_by_progress_rule()
            {
                let defenders = state.get_board().pieces_by_color(them).to_vec();
                let mut escaped = false;
                'defense: for defender in defenders {
                    if Instant::now() >= deadline || r.defenses >= cap.saturating_mul(4) {
                        unknown = true;
                        break;
                    }
                    r.generation_calls += 1;
                    let replies = state.generate_legal_moves_for_pieces(&[defender]);
                    for reply in replies {
                        if Instant::now() >= deadline || r.defenses >= cap.saturating_mul(4) {
                            unknown = true;
                            break 'defense;
                        }
                        r.defenses += 1;
                        let Some(reply_undo) = state.make_move_for_search(reply) else {
                            continue;
                        };
                        escaped = safe_for(state, them)
                            || state.is_repetition_draw_for_search()
                            || state.is_draw_by_progress_rule();
                        state.unmake_move_for_search(reply_undo);
                        if escaped {
                            break 'defense;
                        }
                    }
                }
                // A generation call itself may exceed the deadline, even with no moves.
                unknown |= Instant::now() >= deadline;
                found = !escaped && !unknown;
            }
            state.unmake_move_for_search(undo);
            if found {
                r.status = ProbeStatus::Found;
                r.winning_check = Some(mv);
                break 'attack;
            }
            if unknown {
                break 'attack;
            }
        }
        if Instant::now() >= deadline {
            break;
        }
        // NoMate is assigned only after exhausting the final attacker below.
    }
    if r.status != ProbeStatus::Found && Instant::now() < deadline && r.candidates < cap {
        // All loops complete without cap/time exhaustion iff the defense budget
        // is also intact. A partial probe is never evidence of safety.
        if r.defenses < cap.saturating_mul(4) {
            r.status = ProbeStatus::NoMate;
        }
    }
    r.elapsed_us = start.elapsed().as_micros();
    r
}

/// Generated moves filtered with the production check-evasion predicate.
/// Diagnostic helper only; the search path reuses its existing list.
pub fn diagnostic_evasions(state: &GameState) -> Option<Vec<Move>> {
    crate::search::royal_probe_evasions(&mut state.clone())
}

pub fn term_scores(board: &Board, weights: &EvalWeights) -> (f32, f32) {
    (
        two_mover_align_of(
            board,
            board.pieces_by_color(Color::Black),
            &enemy_royal_positions(board.pieces_by_color(Color::White)),
            weights,
        ) - two_mover_align_of(
            board,
            board.pieces_by_color(Color::White),
            &enemy_royal_positions(board.pieces_by_color(Color::Black)),
            weights,
        ),
        last_royal_flight_penalty(board, Color::White, weights)
            - last_royal_flight_penalty(board, Color::Black, weights),
    )
}

#[cfg(test)]
mod tests;
