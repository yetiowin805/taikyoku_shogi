//! Supplemental, color-swapped leader pairs. Only worker 2 claims these slots.
use super::tournament::{next_slot_id, SlotStartMode, SlotStatus, TourneySlot, TourneyState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pair {
    pub slot_ids: [usize; 2],
    pub agents: [String; 2],
    pub selected_ratings: [f64; 2],
    pub selected_after_games: usize,
}

pub fn is_slot(state: &TourneyState, id: usize) -> bool {
    state.top_two_pairs.iter().any(|p| p.slot_ids.contains(&id))
}

/// Finish the committed pair before refitting; a draw is not a tie-break trigger.
/// None before a finite ranking exists lets this CPU play normal bracket games.
pub fn claim(state: &mut TourneyState) -> Option<usize> {
    if let Some(pair) = state.top_two_pairs.last() {
        for id in pair.slot_ids {
            let slot = state
                .slots
                .iter_mut()
                .find(|s| s.id == id)
                .expect("top-two pair references missing slot");
            if slot.status != SlotStatus::Done {
                assert_ne!(
                    slot.status,
                    SlotStatus::Running,
                    "top-two worker double claim"
                );
                slot.status = SlotStatus::Running;
                return Some(id);
            }
        }
    }
    let fit = super::order_neutral::from_state(state);
    if fit.status != "converged" {
        return None;
    }
    let mut ranked: Vec<_> = state
        .entrants
        .iter()
        .filter_map(|e| fit.ratings.get(&e.id).map(|r| (e.id.clone(), *r)))
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    if ranked.len() < 2 {
        return None;
    }
    let id = next_slot_id(state);
    let seed = state
        .seed_base
        .wrapping_add((id as u64).wrapping_mul(0x9e3779b97f4a7c15));
    for offset in 0..2 {
        state.slots.push(TourneySlot {
            id: id + offset,
            model_a: ranked[0].0.clone(),
            model_b: ranked[1].0.clone(),
            start_seed: seed,
            a_is_black: offset == 0,
            status: if offset == 0 {
                SlotStatus::Running
            } else {
                SlotStatus::Pending
            },
            game_path: None,
            score_a: None,
            round: 0,
            start_mode: SlotStartMode::Opening,
        });
    }
    eprintln!(
        "top-two pair {}: {} ({:.1}) vs {} ({:.1}), after {} games",
        state.top_two_pairs.len() + 1,
        ranked[0].0,
        ranked[0].1,
        ranked[1].0,
        ranked[1].1,
        fit.games
    );
    state.top_two_pairs.push(Pair {
        slot_ids: [id, id + 1],
        agents: [ranked[0].0.clone(), ranked[1].0.clone()],
        selected_ratings: [ranked[0].1, ranked[1].1],
        selected_after_games: fit.games,
    });
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::training::{knockout::pending_slot_claim_index, tournament::*};

    fn fixture() -> TourneyState {
        let cfg = TourneyConfig {
            format: TourneyFormat::Knockout,
            entrants: ["a", "b", "c"]
                .iter()
                .map(|id| TourneyEntrant {
                    id: id.to_string(),
                    model: format!("{id}.json"),
                    engine: None,
                })
                .collect(),
            ..Default::default()
        };
        let mut state = build_schedule(&cfg);
        state.slots.clear();
        state.knockouts.clear();
        // a > b > c, with a finite connected fit.
        for (a, b, score) in [
            ("a", "b", 1.),
            ("a", "b", 0.5),
            ("b", "c", 1.),
            ("b", "c", 0.5),
        ] {
            let id = next_slot_id(&state);
            state.slots.push(TourneySlot {
                id,
                model_a: a.into(),
                model_b: b.into(),
                start_seed: 0,
                a_is_black: true,
                status: SlotStatus::Done,
                game_path: None,
                score_a: Some(score),
                round: 0,
                start_mode: SlotStartMode::Opening,
            });
        }
        state
    }

    fn finish(state: &mut TourneyState, id: usize, score: f64) {
        let s = state.slots.iter_mut().find(|s| s.id == id).unwrap();
        s.status = SlotStatus::Done;
        s.score_a = Some(score);
        let (a, b) = (s.model_a.clone(), s.model_b.clone());
        rate_finished_slot(state, id, &a, &b, score);
    }

    #[test]
    fn leader_pair_is_separate_persistent_color_swapped_and_refits() {
        let mut state = fixture();
        let ratings = serde_json::to_value(&state.ratings).unwrap();
        let elo = state.elo.clone();
        let first = claim(&mut state).unwrap();
        assert_eq!(state.top_two_pairs[0].agents, ["a", "b"]);
        assert!(pending_slot_claim_index(&state).is_none());
        assert_eq!(inflight_count(&state), 0);
        let pair = state.top_two_pairs[0].slot_ids;
        assert_eq!(
            state.slots[first].start_seed,
            state.slots[pair[1]].start_seed
        );
        assert_ne!(
            state.slots[first].a_is_black,
            state.slots[pair[1]].a_is_black
        );
        // Resume an interrupted first game, then change leaders before its return game.
        state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        state.slots[first].status = SlotStatus::Pending;
        assert_eq!(claim(&mut state), Some(first));
        finish(&mut state, first, 0.);
        assert_eq!(claim(&mut state), Some(pair[1]));
        finish(&mut state, pair[1], 0.);
        assert_eq!(serde_json::to_value(&state.ratings).unwrap(), ratings);
        assert_eq!(state.elo, elo);
        assert_eq!(state.rd_tick_done_counter, 0);
        assert!(state.knockouts.is_empty());
        let fit = crate::training::order_neutral::from_state(&state);
        assert_eq!(fit.games, 6);
        assert!(fit.ratings["b"] > fit.ratings["a"]);
        claim(&mut state).unwrap();
        assert_eq!(state.top_two_pairs.len(), 2);
        assert_eq!(state.top_two_pairs[1].agents, ["b", "a"]);
        assert_eq!(state.top_two_pairs[1].selected_after_games, 6);
    }

    #[test]
    fn no_ranking_no_pair_and_legacy_state_loads() {
        let mut state = fixture();
        for s in &mut state.slots {
            s.score_a = Some(1.);
        }
        assert!(claim(&mut state).is_none());
        assert!(state.top_two_pairs.is_empty());
        let mut json = serde_json::to_value(state).unwrap();
        json.as_object_mut().unwrap().remove("top_two_pairs");
        let old: TourneyState = serde_json::from_value(json).unwrap();
        assert!(old.top_two_pairs.is_empty());
    }
}
