//! Static evaluation and versioned weight checkpoints for the alpha-beta agent.

use crate::board::Board;
use crate::game_state::GameState;
use crate::movement::{
    BlockingMode, MovementCapability, MovementConfig, MovementGenerator,
};
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
pub const TARIFF_RANGE_CAPTURING: f32 = 450.0;
/// Seed buff for Tengu-style two-movers (both legs are range slides).
pub const RANGE_TWO_MOVER_BUFF: f32 = 1.10;
/// Unpromoted FreeKing seed material as a fraction of GreatGeneral (promo potential).
pub const FREE_KING_GG_FRAC: f32 = 0.75;
/// Material for a *promoted* FreeKing (FreeBaku / FreeDemon / FlyingHorse / … → FK).
/// Matches queen-range NoJump formula (`8 × 10`), not the unpromoted FK table value.
pub const PROMOTED_FREE_KING_VALUE: f32 = 8.0 * TARIFF_RANGE_NO_JUMP;
/// Prior all-two-mover retune (T150C50×T150C120) before H120O80 split.
const SEED_TWO_MOVER_BASE: f32 = 1.5 * 1.5;
/// Cumulative HookMover: ×2.25 → H120 (×1.2) → elite-swiss nudge (×1.05) → ×2.835.
pub const SEED_HOOK_MOVER_SCALE: f32 = SEED_TWO_MOVER_BASE * 1.2 * 1.05;
/// Capricorn held at H120O80 other-scale (elite C axis looked noisy).
pub const SEED_CAPRICORN_SCALE: f32 = SEED_TWO_MOVER_BASE * 0.8;
/// Other range two-movers excl. Hook/Capricorn: ×1.8 then elite nudge (×1.05) → ×1.89.
pub const SEED_OTHER_TWO_MOVER_SCALE: f32 = SEED_TWO_MOVER_BASE * 0.8 * 1.05;
/// Cumulative capturer retune: prior T150C50 (×0.5) then T150C120 (×1.2) → ×0.6.
pub const SEED_CAPTURER_SCALE: f32 = 0.5 * 1.2;

/// Quiescence / worthwhile-capture floor derived from range tariffs.
///
/// About 2.4 capturing-dirs after capturer scale (`450×0.6×2.4=648`) so mid-heavy
/// takes enter q; also at least a full 8-dir jump-ray.
pub fn seed_loud_capture_floor() -> f32 {
    (TARIFF_RANGE_CAPTURING * SEED_CAPTURER_SCALE * 2.4).max(TARIFF_RANGE_JUMP * 8.0)
}

/// Two-movers (TwoStep / FreeEagleMultiMove) or capturing-range pieces.
///
/// Used by scale-sample free params and by search for "loud" promotions into
/// these types (e.g. FreeKing→GreatGeneral).
pub fn is_big_piece(pt: PieceType) -> bool {
    if pt == PieceType::King {
        return false;
    }
    let cfg = MovementConfig::for_piece_type(pt);
    cfg.capabilities.iter().any(|cap| match cap {
        MovementCapability::TwoStep { .. } | MovementCapability::FreeEagleMultiMove { .. } => true,
        MovementCapability::Range {
            blocking: BlockingMode::Capturing,
            ..
        } => true,
        _ => false,
    })
}

/// True when promoting this type yields a [`is_big_piece`] result.
pub fn promotes_into_big_piece(pt: PieceType) -> bool {
    pt.promotes_to().is_some_and(is_big_piece)
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
            let raw = capability_material_value(first) + capability_material_value(second);
            if is_range_capability(first) && is_range_capability(second) {
                raw * RANGE_TWO_MOVER_BUFF
            } else {
                raw
            }
        }
        // Covered by overrides (WoodenDove / FreeEagle).
        MovementCapability::ConditionalDiagonalJump { .. } => 0.0,
        MovementCapability::FreeEagleMultiMove { .. } => 0.0,
    }
}

fn is_range_capability(cap: &MovementCapability) -> bool {
    matches!(cap, MovementCapability::Range { .. })
}

/// First-leg empty+enemy landings for a range two-mover (0 otherwise).
pub fn first_leg_landing_count(piece: &Piece, board: &Board) -> u32 {
    if !is_range_two_mover(piece.piece_type) {
        return 0;
    }
    let cfg = MovementConfig::for_piece_type(piece.piece_type);
    for cap in &cfg.capabilities {
        if let MovementCapability::TwoStep { first, second } = cap {
            if is_range_capability(first) && is_range_capability(second) {
                return MovementGenerator::capability_landings(piece, board, first).len() as u32;
            }
        }
    }
    0
}

fn two_mover_mob_curve(m: f32, curve: u8) -> f32 {
    match curve {
        1 => m.sqrt(),
        2 => m / (m + 10.0),
        _ => m,
    }
}

fn two_mover_mob_apply(piece: &Piece, weights: &EvalWeights) -> f32 {
    match weights.two_mover_mob_apply {
        1 => {
            let progress = match piece.color {
                Color::Black => piece.position.rank as usize,
                Color::White => (35 - piece.position.rank) as usize,
            };
            weights
                .rank_factor_fast
                .get(progress)
                .copied()
                .unwrap_or(1.0)
        }
        2 => weights
            .file_factor
            .get(piece.position.file as usize)
            .copied()
            .unwrap_or(1.0),
        _ => 1.0,
    }
}

fn two_mover_mobility_of(pieces: &[Piece], board: &Board, weights: &EvalWeights) -> f32 {
    if weights.two_mover_mob_k == 0.0 {
        return 0.0;
    }
    let mut s = 0.0f32;
    for p in pieces {
        if !is_range_two_mover(p.piece_type) {
            continue;
        }
        let m = first_leg_landing_count(p, board) as f32;
        s += weights.two_mover_mob_k * two_mover_mob_curve(m, weights.two_mover_mob_curve)
            * two_mover_mob_apply(p, weights);
    }
    s
}

/// True when the piece has a TwoStep whose both legs are range slides (Tengu family).
pub fn is_range_two_mover(pt: PieceType) -> bool {
    MovementConfig::for_piece_type(pt)
        .capabilities
        .iter()
        .any(|cap| match cap {
            MovementCapability::TwoStep { first, second } => {
                is_range_capability(first) && is_range_capability(second)
            }
            _ => false,
        })
}

/// Capturing-range pieces, plus FreeKing (regularly promotes to GreatGeneral).
pub fn is_range_capturer(pt: PieceType) -> bool {
    if pt == PieceType::FreeKing {
        return true;
    }
    MovementConfig::for_piece_type(pt)
        .capabilities
        .iter()
        .any(|cap| {
            matches!(
                cap,
                MovementCapability::Range {
                    blocking: BlockingMode::Capturing,
                    ..
                }
            )
        })
}

fn explicit_material_override(pt: PieceType) -> Option<f32> {
    match pt {
        PieceType::King => Some(100.0),
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
///
/// After the capability formula, applies cumulative loud-grid retunes: HookMover
/// ×[`SEED_HOOK_MOVER_SCALE`], Capricorn ×[`SEED_CAPRICORN_SCALE`], other range
/// two-movers ×[`SEED_OTHER_TWO_MOVER_SCALE`], capturers ×[`SEED_CAPTURER_SCALE`].
/// Unpromoted FreeKing is priced off scaled GreatGeneral (inherits capturer
/// scale once). Pieces that *promote into* FreeKing use
/// [`PROMOTED_FREE_KING_VALUE`] via [`material_piece_value`].
pub fn seed_piece_value(pt: PieceType) -> f32 {
    // Starting queen: already very mobile; price in most of the GG it becomes.
    if pt == PieceType::FreeKing {
        return seed_piece_value(PieceType::GreatGeneral) * FREE_KING_GG_FRAC;
    }
    let raw = match explicit_material_override(pt) {
        // Overrides skip the TwoStep formula path — still apply the range-two-mover buff.
        Some(v) if is_range_two_mover(pt) => v * RANGE_TWO_MOVER_BUFF,
        Some(v) => v,
        None => formula_piece_value(pt),
    };
    if pt == PieceType::HookMover {
        raw * SEED_HOOK_MOVER_SCALE
    } else if pt == PieceType::Capricorn {
        raw * SEED_CAPRICORN_SCALE
    } else if is_range_two_mover(pt) {
        raw * SEED_OTHER_TWO_MOVER_SCALE
    } else if is_range_capturer(pt) {
        raw * SEED_CAPTURER_SCALE
    } else {
        raw
    }
}

/// Board material for one piece. Promoted FreeKings stay at queen-range 80.
pub fn material_piece_value(piece: &Piece, weights: &EvalWeights) -> f32 {
    if piece.piece_type == PieceType::FreeKing && piece.is_promoted {
        return PROMOTED_FREE_KING_VALUE;
    }
    weights.piece_value(piece.piece_type)
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

/// Tropism class scale: short=1, normal range≈0.25, capturing-range-only=0.
pub fn tropism_class_scale(pt: PieceType) -> f32 {
    match tropism_class_bucket(pt) {
        TropismClass::None | TropismClass::CapturingRange => 0.0,
        TropismClass::Short => 1.0,
        TropismClass::Range => DEFAULT_EG_TROPISM_RANGE_SCALE,
    }
}

fn chebyshev_distance(a: Position, b: Position) -> u8 {
    let df = (a.file as i16 - b.file as i16).unsigned_abs() as u8;
    let dr = (a.rank as i16 - b.rank as i16).unsigned_abs() as u8;
    df.max(dr)
}

/// Opponent-density endgame gate in [0, 1]: high when enemy non-royals are few.
pub fn eg_density_weight(enemy_non_royals: usize, density_n: f32) -> f32 {
    if density_n <= 0.0 {
        return 0.0;
    }
    ((density_n - enemy_non_royals as f32) / density_n).clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TropismClass {
    None,
    Short,
    Range,
    CapturingRange,
}

fn tropism_class_bucket(pt: PieceType) -> TropismClass {
    if pt.is_royal() {
        return TropismClass::None;
    }
    let cfg = MovementConfig::for_piece_type(pt);
    let mut has_capturing_range = false;
    let mut has_other_range = false;
    for cap in &cfg.capabilities {
        if let MovementCapability::Range { blocking, .. } = cap {
            if *blocking == BlockingMode::Capturing {
                has_capturing_range = true;
            } else {
                has_other_range = true;
            }
        }
    }
    if has_capturing_range && !has_other_range {
        TropismClass::CapturingRange
    } else if has_capturing_range || has_other_range {
        TropismClass::Range
    } else {
        TropismClass::Short
    }
}

fn piece_tropism_scale(pt: PieceType, weights: &EvalWeights) -> f32 {
    match tropism_class_bucket(pt) {
        TropismClass::None | TropismClass::CapturingRange => 0.0,
        TropismClass::Short => weights.eg_tropism_short_scale,
        TropismClass::Range => weights.eg_tropism_range_scale,
    }
}

/// Linear top-k + tail tropism (not density-gated): `σ·T_top + σ_tail·T_tail`.
///
/// Per piece: `u = s·(d_ref − d)`. Closest `k` pieces (tie: larger s) get `σ`;
/// the rest get `σ_tail` so conversion does not stall once k are close.
fn linear_topk_tail_tropism(
    our_pieces: &[Piece],
    enemy_royals: &[Position],
    weights: &EvalWeights,
) -> f32 {
    if enemy_royals.is_empty() {
        return 0.0;
    }
    let d_ref = weights.eg_tropism_d_ref;
    // (distance, -scale for reverse tie-break, u)
    let mut entries: Vec<(u8, f32, f32)> = Vec::new();
    for p in our_pieces {
        // Royals never receive tropism (approaching with the king is not the goal).
        if p.piece_type.is_royal() {
            continue;
        }
        let s = piece_tropism_scale(p.piece_type, weights);
        if s == 0.0 {
            continue;
        }
        let mut best = u8::MAX;
        for &r in enemy_royals {
            best = best.min(chebyshev_distance(p.position, r));
        }
        let u = s * (d_ref - best as f32);
        entries.push((best, -s, u));
    }
    if entries.is_empty() {
        return 0.0;
    }
    entries.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    let k = weights.eg_tropism_topk.max(0) as usize;
    let mut t_top = 0.0f32;
    let mut t_tail = 0.0f32;
    for (i, (_d, _neg_s, u)) in entries.iter().enumerate() {
        if i < k {
            t_top += *u;
        } else {
            t_tail += *u;
        }
    }
    weights.eg_tropism_scale * t_top + weights.eg_tropism_tail_scale * t_tail
}

fn apply_tropism_cap(trop: f32, weights: &EvalWeights) -> f32 {
    let cap = weights.eg_tropism_cap.abs();
    if cap > 0.0 {
        trop.clamp(-cap, cap)
    } else {
        trop
    }
}

fn count_non_royals(pieces: &[Piece]) -> usize {
    pieces.iter().filter(|p| !p.piece_type.is_royal()).count()
}

fn raw_material_of(pieces: &[Piece], weights: &EvalWeights) -> f32 {
    pieces
        .iter()
        .map(|p| material_piece_value(p, weights))
        .sum()
}

/// Rank-PST excess over base material: `positional - base` (0 for royals / skip-PST).
pub fn rank_pst_excess(piece: &Piece, weights: &EvalWeights) -> f32 {
    positional_piece_value(piece, weights) - material_piece_value(piece, weights)
}

fn pst_excess_of(pieces: &[Piece], weights: &EvalWeights) -> f32 {
    pieces.iter().map(|p| rank_pst_excess(p, weights)).sum()
}

fn enemy_royal_positions(pieces: &[Piece]) -> Vec<Position> {
    pieces
        .iter()
        .filter(|p| p.piece_type.is_royal())
        .map(|p| p.position)
        .collect()
}

/// Phase weight in [0,1]: density gate, zero when behind on raw material.
pub fn side_phase_weight(
    our_raw_mat: f32,
    enemy_raw_mat: f32,
    enemy_non_royals: usize,
    weights: &EvalWeights,
) -> f32 {
    if our_raw_mat - enemy_raw_mat < weights.eg_ahead_min {
        return 0.0;
    }
    eg_density_weight(enemy_non_royals, weights.eg_density_n)
}

/// One side's blended positional: `(1-w)·PST_excess + w·trop` (linear top-k+tail).
fn side_blended_positional(
    our_pieces: &[Piece],
    enemy_pieces: &[Piece],
    our_raw_mat: f32,
    enemy_raw_mat: f32,
    weights: &EvalWeights,
) -> f32 {
    let w = side_phase_weight(
        our_raw_mat,
        enemy_raw_mat,
        count_non_royals(enemy_pieces),
        weights,
    );
    let pst = pst_excess_of(our_pieces, weights);
    if w <= 0.0 {
        return pst;
    }
    let royals = enemy_royal_positions(enemy_pieces);
    let trop = if royals.is_empty() {
        0.0
    } else {
        apply_tropism_cap(
            linear_topk_tail_tropism(our_pieces, &royals, weights),
            weights,
        )
    };
    (1.0 - w) * pst + w * trop
}

/// Black-positive endgame tropism contribution only (`w·trop`), for tests / diagnostics.
pub fn eg_tropism_term_black(black: &[Piece], white: &[Piece], weights: &EvalWeights) -> f32 {
    if weights.eg_tropism_scale == 0.0 && weights.eg_tropism_tail_scale == 0.0 {
        return 0.0;
    }
    let black_mat = raw_material_of(black, weights);
    let white_mat = raw_material_of(white, weights);
    let w_b = side_phase_weight(black_mat, white_mat, count_non_royals(white), weights);
    let w_w = side_phase_weight(white_mat, black_mat, count_non_royals(black), weights);
    let t_b = if w_b > 0.0 {
        apply_tropism_cap(
            linear_topk_tail_tropism(black, &enemy_royal_positions(white), weights),
            weights,
        )
    } else {
        0.0
    };
    let t_w = if w_w > 0.0 {
        apply_tropism_cap(
            linear_topk_tail_tropism(white, &enemy_royal_positions(black), weights),
            weights,
        )
    } else {
        0.0
    };
    w_b * t_b - w_w * t_w
}

/// Black-positive phase-blended positional (PST fade + tropism fade-in).
pub fn eg_blended_positional_black(
    black: &[Piece],
    white: &[Piece],
    weights: &EvalWeights,
) -> f32 {
    let black_mat = raw_material_of(black, weights);
    let white_mat = raw_material_of(white, weights);
    side_blended_positional(black, white, black_mat, white_mat, weights)
        - side_blended_positional(white, black, white_mat, black_mat, weights)
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

/// Default range-piece tropism multiplier (short movers use 1.0).
pub const DEFAULT_EG_TROPISM_RANGE_SCALE: f32 = 0.25;
/// Density ramp length: w=1 at 0 enemy non-royals, w=0 at ≥N.
fn default_eg_density_n() -> f32 {
    20.0
}
/// Neutral Chebyshev distance for centered tropism (~half board).
fn default_eg_tropism_d_ref() -> f32 {
    18.0
}
/// Top-k tropism scale: ~+σ eval per short Chebyshev step among the closest k.
fn default_eg_tropism_scale() -> f32 {
    1.2
}
/// Absolute clamp on tropism contribution (0 = uncapped).
fn default_eg_tropism_cap() -> f32 {
    0.0
}
/// Must be at least this far ahead on raw material to apply our tropism.
fn default_eg_ahead_min() -> f32 {
    10.0
}
fn default_eg_tropism_short_scale() -> f32 {
    1.0
}
fn default_eg_tropism_range_scale() -> f32 {
    DEFAULT_EG_TROPISM_RANGE_SCALE
}
fn default_eg_tropism_topk() -> u32 {
    8
}
fn default_eg_tropism_tail_scale() -> f32 {
    0.25
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
    /// Seed: `[0, 0, 100, 110]` → 1→0, 2→100, 3+→110.
    /// Encourages keeping a spare royal (CP from DE) without the old huge promo push.
    #[serde(default = "default_royal_bonus_by_count")]
    pub royal_bonus_by_count: Vec<i32>,
    /// Legacy DE/GoBetween advance scale (unused by seed; DE uses generic rank PST).
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
    /// Horizontally symmetric file multipliers (absolute file 0..35). Seed: all 1.0.
    #[serde(default = "seed_file_factors_vec")]
    pub file_factor: Vec<f32>,
    /// Endgame tropism scale for the closest `eg_tropism_topk` pieces.
    #[serde(default = "default_eg_tropism_scale")]
    pub eg_tropism_scale: f32,
    /// Absolute cap on tropism term magnitude (0 = uncapped).
    #[serde(default = "default_eg_tropism_cap")]
    pub eg_tropism_cap: f32,
    /// Density gate N: w = clamp((N - enemy_non_royals) / N, 0, 1).
    #[serde(default = "default_eg_density_n")]
    pub eg_density_n: f32,
    /// Neutral Chebyshev distance for tropism centering (`u = s*(d_ref - d)`).
    #[serde(default = "default_eg_tropism_d_ref")]
    pub eg_tropism_d_ref: f32,
    /// Raw-material lead required before our tropism applies.
    #[serde(default = "default_eg_ahead_min")]
    pub eg_ahead_min: f32,
    /// Tropism class scale for short / no-range movers.
    #[serde(default = "default_eg_tropism_short_scale")]
    pub eg_tropism_short_scale: f32,
    /// Tropism class scale for normal range movers.
    #[serde(default = "default_eg_tropism_range_scale")]
    pub eg_tropism_range_scale: f32,
    /// Number of closest eligible pieces getting full tropism scale.
    #[serde(default = "default_eg_tropism_topk")]
    pub eg_tropism_topk: u32,
    /// Tropism scale for pieces outside the top-k set.
    #[serde(default = "default_eg_tropism_tail_scale")]
    pub eg_tropism_tail_scale: f32,
    /// First-leg mobility scale for range two-movers (0 = off).
    #[serde(default)]
    pub two_mover_mob_k: f32,
    /// 0 linear `m`, 1 `sqrt(m)`, 2 `m/(m+10)`.
    #[serde(default)]
    pub two_mover_mob_curve: u8,
    /// 0 raw, 1 `× rank_factor_fast[progress]`, 2 `× file_factor[file]`.
    #[serde(default)]
    pub two_mover_mob_apply: u8,
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
    vec![0, 0, 100, 110]
}

/// Fast PST shape with tunable anchors.
///
/// - ranks `[0, pawn]`: linear `back → 1.0`
/// - ranks `(pawn, opp)`: flat `1.0` (mid)
/// - ranks `[opp, promo)`: flat `1.0 + opp_half_frac·(promo_factor − 1.0)`
/// - ranks `[promo, 35]`: flat `promo_factor`
///
/// Seed defaults after file-PST Swiss: `back=0.65`, `opp_half_frac=0.75`
/// (→ 115% when promo is 120%), `promo_factor=1.2`.
pub fn seed_rank_factors_fast_params(back: f32, opp_half_frac: f32, promo_factor: f32) -> [f32; 36] {
    let pawn = RANK_PAWN_START;
    let opp = RANK_OPPONENT_HALF;
    let promo = RANK_PST_PROMO;
    let mid = 1.0f32;
    let opp_half = mid + opp_half_frac * (promo_factor - mid);
    let mut factors = [1.0f32; 36];
    for r in 0u8..36 {
        factors[r as usize] = if r <= pawn {
            lerp(back, mid, r as f32 / pawn as f32)
        } else if r < opp {
            mid
        } else if r < promo {
            opp_half
        } else {
            promo_factor
        };
    }
    factors
}

/// Fast PST: 65% back → 100% pawn start → 100% to mid → 115% opponent half → 120% promo.
pub fn seed_rank_factors_fast() -> [f32; 36] {
    seed_rank_factors_fast_params(0.65, 0.75, 1.2)
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

/// Left-wing dirs (file−): W|NW|SW.
const FILE_PST_LEFT_DIRS: u8 = 0xE0;
/// Right-wing dirs (file+): E|NE|SE.
const FILE_PST_RIGHT_DIRS: u8 = 0x0E;

/// Strongly one-sided / Left–Right family pieces excluded from file PST.
pub fn is_file_pst_asymmetric(pt: PieceType) -> bool {
    matches!(
        pt,
        PieceType::LeftArmy
            | PieceType::RightArmy
            | PieceType::LeftDog
            | PieceType::RightDog
            | PieceType::LeftDragon
            | PieceType::RightDragon
            | PieceType::LeftGeneral
            | PieceType::RightGeneral
            | PieceType::LeftHowlingDog
            | PieceType::RightHowlingDog
            | PieceType::LeftTiger
            | PieceType::RightTiger
            | PieceType::BlueDragon
            | PieceType::DivineDragon
    )
}

fn capability_wing_dirs(cap: &MovementCapability) -> u8 {
    match cap {
        MovementCapability::Simple { directions, .. }
        | MovementCapability::Range { directions, .. }
        | MovementCapability::ConditionalDiagonalJump { directions, .. } => *directions,
        MovementCapability::TwoStep { first, second } => {
            capability_wing_dirs(first) | capability_wing_dirs(second)
        }
        MovementCapability::FreeEagleMultiMove { .. } => 0xFF,
        MovementCapability::Jumping { offsets } => {
            let mut d = 0u8;
            for &(df, _) in offsets {
                if df > 0 {
                    d |= FILE_PST_RIGHT_DIRS;
                }
                if df < 0 {
                    d |= FILE_PST_LEFT_DIRS;
                }
            }
            d
        }
    }
}

/// True when the piece has at least one left-wing and one right-wing direction.
pub fn has_both_wing_dirs(pt: PieceType) -> bool {
    let cfg = MovementConfig::for_piece_type(pt);
    let mut dirs = 0u8;
    for cap in &cfg.capabilities {
        dirs |= capability_wing_dirs(cap);
    }
    (dirs & FILE_PST_LEFT_DIRS) != 0 && (dirs & FILE_PST_RIGHT_DIRS) != 0
}

/// File-PST eligibility: not royal, not enumerated asymmetric, both wings present.
pub fn uses_file_pst(pt: PieceType) -> bool {
    !pt.is_royal() && !is_file_pst_asymmetric(pt) && has_both_wing_dirs(pt)
}

/// Horizontally symmetric file table: `lerp(center, edge, |file-17.5|/17.5)`.
pub fn seed_file_factors(edge: f32, center: f32) -> [f32; 36] {
    let mut factors = [1.0f32; 36];
    for f in 0u8..36 {
        let t = ((f as f32) - 17.5).abs() / 17.5;
        factors[f as usize] = lerp(center, edge, t);
    }
    factors
}

fn seed_file_factors_vec() -> Vec<f32> {
    seed_file_factors(1.0, 1.0).to_vec()
}

/// Material contribution for one piece including rank (+ optional file) PST.
/// Royals and jump/capturing-range pieces skip rank PST (factor 1); file PST is separate.
pub fn positional_piece_value(piece: &Piece, weights: &EvalWeights) -> f32 {
    let mut score = material_piece_value(piece, weights);
    if !piece.piece_type.is_royal() && !skips_rank_pst(piece.piece_type) {
        let progress = match piece.color {
            Color::Black => piece.position.rank as usize,
            Color::White => (35 - piece.position.rank) as usize,
        };
        let table = if is_fast_piece(piece.piece_type) {
            &weights.rank_factor_fast
        } else {
            &weights.rank_factor_slow
        };
        score *= table.get(progress).copied().unwrap_or(1.0);
    }
    if uses_file_pst(piece.piece_type) {
        let file = piece.position.file as usize;
        score *= weights.file_factor.get(file).copied().unwrap_or(1.0);
    }
    score
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
            file_factor: seed_file_factors_vec(),
            eg_tropism_scale: default_eg_tropism_scale(),
            eg_tropism_cap: default_eg_tropism_cap(),
            eg_density_n: default_eg_density_n(),
            eg_tropism_d_ref: default_eg_tropism_d_ref(),
            eg_ahead_min: default_eg_ahead_min(),
            eg_tropism_short_scale: default_eg_tropism_short_scale(),
            eg_tropism_range_scale: default_eg_tropism_range_scale(),
            eg_tropism_topk: default_eg_tropism_topk(),
            eg_tropism_tail_scale: default_eg_tropism_tail_scale(),
            two_mover_mob_k: 0.0,
            two_mover_mob_curve: 0,
            two_mover_mob_apply: 0,
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
    // Base material (no PST) + phase-blended positional (PST ↔ tropism).
    score += raw_material_of(black, weights) - raw_material_of(white, weights);
    score += eg_blended_positional_black(black, white, weights);

    score += weights.royal_bonus(black_royals) as f32 - weights.royal_bonus(white_royals) as f32;

    score -= undeveloped_home_penalty(black, weights);
    score += undeveloped_home_penalty(white, weights);

    score += advance_positional(black, Color::Black, weights) as f32;
    score -= advance_positional(white, Color::White, weights) as f32;

    score += two_mover_mobility_of(black, board, weights);
    score -= two_mover_mobility_of(white, board, weights);

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
    let value = material_piece_value(piece, weights);
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
    fn range_two_mover_and_capturer_classes() {
        assert!(is_range_two_mover(PieceType::Tengu));
        assert!(is_range_two_mover(PieceType::Peacock));
        assert!(is_range_two_mover(PieceType::Capricorn));
        assert!(is_range_two_mover(PieceType::HookMover));
        assert!(!is_range_two_mover(PieceType::Lion));
        assert!(!is_range_two_mover(PieceType::GreatGeneral));

        assert!(is_range_capturer(PieceType::GreatGeneral));
        assert!(is_range_capturer(PieceType::ViceGeneral));
        assert!(is_range_capturer(PieceType::FreeKing));
        assert!(is_range_capturer(PieceType::FierceDragon));
        assert!(!is_range_capturer(PieceType::Tengu));
        assert!(!is_range_capturer(PieceType::Lion));
        assert!(!is_range_capturer(PieceType::FreeEagle));
    }

    #[test]
    fn seed_material_values_match_tariffs() {
        let w = EvalWeights::seed();
        assert!((w.piece_value(PieceType::Pawn) - 1.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::CrownPrince) - 8.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::King) - 100.0).abs() < 1e-3);
        // Elite nudge on H120O80: hook ×2.835, Capricorn held ×1.8, other ×1.89, capturers ×0.6.
        // Unpromoted FK = ¾ scaled GG (1620); promoted FK stays queen-range 80.
        assert!((SEED_HOOK_MOVER_SCALE - 2.835).abs() < 1e-6);
        assert!((SEED_CAPRICORN_SCALE - 1.8).abs() < 1e-6);
        assert!((SEED_OTHER_TWO_MOVER_SCALE - 1.89).abs() < 1e-6);
        assert!((SEED_CAPTURER_SCALE - 0.6).abs() < 1e-6);
        assert!(
            (w.piece_value(PieceType::GreatGeneral)
                - 8.0 * TARIFF_RANGE_CAPTURING * SEED_CAPTURER_SCALE)
                .abs()
                < 1e-3
        );
        assert!(
            (w.piece_value(PieceType::FreeKing)
                - w.piece_value(PieceType::GreatGeneral) * FREE_KING_GG_FRAC)
                .abs()
                < 1e-3
        );
        assert!((w.piece_value(PieceType::FreeKing) - 1620.0).abs() < 1e-3);
        assert!((PROMOTED_FREE_KING_VALUE - 8.0 * TARIFF_RANGE_NO_JUMP).abs() < 1e-3);
        let mut promo_fk = Piece::new(
            PieceType::FreeBaku,
            Color::Black,
            Position::new(6, 18).unwrap(),
        );
        promo_fk.promote();
        assert_eq!(promo_fk.piece_type, PieceType::FreeKing);
        assert!(promo_fk.is_promoted);
        assert!(
            (material_piece_value(&promo_fk, &w) - PROMOTED_FREE_KING_VALUE).abs() < 1e-3
        );
        let natural_fk = Piece::new(
            PieceType::FreeKing,
            Color::Black,
            Position::new(6, 18).unwrap(),
        );
        assert!(
            (material_piece_value(&natural_fk, &w) - w.piece_value(PieceType::FreeKing)).abs()
                < 1e-3
        );
        assert!((w.piece_value(PieceType::Shitennou) - 400.0).abs() < 1e-3);
        assert!((w.piece_value(PieceType::GreatEagle) - 160.0).abs() < 1e-3);
        assert!(
            (w.piece_value(PieceType::FierceDragon) - 1806.0 * SEED_CAPTURER_SCALE).abs() < 1e-3
        );
        assert!(
            (w.piece_value(PieceType::ViceGeneral) - 2304.0 * SEED_CAPTURER_SCALE).abs() < 1e-3
        );
        // Range two-movers: base override × buff × class scale (H/C/O split).
        assert!(
            (w.piece_value(PieceType::Peacock)
                - 800.0 * RANGE_TWO_MOVER_BUFF * SEED_OTHER_TWO_MOVER_SCALE)
                .abs()
                < 1e-3
        );
        assert!(
            (w.piece_value(PieceType::Tengu)
                - 1200.0 * RANGE_TWO_MOVER_BUFF * SEED_OTHER_TWO_MOVER_SCALE)
                .abs()
                < 1e-3
        );
        assert!(
            (w.piece_value(PieceType::Capricorn)
                - 1500.0 * RANGE_TWO_MOVER_BUFF * SEED_CAPRICORN_SCALE)
                .abs()
                < 1e-3
        );
        assert!(
            (w.piece_value(PieceType::HookMover)
                - 2000.0 * RANGE_TWO_MOVER_BUFF * SEED_HOOK_MOVER_SCALE)
                .abs()
                < 1e-3
        );
        assert!((w.piece_value(PieceType::HookMover) - 6237.0).abs() < 1e-2);
        assert!((w.piece_value(PieceType::Capricorn) - 2970.0).abs() < 1e-2);
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
        assert!((seed_loud_capture_floor() - 648.0).abs() < 1e-3);
        assert!(
            (seed_loud_capture_floor()
                - (TARIFF_RANGE_CAPTURING * SEED_CAPTURER_SCALE * 2.4).max(TARIFF_RANGE_JUMP * 8.0))
                .abs()
                < 1e-6
        );
        assert!(TARIFF_RANGE_CAPTURING * SEED_CAPTURER_SCALE * 2.4 >= TARIFF_RANGE_JUMP * 8.0);
    }

    #[test]
    fn seed_round_trip_json() {
        let cp = EvalCheckpoint::seed("ab-seed");
        let text = serde_json::to_string(&cp).unwrap();
        let mut back: EvalCheckpoint = serde_json::from_str(&text).unwrap();
        back.weights.rebuild_piece_value_table();
        assert_eq!(back.format_version, 1);
        assert_eq!(back.weights.piece_value(PieceType::King), 100.0);
        assert_eq!(back.weights.piece_value(PieceType::CrownPrince), 8.0);
        assert_eq!(
            back.weights.piece_value(PieceType::GreatGeneral),
            8.0 * TARIFF_RANGE_CAPTURING * SEED_CAPTURER_SCALE
        );
        assert_eq!(back.weights.piece_value(PieceType::FreeEagle), 30.0);
        assert_eq!(back.weights.piece_value(PieceType::WoodenDove), 50.0);
        assert!(
            (back.weights.piece_value(PieceType::HookMover)
                - 2000.0 * RANGE_TWO_MOVER_BUFF * SEED_HOOK_MOVER_SCALE)
                .abs()
                < 1e-3
        );
        assert_eq!(back.weights.piece_value(PieceType::Lion), 15.0);
        assert!((back.weights.piece_value(PieceType::Pawn) - 1.0).abs() < 1e-3);
        assert_eq!(back.weights.piece.len(), ALL_PIECE_TYPES.len());
        assert!(!back.weights.piece_value_table.is_empty());
        assert_eq!(back.weights.advance, 0);
        assert_eq!(back.weights.undeveloped_home, 0);
        assert_eq!(back.weights.de_advance, 0);
        assert!((back.weights.rank_factor_fast[0] - 0.65).abs() < 1e-3);
        assert!((back.weights.rank_factor_fast[RANK_PAWN_START as usize] - 1.0).abs() < 1e-3);
        assert!((back.weights.rank_factor_fast[RANK_OPPONENT_HALF as usize] - 1.15).abs() < 1e-3);
        assert!((back.weights.rank_factor_fast[RANK_PST_PROMO as usize] - 1.2).abs() < 1e-3);
        assert!((back.weights.rank_factor_slow[0] - 0.1).abs() < 1e-3);
        assert!((back.weights.rank_factor_slow[RANK_PAWN_START as usize] - 0.6).abs() < 1e-3);
        assert!((back.weights.rank_factor_slow[RANK_OPPONENT_HALF as usize] - 1.0).abs() < 1e-3);
        assert!((back.weights.rank_factor_slow[RANK_PST_PROMO as usize] - 1.2).abs() < 1e-3);
        assert_eq!(back.weights.royal_bonus(1), 0);
        assert_eq!(back.weights.royal_bonus(2), 100);
        assert_eq!(back.weights.royal_bonus(3), 110);
        assert!((back.weights.eg_tropism_scale - 1.2).abs() < 1e-6);
        assert!((back.weights.two_mover_mob_k - 0.0).abs() < 1e-6);
        assert!((back.weights.eg_tropism_cap - 0.0).abs() < 1e-6);
        assert!((back.weights.eg_density_n - 20.0).abs() < 1e-6);
        assert!((back.weights.eg_tropism_d_ref - 18.0).abs() < 1e-6);
        assert_eq!(back.weights.eg_tropism_topk, 8);
        assert!((back.weights.eg_tropism_tail_scale - 0.25).abs() < 1e-6);
        assert!((back.weights.eg_tropism_range_scale - DEFAULT_EG_TROPISM_RANGE_SCALE).abs() < 1e-6);
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
        assert!((positional_piece_value(&back, &weights) - 0.65 * v).abs() < 1e-2);
        assert!((positional_piece_value(&pawn_rank, &weights) - v).abs() < 1e-2);
        assert!((positional_piece_value(&promo, &weights) - 1.2 * v).abs() < 1e-2);

        let pv = weights.piece_value(PieceType::Pawn);
        let pawn_back = Piece::new(PieceType::Pawn, Color::Black, Position::new(6, 0).unwrap());
        assert!((positional_piece_value(&pawn_back, &weights) - 0.1 * pv).abs() < 1e-2);
    }

    #[test]
    fn file_pst_eligibility_and_symmetry() {
        assert!(!uses_file_pst(PieceType::King));
        assert!(!uses_file_pst(PieceType::Pawn));
        assert!(!uses_file_pst(PieceType::LeftDragon));
        assert!(!uses_file_pst(PieceType::BlueDragon));
        assert!(uses_file_pst(PieceType::GoldGeneral));
        assert!(uses_file_pst(PieceType::Bishop));
        assert!(uses_file_pst(PieceType::FreeKing));
        assert!(uses_file_pst(PieceType::WindDragon));
        assert!(uses_file_pst(PieceType::WhiteTiger));
        assert!(uses_file_pst(PieceType::LeftChariot));
        assert!(uses_file_pst(PieceType::LeftMountainEagle));
        assert!(uses_file_pst(PieceType::RightMountainEagle));
        assert!(is_file_pst_asymmetric(PieceType::LeftTiger));
        assert!(!is_file_pst_asymmetric(PieceType::WindDragon));
        assert!(!is_file_pst_asymmetric(PieceType::LeftMountainEagle));

        let f = seed_file_factors(0.7, 1.5);
        assert!((f[0] - 0.7).abs() < 1e-5);
        assert!((f[35] - 0.7).abs() < 1e-5);
        let mid_t = 0.5f32 / 17.5;
        let mid_expect = 1.5 + mid_t * (0.7 - 1.5);
        assert!((f[17] - mid_expect).abs() < 1e-5);
        assert!((f[18] - mid_expect).abs() < 1e-5);
        for i in 0..36 {
            assert!((f[i] - f[35 - i]).abs() < 1e-6, "asymmetry at {i}");
        }
    }

    #[test]
    fn file_pst_multiplies_positional() {
        let mut weights = EvalWeights::seed();
        weights.file_factor = seed_file_factors(0.5, 2.0).to_vec();
        let edge = Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(0, RANK_PAWN_START).unwrap(),
        );
        let near_center = Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(18, RANK_PAWN_START).unwrap(),
        );
        let base = weights.piece_value(PieceType::GoldGeneral);
        let rank_f = weights.rank_factor_slow[RANK_PAWN_START as usize];
        assert!((positional_piece_value(&edge, &weights) - base * rank_f * 0.5).abs() < 1e-2);
        let expect_c = base * rank_f * weights.file_factor[18];
        assert!((positional_piece_value(&near_center, &weights) - expect_c).abs() < 1e-2);
        assert!(weights.file_factor[18] > 1.9);
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
        assert_eq!(loaded.weights.piece_value(PieceType::King), 100.0);
        assert_eq!(
            loaded.weights.piece_value(PieceType::GreatGeneral),
            8.0 * TARIFF_RANGE_CAPTURING * SEED_CAPTURER_SCALE
        );
        assert_eq!(loaded.weights.royal_bonus(2), 100);
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
        // Two royals → +100 royal bonus vs one (0), plus CP material 8.
        assert!(score > 100, "black with two royals vs one should be positive, got {score}");
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

        // Tengu seed (with range-two-mover buff) → uncapped/5, capped 20.
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

    fn quiet_weights() -> EvalWeights {
        let mut w = EvalWeights::seed();
        w.noise_scale = 0.0;
        w
    }

    /// Flatten PST — used only where we want to isolate tropism arithmetic.
    fn flat_pst_weights() -> EvalWeights {
        let mut w = quiet_weights();
        w.rank_factor_fast = vec![1.0; 36];
        w.rank_factor_slow = vec![1.0; 36];
        w.rank_factor = vec![1.0; 36];
        w
    }

    #[test]
    fn tropism_class_scales_short_range_capturing() {
        assert!((tropism_class_scale(PieceType::GoldGeneral) - 1.0).abs() < 1e-6);
        assert!((tropism_class_scale(PieceType::Pawn) - 1.0).abs() < 1e-6);
        assert!((tropism_class_scale(PieceType::FreeKing) - DEFAULT_EG_TROPISM_RANGE_SCALE).abs() < 1e-6);
        assert!((tropism_class_scale(PieceType::GreatGeneral) - 0.0).abs() < 1e-6);
        assert!((tropism_class_scale(PieceType::King) - 0.0).abs() < 1e-6);
        assert!((tropism_class_scale(PieceType::CrownPrince) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn eg_tropism_ignores_own_royals() {
        let weights = flat_pst_weights();
        let white_king = Position::new(20, 20).unwrap();
        let mut far = Board::new();
        far.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        far.place_piece(Piece::new(PieceType::King, Color::White, white_king));
        far.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        let mut near = far.clone();
        // Walk own king next to enemy royal — must not create tropism.
        near.move_piece(Position::new(10, 10).unwrap(), Position::new(20, 21).unwrap());

        let t_far = eg_tropism_term_black(
            far.pieces_by_color(Color::Black),
            far.pieces_by_color(Color::White),
            &weights,
        );
        let t_near = eg_tropism_term_black(
            near.pieces_by_color(Color::Black),
            near.pieces_by_color(Color::White),
            &weights,
        );
        assert!(
            (t_far - t_near).abs() < 1e-4,
            "own royal proximity must not affect tropism: far={t_far} near={t_near}"
        );
    }

    fn lone_royal_boards() -> (Board, Board) {
        let white_king = Position::new(20, 20).unwrap();
        let mut near = Board::new();
        near.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(5, 5).unwrap(),
        ));
        near.place_piece(Piece::new(PieceType::King, Color::White, white_king));
        for (f, r) in [(19, 19), (21, 19), (19, 21), (18, 20)] {
            near.place_piece(Piece::new(
                PieceType::GoldGeneral,
                Color::Black,
                Position::new(f, r).unwrap(),
            ));
        }

        let mut far = Board::new();
        far.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(5, 5).unwrap(),
        ));
        far.place_piece(Piece::new(PieceType::King, Color::White, white_king));
        for (f, r) in [(2, 33), (3, 33), (2, 34), (3, 34)] {
            far.place_piece(Piece::new(
                PieceType::GoldGeneral,
                Color::Black,
                Position::new(f, r).unwrap(),
            ));
        }
        (near, far)
    }

    #[test]
    fn eg_tropism_prefers_cluster_near_lone_royal() {
        let weights = flat_pst_weights();
        let (near, far) = lone_royal_boards();
        let t_near = eg_tropism_term_black(
            near.pieces_by_color(Color::Black),
            near.pieces_by_color(Color::White),
            &weights,
        );
        let t_far = eg_tropism_term_black(
            far.pieces_by_color(Color::Black),
            far.pieces_by_color(Color::White),
            &weights,
        );
        assert!(
            t_near > t_far + 0.01,
            "near tropism should beat far: near={t_near} far={t_far}"
        );
        let s_near = evaluate_absolute_black(&near, &weights, 0);
        let s_far = evaluate_absolute_black(&far, &weights, 0);
        assert!(
            s_near > s_far,
            "near cluster should outscore far park: near={s_near} far={s_far}"
        );
    }

    #[test]
    fn phase_blend_near_beats_promo_park_with_real_pst() {
        // Real seed PST: promo park gets +20% on golds. Phase blend must still prefer
        // approaching the lone royal (PST fades when w→1).
        let weights = quiet_weights();
        let (near, far) = lone_royal_boards();

        let pst_near = pst_excess_of(near.pieces_by_color(Color::Black), &weights);
        let pst_far = pst_excess_of(far.pieces_by_color(Color::Black), &weights);
        assert!(
            pst_far > pst_near + 1.0,
            "sanity: far promo park has more PST excess: near={pst_near} far={pst_far}"
        );

        let s_near = evaluate_absolute_black(&near, &weights, 0);
        let s_far = evaluate_absolute_black(&far, &weights, 0);
        assert!(
            s_near > s_far,
            "with real PST, near royal must beat promo park under gate: near={s_near} far={s_far}"
        );
    }

    #[test]
    fn eg_tropism_at_d_ref_is_near_zero_when_gated_on() {
        let weights = flat_pst_weights();
        let white_king = Position::new(18, 18).unwrap();
        let mut board = Board::new();
        board.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        board.place_piece(Piece::new(PieceType::King, Color::White, white_king));
        board.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(0, 18).unwrap(),
        ));
        // Stay ahead of eg_ahead_min without affecting tropism (capturing-range s=0).
        board.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::Black,
            Position::new(1, 1).unwrap(),
        ));

        let term = eg_tropism_term_black(
            board.pieces_by_color(Color::Black),
            board.pieces_by_color(Color::White),
            &weights,
        );
        assert!(
            term.abs() < 1e-4,
            "piece at d_ref should give ~0 linear tropism, got {term}"
        );

        let blended = eg_blended_positional_black(
            board.pieces_by_color(Color::Black),
            board.pieces_by_color(Color::White),
            &weights,
        );
        assert!(
            blended.abs() < 1e-4,
            "gate-on at d_ref should not grant free positional lunch, got {blended}"
        );
    }

    #[test]
    fn eg_tropism_opening_density_gate_near_zero() {
        let weights = quiet_weights();
        let mut state = GameState::new();
        state.setup_initial_position();
        let board = state.get_board();
        let black = board.pieces_by_color(Color::Black);
        let white = board.pieces_by_color(Color::White);
        assert!(count_non_royals(white) >= weights.eg_density_n as usize);
        let w = eg_density_weight(count_non_royals(white), weights.eg_density_n);
        assert!(w <= 0.0, "opening should have density weight 0, got {w}");
        let term = eg_tropism_term_black(black, white, &weights);
        assert!(
            term.abs() < 1e-6,
            "opening tropism term should be ~0, got {term}"
        );
        let blended = eg_blended_positional_black(black, white, &weights);
        let pst = pst_excess_of(black, &weights) - pst_excess_of(white, &weights);
        assert!(
            (blended - pst).abs() < 1e-3,
            "opening blend should equal PST excess: blended={blended} pst={pst}"
        );
    }

    #[test]
    fn eg_tropism_one_short_step_about_sigma() {
        let mut weights = flat_pst_weights();
        weights.eg_tropism_topk = 8;
        let white_king = Position::new(20, 20).unwrap();
        let mut far = Board::new();
        far.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        far.place_piece(Piece::new(PieceType::King, Color::White, white_king));
        far.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::Black,
            Position::new(1, 1).unwrap(),
        ));
        far.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(20, 26).unwrap(), // d=6
        ));
        let mut near = far.clone();
        near.move_piece(Position::new(20, 26).unwrap(), Position::new(20, 25).unwrap()); // d=5

        let t_far = eg_tropism_term_black(
            far.pieces_by_color(Color::Black),
            far.pieces_by_color(Color::White),
            &weights,
        );
        let t_near = eg_tropism_term_black(
            near.pieces_by_color(Color::Black),
            near.pieces_by_color(Color::White),
            &weights,
        );
        let delta = t_near - t_far;
        assert!(
            (delta - weights.eg_tropism_scale).abs() < 0.05,
            "one short top-k step should be ≈σ={}: delta={delta}",
            weights.eg_tropism_scale
        );
    }

    #[test]
    fn eg_tropism_tail_step_uses_tail_scale() {
        let mut weights = flat_pst_weights();
        weights.eg_tropism_topk = 2;
        let white_king = Position::new(20, 20).unwrap();
        let mut far = Board::new();
        far.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        far.place_piece(Piece::new(PieceType::King, Color::White, white_king));
        far.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::Black,
            Position::new(1, 1).unwrap(),
        ));
        // Two close (top-k) and one far (tail).
        far.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(20, 22).unwrap(), // d=2
        ));
        far.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(21, 22).unwrap(), // d=2
        ));
        far.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(20, 30).unwrap(), // d=10 tail
        ));
        let mut closer = far.clone();
        closer.move_piece(Position::new(20, 30).unwrap(), Position::new(20, 29).unwrap()); // d=9

        let t0 = eg_tropism_term_black(
            far.pieces_by_color(Color::Black),
            far.pieces_by_color(Color::White),
            &weights,
        );
        let t1 = eg_tropism_term_black(
            closer.pieces_by_color(Color::Black),
            closer.pieces_by_color(Color::White),
            &weights,
        );
        let delta = t1 - t0;
        assert!(
            (delta - weights.eg_tropism_tail_scale).abs() < 0.05,
            "tail step should be ≈σ_tail={}: delta={delta}",
            weights.eg_tropism_tail_scale
        );
    }

    #[test]
    fn eg_tropism_losing_side_damped() {
        let mut weights = quiet_weights();
        weights.eg_ahead_min = 0.0;
        let mut board = Board::new();
        board.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        board.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(20, 20).unwrap(),
        ));
        board.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::White,
            Position::new(11, 10).unwrap(),
        ));
        board.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::White,
            Position::new(9, 10).unwrap(),
        ));

        let black = board.pieces_by_color(Color::Black);
        let white = board.pieces_by_color(Color::White);
        let black_mat = raw_material_of(black, &weights);
        let white_mat = raw_material_of(white, &weights);
        assert!(black_mat < white_mat);

        let w_black = side_phase_weight(black_mat, white_mat, count_non_royals(white), &weights);
        assert!(
            w_black.abs() < 1e-6,
            "losing side phase weight should be 0, got {w_black}"
        );
    }

    #[test]
    fn eg_tropism_density_delta_no_free_lunch_at_neutral() {
        let weights = flat_pst_weights();
        let white_king = Position::new(18, 18).unwrap();
        let mut depleted = Board::new();
        depleted.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        depleted.place_piece(Piece::new(PieceType::King, Color::White, white_king));
        depleted.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(0, 18).unwrap(),
        ));
        depleted.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::Black,
            Position::new(1, 1).unwrap(),
        ));

        let mut fat = depleted.clone();
        for i in 0..20 {
            let f = (25 + (i % 5)) as u8;
            let r = (25 + (i / 5)) as u8;
            fat.place_piece(Piece::new(
                PieceType::Pawn,
                Color::White,
                Position::new(f, r).unwrap(),
            ));
        }

        let t_depleted = eg_tropism_term_black(
            depleted.pieces_by_color(Color::Black),
            depleted.pieces_by_color(Color::White),
            &weights,
        );
        let t_fat = eg_tropism_term_black(
            fat.pieces_by_color(Color::Black),
            fat.pieces_by_color(Color::White),
            &weights,
        );
        assert!(
            (t_depleted - t_fat).abs() < 1e-3,
            "gate-on at d_ref must not create tropism windfall: depleted={t_depleted} fat={t_fat}"
        );
    }

    fn kings_and(piece: Piece) -> Board {
        let mut b = Board::new();
        b.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        b.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(35, 35).unwrap(),
        ));
        b.place_piece(piece);
        b
    }

    #[test]
    fn first_leg_count_hook_tengu_not_gold() {
        let open_hook = kings_and(Piece::new(
            PieceType::HookMover,
            Color::Black,
            Position::new(18, 18).unwrap(),
        ));
        let hook = open_hook
            .pieces_by_color(Color::Black)
            .iter()
            .find(|p| p.piece_type == PieceType::HookMover)
            .copied()
            .unwrap();
        let open_n = first_leg_landing_count(&hook, &open_hook);
        assert!(open_n > 10, "open Hook first-leg should be many, got {open_n}");

        let mut boxed = open_hook.clone();
        for (df, dr) in [(0i8, 1), (0, -1), (1, 0), (-1, 0)] {
            let pos = Position::new((18 + df) as u8, (18 + dr) as u8).unwrap();
            boxed.place_piece(Piece::new(PieceType::Pawn, Color::Black, pos));
        }
        let boxed_n = first_leg_landing_count(&hook, &boxed);
        assert!(boxed_n <= 2, "boxed Hook first-leg should be 0–2, got {boxed_n}");

        let tengu_board = kings_and(Piece::new(
            PieceType::Tengu,
            Color::Black,
            Position::new(18, 18).unwrap(),
        ));
        let tengu = tengu_board
            .pieces_by_color(Color::Black)
            .iter()
            .find(|p| p.piece_type == PieceType::Tengu)
            .copied()
            .unwrap();
        assert!(first_leg_landing_count(&tengu, &tengu_board) > 0);

        let gold_board = kings_and(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(18, 18).unwrap(),
        ));
        let gold = gold_board
            .pieces_by_color(Color::Black)
            .iter()
            .find(|p| p.piece_type == PieceType::GoldGeneral)
            .copied()
            .unwrap();
        assert_eq!(first_leg_landing_count(&gold, &gold_board), 0);
    }

    #[test]
    fn two_mover_mob_k0_matches_seed() {
        let board = kings_and(Piece::new(
            PieceType::HookMover,
            Color::Black,
            Position::new(18, 18).unwrap(),
        ));
        let seed = EvalWeights::seed();
        let mut off = seed.clone();
        off.two_mover_mob_k = 0.0;
        off.two_mover_mob_curve = 1;
        off.two_mover_mob_apply = 2;
        assert_eq!(
            evaluate_absolute_black(&board, &seed, 0),
            evaluate_absolute_black(&board, &off, 0)
        );
    }

    #[test]
    fn two_mover_mob_linear_k100_per_landing() {
        let open = kings_and(Piece::new(
            PieceType::HookMover,
            Color::Black,
            Position::new(18, 18).unwrap(),
        ));
        let hook = open
            .pieces_by_color(Color::Black)
            .iter()
            .find(|p| p.piece_type == PieceType::HookMover)
            .copied()
            .unwrap();
        let m_open = first_leg_landing_count(&hook, &open) as i32;
        assert!(m_open > 0);

        let mut off = EvalWeights::seed();
        off.noise_scale = 0.0;
        let mut on = off.clone();
        on.two_mover_mob_k = 100.0;
        on.two_mover_mob_curve = 0;
        on.two_mover_mob_apply = 0;
        let d = evaluate_absolute_black(&open, &on, 0) - evaluate_absolute_black(&open, &off, 0);
        assert_eq!(d, 100 * m_open);
    }
}
