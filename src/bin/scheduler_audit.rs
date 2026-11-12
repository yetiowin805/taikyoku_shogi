//! Read-only migration/selection rehearsal against an immutable saved state.
use std::collections::BTreeSet;
use std::time::Instant;
use taikyoku_shogi::training::{
    adaptive,
    tournament::{SlotStatus, TourneyFormat, TourneyState},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: scheduler_audit STATE_OR_SNAPSHOT.json")?;
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let mut state: TourneyState =
        serde_json::from_value(value.get("state").cloned().unwrap_or(value))?;
    let original: Vec<_> = state
        .slots
        .iter()
        .filter(|s| s.status == SlotStatus::Done)
        .cloned()
        .collect();
    let start = Instant::now();
    // A snapshot cannot predict drain results. Preserve current running games
    // as commitments; only the selection model treats them as draws.
    let running: BTreeSet<_> = state
        .slots
        .iter()
        .filter(|s| s.status == SlotStatus::Running)
        .map(|s| s.id)
        .collect();
    for slot in &mut state.slots {
        if slot.status == SlotStatus::Running {
            slot.status = SlotStatus::Pending;
        }
    }
    if state.format != TourneyFormat::Adaptive {
        adaptive::migrate(&mut state)?;
    } else {
        adaptive::refresh(&mut state)?;
    }
    for slot in &mut state.slots {
        if running.contains(&slot.id) {
            slot.status = SlotStatus::Pending;
        }
    }
    state
        .adaptive
        .as_mut()
        .unwrap()
        .archived_slots
        .retain(|id| !running.contains(id));
    let fit_seconds = start.elapsed().as_secs_f64();
    let existing = state.adaptive.as_ref().unwrap().pairs.len();
    // Occupy existing commitments, then inspect four additional pairs/eight
    // sequential worker claims. This does not launch or change any real game.
    for slot in &mut state.slots {
        if slot.status == SlotStatus::Pending {
            slot.status = SlotStatus::Running;
        }
    }
    let start = Instant::now();
    for _ in 0..8 {
        adaptive::claim(&mut state)?;
    }
    let selection_seconds = start.elapsed().as_secs_f64();
    let completed: Vec<_> = state
        .slots
        .iter()
        .filter(|s| s.status == SlotStatus::Done)
        .collect();
    assert_eq!(
        serde_json::to_value(original)?,
        serde_json::to_value(completed)?
    );
    let scheduler = state.adaptive.as_ref().unwrap();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "agents":state.entrants.len(),"completed_games":scheduler.completed_games,
            "inactive":state.retired,"actual_components":scheduler.actual_components,
            "assumed_components":scheduler.assumed_components,"fit_seconds":fit_seconds,
            "eight_claims_seconds":selection_seconds,"upcoming_pairs":&scheduler.pairs[existing..],
            "exposure":scheduler.exposure,"completed_results_unchanged":true
        }))?
    );
    Ok(())
}
