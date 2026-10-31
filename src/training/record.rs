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

fn deserialize_format_version<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<u32, D::Error> {
    let version = u32::deserialize(deserializer)?;
    if version != FORMAT_VERSION {
        return Err(serde::de::Error::custom(format!(
            "unsupported game format version {version}; expected {FORMAT_VERSION}"
        )));
    }
    Ok(version)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    /// Exact executable used by this side; absent in legacy records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_sha256: Option<String>,
    /// Immutable rolling build, including its matching analysis helper.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_build: Option<super::game_process::EngineBundle>,
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
            engine_sha256: None,
            engine_build: None,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock: Option<super::clock::ClockRecord>,
    #[serde(default)]
    pub move_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameRecordV2 {
    #[serde(deserialize_with = "deserialize_format_version")]
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
        // Compact: per-move telemetry makes indentation a large share of each game.
        let json =
            serde_json::to_string(self).map_err(|e| format!("Failed to serialize game: {}", e))?;
        super::durable::write(path, json.as_bytes(), false)
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
    if let Some(record) = load_game_json_if_game(contents)? {
        return Ok(record);
    }
    // Preserve the public legacy decoder's diagnostics and sequence support.
    // Directory featurization instead skips JSON without game-identifying keys.
    serde_json::from_str(contents)
        .map(legacy_to_v2)
        .map_err(|e| format!("Failed to parse legacy game JSON: {e}"))
}

#[derive(Deserialize)]
struct GameFields {
    #[serde(default, deserialize_with = "field_is_present")]
    format_version: bool,
    #[serde(default, deserialize_with = "field_is_present")]
    moves: bool,
}

fn field_is_present<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    // Presence, including a null value, selects the versioned schema. Ignore
    // payloads without materializing their strings, objects, or move arrays.
    serde::de::IgnoredAny::deserialize(deserializer)?;
    Ok(true)
}

/// Like `load_game_json`, but skip valid JSON without game-identifying fields.
/// Featurization uses this so ordinary v2 games are deserialized only once.
pub(crate) fn load_game_json_if_game(contents: &str) -> Result<Option<GameRecordV2>, String> {
    if !contents.trim_start().starts_with('{') {
        serde_json::from_str::<serde::de::IgnoredAny>(contents)
            .map_err(|e| format!("Failed to parse game JSON: {e}"))?;
        return Ok(None);
    }
    // Ordinary v2 records need no intermediate JSON tree. Keep the error until
    // the fallback inspects version presence: malformed v2 must never be
    // reinterpreted as a legacy game, even if the legacy fields are valid.
    let versioned_error = match serde_json::from_str(contents) {
        Ok(record) => return Ok(Some(record)),
        Err(error) => error,
    };
    let fields: GameFields =
        serde_json::from_str(contents).map_err(|e| format!("Failed to parse game JSON: {e}"))?;
    if fields.format_version {
        return Err(format!(
            "Failed to parse versioned game JSON: {versioned_error}"
        ));
    }
    if !fields.moves {
        return Ok(None);
    }
    let legacy: GameRecord = serde_json::from_str(contents)
        .map_err(|e| format!("Failed to parse legacy game JSON: {e}"))?;
    Ok(Some(legacy_to_v2(legacy)))
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
            clock: None,
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

    fn versioned_fixture() -> serde_json::Value {
        serde_json::json!({
            "format_version": FORMAT_VERSION, "game_id": "versioned", "seed": 42,
            "black": {"name": "ab"}, "white": {"name": "mi"},
            "start": {"kind": "position", "position": {
                "pieces": [], "turn": "White", "draw_counter": 12
            }},
            "moves": [], "result": "Draw", "timestamp": 1,
            "stats": {"move_count": 0}
        })
    }

    #[test]
    fn game_detection_preserves_metadata_and_legacy_behavior() {
        for metadata in [r#"{"slots":[]}"#, "[]", "null", "42"] {
            assert!(load_game_json_if_game(metadata).unwrap().is_none());
            assert!(load_game_json(metadata).is_err());
        }
        assert!(load_game_json_if_game("{broken").is_err());
        assert!(load_game_json_if_game(r#"{"moves":"bad"}"#).is_err());
        assert!(
            load_game_json(r#"{"timestamp":1,"moves":[],"moves":[],"result":"Draw"}"#).is_err()
        );
        let legacy = r#"{"timestamp":123,"moves":[],"result":"Draw"}"#;
        let detected = load_game_json_if_game(legacy).unwrap().unwrap();
        assert_eq!(detected.game_id, "legacy-123");
        assert_eq!(
            detected.timestamp,
            load_game_json(legacy).unwrap().timestamp
        );

        // A sequence is accepted by the existing legacy deserializer, but is
        // metadata to the directory featurizer because it has no named fields.
        assert!(load_game_json_if_game(r#"[123,[],"Draw"]"#)
            .unwrap()
            .is_none());
        assert_eq!(load_game_json(r#"[123,[],"Draw"]"#).unwrap().timestamp, 123);
    }

    #[test]
    fn fast_game_detection_preserves_versioned_metadata_and_errors() {
        let valid = versioned_fixture().to_string();
        let detected = load_game_json_if_game(&valid).unwrap().unwrap();
        assert_eq!(detected.game_id, "versioned");
        assert_eq!(detected.seed, 42);
        assert!(matches!(detected.start, GameStart::Position { .. }));
        for version in [serde_json::Value::Null, serde_json::json!(3)] {
            let mut value = versioned_fixture();
            value["format_version"] = version;
            assert!(load_game_json_if_game(&value.to_string()).is_err());
        }
        let duplicate = valid.replacen('{', r#"{"format_version":2,"#, 1);
        assert!(load_game_json(&duplicate).is_err());
        assert!(load_game_json_if_game(&duplicate).is_err());
    }

    #[test]
    fn malformed_versioned_fields_never_fall_back_to_legacy() {
        for (field, invalid) in [
            (
                "start",
                serde_json::json!({"kind": "position", "position": "bad"}),
            ),
            ("black", serde_json::json!(false)),
            ("stats", serde_json::json!({"move_count": "bad"})),
        ] {
            let mut value = versioned_fixture();
            value[field] = invalid;
            let error = load_game_json(&value.to_string()).unwrap_err();
            assert!(error.contains("versioned game"), "{error}");
            // This is exactly the permissive legacy fallback that used to
            // discard the malformed versioned metadata and select Opening.
            assert!(serde_json::from_value::<GameRecord>(value).is_ok());
        }
    }

    #[test]
    fn explicit_versions_must_be_supported_in_all_deserialization_paths() {
        for version in [
            serde_json::json!(0),
            serde_json::json!(1),
            serde_json::json!(3),
            serde_json::json!(999),
            serde_json::json!(null),
            serde_json::json!("2"),
        ] {
            let mut value = versioned_fixture();
            value["format_version"] = version;
            assert!(load_game_json(&value.to_string()).is_err());
            assert!(serde_json::from_value::<GameRecordV2>(value).is_err());
        }
    }

    #[test]
    fn versioned_position_and_agent_metadata_survive_loading() {
        let record = load_game_json(&versioned_fixture().to_string()).unwrap();
        assert_eq!(record.game_id, "versioned");
        assert_eq!(record.seed, 42);
        assert_eq!(record.black.name, "ab");
        assert_eq!(record.white.name, "mi");
        let GameStart::Position { position } = record.start else {
            panic!("versioned start was lost");
        };
        assert_eq!(position.turn, Color::White);
        assert_eq!(position.draw_counter, 12);
    }

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
                clock: None,
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
        assert_eq!(v2.game_id, "g1");
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
