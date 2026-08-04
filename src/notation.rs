//! Taikyoku position/move interchange strings (TSFEN1 / TM1).
//!
//! Coordinates are engine-native 0-based `file,rank` (same as JSON / [`Position`]).
//! Piece ids are unique PascalCase [`PieceType`] variant names (not display symbols).

use crate::board_position::BoardPosition;
use crate::game_state::{Move, MoveData};
use crate::piece::{Color, Piece, PieceType};
use crate::position::Position;

/// Encode a board snapshot as a single-line TSFEN1 string.
pub fn tsfen_encode(pos: &BoardPosition) -> String {
    let turn = match pos.turn {
        Color::Black => "b",
        Color::White => "w",
    };
    let mut pieces: Vec<&Piece> = pos.pieces.iter().collect();
    pieces.sort_by_key(|p| (p.position.file, p.position.rank, color_key(p.color)));
    let mut out = format!("TSFEN1 {} {} {}", turn, pos.draw_counter, pieces.len());
    for p in pieces {
        out.push(' ');
        out.push_str(&encode_piece_token(p));
    }
    out
}

/// Decode TSFEN1 (or the alias `startpos`) into a [`BoardPosition`].
///
/// Accepts multiline input and strips `#` line comments.
pub fn tsfen_decode(input: &str) -> Result<BoardPosition, String> {
    let cleaned = strip_comments_and_join(input);
    let s = cleaned.trim();
    if s.is_empty() {
        return Err("empty TSFEN".into());
    }
    if s.eq_ignore_ascii_case("startpos") {
        let mut state = crate::game_state::GameState::new();
        state.setup_initial_position();
        return Ok(BoardPosition::from_state(&state));
    }

    let mut parts = s.split_whitespace();
    let magic = parts
        .next()
        .ok_or_else(|| "TSFEN: missing magic".to_string())?;
    if magic != "TSFEN1" {
        return Err(format!("TSFEN: expected TSFEN1, got '{}'", magic));
    }
    let turn = match parts
        .next()
        .ok_or_else(|| "TSFEN: missing turn".to_string())?
    {
        "b" | "B" => Color::Black,
        "w" | "W" => Color::White,
        other => return Err(format!("TSFEN: bad turn '{}'", other)),
    };
    let draw: u32 = parts
        .next()
        .ok_or_else(|| "TSFEN: missing draw".to_string())?
        .parse()
        .map_err(|_| "TSFEN: draw must be an integer".to_string())?;
    let n: usize = parts
        .next()
        .ok_or_else(|| "TSFEN: missing piece count".to_string())?
        .parse()
        .map_err(|_| "TSFEN: piece count must be an integer".to_string())?;

    let mut pieces = Vec::with_capacity(n);
    for _ in 0..n {
        let tok = parts
            .next()
            .ok_or_else(|| format!("TSFEN: expected {} pieces, found fewer", n))?;
        pieces.push(decode_piece_token(tok)?);
    }
    if parts.next().is_some() {
        return Err("TSFEN: trailing tokens after piece list".into());
    }

    Ok(BoardPosition {
        pieces,
        turn,
        draw_counter: draw,
    })
}

/// Encode a move as a TM1 token.
pub fn move_encode(mv: &Move) -> String {
    let mut squares = Vec::new();
    squares.push(mv.from);
    match &mv.data {
        MoveData::Standard => {
            squares.push(mv.to);
        }
        MoveData::TwoStep { intermediate } => {
            squares.push(*intermediate);
            squares.push(mv.to);
        }
        MoveData::FreeEagle { path } => {
            squares.extend_from_slice(path);
            if path.last() != Some(&mv.to) {
                squares.push(mv.to);
            }
        }
    }
    let mut s = squares
        .iter()
        .map(|p| format!("{},{}", p.file, p.rank))
        .collect::<Vec<_>>()
        .join("-");
    if mv.promoted {
        s.push('+');
    }
    s
}

/// Decode a TM1 move token into a [`Move`].
pub fn move_decode(token: &str) -> Result<Move, String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("empty move token".into());
    }
    let (body, promoted) = if let Some(rest) = token.strip_suffix('+') {
        (rest, true)
    } else {
        (token, false)
    };
    let square_strs: Vec<&str> = body.split('-').collect();
    if square_strs.len() < 2 {
        return Err(format!("TM1: need at least from-to, got '{}'", token));
    }
    let mut squares = Vec::with_capacity(square_strs.len());
    for s in square_strs {
        squares.push(parse_square(s)?);
    }
    let from = squares[0];
    let to = *squares.last().unwrap();
    let data = match squares.len() {
        2 => MoveData::Standard,
        3 => MoveData::TwoStep {
            intermediate: squares[1],
        },
        _ => {
            // from + path (path should end at `to`)
            let path = squares[1..].to_vec();
            MoveData::FreeEagle { path }
        }
    };
    Ok(Move {
        from,
        to,
        promoted,
        data,
    })
}

fn color_key(c: Color) -> u8 {
    match c {
        Color::Black => 0,
        Color::White => 1,
    }
}

fn strip_comments_and_join(input: &str) -> String {
    let mut out = String::new();
    for line in input.lines() {
        let line = match line.split_once('#') {
            Some((before, _)) => before,
            None => line,
        };
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(t);
    }
    if out.is_empty() {
        input.split('#').next().unwrap_or("").trim().to_string()
    } else {
        out
    }
}

fn piece_type_name(pt: PieceType) -> String {
    serde_json::to_value(pt)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| format!("{:?}", pt))
}

fn parse_piece_type(name: &str) -> Result<PieceType, String> {
    serde_json::from_value(serde_json::Value::String(name.to_string()))
        .map_err(|_| format!("unknown PieceType '{}'", name))
}

fn encode_piece_token(p: &Piece) -> String {
    let color = match p.color {
        Color::Black => 'B',
        Color::White => 'W',
    };
    let mut type_part = String::new();
    if p.is_promoted {
        type_part.push('+');
    }
    type_part.push_str(&piece_type_name(p.piece_type));
    if let Some(base) = p.base_piece_type {
        type_part.push('<');
        type_part.push_str(&piece_type_name(base));
        type_part.push('>');
    }
    format!(
        "{}:{}@{},{}",
        color, type_part, p.position.file, p.position.rank
    )
}

fn decode_piece_token(tok: &str) -> Result<Piece, String> {
    // B:+DragonKing<Rook>@18,30
    let (color_s, rest) = tok
        .split_once(':')
        .ok_or_else(|| format!("piece token missing ':': '{}'", tok))?;
    let color = match color_s {
        "B" | "b" => Color::Black,
        "W" | "w" => Color::White,
        other => return Err(format!("bad color '{}' in '{}'", other, tok)),
    };
    let (type_part, coord) = rest
        .split_once('@')
        .ok_or_else(|| format!("piece token missing '@': '{}'", tok))?;
    let pos = parse_square(coord)?;

    let (promoted, type_body) = if let Some(rest) = type_part.strip_prefix('+') {
        (true, rest)
    } else {
        (false, type_part)
    };

    let (type_name, base) = if let Some(open) = type_body.find('<') {
        if !type_body.ends_with('>') {
            return Err(format!("piece token bad base brackets: '{}'", tok));
        }
        let name = &type_body[..open];
        let base_name = &type_body[open + 1..type_body.len() - 1];
        if name.is_empty() || base_name.is_empty() {
            return Err(format!("piece token empty type/base: '{}'", tok));
        }
        (name, Some(parse_piece_type(base_name)?))
    } else {
        (type_body, None)
    };

    let piece_type = parse_piece_type(type_name)?;
    Ok(Piece {
        piece_type,
        color,
        position: pos,
        is_promoted: promoted,
        base_piece_type: base,
    })
}

fn parse_square(s: &str) -> Result<Position, String> {
    let (f, r) = s
        .split_once(',')
        .ok_or_else(|| format!("bad square '{}', expected file,rank", s))?;
    let file: u8 = f
        .parse()
        .map_err(|_| format!("bad file in '{}'", s))?;
    let rank: u8 = r
        .parse()
        .map_err(|_| format!("bad rank in '{}'", s))?;
    Position::new(file, rank).ok_or_else(|| format!("square out of range: '{}'", s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::GameState;

    fn opening_pos() -> BoardPosition {
        let mut state = GameState::new();
        state.setup_initial_position();
        BoardPosition::from_state(&state)
    }

    #[test]
    fn opening_tsfen_round_trip() {
        let pos = opening_pos();
        let s = tsfen_encode(&pos);
        assert!(s.starts_with("TSFEN1 b "));
        let back = tsfen_decode(&s).unwrap();
        assert_eq!(back.turn, pos.turn);
        assert_eq!(back.draw_counter, pos.draw_counter);
        assert_eq!(back.pieces.len(), pos.pieces.len());
        let again = tsfen_encode(&back);
        assert_eq!(s, again);
    }

    #[test]
    fn startpos_alias() {
        let a = tsfen_decode("startpos").unwrap();
        let b = opening_pos();
        assert_eq!(a.pieces.len(), b.pieces.len());
        assert_eq!(a.turn, Color::Black);
    }

    #[test]
    fn comments_and_multiline() {
        let pos = opening_pos();
        let s = tsfen_encode(&pos);
        let wrapped = format!("# header\n{}\n# trailer", s);
        let back = tsfen_decode(&wrapped).unwrap();
        assert_eq!(tsfen_encode(&back), s);
    }

    #[test]
    fn reject_unknown_piece() {
        let err = tsfen_decode("TSFEN1 b 0 1 B:NotARealPiece@0,0").unwrap_err();
        assert!(err.contains("unknown PieceType"));
    }

    #[test]
    fn promoted_with_base_round_trip() {
        let pos = BoardPosition {
            pieces: vec![Piece {
                piece_type: PieceType::DragonKing,
                color: Color::Black,
                position: Position::new(10, 20).unwrap(),
                is_promoted: true,
                base_piece_type: Some(PieceType::Rook),
            }],
            turn: Color::White,
            draw_counter: 3,
        };
        let s = tsfen_encode(&pos);
        assert!(s.contains("B:+DragonKing<Rook>@10,20"));
        let back = tsfen_decode(&s).unwrap();
        assert_eq!(back.pieces.len(), 1);
        assert_eq!(back.pieces[0].piece_type, PieceType::DragonKing);
        assert!(back.pieces[0].is_promoted);
        assert_eq!(back.pieces[0].base_piece_type, Some(PieceType::Rook));
        assert_eq!(back.turn, Color::White);
        assert_eq!(back.draw_counter, 3);
    }

    #[test]
    fn move_standard_and_promote() {
        let mv = Move::new_with_promotion(
            Position::new(1, 2).unwrap(),
            Position::new(1, 3).unwrap(),
            true,
        );
        let s = move_encode(&mv);
        assert_eq!(s, "1,2-1,3+");
        let back = move_decode(&s).unwrap();
        assert_eq!(back.from, mv.from);
        assert_eq!(back.to, mv.to);
        assert!(back.promoted);
        assert!(matches!(back.data, MoveData::Standard));
    }

    #[test]
    fn move_two_step() {
        let mv = Move::new_two_step(
            Position::new(0, 0).unwrap(),
            Position::new(1, 1).unwrap(),
            Position::new(2, 2).unwrap(),
        );
        let s = move_encode(&mv);
        assert_eq!(s, "0,0-1,1-2,2");
        let back = move_decode(&s).unwrap();
        match back.data {
            MoveData::TwoStep { intermediate } => {
                assert_eq!(intermediate, Position::new(1, 1).unwrap());
            }
            other => panic!("expected TwoStep, got {:?}", other),
        }
    }

    #[test]
    fn move_free_eagle() {
        let path = vec![
            Position::new(1, 0).unwrap(),
            Position::new(2, 0).unwrap(),
            Position::new(3, 0).unwrap(),
        ];
        let mv = Move::new_free_eagle(
            Position::new(0, 0).unwrap(),
            Position::new(3, 0).unwrap(),
            path.clone(),
        );
        let s = move_encode(&mv);
        assert_eq!(s, "0,0-1,0-2,0-3,0");
        let back = move_decode(&s).unwrap();
        match back.data {
            MoveData::FreeEagle { path: p } => assert_eq!(p, path),
            other => panic!("expected FreeEagle, got {:?}", other),
        }
        assert_eq!(back.to, Position::new(3, 0).unwrap());
    }
}
