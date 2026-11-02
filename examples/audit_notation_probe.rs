use taikyoku_shogi::game_state::GameState;
use taikyoku_shogi::notation::move_decode;
use taikyoku_shogi::piece::{Color, Piece, PieceType};
use taikyoku_shogi::position::Position;

fn p(file: u8, rank: u8) -> Position {
    Position::new(file, rank).unwrap()
}

fn main() {
    let mut state = GameState::new();
    state.place_piece(Piece::new(PieceType::FreeEagle, Color::Black, p(18, 18)));
    state.place_piece(Piece::new(PieceType::Pawn, Color::White, p(19, 19)));
    let token = "18,18-19,19-20,20-21,21";
    let decoded = move_decode(token).unwrap();
    let intended = state
        .generate_legal_moves()
        .into_iter()
        .find(|mv| {
            mv.free_eagle_path()
                .is_some_and(|path| path == &[p(18, 18), p(19, 19), p(20, 20), p(21, 21)])
        })
        .unwrap();
    let before_hash = state.hash();
    let before_turn = state.get_current_turn();
    let before_board = state.clone_board();
    state.make_move(decoded.clone());
    let rejected = state.get_current_turn() == before_turn;
    let no_mutation = state.hash() == before_hash
        && (0..1296).all(|i| {
            let square = Position::from_index(i).unwrap();
            state.get_board().get_piece(square) == before_board.get_piece(square)
        });
    let mut generated_state = GameState::new();
    generated_state.restore_position(before_board, before_turn, 0);
    generated_state.make_move(intended.clone());
    println!("token={token} parsed={decoded:?} intended={intended:?} checked_rejected={rejected} no_mutation={no_mutation} generated_applied={} captured_pawn={}",
        generated_state.get_current_turn() != before_turn,
        generated_state.get_board().get_piece(p(19, 19)).is_none());
    assert!(rejected && no_mutation);
    assert!(generated_state.get_current_turn() != before_turn);
}
