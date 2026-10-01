//! Read-only order-neutral ratings for a saved tournament state.
use taikyoku_shogi::training::{order_neutral, tournament::TourneyState};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: tournament_ratings STATE.json")?;
    let state: TourneyState = serde_json::from_slice(&std::fs::read(path)?)?;
    println!("{}", serde_json::to_string_pretty(&order_neutral::from_state(&state))?);
    Ok(())
}
