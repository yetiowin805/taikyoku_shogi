//! JSON-friendly session views and commands for the HTTP / GUI layer.
use crate::debug_tool::DebugTool;
use crate::game_history::{GameHistory, MoveRecord};
use crate::piece::Color;
use crate::training::eval_trace::{GameTrace, DEFAULT_EVAL_CLIP, DEFAULT_OUT_DIR};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

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

/// One ply's recorded AB telemetry from a saved game (black-absolute scores).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedMoveEval {
    /// 1-based ply index of this move in the effective timeline.
    pub ply: usize,
    pub side: String,
    pub label: String,
    pub eval: Option<i32>,
    pub static_eval: Option<i32>,
    pub nodes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecordedEvals {
    /// Move that produced the current position (`None` at ply 0).
    pub last: Option<RecordedMoveEval>,
    /// Next move still ahead in the game (`None` at the end).
    pub next: Option<RecordedMoveEval>,
}

/// One sample on the eval chart (black-absolute; positive = Black better).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalPointDto {
    pub ply: usize,
    pub eval: f32,
    /// Present for eval-trace samples (`true` = quiet / non-loud ply).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quiet: Option<bool>,
}

/// Eval trajectory for the loaded game (recorded AB scores or offline eval-trace).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSeriesDto {
    /// `"recorded"` (per-move AB) or `"eval_trace"` (offline scan).
    pub source: String,
    pub game_id: String,
    pub points: Vec<EvalPointDto>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub focus_plies: Vec<usize>,
}

impl EvalSeriesDto {
    /// Prefer search `eval`, else `static_eval`. Plotted at ply = move index + 1
    /// (position after the move). Soft-clipped so mate-ish scores don't crush the chart.
    pub fn from_recorded_moves(moves: &[MoveRecord], game_id: &str) -> Option<Self> {
        let cap = DEFAULT_EVAL_CLIP;
        let points: Vec<EvalPointDto> = moves
            .iter()
            .enumerate()
            .filter_map(|(i, mv)| {
                mv.eval.or(mv.static_eval).map(|score| {
                    let v = (score as f32).clamp(-cap, cap);
                    EvalPointDto {
                        ply: i + 1,
                        eval: v,
                        quiet: None,
                    }
                })
            })
            .collect();
        if points.is_empty() {
            return None;
        }
        Some(Self {
            source: "recorded".into(),
            game_id: game_id.to_string(),
            points,
            focus_plies: Vec::new(),
        })
    }

    pub fn from_eval_trace_file(game_id: &str) -> Option<Self> {
        let path = Path::new(DEFAULT_OUT_DIR).join(format!("{game_id}.json"));
        if !path.is_file() {
            return None;
        }
        let text = fs::read_to_string(&path).ok()?;
        let trace: GameTrace = serde_json::from_str(&text).ok()?;
        if trace.points.is_empty() {
            return None;
        }
        Some(Self {
            source: "eval_trace".into(),
            game_id: trace.game_id,
            points: trace
                .points
                .iter()
                .map(|p| EvalPointDto {
                    ply: p.ply,
                    eval: p.eval_clipped,
                    quiet: Some(p.quiet),
                })
                .collect(),
            focus_plies: trace.focus_plies,
        })
    }

    /// Recorded move evals win when present; otherwise companion eval-trace JSON.
    pub fn resolve_for_game(moves: &[MoveRecord], game_id: &str) -> Option<Self> {
        Self::from_recorded_moves(moves, game_id).or_else(|| Self::from_eval_trace_file(game_id))
    }
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
    #[serde(default)]
    pub recorded: RecordedEvals,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub ok: bool,
    pub message: String,
    pub snapshot: SessionSnapshot,
    pub moves: Option<Vec<MoveDto>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<crate::search::SearchInfo>,
    /// Always serialized (including `null`) so the GUI can clear a prior chart on load/new.
    #[serde(default)]
    pub eval_series: Option<EvalSeriesDto>,
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
        let draw = if self.game_state_ref().is_draw_by_progress_rule() {
            Some("100-move rule".to_string())
        } else if self.game_state_ref().is_draw_by_fivefold_repetition() {
            Some("fivefold repetition".to_string())
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
            recorded: self.recorded_evals(),
        }
    }

    fn recorded_from_move(&self, record: &MoveRecord, ply: usize) -> RecordedMoveEval {
        let label = GameHistory::record_to_move(record)
            .map(|mv| Self::format_move_public(&mv))
            .unwrap_or_else(|_| {
                format!(
                    "{},{} -> {},{}",
                    record.from_file, record.from_rank, record.to_file, record.to_rank
                )
            });
        RecordedMoveEval {
            ply,
            side: color_name(record.color),
            label,
            eval: record.eval,
            static_eval: record.static_eval,
            nodes: record.nodes,
        }
    }

    pub fn recorded_evals(&self) -> RecordedEvals {
        let moves = self.effective_moves();
        let cursor = self.cursor_ply();
        let last = cursor
            .checked_sub(1)
            .and_then(|i| moves.get(i).map(|m| self.recorded_from_move(m, cursor)));
        let next = moves
            .get(cursor)
            .map(|m| self.recorded_from_move(m, cursor + 1));
        RecordedEvals { last, next }
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
        self.suggest_agent_with_options(name, &crate::player::AgentOptions::default())
            .map(|(msg, _)| msg)
    }

    pub fn suggest_agent_with_options(
        &self,
        name: &str,
        opts: &crate::player::AgentOptions,
    ) -> Result<(String, Option<crate::search::SearchInfo>), String> {
        if matches!(name, "ab" | "search") {
            let player = crate::alphabeta_player::AlphaBetaPlayer::from_options(opts);
            let info = player.search_info(self.game_state_ref());
            let msg = match &info.best_move {
                Some(label) => format!(
                    "ab suggests: {} (eval {} → search {}, nodes {})",
                    label, info.static_eval, info.score, info.nodes
                ),
                None => "ab has no legal moves".to_string(),
            };
            return Ok((msg, Some(info)));
        }

        let player = crate::player::player_by_name_with_options(name, opts)?;
        match player.choose_move(self.game_state_ref()) {
            Some(mv) => Ok((
                format!(
                    "{} suggests: {}",
                    player.name(),
                    Self::format_move_public(&mv)
                ),
                None,
            )),
            None => Ok((format!("{} has no legal moves", player.name()), None)),
        }
    }

    pub fn play_agent(&mut self, name: &str) -> Result<String, String> {
        self.play_agent_with_options(name, &crate::player::AgentOptions::default())
            .map(|(msg, _)| msg)
    }

    pub fn play_agent_with_options(
        &mut self,
        name: &str,
        opts: &crate::player::AgentOptions,
    ) -> Result<(String, Option<crate::search::SearchInfo>), String> {
        if matches!(name, "ab" | "search") {
            let player = crate::alphabeta_player::AlphaBetaPlayer::from_options(opts);
            let side = match self.game_state_ref().get_current_turn() {
                Color::Black => "Black",
                Color::White => "White",
            };
            let result = player.analyze(self.game_state_ref());
            let info = crate::search::search_info_from_result(
                "ab",
                side,
                player.config().depth,
                &result,
            );
            let Some(mv) = result.best_move else {
                return Err("ab has no legal moves".to_string());
            };
            let msg = self.apply_live_move_pub(mv)?;
            let msg = format!(
                "ab: {} (eval {} → search {}, nodes {})",
                msg, info.static_eval, info.score, info.nodes
            );
            return Ok((msg, Some(info)));
        }

        let player = crate::player::player_by_name_with_options(name, opts)?;
        let pname = player.name().to_string();
        match player.choose_move(self.game_state_ref()) {
            Some(mv) => {
                let msg = self.apply_live_move_pub(mv)?;
                Ok((format!("{}: {}", pname, msg), None))
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
            search: None,
            eval_series: self.eval_series(),
        }
    }

    pub fn ok_result_with_search(
        &self,
        message: impl Into<String>,
        search: crate::search::SearchInfo,
    ) -> CommandResult {
        CommandResult {
            ok: true,
            message: message.into(),
            snapshot: self.snapshot(),
            moves: None,
            search: Some(search),
            eval_series: self.eval_series(),
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
            search: None,
            eval_series: self.eval_series(),
        }
    }

    pub fn err_result(&self, message: impl Into<String>) -> CommandResult {
        CommandResult {
            ok: false,
            message: message.into(),
            snapshot: self.snapshot(),
            moves: None,
            search: None,
            eval_series: self.eval_series(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_history::{MoveRecord, MoveRecordData};
    use crate::training::record::{AgentSpec, GameRecordV2, GameStart, GameStats, FORMAT_VERSION};

    #[test]
    fn snapshot_exposes_recorded_evals_at_cursor() {
        // Legal opening pawn push from a real AB game (internal coords).
        let mv = MoveRecord {
            move_number: 1,
            color: Color::Black,
            from_file: 17,
            from_rank: 9,
            to_file: 17,
            to_rank: 12,
            promoted: false,
            data: MoveRecordData::Standard,
            eval: Some(42),
            static_eval: Some(10),
            nodes: Some(99),
        };
        let v2 = GameRecordV2 {
            format_version: FORMAT_VERSION,
            game_id: "t".into(),
            seed: 0,
            black: AgentSpec::new("ab"),
            white: AgentSpec::new("ab"),
            start: GameStart::Opening,
            moves: vec![mv],
            result: None,
            stats: GameStats::default(),
            timestamp: 0,
            abort_reason: None,
        };
        let mut tool = DebugTool::new();
        tool.load_game_record_v2(v2).unwrap();
        let at0 = tool.recorded_evals();
        assert!(at0.last.is_none());
        assert_eq!(at0.next.as_ref().and_then(|m| m.eval), Some(42));
        tool.forward(1).unwrap();
        let at1 = tool.recorded_evals();
        assert_eq!(at1.last.as_ref().and_then(|m| m.eval), Some(42));
        assert_eq!(at1.last.as_ref().and_then(|m| m.static_eval), Some(10));
        assert!(at1.next.is_none());
        let series = tool.eval_series().expect("series");
        assert_eq!(series.source, "recorded");
        assert_eq!(series.points.len(), 1);
        assert_eq!(series.points[0].eval, 42.0);
    }

    #[test]
    fn recorded_series_prefers_search_eval() {
        let moves = vec![
            MoveRecord {
                move_number: 1,
                color: Color::Black,
                from_file: 0,
                from_rank: 0,
                to_file: 0,
                to_rank: 1,
                promoted: false,
                data: MoveRecordData::Standard,
                eval: Some(100),
                static_eval: Some(50),
                nodes: None,
            },
            MoveRecord {
                move_number: 2,
                color: Color::White,
                from_file: 0,
                from_rank: 1,
                to_file: 0,
                to_rank: 2,
                promoted: false,
                data: MoveRecordData::Standard,
                eval: None,
                static_eval: Some(20),
                nodes: None,
            },
        ];
        let s = EvalSeriesDto::from_recorded_moves(&moves, "g1").unwrap();
        assert_eq!(s.source, "recorded");
        assert_eq!(s.points.len(), 2);
        assert_eq!(s.points[0].ply, 1);
        assert_eq!(s.points[0].eval, 100.0);
        assert_eq!(s.points[1].ply, 2);
        assert_eq!(s.points[1].eval, 20.0);
    }

    #[test]
    fn recorded_series_empty_without_scores() {
        let moves = vec![MoveRecord {
            move_number: 1,
            color: Color::Black,
            from_file: 0,
            from_rank: 0,
            to_file: 0,
            to_rank: 1,
            promoted: false,
            data: MoveRecordData::Standard,
            eval: None,
            static_eval: None,
            nodes: None,
        }];
        assert!(EvalSeriesDto::from_recorded_moves(&moves, "g1").is_none());
    }
}
