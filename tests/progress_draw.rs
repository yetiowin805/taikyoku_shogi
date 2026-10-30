//! Progress-clock regressions against return paths, not immediate reverse reach.
use std::collections::{HashMap, HashSet};
use taikyoku_shogi::{
    board::Board,
    eval::ALL_PIECE_TYPES,
    game_state::{GameState, Move, PROGRESS_DRAW_LIMIT},
    movement::{
        move_is_directionally_irreversible, MovementCapability, MovementConfig, MovementGenerator,
    },
    piece::{Color, Piece, PieceType},
    position::Position,
};

fn forms() -> Vec<Piece> {
    let mut seen = HashSet::new();
    let mut out = vec![];
    for &kind in ALL_PIECE_TYPES {
        for color in [Color::Black, Color::White] {
            let mut piece = Piece::new(kind, color, Position::new(17, 17).unwrap());
            loop {
                if !seen.insert((
                    piece.piece_type,
                    piece.color,
                    piece.is_promoted,
                    piece.base_piece_type,
                )) {
                    break;
                }
                out.push(piece);
                if piece.piece_type.promotes_to().is_none() {
                    break;
                }
                piece.promote();
            }
        }
    }
    out
}

// Build a deliberately generous quiet-movement graph: each leg is allowed on
// its own, sliders can take unit steps, and jump distances can be subdivided.
// Every real route is contained in this envelope. We obtain the leg directions
// from the actual generator so the oracle doesn't duplicate color conventions.
fn envelope_offsets(piece: &Piece, cap: &MovementCapability, offsets: &mut Vec<(i8, i8)>) {
    if let MovementCapability::TwoStep { first, second } = cap {
        envelope_offsets(piece, first, offsets);
        envelope_offsets(piece, second, offsets);
        return;
    }
    for to in MovementGenerator::capability_landings(piece, &Board::new(), cap) {
        let x = to.file() as i8 - piece.position.file() as i8;
        let y = to.rank() as i8 - piece.position.rank() as i8;
        let (mut a, mut b) = (x.abs(), y.abs());
        while b != 0 {
            (a, b) = (b, a % b);
        }
        if a != 0 {
            offsets.push((x / a, y / a));
        }
    }
}

fn visit(node: usize, graph: &[Vec<usize>], seen: &mut [bool], order: &mut Vec<usize>) {
    if seen[node] {
        return;
    }
    seen[node] = true;
    for &next in &graph[node] {
        visit(next, graph, seen, order);
    }
    order.push(node);
}

fn mark(node: usize, graph: &[Vec<usize>], component: usize, ids: &mut [usize]) {
    if ids[node] != usize::MAX {
        return;
    }
    ids[node] = component;
    for &next in &graph[node] {
        mark(next, graph, component, ids);
    }
}

fn components(offsets: &[(i8, i8)]) -> Vec<usize> {
    let mut graph = vec![vec![]; 1296];
    let mut reverse = graph.clone();
    for (index, edges) in graph.iter_mut().enumerate() {
        let from = Position::new((index % 36) as u8, (index / 36) as u8).unwrap();
        for &(x, y) in offsets {
            if let Some(to) = from.offset(x, y) {
                edges.push(to.to_index());
                reverse[to.to_index()].push(index);
            }
        }
    }
    let mut order = vec![];
    let mut seen = vec![false; 1296];
    for i in 0..1296 {
        visit(i, &graph, &mut seen, &mut order);
    }
    let mut ids = vec![usize::MAX; 1296];
    for node in order.into_iter().rev() {
        mark(node, &reverse, node, &mut ids);
    }
    ids
}

#[test]
fn all_piece_forms_have_no_progress_resets_inside_quiet_cycles() {
    let mut cache = HashMap::new();
    let forms = forms();
    let mut transitions = 0;
    for piece in &forms {
        let mut offsets = vec![];
        for cap in &MovementConfig::for_piece(piece).capabilities {
            envelope_offsets(piece, cap, &mut offsets);
        }
        offsets.sort_unstable();
        offsets.dedup();
        let ids = cache
            .entry(offsets.clone())
            .or_insert_with(|| components(&offsets));
        for index in 0..1296 {
            let from = Position::new((index % 36) as u8, (index / 36) as u8).unwrap();
            for &(x, y) in &offsets {
                if let Some(to) = from.offset(x, y) {
                    transitions += 1;
                    if ids[index] == ids[to.to_index()] {
                        assert!(
                            !move_is_directionally_irreversible(piece, from, to),
                            "quiet return route exists: {piece:?}, {from:?} -> {to:?}"
                        );
                    }
                }
            }
        }
    }
    println!(
        "{} forms, {} movement envelopes, {} board transitions audited",
        forms.len(),
        cache.len(),
        transitions
    );
}

fn state_with_piece(piece: Piece) -> GameState {
    let mut state = GameState::new();
    state.place_piece(piece);
    state.set_current_turn(piece.color);
    state.set_turns_without_capture_or_promotion(PROGRESS_DRAW_LIMIT - 1);
    state.recompute_hash();
    state.reset_rep_history();
    state
}

#[test]
fn real_closed_routes_never_reset_in_play_or_search() {
    for (kind, route) in [
        (PieceType::GoldGeneral, vec![(0, 0), (1, 1), (1, 0), (0, 0)]),
        (
            PieceType::SilverGeneral,
            vec![(0, 0), (0, 1), (1, 0), (0, -1), (0, 0)],
        ),
        (
            PieceType::DrunkenElephant,
            vec![(0, 0), (0, 1), (1, 0), (0, 0)],
        ),
        (PieceType::BlindTiger, vec![(0, 0), (0, -1), (1, 0), (0, 0)]),
        (PieceType::FreeTiger, vec![(0, 0), (0, -1), (1, 0), (0, 0)]),
        (PieceType::Kirin, vec![(0, 0), (2, 0), (0, 0)]),
        (PieceType::Phoenix, vec![(0, 0), (2, 2), (0, 0)]),
    ] {
        for color in [Color::Black, Color::White] {
            let sign = if color == Color::Black { 1 } else { -1 };
            let positions: Vec<_> = route
                .iter()
                .map(|&(x, y)| Position::new((17 + sign * x) as u8, (17 + sign * y) as u8).unwrap())
                .collect();
            for pair in positions.windows(2) {
                let piece = Piece::new(kind, color, pair[0]);
                let mut play = state_with_piece(piece);
                let mut search = play.clone();
                let mv = Move::new(pair[0], pair[1]);
                assert!(
                    play.generate_legal_moves_for_pieces(&[piece]).contains(&mv),
                    "{piece:?} {mv:?}"
                );
                let hash = search.hash();
                let undo = search.make_move_for_search(mv.clone()).unwrap();
                let turn = play.get_current_turn();
                play.make_move(mv);
                assert_ne!(play.get_current_turn(), turn);
                assert_eq!(
                    play.get_turns_without_capture_or_promotion(),
                    PROGRESS_DRAW_LIMIT
                );
                assert_eq!(
                    search.get_turns_without_capture_or_promotion(),
                    PROGRESS_DRAW_LIMIT
                );
                assert!(play.is_draw_by_progress_rule());
                assert_eq!(play.hash(), search.hash());
                search.unmake_move_for_search(undo);
                assert_eq!(search.hash(), hash);
                assert_eq!(
                    search.get_turns_without_capture_or_promotion(),
                    PROGRESS_DRAW_LIMIT - 1
                );
            }
        }
    }
}

#[test]
fn irreversible_advancement_and_actual_promotion_still_reset() {
    for color in [Color::Black, Color::White] {
        for (kind, dx, dy) in [
            (PieceType::Pawn, 0, 1),
            (PieceType::Lance, 0, 5),
            (PieceType::Knight, 1, 2),
        ] {
            let piece = Piece::new(kind, color, Position::new(17, 17).unwrap());
            let to = piece
                .position
                .offset(dx, if color == Color::Black { dy } else { -dy })
                .unwrap();
            let mut state = state_with_piece(piece);
            state.make_move(Move::new(piece.position, to));
            assert_eq!(state.get_current_turn(), color.opposite(), "{piece:?}");
            assert_eq!(
                state.get_turns_without_capture_or_promotion(),
                0,
                "{piece:?}"
            );
        }
        let rank = if color == Color::Black { 25 } else { 10 };
        let from = Position::new(17, rank).unwrap();
        let to = from.offset(1, 0).unwrap();
        let piece = Piece::new(PieceType::GoldGeneral, color, from);
        let mut state = state_with_piece(piece);
        state.make_move(Move::new_with_promotion(from, to, true));
        assert_eq!(state.get_current_turn(), color.opposite());
        assert_eq!(state.get_turns_without_capture_or_promotion(), 0);
        assert_ne!(
            state.get_board().get_piece(to).unwrap().piece_type,
            piece.piece_type
        );
    }
}

#[test]
fn quiet_multi_leg_routes_share_play_search_and_undo_clock_effects() {
    let from = Position::new(17, 17).unwrap();
    for color in [Color::Black, Color::White] {
        let sign = if color == Color::Black { 1 } else { -1 };
        let diagonal = from.offset(2 * sign, 2 * sign).unwrap();
        let horizontal = from.offset(4 * sign, 0).unwrap();
        let orthogonal = from.offset(0, 2 * sign).unwrap();
        for (kind, mv) in [
            (
                PieceType::Tengu,
                Move::new_two_step(from, diagonal, horizontal),
            ),
            (
                PieceType::Peacock,
                Move::new_two_step(from, diagonal, horizontal),
            ),
            (
                PieceType::HookMover,
                Move::new_two_step(from, orthogonal, diagonal),
            ),
            (
                PieceType::FreeEagle,
                Move::new_free_eagle(
                    from,
                    diagonal,
                    vec![from, from.offset(sign, sign).unwrap(), diagonal],
                ),
            ),
        ] {
            let piece = Piece::new(kind, color, from);
            let mut play = state_with_piece(piece);
            let mut search = play.clone();
            let hash = search.hash();
            let undo = search.make_move_for_search(mv.clone()).unwrap();
            play.make_move(mv);
            assert_eq!(play.get_current_turn(), color.opposite(), "{piece:?}");
            assert_eq!(
                play.get_turns_without_capture_or_promotion(),
                PROGRESS_DRAW_LIMIT,
                "{piece:?}"
            );
            assert_eq!(play.hash(), search.hash());
            assert_eq!(
                search.get_turns_without_capture_or_promotion(),
                PROGRESS_DRAW_LIMIT
            );
            search.unmake_move_for_search(undo);
            assert_eq!(search.hash(), hash);
            assert_eq!(
                search.get_turns_without_capture_or_promotion(),
                PROGRESS_DRAW_LIMIT - 1
            );
        }
    }
}

#[test]
fn recorded_free_tiger_maneuvering_reaches_the_progress_draw() {
    // Slot 5646, original recorded plies 2016..=2115, from game
    // 18e33144739485b8-6b7e99155cbc9727. This is a replay, not a new match:
    // search aware of the corrected rule may choose different moves earlier.
    let record: taikyoku_shogi::training::record::GameRecordV2 =
        serde_json::from_str(include_str!("fixtures/progress_draw_free_tiger.json")).unwrap();
    let taikyoku_shogi::training::record::GameStart::Position { position } = &record.start else {
        panic!("fixture must contain its historical starting position");
    };
    let mut state = position.to_state().unwrap();
    assert_eq!(state.get_turns_without_capture_or_promotion(), 0);
    assert_eq!(record.moves.len(), PROGRESS_DRAW_LIMIT as usize);
    for (index, record) in record.moves.iter().enumerate() {
        let mv = taikyoku_shogi::game_history::GameHistory::record_to_move(record).unwrap();
        let turn = state.get_current_turn();
        state.make_move(mv);
        assert_ne!(state.get_current_turn(), turn);
        assert_eq!(
            state.get_turns_without_capture_or_promotion(),
            index as u32 + 1
        );
        assert_eq!(
            state.is_draw_by_progress_rule(),
            index + 1 == PROGRESS_DRAW_LIMIT as usize
        );
    }
}
