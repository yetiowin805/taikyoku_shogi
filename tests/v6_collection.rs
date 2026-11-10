use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use taikyoku_shogi::{
    board_position::BoardPosition,
    game_state::GameState,
    piece::{Color, Piece, PieceType},
    position::Position,
};

#[test]
fn collection_worker_reuses_history_and_checks_branches() {
    let root = std::env::temp_dir().join(format!("v6-worker-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut state = GameState::new();
    state.clear_board();
    for (kind, color, x, y) in [
        (PieceType::King, Color::Black, 0, 0),
        (PieceType::King, Color::White, 35, 35),
        (PieceType::Rook, Color::Black, 4, 4),
        (PieceType::Pawn, Color::White, 20, 20),
    ] {
        state.place_piece(Piece::new(kind, color, Position::new(x, y).unwrap()));
    }
    let record = json!({"format_version":2,"game_id":"v6-test","seed":1,
        "black":{"name":"ab"},"white":{"name":"ab"},
        "start":{"kind":"position","position":BoardPosition::from_state(&state)},
        "moves":[{"move_number":1,"color":"Black","from_file":0,"from_rank":0,"to_file":0,"to_rank":1,"promoted":false,"data":"Standard"},
                 {"move_number":2,"color":"White","from_file":35,"from_rank":35,"to_file":35,"to_rank":34,"promoted":false,"data":"Standard"}],
        "result":"Draw"});
    let game = root.join("game.json");
    std::fs::write(&game, serde_json::to_vec(&record).unwrap()).unwrap();
    let mut model: Value =
        serde_json::from_slice(&std::fs::read("models/ab-seed.json").unwrap()).unwrap();
    model["search_defaults"]["quiescence_depth"] = json!(0);
    model["search_defaults"]["experiment"] = json!("history_tactical_lmr_q_fix");
    model["search_defaults"]["trial"] = json!("growth_aware");
    let checkpoint = root.join("model.json");
    std::fs::write(&checkpoint, serde_json::to_vec(&model).unwrap()).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_tactical_tree_worker"))
        .args([game.to_str().unwrap(), "1", checkpoint.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&line).unwrap()["collection_protocol"],
        1
    );
    let mut call = |request: Value| {
        writeln!(input, "{request}").unwrap();
        input.flush().unwrap();
        let mut events = Vec::new();
        loop {
            let mut line = String::new();
            assert!(output.read_line(&mut line).unwrap() > 0);
            let event: Value = serde_json::from_str(&line).unwrap();
            let done = event["event"] == "complete" || event["event"] == "error";
            events.push(event);
            if done {
                return events;
            }
        }
    };
    let request = |id: &str, ply| json!({"version":1,"id":id,"op":"collection_probe","root_ply":ply,"depth":0,"time_ms":10});
    let start = call(request("first", 1)).pop().unwrap();
    assert_eq!(start["history_len"], 0);
    assert_eq!(start["turn"], 1);
    let exported = call(json!({"version":1,"id":"features","op":"training_features","root_ply":1}))
        .pop()
        .unwrap();
    assert_eq!(
        exported["canonical_position_hash"],
        start["canonical_position_hash"]
    );
    assert_eq!(exported["state_history_hash"], start["state_history_hash"]);
    assert!(!exported["us"].as_array().unwrap().is_empty());
    let later = call(request("later", 3)).pop().unwrap();
    assert_eq!(later["history_len"], 2);
    let back = call(request("back", 2)).pop().unwrap();
    assert_eq!(back["history_len"], 1);
    assert_eq!(back["turn"], -1);
    let again = call(request("again", 1)).pop().unwrap();
    assert_eq!(again["state_history_hash"], start["state_history_hash"]);
    let bad = call(
        json!({"version":1,"id":"bad","op":"collection_probe","root_ply":1,
        "depth":0,"time_ms":10,"path":[{"from":{"file":0,"rank":0},"to":{"file":30,"rank":30},"promoted":false,"data":"Standard"}]}),
    );
    assert_eq!(bad.last().unwrap()["event"], "error");
    let searched = call(
        json!({"version":1,"id":"search","op":"search","root_ply":1,"depth":1,
        "time_ms":1000}),
    );
    assert!(searched.iter().any(|event| event["event"] == "iteration"));
    assert_eq!(searched.last().unwrap()["completed_depth"], 1);
    let policy = searched.last().unwrap()["search_config"].as_str().unwrap();
    assert!(policy.contains("GrowthAware"), "{policy}");
    assert!(policy.contains("quiescence_depth: 0"), "{policy}");
    drop(input);
    assert!(child.wait().unwrap().success());
    std::fs::remove_dir_all(root).unwrap();
}
