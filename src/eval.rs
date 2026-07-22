//! Static evaluation and versioned weight checkpoints for the alpha-beta agent.

use crate::board::Board;
use crate::game_state::GameState;
use crate::piece::{Color, Piece, PieceType};
use crate::position::Position;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// All piece types (for seed export / complete tables).
pub const ALL_PIECE_TYPES: &[PieceType] = &[
    PieceType::King,
    PieceType::Pawn,
    PieceType::GoldGeneral,
    PieceType::Dog,
    PieceType::MixedGeneral,
    PieceType::GoBetween,
    PieceType::DrunkenElephant,
    PieceType::CrownPrince,
    PieceType::NeighboringKing,
    PieceType::FrontStandard,
    PieceType::Rook,
    PieceType::LeftGeneral,
    PieceType::RightGeneral,
    PieceType::LeftArmy,
    PieceType::RightArmy,
    PieceType::RearStandard,
    PieceType::CenterStandard,
    PieceType::FreeKing,
    PieceType::GreatGeneral,
    PieceType::FreeBaku,
    PieceType::FreeDemon,
    PieceType::RunningHorse,
    PieceType::Tengu,
    PieceType::WoodenDove,
    PieceType::CeramicDove,
    PieceType::EarthDragon,
    PieceType::RainDragon,
    PieceType::LeftMountainEagle,
    PieceType::RightMountainEagle,
    PieceType::FlyingEagle,
    PieceType::FireDemon,
    PieceType::FreeFire,
    PieceType::Whale,
    PieceType::GreatWhale,
    PieceType::RunningRabbit,
    PieceType::TreacherousFox,
    PieceType::MountainCrane,
    PieceType::TurtleSnake,
    PieceType::DivineTurtle,
    PieceType::WhiteTiger,
    PieceType::DivineTiger,
    PieceType::Lance,
    PieceType::WhiteFoal,
    PieceType::BeastCadet,
    PieceType::BeastOfficer,
    PieceType::BeastBird,
    PieceType::FlyingSwallow,
    PieceType::GreatDragon,
    PieceType::PrimordialDragon,
    PieceType::MountainStag,
    PieceType::GreatStag,
    PieceType::SilverGeneral,
    PieceType::VerticalMover,
    PieceType::Rikishi,
    PieceType::Kongou,
    PieceType::Rasetsu,
    PieceType::Yasha,
    PieceType::Shiten,
    PieceType::RunningBear,
    PieceType::FreeBear,
    PieceType::RunningTiger,
    PieceType::FreeTiger,
    PieceType::GreatDove,
    PieceType::SideSerpent,
    PieceType::GreatShark,
    PieceType::RunningSerpent,
    PieceType::FreeSerpent,
    PieceType::RunningPup,
    PieceType::FreeLeopard,
    PieceType::ForestDemon,
    PieceType::ThunderRunner,
    PieceType::FowlOfficer,
    PieceType::Fowl,
    PieceType::Turtledove,
    PieceType::WhiteElephant,
    PieceType::FragrantElephant,
    PieceType::ElephantKing,
    PieceType::ReverseChariot,
    PieceType::LeftDragon,
    PieceType::VermillionSparrow,
    PieceType::DivineSparrow,
    PieceType::RightDragon,
    PieceType::BlueDragon,
    PieceType::DivineDragon,
    PieceType::LeftTiger,
    PieceType::RightTiger,
    PieceType::FlyingGeneral,
    PieceType::FlyingCrocodile,
    PieceType::BishopGeneral,
    PieceType::RainDemon,
    PieceType::KirinMaster,
    PieceType::PhoenixMaster,
    PieceType::CopperGeneral,
    PieceType::HorizontalMover,
    PieceType::FireDragon,
    PieceType::WaterDragon,
    PieceType::Peacock,
    PieceType::OldKite,
    PieceType::RushingBird,
    PieceType::FreePup,
    PieceType::FreeDog,
    PieceType::WindDragon,
    PieceType::FreeDragon,
    PieceType::RunningWolf,
    PieceType::FreeWolf,
    PieceType::RunningStag,
    PieceType::FreeStag,
    PieceType::SideDragon,
    PieceType::RunningDragon,
    PieceType::GoldenChariot,
    PieceType::PlayfulParrot,
    PieceType::ViceGeneral,
    PieceType::WoodlandDemon,
    PieceType::OldPeng,
    PieceType::FierceDragon,
    PieceType::FowlCadet,
    PieceType::Lion,
    PieceType::FuriousFiend,
    PieceType::GoldStag,
    PieceType::SilverRabbit,
    PieceType::SideBoar,
    PieceType::FreeBoar,
    PieceType::OxGeneral,
    PieceType::FreeOx,
    PieceType::HorseGeneral,
    PieceType::FreeHorse,
    PieceType::PupGeneral,
    PieceType::ChickenGeneral,
    PieceType::FreeChicken,
    PieceType::PigGeneral,
    PieceType::FreePig,
    PieceType::Knight,
    PieceType::SideSoldier,
    PieceType::VerticalBear,
    PieceType::SilverChariot,
    PieceType::GooseWing,
    PieceType::Daiba,
    PieceType::KingOfTeachings,
    PieceType::DarkSpirit,
    PieceType::BuddhistSpirit,
    PieceType::GoldBird,
    PieceType::FreeBird,
    PieceType::FierceOx,
    PieceType::FlyingOx,
    PieceType::FireOx,
    PieceType::SheepSoldier,
    PieceType::TigerSoldier,
    PieceType::RunningChariot,
    PieceType::CannonChariot,
    PieceType::CopperChariot,
    PieceType::CopperElephant,
    PieceType::CloudDragon,
    PieceType::LittleStandard,
    PieceType::Soldier,
    PieceType::Cavalier,
    PieceType::VerticalTiger,
    PieceType::MountainHawk,
    PieceType::HornedHawk,
    PieceType::FlyingCat,
    PieceType::SideWolf,
    PieceType::DragonKing,
    PieceType::CloudEagle,
    PieceType::StrongEagle,
    PieceType::StoneChariot,
    PieceType::WalkingHeron,
    PieceType::Bishop,
    PieceType::DragonHorse,
    PieceType::VerticalHorse,
    PieceType::VerticalPup,
    PieceType::LeopardKing,
    PieceType::LongbowSoldier,
    PieceType::LongbowGeneral,
    PieceType::SideMonkey,
    PieceType::LeftChariot,
    PieceType::LeftIronChariot,
    PieceType::RightChariot,
    PieceType::RightIronChariot,
    PieceType::FreeEagle,
    PieceType::CannonSoldier,
    PieceType::CannonGeneral,
    PieceType::GreatTurtle,
    PieceType::SpiritTurtle,
    PieceType::LittleTurtle,
    PieceType::TreasureTurtle,
    PieceType::Capricorn,
    PieceType::HookMover,
    PieceType::Kirin,
    PieceType::Phoenix,
    PieceType::FireGeneral,
    PieceType::WaterGeneral,
    PieceType::BlindDog,
    PieceType::FierceStag,
    PieceType::MovingBoar,
    PieceType::CrowMover,
    PieceType::FlyingHawk,
    PieceType::FlyingGoose,
    PieceType::SwallowsWings,
    PieceType::PoisonousSerpent,
    PieceType::FlyingDragon,
    PieceType::FierceEagle,
    PieceType::FierceLeopard,
    PieceType::WaterOx,
    PieceType::GreatBaku,
    PieceType::DancingStag,
    PieceType::SquareMover,
    PieceType::SideMover,
    PieceType::LeftHowlingDog,
    PieceType::RightHowlingDog,
    PieceType::LeftDog,
    PieceType::RightDog,
    PieceType::GreatFoal,
    PieceType::WoodChariot,
    PieceType::WindSnappingTurtle,
    PieceType::PengMaster,
    PieceType::CenterMaster,
    PieceType::FierceWolf,
    PieceType::BearsEyes,
    PieceType::EasternBarbarian,
    PieceType::WesternBarbarian,
    PieceType::LionDog,
    PieceType::SouthernBarbarian,
    PieceType::NorthernBarbarian,
    PieceType::LionHawk,
    PieceType::RecliningDragon,
    PieceType::CoiledSerpent,
    PieceType::CoiledDragon,
    PieceType::HuaiChicken,
    PieceType::WizardStork,
    PieceType::OldMonkey,
    PieceType::MountainWitch,
    PieceType::FlyingChicken,
    PieceType::RaidingHawk,
    PieceType::WindHorse,
    PieceType::HeavenlyHorse,
    PieceType::EvilWolf,
    PieceType::PoisonousWolf,
    PieceType::AngryBoar,
    PieceType::FierceBear,
    PieceType::GreatBear,
    PieceType::FlyingHorse,
    PieceType::Donkey,
    PieceType::SideOx,
    PieceType::VerticalWolf,
    PieceType::TileChariot,
    PieceType::RunningTile,
    PieceType::StrongChariot,
    PieceType::OldRat,
    PieceType::JiBird,
    PieceType::BlindBear,
    PieceType::FlyingStag,
    PieceType::SideFlyer,
    PieceType::OxChariot,
    PieceType::PloddingOx,
    PieceType::BlindTiger,
    PieceType::BlindMonkey,
    PieceType::SwallowMover,
    PieceType::CatSword,
    PieceType::ClimbingMonkey,
    PieceType::OwlMover,
    PieceType::Horseman,
    PieceType::Tanuki,
    PieceType::EarthChariot,
    PieceType::ReedBird,
    PieceType::GreatMaster,
    PieceType::GreatStandard,
    PieceType::IronGeneral,
    PieceType::RunningOx,
    PieceType::BearSoldier,
    PieceType::StrongBear,
    PieceType::TileGeneral,
    PieceType::LeopardSoldier,
    PieceType::RunningLeopard,
    PieceType::StoneGeneral,
    PieceType::BoarSoldier,
    PieceType::RunningBoar,
    PieceType::EarthGeneral,
    PieceType::OxSoldier,
    PieceType::WoodGeneral,
    PieceType::HorseSoldier,
    PieceType::MountainGeneral,
    PieceType::MountTai,
    PieceType::RiverGeneral,
    PieceType::HuaiRiver,
    PieceType::WindGeneral,
    PieceType::FierceWind,
    PieceType::VerticalSoldier,
    PieceType::ChariotSoldier,
    PieceType::SideGeneral,
    PieceType::Shitennou,
    PieceType::GreatElephant,
    PieceType::RoaringDog,
    PieceType::CrossbowSoldier,
    PieceType::CrossbowGeneral,
    PieceType::FierceTiger,
    PieceType::GreatTiger,
    PieceType::VerticalLeopard,
    PieceType::GreatLeopard,
    PieceType::SpearSoldier,
    PieceType::SpearGeneral,
    PieceType::GreatEagle,
    PieceType::GreatHawk,
    PieceType::SwordSoldier,
    PieceType::SwordGeneral,
];

const DEFAULT_PIECE_VALUE: i32 = 1;
const HIGH_PIECE_VALUE: i32 = 100;
/// Capturing-range generals (opening long-range jump takes). High so hanging
/// them after a cheap sweep is clearly refuted in search / quiescence.
const JUMP_CAPTURE_GENERAL_VALUE: i32 = 90;

fn is_high_value_piece(pt: PieceType) -> bool {
    matches!(
        pt,
        PieceType::King
            | PieceType::CrownPrince
            | PieceType::FreeKing
            | PieceType::Lion
            | PieceType::LionHawk
            | PieceType::Tengu
            | PieceType::HookMover
            | PieceType::FreeEagle
            | PieceType::GreatDove
            | PieceType::FireDemon
            | PieceType::FreeFire
            | PieceType::FuriousFiend
            | PieceType::BuddhistSpirit
            | PieceType::FreeDemon
    )
}

fn is_jump_capture_general(pt: PieceType) -> bool {
    matches!(
        pt,
        PieceType::GreatGeneral | PieceType::BishopGeneral | PieceType::FlyingGeneral
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchDefaults {
    pub depth: u32,
    pub max_time_ms: Option<u64>,
    /// Capture-only quiescence depth (0 = off). Missing in old checkpoints → 4.
    #[serde(default = "default_quiescence_depth")]
    pub quiescence_depth: u32,
}

fn default_quiescence_depth() -> u32 {
    4
}

impl Default for SearchDefaults {
    fn default() -> Self {
        Self {
            depth: 2,
            max_time_ms: None,
            quiescence_depth: 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalWeights {
    /// Material value keyed by current piece type (after promotion).
    pub piece: HashMap<PieceType, i32>,
    /// Bonus per living royal (friendly positive / enemy negative via difference).
    pub royal_alive: i32,
    /// Extra weight when a side is down to a single royal.
    pub sole_royal_factor: i32,
    /// Scale for Drunken Elephant / Go-Between advance toward promotion.
    pub de_advance: i32,
    /// Max absolute noise contribution (deterministic).
    pub noise_scale: f64,
    pub mate_score: i32,
    /// Mix into the position hash for reproducible noise.
    #[serde(default)]
    pub weight_seed: u64,
    /// Dense lookup rebuilt after load/seed (not serialized).
    #[serde(skip)]
    pub(crate) piece_value_table: Vec<i32>,
}

impl Default for EvalWeights {
    fn default() -> Self {
        Self::seed()
    }
}

impl EvalWeights {
    pub fn seed() -> Self {
        let mut piece = HashMap::with_capacity(ALL_PIECE_TYPES.len());
        for &pt in ALL_PIECE_TYPES {
            let v = if is_jump_capture_general(pt) {
                JUMP_CAPTURE_GENERAL_VALUE
            } else if is_high_value_piece(pt) {
                HIGH_PIECE_VALUE
            } else {
                DEFAULT_PIECE_VALUE
            };
            piece.insert(pt, v);
        }
        let mut w = Self {
            piece,
            royal_alive: 50,
            sole_royal_factor: 80,
            de_advance: 5,
            noise_scale: 1.0,
            mate_score: 1_000_000,
            weight_seed: 0xA11B_E7A1,
            piece_value_table: Vec::new(),
        };
        w.rebuild_piece_value_table();
        w
    }

    pub fn rebuild_piece_value_table(&mut self) {
        let mut max_idx = 0usize;
        for &pt in ALL_PIECE_TYPES {
            max_idx = max_idx.max(pt as usize);
        }
        self.piece_value_table = vec![DEFAULT_PIECE_VALUE; max_idx + 1];
        for (&pt, &v) in &self.piece {
            let i = pt as usize;
            if i >= self.piece_value_table.len() {
                self.piece_value_table.resize(i + 1, DEFAULT_PIECE_VALUE);
            }
            self.piece_value_table[i] = v;
        }
    }

    pub fn piece_value(&self, pt: PieceType) -> i32 {
        let i = pt as usize;
        if i < self.piece_value_table.len() {
            self.piece_value_table[i]
        } else {
            self.piece
                .get(&pt)
                .copied()
                .unwrap_or(DEFAULT_PIECE_VALUE)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCheckpoint {
    pub format_version: u32,
    pub name: String,
    pub created_at: String,
    pub search_defaults: SearchDefaults,
    pub weights: EvalWeights,
}

impl EvalCheckpoint {
    pub fn seed(name: &str) -> Self {
        Self {
            format_version: 1,
            name: name.to_string(),
            created_at: chrono_like_now(),
            search_defaults: SearchDefaults::default(),
            weights: EvalWeights::seed(),
        }
    }

    pub fn load_path(path: impl AsRef<Path>) -> Result<Self, String> {
        let text = fs::read_to_string(path.as_ref()).map_err(|e| e.to_string())?;
        let mut cp: Self = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        cp.weights.rebuild_piece_value_table();
        Ok(cp)
    }

    pub fn save_path(&self, path: impl AsRef<Path>) -> Result<(), String> {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path.as_ref(), text).map_err(|e| e.to_string())
    }
}

fn chrono_like_now() -> String {
    // Avoid extra chrono dependency: unix seconds is enough for checkpoints.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}

/// Evaluate `state` from the side-to-move's perspective (positive = good for STM).
pub fn evaluate(state: &GameState, weights: &EvalWeights) -> i32 {
    evaluate_with_ply(state, weights, state.get_move_history().len())
}

/// Like [`evaluate`], but use an explicit ply for deterministic noise (search without history).
pub fn evaluate_with_ply(state: &GameState, weights: &EvalWeights, ply: usize) -> i32 {
    let stm = state.get_current_turn();
    if let Some(winner) = state.get_winner() {
        return if winner == stm {
            weights.mate_score
        } else {
            -weights.mate_score
        };
    }

    let absolute_black = evaluate_absolute_black(state.get_board(), weights, ply);
    if stm == Color::Black {
        absolute_black
    } else {
        -absolute_black
    }
}

/// Black-positive absolute evaluation (independent of who moves).
pub fn evaluate_absolute_black(board: &Board, weights: &EvalWeights, ply: usize) -> i32 {
    let black = board.pieces_by_color(Color::Black);
    let white = board.pieces_by_color(Color::White);

    let black_royals = count_royals(black);
    let white_royals = count_royals(white);

    if black_royals == 0 {
        return -weights.mate_score;
    }
    if white_royals == 0 {
        return weights.mate_score;
    }

    let mut score = 0i32;
    score += material_of(black, weights) - material_of(white, weights);

    score += weights.royal_alive * (black_royals as i32 - white_royals as i32);
    if black_royals == 1 {
        score -= weights.sole_royal_factor;
    }
    if white_royals == 1 {
        score += weights.sole_royal_factor;
    }

    score += de_positional(black, Color::Black, weights);
    score -= de_positional(white, Color::White, weights);

    score += noise_component(board, weights, ply);
    score
}

fn count_royals(pieces: &[Piece]) -> usize {
    pieces.iter().filter(|p| p.piece_type.is_royal()).count()
}

fn material_of(pieces: &[Piece], weights: &EvalWeights) -> i32 {
    pieces.iter().map(|p| weights.piece_value(p.piece_type)).sum()
}

fn de_positional(pieces: &[Piece], color: Color, weights: &EvalWeights) -> i32 {
    let mut s = 0i32;
    for p in pieces {
        let is_de_path = matches!(
            p.piece_type,
            PieceType::DrunkenElephant | PieceType::GoBetween
        );
        if !is_de_path {
            continue;
        }
        let progress = match color {
            Color::Black => p.position.rank as i32,
            Color::White => 35 - p.position.rank as i32,
        };
        // Scale so full-board advance is a few times de_advance.
        s += weights.de_advance * progress / 7;
        if in_promotion_zone(p.position, color) {
            s += weights.de_advance * 2;
        }
    }
    s
}

fn in_promotion_zone(pos: Position, color: Color) -> bool {
    match color {
        Color::Black => pos.rank >= 25,
        Color::White => pos.rank <= 10,
    }
}

fn noise_component(board: &Board, weights: &EvalWeights, ply: usize) -> i32 {
    if weights.noise_scale == 0.0 {
        return 0;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    weights.weight_seed.hash(&mut hasher);
    ply.hash(&mut hasher);
    for color in [Color::Black, Color::White] {
        for p in board.pieces_by_color(color) {
            p.piece_type.hash(&mut hasher);
            p.color.hash(&mut hasher);
            p.position.file.hash(&mut hasher);
            p.position.rank.hash(&mut hasher);
            p.is_promoted.hash(&mut hasher);
        }
    }
    let h = hasher.finish();
    let unit = (h % 10_001) as f64 / 10_000.0; // [0, 1]
    let n = (unit - 0.5) * 2.0 * weights.noise_scale;
    n.round() as i32
}

/// Default on-disk seed path.
pub const DEFAULT_MODEL_PATH: &str = "models/ab-seed.json";

/// List `*.json` checkpoint filenames under `dir` (e.g. `models`).
pub fn list_model_files(dir: impl AsRef<Path>) -> Result<Vec<String>, String> {
    let dir = dir.as_ref();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            names.push(name.to_string());
        }
    }
    names.sort();
    Ok(names)
}

/// Load checkpoint from path, or built-in seed if missing.
pub fn load_checkpoint_or_seed(path: impl AsRef<Path>) -> EvalCheckpoint {
    match EvalCheckpoint::load_path(path.as_ref()) {
        Ok(cp) => cp,
        Err(_) => EvalCheckpoint::seed("ab-seed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::GameState;
    use crate::piece::Piece;
    use crate::position::Position;

    #[test]
    fn seed_round_trip_json() {
        let cp = EvalCheckpoint::seed("ab-seed");
        let text = serde_json::to_string(&cp).unwrap();
        let mut back: EvalCheckpoint = serde_json::from_str(&text).unwrap();
        back.weights.rebuild_piece_value_table();
        assert_eq!(back.format_version, 1);
        assert_eq!(back.weights.piece_value(PieceType::King), 100);
        assert_eq!(back.weights.piece_value(PieceType::Pawn), 1);
        assert_eq!(back.weights.piece.len(), ALL_PIECE_TYPES.len());
        assert!(!back.weights.piece_value_table.is_empty());
    }

    #[test]
    fn prefers_side_with_extra_royal_material() {
        let weights = EvalWeights::seed();
        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(20, 20).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::CrownPrince,
            Color::Black,
            Position::new(11, 10).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        let score = evaluate(&state, &weights);
        assert!(score > 0, "black with two royals vs one should be positive, got {score}");
    }

    #[test]
    fn zero_enemy_royals_is_mate() {
        let weights = EvalWeights::seed();
        let mut board = Board::new();
        board.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        let score = evaluate_absolute_black(&board, &weights, 0);
        assert_eq!(score, weights.mate_score);
    }
}
