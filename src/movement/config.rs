use crate::movement::types::{MovementCapability, BlockingMode};
use crate::movement::direction::DIRECTION_SET_ALL;
use crate::piece::PieceType;
use std::collections::HashSet;

/// Movement configuration for a piece type
/// Contains a list of movement capabilities that the piece can use
#[derive(Debug, Clone)]
pub struct MovementConfig {
    pub capabilities: Vec<MovementCapability>,
}

/// Helper function to create a Range capability with an empty cannot_jump_over set
/// This is the default for most pieces
fn range_capability(directions: crate::movement::direction::DirectionSet, blocking: BlockingMode) -> MovementCapability {
    MovementCapability::Range {
        directions,
        blocking,
        cannot_jump_over: HashSet::new(),
    }
}

/// Helper function to create a Range capability with a specific cannot_jump_over set
/// Used for pieces like Great General that have restrictions
fn range_capability_with_restrictions(
    directions: crate::movement::direction::DirectionSet,
    blocking: BlockingMode,
    cannot_jump_over: HashSet<PieceType>,
) -> MovementCapability {
    MovementCapability::Range {
        directions,
        blocking,
        cannot_jump_over,
    }
}

/// Blocking Set 1: King, CrownPrince, GreatGeneral
fn blocking_set_1() -> HashSet<PieceType> {
    let mut set = HashSet::new();
    set.insert(PieceType::King);
    set.insert(PieceType::CrownPrince);
    set.insert(PieceType::GreatGeneral);
    set
}

/// Blocking Set 2: Blocking Set 1 + ViceGeneral
fn blocking_set_2() -> HashSet<PieceType> {
    let mut set = blocking_set_1();
    set.insert(PieceType::ViceGeneral);
    set
}

/// Blocking Set 3: Pieces that cannot be jumped over by Flying General, Flying Crocodile, Bishop General, and Rain Demon
/// Includes: King, CrownPrince, GreatGeneral, FlyingGeneral, FlyingCrocodile, BishopGeneral, ViceGeneral, FierceDragon

pub fn blocking_set_3() -> HashSet<PieceType> {
    let mut set = HashSet::new();
    set.insert(PieceType::King);
    set.insert(PieceType::CrownPrince);
    set.insert(PieceType::GreatGeneral);
    set.insert(PieceType::FlyingGeneral);
    set.insert(PieceType::FlyingCrocodile);
    set.insert(PieceType::BishopGeneral);
    set.insert(PieceType::ViceGeneral);
    set.insert(PieceType::FierceDragon);
    set
}

impl MovementConfig {
    pub fn new(capabilities: Vec<MovementCapability>) -> MovementConfig {
        MovementConfig { capabilities }
    }

    /// Get movement config for a piece (handles special cases like promoted vs unpromoted RainDragon and Whale)
    pub fn for_piece(piece: &crate::piece::Piece) -> MovementConfig {
        // Special case: RainDragon has different movement if promoted (from EarthDragon) vs unpromoted (starting piece)
        if piece.piece_type == crate::piece::PieceType::RainDragon {
            if piece.is_promoted {
                // Promoted RainDragon (from EarthDragon): has range backwards movement
                return rain_dragon_promoted_movement();
            } else {
                // Unpromoted RainDragon (starting piece): no range backwards movement
                return rain_dragon_unpromoted_movement();
            }
        }
        
        // Special case: Whale has different movement if promoted (from ReverseChariot) vs unpromoted (starting piece)
        if piece.piece_type == crate::piece::PieceType::Whale {
            if let Some(base_type) = piece.base_piece_type {
                if base_type == crate::piece::PieceType::ReverseChariot {
                    // Promoted Whale (from ReverseChariot): simple 1 forwards, range backwards
                    return whale_promoted_movement();
                }
            }
            // Starting Whale (unpromoted): range forwards and backwards
            return whale_starting_movement();
        }
        
        // For all other pieces, use the standard piece type lookup
        Self::for_piece_type(piece.piece_type)
    }

    /// Get movement config for a piece type
    pub fn for_piece_type(piece_type: crate::piece::PieceType) -> MovementConfig {
        match piece_type {
            crate::piece::PieceType::King => king_movement(),
            crate::piece::PieceType::Pawn => pawn_movement(),
            crate::piece::PieceType::GoldGeneral => gold_general_movement(),
            crate::piece::PieceType::Dog => dog_movement(),
            crate::piece::PieceType::MixedGeneral => mixed_general_movement(),
            crate::piece::PieceType::GoBetween => go_between_movement(),
            crate::piece::PieceType::DrunkenElephant => drunken_elephant_movement(),
            crate::piece::PieceType::CrownPrince => crown_prince_movement(),
            crate::piece::PieceType::NeighboringKing => neighboring_king_movement(),
            crate::piece::PieceType::FrontStandard => front_standard_movement(),
            crate::piece::PieceType::Rook => rook_movement(),
            crate::piece::PieceType::DragonKing => dragon_king_movement(),
            crate::piece::PieceType::CloudEagle => cloud_eagle_movement(),
            crate::piece::PieceType::StrongEagle => strong_eagle_movement(),
            crate::piece::PieceType::StoneChariot => stone_chariot_movement(),
            crate::piece::PieceType::WalkingHeron => walking_heron_movement(),
            crate::piece::PieceType::Bishop => bishop_movement(),
            crate::piece::PieceType::DragonHorse => dragon_horse_movement(),
            crate::piece::PieceType::GreatTurtle => great_turtle_movement(),
            crate::piece::PieceType::SpiritTurtle => spirit_turtle_movement(),
            crate::piece::PieceType::LittleTurtle => little_turtle_movement(),
            crate::piece::PieceType::TreasureTurtle => treasure_turtle_movement(),
            crate::piece::PieceType::Capricorn => capricorn_movement(),
            crate::piece::PieceType::HookMover => hook_mover_movement(),
            crate::piece::PieceType::Kirin => kirin_movement(),
            crate::piece::PieceType::Phoenix => phoenix_movement(),
            crate::piece::PieceType::FireGeneral => fire_general_movement(),
            crate::piece::PieceType::WaterGeneral => water_general_movement(),
            crate::piece::PieceType::BlindDog => blind_dog_movement(),
            crate::piece::PieceType::FierceStag => fierce_stag_movement(),
            crate::piece::PieceType::MovingBoar => moving_boar_movement(),
            crate::piece::PieceType::CrowMover => crow_mover_movement(),
            crate::piece::PieceType::FlyingHawk => flying_hawk_movement(),
            crate::piece::PieceType::FlyingGoose => flying_goose_movement(),
            crate::piece::PieceType::SwallowsWings => swallows_wings_movement(),
            crate::piece::PieceType::SwallowMover => swallow_mover_movement(),
            crate::piece::PieceType::CatSword => cat_sword_movement(),
            crate::piece::PieceType::ClimbingMonkey => climbing_monkey_movement(),
            crate::piece::PieceType::OwlMover => owl_mover_movement(),
            crate::piece::PieceType::Horseman => horseman_movement(),
            crate::piece::PieceType::Tanuki => tanuki_movement(),
            crate::piece::PieceType::EarthChariot => earth_chariot_movement(),
            crate::piece::PieceType::ReedBird => reed_bird_movement(),
            crate::piece::PieceType::GreatMaster => great_master_movement(),
            crate::piece::PieceType::GreatStandard => great_standard_movement(),
            crate::piece::PieceType::IronGeneral => iron_general_movement(),
            crate::piece::PieceType::RunningOx => running_ox_movement(),
            crate::piece::PieceType::BearSoldier => bear_soldier_movement(),
            crate::piece::PieceType::StrongBear => strong_bear_movement(),
            crate::piece::PieceType::TileGeneral => tile_general_movement(),
            crate::piece::PieceType::LeopardSoldier => leopard_soldier_movement(),
            crate::piece::PieceType::RunningLeopard => running_leopard_movement(),
            crate::piece::PieceType::StoneGeneral => stone_general_movement(),
            crate::piece::PieceType::BoarSoldier => boar_soldier_movement(),
            crate::piece::PieceType::RunningBoar => running_boar_movement(),
            crate::piece::PieceType::EarthGeneral => earth_general_movement(),
            crate::piece::PieceType::OxSoldier => ox_soldier_movement(),
            crate::piece::PieceType::WoodGeneral => wood_general_movement(),
            crate::piece::PieceType::HorseSoldier => horse_soldier_movement(),
            crate::piece::PieceType::MountainGeneral => mountain_general_movement(),
            crate::piece::PieceType::MountTai => mount_tai_movement(),
            crate::piece::PieceType::RiverGeneral => river_general_movement(),
            crate::piece::PieceType::HuaiRiver => huai_river_movement(),
            crate::piece::PieceType::WindGeneral => wind_general_movement(),
            crate::piece::PieceType::FierceWind => fierce_wind_movement(),
            crate::piece::PieceType::VerticalSoldier => vertical_soldier_movement(),
            crate::piece::PieceType::ChariotSoldier => chariot_soldier_movement(),
            crate::piece::PieceType::SideMonkey => side_monkey_movement(),
            crate::piece::PieceType::LeftChariot => left_chariot_movement(),
            crate::piece::PieceType::LeftIronChariot => left_iron_chariot_movement(),
            crate::piece::PieceType::RightChariot => right_chariot_movement(),
            crate::piece::PieceType::RightIronChariot => right_iron_chariot_movement(),
            crate::piece::PieceType::SideGeneral => side_general_movement(),
            crate::piece::PieceType::Shitennou => shitennou_movement(),
            crate::piece::PieceType::GreatElephant => great_elephant_movement(),
            crate::piece::PieceType::RoaringDog => roaring_dog_movement(),
            crate::piece::PieceType::CrossbowSoldier => crossbow_soldier_movement(),
            crate::piece::PieceType::CrossbowGeneral => crossbow_general_movement(),
            crate::piece::PieceType::VerticalHorse => vertical_horse_movement(),
            crate::piece::PieceType::VerticalPup => vertical_pup_movement(),
            crate::piece::PieceType::LeopardKing => leopard_king_movement(),
            crate::piece::PieceType::LongbowSoldier => longbow_soldier_movement(),
            crate::piece::PieceType::LongbowGeneral => longbow_general_movement(),
            crate::piece::PieceType::CannonSoldier => cannon_soldier_movement(),
            crate::piece::PieceType::CannonGeneral => cannon_general_movement(),
            crate::piece::PieceType::FierceTiger => fierce_tiger_movement(),
            crate::piece::PieceType::GreatTiger => great_tiger_movement(),
            crate::piece::PieceType::VerticalLeopard => vertical_leopard_movement(),
            crate::piece::PieceType::GreatLeopard => great_leopard_movement(),
            crate::piece::PieceType::SpearSoldier => spear_soldier_movement(),
            crate::piece::PieceType::SpearGeneral => spear_general_movement(),
            crate::piece::PieceType::SwordSoldier => sword_soldier_movement(),
            crate::piece::PieceType::SwordGeneral => sword_general_movement(),
            crate::piece::PieceType::PoisonousSerpent => poisonous_serpent_movement(),
            crate::piece::PieceType::FlyingDragon => flying_dragon_movement(),
            crate::piece::PieceType::FierceEagle => fierce_eagle_movement(),
            crate::piece::PieceType::FlyingEagle => flying_eagle_movement(),
            crate::piece::PieceType::GreatEagle => great_eagle_movement(),
            crate::piece::PieceType::FreeEagle => free_eagle_movement(),
            crate::piece::PieceType::HornedHawk => horned_hawk_movement(),
            crate::piece::PieceType::GreatHawk => great_hawk_movement(),
            crate::piece::PieceType::FierceLeopard => fierce_leopard_movement(),
            crate::piece::PieceType::WaterOx => water_ox_movement(),
            crate::piece::PieceType::GreatBaku => great_baku_movement(),
            crate::piece::PieceType::DancingStag => dancing_stag_movement(),
            crate::piece::PieceType::SquareMover => square_mover_movement(),
            crate::piece::PieceType::StrongChariot => strong_chariot_movement(),
            crate::piece::PieceType::OldRat => old_rat_movement(),
            crate::piece::PieceType::JiBird => ji_bird_movement(),
            crate::piece::PieceType::BlindBear => blind_bear_movement(),
            crate::piece::PieceType::FlyingStag => flying_stag_movement(),
            crate::piece::PieceType::SideFlyer => side_flyer_movement(),
            crate::piece::PieceType::OxChariot => ox_chariot_movement(),
            crate::piece::PieceType::PloddingOx => plodding_ox_movement(),
            crate::piece::PieceType::BlindTiger => blind_tiger_movement(),
            crate::piece::PieceType::BlindMonkey => blind_monkey_movement(),
            crate::piece::PieceType::SideMover => side_mover_movement(),
            crate::piece::PieceType::LeftHowlingDog => left_howling_dog_movement(),
            crate::piece::PieceType::RightHowlingDog => right_howling_dog_movement(),
            crate::piece::PieceType::LeftDog => left_dog_movement(),
            crate::piece::PieceType::RightDog => right_dog_movement(),
            crate::piece::PieceType::GreatFoal => great_foal_movement(),
            crate::piece::PieceType::WoodChariot => wood_chariot_movement(),
            crate::piece::PieceType::WindSnappingTurtle => wind_snapping_turtle_movement(),
            crate::piece::PieceType::PengMaster => peng_master_movement(),
            crate::piece::PieceType::CenterMaster => center_master_movement(),
            crate::piece::PieceType::FierceWolf => fierce_wolf_movement(),
            crate::piece::PieceType::BearsEyes => bears_eyes_movement(),
            crate::piece::PieceType::EasternBarbarian => eastern_barbarian_movement(),
            crate::piece::PieceType::WesternBarbarian => western_barbarian_movement(),
            crate::piece::PieceType::LionDog => lion_dog_movement(),
            crate::piece::PieceType::SouthernBarbarian => southern_barbarian_movement(),
            crate::piece::PieceType::NorthernBarbarian => northern_barbarian_movement(),
            crate::piece::PieceType::LionHawk => lion_hawk_movement(),
            crate::piece::PieceType::RecliningDragon => reclining_dragon_movement(),
            crate::piece::PieceType::CoiledSerpent => coiled_serpent_movement(),
            crate::piece::PieceType::CoiledDragon => coiled_dragon_movement(),
            crate::piece::PieceType::HuaiChicken => huai_chicken_movement(),
            crate::piece::PieceType::WizardStork => wizard_stork_movement(),
            crate::piece::PieceType::OldMonkey => old_monkey_movement(),
            crate::piece::PieceType::MountainWitch => mountain_witch_movement(),
            crate::piece::PieceType::FlyingChicken => flying_chicken_movement(),
            crate::piece::PieceType::RaidingHawk => raiding_hawk_movement(),
            crate::piece::PieceType::WindHorse => wind_horse_movement(),
            crate::piece::PieceType::HeavenlyHorse => heavenly_horse_movement(),
            crate::piece::PieceType::EvilWolf => evil_wolf_movement(),
            crate::piece::PieceType::PoisonousWolf => poisonous_wolf_movement(),
            crate::piece::PieceType::AngryBoar => angry_boar_movement(),
            crate::piece::PieceType::FierceBear => fierce_bear_movement(),
            crate::piece::PieceType::GreatBear => great_bear_movement(),
            crate::piece::PieceType::FlyingHorse => flying_horse_movement(),
            crate::piece::PieceType::Donkey => donkey_movement(),
            crate::piece::PieceType::SideOx => side_ox_movement(),
            crate::piece::PieceType::VerticalWolf => vertical_wolf_movement(),
            crate::piece::PieceType::TileChariot => tile_chariot_movement(),
            crate::piece::PieceType::RunningTile => running_tile_movement(),
            crate::piece::PieceType::LeftGeneral => left_general_movement(),
            crate::piece::PieceType::RightGeneral => right_general_movement(),
            crate::piece::PieceType::LeftArmy => left_army_movement(),
            crate::piece::PieceType::RightArmy => right_army_movement(),
            crate::piece::PieceType::RearStandard => rear_standard_movement(),
            crate::piece::PieceType::CenterStandard => center_standard_movement(),
            crate::piece::PieceType::FreeKing => free_king_movement(),
            crate::piece::PieceType::GreatGeneral => great_general_movement(),
            crate::piece::PieceType::FreeBaku => free_baku_movement(),
            crate::piece::PieceType::FreeDemon => free_demon_movement(),
            crate::piece::PieceType::RunningHorse => running_horse_movement(),
            crate::piece::PieceType::Tengu => tengu_movement(),
            crate::piece::PieceType::WoodenDove => wooden_dove_movement(),
            crate::piece::PieceType::CeramicDove => ceramic_dove_movement(),
            crate::piece::PieceType::EarthDragon => earth_dragon_movement(),
            crate::piece::PieceType::RainDragon => rain_dragon_unpromoted_movement(),  // Default to unpromoted, but for_piece() handles the promoted case
            crate::piece::PieceType::LeftMountainEagle => left_mountain_eagle_movement(),
            crate::piece::PieceType::RightMountainEagle => right_mountain_eagle_movement(),
            crate::piece::PieceType::FireDemon => fire_demon_movement(),
            crate::piece::PieceType::FreeFire => free_fire_movement(),
            crate::piece::PieceType::Whale => whale_starting_movement(),  // Default to starting, but for_piece() handles the promoted case
            crate::piece::PieceType::ReverseChariot => reverse_chariot_movement(),
            crate::piece::PieceType::LeftDragon => left_dragon_movement(),
            crate::piece::PieceType::RightDragon => right_dragon_movement(),
            crate::piece::PieceType::VermillionSparrow => vermillion_sparrow_movement(),
            crate::piece::PieceType::DivineSparrow => divine_sparrow_movement(),
            crate::piece::PieceType::BlueDragon => blue_dragon_movement(),
            crate::piece::PieceType::DivineDragon => divine_dragon_movement(),
            crate::piece::PieceType::LeftTiger => left_tiger_movement(),
            crate::piece::PieceType::RightTiger => right_tiger_movement(),
            crate::piece::PieceType::FlyingGeneral => flying_general_movement(),
            crate::piece::PieceType::FlyingCrocodile => flying_crocodile_movement(),
            crate::piece::PieceType::BishopGeneral => bishop_general_movement(),
            crate::piece::PieceType::RainDemon => rain_demon_movement(),
            crate::piece::PieceType::KirinMaster => kirin_master_movement(),
            crate::piece::PieceType::PhoenixMaster => phoenix_master_movement(),
            crate::piece::PieceType::CopperGeneral => copper_general_movement(),
            crate::piece::PieceType::HorizontalMover => horizontal_mover_movement(),
            crate::piece::PieceType::FireDragon => fire_dragon_movement(),
            crate::piece::PieceType::WaterDragon => water_dragon_movement(),
            crate::piece::PieceType::Peacock => peacock_movement(),
            crate::piece::PieceType::OldKite => old_kite_movement(),
            crate::piece::PieceType::RushingBird => rushing_bird_movement(),
            crate::piece::PieceType::FreePup => free_pup_movement(),
            crate::piece::PieceType::FreeDog => free_dog_movement(),
            crate::piece::PieceType::WindDragon => wind_dragon_movement(),
            crate::piece::PieceType::FreeDragon => free_dragon_movement(),
            crate::piece::PieceType::RunningWolf => running_wolf_movement(),
            crate::piece::PieceType::FreeWolf => free_wolf_movement(),
            crate::piece::PieceType::RunningStag => running_stag_movement(),
            crate::piece::PieceType::FreeStag => free_stag_movement(),
            crate::piece::PieceType::SideDragon => side_dragon_movement(),
            crate::piece::PieceType::RunningDragon => running_dragon_movement(),
            crate::piece::PieceType::GoldenChariot => golden_chariot_movement(),
            crate::piece::PieceType::PlayfulParrot => playful_parrot_movement(),
            crate::piece::PieceType::ViceGeneral => vice_general_movement(),
            crate::piece::PieceType::WoodlandDemon => woodland_demon_movement(),
            crate::piece::PieceType::OldPeng => old_peng_movement(),
            crate::piece::PieceType::FierceDragon => fierce_dragon_movement(),
            crate::piece::PieceType::FowlCadet => fowl_cadet_movement(),
            crate::piece::PieceType::Lion => lion_movement(),
            crate::piece::PieceType::FuriousFiend => furious_fiend_movement(),
            crate::piece::PieceType::GoldStag => gold_stag_movement(),
            crate::piece::PieceType::SilverRabbit => silver_rabbit_movement(),
            crate::piece::PieceType::SideBoar => side_boar_movement(),
            crate::piece::PieceType::FreeBoar => free_boar_movement(),
            crate::piece::PieceType::OxGeneral => ox_general_movement(),
            crate::piece::PieceType::FreeOx => free_ox_movement(),
            crate::piece::PieceType::HorseGeneral => horse_general_movement(),
            crate::piece::PieceType::FreeHorse => free_horse_movement(),
            crate::piece::PieceType::PupGeneral => pup_general_movement(),
            crate::piece::PieceType::ChickenGeneral => chicken_general_movement(),
            crate::piece::PieceType::FreeChicken => free_chicken_movement(),
            crate::piece::PieceType::PigGeneral => pig_general_movement(),
            crate::piece::PieceType::FreePig => free_pig_movement(),
            crate::piece::PieceType::Knight => knight_movement(),
            crate::piece::PieceType::SideSoldier => side_soldier_movement(),
            crate::piece::PieceType::VerticalBear => vertical_bear_movement(),
            crate::piece::PieceType::SilverChariot => silver_chariot_movement(),
            crate::piece::PieceType::GooseWing => goose_wing_movement(),
            crate::piece::PieceType::Daiba => daiba_movement(),
            crate::piece::PieceType::KingOfTeachings => king_of_teachings_movement(),
            crate::piece::PieceType::DarkSpirit => dark_spirit_movement(),
            crate::piece::PieceType::BuddhistSpirit => buddhist_spirit_movement(),
            crate::piece::PieceType::GoldBird => gold_bird_movement(),
            crate::piece::PieceType::FreeBird => free_bird_movement(),
            crate::piece::PieceType::FierceOx => fierce_ox_movement(),
            crate::piece::PieceType::FlyingOx => flying_ox_movement(),
            crate::piece::PieceType::FireOx => fire_ox_movement(),
            crate::piece::PieceType::SheepSoldier => sheep_soldier_movement(),
            crate::piece::PieceType::TigerSoldier => tiger_soldier_movement(),
            crate::piece::PieceType::RunningChariot => running_chariot_movement(),
            crate::piece::PieceType::CannonChariot => cannon_chariot_movement(),
            crate::piece::PieceType::CopperChariot => copper_chariot_movement(),
            crate::piece::PieceType::CopperElephant => copper_elephant_movement(),
            crate::piece::PieceType::CloudDragon => cloud_dragon_movement(),
            crate::piece::PieceType::LittleStandard => little_standard_movement(),
            crate::piece::PieceType::Soldier => soldier_movement(),
            crate::piece::PieceType::Cavalier => cavalier_movement(),
            crate::piece::PieceType::VerticalTiger => vertical_tiger_movement(),
            crate::piece::PieceType::MountainHawk => mountain_hawk_movement(),
            crate::piece::PieceType::FlyingCat => flying_cat_movement(),
            crate::piece::PieceType::SideWolf => side_wolf_movement(),
            crate::piece::PieceType::GreatWhale => great_whale_movement(),
            crate::piece::PieceType::RunningRabbit => running_rabbit_movement(),
            crate::piece::PieceType::TreacherousFox => treacherous_fox_movement(),
            crate::piece::PieceType::MountainCrane => mountain_crane_movement(),
            crate::piece::PieceType::TurtleSnake => turtle_snake_movement(),
            crate::piece::PieceType::DivineTurtle => divine_turtle_movement(),
            crate::piece::PieceType::WhiteTiger => white_tiger_movement(),
            crate::piece::PieceType::DivineTiger => divine_tiger_movement(),
            crate::piece::PieceType::Lance => lance_movement(),
            crate::piece::PieceType::WhiteFoal => white_foal_movement(),
            crate::piece::PieceType::BeastCadet => beast_cadet_movement(),
            crate::piece::PieceType::BeastOfficer => beast_officer_movement(),
            crate::piece::PieceType::BeastBird => beast_bird_movement(),
            crate::piece::PieceType::FlyingSwallow => flying_swallow_movement(),
            crate::piece::PieceType::GreatDragon => great_dragon_movement(),
            crate::piece::PieceType::PrimordialDragon => primordial_dragon_movement(),
            crate::piece::PieceType::MountainStag => mountain_stag_movement(),
            crate::piece::PieceType::GreatStag => great_stag_movement(),
            crate::piece::PieceType::SilverGeneral => silver_general_movement(),
            crate::piece::PieceType::VerticalMover => vertical_mover_movement(),
            crate::piece::PieceType::Rikishi => rikishi_movement(),
            crate::piece::PieceType::Kongou => kongou_movement(),
            crate::piece::PieceType::Rasetsu => rasetsu_movement(),
            crate::piece::PieceType::Yasha => yasha_movement(),
            crate::piece::PieceType::Shiten => shiten_movement(),
            crate::piece::PieceType::RunningBear => running_bear_movement(),
            crate::piece::PieceType::FreeBear => free_bear_movement(),
            crate::piece::PieceType::RunningTiger => running_bear_movement(),  // Same movement as running bear
            crate::piece::PieceType::FreeTiger => free_tiger_movement(),
            crate::piece::PieceType::GreatDove => great_dove_movement(),
            crate::piece::PieceType::SideSerpent => side_serpent_movement(),
            crate::piece::PieceType::GreatShark => great_shark_movement(),
            crate::piece::PieceType::RunningSerpent => running_serpent_movement(),
            crate::piece::PieceType::FreeSerpent => free_serpent_movement(),
            crate::piece::PieceType::RunningPup => running_serpent_movement(),  // Same movement as running serpent
            crate::piece::PieceType::FreeLeopard => free_leopard_movement(),
            crate::piece::PieceType::ForestDemon => forest_demon_movement(),
            crate::piece::PieceType::ThunderRunner => thunder_runner_movement(),
            crate::piece::PieceType::FowlOfficer => fowl_officer_movement(),
            crate::piece::PieceType::Fowl => fowl_movement(),
            crate::piece::PieceType::Turtledove => turtledove_movement(),
            crate::piece::PieceType::WhiteElephant => white_elephant_movement(),
            crate::piece::PieceType::FragrantElephant => white_elephant_movement(),  // Same movement as white elephant
            crate::piece::PieceType::ElephantKing => elephant_king_movement(),
        }
    }
}

// Predefined movement configs for existing pieces

/// King movement: Simple movement, all 8 directions, max 2 squares
pub fn king_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            MovementCapability::Simple {
                directions: DIRECTION_SET_ALL,
                max_distance: 2,
            },
        ],
    }
}

/// Pawn movement: Forward only (1 square)
/// Pawns move and capture only directly forwards
/// Directions are relative to piece color (forward = N for Black, S for White)
/// The generator will handle color-aware direction adjustment
pub fn pawn_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            // Forward move only (N for Black, will be flipped to S for White)
            MovementCapability::Simple {
                directions: 0x01, // N (bit 0 = 1)
                max_distance: 1,
            },
        ],
    }
}

/// Gold General movement: 6 directions (forward, diagonally forward, sideways, backward orthogonal)
/// Cannot move diagonally backward
/// Directions are relative to piece color
/// For Black: N, S, E, W, NE, NW (forward, backward, sideways, forward diagonals)
/// For White: flipped (S, N, E, W, SE, SW)
pub fn gold_general_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            MovementCapability::Simple {
                // N, S, E, W, NE, NW (will be adjusted by color)
                // N (1) | S (16) | E (4) | W (64) | NE (2) | NW (128) = 215 = 0xD7
                directions: 0xD7,
                max_distance: 1,
            },
        ],
    }
}

/// Dog movement: Forward (1 square) and forward diagonals (1 square each)
/// Directions are relative to piece color
/// For Black: N (forward), NE, NW (forward diagonals)
/// For White: flipped (S, SE, SW)
pub fn dog_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            // Forward and forward diagonals (N, NE, NW for Black, will be flipped to S, SE, SW for White)
            MovementCapability::Simple {
                // N (1) | NE (2) | NW (128) = 131 = 0x83
                directions: 0x83,
                max_distance: 1,
            },
        ],
    }
}

/// Mixed General movement: No-jump range movement in all 3 forward directions plus straight backwards
/// Directions are relative to piece color
/// For Black: N, NE, NW (forward directions), S (backward)
/// For White: flipped (S, SE, SW, N)
pub fn mixed_general_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            range_capability(0x93, BlockingMode::NoJump),  // N, NE, NW, S (will be adjusted by color)
        ],
    }
}

/// Go-between movement: Forward and backward (1 space each)
/// Directions are relative to piece color
/// For Black: N (forward), S (backward)
/// For White: flipped (S, N)
pub fn go_between_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            MovementCapability::Simple {
                // N, S (will be adjusted by color)
                // N (1) | S (16) = 17 = 0x11
                directions: 0x11,
                max_distance: 1,
            },
        ],
    }
}

/// Drunken Elephant movement: All directions except directly backward (1 space)
/// Directions are relative to piece color
/// For Black: N, NE, E, SE, SW, W, NW (all except S)
/// For White: flipped (S, SE, E, NE, NW, W, SW - all except N)
pub fn drunken_elephant_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            MovementCapability::Simple {
                // All directions except S (will be adjusted by color)
                // N (1) | NE (2) | E (4) | SE (8) | SW (32) | W (64) | NW (128) = 239 = 0xEF
                // Or: 0xFF - 0x10 (S) = 0xEF
                directions: 0xEF,
                max_distance: 1,
            },
        ],
    }
}

/// Crown Prince movement: 1 space in all 8 directions
/// Same as standard king in chess/shogi
pub fn crown_prince_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            MovementCapability::Simple {
                directions: DIRECTION_SET_ALL,
                max_distance: 1,
            },
        ],
    }
}

/// Neighboring King movement: Same as Drunken Elephant (all directions except backward, 1 space)
pub fn neighboring_king_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            MovementCapability::Simple {
                // All directions except S (will be adjusted by color)
                // N (1) | NE (2) | E (4) | SE (8) | SW (32) | W (64) | NW (128) = 239 = 0xEF
                directions: 0xEF,
                max_distance: 1,
            },
        ],
    }
}

/// Front Standard movement: Unlimited range in orthogonal directions, up to 3 spaces in diagonal directions
pub fn front_standard_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            // Unlimited range in orthogonal directions (N, S, E, W)
            range_capability(
                // N (1) | S (16) | E (4) | W (64) = 85 = 0x55
                0x55,
                BlockingMode::NoJump,
            ),
            // Up to 3 spaces in diagonal directions (NE, SE, SW, NW)
            MovementCapability::Simple {
                // NE (2) | SE (8) | SW (32) | NW (128) = 170 = 0xAA
                directions: 0xAA,
                max_distance: 3,
            },
        ],
    }
}

/// Rook movement: Unlimited range in orthogonal directions (N, S, E, W)
pub fn rook_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            range_capability(
                // N (1) | S (16) | E (4) | W (64) = 85 = 0x55
                0x55,
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Bishop movement: Range diagonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Diagonally: All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
pub fn bishop_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    
    MovementConfig {
        capabilities: vec![
            range_capability(
                DIRECTION_SET_DIAGONAL,  // All diagonals (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Dragon Horse movement: Range diagonally, simple 1 orthogonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Diagonally: All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
/// Orthogonally: N (1), E (4), S (16), W (64) = 85 = 0x55 (same for both colors)
pub fn dragon_horse_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::{DIRECTION_SET_DIAGONAL, DIRECTION_SET_ORTHOGONAL};
    
    MovementConfig {
        capabilities: vec![
            // Range movement: diagonally
            // All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
            range_capability(
                DIRECTION_SET_DIAGONAL,  // All diagonals (same for both colors)
                BlockingMode::NoJump,
            ),
            // Simple movement: 1 space orthogonally
            // N (1), E (4), S (16), W (64) = 85 = 0x55 (same for both colors)
            MovementCapability::Simple {
                directions: DIRECTION_SET_ORTHOGONAL,  // All orthogonal directions (same for both colors)
                max_distance: 1,
            },
        ],
    }
}

/// Great Turtle movement: Simple 3 sideways, range all other directions, jump 3 straight forwards and backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// All other directions: N (1) | NE (2) | SE (8) | S (16) | SW (32) | NW (128) = 187 = 0xBB (for Black)
/// Jump: 3 spaces straight forwards and backwards (N and S for Black)
pub fn great_turtle_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 3 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 3,
            },
            // Range movement: all other directions (all except sideways)
            // For Black: All except E (4), W (64) = N (1) | NE (2) | SE (8) | S (16) | SW (32) | NW (128) = 187 = 0xBB
            // For White: All except E (4), W (64) (adjusted automatically)
            range_capability(
                0xBB,  // All directions except E, W (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
            // Jump movement: 3 spaces straight forwards and backwards
            // For Black: N (0, 3), S (0, -3)
            // For White: S (0, -3), N (0, 3) (adjusted automatically)
            MovementCapability::Jumping {
                offsets: vec![
                    (0, 3),   // N for Black
                    (0, -3),  // S for Black
                ],
            },
        ],
    }
}

/// Spirit Turtle movement: Range all directions, jump 3 in all 4 orthogonal directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// All 8 directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
/// Jump: 3 spaces in all 4 orthogonal directions (N, E, S, W)
pub fn spirit_turtle_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_ALL;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: all directions
            // All 8 directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
            range_capability(
                DIRECTION_SET_ALL,  // All directions (same for both colors)
                BlockingMode::NoJump,
            ),
            // Jump movement: 3 spaces in all 4 orthogonal directions
            // N (0, 3), E (3, 0), S (0, -3), W (-3, 0)
            MovementCapability::Jumping {
                offsets: vec![
                    (0, 3),   // N for Black
                    (3, 0),   // E (same for both colors)
                    (0, -3),  // S for Black
                    (-3, 0),  // W (same for both colors)
                ],
            },
        ],
    }
}

/// Little Turtle movement: Simple 3 sideways, range all other directions, jump 2 straight forwards and backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// All other directions: N (1) | NE (2) | SE (8) | S (16) | SW (32) | NW (128) = 187 = 0xBB (for Black)
/// Jump: 2 spaces straight forwards and backwards (N and S for Black)
pub fn little_turtle_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 3 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 3,
            },
            // Range movement: all other directions (all except sideways)
            // For Black: All except E (4), W (64) = N (1) | NE (2) | SE (8) | S (16) | SW (32) | NW (128) = 187 = 0xBB
            // For White: All except E (4), W (64) (adjusted automatically)
            range_capability(
                0xBB,  // All directions except E, W (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
            // Jump movement: 2 spaces straight forwards and backwards
            // For Black: N (0, 2), S (0, -2)
            // For White: S (0, -2), N (0, 2) (adjusted automatically)
            MovementCapability::Jumping {
                offsets: vec![
                    (0, 2),   // N for Black
                    (0, -2),  // S for Black
                ],
            },
        ],
    }
}

/// Treasure Turtle movement: Range all directions, jump 2 in all 4 orthogonal directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// All 8 directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
/// Jump: 2 spaces in all 4 orthogonal directions (N, E, S, W)
pub fn treasure_turtle_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_ALL;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: all directions
            // All 8 directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
            range_capability(
                DIRECTION_SET_ALL,  // All directions (same for both colors)
                BlockingMode::NoJump,
            ),
            // Jump movement: 2 spaces in all 4 orthogonal directions
            // N (0, 2), E (2, 0), S (0, -2), W (-2, 0)
            MovementCapability::Jumping {
                offsets: vec![
                    (0, 2),   // N for Black
                    (2, 0),   // E (same for both colors)
                    (0, -2),  // S for Black
                    (-2, 0),  // W (same for both colors)
                ],
            },
        ],
    }
}

/// Capricorn movement: Same as Tengu - two-step movement where both steps are range diagonal moves
/// Directions are relative to piece color (Black moves up, White moves down)
/// Diagonally: All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
pub fn capricorn_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    use crate::movement::types::BlockingMode;
    
    // Single step: range diagonal movement
    let diagonal_range = range_capability(
        DIRECTION_SET_DIAGONAL,  // 0xAA = NE, SE, SW, NW
        BlockingMode::NoJump,
    );
    
    MovementConfig {
        capabilities: vec![
            // Single step: range diagonal movement
            diagonal_range.clone(),
            // Two-step: both steps are range diagonal movements
            MovementCapability::TwoStep {
                first: Box::new(diagonal_range.clone()),
                second: Box::new(diagonal_range),
            },
        ],
    }
}

/// Hook Mover movement: Two-step movement where both steps are range orthogonal moves
/// Directions are relative to piece color (Black moves up, White moves down)
/// Orthogonally: N (1), E (4), S (16), W (64) = 85 = 0x55 (same for both colors)
pub fn hook_mover_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::direction::DIRECTION_SET_ORTHOGONAL;
    use crate::movement::types::BlockingMode;
    
    // Single step: range orthogonal movement
    let orthogonal_range = range_capability(
        DIRECTION_SET_ORTHOGONAL,  // 0x55 = N, E, S, W
        BlockingMode::NoJump,
    );
    
    MovementConfig {
        capabilities: vec![
            // Single step: range orthogonal movement
            orthogonal_range.clone(),
            // Two-step: both steps are range orthogonal movements
            MovementCapability::TwoStep {
                first: Box::new(orthogonal_range.clone()),
                second: Box::new(orthogonal_range),
            },
        ],
    }
}

/// Kirin movement: Simple 1 in all directions except sideways, jump 2 sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// All directions except sideways: N (1) | NE (2) | SE (8) | S (16) | SW (32) | NW (128) = 187 = 0xBB (for Black)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Jump: 2 spaces sideways (E, W)
pub fn kirin_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all directions except sideways
            // For Black: N (1) | NE (2) | SE (8) | S (16) | SW (32) | NW (128) = 187 = 0xBB
            // For White: S (16) | SW (32) | NW (128) | N (1) | NE (2) | SE (8) = 187 = 0xBB (adjusted automatically)
            MovementCapability::Simple {
                directions: 0xBB,  // All directions except E, W (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Jump movement: 2 spaces sideways
            // E (2, 0), W (-2, 0)
            MovementCapability::Jumping {
                offsets: vec![
                    (2, 0),   // E (same for both colors)
                    (-2, 0),  // W (same for both colors)
                ],
            },
        ],
    }
}

/// Phoenix movement: Simple 1 orthogonally, jump 2 in all diagonal directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Orthogonally: N (1), E (4), S (16), W (64) = 85 = 0x55 (same for both colors)
/// Diagonally: All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
/// Jump: 2 spaces in all diagonal directions
pub fn phoenix_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::direction::DIRECTION_SET_ORTHOGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space orthogonally
            // N (1), E (4), S (16), W (64) = 85 = 0x55 (same for both colors)
            MovementCapability::Simple {
                directions: DIRECTION_SET_ORTHOGONAL,  // All orthogonal directions (same for both colors)
                max_distance: 1,
            },
            // Jump movement: 2 spaces in all diagonal directions
            // NE (2, 2), SE (2, -2), SW (-2, -2), NW (-2, 2)
            MovementCapability::Jumping {
                offsets: vec![
                    (2, 2),    // NE for Black
                    (2, -2),   // SE for Black
                    (-2, -2),  // SW for Black
                    (-2, 2),   // NW for Black
                ],
            },
        ],
    }
}

/// Fire General movement: Simple 1 in forwards diagonal directions, simple 3 vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forwards diagonals: For Black: NE (2), NW (128) = 130 = 0x82
///                      For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
pub fn fire_general_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in forwards diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 3 spaces vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            MovementCapability::Simple {
                directions: 0x11,  // N, S (same for both colors)
                max_distance: 3,
            },
        ],
    }
}

/// Water General movement: Simple 1 vertically, simple 3 in forwards diagonal directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
/// Forwards diagonals: For Black: NE (2), NW (128) = 130 = 0x82
///                      For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
pub fn water_general_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            MovementCapability::Simple {
                directions: 0x11,  // N, S (same for both colors)
                max_distance: 1,
            },
            // Simple movement: up to 3 spaces in forwards diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 3,
            },
        ],
    }
}

/// Blind Dog movement: Simple 1 in forwards diagonal directions, sideways, and straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Forwards diagonals NE (2), NW (128) = 130 = 0x82
///            Sideways E (4), W (64) = 68 = 0x44
///            Straight backwards S (16) = 16 = 0x10
///            Combined: 0x82 | 0x44 | 0x10 = 214 = 0xD6
/// For White: Forwards diagonals SE (8), SW (32) = 40 = 0x28
///            Sideways E (4), W (64) = 68 = 0x44 (same)
///            Straight backwards N (1) = 1 = 0x01
///            Combined: 0x28 | 0x44 | 0x01 = 109 = 0x6D (adjusted automatically)
pub fn blind_dog_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in forwards diagonal directions, sideways, and straight backwards
            // For Black: NE (2) | NW (128) | E (4) | W (64) | S (16) = 214 = 0xD6
            // For White: SE (8) | SW (32) | E (4) | W (64) | N (1) = 109 = 0x6D (adjusted automatically)
            MovementCapability::Simple {
                directions: 0xD6,  // NE, NW, E, W, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Fierce Stag movement: Same movement as Silver General
/// Simple 1 diagonally and straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Diagonals NE (2), SE (8), SW (32), NW (128) = 0xAA; Forward N (1) = 0x01; Combined = 0xAB
/// For White: Diagonals are symmetric, forward S (16) is substituted for N (1) (handled automatically)
pub fn fierce_stag_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all diagonals and straight forward
            // For Black: Diagonals NE (2), SE (8), SW (32), NW (128) = 0xAA; Forward N (1) = 0x01; Combined = 0xAB
            // For White: Diagonals are symmetric, forward S (16) is substituted for N (1) (handled automatically)
            MovementCapability::Simple {
                directions: 0xAB,  // All diagonals and N (forward for Black); adjusted automatically for White
                max_distance: 1,
            },
        ],
    }
}

/// Moving Boar movement: Simple 1 space in all directions except straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: all directions except S (16)
/// So: N (1) | NE (2) | E (4) | SE (8) | SW (32) | W (64) | NW (128) = 239 = 0xEF
/// For White: adjusted automatically (excludes N which is backwards for White)
pub fn moving_boar_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            MovementCapability::Simple {
                // All directions except straight backwards
                // For Black: N (1) | NE (2) | E (4) | SE (8) | SW (32) | W (64) | NW (128) = 239 = 0xEF
                // For White: adjusted automatically (excludes N which is backwards for White)
                directions: 0xEF,  // All except S (16) for Black - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Crow Mover movement: Simple 1 in backwards diagonal directions and straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Backwards diagonals: For Black: SW (32), SE (8) = 40 = 0x28
///                      For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
/// Straight forwards: For Black: N (1) = 1 = 0x01
///                    For White: S (16) = 16 = 0x10 (adjusted automatically)
/// Combined: For Black: 0x28 | 0x01 = 41 = 0x29
///           For White: 0x82 | 0x10 = 146 = 0x92 (adjusted automatically)
pub fn crow_mover_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in backwards diagonal directions and straight forwards
            // For Black: SW (32) | SE (8) | N (1) = 41 = 0x29
            // For White: NW (128) | NE (2) | S (16) = 146 = 0x92 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x29,  // SW, SE, N (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Flying Hawk movement: Simple 1 straight forwards, range diagonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight forwards: For Black: N (1) = 1 = 0x01
///                    For White: S (16) = 16 = 0x10 (adjusted automatically)
/// Diagonally: All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
pub fn flying_hawk_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight forwards
            // For Black: N (1) = 1 = 0x01
            // For White: S (16) = 16 = 0x10 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: diagonally
            // All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
            range_capability(
                DIRECTION_SET_DIAGONAL,  // All diagonals (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Flying Goose movement: Simple 1 in all 3 forwards directions and straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// All 3 forwards: For Black: N (1), NE (2), NW (128) = 131 = 0x83
///                 For White: S (16), SE (8), SW (32) = 56 = 0x38 (adjusted automatically)
/// Straight backwards: For Black: S (16) = 16 = 0x10
///                      For White: N (1) = 1 = 0x01 (adjusted automatically)
/// Combined: For Black: 0x83 | 0x10 = 147 = 0x93
///           For White: 0x38 | 0x01 = 57 = 0x39 (adjusted automatically)
pub fn flying_goose_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all 3 forwards directions and straight backwards
            // For Black: N (1) | NE (2) | NW (128) | S (16) = 147 = 0x93
            // For White: S (16) | SE (8) | SW (32) | N (1) = 57 = 0x39 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x93,  // N, NE, NW, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Swallow's Wings movement: Simple 1 vertically, range sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
pub fn swallows_wings_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            MovementCapability::Simple {
                directions: 0x11,  // N, S (same for both colors)
                max_distance: 1,
            },
            // Range movement: sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            range_capability(
                0x44,  // E, W (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Swallow Mover movement: Range movement orthogonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Orthogonally: N (1) | E (4) | S (16) | W (64) = 85 = 0x55 (same for both colors)
pub fn swallow_mover_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_ORTHOGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: orthogonally
            // All 4 orthogonal directions: N (1) | E (4) | S (16) | W (64) = 85 = 0x55 (same for both colors)
            range_capability(
                DIRECTION_SET_ORTHOGONAL,  // All orthogonals (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Cat Sword movement: Simple 1 space diagonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Diagonally: NE (2) | SE (8) | SW (32) | NW (128) = 170 = 0xAA (same for both colors)
pub fn cat_sword_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space diagonally
            // All 4 diagonal directions: NE (2) | SE (8) | SW (32) | NW (128) = 170 = 0xAA (same for both colors)
            MovementCapability::Simple {
                directions: DIRECTION_SET_DIAGONAL,  // All diagonals (same for both colors)
                max_distance: 1,
            },
        ],
    }
}

/// Climbing Monkey movement: Simple 1 space in all 3 forwards directions, as well as straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: N (1) | NE (2) | NW (128) | S (16) = 147 = 0x93
/// For White: S (16) | SE (8) | SW (32) | N (1) = 57 = 0x39 (adjusted automatically)
pub fn climbing_monkey_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all 3 forwards directions and straight backwards
            // For Black: N (1) | NE (2) | NW (128) | S (16) = 147 = 0x93
            // For White: S (16) | SE (8) | SW (32) | N (1) = 57 = 0x39 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x93,  // N, NE, NW, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Owl Mover movement: Simple 1 space straight forward, and backward diagonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Straight forward: N (1) = 0x01, Backward diagonally: SE (8) | SW (32) = 40 = 0x28
/// For White: Straight forward: S (16) = 0x10, Backward diagonally: NE (2) | NW (128) = 130 = 0x82
pub fn owl_mover_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight forward and backward diagonally
            // For Black: N (1) | SE (8) | SW (32) = 41 = 0x29
            // For White: S (16) | NE (2) | NW (128) = 146 = 0x92 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x29,  // N, SE, SW (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Horseman movement: Simple up to 2 spaces sideways, range in all 3 forward directions and straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// For Black: Forwards: N (1), NE (2), NW (128) = 131 = 0x83, Backwards: S (16) = 0x10
/// For White: Forwards: S (16), SE (8), SW (32) = 56 = 0x38, Backwards: N (1) = 0x01
pub fn horseman_movement() -> MovementConfig {
    use crate::movement::types::{BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,
                max_distance: 2,
            },
            // Range movement: all 3 forward directions and straight backwards
            // For Black: N (1) | NE (2) | NW (128) | S (16) = 147 = 0x93
            // For White: S (16) | SE (8) | SW (32) | N (1) = 57 = 0x39 (adjusted automatically)
            range_capability(
                0x93,
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Tanuki movement: Simple up to 2 spaces orthogonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Orthogonally: N (1) | E (4) | S (16) | W (64) = 85 = 0x55 (same for both colors)
pub fn tanuki_movement() -> MovementConfig {
    use crate::movement::direction::DIRECTION_SET_ORTHOGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces orthogonally
            // All 4 orthogonal directions: N (1) | E (4) | S (16) | W (64) = 85 = 0x55 (same for both colors)
            MovementCapability::Simple {
                directions: DIRECTION_SET_ORTHOGONAL,  // All orthogonals (same for both colors)
                max_distance: 2,
            },
        ],
    }
}

/// Earth Chariot movement: Simple 1 space sideways, range movement vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
pub fn earth_chariot_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 1,
            },
            // Range movement: vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Reed Bird movement: Simple up to 2 spaces sideways and backwards diagonally, range movement vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// For Black: Backwards diagonally: SE (8), SW (32) = 40 = 0x28
/// For White: Backwards diagonally: NE (2), NW (128) = 130 = 0x82
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
pub fn reed_bird_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces sideways and backwards diagonally
            // For Black: E (4) | W (64) | SE (8) | SW (32) = 108 = 0x6C
            // For White: E (4) | W (64) | NE (2) | NW (128) = 198 = 0xC6 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x6C,  // E, W, SE, SW (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Range movement: vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Great Master movement: Simple up to 5 spaces sideways and backwards diagonally, range in other 4 directions, jump 3 in all 3 forwards directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// For Black: Backwards diagonally: SE (8), SW (32) = 40 = 0x28
/// For White: Backwards diagonally: NE (2), NW (128) = 130 = 0x82
/// For Black: Other 4 directions (range): N (1), S (16), NE (2), NW (128) = 147 = 0x93
/// For White: Other 4 directions (range): S (16), N (1), SE (8), SW (32) = 57 = 0x39
/// For Black: Forwards directions (jump 3): N (1), NE (2), NW (128) = 131 = 0x83
/// For White: Forwards directions (jump 3): S (16), SE (8), SW (32) = 56 = 0x38
pub fn great_master_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 5 spaces sideways and backwards diagonally
            // For Black: E (4) | W (64) | SE (8) | SW (32) = 108 = 0x6C
            // For White: E (4) | W (64) | NE (2) | NW (128) = 198 = 0xC6 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x6C,  // E, W, SE, SW (for Black) - will be adjusted for White
                max_distance: 5,
            },
            // Range movement: in the other 4 directions (straight forwards/backwards and forwards diagonals)
            // For Black: N (1) | S (16) | NE (2) | NW (128) = 147 = 0x93
            // For White: S (16) | N (1) | SE (8) | SW (32) = 57 = 0x39 (adjusted automatically)
            range_capability(
                0x93,  // N, S, NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
            // Jump movement: 3 spaces in all 3 forwards directions
            // For Black: N (0, 3), NE (3, 3), NW (-3, 3)
            // For White: S (0, -3), SE (3, -3), SW (-3, -3)
            MovementCapability::Jumping {
                offsets: vec![
                    (0, 3),   // N (for Black)
                    (3, 3),   // NE (for Black)
                    (-3, 3),  // NW (for Black)
                    (0, -3),  // S (for White)
                    (3, -3),  // SE (for White)
                    (-3, -3), // SW (for White)
                ],
            },
        ],
    }
}

/// Great Standard movement: Simple up to 3 spaces backwards diagonally, range in other 6 directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Backwards diagonally: SE (8), SW (32) = 40 = 0x28
/// For White: Backwards diagonally: NE (2), NW (128) = 130 = 0x82
/// For Black: Other 6 directions (range): N (1), S (16), E (4), W (64), NE (2), NW (128) = 215 = 0xD7
/// For White: Other 6 directions (range): S (16), N (1), E (4), W (64), SE (8), SW (32) = 125 = 0x7D
pub fn great_standard_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 3 spaces backwards diagonally
            // For Black: SE (8), SW (32) = 40 = 0x28
            // For White: NE (2), NW (128) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SE, SW (for Black) - will be adjusted for White
                max_distance: 3,
            },
            // Range movement: in the other 6 directions (orthogonally and forwards diagonals)
            // For Black: N (1) | S (16) | E (4) | W (64) | NE (2) | NW (128) = 215 = 0xD7
            // For White: S (16) | N (1) | E (4) | W (64) | SE (8) | SW (32) = 125 = 0x7D (adjusted automatically)
            range_capability(
                0xD7,  // N, S, E, W, NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Iron General movement: Simple 1 space in all 3 forward directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: N (1) | NE (2) | NW (128) = 131 = 0x83
/// For White: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
pub fn iron_general_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all 3 forward directions
            // For Black: N (1) | NE (2) | NW (128) = 131 = 0x83
            // For White: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x83,  // N, NE, NW (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Running Ox movement: Simple up to 2 spaces straight backwards, range sideways and in all 3 forwards directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 0x10
///                    For White: N (1) = 0x01 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// For Black: Forwards: N (1) | NE (2) | NW (128) = 131 = 0x83
/// For White: Forwards: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
pub fn running_ox_movement() -> MovementConfig {
    use crate::movement::types::{BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces straight backwards
            // For Black: S (16) = 0x10
            // For White: N (1) = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Range movement: sideways and in all 3 forwards directions
            // For Black: E (4) | W (64) | N (1) | NE (2) | NW (128) = 199 = 0xC7
            // For White: E (4) | W (64) | S (16) | SE (8) | SW (32) = 124 = 0x7C (adjusted automatically)
            range_capability(
                0xC7,  // E, W, N, NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Bear Soldier movement: Simple 1 space straight backwards, simple up to 2 spaces sideways, range in all 3 forwards directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 0x10
///                    For White: N (1) = 0x01 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// For Black: Forwards: N (1) | NE (2) | NW (128) = 131 = 0x83
/// For White: Forwards: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
pub fn bear_soldier_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16) = 0x10
            // For White: N (1) = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
            // Range movement: in all 3 forwards directions
            // For Black: N (1) | NE (2) | NW (128) = 131 = 0x83
            // For White: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
            range_capability(
                0x83,  // N, NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Strong Bear movement: Simple up to 2 spaces straight backwards, range in all other directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 0x10
///                    For White: N (1) = 0x01 (adjusted automatically)
/// All other directions: For Black: N (1) | NE (2) | NW (128) | E (4) | W (64) = 199 = 0xC7
///                        For White: S (16) | SE (8) | SW (32) | E (4) | W (64) = 124 = 0x7C (adjusted automatically)
pub fn strong_bear_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces straight backwards
            // For Black: S (16) = 0x10
            // For White: N (1) = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Range movement: in all other directions (forwards and sideways)
            // For Black: N (1) | NE (2) | NW (128) | E (4) | W (64) = 199 = 0xC7
            // For White: S (16) | SE (8) | SW (32) | E (4) | W (64) = 124 = 0x7C (adjusted automatically)
            range_capability(
                0xC7,  // N, NE, NW, E, W (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Tile General movement: Simple 1 space in forwards diagonal and straight backwards directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forwards diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                     For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// Straight backwards: For Black: S (16) = 0x10
///                     For White: N (1) = 0x01 (adjusted automatically)
pub fn tile_general_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in forwards diagonal and straight backwards directions
            // For Black: NE (2) | NW (128) | S (16) = 146 = 0x92
            // For White: SE (8) | SW (32) | N (1) = 41 = 0x29 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x92,  // NE, NW, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Leopard Soldier movement: Same as bear soldier (simple 1 space straight backwards, simple up to 2 spaces sideways, range in all 3 forwards directions)
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 0x10
///                    For White: N (1) = 0x01 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// For Black: Forwards: N (1) | NE (2) | NW (128) = 131 = 0x83
/// For White: Forwards: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
pub fn leopard_soldier_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16) = 0x10
            // For White: N (1) = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
            // Range movement: in all 3 forwards directions
            // For Black: N (1) | NE (2) | NW (128) = 131 = 0x83
            // For White: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
            range_capability(
                0x83,  // N, NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Running Leopard movement: Range movements in all directions except the 3 backwards directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// The 3 backwards directions are: straight backwards, backwards left diagonal, backwards right diagonal
/// So Running Leopard has range in: forwards (3 directions) + sideways (2 directions) = 5 directions
/// For Black: N (1) | NE (2) | NW (128) | E (4) | W (64) = 199 = 0xC7
/// For White: S (16) | SE (8) | SW (32) | E (4) | W (64) = 124 = 0x7C (adjusted automatically)
pub fn running_leopard_movement() -> MovementConfig {
    use crate::movement::types::{BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Range movement: in all directions except the 3 backwards directions
            // For Black: N (1) | NE (2) | NW (128) | E (4) | W (64) = 199 = 0xC7
            // For White: S (16) | SE (8) | SW (32) | E (4) | W (64) = 124 = 0x7C (adjusted automatically)
            range_capability(
                0xC7,  // N, NE, NW, E, W (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Stone General movement: Simple 1 space in forward diagonal directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: NE (2) | NW (128) = 130 = 0x82
/// For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
pub fn stone_general_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in forward diagonal directions
            // For Black: NE (2) | NW (128) = 130 = 0x82
            // For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Boar Soldier movement: Same as bear soldier and leopard soldier (simple 1 space straight backwards, simple up to 2 spaces sideways, range in all 3 forwards directions)
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 0x10
///                    For White: N (1) = 0x01 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// For Black: Forwards: N (1) | NE (2) | NW (128) = 131 = 0x83
/// For White: Forwards: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
pub fn boar_soldier_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16) = 0x10
            // For White: N (1) = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
            // Range movement: in all 3 forwards directions
            // For Black: N (1) | NE (2) | NW (128) = 131 = 0x83
            // For White: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
            range_capability(
                0x83,  // N, NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Running Boar movement: Simple 1 space sideways, range movement vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
pub fn running_boar_movement() -> MovementConfig {
    use crate::movement::types::{BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 1,
            },
            // Range movement: vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Earth General movement: Simple 1 space vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
pub fn earth_general_movement() -> MovementConfig {
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            MovementCapability::Simple {
                directions: 0x11,  // N, S (same for both colors)
                max_distance: 1,
            },
        ],
    }
}

/// Ox Soldier movement: Same as bear, boar, and leopard soldiers except sideways movement is up to 3 spaces rather than 2
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 0x10
///                    For White: N (1) = 0x01 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// For Black: Forwards: N (1) | NE (2) | NW (128) = 131 = 0x83
/// For White: Forwards: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
pub fn ox_soldier_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16) = 0x10
            // For White: N (1) = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 3 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 3,
            },
            // Range movement: in all 3 forwards directions
            // For Black: N (1) | NE (2) | NW (128) = 131 = 0x83
            // For White: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
            range_capability(
                0x83,  // N, NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Wood General movement: Simple up to 2 spaces forwards diagonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: NE (2) | NW (128) = 130 = 0x82
/// For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
pub fn wood_general_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces forwards diagonally
            // For Black: NE (2) | NW (128) = 130 = 0x82
            // For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 2,
            },
        ],
    }
}

/// Horse Soldier movement: Same as ox soldier (simple 1 space straight backwards, simple up to 3 spaces sideways, range in all 3 forwards directions)
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 0x10
///                    For White: N (1) = 0x01 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// For Black: Forwards: N (1) | NE (2) | NW (128) = 131 = 0x83
/// For White: Forwards: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
pub fn horse_soldier_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16) = 0x10
            // For White: N (1) = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 3 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 3,
            },
            // Range movement: in all 3 forwards directions
            // For Black: N (1) | NE (2) | NW (128) = 131 = 0x83
            // For White: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
            range_capability(
                0x83,  // N, NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Mountain General movement: Simple 1 space vertically, simple up to 3 spaces in forwards diagonal directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
/// Forwards diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                     For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
pub fn mountain_general_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            MovementCapability::Simple {
                directions: 0x11,  // N, S (same for both colors)
                max_distance: 1,
            },
            // Simple movement: up to 3 spaces in forwards diagonal directions
            // For Black: NE (2) | NW (128) = 130 = 0x82
            // For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 3,
            },
        ],
    }
}

/// Mount Tai movement: Simple up to 5 spaces orthogonally except backwards, range movement diagonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Orthogonally except backwards: For Black: N (1) | E (4) | W (64) = 69 = 0x45
///                                For White: S (16) | E (4) | W (64) = 84 = 0x54 (adjusted automatically)
/// Diagonally: NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
pub fn mount_tai_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 5 spaces orthogonally except backwards
            // For Black: N (1) | E (4) | W (64) = 69 = 0x45
            // For White: S (16) | E (4) | W (64) = 84 = 0x54 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x45,  // N, E, W (for Black) - will be adjusted for White
                max_distance: 5,
            },
            // Range movement: diagonally
            // NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
            range_capability(
                0xAA,  // NE, SE, SW, NW (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// River General movement: Simple 1 space in forward diagonal and straight backward directions, simple up to 3 spaces straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forward diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// Straight backwards: For Black: S (16) = 0x10
///                     For White: N (1) = 0x01 (adjusted automatically)
/// Straight forwards: For Black: N (1) = 0x01
///                   For White: S (16) = 0x10 (adjusted automatically)
pub fn river_general_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in forward diagonal and straight backward directions
            // For Black: NE (2) | NW (128) | S (16) = 146 = 0x92
            // For White: SE (8) | SW (32) | N (1) = 41 = 0x29 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x92,  // NE, NW, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 3 spaces straight forwards
            // For Black: N (1) = 0x01
            // For White: S (16) = 0x10 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (for Black) - will be adjusted for White
                max_distance: 3,
            },
        ],
    }
}

/// Huai River movement: Simple 1 space vertically, range movement in all other directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
/// All other directions: For Black: NE (2) | NW (128) | E (4) | W (64) | SE (8) | SW (32) = 238 = 0xEE
///                        For White: SE (8) | SW (32) | E (4) | W (64) | NE (2) | NW (128) = 238 = 0xEE (same for both colors)
pub fn huai_river_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            MovementCapability::Simple {
                directions: 0x11,  // N, S (same for both colors)
                max_distance: 1,
            },
            // Range movement: in all other directions (diagonally and sideways)
            // NE (2) | NW (128) | E (4) | W (64) | SE (8) | SW (32) = 238 = 0xEE (same for both colors)
            range_capability(
                0xEE,  // NE, NW, E, W, SE, SW (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Wind General movement: Same as River General
/// Simple 1 space in forward diagonal and straight backward directions, simple up to 3 spaces straight forwards
pub fn wind_general_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in forward diagonal and straight backward directions
            // For Black: NE (2) | NW (128) | S (16) = 146 = 0x92
            // For White: SE (8) | SW (32) | N (1) = 41 = 0x29 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x92,  // NE, NW, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 3 spaces straight forwards
            // For Black: N (1) = 0x01
            // For White: S (16) = 0x10 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (for Black) - will be adjusted for White
                max_distance: 3,
            },
        ],
    }
}

/// Fierce Wind movement: Simple 1 space sideways, range movement in all other directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// All other directions: For Black: N (1) | NE (2) | NW (128) | S (16) | SE (8) | SW (32) = 187 = 0xBB
///                        For White: S (16) | SE (8) | SW (32) | N (1) | NE (2) | NW (128) = 187 = 0xBB (same for both colors)
pub fn fierce_wind_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 1,
            },
            // Range movement: in all other directions (vertically and diagonally)
            // N (1) | NE (2) | NW (128) | S (16) | SE (8) | SW (32) = 187 = 0xBB (same for both colors)
            range_capability(
                0xBB,  // N, NE, NW, S, SE, SW (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Vertical Soldier movement: Simple 1 space straight backwards, simple up to 2 spaces sideways, range movement straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 0x10
///                     For White: N (1) = 0x01 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Straight forwards: For Black: N (1) = 0x01
///                   For White: S (16) = 0x10 (adjusted automatically)
pub fn vertical_soldier_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16) = 0x10
            // For White: N (1) = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
            // Range movement: straight forwards
            // For Black: N (1) = 0x01
            // For White: S (16) = 0x10 (adjusted automatically)
            range_capability(
                0x01,  // N (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Chariot Soldier movement: Simple up to 2 spaces sideways, range movement in all other directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// All other directions: For Black: N (1) | NE (2) | NW (128) | S (16) | SE (8) | SW (32) = 187 = 0xBB
///                        For White: S (16) | SE (8) | SW (32) | N (1) | NE (2) | NW (128) = 187 = 0xBB (same for both colors)
pub fn chariot_soldier_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
            // Range movement: in all other directions (vertically and diagonally)
            // N (1) | NE (2) | NW (128) | S (16) | SE (8) | SW (32) = 187 = 0xBB (same for both colors)
            range_capability(
                0xBB,  // N, NE, NW, S, SE, SW (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Side General movement: Simple up to 2 spaces vertically, range movement in all other directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
/// All other directions: For Black: NE (2) | NW (128) | E (4) | W (64) | SE (8) | SW (32) = 238 = 0xEE
///                        For White: SE (8) | SW (32) | E (4) | W (64) | NE (2) | NW (128) = 238 = 0xEE (same for both colors)
pub fn side_general_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            MovementCapability::Simple {
                directions: 0x11,  // N, S (same for both colors)
                max_distance: 2,
            },
            // Range movement: in all other directions (diagonally and sideways)
            // NE (2) | NW (128) | E (4) | W (64) | SE (8) | SW (32) = 238 = 0xEE (same for both colors)
            range_capability(
                0xEE,  // NE, NW, E, W, SE, SW (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Shitennou movement: Jumping range movements in all directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// All directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF (same for both colors)
pub fn shitennou_movement() -> MovementConfig {
    use crate::movement::types::{BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Range movement with jumping: in all 8 directions
            // N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF (same for both colors)
            range_capability(
                0xFF,  // All 8 directions (same for both colors)
                BlockingMode::Jump,  // Can jump over pieces
            ),
        ],
    }
}

/// Great Elephant movement: Simple up to 3 spaces in forward diagonal directions, jump movement up to 3 spaces in all other directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forward diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// All other directions: N, S, E, W, SE (for Black), SW (for Black)
///                       For White: S, N, E, W, NE, NW (adjusted automatically)
/// Note: Jump movement means it can jump over pieces, but is limited to 3 spaces. We use Jumping with all offsets 1-3.
pub fn great_elephant_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 3 spaces in forward diagonal directions
            // For Black: NE (2) | NW (128) = 130 = 0x82
            // For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 3,
            },
            // Jump movement: up to 3 spaces in all other directions
            // Offsets are (file_delta, rank_delta) relative to starting position
            // For Black (moving up): N (0, -1 to -3), S (0, 1 to 3), E (1 to 3, 0), W (-1 to -3, 0), SE (1 to 3, 1 to 3), SW (-1 to -3, 1 to 3)
            // For White (moving down): S (0, -1 to -3), N (0, 1 to 3), E (1 to 3, 0), W (-1 to -3, 0), NE (1 to 3, -1 to -3), NW (-1 to -3, -1 to -3)
            // The system will adjust these automatically based on piece color
            MovementCapability::Jumping {
                offsets: vec![
                    // N (for Black), S (for White): 1, 2, 3 spaces
                    (0, -1), (0, -2), (0, -3),
                    // S (for Black), N (for White): 1, 2, 3 spaces
                    (0, 1), (0, 2), (0, 3),
                    // E (same for both): 1, 2, 3 spaces
                    (1, 0), (2, 0), (3, 0),
                    // W (same for both): 1, 2, 3 spaces
                    (-1, 0), (-2, 0), (-3, 0),
                    // SE (for Black), NE (for White): 1, 2, 3 spaces
                    (1, 1), (2, 2), (3, 3),
                    // SW (for Black), NW (for White): 1, 2, 3 spaces
                    (-1, 1), (-2, 2), (-3, 3),
                ],
            },
        ],
    }
}

/// Poisonous Serpent movement: Simple 1 straight backwards and forwards diagonals, simple 2 straight forwards and sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 16 = 0x10
///                      For White: N (1) = 1 = 0x01 (adjusted automatically)
/// Forwards diagonals: For Black: NE (2), NW (128) = 130 = 0x82
///                      For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
/// Straight forwards: For Black: N (1) = 1 = 0x01
///                    For White: S (16) = 16 = 0x10 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
pub fn poisonous_serpent_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards and forwards diagonals
            // For Black: S (16) | NE (2) | NW (128) = 146 = 0x92
            // For White: N (1) | SE (8) | SW (32) = 41 = 0x29 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x92,  // S, NE, NW (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces straight forwards and sideways
            // For Black: N (1) | E (4) | W (64) = 69 = 0x45
            // For White: S (16) | E (4) | W (64) = 84 = 0x54 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x45,  // N, E, W (for Black) - will be adjusted for White
                max_distance: 2,
            },
        ],
    }
}

/// Roaring Dog movement: Simple up to 3 spaces in backwards diagonal directions, range and jump 3 spaces in all other directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Backwards diagonals: For Black: SE (8) | SW (32) = 40 = 0x28
///                      For White: NE (2) | NW (128) = 130 = 0x82 (adjusted automatically)
/// All other directions: For Black: N (1) | E (4) | S (16) | W (64) | NE (2) | NW (128) = 199 = 0xC7
///                       For White: S (16) | E (4) | N (1) | W (64) | SE (8) | SW (32) = 125 = 0x7D (adjusted automatically)
pub fn roaring_dog_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 3 spaces in backwards diagonal directions
            // For Black: SE (8) | SW (32) = 40 = 0x28
            // For White: NE (2) | NW (128) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SE, SW (for Black) - will be adjusted for White
                max_distance: 3,
            },
            // Range movement: in all other directions
            // For Black: N (1) | E (4) | S (16) | W (64) | NE (2) | NW (128) = 199 = 0xC7
            // For White: S (16) | E (4) | N (1) | W (64) | SE (8) | SW (32) = 125 = 0x7D (adjusted automatically)
            MovementCapability::Range {
                directions: 0xC7,  // N, E, S, W, NE, NW (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
            // Jump movement: 3 spaces in all other directions
            // For Black: N (0, -3), S (0, 3), E (3, 0), W (-3, 0), NE (3, -3), NW (-3, -3)
            // For White: S (0, -3), N (0, 3), E (3, 0), W (-3, 0), SE (3, 3), SW (-3, 3)
            MovementCapability::Jumping {
                offsets: vec![
                    // N (for Black), S (for White): 3 spaces
                    (0, -3),
                    // S (for Black), N (for White): 3 spaces
                    (0, 3),
                    // E (same for both): 3 spaces
                    (3, 0),
                    // W (same for both): 3 spaces
                    (-3, 0),
                    // NE (for Black), SE (for White): 3 spaces
                    (3, -3),
                    // NW (for Black), SW (for White): 3 spaces
                    (-3, -3),
                ],
            },
        ],
    }
}

/// Crossbow Soldier movement: Simple 1 space straight backwards, simple up to 3 spaces sideways and forwards diagonals, simple up to 5 spaces straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 0x10
///                     For White: N (1) = 0x01 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Forwards diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// Combined sideways and forwards diagonals: For Black: 0x44 | 0x82 = 0xC6
///                                           For White: 0x44 | 0x28 = 0x6C (adjusted automatically)
/// Straight forwards: For Black: N (1) = 0x01
///                   For White: S (16) = 0x10 (adjusted automatically)
pub fn crossbow_soldier_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16) = 0x10
            // For White: N (1) = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 3 spaces sideways and forwards diagonals (combined)
            // For Black: E (4) | W (64) | NE (2) | NW (128) = 0xC6
            // For White: E (4) | W (64) | SE (8) | SW (32) = 0x6C (adjusted automatically)
            MovementCapability::Simple {
                directions: 0xC6,  // E, W, NE, NW (for Black) - will be adjusted for White
                max_distance: 3,
            },
            // Simple movement: up to 5 spaces straight forwards
            // For Black: N (1) = 0x01
            // For White: S (16) = 0x10 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (for Black) - will be adjusted for White
                max_distance: 5,
            },
        ],
    }
}

/// Crossbow General movement: Simple up to 2 spaces straight backwards, simple up to 3 spaces sideways, simple up to 5 spaces forwards diagonally, range movement straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 0x10
///                     For White: N (1) = 0x01 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Forwards diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// Straight forwards: For Black: N (1) = 0x01
///                   For White: S (16) = 0x10 (adjusted automatically)
pub fn crossbow_general_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces straight backwards
            // For Black: S (16) = 0x10
            // For White: N (1) = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Simple movement: up to 3 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 3,
            },
            // Simple movement: up to 5 spaces in forwards diagonal directions
            // For Black: NE (2) | NW (128) = 130 = 0x82
            // For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 5,
            },
            // Range movement: straight forwards
            // For Black: N (1) = 0x01
            // For White: S (16) = 0x10 (adjusted automatically)
            MovementCapability::Range {
                directions: 0x01,  // N (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Cannon Soldier movement: Simple 1 space straight backwards, simple up to 3 spaces sideways, simple up to 5 spaces in forwards diagonal directions, simple up to 7 spaces straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 0x10
///                     For White: N (1) = 0x01 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Forwards diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// Straight forwards: For Black: N (1) = 0x01
///                   For White: S (16) = 0x10 (adjusted automatically)
pub fn cannon_soldier_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16) = 0x10
            // For White: N (1) = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 3 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 3,
            },
            // Simple movement: up to 5 spaces in forwards diagonal directions
            // For Black: NE (2) | NW (128) = 130 = 0x82
            // For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 5,
            },
            // Simple movement: up to 7 spaces straight forwards
            // For Black: N (1) = 0x01
            // For White: S (16) = 0x10 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (for Black) - will be adjusted for White
                max_distance: 7,
            },
        ],
    }
}

/// Cannon General movement: Simple up to 2 spaces straight backwards, simple up to 3 spaces sideways, range movement in all 3 forwards directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 0x10
///                     For White: N (1) = 0x01 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// All 3 forwards directions: For Black: N (1) | NE (2) | NW (128) = 131 = 0x83
///                            For White: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
pub fn cannon_general_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces straight backwards
            // For Black: S (16) = 0x10
            // For White: N (1) = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Simple movement: up to 3 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 3,
            },
            // Range movement: in all 3 forwards directions
            // For Black: N (1) | NE (2) | NW (128) = 131 = 0x83
            // For White: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
            MovementCapability::Range {
                directions: 0x83,  // N, NE, NW (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Vertical Horse movement: Simple 1 space in forwards diagonal and straight backwards directions, range movement straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forwards diagonal and straight backwards: For Black: NE (2) | NW (128) | S (16) = 146 = 0x92
///                                          For White: SE (8) | SW (32) | N (1) = 41 = 0x29 (adjusted automatically)
/// Straight forwards: For Black: N (1) = 0x01
///                   For White: S (16) = 0x10 (adjusted automatically)
pub fn vertical_horse_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in forwards diagonal and straight backwards directions
            // For Black: NE (2) | NW (128) | S (16) = 146 = 0x92
            // For White: SE (8) | SW (32) | N (1) = 41 = 0x29 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x92,  // NE, NW, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: straight forwards
            // For Black: N (1) = 0x01
            // For White: S (16) = 0x10 (adjusted automatically)
            MovementCapability::Range {
                directions: 0x01,  // N (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Vertical Pup movement: Simple 1 space in all 3 backwards directions, range movement straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// All 3 backwards directions: For Black: S (16) | SW (32) | SE (8) = 56 = 0x38
///                            For White: N (1) | NE (2) | NW (128) = 131 = 0x83 (adjusted automatically)
/// Straight forwards: For Black: N (1) = 0x01
///                   For White: S (16) = 0x10 (adjusted automatically)
pub fn vertical_pup_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all 3 backwards directions
            // For Black: S (16) | SW (32) | SE (8) = 56 = 0x38
            // For White: N (1) | NE (2) | NW (128) = 131 = 0x83 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x38,  // S, SW, SE (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: straight forwards
            // For Black: N (1) = 0x01
            // For White: S (16) = 0x10 (adjusted automatically)
            MovementCapability::Range {
                directions: 0x01,  // N (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Leopard King movement: Simple up to 5 spaces in all directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// All directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF (same for both colors)
pub fn leopard_king_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 5 spaces in all 8 directions
            // N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF (same for both colors)
            MovementCapability::Simple {
                directions: 0xFF,  // All 8 directions (same for both colors)
                max_distance: 5,
            },
        ],
    }
}

/// Longbow Soldier movement: Simple 1 space straight backwards, simple up to 2 spaces sideways, simple up to 5 spaces in forwards diagonal directions, range movement straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 0x10
///                     For White: N (1) = 0x01 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Forwards diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// Straight forwards: For Black: N (1) = 0x01
///                   For White: S (16) = 0x10 (adjusted automatically)
pub fn longbow_soldier_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16) = 0x10
            // For White: N (1) = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
            // Simple movement: up to 5 spaces in forwards diagonal directions
            // For Black: NE (2) | NW (128) = 130 = 0x82
            // For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 5,
            },
            // Range movement: straight forwards
            // For Black: N (1) = 0x01
            // For White: S (16) = 0x10 (adjusted automatically)
            MovementCapability::Range {
                directions: 0x01,  // N (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Longbow General movement: Simple up to 5 spaces sideways, range movement in all 3 forwards directions and straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// All 3 forwards directions and straight backwards: For Black: N (1) | NE (2) | NW (128) | S (16) = 147 = 0x93
///                                                  For White: S (16) | SE (8) | SW (32) | N (1) = 57 = 0x39 (adjusted automatically)
pub fn longbow_general_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 5 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 5,
            },
            // Range movement: in all 3 forwards directions and straight backwards
            // For Black: N (1) | NE (2) | NW (128) | S (16) = 147 = 0x93
            // For White: S (16) | SE (8) | SW (32) | N (1) = 57 = 0x39 (adjusted automatically)
            MovementCapability::Range {
                directions: 0x93,  // N, NE, NW, S (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Side Monkey movement: Simple 1 space forwards diagonally and straight backwards, range movement sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forwards diagonally and straight backwards: For Black: NE (2) | NW (128) | S (16) = 146 = 0x92
///                                          For White: SE (8) | SW (32) | N (1) = 41 = 0x29 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
pub fn side_monkey_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space forwards diagonally and straight backwards
            // For Black: NE (2) | NW (128) | S (16) = 146 = 0x92
            // For White: SE (8) | SW (32) | N (1) = 41 = 0x29 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x92,  // NE, NW, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Range {
                directions: 0x44,  // E, W (same for both colors)
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Left Chariot movement: Simple 1 space leftwards, range movement straight forwards, forwards right, and backwards left
/// Directions are relative to piece color (Black moves up, White moves down)
/// Leftwards: For Black: W (64) = 0x40
///          For White: E (4) = 0x04 (mirrored)
/// Straight forwards, forwards right, and backwards left: For Black: N (1) | NE (2) | SW (32) = 35 = 0x23
///                                                      For White: S (16) | SE (8) | NW (128) = 152 = 0x98 (adjusted automatically)
pub fn left_chariot_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space leftwards
            // For Black: W (64) = 0x40
            // For White: E (4) = 0x04 (mirrored - left for Black is right for White)
            MovementCapability::Simple {
                directions: 0x40,  // W (for Black) - will be adjusted for White (becomes E)
                max_distance: 1,
            },
            // Range movement: straight forwards, forwards right, and backwards left
            // For Black: N (1) | NE (2) | SW (32) = 35 = 0x23
            // For White: S (16) | SE (8) | NW (128) = 152 = 0x98 (adjusted automatically)
            MovementCapability::Range {
                directions: 0x23,  // N, NE, SW (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Left Iron Chariot movement: Simple 1 space leftwards, range movement diagonally except forwards left
/// Directions are relative to piece color (Black moves up, White moves down)
/// Leftwards: For Black: W (64) = 0x40
///          For White: E (4) = 0x04 (mirrored)
/// Diagonally except forwards left: For Black: NE (2) | SE (8) | SW (32) = 42 = 0x2A (excludes NW (128))
///                                  For White: NW (128) | NE (2) | SW (32) = 162 = 0xA2 (excludes SE (8), adjusted automatically)
pub fn left_iron_chariot_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space leftwards
            // For Black: W (64) = 0x40
            // For White: E (4) = 0x04 (mirrored)
            MovementCapability::Simple {
                directions: 0x40,  // W (for Black) - will be adjusted for White (becomes E)
                max_distance: 1,
            },
            // Range movement: diagonally except forwards left
            // For Black: NE (2) | SE (8) | SW (32) = 42 = 0x2A (excludes NW (128))
            // For White: NW (128) | NE (2) | SW (32) = 162 = 0xA2 (excludes SE (8), adjusted automatically)
            MovementCapability::Range {
                directions: 0x2A,  // NE, SE, SW (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Right Chariot movement: Simple 1 space rightwards, range movement straight forwards, forwards left, and backwards right
/// Directions are relative to piece color (Black moves up, White moves down)
/// Rightwards: For Black: E (4) = 0x04
///           For White: W (64) = 0x40 (mirrored)
/// Straight forwards, forwards left, and backwards right: For Black: N (1) | NW (128) | SE (8) = 137 = 0x89
///                                                       For White: S (16) | SW (32) | NE (2) = 50 = 0x32 (adjusted automatically)
pub fn right_chariot_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space rightwards
            // For Black: E (4) = 0x04
            // For White: W (64) = 0x40 (mirrored - right for Black is left for White)
            MovementCapability::Simple {
                directions: 0x04,  // E (for Black) - will be adjusted for White (becomes W)
                max_distance: 1,
            },
            // Range movement: straight forwards, forwards left, and backwards right
            // For Black: N (1) | NW (128) | SE (8) = 137 = 0x89
            // For White: S (16) | SW (32) | NE (2) = 50 = 0x32 (adjusted automatically)
            MovementCapability::Range {
                directions: 0x89,  // N, NW, SE (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Right Iron Chariot movement: Simple 1 space rightwards, range movement diagonally except forwards right
/// Directions are relative to piece color (Black moves up, White moves down)
/// Rightwards: For Black: E (4) = 0x04
///           For White: W (64) = 0x40 (mirrored)
/// Diagonally except forwards right: For Black: NW (128) | SE (8) | SW (32) = 168 = 0xA8 (excludes NE (2))
///                                   For White: NE (2) | SW (32) | SE (8) = 42 = 0x2A (excludes NW (128), adjusted automatically)
pub fn right_iron_chariot_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space rightwards
            // For Black: E (4) = 0x04
            // For White: W (64) = 0x40 (mirrored)
            MovementCapability::Simple {
                directions: 0x04,  // E (for Black) - will be adjusted for White (becomes W)
                max_distance: 1,
            },
            // Range movement: diagonally except forwards right
            // For Black: NW (128) | SE (8) | SW (32) = 168 = 0xA8 (excludes NE (2))
            // For White: NE (2) | SW (32) | SE (8) = 42 = 0x2A (excludes NW (128), adjusted automatically)
            MovementCapability::Range {
                directions: 0xA8,  // NW, SE, SW (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Fierce Tiger movement: Range movement straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight forwards: For Black: N (1) = 0x01
///                   For White: S (16) = 0x10 (adjusted automatically)
pub fn fierce_tiger_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: straight forwards
            // For Black: N (1) = 0x01
            // For White: S (16) = 0x10 (adjusted automatically)
            MovementCapability::Range {
                directions: 0x01,  // N (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Great Tiger movement: Simple 1 space straight forwards, range movement in other orthogonal directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight forwards: For Black: N (1) = 0x01
///                   For White: S (16) = 0x10 (adjusted automatically)
/// Other orthogonal directions: For Black: E (4) | S (16) | W (64) = 84 = 0x54
///                              For White: E (4) | N (1) | W (64) = 69 = 0x45 (adjusted automatically)
pub fn great_tiger_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight forwards
            // For Black: N (1) = 0x01
            // For White: S (16) = 0x10 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: in other orthogonal directions (E, S, W for Black; E, N, W for White)
            // For Black: E (4) | S (16) | W (64) = 84 = 0x54
            // For White: E (4) | N (1) | W (64) = 69 = 0x45 (adjusted automatically)
            MovementCapability::Range {
                directions: 0x54,  // E, S, W (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Vertical Leopard movement: Simple 1 space in forwards diagonal, sideways, and straight backwards directions, range movement straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forwards diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Straight backwards: For Black: S (16) = 0x10
///                     For White: N (1) = 0x01 (adjusted automatically)
/// Combined: For Black: 0x82 | 0x44 | 0x10 = 0xD6
///           For White: 0x28 | 0x44 | 0x01 = 0x6D (adjusted automatically)
/// Straight forwards: For Black: N (1) = 0x01
///                   For White: S (16) = 0x10 (adjusted automatically)
pub fn vertical_leopard_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in forwards diagonal, sideways, and straight backwards directions
            // For Black: NE (2) | NW (128) | E (4) | W (64) | S (16) = 0xD6
            // For White: SE (8) | SW (32) | E (4) | W (64) | N (1) = 0x6D (adjusted automatically)
            MovementCapability::Simple {
                directions: 0xD6,  // NE, NW, E, W, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: straight forwards
            // For Black: N (1) = 0x01
            // For White: S (16) = 0x10 (adjusted automatically)
            MovementCapability::Range {
                directions: 0x01,  // N (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Great Leopard movement: Simple 1 space straight backwards, simple 2 spaces sideways, simple 3 spaces in forwards diagonal directions, range movement straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 0x10
///                     For White: N (1) = 0x01 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Forwards diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// Straight forwards: For Black: N (1) = 0x01
///                   For White: S (16) = 0x10 (adjusted automatically)
pub fn great_leopard_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16) = 0x10
            // For White: N (1) = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: 2 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
            // Simple movement: 3 spaces in forwards diagonal directions
            // For Black: NE (2) | NW (128) = 130 = 0x82
            // For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 3,
            },
            // Range movement: straight forwards
            // For Black: N (1) = 0x01
            // For White: S (16) = 0x10 (adjusted automatically)
            MovementCapability::Range {
                directions: 0x01,  // N (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Spear Soldier movement: Simple 1 space sideways and straight backwards, range movement straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Straight backwards: For Black: S (16) = 0x10
///                     For White: N (1) = 0x01 (adjusted automatically)
/// Combined: For Black: 0x44 | 0x10 = 0x54
///           For White: 0x44 | 0x01 = 0x45 (adjusted automatically)
/// Straight forwards: For Black: N (1) = 0x01
///                   For White: S (16) = 0x10 (adjusted automatically)
pub fn spear_soldier_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space sideways and straight backwards
            // For Black: E (4) | W (64) | S (16) = 0x54
            // For White: E (4) | W (64) | N (1) = 0x45 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x54,  // E, W, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: straight forwards
            // For Black: N (1) = 0x01
            // For White: S (16) = 0x10 (adjusted automatically)
            MovementCapability::Range {
                directions: 0x01,  // N (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Spear General movement: Simple up to 2 spaces straight backwards, simple up to 3 spaces sideways, range movement straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 0x10
///                     For White: N (1) = 0x01 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Straight forwards: For Black: N (1) = 0x01
///                   For White: S (16) = 0x10 (adjusted automatically)
pub fn spear_general_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces straight backwards
            // For Black: S (16) = 0x10
            // For White: N (1) = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Simple movement: up to 3 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 3,
            },
            // Range movement: straight forwards
            // For Black: N (1) = 0x01
            // For White: S (16) = 0x10 (adjusted automatically)
            MovementCapability::Range {
                directions: 0x01,  // N (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Great Eagle movement: Jumping range movement in forward diagonal directions, normal range movement in all other directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forward diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// All other directions: For Black: N (1) | E (4) | S (16) | W (64) | SE (8) | SW (32) = 125 = 0x7D
///                       For White: S (16) | E (4) | N (1) | W (64) | NE (2) | NW (128) = 199 = 0xC7 (adjusted automatically)
pub fn great_eagle_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Range movement with jumping: in forward diagonal directions
            // For Black: NE (2) | NW (128) = 130 = 0x82
            // For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Range {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                blocking: BlockingMode::Jump,  // Can jump over pieces
                cannot_jump_over: HashSet::new(),
            },
            // Range movement (normal): in all other directions
            // For Black: N (1) | E (4) | S (16) | W (64) | SE (8) | SW (32) = 125 = 0x7D
            // For White: S (16) | E (4) | N (1) | W (64) | NE (2) | NW (128) = 199 = 0xC7 (adjusted automatically)
            MovementCapability::Range {
                directions: 0x7D,  // N, E, S, W, SE, SW (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Great Hawk movement: Jumping range movement straight forwards, normal range movement in all other directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight forwards: For Black: N (1) = 0x01
///                   For White: S (16) = 0x10 (adjusted automatically)
/// All other directions: For Black: NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 254 = 0xFE
///                       For White: SE (8) | E (4) | NE (2) | S (16) | NW (128) | W (64) | SW (32) = 254 = 0xFE (same for both colors)
pub fn great_hawk_movement() -> MovementConfig {
    use crate::movement::types::{BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Range movement with jumping: straight forwards
            // For Black: N (1) = 0x01
            // For White: S (16) = 0x10 (adjusted automatically)
            MovementCapability::Range {
                directions: 0x01,  // N (for Black) - will be adjusted for White
                blocking: BlockingMode::Jump,  // Can jump over pieces
                cannot_jump_over: HashSet::new(),
            },
            // Range movement (normal): in all other directions
            // NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 254 = 0xFE (same for both colors)
            MovementCapability::Range {
                directions: 0xFE,  // All directions except N (for Black) - will be adjusted for White
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Sword Soldier movement: Simple 1 space in forwards diagonal and straight backwards directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forwards diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// Straight backwards: For Black: S (16) = 0x10
///                     For White: N (1) = 0x01 (adjusted automatically)
/// Combined: For Black: 0x82 | 0x10 = 0x92
///           For White: 0x28 | 0x01 = 0x29 (adjusted automatically)
pub fn sword_soldier_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in forwards diagonal and straight backwards directions
            // For Black: NE (2) | NW (128) | S (16) = 0x92
            // For White: SE (8) | SW (32) | N (1) = 0x29 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x92,  // NE, NW, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Sword General movement: Simple 1 space straight backwards, simple up to 3 spaces in forwards diagonal directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 0x10
///                     For White: N (1) = 0x01 (adjusted automatically)
/// Forwards diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
pub fn sword_general_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16) = 0x10
            // For White: N (1) = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 3 spaces in forwards diagonal directions
            // For Black: NE (2) | NW (128) = 130 = 0x82
            // For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 3,
            },
        ],
    }
}

/// Flying Dragon movement: Jump 2 spaces in all 4 diagonal directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Diagonally: All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
/// Jump: 2 spaces in all diagonal directions
pub fn flying_dragon_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Jump movement: 2 spaces in all 4 diagonal directions
            // NE (2, 2), SE (2, -2), SW (-2, -2), NW (-2, 2)
            MovementCapability::Jumping {
                offsets: vec![
                    (2, 2),    // NE for Black
                    (2, -2),   // SE for Black
                    (-2, -2),  // SW for Black
                    (-2, 2),   // NW for Black
                ],
            },
        ],
    }
}

/// Fierce Eagle movement: Simple 1 straight forwards and sideways, simple 2 diagonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight forwards: For Black: N (1) = 1 = 0x01
///                    For White: S (16) = 16 = 0x10 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Diagonally: All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
pub fn fierce_eagle_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight forwards and sideways
            // For Black: N (1) | E (4) | W (64) = 69 = 0x45
            // For White: S (16) | E (4) | W (64) = 84 = 0x54 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x45,  // N, E, W (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces diagonally
            // All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
            MovementCapability::Simple {
                directions: DIRECTION_SET_DIAGONAL,  // All diagonals (same for both colors)
                max_distance: 2,
            },
        ],
    }
}

/// Fierce Leopard movement: Simple 1 in all directions except sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// All directions except sideways: For Black: N (1) | NE (2) | SE (8) | S (16) | SW (32) | NW (128) = 187 = 0xBB
///                                  For White: S (16) | SW (32) | NW (128) | N (1) | NE (2) | SE (8) = 187 = 0xBB (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (excluded)
pub fn fierce_leopard_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all directions except sideways
            // For Black: N (1) | NE (2) | SE (8) | S (16) | SW (32) | NW (128) = 187 = 0xBB
            // For White: S (16) | SW (32) | NW (128) | N (1) | NE (2) | SE (8) = 187 = 0xBB (adjusted automatically)
            MovementCapability::Simple {
                directions: 0xBB,  // All directions except E, W (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Water Ox movement: Simple 2 vertically, range in all other directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
/// All other directions: NE (2), SE (8), SW (32), NW (128), E (4), W (64) = 238 = 0xEE (same for both colors)
pub fn water_ox_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            MovementCapability::Simple {
                directions: 0x11,  // N, S (same for both colors)
                max_distance: 2,
            },
            // Range movement: all other directions (all except vertical)
            // NE (2), SE (8), SW (32), NW (128), E (4), W (64) = 238 = 0xEE (same for both colors)
            range_capability(
                0xEE,  // All directions except N, S (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Great Baku movement: Range in all directions, jump 3 sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// All 8 directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
pub fn great_baku_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_ALL;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: all directions
            // All 8 directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
            range_capability(
                DIRECTION_SET_ALL,  // All directions (same for both colors)
                BlockingMode::NoJump,
            ),
            // Jump movement: 3 spaces sideways
            // E (3, 0), W (-3, 0)
            MovementCapability::Jumping {
                offsets: vec![
                    (3, 0),   // E (same for both colors)
                    (-3, 0),  // W (same for both colors)
                ],
            },
        ],
    }
}

/// Dancing Stag movement: Simple 1 in all forwards directions and straight backwards, simple 2 sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// All forwards: For Black: N (1), NE (2), NW (128) = 131 = 0x83
///               For White: S (16), SE (8), SW (32) = 56 = 0x38 (adjusted automatically)
/// Straight backwards: For Black: S (16) = 16 = 0x10
///                      For White: N (1) = 1 = 0x01 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
pub fn dancing_stag_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all forwards directions and straight backwards
            // For Black: N (1) | NE (2) | NW (128) | S (16) = 147 = 0x93
            // For White: S (16) | SE (8) | SW (32) | N (1) = 57 = 0x39 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x93,  // N, NE, NW, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
        ],
    }
}

/// Square Mover movement: Range diagonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Diagonally: All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
pub fn square_mover_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: diagonally
            // All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
            range_capability(
                DIRECTION_SET_DIAGONAL,  // All diagonals (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Strong Chariot movement: Range movement orthogonally and forwards diagonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Orthogonally: N (1) | E (4) | S (16) | W (64) = 85 = 0x55 (same for both colors)
/// Forwards diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
pub fn strong_chariot_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_ORTHOGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: orthogonally
            // All 4 orthogonal directions: N (1) | E (4) | S (16) | W (64) = 85 = 0x55 (same for both colors)
            range_capability(
                DIRECTION_SET_ORTHOGONAL,  // All orthogonals (same for both colors)
                BlockingMode::NoJump,
            ),
            // Range movement: forwards diagonally
            // For Black: NE (2) | NW (128) = 130 = 0x82
            // For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
            range_capability(
                0x82,  // NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Old Rat movement: Simple 1 space straight forwards and backwards diagonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight forwards: For Black: N (1) = 1 = 0x01
///                    For White: S (16) = 16 = 0x10 (adjusted automatically)
/// Backwards diagonally: For Black: SE (8) | SW (32) = 40 = 0x28
///                      For White: NE (2) | NW (128) = 130 = 0x82 (adjusted automatically)
pub fn old_rat_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight forwards and backwards diagonally
            // For Black: N (1) | SE (8) | SW (32) = 41 = 0x29
            // For White: S (16) | NE (2) | NW (128) = 146 = 0x92 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x29,  // N, SE, SW (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Ji Bird movement: Range movement in all 3 forwards directions and straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// All 3 forwards: For Black: N (1) | NE (2) | NW (128) = 131 = 0x83
///                 For White: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
/// Straight backwards: For Black: S (16) = 16 = 0x10
///                    For White: N (1) = 1 = 0x01 (adjusted automatically)
pub fn ji_bird_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: all 3 forwards directions
            // For Black: N (1) | NE (2) | NW (128) = 131 = 0x83
            // For White: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
            range_capability(
                0x83,  // N, NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
            // Range movement: straight backwards
            // For Black: S (16) = 16 = 0x10
            // For White: N (1) = 1 = 0x01 (adjusted automatically)
            range_capability(
                0x10,  // S (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Blind Bear movement: Simple 1 space in all directions except vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// All directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
/// Vertically: N (1) | S (16) = 17 = 0x11
/// All except vertically: 0xFF & !0x11 = 0xEE = NE (2) | E (4) | SE (8) | SW (32) | W (64) | NW (128) = 238 = 0xEE (same for both colors)
pub fn blind_bear_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all directions except vertically
            // All diagonals and sideways: NE (2) | E (4) | SE (8) | SW (32) | W (64) | NW (128) = 238 = 0xEE (same for both colors)
            MovementCapability::Simple {
                directions: 0xEE,  // All except N, S (same for both colors)
                max_distance: 1,
            },
        ],
    }
}

/// Flying Stag movement: Same as Blind Bear, plus range movement vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// All directions except vertically: NE (2) | E (4) | SE (8) | SW (32) | W (64) | NW (128) = 238 = 0xEE (same for both colors)
/// Vertically: N (1) | S (16) = 17 = 0x11 (same for both colors)
pub fn flying_stag_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all directions except vertically (same as Blind Bear)
            // All diagonals and sideways: NE (2) | E (4) | SE (8) | SW (32) | W (64) | NW (128) = 238 = 0xEE (same for both colors)
            MovementCapability::Simple {
                directions: 0xEE,  // All except N, S (same for both colors)
                max_distance: 1,
            },
            // Range movement: vertically
            // N (1) | S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Side Mover movement: Simple 1 vertically, range sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
pub fn side_mover_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            MovementCapability::Simple {
                directions: 0x11,  // N, S (same for both colors)
                max_distance: 1,
            },
            // Range movement: sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            range_capability(
                0x44,  // E, W (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Side Flyer movement: Simple 1 space diagonally, range movement sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// Diagonally: NE (2) | SE (8) | SW (32) | NW (128) = 170 = 0xAA (same for both colors)
/// Sideways: E (4) | W (64) = 68 = 0x44 (same for both colors)
pub fn side_flyer_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space diagonally
            // All 4 diagonal directions: NE (2) | SE (8) | SW (32) | NW (128) = 170 = 0xAA (same for both colors)
            MovementCapability::Simple {
                directions: DIRECTION_SET_DIAGONAL,  // All diagonals (same for both colors)
                max_distance: 1,
            },
            // Range movement: sideways
            // E (4) | W (64) = 68 = 0x44 (same for both colors)
            range_capability(
                0x44,  // E, W (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Ox Chariot movement: Range movement straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight forwards: For Black: N (1) = 1 = 0x01
///                    For White: S (16) = 16 = 0x10 (adjusted automatically)
pub fn ox_chariot_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: straight forwards
            // For Black: N (1) = 1 = 0x01
            // For White: S (16) = 16 = 0x10 (adjusted automatically)
            range_capability(
                0x01,  // N (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Plodding Ox movement: Simple 1 space diagonally, range movement vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Diagonally: NE (2) | SE (8) | SW (32) | NW (128) = 170 = 0xAA (same for both colors)
/// Vertically: N (1) | S (16) = 17 = 0x11 (same for both colors)
pub fn plodding_ox_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space diagonally
            // All 4 diagonal directions: NE (2) | SE (8) | SW (32) | NW (128) = 170 = 0xAA (same for both colors)
            MovementCapability::Simple {
                directions: DIRECTION_SET_DIAGONAL,  // All diagonals (same for both colors)
                max_distance: 1,
            },
            // Range movement: vertically
            // N (1) | S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Blind Tiger movement: Simple 1 space in all directions except straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// All directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
/// Straight forwards: For Black: N (1) = 1 = 0x01
///                    For White: S (16) = 16 = 0x10 (adjusted automatically)
/// All except straight forwards: For Black: 0xFF & !0x01 = 0xFE = NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 254 = 0xFE
///                              For White: 0xFF & !0x10 = 0xEF = N (1) | NE (2) | E (4) | SE (8) | SW (32) | W (64) | NW (128) = 239 = 0xEF (adjusted automatically)
pub fn blind_tiger_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all directions except straight forwards
            // For Black: NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 254 = 0xFE
            // For White: N (1) | NE (2) | E (4) | SE (8) | SW (32) | W (64) | NW (128) = 239 = 0xEF (adjusted automatically)
            MovementCapability::Simple {
                directions: 0xFE,  // All except N (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Blind Monkey movement: Simple 1 space in all directions except vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// All directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
/// Vertically: N (1) | S (16) = 17 = 0x11
/// All except vertically: 0xFF & !0x11 = 0xEE = NE (2) | E (4) | SE (8) | SW (32) | W (64) | NW (128) = 238 = 0xEE (same for both colors)
pub fn blind_monkey_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all directions except vertically
            // All diagonals and sideways: NE (2) | E (4) | SE (8) | SW (32) | W (64) | NW (128) = 238 = 0xEE (same for both colors)
            MovementCapability::Simple {
                directions: 0xEE,  // All except N, S (same for both colors)
                max_distance: 1,
            },
        ],
    }
}

/// Left Howling Dog movement: Simple 1 straight backwards, range straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 16 = 0x10
///                      For White: N (1) = 1 = 0x01 (adjusted automatically)
/// Straight forwards: For Black: N (1) = 1 = 0x01
///                     For White: S (16) = 16 = 0x10 (adjusted automatically)
pub fn left_howling_dog_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16) = 16 = 0x10
            // For White: N (1) = 1 = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: straight forwards
            // For Black: N (1) = 1 = 0x01
            // For White: S (16) = 16 = 0x10 (adjusted automatically)
            range_capability(
                0x01,  // N (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Right Howling Dog movement: Simple 1 straight backwards, range straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 16 = 0x10
///                      For White: N (1) = 1 = 0x01 (adjusted automatically)
/// Straight forwards: For Black: N (1) = 1 = 0x01
///                     For White: S (16) = 16 = 0x10 (adjusted automatically)
pub fn right_howling_dog_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16) = 16 = 0x10
            // For White: N (1) = 1 = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: straight forwards
            // For Black: N (1) = 1 = 0x01
            // For White: S (16) = 16 = 0x10 (adjusted automatically)
            range_capability(
                0x01,  // N (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Left Dog movement: Same as howling dogs, plus range backwards right diagonal
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 16 = 0x10
///                      For White: N (1) = 1 = 0x01 (adjusted automatically)
/// Straight forwards: For Black: N (1) = 1 = 0x01
///                     For White: S (16) = 16 = 0x10 (adjusted automatically)
/// Backwards right diagonal: For Black: SE (8) = 8 = 0x08
///                            For White: NW (128) = 128 = 0x80 (adjusted automatically)
pub fn left_dog_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16) = 16 = 0x10
            // For White: N (1) = 1 = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: straight forwards
            // For Black: N (1) = 1 = 0x01
            // For White: S (16) = 16 = 0x10 (adjusted automatically)
            range_capability(
                0x01,  // N (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
            // Range movement: backwards right diagonal
            // For Black: SE (8) = 8 = 0x08
            // For White: NW (128) = 128 = 0x80 (adjusted automatically)
            range_capability(
                0x08,  // SE (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Right Dog movement: Same as howling dogs, plus range backwards left diagonal
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black: S (16) = 16 = 0x10
///                      For White: N (1) = 1 = 0x01 (adjusted automatically)
/// Straight forwards: For Black: N (1) = 1 = 0x01
///                     For White: S (16) = 16 = 0x10 (adjusted automatically)
/// Backwards left diagonal: For Black: SW (32) = 32 = 0x20
///                           For White: NE (2) = 2 = 0x02 (adjusted automatically)
pub fn right_dog_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16) = 16 = 0x10
            // For White: N (1) = 1 = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: straight forwards
            // For Black: N (1) = 1 = 0x01
            // For White: S (16) = 16 = 0x10 (adjusted automatically)
            range_capability(
                0x01,  // N (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
            // Range movement: backwards left diagonal
            // For Black: SW (32) = 32 = 0x20
            // For White: NE (2) = 2 = 0x02 (adjusted automatically)
            range_capability(
                0x20,  // SW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Dragon King movement: Range orthogonally, simple 1 diagonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Orthogonally: N (1), E (4), S (16), W (64) = 85 = 0x55 (same for both colors)
/// Diagonally: All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
pub fn dragon_king_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::{DIRECTION_SET_ORTHOGONAL, DIRECTION_SET_DIAGONAL};
    
    MovementConfig {
        capabilities: vec![
            // Range movement: orthogonally
            // N (1), E (4), S (16), W (64) = 85 = 0x55 (same for both colors)
            range_capability(
                DIRECTION_SET_ORTHOGONAL,  // All orthogonal directions (same for both colors)
                BlockingMode::NoJump,
            ),
            // Simple movement: 1 space diagonally
            // All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
            MovementCapability::Simple {
                directions: DIRECTION_SET_DIAGONAL,  // All diagonals (same for both colors)
                max_distance: 1,
            },
        ],
    }
}

/// Cloud Eagle movement: Simple 1 sideways, simple 3 forward diagonals, range vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Forward diagonals: For Black NE (2), NW (128) = 130 = 0x82, For White SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
pub fn cloud_eagle_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 1,
            },
            // Simple movement: up to 3 spaces in forward diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 3,
            },
            // Range movement: vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Strong Eagle movement: Range all 8 directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// All 8 directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
pub fn strong_eagle_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_ALL;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: all 8 directions
            // All 8 directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
            range_capability(
                DIRECTION_SET_ALL,  // All directions (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Stone Chariot movement: Simple 1 forwards diagonals, simple 2 sideways, range vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forward diagonals: For Black NE (2), NW (128) = 130 = 0x82, For White SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
pub fn stone_chariot_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in forward diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
            // Range movement: vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Walking Heron movement: Simple 2 sideways and forwards diagonals, range vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways and forward diagonals: For Black E (4) | W (64) | NE (2) | NW (128) = 198 = 0xC6, For White E (4) | W (64) | SE (8) | SW (32) = 108 = 0x6C (adjusted automatically)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
pub fn walking_heron_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces sideways and in forward diagonal directions
            // For Black: E (4) | W (64) | NE (2) | NW (128) = 198 = 0xC6
            // For White: E (4) | W (64) | SE (8) | SW (32) = 108 = 0x6C (adjusted automatically)
            MovementCapability::Simple {
                directions: 0xC6,  // E, W, NE, NW (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Range movement: vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Left General movement: 1 space in all 8 directions (same as Crown Prince)
pub fn left_general_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            MovementCapability::Simple {
                directions: DIRECTION_SET_ALL,
                max_distance: 1,
            },
        ],
    }
}

/// Right General movement: 1 space in all 8 directions (same as Crown Prince)
pub fn right_general_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            MovementCapability::Simple {
                directions: DIRECTION_SET_ALL,
                max_distance: 1,
            },
        ],
    }
}

/// Left Army movement: Range movement to left 3 directions (NW, W, SW), single space in other 5 (N, NE, E, SE, S)
pub fn left_army_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            // Range movement to left directions (NW, W, SW)
            range_capability(
                // NW (128) | W (64) | SW (32) = 224 = 0xE0
                0xE0,
                BlockingMode::NoJump,
            ),
            // Single space in other 5 directions (N, NE, E, SE, S)
            MovementCapability::Simple {
                // N (1) | NE (2) | E (4) | SE (8) | S (16) = 31 = 0x1F
                directions: 0x1F,
                max_distance: 1,
            },
        ],
    }
}

/// Right Army movement: Range movement to right 3 directions (NE, E, SE), single space in other 5 (N, NW, W, SW, S)
pub fn right_army_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            // Range movement to right directions (NE, E, SE)
            range_capability(
                // NE (2) | E (4) | SE (8) = 14 = 0x0E
                0x0E,
                BlockingMode::NoJump,
            ),
            // Single space in other 5 directions (N, NW, W, SW, S)
            MovementCapability::Simple {
                // N (1) | NW (128) | W (64) | SW (32) | S (16) = 241 = 0xF1
                directions: 0xF1,
                max_distance: 1,
            },
        ],
    }
}

/// Rear Standard movement: Range movement orthogonally, up to 2 spaces diagonally
pub fn rear_standard_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            // Unlimited range in orthogonal directions (N, S, E, W)
            range_capability(
                // N (1) | S (16) | E (4) | W (64) = 85 = 0x55
                0x55,
                BlockingMode::NoJump,
            ),
            // Up to 2 spaces in diagonal directions (NE, SE, SW, NW)
            MovementCapability::Simple {
                // NE (2) | SE (8) | SW (32) | NW (128) = 170 = 0xAA
                directions: 0xAA,
                max_distance: 2,
            },
        ],
    }
}

/// Center Standard movement: Range movement orthogonally, up to 3 spaces diagonally
pub fn center_standard_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            // Unlimited range in orthogonal directions (N, S, E, W)
            range_capability(
                // N (1) | S (16) | E (4) | W (64) = 85 = 0x55
                0x55,
                BlockingMode::NoJump,
            ),
            // Up to 3 spaces in diagonal directions (NE, SE, SW, NW)
            MovementCapability::Simple {
                // NE (2) | SE (8) | SW (32) | NW (128) = 170 = 0xAA
                directions: 0xAA,
                max_distance: 3,
            },
        ],
    }
}

/// Free King movement: Range movement in all 8 directions (like chess queen)
pub fn free_king_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            range_capability(DIRECTION_SET_ALL, BlockingMode::NoJump),
        ],
    }
}

/// Great General movement: Capturing range movement in all 8 directions
/// Captures all pieces in path (both enemy and friendly) but cannot land on friendly piece
/// Cannot jump over King, CrownPrince, or GreatGeneral (regardless of promotion status)
pub fn great_general_movement() -> MovementConfig {
    
    MovementConfig {
        capabilities: vec![
            range_capability_with_restrictions(
                DIRECTION_SET_ALL,
                BlockingMode::Capturing,
                blocking_set_1(),
            ),
        ],
    }
}

/// Free Baku movement: Range movement in all directions except sideways (E, W)
/// Sideways movement (E, W) is limited to 5 spaces
pub fn free_baku_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            // Unlimited range in all directions except E and W
            // N (1) | NE (2) | SE (8) | S (16) | SW (32) | NW (128) = 187 = 0xBB
            range_capability(0xBB, BlockingMode::NoJump),
            // Limited to 5 spaces in sideways directions (E, W)
            // E (4) | W (64) = 68 = 0x44
            MovementCapability::Simple {
                directions: 0x44,
                max_distance: 5,
            },
        ],
    }
}

/// Free Demon movement: Range movement in all directions except directly forwards and backwards
/// Directly forwards (N) and backwards (S) movement is limited to 5 spaces
/// Directions are relative to piece color (Black moves up, White moves down)
pub fn free_demon_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            // Unlimited range in all directions except N and S
            // For Black: NE (2) | E (4) | SE (8) | SW (32) | W (64) | NW (128) = 238 = 0xEE
            // For White: same (E and W are not color-dependent, diagonals adjusted automatically)
            range_capability(
                0xEE,  // NE, E, SE, SW, W, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

            // Limited to 5 spaces in directly forwards and backwards directions
            // For Black: N (1) | S (16) = 17 = 0x11
            // For White: S (16) | N (1) = 17 = 0x11 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x11,  // N, S (for Black) - will be adjusted for White
                max_distance: 5,
            },
        ],
    }
}

/// Running Horse movement: Range in all 3 forwards directions, 1 space backwards,
/// jump 2 spaces in backwards diagonal directions
/// Directions are relative to piece color (Black moves up, White moves down)
pub fn running_horse_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            // Range movement in all 3 forwards directions
            // For Black: N (1), NE (2), NW (128) = 131 = 0x83
            // For White: S (16), SE (8), SW (32) = 56 = 0x38 (adjusted automatically)
            range_capability(
                0x83,  // N, NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

            // Simple movement: 1 space directly backwards
            // For Black: S (16), For White: N (1) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (16) for Black, will be adjusted for White
                max_distance: 1,
            },
            // Jumping movement: exactly 2 spaces in backwards diagonal directions
            // For Black: SE (2, -2), SW (-2, -2)
            // For White: NE (2, 2), NW (-2, 2) (adjusted automatically)
            MovementCapability::Jumping {
                offsets: vec![
                    (2, -2),   // SE (for Black)
                    (-2, -2),  // SW (for Black)
                ],
            },
        ],
    }
}

/// Tengu movement: Can move once (range diagonal) or twice (two range diagonal moves)
/// In notation, two-step moves are written as two separate moves
pub fn tengu_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    use crate::movement::types::BlockingMode;
    
    // Single step: range diagonal movement
    let diagonal_range = range_capability(
        DIRECTION_SET_DIAGONAL,  // 0xAA = NE, SE, SW, NW
        BlockingMode::NoJump,
    );
    
    MovementConfig {
        capabilities: vec![
            // Single step: range diagonal movement
            diagonal_range.clone(),
            // Two-step: both steps are range diagonal movements
            MovementCapability::TwoStep {
                first: Box::new(diagonal_range.clone()),
                second: Box::new(diagonal_range),
            },
        ],
    }
}

/// Wooden Dove movement: Range diagonal, orthogonal up to 2 spaces, 
/// jump exactly 3 spaces diagonally, and conditional diagonal jumps of 4-5 spaces
pub fn wooden_dove_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in diagonal directions (unlimited)
            range_capability(
                DIRECTION_SET_DIAGONAL,  // 0xAA = NE, SE, SW, NW
                BlockingMode::NoJump,
            ),

            // Orthogonal movement up to 2 spaces
            MovementCapability::Simple {
                directions: 0x55,  // N, S, E, W
                max_distance: 2,
            },
            // Jump exactly 3 spaces diagonally
            MovementCapability::Jumping {
                offsets: vec![
                    (3, 3),   // NE
                    (3, -3),  // SE
                    (-3, -3), // SW
                    (-3, 3),  // NW
                ],
            },
            // Conditional diagonal jump: can jump 4-5 spaces if positions 1-2 have pieces and position 3 is empty
            MovementCapability::ConditionalDiagonalJump {
                directions: DIRECTION_SET_DIAGONAL,  // 0xAA = NE, SE, SW, NW
                base_jump: 3,  // Base jump is 3 spaces (handled by Jumping above, but included for consistency)
                conditional_jumps: vec![4, 5],  // Can jump 4 or 5 spaces conditionally
                required_jump_positions: 2,  // Positions 1-2 must have pieces
                empty_after_jump: 1,  // Position 3 must be empty
            },
        ],
    }
}

/// Ceramic Dove movement: Range diagonal, orthogonal up to 2 spaces
pub fn ceramic_dove_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in diagonal directions (unlimited)
            range_capability(
                DIRECTION_SET_DIAGONAL,  // 0xAA = NE, SE, SW, NW
                BlockingMode::NoJump,
            ),

            // Orthogonal movement up to 2 spaces
            MovementCapability::Simple {
                directions: 0x55,  // N, S, E, W
                max_distance: 2,
            },
        ],
    }
}

/// Earth Dragon movement: Range backwards diagonals, forward up to 2, forward diagonals 1, backwards 1
/// Directions are relative to piece color (Black moves up, White moves down)
pub fn earth_dragon_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in backwards diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            range_capability(
                0x28,  // SW, SE (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

            // Simple movement: up to 2 spaces directly forwards
            // For Black: N (1), For White: S (16) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (1) for Black, will be adjusted for White
                max_distance: 2,
            },
            // Simple movement: 1 space in forward diagonals and backwards
            // For Black: NE (2), NW (128), S (16) = 146 = 0x92
            // For White: SE (8), SW (32), N (1) = 41 = 0x29 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x92,  // NE, NW, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Rain Dragon movement (unpromoted/starting piece): Range sideways and backwards diagonals, 1 space in all 3 forwards directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Range in E (4), W (64), SW (32), SE (8) = 108 = 0x6C
/// For White: Range in E (4), W (64), NW (128), NE (2) = 198 = 0xC6 (adjusted automatically)
pub fn rain_dragon_unpromoted_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: sideways and backwards diagonal directions (NOT straight backwards)
            // For Black: E (4), W (64), SW (32), SE (8) = 108 = 0x6C
            // For White: E (4), W (64), NW (128), NE (2) = 198 = 0xC6 (adjusted automatically)
            range_capability(
                0x6C,  // E, W, SW, SE (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

            // Simple movement: 1 space in all 3 forwards directions
            // For Black: N (1), NE (2), NW (128) = 131 = 0x83
            // For White: S (16), SW (32), SE (8) = 56 = 0x38 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x83,  // N, NE, NW (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Rain Dragon movement (promoted from EarthDragon): Range sideways and all 3 backwards directions, 1 space in all 3 forwards directions
/// This version includes straight backwards in the range movement
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Range in E (4), W (64), S (16), SW (32), SE (8) = 124 = 0x7C
/// For White: Range in E (4), W (64), N (1), NE (2), NW (128) = 199 = 0xC7 (adjusted automatically)
pub fn rain_dragon_promoted_movement() -> MovementConfig {
    
    MovementConfig {
        capabilities: vec![
            // Range movement: sideways and all 3 backwards directions (including straight backwards)
            // For Black: E (4), W (64), S (16), SW (32), SE (8) = 124 = 0x7C
            // For White: E (4), W (64), N (1), NE (2), NW (128) = 199 = 0xC7 (adjusted automatically)
            range_capability(
                0x7C,  // E, W, S, SW, SE (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

            // Simple movement: 1 space in all 3 forwards directions
            // For Black: N (1), NE (2), NW (128) = 131 = 0x83
            // For White: S (16), SW (32), SE (8) = 56 = 0x38 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x83,  // N, NE, NW (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Left Mountain Eagle movement: Range in all directions except right backwards, simple 2 in right backwards, jump 2 in left diagonals
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: right backwards = SE (8), left diagonals = SW (32), NW (128)
/// For White: right backwards = NE (2), left diagonals = SE (8), NW (128) - adjusted automatically
pub fn left_mountain_eagle_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all directions EXCEPT right backwards
            // For Black: N (1) | NE (2) | E (4) | S (16) | SW (32) | W (64) | NW (128) = 247 = 0xF7
            // For White: adjusted automatically (excludes NE which is right backwards for White)
            range_capability(
                0xF7,  // All except SE (8) for Black - will be adjusted for White
                BlockingMode::NoJump,
            ),

            // Simple movement up to 2 spaces in right backwards direction
            // For Black: SE (8), For White: NE (2) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x08,  // SE (8) for Black - will be adjusted for White
                max_distance: 2,
            },
            // Jump exactly 2 spaces in left diagonal directions
            // For Black: SW (32), NW (128) = 160 = 0xA0
            // For White: SE (8), NW (128) = 136 = 0x88 (adjusted automatically)
            MovementCapability::Jumping {
                offsets: vec![
                    (-2, -2),  // SW for Black
                    (-2, 2),   // NW for Black
                ],
            },
        ],
    }
}

/// Right Mountain Eagle movement: Range in all directions except left backwards, simple 2 in left backwards, jump 2 in right diagonals
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: left backwards = SW (32), right diagonals = NE (2), SE (8)
/// For White: left backwards = NW (128), right diagonals = SW (32), SE (8) - adjusted automatically
pub fn right_mountain_eagle_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all directions EXCEPT left backwards
            // For Black: N (1) | NE (2) | E (4) | SE (8) | S (16) | W (64) | NW (128) = 223 = 0xDF
            // For White: adjusted automatically (excludes NW which is left backwards for White)
            range_capability(
                0xDF,  // All except SW (32) for Black - will be adjusted for White
                BlockingMode::NoJump,
            ),

            // Simple movement up to 2 spaces in left backwards direction
            // For Black: SW (32), For White: NW (128) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x20,  // SW (32) for Black - will be adjusted for White
                max_distance: 2,
            },
            // Jump exactly 2 spaces in right diagonal directions
            // For Black: NE (2), SE (8) = 10 = 0x0A
            // For White: SW (32), SE (8) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Jumping {
                offsets: vec![
                    (2, 2),    // NE for Black
                    (2, -2),   // SE for Black
                ],
            },
        ],
    }
}

/// Flying Eagle movement: Range in all 8 directions, jump 2 in forward diagonals
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: forward diagonals = NE (2), NW (128)
/// For White: forward diagonals = SE (8), SW (32) - adjusted automatically
pub fn flying_eagle_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_ALL;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all 8 directions
            range_capability(
                DIRECTION_SET_ALL,  // 0xFF = all 8 directions
                BlockingMode::NoJump,
            ),

            // Jump exactly 2 spaces in forward diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Jumping {
                offsets: vec![
                    (2, 2),    // NE for Black
                    (-2, 2),   // NW for Black
                ],
            },
        ],
    }
}

/// Fire Demon movement: Range in all directions except directly forwards and backwards, simple 2 in forwards/backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: forwards = N (1), backwards = S (16)
/// For White: forwards = S (16), backwards = N (1) - adjusted automatically
pub fn fire_demon_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all directions EXCEPT directly forwards and backwards
            // For Black: NE (2) | E (4) | SE (8) | SW (32) | W (64) | NW (128) = 238 = 0xEE
            // For White: adjusted automatically (excludes N and S)
            range_capability(
                0xEE,  // All except N (1) and S (16) for Black - will be adjusted for White
                BlockingMode::NoJump,
            ),

            // Simple movement up to 2 spaces in forwards and backwards directions
            // For Black: N (1), S (16) = 17 = 0x11
            // For White: S (16), N (1) = 17 = 0x11 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x11,  // N, S (for Black) - will be adjusted for White
                max_distance: 2,
            },
        ],
    }
}

/// Free Fire movement: Range in all directions except directly forwards and backwards, simple 5 in forwards/backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Same as Fire Demon but with max_distance 5 instead of 2
pub fn free_fire_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all directions EXCEPT directly forwards and backwards
            // For Black: NE (2) | E (4) | SE (8) | SW (32) | W (64) | NW (128) = 238 = 0xEE
            // For White: adjusted automatically (excludes N and S)
            range_capability(
                0xEE,  // All except N (1) and S (16) for Black - will be adjusted for White
                BlockingMode::NoJump,
            ),

            // Simple movement up to 5 spaces in forwards and backwards directions
            // For Black: N (1), S (16) = 17 = 0x11
            // For White: S (16), N (1) = 17 = 0x11 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x11,  // N, S (for Black) - will be adjusted for White
                max_distance: 5,
            },
        ],
    }
}

/// Whale movement (starting piece): Range in all 3 backwards directions and directly forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: forwards = N (1), backwards = S (16), SW (32), SE (8) = 56 = 0x38
/// Combined: N (1) | S (16) | SW (32) | SE (8) = 57 = 0x39
/// For White: adjusted automatically
pub fn whale_starting_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all 3 backwards directions and directly forwards
            // For Black: N (1) | S (16) | SW (32) | SE (8) = 57 = 0x39
            // For White: adjusted automatically
            range_capability(
                0x39,  // N, S, SW, SE (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Whale movement (promoted from ReverseChariot): Simple 1 forwards, range in all 3 backwards directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: forwards = N (1), backwards = S (16), SW (32), SE (8) = 56 = 0x38
/// For White: adjusted automatically
pub fn whale_promoted_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight forwards
            // For Black: N (1)
            // For White: S (16) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (1) for Black - will be adjusted for White
                max_distance: 1,
            },
            // Range movement in all 3 backwards directions
            // For Black: S (16), SW (32), SE (8) = 56 = 0x38
            // For White: N (1), NE (2), NW (128) = 131 = 0x83 (adjusted automatically)
            range_capability(
                0x38,  // S, SW, SE (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Great Whale movement: Range in all 3 forwards and all 3 backwards directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: forwards = N (1), NE (2), NW (128) = 131 = 0x83
/// For Black: backwards = S (16), SW (32), SE (8) = 56 = 0x38
/// Combined: 0x83 | 0x38 = 187 = 0xBB
/// For White: adjusted automatically
pub fn great_whale_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all 3 forwards and all 3 backwards directions
            // For Black: N (1) | NE (2) | NW (128) | S (16) | SW (32) | SE (8) = 187 = 0xBB
            // For White: adjusted automatically
            range_capability(
                0xBB,  // N, NE, NW, S, SW, SE (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Running Rabbit movement: Range in all 3 forwards directions, simple 1 in all 3 backwards directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: forwards = N (1), NE (2), NW (128) = 131 = 0x83
/// For Black: backwards = S (16), SW (32), SE (8) = 56 = 0x38
/// For White: adjusted automatically
pub fn running_rabbit_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all 3 forwards directions
            // For Black: N (1) | NE (2) | NW (128) = 131 = 0x83
            // For White: adjusted automatically
            range_capability(
                0x83,  // N, NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

            // Simple movement 1 space in all 3 backwards directions
            // For Black: S (16) | SW (32) | SE (8) = 56 = 0x38
            // For White: adjusted automatically
            MovementCapability::Simple {
                directions: 0x38,  // S, SW, SE (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Treacherous Fox movement: Range in all 6 forwards and backwards directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: forwards = N (1), NE (2), NW (128) = 131 = 0x83
/// For Black: backwards = S (16), SW (32), SE (8) = 56 = 0x38
/// Combined: 0x83 | 0x38 = 187 = 0xBB
/// For White: adjusted automatically
pub fn treacherous_fox_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all 6 forwards and backwards directions
            // For Black: N (1) | NE (2) | NW (128) | S (16) | SW (32) | SE (8) = 187 = 0xBB
            // For White: adjusted automatically
            range_capability(
                0xBB,  // N, NE, NW, S, SW, SE (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Mountain Crane movement: Range in all 8 directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// All 8 directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
/// For White: adjusted automatically
pub fn mountain_crane_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all 8 directions
            // For Black: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
            // For White: adjusted automatically
            range_capability(
                0xFF,  // All 8 directions (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Turtle-Snake movement: Range in forward-right and backward-left, simple 1 in other 6 directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: forward-right = NE (2), backward-left = SW (32) = 34 = 0x22
/// For Black: other 6 = N (1) | E (4) | SE (8) | S (16) | W (64) | NW (128) = 221 = 0xDD
/// For White: adjusted automatically
pub fn turtle_snake_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in forward-right and backward-left
            // For Black: NE (2) | SW (32) = 34 = 0x22
            // For White: adjusted automatically
            range_capability(
                0x22,  // NE, SW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

            // Simple movement 1 space in the other 6 directions
            // For Black: N (1) | E (4) | SE (8) | S (16) | W (64) | NW (128) = 221 = 0xDD
            // For White: adjusted automatically
            MovementCapability::Simple {
                directions: 0xDD,  // N, E, SE, S, W, NW (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Divine Turtle movement: Range in forward-right, backward-left, and backward-right, simple 1 in other 5 directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: forward-right = NE (2), backward-left = SW (32), backward-right = SE (8) = 42 = 0x2A
/// For Black: other 5 = N (1) | E (4) | S (16) | W (64) | NW (128) = 213 = 0xD5
/// For White: adjusted automatically
pub fn divine_turtle_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in forward-right, backward-left, and backward-right
            // For Black: NE (2) | SW (32) | SE (8) = 42 = 0x2A
            // For White: adjusted automatically
            range_capability(
                0x2A,  // NE, SW, SE (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

            // Simple movement 1 space in the other 5 directions
            // For Black: N (1) | E (4) | S (16) | W (64) | NW (128) = 213 = 0xD5
            // For White: adjusted automatically
            MovementCapability::Simple {
                directions: 0xD5,  // N, E, S, W, NW (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// White Tiger movement: Range in sideways and forward-right, simple 2 in forward/backward
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: sideways = E (4), W (64) = 68 = 0x44
/// For Black: forward-right = NE (2)
/// Combined range: E (4) | W (64) | NE (2) = 70 = 0x46
/// Simple forward/backward: N (1) | S (16) = 17 = 0x11
/// For White: adjusted automatically
pub fn white_tiger_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in sideways and forward-right
            // For Black: E (4) | W (64) | NE (2) = 70 = 0x46
            // For White: adjusted automatically
            range_capability(
                0x46,  // E, W, NE (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

            // Simple movement up to 2 spaces in forward and backward
            // For Black: N (1), S (16) = 17 = 0x11
            // For White: adjusted automatically
            MovementCapability::Simple {
                directions: 0x11,  // N, S (for Black) - will be adjusted for White
                max_distance: 2,
            },
        ],
    }
}

/// Divine Tiger movement: Range in sideways, forward-right, and forward, simple 2 in backward
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: sideways = E (4), W (64) = 68 = 0x44
/// For Black: forward-right = NE (2), forward = N (1)
/// Combined range: E (4) | W (64) | NE (2) | N (1) = 71 = 0x47
/// Simple backward: S (16) = 16 = 0x10
/// For White: adjusted automatically
pub fn divine_tiger_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in sideways, forward-right, and forward
            // For Black: E (4) | W (64) | NE (2) | N (1) = 71 = 0x47
            // For White: adjusted automatically
            range_capability(
                0x47,  // E, W, NE, N (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

            // Simple movement up to 2 spaces in backward
            // For Black: S (16) = 16 = 0x10
            // For White: adjusted automatically
            MovementCapability::Simple {
                directions: 0x10,  // S (16) for Black - will be adjusted for White
                max_distance: 2,
            },
        ],
    }
}

/// Lance movement: Range movement straight forward only
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: forward = N (1)
/// For White: forward = S (16) - adjusted automatically
pub fn lance_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement straight forward only
            // For Black: N (1)
            // For White: S (16) - adjusted automatically
            range_capability(
                0x01,  // N (1) for Black - will be adjusted for White
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// White Foal movement: Range in all 3 forwards directions and straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: forwards = N (1), NE (2), NW (128) = 131 = 0x83
/// For Black: backwards = S (16) = 16 = 0x10
/// Combined: 0x83 | 0x10 = 147 = 0x93
/// For White: adjusted automatically
pub fn white_foal_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all 3 forwards directions and straight backwards
            // For Black: N (1) | NE (2) | NW (128) | S (16) = 147 = 0x93
            // For White: adjusted automatically
            range_capability(
                0x93,  // N, NE, NW, S (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Great Foal movement: Same movement as White Foal, plus simple 2 sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// Range in all 3 forwards directions and straight backwards: For Black: N (1) | NE (2) | NW (128) | S (16) = 147 = 0x93
///                                                                    For White: adjusted automatically
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
pub fn great_foal_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all 3 forwards directions and straight backwards
            // For Black: N (1) | NE (2) | NW (128) | S (16) = 147 = 0x93
            // For White: adjusted automatically
            range_capability(
                0x93,  // N, NE, NW, S (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
            // Simple movement: up to 2 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
        ],
    }
}

/// Wood Chariot movement: Simple 1 forward left and backward right, range vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forward left: For Black: NW (128) = 128 = 0x80
///               For White: SE (8) = 8 = 0x08 (adjusted automatically)
/// Backward right: For Black: SE (8) = 8 = 0x08
///                 For White: NW (128) = 128 = 0x80 (adjusted automatically)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
pub fn wood_chariot_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space forward left and backward right
            // For Black: NW (128) | SE (8) = 136 = 0x88
            // For White: SE (8) | NW (128) = 136 = 0x88 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x88,  // NW, SE (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Wind Snapping Turtle movement: Simple 2 forwards diagonal, range vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forwards diagonal: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
pub fn wind_snapping_turtle_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces in forwards diagonal directions
            // For Black: NE (2) | NW (128) = 130 = 0x82
            // For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Range movement: vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Peng Master movement: Simple 5 sideways and backwards diagonals, range in other 4 directions, jump 3 forward diagonals
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Backwards diagonals: For Black: SE (8) | SW (32) = 40 = 0x28
///                       For White: NE (2) | NW (128) = 130 = 0x82 (adjusted automatically)
/// Other 4 directions (range): For Black: N (1) | NE (2) | NW (128) | S (16) = 147 = 0x93
///                              For White: S (16) | SE (8) | SW (32) | N (1) = 57 = 0x39 (adjusted automatically)
/// Forward diagonals (jump 3): For Black: NE (2) | NW (128) = 130 = 0x82
///                             For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
pub fn peng_master_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 5 spaces sideways and in backwards diagonal directions
            // Sideways: E (4) | W (64) = 68 = 0x44 (same for both colors)
            // Backwards diagonals: For Black: SE (8) | SW (32) = 40 = 0x28
            //                       For White: NE (2) | NW (128) = 130 = 0x82 (adjusted automatically)
            // Combined: 0x44 | 0x28 = 0x6C (for Black) - will be adjusted for White
            MovementCapability::Simple {
                directions: 0x6C,  // E, W, SE, SW (for Black) - will be adjusted for White
                max_distance: 5,
            },
            // Range movement: in the other 4 directions
            // For Black: N (1) | NE (2) | NW (128) | S (16) = 147 = 0x93
            // For White: S (16) | SE (8) | SW (32) | N (1) = 57 = 0x39 (adjusted automatically)
            range_capability(
                0x93,  // N, NE, NW, S (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
            // Jumping movement: 3 spaces in forward diagonal directions
            // For Black: NE (2) | NW (128) = 130 = 0x82
            // For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Jumping {
                offsets: vec![
                    // Forward right diagonal: 3 spaces
                    // For Black: (3, 3) = NE
                    // For White: (3, -3) = SE (adjusted automatically)
                    (3, 3),
                    // Forward left diagonal: 3 spaces
                    // For Black: (-3, 3) = NW
                    // For White: (-3, -3) = SW (adjusted automatically)
                    (-3, 3),
                ],
            },
        ],
    }
}

/// Center Master movement: Simple 3 sideways and backwards diagonals, range in other 4 directions, jump 2 in 3 forward directions and straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Backwards diagonals: For Black: SE (8) | SW (32) = 40 = 0x28
///                       For White: NE (2) | NW (128) = 130 = 0x82 (adjusted automatically)
/// Other 4 directions (range): For Black: N (1) | NE (2) | NW (128) | S (16) = 147 = 0x93
///                              For White: S (16) | SE (8) | SW (32) | N (1) = 57 = 0x39 (adjusted automatically)
/// Jump 2 in 3 forward directions and straight backwards: For Black: N (1) | NE (2) | NW (128) | S (16) = 147 = 0x93
///                                                          For White: S (16) | SE (8) | SW (32) | N (1) = 57 = 0x39 (adjusted automatically)
pub fn center_master_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 3 spaces sideways and in backwards diagonal directions
            // Sideways: E (4) | W (64) = 68 = 0x44 (same for both colors)
            // Backwards diagonals: For Black: SE (8) | SW (32) = 40 = 0x28
            //                       For White: NE (2) | NW (128) = 130 = 0x82 (adjusted automatically)
            // Combined: 0x44 | 0x28 = 0x6C (for Black) - will be adjusted for White
            MovementCapability::Simple {
                directions: 0x6C,  // E, W, SE, SW (for Black) - will be adjusted for White
                max_distance: 3,
            },
            // Range movement: in the other 4 directions
            // For Black: N (1) | NE (2) | NW (128) | S (16) = 147 = 0x93
            // For White: S (16) | SE (8) | SW (32) | N (1) = 57 = 0x39 (adjusted automatically)
            range_capability(
                0x93,  // N, NE, NW, S (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
            // Jumping movement: 2 spaces in 3 forward directions and straight backwards
            // For Black: N (1) | NE (2) | NW (128) | S (16) = 147 = 0x93
            // For White: S (16) | SE (8) | SW (32) | N (1) = 57 = 0x39 (adjusted automatically)
            MovementCapability::Jumping {
                offsets: vec![
                    // Straight forward: 2 spaces
                    // For Black: (0, 2) = N
                    // For White: (0, -2) = S (adjusted automatically)
                    (0, 2),
                    // Forward right diagonal: 2 spaces
                    // For Black: (2, 2) = NE
                    // For White: (2, -2) = SE (adjusted automatically)
                    (2, 2),
                    // Forward left diagonal: 2 spaces
                    // For Black: (-2, 2) = NW
                    // For White: (-2, -2) = SW (adjusted automatically)
                    (-2, 2),
                    // Straight backwards: 2 spaces
                    // For Black: (0, -2) = S
                    // For White: (0, 2) = N (adjusted automatically)
                    (0, -2),
                ],
            },
        ],
    }
}

/// Fierce Wolf movement: Same as Gold General (6 directions: forward, backward, sideways, forward diagonals)
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: N (1) | S (16) | E (4) | W (64) | NE (2) | NW (128) = 215 = 0xD7
/// For White: adjusted automatically
pub fn fierce_wolf_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            MovementCapability::Simple {
                // N, S, E, W, NE, NW (will be adjusted by color)
                // N (1) | S (16) | E (4) | W (64) | NE (2) | NW (128) = 215 = 0xD7
                directions: 0xD7,
                max_distance: 1,
            },
        ],
    }
}

/// Bear's Eyes movement: Simple 1 space in all 8 directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// All 8 directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
/// For White: adjusted automatically
pub fn bears_eyes_movement() -> MovementConfig {
    MovementConfig {
        capabilities: vec![
            MovementCapability::Simple {
                directions: DIRECTION_SET_ALL,  // All 8 directions
                max_distance: 1,
            },
        ],
    }
}

/// Eastern Barbarian movement: Simple 1 space sideways and forward diagonal, simple 2 spaces vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Forward diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// Vertically: N (1) | S (16) = 17 = 0x11 (same for both colors)
pub fn eastern_barbarian_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space sideways and forward diagonal
            // Sideways: E (4) | W (64) = 68 = 0x44 (same for both colors)
            // Forward diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
            //                     For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
            // Combined: 0x44 | 0x82 = 0xC6 (for Black) - will be adjusted for White
            MovementCapability::Simple {
                directions: 0xC6,  // E, W, NE, NW (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces vertically
            // N (1) | S (16) = 17 = 0x11 (same for both colors)
            MovementCapability::Simple {
                directions: 0x11,  // N, S (same for both colors)
                max_distance: 2,
            },
        ],
    }
}

/// Western Barbarian movement: Simple 1 space sideways and forward diagonal, simple 2 spaces vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Forward diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// Vertically: N (1) | S (16) = 17 = 0x11 (same for both colors)
pub fn western_barbarian_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space sideways and forward diagonal
            // Sideways: E (4) | W (64) = 68 = 0x44 (same for both colors)
            // Forward diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
            //                     For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
            // Combined: 0x44 | 0x82 = 0xC6 (for Black) - will be adjusted for White
            MovementCapability::Simple {
                directions: 0xC6,  // E, W, NE, NW (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces vertically
            // N (1) | S (16) = 17 = 0x11 (same for both colors)
            MovementCapability::Simple {
                directions: 0x11,  // N, S (same for both colors)
                max_distance: 2,
            },
        ],
    }
}

/// Southern Barbarian movement: Simple 1 space in all 3 forward directions and straight backward, simple 2 spaces sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// All 3 forwards: For Black: N (1) | NE (2) | NW (128) = 131 = 0x83
///                 For White: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
/// Straight backward: For Black: S (16) = 16 = 0x10
///                    For White: N (1) = 1 = 0x01 (adjusted automatically)
/// Combined forwards + backward: For Black: 0x83 | 0x10 = 147 = 0x93
///                                For White: 0x38 | 0x01 = 57 = 0x39 (adjusted automatically)
/// Sideways: E (4) | W (64) = 68 = 0x44 (same for both colors)
pub fn southern_barbarian_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all 3 forward directions and straight backward
            // For Black: N (1) | NE (2) | NW (128) | S (16) = 147 = 0x93
            // For White: S (16) | SE (8) | SW (32) | N (1) = 57 = 0x39 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x93,  // N, NE, NW, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces sideways
            // E (4) | W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
        ],
    }
}

/// Northern Barbarian movement: Simple 1 space in all 3 forward directions and straight backward, simple 2 spaces sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// All 3 forwards: For Black: N (1) | NE (2) | NW (128) = 131 = 0x83
///                 For White: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
/// Straight backward: For Black: S (16) = 16 = 0x10
///                    For White: N (1) = 1 = 0x01 (adjusted automatically)
/// Combined forwards + backward: For Black: 0x83 | 0x10 = 147 = 0x93
///                                For White: 0x38 | 0x01 = 57 = 0x39 (adjusted automatically)
/// Sideways: E (4) | W (64) = 68 = 0x44 (same for both colors)
pub fn northern_barbarian_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all 3 forward directions and straight backward
            // For Black: N (1) | NE (2) | NW (128) | S (16) = 147 = 0x93
            // For White: S (16) | SE (8) | SW (32) | N (1) = 57 = 0x39 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x93,  // N, NE, NW, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces sideways
            // E (4) | W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
        ],
    }
}

/// Lion Dog movement: Range movement in all 8 directions, jump 3 spaces in all 8 directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// All 8 directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
/// Jump: 3 spaces in all 8 directions
pub fn lion_dog_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_ALL;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: all directions
            // All 8 directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
            range_capability(
                DIRECTION_SET_ALL,  // All directions (same for both colors)
                BlockingMode::NoJump,
            ),
            // Jump movement: 3 spaces in all 8 directions
            // N (0, 3), NE (3, 3), E (3, 0), SE (3, -3), S (0, -3), SW (-3, -3), W (-3, 0), NW (-3, 3)
            MovementCapability::Jumping {
                offsets: vec![
                    (0, 3),    // N for Black
                    (3, 3),    // NE for Black
                    (3, 0),    // E (same for both colors)
                    (3, -3),   // SE for Black
                    (0, -3),   // S for Black
                    (-3, -3),  // SW for Black
                    (-3, 0),   // W (same for both colors)
                    (-3, 3),   // NW for Black
                ],
            },
        ],
    }
}

/// Beast Cadet movement: Simple 2 steps in all directions except directly backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: all directions except S (16)
/// So: N (1) | NE (2) | E (4) | SE (8) | SW (32) | W (64) | NW (128) = 239 = 0xEF
/// For White: adjusted automatically
pub fn beast_cadet_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement up to 2 steps in all directions except directly backwards
            // For Black: N (1) | NE (2) | E (4) | SE (8) | SW (32) | W (64) | NW (128) = 239 = 0xEF
            // For White: adjusted automatically (excludes N which is backwards for White)
            MovementCapability::Simple {
                directions: 0xEF,  // All except S (16) for Black - will be adjusted for White
                max_distance: 2,
            },
        ],
    }
}

/// Beast Officer movement: Simple 3 steps in forwards and backwards diagonals, simple 2 steps sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: forwards = N (1), NE (2), NW (128) = 131 = 0x83
/// For Black: backwards diagonals = SW (32), SE (8) = 40 = 0x28
/// For Black: sideways = E (4), W (64) = 68 = 0x44
/// For White: adjusted automatically
pub fn beast_officer_movement() -> MovementConfig {
    
    MovementConfig {
        capabilities: vec![
            // Simple movement up to 3 steps in all diagonals and forwards
            // For Black: N (1) | NE (2) | NW (128) | SW (32) | SE (8) = 171 = 0xAB
            // For White: adjusted automatically
            MovementCapability::Simple {
                directions: 0xAB,  // N, NE, NW, SW, SE (for Black) - will be adjusted for White
                max_distance: 3,
            },
            // Simple movement up to 2 steps in sideways directions
            // For Black: E (4) | W (64) = 68 = 0x44
            // For White: same (E and W are not color-dependent)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
        ],
    }
}

pub fn beast_bird_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement up to 2 steps straight backwards
            // For Black: S (16) = 0x10
            // For White: adjusted automatically
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Simple movement up to 3 steps sideways
            // For Black: E (4) | W (64) = 68 = 0x44
            // For White: same (E and W are not color-dependent)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 3,
            },
            // Range movement: all diagonals + straight forwards
            // For Black: NE (2) | NW (128) | SW (32) | SE (8) | N (1) = 171 = 0xAB
            // For White: adjusted automatically
            range_capability(
                0xAB,  // All diagonals and straight forward (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

        ],
    }
}

pub fn left_dragon_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement up to 2 steps straight left
            // For Black: W (64) = 0x40
            // For White: same (W is not color-dependent)
            MovementCapability::Simple {
                directions: 0x40,  // W (same for both colors)
                max_distance: 2,
            },
            // Range movement in all 3 rightward directions
            // For Black: E (4), NE (2), SE (8) = 14 = 0x0E
            // For White: adjusted automatically
            range_capability(
                0x0E,  // E, NE, SE (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

        ],
    }
}

pub fn right_dragon_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement up to 2 steps straight right
            // For Black: E (4) = 0x04
            // For White: same (E is not color-dependent)
            MovementCapability::Simple {
                directions: 0x04,  // E (same for both colors)
                max_distance: 2,
            },
            // Range movement in all 3 leftward directions
            // For Black: W (64), NW (128), SW (32) = 224 = 0xE0
            // For White: adjusted automatically
            range_capability(
                0xE0,  // W, NW, SW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

        ],
    }
}

pub fn vermillion_sparrow_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement 1 space orthogonally and in forwards rightward/backwards leftward
            // For Black: N (1), S (16), E (4), W (64), NE (2), SW (32) = 119 = 0x77
            // For White: N, S, NE, SW are adjusted automatically, E and W are same
            MovementCapability::Simple {
                directions: 0x77,  // N, S, E, W, NE, SW (N, S, NE, SW adjusted for White)
                max_distance: 1,
            },
            // Range movement in forwards leftward and backwards rightward
            // For Black: NW (128), SE (8) = 136 = 0x88
            // For White: adjusted automatically
            range_capability(
                0x88,  // NW, SE (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Divine Sparrow movement: Range backwards left, simple 1 orthogonally and forward-right, range forward-left and backward-right
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Simple in N (1), S (16), E (4), W (64), NE (2) = 87 = 0x57
///            Range in NW (128), SE (8), SW (32) = 168 = 0xA8
/// For White: adjusted automatically
pub fn divine_sparrow_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement 1 space orthogonally and in forwards rightward (backwards left is now range)
            // For Black: N (1), S (16), E (4), W (64), NE (2) = 87 = 0x57
            // For White: N, S, NE are adjusted automatically, E and W are same
            MovementCapability::Simple {
                directions: 0x57,  // N, S, E, W, NE (N, S, NE adjusted for White)
                max_distance: 1,
            },
            // Range movement in forwards leftward, backwards rightward, and backwards leftward
            // For Black: NW (128), SE (8), SW (32) = 168 = 0xA8
            // For White: adjusted automatically
            range_capability(
                0xA8,  // NW, SE, SW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

pub fn blue_dragon_movement() -> MovementConfig {
    use crate::movement::types::{BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement up to 2 steps sideways
            // For Black: E (4), W (64) = 68 = 0x44
            // For White: same (E and W are not color-dependent)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
            // Range movement vertically and forwards rightward
            // For Black: N (1), S (16), NE (2) = 19 = 0x13
            // For White: adjusted automatically
            range_capability(
                0x13,  // N, S, NE (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Divine Dragon movement: Range straight right, simple 2 left, range vertically and forward-right
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Range in E (4), N (1), S (16), NE (2) = 23 = 0x17
///            Simple 2 in W (64) = 0x40
/// For White: adjusted automatically
pub fn divine_dragon_movement() -> MovementConfig {
    use crate::movement::types::{BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement up to 2 steps left (W)
            // For Black: W (64) = 0x40
            // For White: same (W is not color-dependent)
            MovementCapability::Simple {
                directions: 0x40,  // W (same for both colors)
                max_distance: 2,
            },
            // Range movement straight right, vertically, and forward-right
            // For Black: E (4), N (1), S (16), NE (2) = 23 = 0x17
            // For White: adjusted automatically
            range_capability(
                0x17,  // E, N, S, NE (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

pub fn left_tiger_movement() -> MovementConfig {
    use crate::movement::types::{BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement 1 space in leftward diagonal directions
            // For Black: NW (128), SW (32) = 160 = 0xA0
            // For White: adjusted automatically
            MovementCapability::Simple {
                directions: 0xA0,  // NW, SW (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement in all 3 rightward directions
            // For Black: E (4), NE (2), SE (8) = 14 = 0x0E
            // For White: adjusted automatically
            range_capability(
                0x0E,  // E, NE, SE (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

        ],
    }
}

pub fn right_tiger_movement() -> MovementConfig {
    use crate::movement::types::{BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement 1 space in rightward diagonal directions
            // For Black: NE (2), SE (8) = 10 = 0x0A
            // For White: adjusted automatically
            MovementCapability::Simple {
                directions: 0x0A,  // NE, SE (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement in all 3 leftward directions
            // For Black: W (64), NW (128), SW (32) = 224 = 0xE0
            // For White: adjusted automatically
            range_capability(
                0xE0,  // W, NW, SW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Flying General movement: Range capturing movement orthogonally
/// Cannot jump over King, CrownPrince, GreatGeneral, FlyingGeneral, or FlyingCrocodile (regardless of promotion status)
/// Directions: N (1), S (16), E (4), W (64) = 85 = 0x55 (same for both colors)
pub fn flying_general_movement() -> MovementConfig {
    use crate::movement::types::{BlockingMode};
    use crate::movement::direction::DIRECTION_SET_ORTHOGONAL;
    
    let mut cannot_jump_over = HashSet::new();
    cannot_jump_over.insert(PieceType::King);
    cannot_jump_over.insert(PieceType::CrownPrince);
    cannot_jump_over.insert(PieceType::GreatGeneral);
    cannot_jump_over.insert(PieceType::FlyingGeneral);
    cannot_jump_over.insert(PieceType::FlyingCrocodile);
    
    MovementConfig {
        capabilities: vec![
            range_capability_with_restrictions(
                DIRECTION_SET_ORTHOGONAL,  // N, S, E, W = 0x55
                BlockingMode::Capturing,
                cannot_jump_over,
            ),
        ],
    }
}

/// Flying Crocodile movement: Range capturing movement orthogonally (same restrictions as Flying General),
/// plus simple movement up to 2 spaces in backwards diagonals and up to 3 spaces in forwards diagonals
/// Directions are relative to piece color (Black moves up, White moves down)
pub fn flying_crocodile_movement() -> MovementConfig {
    use crate::movement::types::{BlockingMode};
    use crate::movement::direction::DIRECTION_SET_ORTHOGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Range capturing movement orthogonally (same restrictions as Flying General)
            range_capability_with_restrictions(
                DIRECTION_SET_ORTHOGONAL,  // N, S, E, W = 0x55
                BlockingMode::Capturing,
                blocking_set_3(),
            ),
            // Simple movement up to 2 spaces in backwards diagonal directions
            // For Black: SW (32), NW (128) = 160 = 0xA0
            // For White: adjusted automatically
            MovementCapability::Simple {
                directions: 0xA0,  // SW, NW (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Simple movement up to 3 spaces in forwards diagonal directions
            // For Black: NE (2), SE (8) = 10 = 0x0A
            // For White: adjusted automatically
            MovementCapability::Simple {
                directions: 0x0A,  // NE, SE (for Black) - will be adjusted for White
                max_distance: 3,
            },
        ],
    }
}

/// Bishop General movement: Range capturing movement diagonally
/// Cannot jump over blocking set 3 pieces (regardless of promotion status)
/// Directions: NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
pub fn bishop_general_movement() -> MovementConfig {
    use crate::movement::types::{BlockingMode};
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    
    MovementConfig {
        capabilities: vec![
            range_capability_with_restrictions(
                DIRECTION_SET_DIAGONAL,  // NE, SE, SW, NW = 0xAA
                BlockingMode::Capturing,
                blocking_set_3(),
            ),
        ],
    }
}

/// Rain Demon movement: Simple movement up to 2 spaces sideways, up to 3 spaces straight forwards,
/// and regular (non-capturing) range movement backwards
/// Directions are relative to piece color (Black moves up, White moves down)
pub fn rain_demon_movement() -> MovementConfig {
    use crate::movement::types::{BlockingMode};
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces sideways and backwards diagonals
            // For Black: E (4), W (64), SW (32), SE (8) = 108 = 0x6C
            // For White: E (4), W (64), NW (128), NE (2) = 198 = 0xC6 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x6C,  // E, W, SW, SE (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Simple movement up to 3 spaces straight forwards
            // For Black: N (1)
            // For White: S (16) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (1) for Black - will be adjusted for White
                max_distance: 3,
            },
            // Regular (non-capturing) range movement backwards
            // For Black: S (16)
            // For White: N (1) (adjusted automatically)
            range_capability(
                0x10,  // S (16) for Black - will be adjusted for White
                BlockingMode::NoJump,
            ),
            // Jumping (not capturing) range movement: forwards diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            range_capability(
                0x82,  // NE, NW (for Black) - will be adjusted for White
                BlockingMode::Jump,  // Jumping (not capturing) range movement
            ),
        ],
    }
}

/// Kirin Master movement: Simple 3 sideways, range in other 6 directions, jump forward/backward 3 spaces
/// Directions are relative to piece color (Black moves up, White moves down)
pub fn kirin_master_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement up to 3 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 3,
            },
            // Range movement in other 6 directions (all except E, W)
            // N (1), NE (2), SE (8), S (16), SW (32), NW (128) = 187 = 0xBB
            range_capability(
                0xBB,  // All directions except E, W
                BlockingMode::NoJump,
            ),
            // Jump movement: forward exactly 3 spaces
            // For Black: N (0, 3)
            // For White: S (0, -3) (adjusted automatically)
            MovementCapability::Jumping {
                offsets: vec![
                    (0, 3),
                    (0, -3),
                ],
            },
        ],
    }
}

/// Phoenix Master movement: Simple 3 sideways, range in other 6 directions, jump forward diagonals 3 spaces
/// Directions are relative to piece color (Black moves up, White moves down)
pub fn phoenix_master_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement up to 3 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 3,
            },
            // Range movement in other 6 directions (all except E, W)
            // N (1), NE (2), SE (8), S (16), SW (32), NW (128) = 187 = 0xBB
            range_capability(
                0xBB,  // All directions except E, W
                BlockingMode::NoJump,
            ),
            // Jump movement: forward diagonals exactly 3 spaces
            // For Black: NE (3, 3), NW (-3, 3)
            // For White: SE (3, -3), SW (-3, -3) (adjusted automatically)
            MovementCapability::Jumping {
                offsets: vec![
                    (3, 3),    // NE for Black
                    (-3, 3),   // NW for Black
                ],
            },
        ],
    }
}

/// Copper General movement: Simple 1 in all 3 forwards directions and straight backward
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: forwards = N (1), NE (2), NW (128), backward = S (16) = 147 = 0x93
/// For White: adjusted automatically
pub fn copper_general_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all 3 forwards directions and straight backward
            // For Black: N (1), NE (2), NW (128), S (16) = 147 = 0x93
            // For White: S (16), SE (8), SW (32), N (1) = 57 = 0x39 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x93,  // N, NE, NW, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Horizontal Mover movement: Simple 1 vertically, range movement sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// Vertical: N (1), S (16) = 17 = 0x11 (same for both colors)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
pub fn horizontal_mover_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space vertically (forwards and backwards)
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            MovementCapability::Simple {
                directions: 0x11,  // N, S (same for both colors)
                max_distance: 1,
            },
            // Range movement sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            range_capability(
                0x44,  // E, W (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Fire Dragon movement: Backwards diagonals up to 2, forwards diagonals up to 4, range orthogonal
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Backwards diagonals SW (32), SE (8) = 40 = 0x28, Forwards diagonals NE (2), NW (128) = 130 = 0x82
/// For White: Backwards diagonals NW (128), NE (2) = 130 = 0x82, Forwards diagonals SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
pub fn fire_dragon_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces in backwards diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SW, SE (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Simple movement: up to 4 spaces in forwards diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 4,
            },
            // Range movement in all orthogonal directions
            // N (1), S (16), E (4), W (64) = 85 = 0x55 (same for both colors)
            range_capability(
                0x55,  // N, S, E, W (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Water Dragon movement: Forwards diagonals up to 2, backwards diagonals up to 4, range orthogonal
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Forwards diagonals NE (2), NW (128) = 130 = 0x82, Backwards diagonals SW (32), SE (8) = 40 = 0x28
/// For White: Forwards diagonals SE (8), SW (32) = 40 = 0x28, Backwards diagonals NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
pub fn water_dragon_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces in forwards diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Simple movement: up to 4 spaces in backwards diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SW, SE (for Black) - will be adjusted for White
                max_distance: 4,
            },
            // Range movement in all orthogonal directions
            // N (1), S (16), E (4), W (64) = 85 = 0x55 (same for both colors)
            range_capability(
                0x55,  // N, S, E, W (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Peacock movement: Simple 2 backwards diagonals, two-step: forward diagonal then any diagonal
/// Directions are relative to piece color (Black moves up, White moves down)
pub fn peacock_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    use crate::movement::types::BlockingMode;
    
    // Forward diagonal range movement (for first step)
    // For Black: NE (2), NW (128) = 130 = 0x82
    // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
    let forward_diagonal_range = range_capability(
        0x82,  // NE, NW (for Black) - will be adjusted for White
        BlockingMode::NoJump,
    );
    
    // Any diagonal range movement (for second step)
    let diagonal_range = range_capability(
        DIRECTION_SET_DIAGONAL,  // 0xAA = NE, SE, SW, NW (all diagonals)
        BlockingMode::NoJump,
    );
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces in backwards diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SW, SE (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Two-step: first move forward diagonal (range), second move any diagonal (range)
            MovementCapability::TwoStep {
                first: Box::new(forward_diagonal_range),
                second: Box::new(diagonal_range),
            },
        ],
    }
}

/// Old Kite movement: Simple 1 sideways, simple 2 diagonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Diagonals: NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
pub fn old_kite_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces diagonally
            // NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
            MovementCapability::Simple {
                directions: 0xAA,  // All diagonals (same for both colors)
                max_distance: 2,
            },
        ],
    }
}

/// Rushing Bird movement: Simple 1 in all directions except vertically, simple 2 straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// All directions except vertically: NE (2), E (4), SE (8), SW (32), W (64), NW (128) = 238 = 0xEE (same for both colors)
/// Straight forwards: N (1) for Black, S (16) for White (adjusted automatically)
pub fn rushing_bird_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all directions except vertically
            // NE (2), E (4), SE (8), SW (32), W (64), NW (128) = 238 = 0xEE (same for both colors)
            MovementCapability::Simple {
                directions: 0xEE,  // All directions except N, S (same for both colors)
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces straight forwards
            // For Black: N (1) = 0x01
            // For White: S (16) = 0x10 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (for Black) - will be adjusted for White
                max_distance: 2,
            },
        ],
    }
}

/// Free Pup movement: Simple 1 backwards diagonals, simple 2 sideways, range backwards and all 3 forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Backwards diagonals: For Black SW (32), SE (8) = 40 = 0x28; For White NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Straight backwards: For Black S (16), For White N (1) (adjusted automatically)
/// All 3 forwards: For Black N (1), NE (2), NW (128) = 131 = 0x83; For White S (16), SE (8), SW (32) = 56 = 0x38 (adjusted automatically)
pub fn free_pup_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in backwards diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SW, SE (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
            // Range movement: straight backwards and all 3 forwards directions
            // For Black: S (16), N (1), NE (2), NW (128) = 147 = 0x93
            // For White: N (1), S (16), SE (8), SW (32) = 57 = 0x39 (adjusted automatically)
            range_capability(
                0x93,  // S, N, NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Free Dog movement: Simple 2 backwards diagonals and sideways, range backwards and all 3 forwards
/// Same as Free Pup except backwards diagonals can move up to 2 spaces instead of 1
/// Directions are relative to piece color (Black moves up, White moves down)
pub fn free_dog_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces in backwards diagonal and sideways directions
            // For Black: SW (32), SE (8), E (4), W (64) = 108 = 0x6C
            // For White: NW (128), NE (2), E (4), W (64) = 198 = 0xC6 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x6C,  // SW, SE, E, W (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Range movement: straight backwards and all 3 forwards directions
            // For Black: S (16), N (1), NE (2), NW (128) = 147 = 0x93
            // For White: N (1), S (16), SE (8), SW (32) = 57 = 0x39 (adjusted automatically)
            range_capability(
                0x93,  // S, N, NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Wind Dragon movement: Simple 1 backward left, range in other 3 diagonals and sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// Backward left: For Black SW (32), For White NE (2) (adjusted automatically)
/// Other 3 diagonals: For Black NE (2), SE (8), NW (128) = 138 = 0x8A; For White NW (128), SE (8), SW (32) = 168 = 0xA8 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
pub fn wind_dragon_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in backward left direction
            // For Black: SW (32) = 0x20
            // For White: NE (2) = 0x02 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x20,  // SW (32) for Black - will be adjusted to NE (2) for White
                max_distance: 1,
            },
            // Range movement: other 3 diagonal directions and sideways
            // For Black: NE (2), SE (8), NW (128), E (4), W (64) = 206 = 0xCE
            // For White: NW (128), SE (8), SW (32), E (4), W (64) = 236 = 0xEC (adjusted automatically)
            range_capability(
                0xCE,  // NE, SE, NW, E, W (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Free Dragon movement: Range in all directions except straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: All except N (1) = 254 = 0xFE
/// For White: All except S (16) = 239 = 0xEF (adjusted automatically)
pub fn free_dragon_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all directions except straight forwards
            // For Black: All except N (1) = NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 254 = 0xFE
            // For White: All except S (16) = N (1) | NE (2) | E (4) | SE (8) | SW (32) | W (64) | NW (128) = 239 = 0xEF (adjusted automatically)
            range_capability(
                0xFE,  // All except N (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Running Wolf movement: Simple 1 straight forwards, range in forward diagonals and sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight forwards: For Black N (1), For White S (16) (adjusted automatically)
/// Forward diagonals and sideways: For Black NE (2), NW (128), E (4), W (64) = 198 = 0xC6
/// For White SE (8), SW (32), E (4), W (64) = 108 = 0x6C (adjusted automatically)
pub fn running_wolf_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight forwards
            // For Black: N (1) = 0x01
            // For White: S (16) = 0x10 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (1) for Black - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: forward diagonals and sideways
            // For Black: NE (2), NW (128), E (4), W (64) = 198 = 0xC6
            // For White: SE (8), SW (32), E (4), W (64) = 108 = 0x6C (adjusted automatically)
            range_capability(
                0xC6,  // NE, NW, E, W (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Free Wolf movement: Range in all 3 forwards and both sideways directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: N (1), NE (2), NW (128), E (4), W (64) = 199 = 0xC7
/// For White: S (16), SE (8), SW (32), E (4), W (64) = 124 = 0x7C (adjusted automatically)
pub fn free_wolf_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all 3 forwards and both sideways directions
            // For Black: N (1), NE (2), NW (128), E (4), W (64) = 199 = 0xC7
            // For White: S (16), SE (8), SW (32), E (4), W (64) = 124 = 0x7C (adjusted automatically)
            range_capability(
                0xC7,  // N, NE, NW, E, W (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Running Stag movement: Simple 2 straight backwards, range in sideways and forward diagonals
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black S (16), For White N (1) (adjusted automatically)
/// Sideways and forward diagonals: For Black E (4), W (64), NE (2), NW (128) = 198 = 0xC6
/// For White E (4), W (64), SE (8), SW (32) = 108 = 0x6C (adjusted automatically)
pub fn running_stag_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces straight backwards
            // For Black: S (16) = 0x10
            // For White: N (1) = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (16) for Black - will be adjusted for White
                max_distance: 2,
            },
            // Range movement: sideways and forward diagonal directions
            // For Black: E (4), W (64), NE (2), NW (128) = 198 = 0xC6
            // For White: E (4), W (64), SE (8), SW (32) = 108 = 0x6C (adjusted automatically)
            range_capability(
                0xC6,  // E, W, NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Free Stag movement: Range in all directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// All 8 directions: N (1), NE (2), E (4), SE (8), S (16), SW (32), W (64), NW (128) = 255 = 0xFF
pub fn free_stag_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all 8 directions
            // All directions: N (1), NE (2), E (4), SE (8), S (16), SW (32), W (64), NW (128) = 255 = 0xFF
            range_capability(
                DIRECTION_SET_ALL,  // All 8 directions (0xFF)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Side Dragon movement: Range sideways and straight forward
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: N (1), E (4), W (64) = 69 = 0x45
/// For White: S (16), E (4), W (64) = 84 = 0x54 (adjusted automatically)
pub fn side_dragon_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: sideways and straight forward
            // For Black: N (1), E (4), W (64) = 69 = 0x45
            // For White: S (16), E (4), W (64) = 84 = 0x54 (adjusted automatically)
            range_capability(
                0x45,  // N, E, W (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Running Dragon movement: Simple 5 straight backwards, range in all other directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black S (16), For White N (1) (adjusted automatically)
/// All other directions: For Black all except S (16) = N (1), NE (2), E (4), SE (8), SW (32), W (64), NW (128) = 239 = 0xEF
/// For White all except N (1) = NE (2), E (4), SE (8), S (16), SW (32), W (64), NW (128) = 254 = 0xFE (adjusted automatically)
pub fn running_dragon_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 5 spaces straight backwards
            // For Black: S (16) = 0x10
            // For White: N (1) = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (16) for Black - will be adjusted for White
                max_distance: 5,
            },
            // Range movement: all other directions (all except straight backwards)
            // For Black: all except S (16) = N (1), NE (2), E (4), SE (8), SW (32), W (64), NW (128) = 239 = 0xEF
            // For White: all except N (1) = NE (2), E (4), SE (8), S (16), SW (32), W (64), NW (128) = 254 = 0xFE (adjusted automatically)
            range_capability(
                0xEF,  // All except S (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Golden Chariot movement: Simple 1 diagonal, simple 2 sideways, range vertical
/// Directions are relative to piece color (Black moves up, White moves down)
/// Diagonally: All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
pub fn golden_chariot_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space diagonally (all 4 diagonals)
            MovementCapability::Simple {
                directions: DIRECTION_SET_DIAGONAL,  // All diagonals (same for both colors)
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
            // Range movement: vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Playful Parrot movement: Simple 2 backwards diagonal, simple 3 forwards diagonal, simple 5 sideways, range vertical
/// Directions are relative to piece color (Black moves up, White moves down)
/// Backwards diagonal: For Black SW (32), SE (8) = 40 = 0x28, For White NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
/// Forwards diagonal: For Black NE (2), NW (128) = 130 = 0x82, For White SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
pub fn playful_parrot_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces in backwards diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SW, SE (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Simple movement: up to 3 spaces in forwards diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 3,
            },
            // Simple movement: up to 5 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 5,
            },
            // Range movement: vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Vice General movement: Capturing range diagonally (blocking set 2), jump exactly 2 spaces orthogonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Capturing range: All 4 diagonal directions (NE, SE, SW, NW) = 170 = 0xAA (same for both colors)
/// Cannot jump over blocking set 2: King, CrownPrince, GreatGeneral, ViceGeneral
/// Jump: Exactly 2 spaces in all 4 orthogonal directions (N, S, E, W) = 85 = 0x55 (same for both colors)
pub fn vice_general_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Capturing range movement: all diagonal directions, constrained by blocking set 2
            range_capability_with_restrictions(
                DIRECTION_SET_DIAGONAL,  // All 4 diagonals (same for both colors)
                BlockingMode::Capturing,
                blocking_set_2(),
            ),
            // Jumping movement: exactly 2 spaces in all 4 orthogonal directions
            MovementCapability::Jumping {
                offsets: vec![
                    (0, 2),   // N (2 spaces up)
                    (0, -2),  // S (2 spaces down)
                    (2, 0),   // E (2 spaces right)
                    (-2, 0),  // W (2 spaces left)
                ],
            },
        ],
    }
}

/// Woodland Demon movement: Simple 2 sideways and backwards diagonals, range in other 4 directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Simple 2: Sideways and backwards diagonals
/// For Black: E (4), W (64), SW (32), SE (8) = 108 = 0x6C
/// For White: E (4), W (64), NW (128), NE (2) = 198 = 0xC6 (adjusted automatically)
/// Other 4 directions (straight forwards, straight backwards, forward diagonals): 
/// For Black: N (1), S (16), NE (2), NW (128) = 147 = 0x93
/// For White: S (16), N (1), SE (8), SW (32) = 57 = 0x39 (adjusted automatically)
pub fn woodland_demon_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces sideways and backwards diagonals
            // For Black: E (4), W (64), SW (32), SE (8) = 108 = 0x6C
            // For White: E (4), W (64), NW (128), NE (2) = 198 = 0xC6 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x6C,  // E, W, SW, SE (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Range movement: other 4 directions (straight forwards, straight backwards, forward diagonals)
            // For Black: N (1), S (16), NE (2), NW (128) = 147 = 0x93
            // For White: S (16), N (1), SE (8), SW (32) = 57 = 0x39 (adjusted automatically)
            range_capability(
                0x93,  // N, S, NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Old Peng movement: Simple 5 sideways, range diagonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Diagonally: All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
pub fn old_peng_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 5 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 5,
            },
            // Range movement: diagonally (all 4 diagonals)
            range_capability(
                DIRECTION_SET_DIAGONAL,  // All diagonals (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Fierce Dragon movement: Capturing range diagonally (blocking set 3), simple 2 orthogonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Capturing range: All 4 diagonal directions (NE, SE, SW, NW) = 170 = 0xAA (same for both colors)
/// Cannot jump over blocking set 3: King, CrownPrince, GreatGeneral, FlyingGeneral, FlyingCrocodile, BishopGeneral, ViceGeneral, FierceDragon
/// Simple 2: Orthogonal directions (N, S, E, W) = 85 = 0x55 (same for both colors)
pub fn fierce_dragon_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    use crate::movement::direction::DIRECTION_SET_ORTHOGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Capturing range movement: all diagonal directions, constrained by blocking set 3
            range_capability_with_restrictions(
                DIRECTION_SET_DIAGONAL,  // All 4 diagonals (same for both colors)
                BlockingMode::Capturing,
                blocking_set_3(),
            ),
            // Simple movement: up to 2 spaces orthogonally
            // N (1), S (16), E (4), W (64) = 85 = 0x55 (same for both colors)
            MovementCapability::Simple {
                directions: DIRECTION_SET_ORTHOGONAL,  // All orthogonals (same for both colors)
                max_distance: 2,
            },
        ],
    }
}

/// Fowl Cadet movement: Simple 3 in all directions except directly backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: All except S (16) = N (1), NE (2), E (4), SE (8), SW (32), W (64), NW (128) = 239 = 0xEF
/// For White: All except N (1) = NE (2), E (4), SE (8), S (16), SW (32), W (64), NW (128) = 254 = 0xFE (adjusted automatically)
pub fn fowl_cadet_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 3 spaces in all directions except directly backwards
            // For Black: All except S (16) = N (1), NE (2), E (4), SE (8), SW (32), W (64), NW (128) = 239 = 0xEF
            // For White: All except N (1) = NE (2), E (4), SE (8), S (16), SW (32), W (64), NW (128) = 254 = 0xFE (adjusted automatically)
            MovementCapability::Simple {
                directions: 0xEF,  // All except S (for Black) - will be adjusted for White
                max_distance: 3,
            },
        ],
    }
}

/// Lion movement: Jump to all squares 2 steps away, two-step: 1 space any direction then 1 space any direction
/// Jump offsets: Either coordinate changes by exactly 2, and the other changes by at most 2
/// Two-step: First move 1 space in any direction, second move 1 space in any direction
pub fn lion_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::direction::DIRECTION_SET_ALL;
    
    // Jump offsets: all positions where either file_delta or rank_delta is exactly 2 or -2,
    // and the other is at most 2 in absolute value
    // This gives us: (2, -2), (2, -1), (2, 0), (2, 1), (2, 2), (-2, -2), (-2, -1), (-2, 0), (-2, 1), (-2, 2),
    //                (-2, 2), (-1, 2), (0, 2), (1, 2), (-2, -2), (-1, -2), (0, -2), (1, -2)
    // After removing duplicates: (2, -2), (2, -1), (2, 0), (2, 1), (2, 2), (-2, -2), (-2, -1), (-2, 0), (-2, 1), (-2, 2),
    //                            (-1, 2), (0, 2), (1, 2), (-1, -2), (0, -2), (1, -2)
    let jump_offsets = vec![
        (2, -2), (2, -1), (2, 0), (2, 1), (2, 2),
        (-2, -2), (-2, -1), (-2, 0), (-2, 1), (-2, 2),
        (-1, 2), (0, 2), (1, 2),
        (-1, -2), (0, -2), (1, -2),
    ];
    
    // Two-step: first move 1 space in any direction, second move 1 space in any direction
    let first_step = MovementCapability::Simple {
        directions: DIRECTION_SET_ALL,  // All 8 directions
        max_distance: 1,
    };
    let second_step = MovementCapability::Simple {
        directions: DIRECTION_SET_ALL,  // All 8 directions
        max_distance: 1,
    };
    
    MovementConfig {
        capabilities: vec![
            // Jump movement: all squares 2 steps away
            MovementCapability::Jumping {
                offsets: jump_offsets,
            },
            // Two-step: 1 space any direction, then 1 space any direction
            MovementCapability::TwoStep {
                first: Box::new(first_step),
                second: Box::new(second_step),
            },
        ],
    }
}

/// Lion Hawk movement: Jump 2 steps away (same as Lion), two-step: first move 1 space orthogonally OR range diagonal, second move 1 space any direction
/// Jump offsets: Same as Lion (all squares 2 steps away)
/// Two-step: First move can be either:
///   - Simple 1 space orthogonally (N, S, E, W), OR
///   - Range movement diagonally (NE, SE, SW, NW)
///   Second move: 1 space in any direction
pub fn lion_hawk_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::direction::{DIRECTION_SET_ALL, DIRECTION_SET_ORTHOGONAL, DIRECTION_SET_DIAGONAL};
    use crate::movement::types::BlockingMode;
    
    // Jump offsets: same as Lion
    let jump_offsets = vec![
        (2, -2), (2, -1), (2, 0), (2, 1), (2, 2),
        (-2, -2), (-2, -1), (-2, 0), (-2, 1), (-2, 2),
        (-1, 2), (0, 2), (1, 2),
        (-1, -2), (0, -2), (1, -2),
    ];
    
    // Second step: 1 space in any direction
    let second_step = MovementCapability::Simple {
        directions: DIRECTION_SET_ALL,  // All 8 directions
        max_distance: 1,
    };
    
    // First step option 1: Simple 1 space orthogonally
    let first_step_orthogonal = MovementCapability::Simple {
        directions: DIRECTION_SET_ORTHOGONAL,  // N, S, E, W
        max_distance: 1,
    };
    
    // First step option 2: Range movement diagonally
    let first_step_diagonal_range = range_capability(
        DIRECTION_SET_DIAGONAL,  // NE, SE, SW, NW
        BlockingMode::NoJump,
    );
    
    MovementConfig {
        capabilities: vec![
            // Jump movement: all squares 2 steps away (same as Lion)
            MovementCapability::Jumping {
                offsets: jump_offsets,
            },
            // Two-step option 1: first move 1 space orthogonally, second move 1 space any direction
            MovementCapability::TwoStep {
                first: Box::new(first_step_orthogonal),
                second: Box::new(second_step.clone()),
            },
            // Two-step option 2: first move range diagonal, second move 1 space any direction
            MovementCapability::TwoStep {
                first: Box::new(first_step_diagonal_range),
                second: Box::new(second_step),
            },
        ],
    }
}

/// Reclining Dragon movement: Simple 1 space orthogonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Orthogonally: N (1), E (4), S (16), W (64) = 85 = 0x55 (same for both colors)
pub fn reclining_dragon_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::direction::DIRECTION_SET_ORTHOGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space orthogonally
            // N (1), E (4), S (16), W (64) = 85 = 0x55 (same for both colors)
            MovementCapability::Simple {
                directions: DIRECTION_SET_ORTHOGONAL,  // N, S, E, W (same for both colors)
                max_distance: 1,
            },
        ],
    }
}

/// Coiled Serpent movement: Simple 1 space straight forwards and all 3 backwards directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight forwards: For Black: N (1) = 1 = 0x01
///                    For White: S (16) = 16 = 0x10 (adjusted automatically)
/// All 3 backwards: For Black: S (16) | SE (8) | SW (32) = 56 = 0x38
///                   For White: N (1) | NE (2) | NW (128) = 131 = 0x83 (adjusted automatically)
/// Combined: For Black: 0x01 | 0x38 = 57 = 0x39
///           For White: 0x10 | 0x83 = 147 = 0x93 (adjusted automatically)
pub fn coiled_serpent_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight forwards and all 3 backwards directions
            // For Black: N (1) | S (16) | SE (8) | SW (32) = 57 = 0x39
            // For White: S (16) | N (1) | NE (2) | NW (128) = 147 = 0x93 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x39,  // N, S, SE, SW (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Coiled Dragon movement: Range movement in straight forwards and all 3 backwards directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight forwards: For Black: N (1) = 1 = 0x01
///                    For White: S (16) = 16 = 0x10 (adjusted automatically)
/// All 3 backwards: For Black: S (16) | SE (8) | SW (32) = 56 = 0x38
///                   For White: N (1) | NE (2) | NW (128) = 131 = 0x83 (adjusted automatically)
/// Combined: For Black: 0x01 | 0x38 = 57 = 0x39
///           For White: 0x10 | 0x83 = 147 = 0x93 (adjusted automatically)
pub fn coiled_dragon_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: straight forwards and all 3 backwards directions
            // For Black: N (1) | S (16) | SE (8) | SW (32) = 57 = 0x39
            // For White: S (16) | N (1) | NE (2) | NW (128) = 147 = 0x93 (adjusted automatically)
            range_capability(
                0x39,  // N, S, SE, SW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Huai Chicken movement: Simple 1 space forward diagonal, sideways, and straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forward diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// Sideways: E (4) | W (64) = 68 = 0x44 (same for both colors)
/// Straight backwards: For Black: S (16) = 16 = 0x10
///                    For White: N (1) = 1 = 0x01 (adjusted automatically)
/// Combined: For Black: 0x82 | 0x44 | 0x10 = 214 = 0xD6
///           For White: 0x28 | 0x44 | 0x01 = 109 = 0x6D (adjusted automatically)
pub fn huai_chicken_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space forward diagonal, sideways, and straight backwards
            // For Black: NE (2) | NW (128) | E (4) | W (64) | S (16) = 214 = 0xD6
            // For White: SE (8) | SW (32) | E (4) | W (64) | N (1) = 109 = 0x6D (adjusted automatically)
            MovementCapability::Simple {
                directions: 0xD6,  // NE, NW, E, W, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Wizard Stork movement: Range movement in forward diagonal, sideways, and straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forward diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// Sideways: E (4) | W (64) = 68 = 0x44 (same for both colors)
/// Straight backwards: For Black: S (16) = 16 = 0x10
///                    For White: N (1) = 1 = 0x01 (adjusted automatically)
/// Combined: For Black: 0x82 | 0x44 | 0x10 = 214 = 0xD6
///           For White: 0x28 | 0x44 | 0x01 = 109 = 0x6D (adjusted automatically)
pub fn wizard_stork_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: forward diagonal, sideways, and straight backwards
            // For Black: NE (2) | NW (128) | E (4) | W (64) | S (16) = 214 = 0xD6
            // For White: SE (8) | SW (32) | E (4) | W (64) | N (1) = 109 = 0x6D (adjusted automatically)
            range_capability(
                0xD6,  // NE, NW, E, W, S (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Old Monkey movement: Simple 1 space diagonally and straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Diagonally: NE (2) | SE (8) | SW (32) | NW (128) = 170 = 0xAA (same for both colors)
/// Straight backwards: For Black: S (16) = 16 = 0x10
///                    For White: N (1) = 1 = 0x01 (adjusted automatically)
/// Combined: For Black: 0xAA | 0x10 = 186 = 0xBA
///           For White: 0xAA | 0x01 = 171 = 0xAB (adjusted automatically)
pub fn old_monkey_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space diagonally and straight backwards
            // For Black: NE (2) | SE (8) | SW (32) | NW (128) | S (16) = 186 = 0xBA
            // For White: NE (2) | SE (8) | SW (32) | NW (128) | N (1) = 171 = 0xAB (adjusted automatically)
            MovementCapability::Simple {
                directions: 0xBA,  // All diagonals + S (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Mountain Witch movement: Range movement diagonally and straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Diagonally: NE (2) | SE (8) | SW (32) | NW (128) = 170 = 0xAA (same for both colors)
/// Straight backwards: For Black: S (16) = 16 = 0x10
///                    For White: N (1) = 1 = 0x01 (adjusted automatically)
/// Combined: For Black: 0xAA | 0x10 = 186 = 0xBA
///           For White: 0xAA | 0x01 = 171 = 0xAB (adjusted automatically)
pub fn mountain_witch_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: diagonally and straight backwards
            // For Black: NE (2) | SE (8) | SW (32) | NW (128) | S (16) = 186 = 0xBA
            // For White: NE (2) | SE (8) | SW (32) | NW (128) | N (1) = 171 = 0xAB (adjusted automatically)
            range_capability(
                0xBA,  // All diagonals + S (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Flying Chicken movement: Simple 1 space forward diagonal and sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forward diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// Sideways: E (4) | W (64) = 68 = 0x44 (same for both colors)
/// Combined: For Black: 0x82 | 0x44 = 214 = 0xD6
///           For White: 0x28 | 0x44 = 108 = 0x6C (adjusted automatically)
pub fn flying_chicken_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space forward diagonal and sideways
            // For Black: NE (2) | NW (128) | E (4) | W (64) = 214 = 0xD6
            // For White: SE (8) | SW (32) | E (4) | W (64) = 108 = 0x6C (adjusted automatically)
            MovementCapability::Simple {
                directions: 0xD6,  // NE, NW, E, W (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Raiding Hawk movement: Same as Flying Chicken, plus range movement straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forward diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// Sideways: E (4) | W (64) = 68 = 0x44 (same for both colors)
/// Straight forwards: For Black: N (1) = 1 = 0x01
///                    For White: S (16) = 16 = 0x10 (adjusted automatically)
pub fn raiding_hawk_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space forward diagonal and sideways (same as Flying Chicken)
            // For Black: NE (2) | NW (128) | E (4) | W (64) = 214 = 0xD6
            // For White: SE (8) | SW (32) | E (4) | W (64) = 108 = 0x6C (adjusted automatically)
            MovementCapability::Simple {
                directions: 0xD6,  // NE, NW, E, W (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: straight forwards
            // For Black: N (1) = 1 = 0x01
            // For White: S (16) = 16 = 0x10 (adjusted automatically)
            range_capability(
                0x01,  // N (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Wind Horse movement: Simple 1 space forward diagonal, simple 2 spaces straight backwards, range straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forward diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                    For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
/// Straight backwards: For Black: S (16) = 16 = 0x10
///                    For White: N (1) = 1 = 0x01 (adjusted automatically)
/// Straight forwards: For Black: N (1) = 1 = 0x01
///                    For White: S (16) = 16 = 0x10 (adjusted automatically)
pub fn wind_horse_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space forward diagonal
            // For Black: NE (2) | NW (128) = 130 = 0x82
            // For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces straight backwards
            // For Black: S (16) = 16 = 0x10
            // For White: N (1) = 1 = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Range movement: straight forwards
            // For Black: N (1) = 1 = 0x01
            // For White: S (16) = 16 = 0x10 (adjusted automatically)
            range_capability(
                0x01,  // N (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Heavenly Horse movement: Range movement straight forwards, plus 4 knight movements (forwards and backwards, jumping)
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight forwards: For Black: N (1) = 1 = 0x01
///                    For White: S (16) = 16 = 0x10 (adjusted automatically)
/// Knight movements: 2 spaces in one direction, 1 space perpendicular
/// Forward knight: (1, 2) and (-1, 2) for Black
/// Backward knight: (1, -2) and (-1, -2) for Black
pub fn heavenly_horse_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: straight forwards
            // For Black: N (1) = 1 = 0x01
            // For White: S (16) = 16 = 0x10 (adjusted automatically)
            range_capability(
                0x01,  // N (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
            // Jumping movement: 4 knight movements (forwards and backwards)
            // Forward knight: 2 forward, 1 sideways = (1, 2) and (-1, 2) for Black
            // Backward knight: 2 backward, 1 sideways = (1, -2) and (-1, -2) for Black
            // System automatically adjusts for White
            MovementCapability::Jumping {
                offsets: vec![
                    (1, 2),    // Forward right knight (for Black)
                    (-1, 2),   // Forward left knight (for Black)
                    (1, -2),   // Backward right knight (for Black)
                    (-1, -2),  // Backward left knight (for Black)
                ],
            },
        ],
    }
}

/// Evil Wolf movement: Simple 1 space in all directions except the 3 backwards directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// All directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
/// 3 backwards directions: For Black: S (16) | SE (8) | SW (32) = 56 = 0x38
///                         For White: N (1) | NE (2) | NW (128) = 131 = 0x83 (adjusted automatically)
/// All except backwards: For Black: 0xFF & !0x38 = 0xC7 = N (1) | NE (2) | E (4) | W (64) | NW (128) = 199 = 0xC7
///                       For White: 0xFF & !0x83 = 0x7C = S (16) | SE (8) | SW (32) | E (4) | W (64) = 124 = 0x7C (adjusted automatically)
pub fn evil_wolf_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all directions except the 3 backwards directions
            // For Black: N (1) | NE (2) | E (4) | W (64) | NW (128) = 199 = 0xC7
            // For White: S (16) | SE (8) | SW (32) | E (4) | W (64) = 124 = 0x7C (adjusted automatically)
            MovementCapability::Simple {
                directions: 0xC7,  // All except S, SE, SW (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Poisonous Wolf movement: Simple 1 space in all directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// All directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
pub fn poisonous_wolf_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::direction::DIRECTION_SET_ALL;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all directions
            // All 8 directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
            MovementCapability::Simple {
                directions: DIRECTION_SET_ALL,  // All 8 directions (same for both colors)
                max_distance: 1,
            },
        ],
    }
}

/// Angry Boar movement: Simple 1 space diagonally and sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// Diagonally: NE (2) | SE (8) | SW (32) | NW (128) = 170 = 0xAA (same for both colors)
/// Sideways: E (4) | W (64) = 68 = 0x44 (same for both colors)
/// Combined: 0xAA | 0x44 = 238 = 0xEE (same for both colors)
pub fn angry_boar_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space diagonally and sideways
            // Diagonally: NE (2) | SE (8) | SW (32) | NW (128) = 170 = 0xAA
            // Sideways: E (4) | W (64) = 68 = 0x44
            // Combined: 0xAA | 0x44 = 238 = 0xEE (same for both colors)
            MovementCapability::Simple {
                directions: 0xEE,  // All diagonals + E, W (same for both colors)
                max_distance: 1,
            },
        ],
    }
}

/// Fierce Bear movement: Simple 1 space sideways, simple up to 2 spaces in the forwards diagonal directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4) | W (64) = 68 = 0x44 (same for both colors)
/// Forwards diagonals: For Black: NE (2) | NW (128) = 130 = 0x82
///                     For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
pub fn fierce_bear_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space sideways
            // E (4) | W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces in forwards diagonal directions
            // For Black: NE (2) | NW (128) = 130 = 0x82
            // For White: SE (8) | SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 2,
            },
        ],
    }
}

/// Great Bear movement: Simple 1 space orthogonally except forwards, range movement in all 3 forwards directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Orthogonally except forwards: For Black: E (4) | W (64) | S (16) = 84 = 0x54
///                               For White: E (4) | W (64) | N (1) = 69 = 0x45 (adjusted automatically)
/// All 3 forwards: For Black: N (1) | NE (2) | NW (128) = 131 = 0x83
///                 For White: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
pub fn great_bear_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space orthogonally except forwards
            // For Black: E (4) | W (64) | S (16) = 84 = 0x54
            // For White: E (4) | W (64) | N (1) = 69 = 0x45 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x54,  // E, W, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: all 3 forwards directions
            // For Black: N (1) | NE (2) | NW (128) = 131 = 0x83
            // For White: S (16) | SE (8) | SW (32) = 56 = 0x38 (adjusted automatically)
            range_capability(
                0x83,  // N, NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Flying Horse movement: Simple up to 2 spaces diagonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Diagonally: NE (2) | SE (8) | SW (32) | NW (128) = 170 = 0xAA (same for both colors)
pub fn flying_horse_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces diagonally
            // All 4 diagonal directions: NE (2) | SE (8) | SW (32) | NW (128) = 170 = 0xAA (same for both colors)
            MovementCapability::Simple {
                directions: DIRECTION_SET_DIAGONAL,  // All diagonals (same for both colors)
                max_distance: 2,
            },
        ],
    }
}

/// Donkey movement: Simple up to 2 spaces orthogonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Orthogonally: N (1) | E (4) | S (16) | W (64) = 85 = 0x55 (same for both colors)
pub fn donkey_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::direction::DIRECTION_SET_ORTHOGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces orthogonally
            // All 4 orthogonal directions: N (1) | E (4) | S (16) | W (64) = 85 = 0x55 (same for both colors)
            MovementCapability::Simple {
                directions: DIRECTION_SET_ORTHOGONAL,  // All orthogonals (same for both colors)
                max_distance: 2,
            },
        ],
    }
}

/// Side Ox movement: Simple 1 space to upper right and lower left, range movement sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// Upper right and lower left: For Black: NE (2) | SW (32) = 34 = 0x22
///                            For White: SE (8) | NW (128) = 136 = 0x88 (adjusted automatically)
/// Sideways: E (4) | W (64) = 68 = 0x44 (same for both colors)
pub fn side_ox_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space to upper right and lower left
            // For Black: NE (2) | SW (32) = 34 = 0x22
            // For White: SE (8) | NW (128) = 136 = 0x88 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x22,  // NE, SW (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: sideways
            // E (4) | W (64) = 68 = 0x44 (same for both colors)
            range_capability(
                0x44,  // E, W (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Vertical Wolf movement: Simple 1 space sideways, simple up to 3 spaces straight backwards, range movement straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4) | W (64) = 68 = 0x44 (same for both colors)
/// Straight backwards: For Black: S (16) = 16 = 0x10
///                    For White: N (1) = 1 = 0x01 (adjusted automatically)
/// Straight forwards: For Black: N (1) = 1 = 0x01
///                    For White: S (16) = 16 = 0x10 (adjusted automatically)
pub fn vertical_wolf_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space sideways
            // E (4) | W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 1,
            },
            // Simple movement: up to 3 spaces straight backwards
            // For Black: S (16) = 16 = 0x10
            // For White: N (1) = 1 = 0x01 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (for Black) - will be adjusted for White
                max_distance: 3,
            },
            // Range movement: straight forwards
            // For Black: N (1) = 1 = 0x01
            // For White: S (16) = 16 = 0x10 (adjusted automatically)
            range_capability(
                0x01,  // N (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Tile Chariot movement: Simple 1 space to upper right and lower left, range movement vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Upper right and lower left: For Black: NE (2) | SW (32) = 34 = 0x22
///                            For White: SE (8) | NW (128) = 136 = 0x88 (adjusted automatically)
/// Vertically: N (1) | S (16) = 17 = 0x11 (same for both colors)
pub fn tile_chariot_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space to upper right and lower left
            // For Black: NE (2) | SW (32) = 34 = 0x22
            // For White: SE (8) | NW (128) = 136 = 0x88 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x22,  // NE, SW (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: vertically
            // N (1) | S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Running Tile movement: Simple up to 2 spaces sideways, range movement vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4) | W (64) = 68 = 0x44 (same for both colors)
/// Vertically: N (1) | S (16) = 17 = 0x11 (same for both colors)
pub fn running_tile_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces sideways
            // E (4) | W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
            // Range movement: vertically
            // N (1) | S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Furious Fiend movement: Same as Lion, except two-step first move is up to 3 spaces in any direction
/// Jump offsets: Same as Lion (all squares 2 steps away)
/// Two-step: First move up to 3 spaces in any direction, second move 1 space in any direction
pub fn furious_fiend_movement() -> MovementConfig {
    use crate::movement::direction::DIRECTION_SET_ALL;
    
    // Jump offsets: same as Lion
    let jump_offsets = vec![
        (2, -2), (2, -1), (2, 0), (2, 1), (2, 2),
        (-2, -2), (-2, -1), (-2, 0), (-2, 1), (-2, 2),
        (-1, 2), (0, 2), (1, 2),
        (-1, -2), (0, -2), (1, -2),
    ];
    
    // Two-step: first move up to 3 spaces in any direction, second move 1 space in any direction
    let first_step = MovementCapability::Simple {
        directions: DIRECTION_SET_ALL,  // All 8 directions
        max_distance: 3,
    };
    let second_step = MovementCapability::Simple {
        directions: DIRECTION_SET_ALL,  // All 8 directions
        max_distance: 1,
    };
    
    MovementConfig {
        capabilities: vec![
            // Jump movement: all squares 2 steps away
            MovementCapability::Jumping {
                offsets: jump_offsets,
            },
            // Two-step: up to 3 spaces any direction, then 1 space any direction
            MovementCapability::TwoStep {
                first: Box::new(first_step),
                second: Box::new(second_step),
            },
        ],
    }
}

/// Gold Stag movement: Simple 2 backwards diagonals, range forwards diagonals
/// Directions are relative to piece color (Black moves up, White moves down)
/// Backwards diagonals: For Black SW (32), SE (8) = 40 = 0x28, For White NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
/// Forwards diagonals: For Black NE (2), NW (128) = 130 = 0x82, For White SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
pub fn gold_stag_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces in backwards diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SW, SE (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Range movement: forwards diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            range_capability(
                0x82,  // NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Silver Rabbit movement: Simple 2 forwards diagonals, range backwards diagonals
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forwards diagonals: For Black NE (2), NW (128) = 130 = 0x82, For White SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
/// Backwards diagonals: For Black SW (32), SE (8) = 40 = 0x28, For White NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
pub fn silver_rabbit_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces in forwards diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Range movement: backwards diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            range_capability(
                0x28,  // SW, SE (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Side Boar movement: Range sideways, simple 1 in all other directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// All other directions: N (1), S (16), NE (2), SE (8), SW (32), NW (128) = 187 = 0xBB
/// For Black: N (1), S (16), NE (2), SE (8), SW (32), NW (128) = 187 = 0xBB
/// For White: S (16), N (1), SE (8), SW (32), NW (128), NE (2) = 187 = 0xBB (same)
pub fn side_boar_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            range_capability(
                0x44,  // E, W (same for both colors)
                BlockingMode::NoJump,
            ),
            // Simple movement: 1 space in all other directions
            // N (1), S (16), NE (2), SE (8), SW (32), NW (128) = 187 = 0xBB (same for both colors)
            MovementCapability::Simple {
                directions: 0xBB,  // All except sideways (same for both colors)
                max_distance: 1,
            },
        ],
    }
}

/// Free Boar movement: Simple 1 straight backwards, range in all 3 forwards and sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black S (16), For White N (1) (adjusted automatically)
/// Range: For Black N (1), NE (2), NW (128), E (4), W (64) = 199 = 0xC7
/// Range: For White S (16), SE (8), SW (32), E (4), W (64) = 124 = 0x7C (adjusted automatically)
pub fn free_boar_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16)
            // For White: N (1) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (16) for Black - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: all 3 forwards and sideways
            // For Black: N (1), NE (2), NW (128), E (4), W (64) = 199 = 0xC7
            // For White: S (16), SE (8), SW (32), E (4), W (64) = 124 = 0x7C (adjusted automatically)
            // All 3 forwards: For Black N (1), NE (2), NW (128) = 131 = 0x83
            // Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
            // Combined: 0x83 | 0x44 = 0xC7 for Black, adjusted for White
            range_capability(
                0xC7,  // N, NE, NW, E, W (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Ox General movement: Simple 1 forward diagonal and straight backwards, simple 3 straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forward diagonals: For Black NE (2), NW (128) = 130 = 0x82, For White SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
/// Straight backwards: For Black S (16), For White N (1) (adjusted automatically)
/// Straight forwards: For Black N (1), For White S (16) (adjusted automatically)
pub fn ox_general_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in forward diagonal and straight backwards directions
            // For Black: NE (2), NW (128), S (16) = 146 = 0x92
            // For White: SE (8), SW (32), N (1) = 41 = 0x29 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x92,  // NE, NW, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 3 spaces straight forwards
            // For Black: N (1)
            // For White: S (16) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (1) for Black - will be adjusted for White
                max_distance: 3,
            },
        ],
    }
}

/// Free Ox movement: Simple 1 backwards diagonals, simple 2 sideways, range in all 3 forwards and straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Backwards diagonals: For Black SW (32), SE (8) = 40 = 0x28, For White NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Range: For Black N (1), NE (2), NW (128), S (16) = 147 = 0x93
/// Range: For White S (16), SE (8), SW (32), N (1) = 57 = 0x39 (adjusted automatically)
pub fn free_ox_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in backwards diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SW, SE (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
            // Range movement: all 3 forwards and straight backwards
            // For Black: N (1), NE (2), NW (128), S (16) = 147 = 0x93
            // For White: S (16), SE (8), SW (32), N (1) = 57 = 0x39 (adjusted automatically)
            range_capability(
                0x93,  // N, NE, NW, S (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Horse General movement: Simple 1 forward diagonal and straight backwards, simple 3 straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forward diagonals: For Black NE (2), NW (128) = 130 = 0x82, For White SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
/// Straight backwards: For Black S (16), For White N (1) (adjusted automatically)
/// Straight forwards: For Black N (1), For White S (16) (adjusted automatically)
pub fn horse_general_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in forward diagonal and straight backwards directions
            // For Black: NE (2), NW (128), S (16) = 146 = 0x92
            // For White: SE (8), SW (32), N (1) = 41 = 0x29 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x92,  // NE, NW, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 3 spaces straight forwards
            // For Black: N (1)
            // For White: S (16) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (1) for Black - will be adjusted for White
                max_distance: 3,
            },
        ],
    }
}

/// Free Horse movement: Simple 1 backwards diagonals, simple 2 sideways, range in all 3 forwards and straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Backwards diagonals: For Black SW (32), SE (8) = 40 = 0x28, For White NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Range: For Black N (1), NE (2), NW (128), S (16) = 147 = 0x93
/// Range: For White S (16), SE (8), SW (32), N (1) = 57 = 0x39 (adjusted automatically)
pub fn free_horse_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in backwards diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SW, SE (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
            // Range movement: all 3 forwards and straight backwards
            // For Black: N (1), NE (2), NW (128), S (16) = 147 = 0x93
            // For White: S (16), SE (8), SW (32), N (1) = 57 = 0x39 (adjusted automatically)
            range_capability(
                0x93,  // N, NE, NW, S (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Pup General movement: Simple 1 backwards diagonals, simple 4 straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Backwards diagonals: For Black SW (32), SE (8) = 40 = 0x28, For White NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
/// Straight forwards: For Black N (1), For White S (16) (adjusted automatically)
pub fn pup_general_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in backwards diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SW, SE (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 4 spaces straight forwards
            // For Black: N (1)
            // For White: S (16) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (1) for Black - will be adjusted for White
                max_distance: 4,
            },
        ],
    }
}

/// Chicken General movement: Simple 1 backwards diagonals, simple 4 straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Backwards diagonals: For Black SW (32), SE (8) = 40 = 0x28, For White NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
/// Straight forwards: For Black N (1), For White S (16) (adjusted automatically)
pub fn chicken_general_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in backwards diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SW, SE (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 4 spaces straight forwards
            // For Black: N (1)
            // For White: S (16) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (1) for Black - will be adjusted for White
                max_distance: 4,
            },
        ],
    }
}

/// Free Chicken movement: Simple 2 sideways and backwards diagonals, range in 3 forwards and straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Backwards diagonals: For Black SW (32), SE (8) = 40 = 0x28, For White NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
/// Range: For Black N (1), NE (2), NW (128), S (16) = 147 = 0x93
/// Range: For White S (16), SE (8), SW (32), N (1) = 57 = 0x39 (adjusted automatically)
pub fn free_chicken_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces sideways and backwards diagonals
            // For Black: E (4), W (64), SW (32), SE (8) = 108 = 0x6C
            // For White: E (4), W (64), NW (128), NE (2) = 198 = 0xC6 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x6C,  // E, W, SW, SE (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Range movement: all 3 forwards and straight backwards
            // For Black: N (1), NE (2), NW (128), S (16) = 147 = 0x93
            // For White: S (16), SE (8), SW (32), N (1) = 57 = 0x39 (adjusted automatically)
            range_capability(
                0x93,  // N, NE, NW, S (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Pig General movement: Simple 2 straight backwards, simple 4 forward diagonals
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black S (16), For White N (1) (adjusted automatically)
/// Forward diagonals: For Black NE (2), NW (128) = 130 = 0x82, For White SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
pub fn pig_general_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces straight backwards
            // For Black: S (16)
            // For White: N (1) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (16) for Black - will be adjusted for White
                max_distance: 2,
            },
            // Simple movement: up to 4 spaces in forward diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 4,
            },
        ],
    }
}

/// Free Pig movement: Same as Free Pup, Free Horse, and Free Ox
/// Simple 1 backwards diagonals, simple 2 sideways, range in all 3 forwards and straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Backwards diagonals: For Black SW (32), SE (8) = 40 = 0x28, For White NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Range: For Black N (1), NE (2), NW (128), S (16) = 147 = 0x93
/// Range: For White S (16), SE (8), SW (32), N (1) = 57 = 0x39 (adjusted automatically)
pub fn free_pig_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in backwards diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SW, SE (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
            // Range movement: all 3 forwards and straight backwards
            // For Black: N (1), NE (2), NW (128), S (16) = 147 = 0x93
            // For White: S (16), SE (8), SW (32), N (1) = 57 = 0x39 (adjusted automatically)
            range_capability(
                0x93,  // N, NE, NW, S (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Knight movement: Jumping move 2 spaces forward and 1 space sideways (shogi knight)
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: 2 spaces up (rank +2) and 1 space left or right (file ±1)
/// For White: 2 spaces down (rank -2) and 1 space left or right (file ±1)
pub fn knight_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Jumping movement: 2 spaces forward and 1 space sideways
            // For Black: (1, 2) and (-1, 2) - right/left and up
            // For White: (1, -2) and (-1, -2) - right/left and down
            // Note: The movement system will automatically adjust directions for White
            MovementCapability::Jumping {
                offsets: vec![
                    (1, 2),   // Right and forward (for Black) / Right and forward (for White)
                    (-1, 2),  // Left and forward (for Black) / Left and forward (for White)
                ],
            },
        ],
    }
}

/// Side Soldier movement: Simple 1 straight backwards, simple 2 straight forwards, range sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black S (16), For White N (1) (adjusted automatically)
/// Straight forwards: For Black N (1), For White S (16) (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
pub fn side_soldier_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16)
            // For White: N (1) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (16) for Black - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces straight forwards
            // For Black: N (1)
            // For White: S (16) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (1) for Black - will be adjusted for White
                max_distance: 2,
            },
            // Range movement: sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            range_capability(
                0x44,  // E, W (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Vertical Bear movement: Simple 1 straight backwards, simple 2 sideways, range straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black S (16), For White N (1) (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Straight forwards: For Black N (1), For White S (16) (adjusted automatically)
pub fn vertical_bear_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16)
            // For White: N (1) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (16) for Black - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
            // Range movement: straight forwards
            // For Black: N (1)
            // For White: S (16) (adjusted automatically)
            range_capability(
                0x01,  // N (1) for Black - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Silver Chariot movement: Simple 1 backwards diagonals, simple 2 forwards diagonals, range vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Backwards diagonals: For Black SW (32), SE (8) = 40 = 0x28, For White NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
/// Forwards diagonals: For Black NE (2), NW (128) = 130 = 0x82, For White SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
pub fn silver_chariot_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in backwards diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SW, SE (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces in forwards diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Range movement: vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Goose Wing movement: Simple 1 diagonally, simple 3 sideways, range vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Diagonally: All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
pub fn goose_wing_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space diagonally
            // All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
            MovementCapability::Simple {
                directions: DIRECTION_SET_DIAGONAL,  // All diagonals (same for both colors)
                max_distance: 1,
            },
            // Simple movement: up to 3 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 3,
            },
            // Range movement: vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Daiba movement: Simple 1 in all directions except upper right
/// Directions are relative to piece color (Black moves up, White moves down)
/// Upper right: For Black NE (2), For White SE (8) (adjusted automatically)
/// All directions except upper right: N (1) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 253 = 0xFD
/// For Black: excludes NE (2)
/// For White: excludes SE (8) (adjusted automatically)
pub fn daiba_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all directions except upper right
            // For Black: all except NE (2) = N (1) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 253 = 0xFD
            // For White: all except SE (8) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0xFD,  // All directions except NE (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// King of Teachings movement: Jump up to 3 spaces in all directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// All 8 directions at distances 1, 2, and 3
pub fn king_of_teachings_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Jumping movement: up to 3 spaces in all directions
            // For each of the 8 directions, include offsets for distances 1, 2, and 3
            // The system automatically adjusts rank_delta for White
            MovementCapability::Jumping {
                offsets: vec![
                    // N direction: (0, 1), (0, 2), (0, 3)
                    (0, 1), (0, 2), (0, 3),
                    // NE direction: (1, 1), (2, 2), (3, 3)
                    (1, 1), (2, 2), (3, 3),
                    // E direction: (1, 0), (2, 0), (3, 0)
                    (1, 0), (2, 0), (3, 0),
                    // SE direction: (1, -1), (2, -2), (3, -3)
                    (1, -1), (2, -2), (3, -3),
                    // S direction: (0, -1), (0, -2), (0, -3)
                    (0, -1), (0, -2), (0, -3),
                    // SW direction: (-1, -1), (-2, -2), (-3, -3)
                    (-1, -1), (-2, -2), (-3, -3),
                    // W direction: (-1, 0), (-2, 0), (-3, 0)
                    (-1, 0), (-2, 0), (-3, 0),
                    // NW direction: (-1, 1), (-2, 2), (-3, 3)
                    (-1, 1), (-2, 2), (-3, 3),
                ],
            },
        ],
    }
}

/// Dark Spirit movement: Simple 1 in all directions except upper left
/// Directions are relative to piece color (Black moves up, White moves down)
/// Upper left: For Black NW (128), For White SW (32) (adjusted automatically)
/// All directions except upper left: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) = 127 = 0x7F
/// For Black: excludes NW (128)
/// For White: excludes SW (32) (adjusted automatically)
pub fn dark_spirit_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all directions except upper left
            // For Black: all except NW (128) = N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) = 127 = 0x7F
            // For White: all except SW (32) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x7F,  // All directions except NW (for Black) - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Buddhist Spirit movement: Jump to all squares 2 steps away, two-step: range all directions then 1 space any direction
/// Same jumping move as Lion, but two-step first move is range in all directions (not just 1 space)
pub fn buddhist_spirit_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_ALL;
    
    // Jump offsets: all positions where either file_delta or rank_delta is exactly 2 or -2,
    // and the other is at most 2 in absolute value (same as Lion)
    let jump_offsets = vec![
        (2, -2), (2, -1), (2, 0), (2, 1), (2, 2),
        (-2, -2), (-2, -1), (-2, 0), (-2, 1), (-2, 2),
        (-1, 2), (0, 2), (1, 2),
        (-1, -2), (0, -2), (1, -2),
    ];
    
    // Two-step: first move is range in all directions, second move is 1 space in any direction
    let first_step = MovementCapability::Range {
        directions: DIRECTION_SET_ALL,  // All 8 directions
        blocking: BlockingMode::NoJump,
        cannot_jump_over: std::collections::HashSet::new(),
    };
    let second_step = MovementCapability::Simple {
        directions: DIRECTION_SET_ALL,  // All 8 directions
        max_distance: 1,
    };
    
    MovementConfig {
        capabilities: vec![
            // Jump movement: all squares 2 steps away (same as Lion)
            MovementCapability::Jumping {
                offsets: jump_offsets,
            },
            // Two-step: range all directions, then 1 space any direction
            MovementCapability::TwoStep {
                first: Box::new(first_step),
                second: Box::new(second_step),
            },
        ],
    }
}

/// Gold Bird movement: Simple 3 sideways and backward diagonals, range vertically, jump up to 3 forward diagonals
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Backward diagonals: For Black SW (32), SE (8) = 40 = 0x28, For White NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
/// Forward diagonals: For Black NE (2), NW (128) = 130 = 0x82, For White SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
pub fn gold_bird_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 3 spaces sideways and backward diagonals
            // Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
            // Backward diagonals: For Black SW (32), SE (8) = 40 = 0x28, For White NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            // Combined: 0x44 | 0x28 = 0x6C (for Black), 0x44 | 0x82 = 0xC6 (for White, adjusted automatically)
            MovementCapability::Simple {
                directions: 0x6C,  // E, W, SW, SE (for Black) - will be adjusted for White
                max_distance: 3,
            },
            // Range movement: vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
            // Jumping movement: up to 3 spaces in forward diagonal directions
            // For Black: NE, NW
            // For White: SE, SW (adjusted automatically)
            MovementCapability::Jumping {
                offsets: vec![
                    // NE direction: (1, 1), (2, 2), (3, 3)
                    (1, 1), (2, 2), (3, 3),
                    // NW direction: (-1, 1), (-2, 2), (-3, 3)
                    (-1, 1), (-2, 2), (-3, 3),
                ],
            },
        ],
    }
}

/// Free Bird movement: Range sideways, simple 3 backward diagonals, range vertically, jump up to 3 forward diagonals
/// Same as Gold Bird except sideways is range movement instead of simple up to 3 spaces
pub fn free_bird_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: sideways and vertically
            // Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
            // Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
            // Combined: 0x44 | 0x11 = 0x55 (same for both colors)
            range_capability(
                0x55,  // E, W, N, S (same for both colors)
                BlockingMode::NoJump,
            ),
            // Simple movement: up to 3 spaces in backward diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SW, SE (for Black) - will be adjusted for White
                max_distance: 3,
            },
            // Jumping movement: up to 3 spaces in forward diagonal directions
            // For Black: NE, NW
            // For White: SE, SW (adjusted automatically)
            MovementCapability::Jumping {
                offsets: vec![
                    // NE direction: (1, 1), (2, 2), (3, 3)
                    (1, 1), (2, 2), (3, 3),
                    // NW direction: (-1, 1), (-2, 2), (-3, 3)
                    (-1, 1), (-2, 2), (-3, 3),
                ],
            },
        ],
    }
}

/// Fierce Ox movement: Simple 1 vertically, range forward diagonals
/// Directions are relative to piece color (Black moves up, White moves down)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
/// Forward diagonals: For Black NE (2), NW (128) = 130 = 0x82, For White SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
pub fn fierce_ox_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            MovementCapability::Simple {
                directions: 0x11,  // N, S (same for both colors)
                max_distance: 1,
            },
            // Range movement: forward diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            range_capability(
                0x82,  // NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Flying Ox movement: Range all directions except sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// All directions except sideways: N (1) | NE (2) | SE (8) | S (16) | SW (32) | NW (128) = 187 = 0xBB
/// Sideways: E (4), W (64) = 68 = 0x44 (excluded)
pub fn flying_ox_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: all directions except sideways
            // N (1) | NE (2) | SE (8) | S (16) | SW (32) | NW (128) = 187 = 0xBB
            // Excludes: E (4), W (64) = 0x44
            range_capability(
                0xBB,  // All directions except E, W (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Fire Ox movement: Simple 1 space sideways, range movement in all other directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// All other directions: N (1) | NE (2) | SE (8) | S (16) | SW (32) | NW (128) = 187 = 0xBB (same for both colors)
pub fn fire_ox_movement() -> MovementConfig {
    use crate::movement::types::{BlockingMode};
    use std::collections::HashSet;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 1,
            },
            // Range movement: in all other directions
            // N (1) | NE (2) | SE (8) | S (16) | SW (32) | NW (128) = 187 = 0xBB (same for both colors)
            MovementCapability::Range {
                directions: 0xBB,  // All directions except E, W (same for both colors)
                blocking: BlockingMode::NoJump,
                cannot_jump_over: HashSet::new(),
            },
        ],
    }
}

/// Sheep Soldier movement: Simple 1 straight backwards, range forward diagonals
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black S (16), For White N (1) (adjusted automatically)
/// Forward diagonals: For Black NE (2), NW (128) = 130 = 0x82, For White SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
pub fn sheep_soldier_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16)
            // For White: N (1) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (16) for Black - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: forward diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            range_capability(
                0x82,  // NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Tiger Soldier movement: Simple 1 straight backwards, range forward diagonals, simple 2 straight forward
/// Same as Sheep Soldier, plus simple 2 straight forward
pub fn tiger_soldier_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16)
            // For White: N (1) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (16) for Black - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: forward diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            range_capability(
                0x82,  // NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
            // Simple movement: up to 2 spaces straight forward
            // For Black: N (1)
            // For White: S (16) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (1) for Black - will be adjusted for White
                max_distance: 2,
            },
        ],
    }
}

/// Running Chariot movement: Range orthogonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Orthogonal: N (1), E (4), S (16), W (64) = 85 = 0x55 (same for both colors)
pub fn running_chariot_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_ORTHOGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: orthogonally
            // N (1), E (4), S (16), W (64) = 85 = 0x55 (same for both colors)
            range_capability(
                DIRECTION_SET_ORTHOGONAL,  // All orthogonal directions (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Cannon Chariot movement: Simple 1 sideways, range all 3 forward directions and straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// All 3 forward directions + straight backwards: For Black N (1), NE (2), NW (128), S (16) = 147 = 0x93, For White S (16), SE (8), SW (32), N (1) = 57 = 0x39 (adjusted automatically)
pub fn cannon_chariot_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 1,
            },
            // Range movement: all 3 forward directions and straight backwards
            // For Black: N (1), NE (2), NW (128), S (16) = 147 = 0x93
            // For White: S (16), SE (8), SW (32), N (1) = 57 = 0x39 (adjusted automatically)
            range_capability(
                0x93,  // N, NE, NW, S (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Copper Chariot movement: Simple 3 forward diagonals, range vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forward diagonals: For Black NE (2), NW (128) = 130 = 0x82, For White SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
pub fn copper_chariot_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 3 spaces in forward diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 3,
            },
            // Range movement: vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Copper Elephant movement: Range vertically, simple 1 in all other directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
/// All other directions: NE (2), E (4), SE (8), SW (32), W (64), NW (128) = 238 = 0xEE (same for both colors)
pub fn copper_elephant_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: vertically
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),
            // Simple movement: 1 space in all other directions
            // NE (2), E (4), SE (8), SW (32), W (64), NW (128) = 238 = 0xEE (same for both colors)
            MovementCapability::Simple {
                directions: 0xEE,  // All directions except N, S (same for both colors)
                max_distance: 1,
            },
        ],
    }
}

/// Cloud Dragon movement: Simple 1 sideways and straight forward, range diagonally and straight backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// Straight forward: For Black N (1), For White S (16) (adjusted automatically)
/// Diagonally: All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
/// Straight backwards: For Black S (16), For White N (1) (adjusted automatically)
pub fn cloud_dragon_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space sideways and straight forward
            // Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
            // Straight forward: For Black N (1), For White S (16) (adjusted automatically)
            // Combined: For Black N (1) | E (4) | W (64) = 69 = 0x45, For White S (16) | E (4) | W (64) = 84 = 0x54 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x45,  // N, E, W (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: diagonally and straight backwards
            // Diagonally: All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
            // Straight backwards: For Black S (16), For White N (1) (adjusted automatically)
            // Combined: For Black 0xAA | S (16) = 186 = 0xBA, For White 0xAA | N (1) = 171 = 0xAB (adjusted automatically)
            range_capability(
                0xBA,  // All diagonals + S (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Little Standard movement: Simple 1 backwards diagonals, simple 2 forwards diagonals, range orthogonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Backwards diagonals: For Black SW (32), SE (8) = 40 = 0x28, For White NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
/// Forward diagonals: For Black NE (2), NW (128) = 130 = 0x82, For White SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
/// Orthogonally: N (1), E (4), S (16), W (64) = 85 = 0x55 (same for both colors)
pub fn little_standard_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_ORTHOGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in backwards diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SW, SE (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces in forward diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Range movement: orthogonally
            // N (1), E (4), S (16), W (64) = 85 = 0x55 (same for both colors)
            range_capability(
                DIRECTION_SET_ORTHOGONAL,  // All orthogonal directions (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Soldier movement: Range orthogonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// Orthogonally: N (1), E (4), S (16), W (64) = 85 = 0x55 (same for both colors)
pub fn soldier_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_ORTHOGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: orthogonally
            // N (1), E (4), S (16), W (64) = 85 = 0x55 (same for both colors)
            range_capability(
                DIRECTION_SET_ORTHOGONAL,  // All orthogonal directions (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Cavalier movement: Range all directions except backwards diagonals
/// Directions are relative to piece color (Black moves up, White moves down)
/// All directions except backwards diagonals: N (1) | NE (2) | E (4) | SE (8) | S (16) | W (64) | NW (128) = 223 = 0xDF
/// Backwards diagonals: For Black SW (32), SE (8) = 40 = 0x28, For White NW (128), NE (2) = 130 = 0x82 (excluded)
/// Note: For Black, backwards diagonals are SW (32) and SE (8), so all except those = N (1) | NE (2) | E (4) | S (16) | W (64) | NW (128) = 223 = 0xDF
/// For White, backwards diagonals are NW (128) and NE (2), so all except those (adjusted automatically)
pub fn cavalier_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: all directions except backwards diagonals
            // For Black: All except SW (32), SE (8) = N (1) | NE (2) | E (4) | S (16) | W (64) | NW (128) = 223 = 0xDF
            // For White: All except NW (128), NE (2) (adjusted automatically)
            range_capability(
                0xDF,  // All directions except SW, SE (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Vertical Tiger movement: Simple 2 straight backwards, range straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// Straight backwards: For Black S (16), For White N (1) (adjusted automatically)
/// Straight forwards: For Black N (1), For White S (16) (adjusted automatically)
pub fn vertical_tiger_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces straight backwards
            // For Black: S (16)
            // For White: N (1) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (16) for Black - will be adjusted for White
                max_distance: 2,
            },
            // Range movement: straight forwards
            // For Black: N (1)
            // For White: S (16) (adjusted automatically)
            range_capability(
                0x01,  // N (1) for Black - will be adjusted for White
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Mountain Hawk movement: Simple 2 backwards diagonals, range all other directions, jump 2 straight ahead
/// Directions are relative to piece color (Black moves up, White moves down)
/// Backwards diagonals: For Black SW (32), SE (8) = 40 = 0x28, For White NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
/// All other directions: N (1) | NE (2) | E (4) | S (16) | W (64) | NW (128) = 223 = 0xDF (for Black)
/// Jump: 2 spaces straight ahead (N for Black, S for White)
pub fn mountain_hawk_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces in backwards diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SW, SE (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Range movement: all other directions (all except backwards diagonals)
            // For Black: All except SW (32), SE (8) = N (1) | NE (2) | E (4) | S (16) | W (64) | NW (128) = 223 = 0xDF
            // For White: All except NW (128), NE (2) (adjusted automatically)
            range_capability(
                0xDF,  // All directions except SW, SE (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),
            // Jump movement: 2 spaces straight ahead
            // For Black: N (0, 2)
            // For White: S (0, -2) (adjusted automatically)
            MovementCapability::Jumping {
                offsets: vec![
                    (0, 2),  // N for Black - will be adjusted for White
                ],
            },
        ],
    }
}

/// Horned Hawk movement: Range all directions, jump 2 straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// All directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
/// Jump: 2 spaces straight forwards (N for Black, S for White)
pub fn horned_hawk_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_ALL;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: all directions
            // All 8 directions: N (1) | NE (2) | E (4) | SE (8) | S (16) | SW (32) | W (64) | NW (128) = 255 = 0xFF
            range_capability(
                DIRECTION_SET_ALL,  // All directions (same for both colors)
                BlockingMode::NoJump,
            ),
            // Jump movement: 2 spaces straight forwards
            // For Black: N (0, 2)
            // For White: S (0, -2) (adjusted automatically)
            MovementCapability::Jumping {
                offsets: vec![
                    (0, 2),  // N for Black - will be adjusted for White
                ],
            },
        ],
    }
}

/// Flying Cat movement: Simple 1 all 3 backwards directions, jump 3 sideways and all 3 forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// All 3 backwards directions: For Black S (16), SW (32), SE (8) = 56 = 0x38, For White N (1), NW (128), NE (2) = 131 = 0x83 (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
/// All 3 forwards directions: For Black N (1), NE (2), NW (128) = 131 = 0x83, For White S (16), SE (8), SW (32) = 56 = 0x38 (adjusted automatically)
pub fn flying_cat_movement() -> MovementConfig {
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all 3 backwards directions
            // For Black: S (16), SW (32), SE (8) = 56 = 0x38
            // For White: N (1), NW (128), NE (2) = 131 = 0x83 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x38,  // S, SW, SE (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Jump movement: 3 spaces sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            // Offsets: (3, 0) and (-3, 0)
            MovementCapability::Jumping {
                offsets: vec![
                    (3, 0),   // E (same for both colors)
                    (-3, 0),  // W (same for both colors)
                ],
            },
            // Jump movement: 3 spaces in all 3 forwards directions
            // For Black: N (0, 3), NE (3, 3), NW (-3, 3)
            // For White: S (0, -3), SE (3, -3), SW (-3, -3) (adjusted automatically)
            MovementCapability::Jumping {
                offsets: vec![
                    (0, 3),   // N for Black
                    (3, 3),   // NE for Black
                    (-3, 3),  // NW for Black
                ],
            },
        ],
    }
}

/// Side Wolf movement: Simple 1 forward left and backward right diagonals, range sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// Forward left diagonal: For Black NW (128), For White SW (32) (adjusted automatically)
/// Backward right diagonal: For Black SE (8), For White NE (2) (adjusted automatically)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
pub fn side_wolf_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in forward left diagonal and backward right diagonal
            // For Black: NW (128), SE (8) = 136 = 0x88
            // For White: SW (32), NE (2) = 34 = 0x22 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x88,  // NW, SE (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Range movement: sideways
            // E (4), W (64) = 68 = 0x44 (same for both colors)
            range_capability(
                0x44,  // E, W (same for both colors)
                BlockingMode::NoJump,
            ),
        ],
    }
}

/// Flying Swallow movement: Range in forward diagonal directions, simple 1 backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Range in NE (2), NW (128) = 130 = 0x82
/// For White: Range in SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
pub fn flying_swallow_movement() -> MovementConfig {
    
    MovementConfig {
        capabilities: vec![
            // Range movement in forward diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            range_capability(
                0x82,  // NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

            // Simple movement: 1 space directly backwards
            // For Black: S (16)
            // For White: N (1) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (16) for Black - will be adjusted for White
                max_distance: 1,
            },
        ],
    }
}

/// Great Dragon movement: Range diagonally, simple 3 forwards and backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Range in NE (2), NW (128), SW (32), SE (8) = 170 = 0xAA
/// For White: same (diagonals are symmetric)
pub fn great_dragon_movement() -> MovementConfig {
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all diagonal directions
            // For Black: NE (2), NW (128), SW (32), SE (8) = 170 = 0xAA
            // For White: same (diagonals are symmetric)
            range_capability(
                0xAA,  // All diagonals (same for both colors)
                BlockingMode::NoJump,
            ),

            // Simple movement: up to 3 spaces straight forwards or backwards
            // For Black: N (1), S (16) = 17 = 0x11
            // For White: S (16), N (1) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x11,  // N and S (for Black) - will be adjusted for White
                max_distance: 3,
            },
        ],
    }
}

/// Primordial Dragon movement: Range diagonally, jumping range vertically
/// Directions are relative to piece color (Black moves up, White moves down)
/// Diagonally: All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
/// Vertically: N (1), S (16) = 17 = 0x11 (same for both colors)
/// Jumping range means it can jump over pieces without capturing them
pub fn primordial_dragon_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    use crate::movement::direction::DIRECTION_SET_DIAGONAL;
    
    MovementConfig {
        capabilities: vec![
            // Range movement: diagonally (standard, blocked by pieces)
            // All 4 diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
            range_capability(
                DIRECTION_SET_DIAGONAL,  // All diagonals (same for both colors)
                BlockingMode::NoJump,
            ),
            // Jumping range movement: vertically (can jump over pieces without capturing)
            // N (1), S (16) = 17 = 0x11 (same for both colors)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::Jump,
            ),
        ],
    }
}

/// Mountain Stag movement: Simple forward 1, sideways 2, forward diagonals 3, backwards 4
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Forward N (1), Sideways E (4) W (64), Forward diagonals NE (2) NW (128), Backwards S (16)
/// For White: Forward S (16), Sideways E (4) W (64), Forward diagonals SE (8) SW (32), Backwards N (1) (adjusted automatically)
pub fn mountain_stag_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight forward
            // For Black: N (1)
            // For White: S (16) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (1) for Black - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 2 spaces sideways
            // For Black: E (4), W (64) = 68 = 0x44
            // For White: same (sideways are symmetric)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
            // Simple movement: up to 3 spaces in forward diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 3,
            },
            // Simple movement: up to 4 spaces straight backwards
            // For Black: S (16)
            // For White: N (1) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (16) for Black - will be adjusted for White
                max_distance: 4,
            },
        ],
    }
}

/// Great Stag movement: Range orthogonal, backwards diagonal 2, forward diagonal jump 2
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Range in N (1), E (4), S (16), W (64) = 85 = 0x55
/// For White: same (orthogonal are symmetric)
/// Backwards diagonals: SW (32), SE (8) = 40 = 0x28 (for Black)
/// Forward diagonal jumps: NE (2, 2), NW (-2, 2) (for Black)
pub fn great_stag_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all orthogonal directions
            // For Black: N (1), E (4), S (16), W (64) = 85 = 0x55
            // For White: same (orthogonal are symmetric)
            range_capability(
                0x55,  // N, E, S, W (same for both colors)
                BlockingMode::NoJump,
            ),

            // Simple movement: up to 2 spaces in backwards diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SW, SE (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Jump exactly 2 spaces in forward diagonal directions
            // For Black: NE (2, 2), NW (-2, 2)
            // For White: SE (2, -2), SW (-2, -2) (adjusted automatically)
            MovementCapability::Jumping {
                offsets: vec![
                    (2, 2),    // NE for Black
                    (-2, 2),   // NW for Black
                ],
            },
        ],
    }
}

/// Silver General movement: Simple 1 space diagonally and straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA, Forward N (1)
/// For White: Diagonals same (symmetric), Forward S (16) (adjusted automatically)
pub fn silver_general_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in all diagonals and straight forward
            // For Black: Diagonals NE (2), SE (8), SW (32), NW (128) = 0xAA; Forward N (1) = 0x01; Combined = 0xAB
            // For White: Diagonals are symmetric, forward S (16) is substituted for N (1) (handled automatically)
            MovementCapability::Simple {
                directions: 0xAB,  // All diagonals and N (forward for Black); adjusted automatically for White
                max_distance: 1,
            },
        ],
    }
}

/// Vertical Mover movement: Range forwards/backwards, simple 1 sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Range in N (1), S (16) = 17 = 0x11
/// For White: same (forwards/backwards are symmetric)
/// Sideways: E (4), W (64) = 68 = 0x44 (same for both colors)
pub fn vertical_mover_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in straight forwards and backwards directions
            // For Black: N (1), S (16) = 17 = 0x11
            // For White: same (forwards/backwards are symmetric)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),

            // Simple movement: 1 space sideways
            // For Black: E (4), W (64) = 68 = 0x44
            // For White: same (sideways are symmetric)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 1,
            },
        ],
    }
}

/// Rikishi movement: Simple up to 3 spaces diagonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA
/// For White: same (diagonals are symmetric)
pub fn rikishi_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 3 spaces in all diagonal directions
            // For Black: NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA
            // For White: same (diagonals are symmetric)
            MovementCapability::Simple {
                directions: 0xAA,  // All diagonals (same for both colors)
                max_distance: 3,
            },
        ],
    }
}

/// Kongou movement: Simple up to 3 spaces orthogonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: N (1), E (4), S (16), W (64) = 85 = 0x55
/// For White: same (orthogonal are symmetric)
pub fn kongou_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 3 spaces in all orthogonal directions
            // For Black: N (1), E (4), S (16), W (64) = 85 = 0x55
            // For White: same (orthogonal are symmetric)
            MovementCapability::Simple {
                directions: 0x55,  // All orthogonals (same for both colors)
                max_distance: 3,
            },
        ],
    }
}

/// Rasetsu movement: Simple 1 sideways/backwards, simple 3 forward diagonals
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Sideways E (4), W (64) = 68 = 0x44, Backwards S (16), Forward diagonals NE (2), NW (128) = 130 = 0x82
/// For White: Sideways same, Backwards N (1), Forward diagonals SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
pub fn rasetsu_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space sideways and straight backwards
            // For Black: E (4), W (64), S (16) = 84 = 0x54
            // For White: E (4), W (64), N (1) = 69 = 0x45 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x54,  // E, W, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 3 spaces in forward diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 3,
            },
        ],
    }
}

/// Yasha movement: Simple 1 forward diagonals/backwards, simple 3 sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Forward diagonals NE (2), NW (128) = 130 = 0x82, Backwards S (16), Sideways E (4), W (64) = 68 = 0x44
/// For White: Forward diagonals SE (8), SW (32) = 40 = 0x28, Backwards N (1), Sideways same (adjusted automatically)
pub fn yasha_movement() -> MovementConfig {
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space in forward diagonal directions and straight backwards
            // For Black: NE (2), NW (128), S (16) = 146 = 0x92
            // For White: SE (8), SW (32), N (1) = 41 = 0x29 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x92,  // NE, NW, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 3 spaces sideways
            // For Black: E (4), W (64) = 68 = 0x44
            // For White: same (sideways are symmetric)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 3,
            },
        ],
    }
}

/// Shiten movement: Simple up to 4 spaces in all directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: All 8 directions = 255 = 0xFF
/// For White: same (all directions are symmetric)
pub fn shiten_movement() -> MovementConfig {
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 4 spaces in all 8 directions
            // For Black: N (1), NE (2), E (4), SE (8), S (16), SW (32), W (64), NW (128) = 255 = 0xFF
            // For White: same (all directions are symmetric)
            MovementCapability::Simple {
                directions: 0xFF,  // All 8 directions (same for both colors)
                max_distance: 4,
            },
        ],
    }
}

/// Running Bear movement: Simple 2 sideways, range forwards/backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Sideways E (4), W (64) = 68 = 0x44, Range in N (1), S (16) = 17 = 0x11
/// For White: Sideways same, Range same (forwards/backwards are symmetric)
pub fn running_bear_movement() -> MovementConfig {
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces sideways
            // For Black: E (4), W (64) = 68 = 0x44
            // For White: same (sideways are symmetric)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 2,
            },
            // Range movement in straight forwards and backwards directions
            // For Black: N (1), S (16) = 17 = 0x11
            // For White: same (forwards/backwards are symmetric)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Free Bear movement: Range in all 3 forwards and all 3 backwards directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: N (1), NE (2), NW (128), S (16), SE (8), SW (32) = 187 = 0xBB
/// For White: same (all 3 forwards and all 3 backwards are symmetric when combined)
pub fn free_bear_movement() -> MovementConfig {
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all 3 forwards and all 3 backwards directions
            // For Black: N (1), NE (2), NW (128), S (16), SE (8), SW (32) = 187 = 0xBB
            // For White: same (all 3 forwards and all 3 backwards are symmetric when combined)
            range_capability(
                0xBB,  // N, NE, NW, S, SE, SW (same for both colors)
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Free Tiger movement: Range in all directions except straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: All directions except N (1) = 0xFF - 0x01 = 0xFE (NE, E, SE, S, SW, W, NW)
/// For White: All directions except S (16) = 0xFF - 0x10 = 0xEF (N, NE, E, SE, SW, W, NW) (adjusted automatically)
pub fn free_tiger_movement() -> MovementConfig {
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all directions except straight forwards
            // For Black: NE (2), E (4), SE (8), S (16), SW (32), W (64), NW (128) = 254 = 0xFE
            // For White: N (1), NE (2), E (4), SE (8), SW (32), W (64), NW (128) = 239 = 0xEF (adjusted automatically)
            range_capability(
                0xFE,  // All except N (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Great Dove movement: Simple 3 orthogonally, range diagonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Orthogonal N (1), E (4), S (16), W (64) = 85 = 0x55
/// For White: same (orthogonal are symmetric)
/// Diagonals: NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
pub fn great_dove_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 3 spaces in all orthogonal directions
            // For Black: N (1), E (4), S (16), W (64) = 85 = 0x55
            // For White: same (orthogonal are symmetric)
            MovementCapability::Simple {
                directions: 0x55,  // All orthogonals (same for both colors)
                max_distance: 3,
            },
            // Range movement in all diagonal directions
            // For Black: NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA
            // For White: same (diagonals are symmetric)
            range_capability(
                0xAA,  // All diagonals (same for both colors)
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Side Serpent movement: Simple 1 backwards, simple 3 forwards, range sideways
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Backwards S (16), Forwards N (1), Sideways E (4), W (64) = 68 = 0x44
/// For White: Backwards N (1), Forwards S (16), Sideways same (adjusted automatically)
pub fn side_serpent_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space straight backwards
            // For Black: S (16)
            // For White: N (1) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (16) for Black - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 3 spaces straight forwards
            // For Black: N (1)
            // For White: S (16) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x01,  // N (1) for Black - will be adjusted for White
                max_distance: 3,
            },
            // Range movement sideways
            // For Black: E (4), W (64) = 68 = 0x44
            // For White: same (sideways are symmetric)
            range_capability(
                0x44,  // E, W (same for both colors)
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Great Shark movement: Simple 2 backwards diagonals, simple 5 forwards diagonals, range orthogonal
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Backwards diagonals SW (32), SE (8) = 40 = 0x28, Forwards diagonals NE (2), NW (128) = 130 = 0x82
/// For White: Backwards diagonals NW (128), NE (2) = 130 = 0x82, Forwards diagonals SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
/// Orthogonal: N (1), E (4), S (16), W (64) = 85 = 0x55 (same for both colors)
pub fn great_shark_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces in backwards diagonal directions
            // For Black: SW (32), SE (8) = 40 = 0x28
            // For White: NW (128), NE (2) = 130 = 0x82 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x28,  // SW, SE (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Simple movement: up to 5 spaces in forwards diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 5,
            },
            // Range movement in all orthogonal directions
            // For Black: N (1), E (4), S (16), W (64) = 85 = 0x55
            // For White: same (orthogonal are symmetric)
            range_capability(
                0x55,  // All orthogonals (same for both colors)
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Running Serpent movement: Simple 1 sideways, range forwards/backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Sideways E (4), W (64) = 68 = 0x44, Range in N (1), S (16) = 17 = 0x11
/// For White: Sideways same, Range same (forwards/backwards are symmetric)
pub fn running_serpent_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space sideways
            // For Black: E (4), W (64) = 68 = 0x44
            // For White: same (sideways are symmetric)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 1,
            },
            // Range movement in straight forwards and backwards directions
            // For Black: N (1), S (16) = 17 = 0x11
            // For White: same (forwards/backwards are symmetric)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Free Serpent movement: Range in all 3 backwards directions and straight forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: N (1), S (16), SE (8), SW (32) = 57 = 0x39
/// For White: S (16), N (1), NE (2), NW (128) = 147 = 0x93 (adjusted automatically)
pub fn free_serpent_movement() -> MovementConfig {
    
    MovementConfig {
        capabilities: vec![
            // Range movement in straight forwards and all 3 backwards directions
            // For Black: N (1), S (16), SE (8), SW (32) = 57 = 0x39
            // For White: S (16), N (1), NE (2), NW (128) = 147 = 0x93 (adjusted automatically)
            range_capability(
                0x39,  // N, S, SE, SW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Free Leopard movement: Range in all 3 forwards and all 3 backwards directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: N (1), NE (2), NW (128), S (16), SE (8), SW (32) = 187 = 0xBB
/// For White: same (all 3 forwards and all 3 backwards are symmetric when combined)
pub fn free_leopard_movement() -> MovementConfig {
    
    MovementConfig {
        capabilities: vec![
            // Range movement in all 3 forwards and all 3 backwards directions
            // For Black: N (1), NE (2), NW (128), S (16), SE (8), SW (32) = 187 = 0xBB
            // For White: same (all 3 forwards and all 3 backwards are symmetric when combined)
            range_capability(
                0xBB,  // N, NE, NW, S, SE, SW (same for both colors)
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Forest Demon movement: Simple 3 sideways/forwards, range forwards diagonals/backwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Sideways E (4), W (64) = 68 = 0x44, Forwards N (1), Forwards diagonals NE (2), NW (128) = 130 = 0x82, Backwards S (16)
/// For White: Sideways same, Forwards S (16), Forwards diagonals SE (8), SW (32) = 40 = 0x28, Backwards N (1) (adjusted automatically)
pub fn forest_demon_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 3 spaces sideways and straight forwards
            // For Black: E (4), W (64), N (1) = 69 = 0x45
            // For White: E (4), W (64), S (16) = 84 = 0x54 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x45,  // E, W, N (for Black) - will be adjusted for White
                max_distance: 3,
            },
            // Range movement in forwards diagonal directions and straight backwards
            // For Black: NE (2), NW (128), S (16) = 146 = 0x92
            // For White: SE (8), SW (32), N (1) = 41 = 0x29 (adjusted automatically)
            range_capability(
                0x92,  // NE, NW, S (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Thunder Runner movement: Simple 4 sideways/backwards, range all 3 forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Sideways E (4), W (64) = 68 = 0x44, Backwards S (16), Forwards N (1), NE (2), NW (128) = 131 = 0x83
/// For White: Sideways same, Backwards N (1), Forwards S (16), SE (8), SW (32) = 56 = 0x38 (adjusted automatically)
pub fn thunder_runner_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 4 spaces sideways and straight backwards
            // For Black: E (4), W (64), S (16) = 84 = 0x54
            // For White: E (4), W (64), N (1) = 69 = 0x45 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x54,  // E, W, S (for Black) - will be adjusted for White
                max_distance: 4,
            },
            // Range movement in all 3 forwards directions
            // For Black: N (1), NE (2), NW (128) = 131 = 0x83
            // For White: S (16), SE (8), SW (32) = 56 = 0x38 (adjusted automatically)
            range_capability(
                0x83,  // N, NE, NW (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Fowl Officer movement: Simple 2 sideways/forwards, simple 3 diagonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Sideways E (4), W (64) = 68 = 0x44, Forwards N (1), Diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA
/// For White: Sideways same, Forwards S (16), Diagonals same (adjusted automatically)
pub fn fowl_officer_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces sideways and straight forwards
            // For Black: E (4), W (64), N (1) = 69 = 0x45
            // For White: E (4), W (64), S (16) = 84 = 0x54 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x45,  // E, W, N (for Black) - will be adjusted for White
                max_distance: 2,
            },
            // Simple movement: up to 3 spaces diagonally
            // For Black: NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA
            // For White: same (diagonals are symmetric)
            MovementCapability::Simple {
                directions: 0xAA,  // All diagonals (same for both colors)
                max_distance: 3,
            },
        ],
    }
}

/// Fowl movement: Simple 2 backwards, simple 3 sideways, range diagonally/forwards
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Backwards S (16), Sideways E (4), W (64) = 68 = 0x44, Diagonals NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA, Forwards N (1)
/// For White: Backwards N (1), Sideways same, Diagonals same, Forwards S (16) (adjusted automatically)
pub fn fowl_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces straight backwards
            // For Black: S (16)
            // For White: N (1) (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x10,  // S (16) for Black - will be adjusted for White
                max_distance: 2,
            },
            // Simple movement: up to 3 spaces sideways
            // For Black: E (4), W (64) = 68 = 0x44
            // For White: same (sideways are symmetric)
            MovementCapability::Simple {
                directions: 0x44,  // E, W (same for both colors)
                max_distance: 3,
            },
            // Range movement in diagonal directions and straight forwards
            // For Black: NE (2), SE (8), SW (32), NW (128), N (1) = 171 = 0xAB
            // For White: NE (2), SE (8), SW (32), NW (128), S (16) = 186 = 0xBA (adjusted automatically)
            range_capability(
                0xAB,  // All diagonals + N (for Black) - will be adjusted for White
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Turtledove movement: Simple 1 sideways/backwards, simple 5 forwards diagonals
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Sideways E (4), W (64) = 68 = 0x44, Backwards S (16), Forwards diagonals NE (2), NW (128) = 130 = 0x82
/// For White: Sideways same, Backwards N (1), Forwards diagonals SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
pub fn turtledove_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: 1 space sideways and straight backwards
            // For Black: E (4), W (64), S (16) = 84 = 0x54
            // For White: E (4), W (64), N (1) = 69 = 0x45 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x54,  // E, W, S (for Black) - will be adjusted for White
                max_distance: 1,
            },
            // Simple movement: up to 5 spaces in forwards diagonal directions
            // For Black: NE (2), NW (128) = 130 = 0x82
            // For White: SE (8), SW (32) = 40 = 0x28 (adjusted automatically)
            MovementCapability::Simple {
                directions: 0x82,  // NE, NW (for Black) - will be adjusted for White
                max_distance: 5,
            },
        ],
    }
}

/// White Elephant movement: Simple up to 2 spaces in all directions
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: All 8 directions = 255 = 0xFF
/// For White: same (all directions are symmetric)
pub fn white_elephant_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces in all 8 directions
            // For Black: N (1), NE (2), E (4), SE (8), S (16), SW (32), W (64), NW (128) = 255 = 0xFF
            // For White: same (all directions are symmetric)
            MovementCapability::Simple {
                directions: 0xFF,  // All 8 directions (same for both colors)
                max_distance: 2,
            },
        ],
    }
}

/// Elephant King movement: Simple 2 orthogonally, range diagonally
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: Orthogonal N (1), E (4), S (16), W (64) = 85 = 0x55
/// For White: same (orthogonal are symmetric)
/// Diagonals: NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA (same for both colors)
pub fn elephant_king_movement() -> MovementConfig {
    use crate::movement::types::MovementCapability;
    
    MovementConfig {
        capabilities: vec![
            // Simple movement: up to 2 spaces in all orthogonal directions
            // For Black: N (1), E (4), S (16), W (64) = 85 = 0x55
            // For White: same (orthogonal are symmetric)
            MovementCapability::Simple {
                directions: 0x55,  // All orthogonals (same for both colors)
                max_distance: 2,
            },
            // Range movement in all diagonal directions
            // For Black: NE (2), SE (8), SW (32), NW (128) = 170 = 0xAA
            // For White: same (diagonals are symmetric)
            range_capability(
                0xAA,  // All diagonals (same for both colors)
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Reverse Chariot movement: Range vertically (forwards and backwards)
/// Directions are relative to piece color (Black moves up, White moves down)
/// For Black: N (1), S (16) = 17 = 0x11
/// For White: same (forwards/backwards are symmetric)
pub fn reverse_chariot_movement() -> MovementConfig {
    use crate::movement::types::BlockingMode;
    
    MovementConfig {
        capabilities: vec![
            // Range movement in straight forwards and backwards directions
            // For Black: N (1), S (16) = 17 = 0x11
            // For White: same (forwards/backwards are symmetric)
            range_capability(
                0x11,  // N, S (same for both colors)
                BlockingMode::NoJump,
            ),

        ],
    }
}

/// Free Eagle movement: Range movement in all 8 directions, plus complex multi-move patterns
/// The Free Eagle has:
/// 1. Standard range movement in all 8 directions
/// 2. Multi-move patterns that can capture multiple pieces along a path
///    - Forward diagonals: up to 4 spaces
///    - Other directions: up to 3 spaces
/// 3. Special patterns:
///    - Forward diagonals: 3 forward + 1 back (only if capture on 3rd space)
///    - Any direction: 2 forward + 1 back (only if capture on 2nd space)
///    - Stay in place: capture enemy 1 space away in any direction
///    - Stay in place: capture enemies on 1st or 2nd space along forward diagonals (only if 2nd space has capture)
pub fn free_eagle_movement() -> MovementConfig {
    use crate::movement::types::{MovementCapability, BlockingMode};
    use crate::movement::direction::DIRECTION_SET_ALL;
    
    MovementConfig {
        capabilities: vec![
            // Standard range movement in all 8 directions
            range_capability(
                DIRECTION_SET_ALL,  // All 8 directions
                BlockingMode::NoJump,
            ),
            // Free Eagle multi-move capability
            MovementCapability::FreeEagleMultiMove {
                max_distance_forward_diagonal: 4,
                max_distance_other: 3,
            },
        ],
    }
}
