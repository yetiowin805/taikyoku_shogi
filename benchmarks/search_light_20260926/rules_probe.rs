//! Counterexamples and invariants relevant to proposed repetition/TT shortcuts.
use taikyoku_shogi::{game_state::{GameState, Move}, piece::{Color, Piece, PieceType}, position::Position};
fn pos(file:u8,rank:u8)->Position { Position::new(file,rank).unwrap() }
fn main() {
    let mut game=GameState::new();game.clear_board();
    for (kind,color,square) in [(PieceType::King,Color::Black,pos(0,0)),
        (PieceType::GoldGeneral,Color::Black,pos(10,10)),(PieceType::King,Color::White,pos(35,35))] {
        game.place_piece(Piece::new(kind,color,square));
    }
    game.set_current_turn(Color::Black);game.reset_rep_history();
    let initial=game.repetition_key();let mut keys=vec![initial];
    let mut predicted=game.clone();
    let route=[((10,10),(11,11)),((35,35),(34,35)),((11,11),(11,10)),
        ((34,35),(34,34)),((11,10),(10,10)),((34,34),(35,35))];
    for ((x,y),(u,v)) in route {
        let mv=Move::new(pos(x,y),pos(u,v));
        assert!(game.generate_legal_moves().contains(&mv));
        let side=game.get_current_turn();
        game.make_move(mv.clone());predicted.make_move_for_search(mv);
        assert_ne!(game.get_current_turn(),side);
        // Future nodes searched from a parent retain the same draw counter as
        // subsequently playing that exact prefix: draw keys do not forbid hits.
        assert_eq!(game.hash(),predicted.hash());
        keys.push(game.repetition_key());
    }
    assert_eq!(game.repetition_key(),initial);
    let actual=game.repetition_count();
    let progress=game.get_turns_without_capture_or_promotion() as usize;
    let bounded=keys.iter().rev().take(progress+1).filter(|&&k|k==initial).count();
    assert_eq!(actual,2);assert_eq!(bounded,1);
    println!("{{\"full_repetition_count\":{actual},\"progress_bounded_count\":{bounded},\"progress_counter\":{progress},\"played_equals_predicted_tt_hash\":true}}");
}
