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
    let ranked = select(state, &fit)?;
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

fn select(state: &TourneyState, fit: &super::order_neutral::Fit) -> Option<Vec<(String, f64)>> {
    if !["converged", "disconnected", "separated"].contains(&fit.status) {
        return None;
    }
    let active: std::collections::BTreeSet<_> = state
        .entrants
        .iter()
        .filter(|e| !state.retired.contains(&e.id))
        .map(|e| e.id.clone())
        .collect();
    let salt = state.seed_base.wrapping_add(next_slot_id(state) as u64);
    // Random ties remain reproducible across restart, independent of map ordering.
    let tie = |id: &str| {
        let mut n = salt ^ 0xcbf29ce484222325u64;
        for b in id.bytes() {
            n = (n ^ b as u64).wrapping_mul(0x100000001b3);
        }
        n ^= n >> 30;
        n = n.wrapping_mul(0xbf58476d1ce4e5b9);
        n ^= n >> 27;
        n.wrapping_mul(0x94d049bb133111eb) ^ (n >> 31)
    };
    let sort = |ids: &[String]| {
        let mut rows: Vec<_> = ids
            .iter()
            .filter(|id| active.contains(*id))
            .filter_map(|id| fit.ratings.get(id).map(|r| (id.clone(), *r)))
            .collect();
        rows.sort_by(|a, b| {
            b.1.total_cmp(&a.1)
                .then_with(|| tie(&a.0).cmp(&tie(&b.0)))
                .then_with(|| a.0.cmp(&b.0))
        });
        rows
    };
    let mut groups: Vec<_> = fit
        .weak_components
        .iter()
        .map(|g| sort(g))
        .filter(|g| !g.is_empty())
        .collect();
    if groups.len() > 1 {
        groups.sort_by_key(|g| (g.len(), tie(&g[0].0)));
        return Some(vec![groups[0][0].clone(), groups[1][0].clone()]);
    }
    let all = sort(&active.iter().cloned().collect::<Vec<_>>());
    if all.len() < 2 {
        return None;
    }
    let component = fit.components.iter().find(|g| g.contains(&all[0].0))?;
    let inside = sort(component);
    let outside: Vec<_> = all
        .iter()
        .filter(|(id, _)| !component.contains(id))
        .cloned()
        .collect();
    if !outside.is_empty() {
        return Some(vec![inside.last()?.clone(), outside[0].clone()]);
    }
    Some(champion_pair(state, &all))
}

// Rank r has inverse weight 2^floor(log2(r-1)). Normalization and the
// total head-to-head count cancel when comparing actual/target ratios.
fn champion_pair(state: &TourneyState, ranked: &[(String, f64)]) -> Vec<(String, f64)> {
    let leader = &ranked[0].0;
    let mut counts = std::collections::BTreeMap::<&str, u128>::new();
    for slot in &state.slots {
        if slot.status != SlotStatus::Done || slot.score_a.is_none() {
            continue;
        }
        let opponent = if &slot.model_a == leader {
            &slot.model_b
        } else if &slot.model_b == leader {
            &slot.model_a
        } else {
            continue;
        };
        *counts.entry(opponent.as_str()).or_default() += 1;
    }
    let opponent = (1..ranked.len())
        .min_by_key(|&i| {
            let inverse_weight = 1u128 << i.ilog2();
            // Rank order breaks equal ratios, including all unplayed opponents.
            (
                counts.get(ranked[i].0.as_str()).copied().unwrap_or(0) * inverse_weight,
                i,
            )
        })
        .expect("at least two ranked agents");
    vec![ranked[0].clone(), ranked[opponent].clone()]
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
        // Cover the leader's remaining opponent so rank two is underrepresented.
        for score in [1., 0.5, 1., 0.5] {
            let mut slot = state.slots[0].clone();
            slot.id = next_slot_id(&state);
            slot.model_b = "c".into();
            slot.score_a = Some(score);
            state.slots.push(slot);
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
    fn coverage_uses_all_completed_games_and_current_rank_weights() {
        let mut state = fixture();
        state.slots.clear();
        let ranked: Vec<_> = (0..32)
            .map(|i| (format!("agent{i}"), 2000. - i as f64))
            .collect();
        // Every band has unit total weight: normalized rank-two target is 1/5.
        let weight_sum: f64 = (1usize..32).map(|i| 1. / (1u64 << i.ilog2()) as f64).sum();
        assert_eq!(weight_sum, 5.);
        for i in 1usize..32 {
            let count = 16 / (1usize << i.ilog2());
            for _ in 0..count {
                state.slots.push(TourneySlot {
                    id: state.slots.len(),
                    model_a: ranked[i].0.clone(),
                    model_b: ranked[0].0.clone(),
                    start_seed: 0,
                    a_is_black: true,
                    status: SlotStatus::Done,
                    game_path: None,
                    score_a: Some(0.5),
                    round: 0,
                    start_mode: SlotStartMode::Opening,
                });
            }
        }
        assert_eq!(champion_pair(&state, &ranked)[1].0, "agent1");
        // One fewer completed game makes the lowest-ranked opponent most deficient.
        state.slots.last_mut().unwrap().status = SlotStatus::Running;
        assert_eq!(champion_pair(&state, &ranked)[1].0, "agent31");
        state.slots.clear();
        assert_eq!(champion_pair(&state, &ranked)[1].0, "agent1");
        // Counts follow identities when rankings move; unrelated leaders do not count.
        let mut st = fixture();
        let ranking = vec![
            ("a".into(), 1600.),
            ("b".into(), 1500.),
            ("c".into(), 1400.),
        ];
        assert_eq!(champion_pair(&st, &ranking)[1].0, "b");
        st.slots.retain(|g| g.model_b != "c" || g.model_a != "a");
        assert_eq!(champion_pair(&st, &ranking)[1].0, "c");
        let swapped = vec![ranking[0].clone(), ranking[2].clone(), ranking[1].clone()];
        assert_eq!(champion_pair(&st, &swapped)[1].0, "c");
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
        assert_eq!(fit.games, 10);
        assert!(fit.ratings["b"] > fit.ratings["a"]);
        claim(&mut state).unwrap();
        assert_eq!(state.top_two_pairs.len(), 2);
        assert_eq!(state.top_two_pairs[1].agents, ["b", "a"]);
        assert_eq!(state.top_two_pairs[1].selected_after_games, 10);
    }

    #[test]
    fn no_ranking_no_pair_and_legacy_state_loads() {
        let mut state = fixture();
        for s in &mut state.slots {
            s.score_a = Some(1.);
        }
        assert!(claim(&mut state).is_some());
        assert_eq!(state.top_two_pairs[0].agents, ["a", "b"]);
        let mut json = serde_json::to_value(state).unwrap();
        json.as_object_mut().unwrap().remove("top_two_pairs");
        let old: TourneyState = serde_json::from_value(json).unwrap();
        assert!(old.top_two_pairs.is_empty());
    }
    #[test]
    fn unplayed_agent_bridges_then_separation_tests_component_boundary() {
        let mut st = fixture();
        st.entrants.push(TourneyEntrant {
            id: "new".into(),
            model: "new.json".into(),
            engine: None,
        });
        let fit = crate::training::order_neutral::from_state(&st);
        let selected = select(&st, &fit).unwrap();
        assert_eq!(
            selected.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
            vec!["new", "a"]
        );
        let first = claim(&mut st).unwrap();
        let second = st.top_two_pairs[0].slot_ids[1];
        finish(&mut st, first, 0.);
        claim(&mut st);
        finish(&mut st, second, 0.);
        let fit = crate::training::order_neutral::from_state(&st);
        let selected = select(&st, &fit).unwrap();
        assert_eq!(
            selected.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
            vec!["c", "new"]
        );
        st.retired.insert("c".into());
        assert_eq!(select(&st, &fit).unwrap()[0].0, "b");
    }
}
