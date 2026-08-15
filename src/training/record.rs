//! Versioned game records for training (v2) with legacy v1 compatibility.

use crate::board_position::BoardPosition;
use crate::game_history::{GameRecord, GameResult, MoveRecord};
use crate::game_state::Move;
use crate::piece::Color;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const FORMAT_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_time_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quiescence_depth: Option<u32>,
    /// Path to a pinned historical binary (`think-loop`). None = in-process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
}

impl AgentSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            depth: None,
            model: None,
            max_time_ms: None,
            quiescence_depth: None,
            engine: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GameStart {
    Opening,
    Position { position: BoardPosition },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GameStats {
    pub move_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameRecordV2 {
    pub format_version: u32,
    pub game_id: String,
    pub seed: u64,
    pub black: AgentSpec,
    pub white: AgentSpec,
    pub start: GameStart,
    pub moves: Vec<MoveRecord>,
    pub result: Option<GameResult>,
    #[serde(default)]
    pub stats: GameStats,
    /// Wall-clock creation time (seconds since epoch).
    #[serde(default)]
    pub timestamp: u64,
    /// Set when the game aborted mid-play (partial dump for inspection).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abort_reason: Option<String>,
}

impl GameRecordV2 {
    pub fn new_id(seed: u64) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{:016x}-{:016x}", nanos, seed)
    }

    pub fn save_path(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize game: {}", e))?;
        fs::write(path, json).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
    }

    pub fn load_path(path: &Path) -> Result<Self, String> {
        let contents = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        load_game_json(&contents)
    }

    pub fn to_legacy(&self) -> GameRecord {
        GameRecord {
            timestamp: self.timestamp,
            moves: self.moves.clone(),
            result: self.result.clone(),
        }
    }
}

/// Load either v2 or legacy v1 game JSON.
pub fn load_game_json(contents: &str) -> Result<GameRecordV2, String> {
    if let Ok(v2) = serde_json::from_str::<GameRecordV2>(contents) {
        if v2.format_version >= 2 {
            return Ok(v2);
        }
    }
    let legacy: GameRecord = serde_json::from_str(contents)
        .map_err(|e| format!("Failed to parse game JSON: {}", e))?;
    Ok(legacy_to_v2(legacy))
}

pub fn legacy_to_v2(legacy: GameRecord) -> GameRecordV2 {
    GameRecordV2 {
        format_version: FORMAT_VERSION,
        game_id: format!("legacy-{}", legacy.timestamp),
        seed: 0,
        black: AgentSpec::new("unknown"),
        white: AgentSpec::new("unknown"),
        start: GameStart::Opening,
        moves: legacy.moves,
        result: legacy.result,
        stats: GameStats {
            move_count: 0,
            elapsed_ms: None,
        },
        timestamp: legacy.timestamp,
        abort_reason: None,
    }
}

pub fn move_to_record(mv: &Move, color: Color, move_number: usize) -> MoveRecord {
    crate::game_history::GameHistory::move_to_record(mv, color, move_number)
}

pub fn move_to_record_with_eval(
    mv: &Move,
    color: Color,
    move_number: usize,
    eval: Option<i32>,
    static_eval: Option<i32>,
    nodes: Option<u64>,
) -> MoveRecord {
    crate::game_history::GameHistory::move_to_record_with_eval(
        mv,
        color,
        move_number,
        eval,
        static_eval,
        nodes,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v2_round_trip() {
        let rec = GameRecordV2 {
            format_version: FORMAT_VERSION,
            game_id: "test-id".into(),
            seed: 42,
            black: AgentSpec::new("ab"),
            white: AgentSpec::new("mi"),
            start: GameStart::Opening,
            moves: vec![],
            result: Some(GameResult::Draw),
            stats: GameStats {
                move_count: 0,
                elapsed_ms: Some(10),
            },
            timestamp: 1,
            abort_reason: None,
        };
        let json = serde_json::to_string(&rec).unwrap();
        let back = load_game_json(&json).unwrap();
        assert_eq!(back.game_id, "test-id");
        assert_eq!(back.seed, 42);
    }

    #[test]
    fn legacy_loads() {
        let legacy = r#"{"timestamp":123,"moves":[],"result":"Draw"}"#;
        let v2 = load_game_json(legacy).unwrap();
        assert_eq!(v2.game_id, "legacy-123");
        assert!(matches!(v2.result, Some(GameResult::Draw)));
    }

    #[test]
    fn move_record_without_eval_fields_deserializes() {
        let json = r#"{
            "format_version": 2,
            "game_id": "g1",
            "seed": 1,
            "black": {"name": "ab"},
            "white": {"name": "ab"},
            "start": {"kind": "opening"},
            "moves": [{
                "move_number": 1,
                "color": "Black",
                "from_file": 0,
                "from_rank": 0,
                "to_file": 0,
                "to_rank": 1,
                "promoted": false,
                "data": "Standard"
            }],
            "result": null,
            "stats": {},
            "timestamp": 1
        }"#;
        let v2 = load_game_json(json).unwrap();
        assert_eq!(v2.moves.len(), 1);
        assert!(v2.moves[0].eval.is_none());
        assert!(v2.moves[0].static_eval.is_none());
        assert!(v2.moves[0].nodes.is_none());
    }

    #[test]
    fn move_record_with_eval_round_trips() {
        let mut rec = GameRecordV2 {
            format_version: FORMAT_VERSION,
            game_id: "g-eval".into(),
            seed: 1,
            black: AgentSpec::new("ab"),
            white: AgentSpec::new("ab"),
            start: GameStart::Opening,
            moves: vec![],
            result: None,
            stats: GameStats::default(),
            timestamp: 1,
            abort_reason: None,
        };
        let mv = crate::game_state::Move::new(
            crate::position::Position::new(0, 0).unwrap(),
            crate::position::Position::new(0, 1).unwrap(),
        );
        rec.moves.push(move_to_record_with_eval(
            &mv,
            Color::Black,
            1,
            Some(1234),
            Some(100),
            Some(99),
        ));
        let json = serde_json::to_string(&rec).unwrap();
        assert!(json.contains("\"eval\":1234"));
        let back = load_game_json(&json).unwrap();
        assert_eq!(back.moves[0].eval, Some(1234));
        assert_eq!(back.moves[0].static_eval, Some(100));
        assert_eq!(back.moves[0].nodes, Some(99));
    }
}
