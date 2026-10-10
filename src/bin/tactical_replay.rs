//! Export exact replay events and requested pre-move positions for tactical corpora.
use std::{
    collections::HashSet,
    io::{self, Write},
};
use taikyoku_shogi::{
    board_position::BoardPosition,
    eval::{material_piece_value, EvalWeights},
    game_history::GameHistory,
    game_state::GameState,
    notation::{move_encode, tsfen_encode},
    piece::Color,
    training::record::{GameRecordV2, GameStart},
};

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: tactical_replay GAME POSITION_PLIES_CSV".into());
    }
    let wanted: HashSet<usize> = args[2]
        .split(',')
        .filter(|x| !x.is_empty())
        .map(|x| x.parse().map_err(|_| format!("invalid ply {x}")))
        .collect::<Result<_, _>>()?;
    let rec = GameRecordV2::load_path(std::path::Path::new(&args[1]))?;
    if wanted.iter().any(|&p| p == 0 || p > rec.moves.len()) {
        return Err("position ply out of range".into());
    }
    let mut state = match &rec.start {
        GameStart::Position { position } => position.to_state(),
        GameStart::Opening => {
            let mut s = GameState::new();
            s.setup_initial_position();
            s
        }
    };
    let weights = EvalWeights::seed();
    let mut output = io::BufWriter::new(io::stdout().lock());
    for (i, record) in rec.moves.iter().enumerate() {
        let ply = i + 1;
        if state.get_current_turn() != record.color {
            return Err(format!("color mismatch at {ply}"));
        }
        let mv = GameHistory::record_to_move(record)?;
        let mover = state
            .get_board()
            .get_piece(mv.from)
            .ok_or(format!("missing mover at {ply}"))?;
        if mover.color != record.color {
            return Err(format!("mover color mismatch at {ply}"));
        }
        let before = BoardPosition::from_state(&state);
        let position = wanted.contains(&ply).then(|| tsfen_encode(&before));
        let from = mv.from;
        let to = mv.to;
        let route = move_encode(&mv);
        let turn = state.get_current_turn();
        state.make_move(mv);
        if state.get_current_turn() == turn {
            return Err(format!("failed move at {ply}"));
        }
        let removed: Vec<_>=before.pieces.iter().filter(|p|p.position!=from && state.get_board().get_piece(p.position)!=Some(**p))
            .map(|p|serde_json::json!({"piece":format!("{:?}",p.piece_type),"color":p.color,
                "square":[p.position.file,p.position.rank],"seed_value":material_piece_value(p,&weights)})).collect();
        let after = BoardPosition::from_state(&state);
        let material = |b: &BoardPosition| -> f32 {
            b.pieces
                .iter()
                .map(|p| {
                    material_piece_value(p, &weights)
                        * if p.color == Color::Black { 1.0 } else { -1.0 }
                })
                .sum()
        };
        let row = serde_json::json!({"ply":ply,"record_move_number":record.move_number,"color":record.color,
            "piece":format!("{:?}",mover.piece_type),"route":route,"promoted":record.promoted,
            "removed":removed,"mover_survives":state.get_board().get_piece(to).is_some_and(|p|p.color==turn),
            "material_black_before":material(&before),"material_black_after":material(&after),
            "position_before":position,"position_hash_before":wanted.contains(&ply).then(||state_hash_text(&before))});
        serde_json::to_writer(&mut output, &row).map_err(|e| e.to_string())?;
        writeln!(output).map_err(|e| e.to_string())?;
    }
    output.flush().map_err(|e| e.to_string())
}
fn state_hash_text(b: &BoardPosition) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(tsfen_encode(b).as_bytes()))
}
fn main() {
    if let Err(e) = run() {
        eprintln!("tactical_replay: {e}");
        std::process::exit(1)
    }
}
