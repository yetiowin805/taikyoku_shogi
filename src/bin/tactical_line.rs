//! Replay proposed tactical lines and report captures and last-royal constraints.
use std::io::{self, Write};
use taikyoku_shogi::{
    board_position::BoardPosition,
    eval::{material_piece_value, EvalWeights},
    game_history::GameHistory,
    game_state::{GameState, LegalMoveGen, Move},
    notation::move_encode,
    piece::Color,
    search::royal_probe_evasions,
    training::record::{GameRecordV2, GameStart},
};

fn choices(state: &mut GameState) -> Vec<Move> {
    royal_probe_evasions(state).unwrap_or_else(|| state.generate_legal_moves())
}

fn apply(state: &mut GameState, mv: Move) -> serde_json::Value {
    let before = BoardPosition::from_state(state);
    let mover = state.get_board().get_piece(mv.from).unwrap();
    let weights = EvalWeights::seed();
    let route = move_encode(&mv);
    state.make_move(mv.clone());
    let removed: Vec<_> = before.pieces.iter()
        .filter(|p| p.position != mv.from && state.get_board().get_piece(p.position) != Some(**p))
        .map(|p| serde_json::json!({"piece":format!("{:?}",p.piece_type),"color":p.color,
            "square":[p.position.file,p.position.rank],"seed_value":material_piece_value(p,&weights)}))
        .collect();
    let material = |b: &BoardPosition| -> f32 {
        b.pieces
            .iter()
            .map(|p| {
                material_piece_value(p, &weights) * if p.color == Color::Black { 1.0 } else { -1.0 }
            })
            .sum()
    };
    serde_json::json!({"route":route,"piece":format!("{:?}",mover.piece_type),"color":mover.color,
        "promoted":mv.promoted,"removed":removed,
        "material_black_delta":material(&BoardPosition::from_state(state))-material(&before)})
}

fn captures(state: &mut GameState) -> Vec<serde_json::Value> {
    let stm = state.get_current_turn();
    let moves = royal_probe_evasions(state)
        .unwrap_or_else(|| state.generate_legal_moves_mode(LegalMoveGen::CapturesOnly));
    let mut rows = Vec::new();
    for mv in moves {
        let mut copy = state.clone();
        let mut row = apply(&mut copy, mv);
        let victims = row["removed"].as_array().unwrap();
        let enemy: f64 = victims
            .iter()
            .filter(|p| p["color"] == serde_json::json!(stm.opposite()))
            .map(|p| p["seed_value"].as_f64().unwrap())
            .sum();
        let royal_victims = victims
            .iter()
            .filter(|p| {
                p["color"] == serde_json::json!(stm.opposite())
                    && matches!(p["piece"].as_str(), Some("King" | "CrownPrince"))
            })
            .count();
        if enemy < 300.0 && royal_victims == 0 {
            continue;
        }
        row["enemy_material_removed"] = serde_json::json!(enemy);
        row["royal_victims"] = serde_json::json!(royal_victims);
        rows.push(row);
    }
    rows.sort_by(|a, b| {
        b["royal_victims"]
            .as_u64()
            .unwrap()
            .cmp(&a["royal_victims"].as_u64().unwrap())
            .then_with(|| {
                b["enemy_material_removed"]
                    .as_f64()
                    .unwrap()
                    .total_cmp(&a["enemy_material_removed"].as_f64().unwrap())
            })
    });
    rows.truncate(12);
    rows
}

fn run() -> Result<(), String> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: tactical_line JOBS_JSON")?;
    let jobs: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut out = io::BufWriter::new(io::stdout().lock());
    for job in jobs.as_array().ok_or("jobs must be an array")? {
        let rec = GameRecordV2::load_path(std::path::Path::new(
            job["game"].as_str().ok_or("game missing")?,
        ))?;
        let ply = job["ply"].as_u64().ok_or("ply missing")? as usize;
        if ply == 0 || ply > rec.moves.len() {
            return Err("ply out of range".into());
        }
        let mut state = match &rec.start {
            GameStart::Position { position } => position.to_state(),
            GameStart::Opening => {
                let mut s = GameState::new();
                s.setup_initial_position();
                s
            }
        };
        for record in &rec.moves[..ply - 1] {
            if state.get_current_turn() != record.color {
                return Err("replay color mismatch".into());
            }
            let turn = state.get_current_turn();
            state.make_move(GameHistory::record_to_move(record)?);
            if state.get_current_turn() == turn {
                return Err("replay failed".into());
            }
        }
        let evasions = royal_probe_evasions(&mut state);
        let check = evasions.is_some();
        let royals: Vec<_> = BoardPosition::from_state(&state).pieces.into_iter()
            .filter(|p| p.piece_type.is_royal()).map(|p| serde_json::json!({
                "piece":format!("{:?}",p.piece_type),"color":p.color,
                "square":[p.position.file,p.position.rank],
                "attacked":state.get_board().is_position_attacked_by_color_for_check(p.position,p.color.opposite())
            })).collect();
        let evasion_routes = evasions.map(|ms| ms.iter().map(move_encode).collect::<Vec<_>>());
        let mut opponent = state.clone();
        opponent.set_current_turn(state.get_current_turn().opposite());
        let threats = captures(&mut opponent);
        let mut lines = Vec::new();
        for line in job["lines"].as_array().ok_or("lines missing")? {
            let mut copy = state.clone();
            let mut events = Vec::new();
            let mut error = None;
            for route in line["routes"].as_array().ok_or("routes missing")? {
                let route = route.as_str().ok_or("route must be string")?;
                let mut found: Vec<_> = choices(&mut copy)
                    .into_iter()
                    .filter(|m| move_encode(m) == route)
                    .collect();
                // Some move generators emit the same exact Move twice.
                let mut unique = Vec::new();
                for mv in found.drain(..) {
                    if !unique.contains(&mv) {
                        unique.push(mv);
                    }
                }
                let found = unique;
                if found.len() != 1 {
                    error = Some(format!("route {route}: {} available matches", found.len()));
                    break;
                }
                events.push(apply(&mut copy, found[0].clone()));
            }
            let replies = line["inspect_replies"]
                .as_bool()
                .unwrap_or(false)
                .then(|| captures(&mut copy));
            let reply_check = line["inspect_replies"]
                .as_bool()
                .unwrap_or(false)
                .then(|| royal_probe_evasions(&mut copy).is_some());
            lines.push(serde_json::json!({"label":line["label"],"events":events,"error":error,"reply_captures":replies,"reply_must_evade_last_royal_check":reply_check}));
        }
        let result = serde_json::json!({"case":job["case"],"ply":ply,"last_royal_in_check":check,"royals":royals,
            "evasion_routes":evasion_routes,"opponent_captures_if_given_turn":threats,
            "counterfactual_warning":"Opponent capture list switches side to move; it is a threat map, not a playable pass.",
            "lines":lines});
        serde_json::to_writer(&mut out, &result).map_err(|e| e.to_string())?;
        writeln!(out).map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("tactical_line: {e}");
        std::process::exit(1);
    }
}
