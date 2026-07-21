//! JSON-friendly session views and commands for the HTTP / GUI layer.
use crate::debug_tool::DebugTool;
use crate::piece::Color;
use crate::player::player_by_name;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PieceDto {
    pub file: u8,
    pub rank: u8,
    pub color: String,
    pub piece_type: String,
    pub symbol: String,
    pub promoted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveDto {
    pub index: usize,
    pub from_file: u8,
    pub from_rank: u8,
    pub to_file: u8,
    pub to_rank: u8,
    pub promoted: bool,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub cursor: usize,
    pub timeline_len: usize,
    pub turn: String,
    pub pieces: Vec<PieceDto>,
    pub black_in_check: bool,
    pub white_in_check: bool,
    pub winner: Option<String>,
    pub draw: Option<String>,
    pub legal_move_count: usize,
    pub status_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub ok: bool,
    pub message: String,
    pub snapshot: SessionSnapshot,
    pub moves: Option<Vec<MoveDto>>,
}

fn color_name(c: Color) -> String {
    match c {
        Color::Black => "Black".to_string(),
        Color::White => "White".to_string(),
    }
}

impl DebugTool {
    pub fn snapshot(&self) -> SessionSnapshot {
        let board = self.game_state_ref().get_board();
        let mut pieces = Vec::new();
        for color in [Color::Black, Color::White] {
            for piece in board.get_pieces_by_color(color) {
                pieces.push(PieceDto {
                    file: Self::to_shogi_file(piece.position.file),
                    rank: Self::to_shogi_rank(piece.position.rank),
                    color: color_name(piece.color),
                    piece_type: format!("{:?}", piece.piece_type),
                    symbol: piece.base_symbol().to_string(),
                    promoted: piece.is_promoted,
                });
            }
        }

        let winner = self.game_state_ref().get_winner().map(color_name);
        let draw = if self.game_state_ref().is_draw_by_500_move_rule() {
            Some("500-move rule".to_string())
        } else if self.game_state_ref().is_draw_by_insufficient_material() {
            Some("insufficient material".to_string())
        } else {
            None
        };

        SessionSnapshot {
            cursor: self.cursor_ply(),
            timeline_len: self.timeline_length(),
            turn: color_name(self.game_state_ref().get_current_turn()),
            pieces,
            black_in_check: self.check_color(Color::Black),
            white_in_check: self.check_color(Color::White),
            winner,
            draw,
            legal_move_count: self.game_state_ref().generate_legal_moves().len(),
            status_text: self.status_summary(),
        }
    }

    pub fn legal_moves_dto(&self, from: Option<(u8, u8)>) -> Result<Vec<MoveDto>, String> {
        let moves = if let Some((sf, sr)) = from {
            let pos = self.parse_shogi_position(sf, sr)?;
            if let Some(piece) = self.game_state_ref().get_board().get_piece(pos) {
                if piece.color == self.game_state_ref().get_current_turn() {
                    self.game_state_ref()
                        .generate_legal_moves_for_pieces(&[piece])
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            self.game_state_ref().generate_legal_moves()
        };

        Ok(moves
            .into_iter()
            .enumerate()
            .map(|(index, mv)| MoveDto {
                index,
                from_file: Self::to_shogi_file(mv.from.file),
                from_rank: Self::to_shogi_rank(mv.from.rank),
                to_file: Self::to_shogi_file(mv.to.file),
                to_rank: Self::to_shogi_rank(mv.to.rank),
                promoted: mv.promoted,
                label: Self::format_move_public(&mv),
            })
            .collect())
    }

    pub fn apply_human_move(
        &mut self,
        from_file: u8,
        from_rank: u8,
        to_file: u8,
        to_rank: u8,
        promote: Option<bool>,
        path_index: Option<usize>,
    ) -> Result<String, String> {
        let from = self.parse_shogi_position(from_file, from_rank)?;
        let to = self.parse_shogi_position(to_file, to_rank)?;
        let matches = self.find_matching_moves_pub(from, to, promote);
        if matches.is_empty() {
            return Err("No legal move matches those squares".to_string());
        }
        let chosen = if matches.len() == 1 {
            matches[0].clone()
        } else if let Some(i) = path_index {
            matches
                .get(i)
                .cloned()
                .ok_or_else(|| format!("Path index {} out of range", i))?
        } else {
            let mut msg = format!("{} matching moves; pass path_index:\n", matches.len());
            for (i, mv) in matches.iter().enumerate() {
                msg.push_str(&format!("  [{}] {}\n", i, Self::format_move_public(mv)));
            }
            return Err(msg);
        };
        self.apply_live_move_pub(chosen)
    }

    pub fn suggest_agent(&self, name: &str) -> Result<String, String> {
        let player = player_by_name(name)?;
        match player.choose_move(self.game_state_ref()) {
            Some(mv) => Ok(format!(
                "{} suggests: {}",
                player.name(),
                Self::format_move_public(&mv)
            )),
            None => Ok(format!("{} has no legal moves", player.name())),
        }
    }

    pub fn play_agent(&mut self, name: &str) -> Result<String, String> {
        let player = player_by_name(name)?;
        let pname = player.name().to_string();
        match player.choose_move(self.game_state_ref()) {
            Some(mv) => {
                let msg = self.apply_live_move_pub(mv)?;
                Ok(format!("{}: {}", pname, msg))
            }
            None => Err(format!("{} has no legal moves", pname)),
        }
    }

    pub fn ok_result(&self, message: impl Into<String>) -> CommandResult {
        CommandResult {
            ok: true,
            message: message.into(),
            snapshot: self.snapshot(),
            moves: None,
        }
    }

    pub fn ok_result_with_moves(
        &self,
        message: impl Into<String>,
        moves: Vec<MoveDto>,
    ) -> CommandResult {
        CommandResult {
            ok: true,
            message: message.into(),
            snapshot: self.snapshot(),
            moves: Some(moves),
        }
    }

    pub fn err_result(&self, message: impl Into<String>) -> CommandResult {
        CommandResult {
            ok: false,
            message: message.into(),
            snapshot: self.snapshot(),
            moves: None,
        }
    }
}
