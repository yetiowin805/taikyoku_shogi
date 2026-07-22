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

fn capability_material_value(cap: &MovementCapability) -> f32 {
    match cap {
        MovementCapability::Simple {
            directions,
            max_distance,
        } => 0.5 * directions.count_ones() as f32 * (*max_distance as f32),
        MovementCapability::Range {
            directions,
            blocking,
            ..
        } => match *blocking {
            BlockingMode::Capturing => 800.0,
            BlockingMode::Jump => 100.0,
            BlockingMode::NoJump => 4.0 * directions.count_ones() as f32,
        },
        MovementCapability::Jumping { offsets } => 2.0 * offsets.len() as f32,
        MovementCapability::TwoStep { first, second } => capability_material_value(first)
            .max(capability_material_value(second)),
        // Only WoodenDove uses this; override usually wins, but keep a jump-class floor.
        MovementCapability::ConditionalDiagonalJump { .. } => 40.0,
        // FreeEagle multi-move is covered by the FreeEagle override.
        MovementCapability::FreeEagleMultiMove { .. } => 0.0,
    }
}

fn explicit_material_override(pt: PieceType) -> Option<f32> {
    match pt {
        PieceType::King => Some(1800.0),
        PieceType::CrownPrince => Some(1600.0),
        PieceType::DrunkenElephant => Some(100.0),
        PieceType::HookMover | PieceType::Tengu | PieceType::Capricorn => Some(1200.0),
        PieceType::Peacock | PieceType::GreatGeneral => Some(1100.0),
        PieceType::FreeEagle | PieceType::WoodenDove => Some(40.0),
        PieceType::BuddhistSpirit => Some(40.0),
        PieceType::LionHawk => Some(25.0),
        PieceType::FuriousFiend => Some(24.0),
        PieceType::Lion => Some(15.0),
        _ => None,
    }
}

/// Seed material from movement capabilities (+ explicit overrides).
pub fn seed_piece_value(pt: PieceType) -> f32 {
    if let Some(v) = explicit_material_override(pt) {
        return v;
    }
    // Other royals (if any are added later).
    if pt.is_royal() {
        return 1600.0;
    }
    let cfg = MovementConfig::for_piece_type(pt);
    let mut best = 0.0f32;
    for cap in &cfg.capabilities {
        best = best.max(capability_material_value(cap));
    }
    best
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
    /// Bonus per living royal (friendly positive / enemy negative via difference).
    pub royal_alive: i32,
    /// Extra weight when a side is down to a single royal.
    pub sole_royal_factor: i32,
    /// Scale for Drunken Elephant / Go-Between advance toward promotion.
    pub de_advance: i32,
    /// Floor for undeveloped penalty (on opening rank or behind):
    /// `min(20, min(0.9 * value, max(undeveloped_home, 0.2 * value)))` per non-royal.
    #[serde(default = "default_undeveloped_home")]
    pub undeveloped_home: i32,
    /// Legacy forwardness bonus for non-royals: `advance * progress / 12`.
    /// Default 0 — rank PST covers development; keep for noisy/perturbed configs.
    #[serde(default = "default_advance")]
    pub advance: i32,
    /// Rank factors indexed by progress toward the enemy (0 = own back rank, 35 = enemy back).
    /// White uses the same table via mirrored progress. See [`seed_rank_factors`].
    #[serde(default = "seed_rank_factors_vec")]
    pub rank_factor: Vec<f32>,
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
    3
}

/// Rank PST replaces the old advance term in the seed; default off.
fn default_advance() -> i32 {
    0
}

/// Central progress rank for the 120% anchor (Black rank 17).
pub const RANK_PST_MID: u8 = 17;
/// First progress rank of the enemy home / promotion zone (Black rank 25).
pub const RANK_PST_PROMO: u8 = 25;

/// Seed rank factors: 50% back → 100% just outside camp → 120% mid/enemy-open → 150% promo.
pub fn seed_rank_factors() -> [f32; 36] {
    let home_edge = black_opening_home_edge();
    let outside = home_edge.saturating_add(1); // 100% row
    let mid = RANK_PST_MID;
    let promo = RANK_PST_PROMO;
    let mut factors = [1.0f32; 36];
    for r in 0u8..36 {
        factors[r as usize] = if r == 0 {
            0.5
        } else if r < outside {
            0.5 + 0.5 * (r as f32) / (outside as f32)
        } else if r <= mid {
            let span = (mid - outside) as f32;
            if span <= 0.0 {
                1.2
            } else {
                1.0 + 0.2 * (r - outside) as f32 / span
            }
        } else if r < promo {
            1.2
        } else {
            1.5
        };
    }
    factors
}

fn seed_rank_factors_vec() -> Vec<f32> {
    seed_rank_factors().to_vec()
}

fn black_opening_home_edge() -> u8 {
    initial_non_royal_home_ranks()
        .iter()
        .filter(|((color, _), _)| *color == Color::Black)
        .map(|(_, &rank)| rank)
        .max()
        .unwrap_or(11)
}

/// Cap on positional *bonus* above 100% (linear in factor between anchors).
fn rank_bonus_cap(factor: f32) -> f32 {
    if factor <= 1.0 {
        0.0
    } else if factor <= 1.2 {
        20.0 * (factor - 1.0) / 0.2
    } else if factor <= 1.5 {
        20.0 + 30.0 * (factor - 1.2) / 0.3
    } else {
        50.0
    }
}

/// Material contribution for one piece including rank PST (royals always factor 1).
pub fn positional_piece_value(piece: &Piece, weights: &EvalWeights) -> f32 {
    let v = weights.piece_value(piece.piece_type);
    if piece.piece_type.is_royal() {
        return v;
    }
    let progress = match piece.color {
        Color::Black => piece.position.rank as usize,
        Color::White => (35 - piece.position.rank) as usize,
    };
    let f = weights
        .rank_factor
        .get(progress)
        .copied()
        .unwrap_or(1.0);
    if f <= 1.0 {
        v * f
    } else {
        v + (v * (f - 1.0)).min(rank_bonus_cap(f))
    }
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
            royal_alive: 50,
            sole_royal_factor: 80,
            de_advance: 5,
            undeveloped_home: default_undeveloped_home(),
            advance: default_advance(),
            rank_factor: seed_rank_factors_vec(),
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

    /// Multiply every tunable weight by an independent `U(lo, hi)` draw (deterministic).
    /// Skips [`Self::mate_score`]. Used to build the noisy training twin checkpoint.
    pub fn perturb_multiplicative(&mut self, lo: f32, hi: f32) {
        debug_assert!(lo > 0.0 && hi >= lo);
        let mut state = self.weight_seed;
        let mut next_u01 = || {
            state = splitmix64(state);
            // Map to (0, 1] excluding 0 for safety.
            ((state >> 11) as f64 / ((1u64 << 53) as f64)).clamp(f64::MIN_POSITIVE, 1.0) as f32
        };
        let mut scale = || lo + (hi - lo) * next_u01();

        for &pt in ALL_PIECE_TYPES {
            if let Some(v) = self.piece.get_mut(&pt) {
                *v *= scale();
            }
        }
        for f in &mut self.rank_factor {
            *f *= scale();
        }
        let mut scale_i32 = |x: i32| -> i32 {
            let s = scale();
            ((x as f32) * s).round() as i32
        };
        self.royal_alive = scale_i32(self.royal_alive);
        self.sole_royal_factor = scale_i32(self.sole_royal_factor);
        self.de_advance = scale_i32(self.de_advance);
        self.undeveloped_home = scale_i32(self.undeveloped_home);
        self.advance = scale_i32(self.advance);
        self.noise_scale *= scale() as f64;
        self.rebuild_piece_value_table();
    }
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
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

    /// Config B: clone of [`Self::seed`] with multiplicative U(lo, hi) weight noise.
    pub fn seed_noisy(name: &str, lo: f32, hi: f32) -> Self {
        let mut cp = Self::seed(name);
        cp.weights.perturb_multiplicative(lo, hi);
        cp
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

    score += weights.royal_alive as f32 * (black_royals as f32 - white_royals as f32);
    if black_royals == 1 {
        score -= weights.sole_royal_factor as f32;
    }
    if white_royals == 1 {
        score += weights.sole_royal_factor as f32;
    }

    score += de_positional(black, Color::Black, weights) as f32;
    score -= de_positional(white, Color::White, weights) as f32;

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
    if piece.piece_type.is_royal() {
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
        if p.piece_type.is_royal() {
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

/// Default on-disk seed path (config A).
pub const DEFAULT_MODEL_PATH: &str = "models/ab-seed.json";
/// Noisy twin of the seed (config B) for training/eval prototyping.
pub const DEFAULT_NOISY_MODEL_PATH: &str = "models/ab-seed-noisy.json";

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
        assert_eq!(back.weights.piece_value(PieceType::King), 1800.0);
        assert_eq!(back.weights.piece_value(PieceType::CrownPrince), 1600.0);
        assert_eq!(back.weights.piece_value(PieceType::GreatGeneral), 1100.0);
        assert_eq!(back.weights.piece_value(PieceType::FreeEagle), 40.0);
        assert_eq!(back.weights.piece_value(PieceType::WoodenDove), 40.0);
        assert_eq!(back.weights.piece_value(PieceType::HookMover), 1200.0);
        assert_eq!(back.weights.piece_value(PieceType::Lion), 15.0);
        // Pawn: Simple 1 dir × 1 step × 0.5
        assert!((back.weights.piece_value(PieceType::Pawn) - 0.5).abs() < 1e-3);
        // Rook-like: 4 orthogonal NoJump → 16
        assert!((back.weights.piece_value(PieceType::Rook) - 16.0).abs() < 1e-3);
        assert_eq!(back.weights.piece.len(), ALL_PIECE_TYPES.len());
        assert!(!back.weights.piece_value_table.is_empty());
        assert_eq!(back.weights.advance, 0);
        assert!((back.weights.rank_factor[0] - 0.5).abs() < 1e-3);
        assert!((back.weights.rank_factor[RANK_PST_MID as usize] - 1.2).abs() < 1e-3);
        assert!((back.weights.rank_factor[RANK_PST_PROMO as usize] - 1.5).abs() < 1e-3);
    }

    #[test]
    fn rank_pst_caps_large_piece_bonus() {
        let weights = EvalWeights::seed();
        // GreatGeneral has no opening home; use Tengu (1200) on mid / promo ranks.
        let mid = Piece::new(
            PieceType::Tengu,
            Color::Black,
            Position::new(6, RANK_PST_MID).unwrap(),
        );
        let promo = Piece::new(
            PieceType::Tengu,
            Color::Black,
            Position::new(6, RANK_PST_PROMO).unwrap(),
        );
        let v = weights.piece_value(PieceType::Tengu);
        let mid_val = positional_piece_value(&mid, &weights);
        let promo_val = positional_piece_value(&promo, &weights);
        assert!(
            (mid_val - (v + 20.0)).abs() < 1e-2,
            "mid should be v+20, got {mid_val} (v={v})"
        );
        assert!(
            (promo_val - (v + 50.0)).abs() < 1e-2,
            "promo should be v+50, got {promo_val} (v={v})"
        );
        let back = Piece::new(
            PieceType::Tengu,
            Color::Black,
            Position::new(6, 0).unwrap(),
        );
        assert!((positional_piece_value(&back, &weights) - 0.5 * v).abs() < 1e-2);
    }

    #[test]
    fn noisy_checkpoint_differs_reproducibly() {
        let a = EvalCheckpoint::seed("ab-seed");
        let b1 = EvalCheckpoint::seed_noisy("ab-seed-noisy", 0.8, 1.2);
        let b2 = EvalCheckpoint::seed_noisy("ab-seed-noisy", 0.8, 1.2);
        assert_eq!(b1.weights.mate_score, a.weights.mate_score);
        assert_ne!(
            b1.weights.piece_value(PieceType::Pawn),
            a.weights.piece_value(PieceType::Pawn)
        );
        assert_eq!(
            b1.weights.piece_value(PieceType::King),
            b2.weights.piece_value(PieceType::King)
        );
        assert_eq!(b1.weights.rank_factor, b2.weights.rank_factor);
        // Perturbed factors stay in a sane band around the seed curve.
        for (i, &f) in b1.weights.rank_factor.iter().enumerate() {
            assert!(
                f > 0.3 && f < 2.0,
                "noisy rank_factor[{i}]={f} out of band"
            );
        }
    }

    #[test]
    fn export_seed_checkpoints_to_models() {
        let a = EvalCheckpoint::seed("ab-seed");
        a.save_path(DEFAULT_MODEL_PATH)
            .expect("write ab-seed.json");
        let b = EvalCheckpoint::seed_noisy("ab-seed-noisy", 0.8, 1.2);
        b.save_path(DEFAULT_NOISY_MODEL_PATH)
            .expect("write ab-seed-noisy.json");
        let loaded_a = EvalCheckpoint::load_path(DEFAULT_MODEL_PATH).unwrap();
        let loaded_b = EvalCheckpoint::load_path(DEFAULT_NOISY_MODEL_PATH).unwrap();
        assert_eq!(loaded_a.weights.advance, 0);
        assert_eq!(loaded_a.name, "ab-seed");
        assert_eq!(loaded_b.name, "ab-seed-noisy");
        assert_ne!(
            loaded_a.weights.piece_value(PieceType::GoldGeneral),
            loaded_b.weights.piece_value(PieceType::GoldGeneral)
        );
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

    #[test]
    fn undeveloped_home_penalizes_home_rank_or_behind() {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.undeveloped_home = 3;

        let mut state = GameState::new();
        state.setup_initial_position();
        let opening = evaluate_absolute_black(state.get_board(), &weights, 0);

        let black_pen =
            undeveloped_home_penalty(state.get_board().pieces_by_color(Color::Black), &weights);
        let white_pen =
            undeveloped_home_penalty(state.get_board().pieces_by_color(Color::White), &weights);
        assert!((black_pen - white_pen).abs() < 1e-3);
        assert!(black_pen > 100.0, "expected full-army undeveloped penalty, got {black_pen}");

        // Pawn: 20% floor would be 3, but cap at 90% of value (0.45).
        let from = Position::new(16, 10).unwrap();
        let to = Position::new(16, 11).unwrap();
        assert_eq!(
            state.get_board().get_piece(from).map(|p| p.piece_type),
            Some(PieceType::Pawn)
        );
        let pawn = state.get_board().get_piece(from).unwrap();
        let pawn_pen = undeveloped_penalty_for_piece(&pawn, &weights);
        assert!(
            (pawn_pen - 0.45).abs() < 1e-3,
            "pawn undeveloped should be 0.9*0.5=0.45, got {pawn_pen}"
        );
        state.get_board_mut().move_piece(from, to);
        let after_pawn = evaluate_absolute_black(state.get_board(), &weights, 0);
        // Total score is rounded; clearing 0.45 may or may not change the i32 by 1.
        assert!(after_pawn >= opening);

        // Still behind home rank (rank < 10 for Black) keeps the penalty.
        let behind = Position::new(16, 9).unwrap();
        state.get_board_mut().move_piece(to, behind);
        let retreated = state.get_board().get_piece(behind).unwrap();
        assert!((undeveloped_penalty_for_piece(&retreated, &weights) - 0.45).abs() < 1e-3);

        // High-value piece: raw would be max(3, 20% * value), but capped at 20.
        // Tengu = 1200 → uncapped 240, capped 20. Tengu's opening home is back rank.
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

        // FireDemon = 24 → 4.8 (under the cap).
        let mut fd_board = Board::new();
        fd_board.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(17, 0).unwrap(),
        ));
        fd_board.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(17, 35).unwrap(),
        ));
        let fd_from = Position::new(4, 0).unwrap();
        let fd_to = Position::new(4, 1).unwrap();
        fd_board.place_piece(Piece::new(
            PieceType::FireDemon,
            Color::Black,
            fd_from,
        ));
        let fd = fd_board.get_piece(fd_from).unwrap();
        let fd_pen = undeveloped_penalty_for_piece(&fd, &weights);
        assert!((fd_pen - 4.8).abs() < 1e-3, "expected 4.8, got {fd_pen}");
        let before_fd = evaluate_absolute_black(&fd_board, &weights, 0);
        fd_board.move_piece(fd_from, fd_to);
        let after_fd = evaluate_absolute_black(&fd_board, &weights, 0);
        // Undeveloped ~4.8 plus a small rank-PST bump from progress 0 → 1.
        assert!(
            after_fd - before_fd >= 5,
            "leaving FireDemon home rank should gain at least round(4.8); delta={}",
            after_fd - before_fd
        );

        // Royals never count as undeveloped.
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
