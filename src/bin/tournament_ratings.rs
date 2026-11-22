//! Read-only order-neutral ratings for a saved tournament state.
use taikyoku_shogi::training::{order_neutral, tournament::TourneyState};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: tournament_ratings STATE.json")?;
    let state: TourneyState = serde_json::from_slice(&std::fs::read(path)?)?;
    let fit = order_neutral::from_state(&state);
    if let Some(error) = fit.error {
        return Err(error.into());
    }
    println!("{}", serde_json::to_string_pretty(&fit)?);
    Ok(())
}
