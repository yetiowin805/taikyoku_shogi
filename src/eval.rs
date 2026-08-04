//! Static evaluation and versioned weight checkpoints for the alpha-beta agent.

use crate::board::Board;
use crate::game_state::GameState;
use crate::movement::{BlockingMode, MovementCapability, MovementConfig};
use crate::piece::{Color, Piece, PieceType};
use crate::position::Position;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::OnceLock;

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

/// Fallback for unknown / missing table entries.
const DEFAULT_PIECE_VALUE: f32 = 1.0;

/// Forward directions in black-relative configs: N | NE | NW.
const FORWARD_DIRS: u8 = 0x01 | 0x02 | 0x80;

/// Pawn opening rank (Black).
pub const RANK_PAWN_START: u8 = 10;
/// First rank of the opponent's half (Black progress).
pub const RANK_OPPONENT_HALF: u8 = 18;
/// First progress rank of the enemy home / promotion zone (Black rank 25).
pub const RANK_PST_PROMO: u8 = 25;
/// Legacy mid anchor (unused by new PST; kept for callers/tests that referenced it).
pub const RANK_PST_MID: u8 = 17;

/// Harmonic number H_n = 1 + 1/2 + ... + 1/n.
fn harmonic(n: u8) -> f32 {
    if n == 0 {
        return 0.0;
    }
    let mut s = 0.0f32;
    for k in 1..=n {
        s += 1.0 / k as f32;
    }
    s
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Per-direction value for blocked sliding (NoJump).
pub const TARIFF_RANGE_NO_JUMP: f32 = 10.0;
/// Per-direction value for jump-range sliding.
pub const TARIFF_RANGE_JUMP: f32 = 50.0;
/// Per-direction value for capturing-range sliding (main strength dial for GG/VG/…).
pub const TARIFF_RANGE_CAPTURING: f32 = 500.0;

/// Quiescence / worthwhile-capture floor derived from range tariffs.
///
/// About 2.4 capturing-dirs (`500×2.4=1200`) so mid-heavy takes enter q; also at
/// least a full 8-dir jump-ray.
pub fn seed_loud_capture_floor() -> f32 {
    (TARIFF_RANGE_CAPTURING * 2.4).max(TARIFF_RANGE_JUMP * 8.0)
}

fn capability_material_value(cap: &MovementCapability) -> f32 {
    match cap {
        MovementCapability::Simple {
            directions,
            max_distance,
        } => directions.count_ones() as f32 * harmonic(*max_distance),
        MovementCapability::Range {
            directions,
            blocking,
            ..
        } => {
            let per = match *blocking {
                BlockingMode::NoJump => TARIFF_RANGE_NO_JUMP,
                BlockingMode::Jump => TARIFF_RANGE_JUMP,
                BlockingMode::Capturing => TARIFF_RANGE_CAPTURING,
            };
            directions.count_ones() as f32 * per
        }
        MovementCapability::Jumping { offsets } => offsets.len() as f32,
        MovementCapability::TwoStep { first, second } => {
            capability_material_value(first) + capability_material_value(second)
        }
        // Covered by overrides (WoodenDove / FreeEagle).
        MovementCapability::ConditionalDiagonalJump { .. } => 0.0,
        MovementCapability::FreeEagleMultiMove { .. } => 0.0,
    }
}

fn explicit_material_override(pt: PieceType) -> Option<f32> {
    match pt {
        PieceType::King => Some(2000.0),
        PieceType::CrownPrince => Some(8.0),
        PieceType::Peacock => Some(800.0),
        PieceType::Tengu => Some(1200.0),
        PieceType::Capricorn => Some(1500.0),
        PieceType::HookMover => Some(2000.0),
        PieceType::Lion => Some(15.0),
        PieceType::FuriousFiend => Some(30.0),
        PieceType::LionHawk => Some(50.0),
        PieceType::BuddhistSpirit => Some(90.0),
        PieceType::WoodenDove => Some(50.0),
        PieceType::FreeEagle => Some(30.0),
        _ => None,
    }
}

/// Additive bonus on top of the capability formula (not a full override).
fn additive_material_bonus(pt: PieceType) -> f32 {
    match pt {
        PieceType::ViceGeneral => 500.0,
        _ => 0.0,
    }
}

fn formula_piece_value(pt: PieceType) -> f32 {
    let cfg = MovementConfig::for_piece_type(pt);
    let mut sum = 0.0f32;
    for cap in &cfg.capabilities {
        sum += capability_material_value(cap);
    }
    sum + additive_material_bonus(pt)
}

/// Seed material from movement capabilities (+ explicit overrides / additive bonuses).
pub fn seed_piece_value(pt: PieceType) -> f32 {
    if let Some(v) = explicit_material_override(pt) {
        return v;
    }
    formula_piece_value(pt)
}

/// True if the piece has range movement in at least one forward direction (black-relative).
pub fn is_fast_piece(pt: PieceType) -> bool {
    fast_piece_table()
        .get(pt as usize)
        .copied()
        .unwrap_or(false)
}

fn fast_piece_table() -> &'static [bool] {
    static TABLE: OnceLock<Vec<bool>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut max_idx = 0usize;
        for &pt in ALL_PIECE_TYPES {
            max_idx = max_idx.max(pt as usize);
        }
        let mut t = vec![false; max_idx + 1];
        for &pt in ALL_PIECE_TYPES {
            let cfg = MovementConfig::for_piece_type(pt);
            let fast = cfg.capabilities.iter().any(|cap| {
                matches!(
                    cap,
                    MovementCapability::Range { directions, .. }
                        if (*directions & FORWARD_DIRS) != 0
                )
            });
            t[pt as usize] = fast;
        }
        t
    })
}

/// Jump-range / capturing-range pieces skip rank PST (high material + board-wide reach).
pub fn skips_rank_pst(pt: PieceType) -> bool {
    skip_rank_pst_table()
        .get(pt as usize)
        .copied()
        .unwrap_or(false)
}

fn skip_rank_pst_table() -> &'static [bool] {
    static TABLE: OnceLock<Vec<bool>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut max_idx = 0usize;
        for &pt in ALL_PIECE_TYPES {
            max_idx = max_idx.max(pt as usize);
        }
        let mut t = vec![false; max_idx + 1];
        for &pt in ALL_PIECE_TYPES {
            let cfg = MovementConfig::for_piece_type(pt);
            let skip = cfg.capabilities.iter().any(|cap| {
                matches!(
                    cap,
                    MovementCapability::Range {
                        blocking: BlockingMode::Jump | BlockingMode::Capturing,
                        ..
                    }
                )
            });
            t[pt as usize] = skip;
        }
        t
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchDefaults {
    pub depth: u32,
    pub max_time_ms: Option<u64>,
    /// Capture-only quiescence depth (0 = off). Missing in old checkpoints → 2.
    #[serde(default = "default_quiescence_depth")]
    pub quiescence_depth: u32,
}

fn default_quiescence_depth() -> u32 {
    2
}

impl Default for SearchDefaults {
    fn default() -> Self {
        Self {
            depth: 2,
            max_time_ms: None,
            quiescence_depth: 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalWeights {
    /// Material value keyed by current piece type (after promotion).
    pub piece: HashMap<PieceType, f32>,
    /// Legacy per-royal linear term (unused by seed; kept for old JSON).
    #[serde(default)]
    pub royal_alive: i32,
    /// Legacy sole-royal term (unused by seed; kept for old JSON).
    #[serde(default)]
    pub sole_royal_factor: i32,
    /// Royal bonus by living count: index = count (0 unused; mate short-circuits).
    /// Seed: `[0, 0, 5000, 6000]` → 1→0, 2→5000, 3+→6000.
    #[serde(default = "default_royal_bonus_by_count")]
    pub royal_bonus_by_count: Vec<i32>,
    /// Legacy DE/GoBetween advance scale (unused by seed).
    #[serde(default)]
    pub de_advance: i32,
    /// Floor for undeveloped penalty. Seed uses 0 (PST is the development signal).
    #[serde(default = "default_undeveloped_home")]
    pub undeveloped_home: i32,
    /// Legacy forwardness bonus for non-royals: `advance * progress / 12`.
    #[serde(default = "default_advance")]
    pub advance: i32,
    /// Legacy single rank table (kept for old JSON compatibility).
    #[serde(default = "seed_rank_factors_fast_vec")]
    pub rank_factor: Vec<f32>,
    /// Fast-piece rank multipliers (range in a forward direction).
    #[serde(default = "seed_rank_factors_fast_vec")]
    pub rank_factor_fast: Vec<f32>,
    /// Slow-piece rank multipliers.
    #[serde(default = "seed_rank_factors_slow_vec")]
    pub rank_factor_slow: Vec<f32>,
    /// Max absolute noise contribution (deterministic).
    pub noise_scale: f64,
    pub mate_score: i32,
    /// Mix into the position hash for reproducible noise / weight perturbation.
    #[serde(default)]
    pub weight_seed: u64,
    /// Dense lookup rebuilt after load/seed (not serialized).
    #[serde(skip)]
    pub(crate) piece_value_table: Vec<f32>,
}

fn default_undeveloped_home() -> i32 {
    0
}

fn default_advance() -> i32 {
    0
}

fn default_royal_bonus_by_count() -> Vec<i32> {
    vec![0, 0, 5000, 6000]
}

/// Fast PST: 50% back → 100% pawn start → 100% to mid → 110% opponent half → 120% promo.
pub fn seed_rank_factors_fast() -> [f32; 36] {
    let pawn = RANK_PAWN_START;
    let opp = RANK_OPPONENT_HALF;
    let promo = RANK_PST_PROMO;
    let mut factors = [1.0f32; 36];
    for r in 0u8..36 {
        factors[r as usize] = if r <= pawn {
            lerp(0.5, 1.0, r as f32 / pawn as f32)
        } else if r < opp {
            1.0
        } else if r < promo {
            1.1
        } else {
            1.2
        };
    }
    factors
}

/// Slow PST: 10% back → 60% pawn start → 100% at opp half → 120% promo, then hold.
pub fn seed_rank_factors_slow() -> [f32; 36] {
    let pawn = RANK_PAWN_START;
    let opp = RANK_OPPONENT_HALF;
    let promo = RANK_PST_PROMO;
    let mut factors = [1.0f32; 36];
    for r in 0u8..36 {
        factors[r as usize] = if r <= pawn {
            lerp(0.1, 0.6, r as f32 / pawn as f32)
        } else if r <= opp {
            lerp(0.6, 1.0, (r - pawn) as f32 / (opp - pawn) as f32)
        } else if r <= promo {
            lerp(1.0, 1.2, (r - opp) as f32 / (promo - opp) as f32)
        } else {
            1.2
        };
    }
    factors
}

fn seed_rank_factors_fast_vec() -> Vec<f32> {
    seed_rank_factors_fast().to_vec()
}

fn seed_rank_factors_slow_vec() -> Vec<f32> {
    seed_rank_factors_slow().to_vec()
}

/// Legacy alias used by older call sites / docs.
pub fn seed_rank_factors() -> [f32; 36] {
    seed_rank_factors_fast()
}

/// Material contribution for one piece including rank PST.
/// Royals and jump/capturing-range pieces always use factor 1.
pub fn positional_piece_value(piece: &Piece, weights: &EvalWeights) -> f32 {
    let v = weights.piece_value(piece.piece_type);
    if piece.piece_type.is_royal() || skips_rank_pst(piece.piece_type) {
        return v;
    }
    let progress = match piece.color {
        Color::Black => piece.position.rank as usize,
        Color::White => (35 - piece.position.rank) as usize,
    };
    let table = if is_fast_piece(piece.piece_type) {
        &weights.rank_factor_fast
    } else {
        &weights.rank_factor_slow
    };
    let f = table.get(progress).copied().unwrap_or(1.0);
    v * f
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
            piece.insert(pt, seed_piece_value(pt));
        }
        let mut w = Self {
            piece,
            royal_alive: 0,
            sole_royal_factor: 0,
            royal_bonus_by_count: default_royal_bonus_by_count(),
            de_advance: 0,
            undeveloped_home: default_undeveloped_home(),
            advance: default_advance(),
            rank_factor: seed_rank_factors_fast_vec(),
            rank_factor_fast: seed_rank_factors_fast_vec(),
            rank_factor_slow: seed_rank_factors_slow_vec(),
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

    pub fn piece_value(&self, pt: PieceType) -> f32 {
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

    /// Rounded material for integer search / MVV comparisons (base value, not PST).
    pub fn piece_value_i32(&self, pt: PieceType) -> i32 {
        self.piece_value(pt).round() as i32
    }

    pub fn royal_bonus(&self, count: usize) -> i32 {
        if count == 0 {
            return 0;
        }
        let table = &self.royal_bonus_by_count;
        if table.is_empty() {
            return 0;
        }
        let idx = count.min(table.len() - 1);
        table[idx]
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

    let mut score = 0.0f32;
    score += material_of(black, weights) - material_of(white, weights);

    score += weights.royal_bonus(black_royals) as f32 - weights.royal_bonus(white_royals) as f32;

    score -= undeveloped_home_penalty(black, weights);
    score += undeveloped_home_penalty(white, weights);

    score += advance_positional(black, Color::Black, weights) as f32;
    score -= advance_positional(white, Color::White, weights) as f32;

    score.round() as i32 + noise_component(board, weights, ply)
}

/// Opening home rank per `(color, piece_type)` for non-royals.
fn initial_non_royal_home_ranks() -> &'static HashMap<(Color, PieceType), u8> {
    static HOMES: OnceLock<HashMap<(Color, PieceType), u8>> = OnceLock::new();
    HOMES.get_or_init(|| {
        let mut state = GameState::new();
        state.setup_initial_position();
        let mut map = HashMap::new();
        for color in [Color::Black, Color::White] {
            for p in state.get_board().pieces_by_color(color) {
                if p.piece_type.is_royal() {
                    continue;
                }
                let key = (p.color, p.piece_type);
                if let Some(&prev) = map.get(&key) {
                    debug_assert_eq!(
                        prev, p.position.rank,
                        "piece type {:?} starts on multiple ranks for {:?}",
                        p.piece_type, p.color
                    );
                }
                map.insert(key, p.position.rank);
            }
        }
        map
    })
}

fn on_home_rank_or_behind(piece: &Piece, home_rank: u8) -> bool {
    match piece.color {
        Color::Black => piece.position.rank <= home_rank,
        Color::White => piece.position.rank >= home_rank,
    }
}

/// Hard cap on per-piece undeveloped penalty (score impact at most -20).
const UNDEVELOPED_PENALTY_CAP: f32 = 20.0;

fn undeveloped_penalty_for_piece(piece: &Piece, weights: &EvalWeights) -> f32 {
    if piece.piece_type.is_royal() || skips_rank_pst(piece.piece_type) {
        return 0.0;
    }
    let Some(&home_rank) = initial_non_royal_home_ranks().get(&(piece.color, piece.piece_type))
    else {
        return 0.0;
    };
    if !on_home_rank_or_behind(piece, home_rank) {
        return 0.0;
    }
    let floor = weights.undeveloped_home as f32;
    let value = weights.piece_value(piece.piece_type);
    floor
        .max(0.2 * value)
        .min(0.9 * value)
        .min(UNDEVELOPED_PENALTY_CAP)
}

fn undeveloped_home_penalty(pieces: &[Piece], weights: &EvalWeights) -> f32 {
    pieces
        .iter()
        .map(|p| undeveloped_penalty_for_piece(p, weights))
        .sum()
}

/// Cheap forwardness: non-royals score for how far they have advanced.
fn advance_positional(pieces: &[Piece], color: Color, weights: &EvalWeights) -> i32 {
    if weights.advance == 0 {
        return 0;
    }
    let mut s = 0i32;
    for p in pieces {
        if p.piece_type.is_royal() || skips_rank_pst(p.piece_type) {
            continue;
        }
        let progress = match color {
            Color::Black => p.position.rank as i32,
            Color::White => 35 - p.position.rank as i32,
        };
        s += weights.advance * progress / 12;
    }
    s
}

fn count_royals(pieces: &[Piece]) -> usize {
    pieces.iter().filter(|p| p.piece_type.is_royal()).count()
}

fn material_of(pieces: &[Piece], weights: &EvalWeights) -> f32 {
    pieces
        .iter()
        .map(|p| positional_piece_value(p, weights))
        .sum()
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

/// Default on-disk seed path (canonical checkpoint).
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
    fn seed_material_values_match_tariffs() {
        let w = EvalWeights::seed();
        assert!((w.piece_value(PieceType::Pawn) - 1.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::CrownPrince) - 8.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::King) - 2000.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::FreeKing) - 80.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::Shitennou) - 400.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::GreatEagle) - 160.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::GreatGeneral) - 4000.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::FierceDragon) - 2006.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::ViceGeneral) - 2504.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::Peacock) - 800.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::Tengu) - 1200.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::Capricorn) - 1500.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::HookMover) - 2000.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::Lion) - 15.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::FuriousFiend) - 30.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::LionHawk) - 50.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::BuddhistSpirit) - 90.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::WoodenDove) - 50.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::FreeEagle) - 30.0).abs() < 1e-3);
        // Limited 2 in 4 dirs: 4 * 1.5 = 6 (sanity for harmonic).
        assert!((harmonic(2) - 1.5).abs() < 1e-6);
        assert!((harmonic(3) - 11.0 / 6.0).abs() < 1e-6);
    }

    #[test]
    fn loud_capture_floor_tracks_capturing_tariff() {
        assert!((seed_loud_capture_floor() - 1200.0).abs() < 1e-3);
        assert!((seed_loud_capture_floor() - (TARIFF_RANGE_CAPTURING * 2.4).max(TARIFF_RANGE_JUMP * 8.0)).abs() < 1e-6);
        assert!(TARIFF_RANGE_CAPTURING * 2.4 >= TARIFF_RANGE_JUMP * 8.0);
    }

    #[test]
    fn seed_round_trip_json() {
        let cp = EvalCheckpoint::seed("ab-seed");
        let text = serde_json::to_string(&cp).unwrap();
        let mut back: EvalCheckpoint = serde_json::from_str(&text).unwrap();
        back.weights.rebuild_piece_value_table();
        assert_eq!(back.format_version, 1);
        assert_eq!(back.weights.piece_value(PieceType::King), 2000.0);
        assert_eq!(back.weights.piece_value(PieceType::CrownPrince), 8.0);
        assert_eq!(back.weights.piece_value(PieceType::GreatGeneral), 4000.0);
        assert_eq!(back.weights.piece_value(PieceType::FreeEagle), 30.0);
        assert_eq!(back.weights.piece_value(PieceType::WoodenDove), 50.0);
        assert_eq!(back.weights.piece_value(PieceType::HookMover), 2000.0);
        assert_eq!(back.weights.piece_value(PieceType::Lion), 15.0);
        assert!((back.weights.piece_value(PieceType::Pawn) - 1.0).abs() < 1e-3);
        assert_eq!(back.weights.piece.len(), ALL_PIECE_TYPES.len());
        assert!(!back.weights.piece_value_table.is_empty());
        assert_eq!(back.weights.advance, 0);
        assert_eq!(back.weights.undeveloped_home, 0);
        assert_eq!(back.weights.de_advance, 0);
        assert!((back.weights.rank_factor_fast[0] - 0.5).abs() < 1e-3);
        assert!((back.weights.rank_factor_fast[RANK_PAWN_START as usize] - 1.0).abs() < 1e-3);
        assert!((back.weights.rank_factor_fast[RANK_OPPONENT_HALF as usize] - 1.1).abs() < 1e-3);
        assert!((back.weights.rank_factor_fast[RANK_PST_PROMO as usize] - 1.2).abs() < 1e-3);
        assert!((back.weights.rank_factor_slow[0] - 0.1).abs() < 1e-3);
        assert!((back.weights.rank_factor_slow[RANK_PAWN_START as usize] - 0.6).abs() < 1e-3);
        assert!((back.weights.rank_factor_slow[RANK_OPPONENT_HALF as usize] - 1.0).abs() < 1e-3);
        assert!((back.weights.rank_factor_slow[RANK_PST_PROMO as usize] - 1.2).abs() < 1e-3);
        assert_eq!(back.weights.royal_bonus(1), 0);
        assert_eq!(back.weights.royal_bonus(2), 5000);
        assert_eq!(back.weights.royal_bonus(3), 6000);
    }

    #[test]
    fn fast_slow_pst_multiplies_purely() {
        let weights = EvalWeights::seed();
        // FreeKing is fast (NoJump range all dirs including forward).
        assert!(is_fast_piece(PieceType::FreeKing));
        assert!(!is_fast_piece(PieceType::Pawn));
        assert!(!skips_rank_pst(PieceType::FreeKing));
        assert!(!skips_rank_pst(PieceType::Pawn));

        let v = weights.piece_value(PieceType::FreeKing);
        let back = Piece::new(
            PieceType::FreeKing,
            Color::Black,
            Position::new(6, 0).unwrap(),
        );
        let pawn_rank = Piece::new(
            PieceType::FreeKing,
            Color::Black,
            Position::new(6, RANK_PAWN_START).unwrap(),
        );
        let promo = Piece::new(
            PieceType::FreeKing,
            Color::Black,
            Position::new(6, RANK_PST_PROMO).unwrap(),
        );
        assert!((positional_piece_value(&back, &weights) - 0.5 * v).abs() < 1e-2);
        assert!((positional_piece_value(&pawn_rank, &weights) - v).abs() < 1e-2);
        assert!((positional_piece_value(&promo, &weights) - 1.2 * v).abs() < 1e-2);

        let pv = weights.piece_value(PieceType::Pawn);
        let pawn_back = Piece::new(PieceType::Pawn, Color::Black, Position::new(6, 0).unwrap());
        assert!((positional_piece_value(&pawn_back, &weights) - 0.1 * pv).abs() < 1e-2);
    }

    #[test]
    fn jump_and_capturing_range_skip_rank_pst() {
        let weights = EvalWeights::seed();
        assert!(skips_rank_pst(PieceType::GreatGeneral));
        assert!(skips_rank_pst(PieceType::ViceGeneral));
        // Shitennou: Jump-range in all dirs.
        assert!(skips_rank_pst(PieceType::Shitennou));

        for &pt in &[
            PieceType::GreatGeneral,
            PieceType::ViceGeneral,
            PieceType::Shitennou,
        ] {
            let v = weights.piece_value(pt);
            let back = Piece::new(pt, Color::Black, Position::new(6, 0).unwrap());
            let promo = Piece::new(pt, Color::Black, Position::new(6, RANK_PST_PROMO).unwrap());
            assert!(
                (positional_piece_value(&back, &weights) - v).abs() < 1e-2,
                "{pt:?} back rank should be unscaled"
            );
            assert!(
                (positional_piece_value(&promo, &weights) - v).abs() < 1e-2,
                "{pt:?} promo rank should be unscaled"
            );
        }
    }

    #[test]
    fn export_seed_checkpoint_to_models() {
        let a = EvalCheckpoint::seed("ab-seed");
        a.save_path(DEFAULT_MODEL_PATH)
            .expect("write ab-seed.json");
        let loaded = EvalCheckpoint::load_path(DEFAULT_MODEL_PATH).unwrap();
        assert_eq!(loaded.weights.advance, 0);
        assert_eq!(loaded.name, "ab-seed");
        assert_eq!(loaded.weights.piece_value(PieceType::King), 2000.0);
        assert_eq!(loaded.weights.piece_value(PieceType::GreatGeneral), 4000.0);
        assert_eq!(loaded.weights.royal_bonus(2), 5000);
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
        // Two royals → +5000 royal bonus vs one (0), plus CP material 8.
        assert!(score > 4000, "black with two royals vs one should be largely positive, got {score}");
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

    #[test]
    fn undeveloped_home_penalizes_home_rank_or_behind() {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.undeveloped_home = 3;

        let mut state = GameState::new();
        state.setup_initial_position();

        let black_pen =
            undeveloped_home_penalty(state.get_board().pieces_by_color(Color::Black), &weights);
        let white_pen =
            undeveloped_home_penalty(state.get_board().pieces_by_color(Color::White), &weights);
        assert!((black_pen - white_pen).abs() < 1e-3);
        assert!(black_pen > 100.0, "expected full-army undeveloped penalty, got {black_pen}");

        // Pawn value is now 1.0 → 90% cap = 0.9.
        let from = Position::new(16, 10).unwrap();
        let to = Position::new(16, 11).unwrap();
        assert_eq!(
            state.get_board().get_piece(from).map(|p| p.piece_type),
            Some(PieceType::Pawn)
        );
        let pawn = state.get_board().get_piece(from).unwrap();
        let pawn_pen = undeveloped_penalty_for_piece(&pawn, &weights);
        assert!(
            (pawn_pen - 0.9).abs() < 1e-3,
            "pawn undeveloped should be 0.9*1.0=0.9, got {pawn_pen}"
        );
        state.get_board_mut().move_piece(from, to);

        let behind = Position::new(16, 9).unwrap();
        state.get_board_mut().move_piece(to, behind);
        let retreated = state.get_board().get_piece(behind).unwrap();
        assert!((undeveloped_penalty_for_piece(&retreated, &weights) - 0.9).abs() < 1e-3);

        // Tengu = 1200 → uncapped 240, capped 20.
        let mut hi_board = Board::new();
        hi_board.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(17, 0).unwrap(),
        ));
        hi_board.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(17, 35).unwrap(),
        ));
        let hi_from = Position::new(6, 0).unwrap();
        hi_board.place_piece(Piece::new(PieceType::Tengu, Color::Black, hi_from));
        let hi = hi_board.get_piece(hi_from).unwrap();
        let hi_pen = undeveloped_penalty_for_piece(&hi, &weights);
        assert!(
            (hi_pen - 20.0).abs() < 1e-3,
            "expected undeveloped cap 20, got {hi_pen}"
        );

        let king = Piece::new(PieceType::King, Color::Black, Position::new(17, 0).unwrap());
        assert_eq!(undeveloped_penalty_for_piece(&king, &weights), 0.0);
    }

    #[test]
    fn advance_prefers_forward_non_royal() {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.advance = 12; // 1 point per rank of progress
        let mut back = Board::new();
        back.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(17, 0).unwrap(),
        ));
        back.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(17, 35).unwrap(),
        ));
        back.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(16, 5).unwrap(),
        ));
        let mut fwd = back.clone();
        fwd.move_piece(Position::new(16, 5).unwrap(), Position::new(16, 17).unwrap());
        let a = evaluate_absolute_black(&back, &weights, 0);
        let b = evaluate_absolute_black(&fwd, &weights, 0);
        assert!(
            b > a,
            "forward gold should score higher: back={a} fwd={b}"
        );
    }
}
