//! Offline recovery from a complete backup and an intact slot ledger.
//! The event JSON is an array of newly completed slot IDs in completion order.
use std::{collections::BTreeMap, fs, io::Write};
use taikyoku_shogi::training::{knockout, order_neutral, top_two, tournament::*};

fn refresh(s: &mut TourneyState) {
    let fit = order_neutral::from_state(s);
    s.ratings = fit
        .ratings
        .iter()
        .map(|(id, r)| (id.clone(), GlickoRating { r: *r, rd: 0. }))
        .collect();
    s.elo = fit.ratings;
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err(
            "usage: recover_tournament BACKUP.json FIELDS.json EVENTS.json OUTPUT.json".into(),
        );
    }
    let mut state: TourneyState = serde_json::from_slice(&fs::read(&args[0])?)?;
    let fields: serde_json::Value = serde_json::from_slice(&fs::read(&args[1])?)?;
    let ledger: Vec<TourneySlot> = serde_json::from_value(fields["slots"].clone())?;
    let by_id: BTreeMap<_, _> = ledger.iter().map(|s| (s.id, s)).collect();
    if by_id.len() != ledger.len() {
        return Err("duplicate slot IDs".into());
    }
    if fields["run_id"].as_str() != Some(&state.run_id) {
        return Err("run mismatch".into());
    }
    state.entrants = serde_json::from_value(fields["entrants"].clone())?;
    state.top_two_pairs = serde_json::from_value(fields["top_two_pairs"].clone())?;
    state.retired = serde_json::from_value(fields["retired"].clone())?;
    state.order_neutral_only = true;
    for old in &mut state.slots {
        let latest = by_id.get(&old.id).ok_or("backup slot absent from ledger")?;
        if old.status == SlotStatus::Done
            && (latest.status != SlotStatus::Done || old.score_a != latest.score_a)
        {
            return Err("completed backup result changed".into());
        }
        if old.status != SlotStatus::Done {
            old.status = SlotStatus::Pending;
        }
    }
    let events: Vec<usize> = serde_json::from_slice(&fs::read(&args[2])?)?;
    for id in events {
        let actual = by_id.get(&id).ok_or("event absent from ledger")?;
        if actual.status != SlotStatus::Done {
            return Err("event is not completed".into());
        }
        refresh(&mut state);
        let slot = state
            .slots
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or("event was not scheduled by replay")?;
        if slot.status == SlotStatus::Done {
            return Err("event replays a completed result".into());
        }
        *slot = (*actual).clone();
        if !top_two::is_slot(&state, id) {
            knockout::on_knockout_slot_finished(&mut state, id);
        }
        refresh(&mut state);
    }
    // A committed supplemental pair may have been selected after the last result.
    for actual in &ledger {
        if !state.slots.iter().any(|s| s.id == actual.id) {
            if !top_two::is_slot(&state, actual.id) || actual.status == SlotStatus::Done {
                return Err(format!("unreconstructed bracket/result slot {}", actual.id).into());
            }
            state.slots.push(actual.clone());
        }
    }
    if state.slots.len() != ledger.len() {
        return Err("replay generated unexpected slots".into());
    }
    for slot in &mut state.slots {
        let actual = by_id[&slot.id];
        if (
            slot.model_a.as_str(),
            slot.model_b.as_str(),
            slot.start_seed,
            slot.a_is_black,
            slot.start_mode,
        ) != (
            actual.model_a.as_str(),
            actual.model_b.as_str(),
            actual.start_seed,
            actual.a_is_black,
            actual.start_mode,
        ) {
            return Err(format!("replay changed pairing/start for slot {}", slot.id).into());
        }
        if (slot.status == SlotStatus::Done) != (actual.status == SlotStatus::Done) {
            return Err(format!("completion event missing for slot {}", slot.id).into());
        }
        if slot.status == SlotStatus::Done && slot.score_a != actual.score_a {
            return Err("replay score mismatch".into());
        }
        *slot = actual.clone();
        if slot.status != SlotStatus::Done {
            slot.status = SlotStatus::Pending;
            slot.score_a = None;
            slot.game_path = None;
        }
    }
    state.updated_at = fields["updated_at"].as_u64().ok_or("missing timestamp")?;
    refresh(&mut state);
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&args[3])?;
    file.write_all(&serde_json::to_vec_pretty(&state)?)?;
    file.sync_all()?;
    println!(
        "Recovered {} slots, {} completed games; pairing/start identities verified",
        state.slots.len(),
        state
            .slots
            .iter()
            .filter(|s| s.status == SlotStatus::Done)
            .count()
    );
    Ok(())
}
