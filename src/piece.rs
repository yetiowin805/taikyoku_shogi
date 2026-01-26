use crate::position::Position;
use crate::movement::{MovementConfig, MovementGenerator};
use crate::board::Board;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Color {
    Black,  // Sente (first player)
    White,  // Gote (second player)
}

impl Color {
    pub fn opposite(&self) -> Color {
        match self {
            Color::Black => Color::White,
            Color::White => Color::Black,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PieceType {
    King,
    Pawn,
    GoldGeneral,  // Promoted pawn
    Dog,
    MixedGeneral,  // Promoted dog
    GoBetween,
    DrunkenElephant,  // Promoted go-between
    CrownPrince,  // Royal piece, promotes to King
    NeighboringKing,
    FrontStandard,  // Promoted neighboring king
    Rook,  // Promoted gold general (starting piece)
    LeftGeneral,
    RightGeneral,
    LeftArmy,  // Promoted left general
    RightArmy,  // Promoted right general
    RearStandard,
    CenterStandard,  // Promoted rear standard
    FreeKing,  // Queen-like movement
    GreatGeneral,  // Promoted free king
    FreeBaku,  // Range in all directions except sideways (limited to 5)
    FreeDemon,  // Range in all directions except forwards/backwards (limited to 5 in those)
    RunningHorse,  // Range forwards, 1 space backwards, jump 2 spaces backwards diagonals
    Tengu,  // Two-step move: both steps are range diagonal moves
    WoodenDove,  // Range diagonal, orthogonal up to 2, conditional diagonal jumps
    CeramicDove,  // Range diagonal, orthogonal up to 2
    EarthDragon,  // Range backwards diagonals, forward up to 2, forward diagonals 1, backwards 1
    RainDragon,  // Promoted earth dragon
    LeftMountainEagle,  // Range all except right backwards (simple 2), jump 2 in left diagonals
    RightMountainEagle,  // Range all except left backwards (simple 2), jump 2 in right diagonals
    FlyingEagle,  // Promoted mountain eagle (range all 8, jump 2 in forward diagonals)
    FireDemon,  // Range all except forwards/backwards (simple 2), promotes to Free Fire
    FreeFire,  // Promoted fire demon (range all except forwards/backwards, simple 5)
    Whale,  // Range in all 3 backwards directions and directly forwards, promotes to Great Whale
    GreatWhale,  // Promoted whale (range in all 3 forwards and all 3 backwards directions)
    RunningRabbit,  // Range in all 3 forwards, simple 1 in all 3 backwards, promotes to Treacherous Fox
    TreacherousFox,  // Promoted running rabbit (range in all 6 forwards and backwards directions), promotes to Mountain Crane
    MountainCrane,  // Promoted treacherous fox (range in all 8 directions)
    TurtleSnake,  // Range in forward-right and backward-left, simple 1 in other 6, promotes to Divine Turtle
    DivineTurtle,  // Promoted turtle-snake (range in forward-right, backward-left, backward-right, simple 1 in other 5)
    WhiteTiger,  // Range in sideways and forward-right, simple 2 in forward/backward, promotes to Divine Tiger
    DivineTiger,  // Promoted white tiger (range in sideways, forward-right, and forward, simple 2 in backward)
    Lance,  // Range forward only, must promote on opponent's back rank, promotes to White Foal
    WhiteFoal,  // Promoted lance (range in all 3 forwards and straight backwards)
    BeastCadet,  // Simple 2 in all directions except backwards, promotes to Beast Officer
    BeastOfficer,  // Promoted beast cadet (simple 3 in forwards and backwards diagonals, simple 2 sideways), promotes to Beast Bird
    BeastBird,  // Promoted beast officer (simple 2 backwards, simple 3 sideways, range diagonally and forwards)
    FlyingSwallow,  // Range in forward diagonals, simple 1 backwards, promotes to Rook
    GreatDragon,  // Promoted rain dragon (range diagonally, simple 3 forwards/backwards), promotes to Primordial Dragon
    PrimordialDragon,  // Range diagonally, jumping range vertically
    MountainStag,  // Simple forward 1, sideways 2, forward diagonals 3, backwards 4, promotes to Great Stag
    GreatStag,  // Promoted mountain stag (range orthogonal, backwards diagonal 2, forward diagonal jump 2)
    SilverGeneral,  // Simple 1 diagonally and straight forwards, promotes to Vertical Mover
    VerticalMover,  // Promoted silver general (range forwards/backwards, simple 1 sideways)
    Rikishi,  // Simple 3 diagonally, promotes to Shiten
    Kongou,  // Simple 3 orthogonally, promotes to Shiten
    Rasetsu,  // Simple 1 sideways/backwards, simple 3 forward diagonals, promotes to Shiten
    Yasha,  // Simple 1 forward diagonals/backwards, simple 3 sideways, promotes to Shiten
    Shiten,  // Promoted form of Rikishi, Kongou, Rasetsu, Yasha (simple 4 all directions)
    RunningBear,  // Simple 2 sideways, range forwards/backwards, promotes to Free Bear
    FreeBear,  // Promoted running bear (range in all 3 forwards and all 3 backwards directions)
    RunningTiger,  // Same as running bear (simple 2 sideways, range forwards/backwards), promotes to Free Tiger
    FreeTiger,  // Promoted running tiger (range in all directions except straight forwards)
    GreatDove,  // Simple 3 orthogonally, range diagonally, promotes to Wooden Dove
    SideSerpent,  // Simple 1 backwards, simple 3 forwards, range sideways, promotes to Great Shark
    GreatShark,  // Promoted side serpent (simple 2 backwards diagonals, simple 5 forwards diagonals, range orthogonal)
    RunningSerpent,  // Simple 1 sideways, range forwards/backwards, promotes to Free Serpent
    FreeSerpent,  // Promoted running serpent (range in all 3 backwards directions and straight forwards)
    RunningPup,  // Same as running serpent (simple 1 sideways, range forwards/backwards), promotes to Free Leopard
    FreeLeopard,  // Promoted running pup (range in all 3 forwards and all 3 backwards directions)
    ForestDemon,  // Simple 3 sideways/forwards, range forwards diagonals/backwards, promotes to Thunder Runner
    ThunderRunner,  // Promoted forest demon (simple 4 sideways/backwards, range all 3 forwards)
    FowlOfficer,  // Simple 2 sideways/forwards, simple 3 diagonally, promotes to Fowl
    Fowl,  // Promoted fowl officer (simple 2 backwards, simple 3 sideways, range diagonally/forwards)
    Turtledove,  // Simple 1 sideways/backwards, simple 5 forwards diagonals, promotes to Great Dove
    WhiteElephant,  // Simple 2 all directions, promotes to Elephant King
    FragrantElephant,  // Simple 2 all directions, promotes to Elephant King
    ElephantKing,  // Promoted white/fragrant elephant (simple 2 orthogonally, range diagonally)
    ReverseChariot,  // Range vertically (forwards/backwards), promotes to Whale
    LeftDragon,  // Simple 2 left, range in all 3 rightward directions, promotes to Vermillion Sparrow
    VermillionSparrow,  // Promoted left dragon (simple 1 orthogonally, forward-right/backward-left, range forward-left/backward-right), promotes to Divine Sparrow
    DivineSparrow,  // Promoted vermillion sparrow (range backwards left, simple 1 orthogonally and forward-right, range forward-left and backward-right)
    RightDragon,  // Simple 2 right, range in all 3 leftward directions, promotes to Blue Dragon
    BlueDragon,  // Promoted right dragon (simple 2 sideways, range vertically and forward-right), promotes to Divine Dragon
    DivineDragon,  // Promoted blue dragon (range straight right, simple 2 left, range vertically and forward-right)
    LeftTiger,  // Simple 1 in leftward diagonals, range in all 3 rightward directions, promotes to Turtle Snake
    RightTiger,  // Simple 1 in rightward diagonals, range in all 3 leftward directions, promotes to White Tiger
    FlyingGeneral,  // Range capturing movement orthogonally, blocked by blocking set 3, promotes to Flying Crocodile
    FlyingCrocodile,  // Promoted flying general (range capturing orthogonal, simple 2 backwards diagonals, simple 3 forwards diagonals)
    BishopGeneral,  // Range capturing movement diagonally, blocked by blocking set 3, promotes to Rain Demon
    RainDemon,  // Promoted bishop general (simple 2 sideways, simple 3 forwards, range backwards)
    KirinMaster,  // Simple 3 sideways, range in other 6 directions, jump forward/backward 3 spaces
    PhoenixMaster,  // Simple 3 sideways, range in other 6 directions, jump forward diagonals 3 spaces
    CopperGeneral,  // Simple 1 in all 3 forwards and straight backward, promotes to Horizontal Mover
    HorizontalMover,  // Promoted copper general (simple 1 vertically, range sideways)
    FireDragon,  // Backwards diagonals up to 2, forwards diagonals up to 4, range orthogonal, promotes to Kirin Master
    WaterDragon,  // Forwards diagonals up to 2, backwards diagonals up to 4, range orthogonal, promotes to Phoenix Master
    Peacock,  // Simple 2 backwards diagonals, two-step: forward diagonal then any diagonal (with restrictions), promotes to Tengu
    OldKite,  // Simple 1 sideways, simple 2 diagonally, promotes to Tengu
    RushingBird,  // Simple 1 in all directions except vertically, simple 2 straight forwards, promotes to Free Demon
    FreePup,  // Simple 1 backwards diagonals, simple 2 sideways, range backwards and all 3 forwards, promotes to Free Dog
    FreeDog,  // Promoted free pup (simple 2 backwards diagonals, simple 2 sideways, range backwards and all 3 forwards)
    WindDragon,  // Simple 1 backward left, range in other 3 diagonals and sideways, promotes to Free Dragon
    FreeDragon,  // Promoted wind dragon (range in all directions except straight forwards)
    RunningWolf,  // Simple 1 straight forwards, range in forward diagonals and sideways, promotes to Free Wolf
    FreeWolf,  // Promoted running wolf (range in all 3 forwards and both sideways)
    RunningStag,  // Simple 2 straight backwards, range in sideways and forward diagonals, promotes to Free Stag
    FreeStag,  // Promoted running stag (range in all directions)
    SideDragon,  // Range sideways and straight forward, promotes to Running Dragon
    RunningDragon,  // Promoted side dragon (simple 5 straight backwards, range in all other directions)
    GoldenChariot,  // Simple 1 diagonal, simple 2 sideways, range vertical, promotes to Playful Parrot
    PlayfulParrot,  // Promoted golden chariot (simple 2 backwards diagonal, simple 3 forwards diagonal, simple 5 sideways, range vertical)
    ViceGeneral,  // Capturing range diagonally (blocking set 2), jump 2 orthogonally, promotes to GreatGeneral, in blocking set 3
    WoodlandDemon,  // Simple 2 sideways and backwards diagonals, range in other 4 directions, promotes to Old Peng
    OldPeng,  // Promoted woodland demon (simple 5 sideways, range diagonally)
    FierceDragon,  // Capturing range diagonally (blocking set 3), simple 2 orthogonally, promotes to GreatDragon, in blocking set 3
    // Note: GreatDragon already exists (promoted rain dragon), FierceDragon also promotes to GreatDragon
    // GreatDragon is NOT in blocking set 3 (even though FierceDragon is)
    FowlCadet,  // Simple 3 in all directions except directly backwards, promotes to Fowl Officer
    Lion,  // Jump 2 steps away, two-step: 1 space any direction then 1 space any direction, promotes to Furious Fiend
    FuriousFiend,  // Promoted lion (jump 2 steps away, two-step: up to 3 spaces any direction then 1 space any direction)
    GoldStag,  // Simple 2 backwards diagonals, range forwards diagonals, promotes to White Foal
    SilverRabbit,  // Simple 2 forwards diagonals, range backwards diagonals, promotes to Whale
    SideBoar,  // Range sideways, simple 1 in all other directions, promotes to Free Boar
    FreeBoar,  // Simple 1 straight backwards, range in all 3 forwards and sideways
    OxGeneral,  // Simple 1 forward diagonal and straight backwards, simple 3 straight forwards, promotes to Free Ox
    FreeOx,  // Simple 1 backwards diagonals, simple 2 sideways, range in all 3 forwards and straight backwards
    HorseGeneral,  // Same movement as Ox General, promotes to Free Horse
    FreeHorse,  // Same movement as Free Ox
    PupGeneral,  // Simple 1 backwards diagonals, simple 4 straight forwards, promotes to Free Pup
    ChickenGeneral,  // Same movement as Pup General, promotes to Free Chicken
    FreeChicken,  // Simple 2 sideways and backwards diagonals, range in 3 forwards and straight backwards
    PigGeneral,  // Simple 2 straight backwards, simple 4 forward diagonals, promotes to Free Pig
    FreePig,  // Same movement as Free Pup, Free Horse, and Free Ox
    Knight,  // Jumping move 2 spaces forward and 1 space sideways (shogi knight), must promote on back 2 ranks, promotes to Side Soldier
    SideSoldier,  // Simple 1 space straight backwards, simple up to 2 spaces straight forwards, range movement sideways, promotes to Side General
    VerticalBear,  // Simple 1 straight backwards, simple 2 sideways, range straight forwards, promotes to Free Bear
    SilverChariot,  // Simple 1 backwards diagonals, simple 2 forwards diagonals, range vertically, promotes to Goose Wing
    GooseWing,  // Simple 1 diagonally, simple 3 sideways, range vertically
    Daiba,  // Simple 1 in all directions except upper right, promotes to King of Teachings
    KingOfTeachings,  // Jump up to 3 spaces in all directions
    DarkSpirit,  // Simple 1 in all directions except upper left, promotes to Buddhist Spirit
    BuddhistSpirit,  // Jump to all squares 2 steps away, two-step: range all directions then 1 space any direction
    GoldBird,  // Simple 3 sideways and backward diagonals, range vertically, jump up to 3 forward diagonals, promotes to Free Bird
    FreeBird,  // Range sideways, simple 3 backward diagonals, range vertically, jump up to 3 forward diagonals
    FierceOx,  // Simple 1 vertically, range forward diagonals, promotes to Flying Ox
    FlyingOx,  // Range all directions except sideways, promotes to Fire Ox
    FireOx,  // Simple 1 sideways, range in all other directions
    SheepSoldier,  // Simple 1 straight backwards, range forward diagonals, promotes to Tiger Soldier
    TigerSoldier,  // Simple 1 straight backwards, range forward diagonals, simple 2 straight forward
    RunningChariot,  // Range orthogonally, promotes to Cannon Chariot
    CannonChariot,  // Simple 1 sideways, range all 3 forward directions and straight backwards
    CopperChariot,  // Simple 3 forward diagonals, range vertically, promotes to Copper Elephant
    CopperElephant,  // Range vertically, simple 1 in all other directions
    CloudDragon,  // Simple 1 sideways and straight forward, range diagonally and straight backwards, promotes to Great Dragon
    LittleStandard,  // Simple 1 backwards diagonals, simple 2 forwards diagonals, range orthogonally, promotes to Rear Standard
    Soldier,  // Range orthogonally, promotes to Cavalier
    Cavalier,  // Range all directions except backwards diagonals
    VerticalTiger,  // Simple 2 straight backwards, range straight forwards, promotes to Free Tiger
    MountainHawk,  // Simple 2 backwards diagonals, range all other directions, jump 2 straight ahead, promotes to Horned Hawk
    HornedHawk,  // Range all directions, jump 2 straight forwards
    FlyingCat,  // Simple 1 all 3 backwards directions, jump 3 sideways and all 3 forwards, promotes to Rook
    SideWolf,  // Simple 1 forward left and backward right diagonals, range sideways, promotes to Free Wolf
    DragonKing,  // Range orthogonally, simple 1 diagonally
    CloudEagle,  // Simple 1 sideways, simple 3 forward diagonals, range vertically, promotes to Strong Eagle
    StrongEagle,  // Range all 8 directions
    StoneChariot,  // Simple 1 forwards diagonals, simple 2 sideways, range vertically, promotes to Walking Heron
    WalkingHeron,  // Simple 2 sideways and forwards diagonals, range vertically
    Bishop,  // Range diagonally, promotes to Dragon Horse
    DragonHorse,  // Range diagonally, simple 1 orthogonally
    VerticalHorse,  // Simple 1 forwards diagonal and straight backwards, range straight forwards, promotes to Dragon Horse
    VerticalPup,  // Simple 1 in all 3 backwards directions, range straight forwards, promotes to Leopard King
    LeopardKing,  // Simple up to 5 spaces in all directions
    LongbowSoldier,  // Simple 1 backwards, simple 2 sideways, simple 5 forwards diagonals, range forwards, promotes to Longbow General
    LongbowGeneral,  // Simple 5 sideways, range in all 3 forwards directions and straight backwards
    SideMonkey,  // Simple 1 forwards diagonally and straight backwards, range sideways, promotes to Side Soldier
    LeftChariot,  // Simple 1 leftwards, range straight forwards, forwards right, and backwards left, promotes to Left Iron Chariot
    LeftIronChariot,  // Simple 1 leftwards, range diagonally except forwards left
    RightChariot,  // Simple 1 rightwards, range straight forwards, forwards left, and backwards right, promotes to Right Iron Chariot
    RightIronChariot,  // Simple 1 rightwards, range diagonally except forwards right
    FreeEagle,  // Range all 8 directions, plus complex multi-move patterns with captures, does not promote
    CannonSoldier,  // Simple 1 backwards, simple 3 sideways, simple 5 forwards diagonals, simple 7 forwards, promotes to Cannon General
    CannonGeneral,  // Simple 2 backwards, simple 3 sideways, range forwards (all 3 directions)
    GreatTurtle,  // Simple 3 sideways, range all other directions, jump 3 straight forwards and backwards, promotes to Spirit Turtle
    SpiritTurtle,  // Range all directions, jump 3 in all 4 orthogonal directions
    LittleTurtle,  // Simple 3 sideways, range all other directions, jump 2 straight forwards and backwards, promotes to Treasure Turtle
    TreasureTurtle,  // Range all directions, jump 2 in all 4 orthogonal directions
    Capricorn,  // Same movement as Tengu (two-step: both steps are range diagonal moves), promotes to Hook Mover
    HookMover,  // Two-step: both steps are range orthogonal moves
    Kirin,  // Simple 1 in all directions except sideways, jump 2 sideways, promotes to Gold Bird
    Phoenix,  // Simple 1 orthogonally, jump 2 in all diagonal directions, promotes to Gold Bird
    FireGeneral,  // Simple 1 in forwards diagonal directions, simple 3 vertically, promotes to Great General
    WaterGeneral,  // Simple 1 vertically, simple 3 in forwards diagonal directions, promotes to Vice General
    BlindDog,  // Simple 1 in forwards diagonal directions, sideways, and straight backwards, promotes to Fierce Stag
    FierceStag,  // Same movement as Silver General, promotes to Moving Boar
    MovingBoar,  // Promoted fierce stag (simple 1 space in all directions except straight backwards)
    CrowMover,  // Simple 1 in backwards diagonal directions and straight forwards, promotes to Flying Hawk
    FlyingHawk,  // Simple 1 straight forwards, range diagonally
    FlyingGoose,  // Simple 1 in all 3 forwards directions and straight backwards, promotes to Swallow's Wings
    SwallowsWings,  // Simple 1 vertically, range sideways
    PoisonousSerpent,  // Simple 1 straight backwards and forwards diagonals, simple 2 straight forwards and sideways, promotes to Hook Mover
    FlyingDragon,  // Jump 2 spaces in all 4 diagonal directions, promotes to Dragon King
    FierceEagle,  // Simple 1 straight forwards and sideways, simple 2 diagonally, promotes to Flying Eagle
    FierceLeopard,  // Simple 1 in all directions except sideways, promotes to Bishop
    WaterOx,  // Simple 2 vertically, range in all other directions, promotes to Great Baku
    GreatBaku,  // Range in all directions, jump 3 sideways
    DancingStag,  // Simple 1 in all forwards directions and straight backwards, simple 2 sideways, promotes to Square Mover
    SquareMover,  // Range diagonally
    SideMover,  // Simple 1 vertically, range sideways, promotes to Free Boar
    LeftHowlingDog,  // Simple 1 straight backwards, range straight forwards, promotes to Left Dog
    RightHowlingDog,  // Simple 1 straight backwards, range straight forwards, promotes to Right Dog
    LeftDog,  // Same as howling dogs, plus range backwards right diagonal
    RightDog,  // Same as howling dogs, plus range backwards left diagonal
    GreatFoal,  // Same movement as White Foal, plus simple 2 sideways
    WoodChariot,  // Simple 1 forward left and backward right, range vertically, promotes to Wind Snapping Turtle
    WindSnappingTurtle,  // Simple 2 forwards diagonal, range vertically
    PengMaster,  // Simple 5 sideways and backwards diagonals, range in other 4 directions, jump 3 forward diagonals, does not promote
    CenterMaster,  // Simple 3 sideways and backwards diagonals, range in other 4 directions, jump 2 in 3 forward directions and straight backwards, does not promote
    FierceWolf,  // Same movement as Gold General, promotes to Bear's Eyes
    BearsEyes,  // Promoted fierce wolf (simple 1 space in all 8 directions)
    EasternBarbarian,  // Simple 1 sideways and forward diagonal, simple 2 vertically, promotes to Lion
    WesternBarbarian,  // Simple 1 sideways and forward diagonal, simple 2 vertically, promotes to Lion Dog
    LionDog,  // Promoted western barbarian (range in all 8 directions, jump 3 in all 8 directions)
    SouthernBarbarian,  // Simple 1 in all 3 forward directions and straight backward, simple 2 sideways, promotes to Gold Bird
    NorthernBarbarian,  // Simple 1 in all 3 forward directions and straight backward, simple 2 sideways, promotes to Wooden Dove
    LionHawk,  // Jump 2 steps away (same as Lion), two-step: first move 1 space orthogonally OR range diagonal, second move 1 space any direction, does not promote
    RecliningDragon,  // Simple 1 space orthogonally, promotes to Great Dragon
    CoiledSerpent,  // Simple 1 space straight forwards and all 3 backwards directions, promotes to Coiled Dragon
    CoiledDragon,  // Promoted coiled serpent (range movement in straight forwards and all 3 backwards directions)
    HuaiChicken,  // Simple 1 space forward diagonal, sideways, and straight backwards, promotes to Wizard Stork
    WizardStork,  // Promoted huai chicken (range movement in forward diagonal, sideways, and straight backwards)
    OldMonkey,  // Simple 1 space diagonally and straight backwards, promotes to Mountain Witch
    MountainWitch,  // Promoted old monkey (range movement diagonally and straight backwards)
    FlyingChicken,  // Simple 1 space forward diagonal and sideways, promotes to Raiding Hawk
    RaidingHawk,  // Promoted flying chicken (same movement as flying chicken, plus range movement straight forwards)
    WindHorse,  // Simple 1 forward diagonal, simple 2 straight backwards, range straight forwards, promotes to Heavenly Horse
    HeavenlyHorse,  // Promoted wind horse (range straight forwards, jump 4 knight movements forwards and backwards)
    EvilWolf,  // Simple 1 space in all directions except the 3 backwards directions, promotes to Poisonous Wolf
    PoisonousWolf,  // Promoted evil wolf (simple 1 space in all directions)
    AngryBoar,  // Simple 1 space diagonally and sideways, promotes to Free Boar
    FierceBear,  // Simple 1 space sideways, simple 2 spaces forwards diagonal, promotes to Great Bear
    GreatBear,  // Promoted fierce bear (simple 1 space orthogonally except forwards, range in all 3 forwards directions)
    FlyingHorse,  // Simple up to 2 spaces diagonally, promotes to Free King
    Donkey,  // Simple up to 2 spaces orthogonally, promotes to Ceramic Dove
    SideOx,  // Simple 1 space upper right and lower left, range sideways, promotes to Flying Ox
    VerticalWolf,  // Simple 1 space sideways, simple 3 spaces straight backwards, range straight forwards, promotes to Running Wolf
    TileChariot,  // Simple 1 space upper right and lower left, range vertically, promotes to Running Tile
    RunningTile,  // Promoted tile chariot (simple up to 2 spaces sideways, range vertically)
    StrongChariot,  // Promoted square mover (range movement orthogonally and forwards diagonally)
    OldRat,  // Simple 1 space straight forwards and backwards diagonally, promotes to Ji Bird
    JiBird,  // Promoted old rat (range movement in all 3 forwards directions and straight backwards)
    BlindBear,  // Simple 1 space in all directions except vertically, promotes to Flying Stag
    FlyingStag,  // Promoted blind bear (same movement as blind bear, plus range movement vertically)
    SideFlyer,  // Simple 1 space diagonally, range sideways, promotes to Side Dragon
    OxChariot,  // Range movement straight forwards, must promote on last rank, promotes to Plodding Ox
    PloddingOx,  // Promoted ox chariot (simple 1 space diagonally, range movement vertically)
    BlindTiger,  // Simple 1 space in all directions except straight forwards, promotes to Flying Stag
    BlindMonkey,  // Simple 1 space in all directions except vertically, promotes to Flying Stag
    SwallowMover,  // Promoted swallow's wings (range movement orthogonally)
    CatSword,  // Simple 1 space diagonally, promotes to Dragon Horse
    ClimbingMonkey,  // Simple 1 space in all 3 forwards directions and straight backwards, promotes to Fierce Stag
    OwlMover,  // Simple 1 space straight forward and backward diagonally, promotes to Cloud Eagle
    Horseman,  // Simple up to 2 spaces sideways, range in all 3 forward directions and straight backwards, promotes to Cavalier
    Tanuki,  // Simple up to 2 spaces orthogonally, promotes to Ceramic Dove
    EarthChariot,  // Simple 1 space sideways, range movement vertically, promotes to Reed Bird
    ReedBird,  // Promoted earth chariot (simple up to 2 spaces sideways and backwards diagonally, range movement vertically)
    GreatMaster,  // Simple up to 5 spaces sideways and backwards diagonally, range in other 4 directions, jump 3 in all 3 forwards directions
    GreatStandard,  // Simple up to 3 spaces backwards diagonally, range in other 6 directions
    IronGeneral,  // Simple 1 space in all 3 forward directions, promotes to Running Ox, must promote on last rank
    RunningOx,  // Promoted iron general (simple up to 2 spaces straight backwards, range sideways and in all 3 forwards directions)
    BearSoldier,  // Simple 1 space straight backwards, simple up to 2 spaces sideways, range in all 3 forwards directions, promotes to Strong Bear
    StrongBear,  // Promoted bear soldier (simple up to 2 spaces straight backwards, range in all other directions)
    TileGeneral,  // Simple 1 space in forwards diagonal and straight backwards directions, promotes to Running Ox
    LeopardSoldier,  // Same movement as bear soldier (simple 1 space straight backwards, simple up to 2 spaces sideways, range in all 3 forwards directions), promotes to Running Leopard
    RunningLeopard,  // Promoted leopard soldier (range movements in all directions except the 3 backwards directions)
    StoneGeneral,  // Simple 1 space in forward diagonal directions, promotes to Running Ox, must promote on last rank
    BoarSoldier,  // Same movement as bear soldier and leopard soldier (simple 1 space straight backwards, simple up to 2 spaces sideways, range in all 3 forwards directions), promotes to Running Boar
    RunningBoar,  // Promoted boar soldier (simple 1 space sideways, range movement vertically)
    EarthGeneral,  // Simple 1 space vertically, promotes to Running Ox
    OxSoldier,  // Same movement as bear, boar, and leopard soldiers except sideways movement is up to 3 spaces rather than 2, promotes to Running Ox
    WoodGeneral,  // Simple up to 2 spaces forwards diagonally, promotes to White Elephant
    HorseSoldier,  // Same movement as ox soldier (simple 1 space straight backwards, simple up to 3 spaces sideways, range in all 3 forwards directions), promotes to Running Horse
    MountainGeneral,  // Simple 1 space vertically, simple up to 3 spaces in forwards diagonal directions, promotes to Mount Tai
    MountTai,  // Promoted mountain general (simple up to 5 spaces orthogonally except backwards, range movement diagonally)
    RiverGeneral,  // Simple 1 space in forward diagonal and straight backward directions, simple up to 3 spaces straight forwards, promotes to Huai River
    HuaiRiver,  // Promoted river general (simple 1 space vertically, range movement in all other directions)
    WindGeneral,  // Same movement as river general (simple 1 space in forward diagonal and straight backward directions, simple up to 3 spaces straight forwards), promotes to Fierce Wind
    FierceWind,  // Promoted wind general (simple 1 space sideways, range movement in all other directions)
    VerticalSoldier,  // Simple 1 space straight backwards, simple up to 2 spaces sideways, range movement straight forwards, promotes to Chariot Soldier
    ChariotSoldier,  // Promoted vertical soldier (simple up to 2 spaces sideways, range movement in all other directions)
    SideGeneral,  // Promoted side soldier (simple up to 2 spaces vertically, range movement in all other directions)
    Shitennou,  // Promoted chariot soldier (jumping range movements in all directions)
    GreatElephant,  // Promoted lion dog (simple up to 3 spaces in forward diagonal directions, jump movement up to 3 spaces in all other directions)
    RoaringDog,  // Simple up to 3 spaces in backwards diagonal directions, range and jump 3 spaces in all other directions, promotes to Lion Dog
    CrossbowSoldier,  // Simple 1 space straight backwards, simple up to 3 spaces sideways and forwards diagonals, simple up to 5 spaces straight forwards, promotes to Crossbow General
    CrossbowGeneral,  // Promoted crossbow soldier (simple up to 2 spaces straight backwards, simple up to 3 spaces sideways, simple up to 5 spaces forwards diagonally, range movement straight forwards)
    FierceTiger,  // Range movement straight forwards, promotes to Great Tiger
    GreatTiger,  // Promoted fierce tiger (simple 1 space straight forwards, range movement in other orthogonal directions)
    VerticalLeopard,  // Simple 1 space in forwards diagonal, sideways, and straight backwards directions, range movement straight forwards, promotes to Great Leopard
    GreatLeopard,  // Promoted vertical leopard (simple 1 space straight backwards, simple 2 spaces sideways, simple 3 spaces in forwards diagonal directions, range movement straight forwards)
    SpearSoldier,  // Simple 1 space sideways and straight backwards, range movement straight forwards, promotes to Spear General
    SpearGeneral,  // Promoted spear soldier (simple up to 2 spaces straight backwards, simple up to 3 spaces sideways, range movement straight forwards)
    GreatEagle,  // Promoted flying eagle (jumping range movement in forward diagonal directions, normal range movement in all other directions)
    GreatHawk,  // Promoted horned hawk (jumping range movement straight forwards, normal range movement in all other directions)
    SwordSoldier,  // Simple 1 space in forwards diagonal and straight backwards directions, promotes to Sword General
    SwordGeneral,  // Promoted sword soldier (simple 1 space straight backwards, simple up to 3 spaces in forwards diagonal directions)
    // More piece types will be added later
}

impl PieceType {
    /// Get the piece type this piece promotes to, or None if it doesn't promote
    pub fn promotes_to(&self) -> Option<PieceType> {
        match self {
            PieceType::Pawn => Some(PieceType::GoldGeneral),
            PieceType::Dog => Some(PieceType::MixedGeneral),
            PieceType::GoBetween => Some(PieceType::DrunkenElephant),
            PieceType::DrunkenElephant => Some(PieceType::CrownPrince),
            PieceType::CrownPrince => Some(PieceType::King),
            PieceType::NeighboringKing => Some(PieceType::FrontStandard),
            PieceType::GoldGeneral => Some(PieceType::Rook),  // Starting gold general promotes to rook
            PieceType::LeftGeneral => Some(PieceType::LeftArmy),
            PieceType::RightGeneral => Some(PieceType::RightArmy),
            PieceType::RearStandard => Some(PieceType::CenterStandard),
            PieceType::FreeKing => Some(PieceType::GreatGeneral),
            PieceType::FreeBaku => Some(PieceType::FreeKing),
            PieceType::FreeDemon => Some(PieceType::FreeKing),
            PieceType::RunningHorse => Some(PieceType::FreeDemon),
            PieceType::EarthDragon => Some(PieceType::RainDragon),
            PieceType::LeftMountainEagle => Some(PieceType::FlyingEagle),
            PieceType::RightMountainEagle => Some(PieceType::FlyingEagle),
            PieceType::FireDemon => Some(PieceType::FreeFire),
            PieceType::Whale => Some(PieceType::GreatWhale),
            PieceType::RunningRabbit => Some(PieceType::TreacherousFox),
            PieceType::TreacherousFox => Some(PieceType::MountainCrane),
            PieceType::FierceWolf => Some(PieceType::BearsEyes),
            PieceType::TurtleSnake => Some(PieceType::DivineTurtle),
            PieceType::WhiteTiger => Some(PieceType::DivineTiger),
            PieceType::LeftTiger => Some(PieceType::TurtleSnake),
            PieceType::RightTiger => Some(PieceType::WhiteTiger),
            PieceType::FlyingGeneral => Some(PieceType::FlyingCrocodile),
            PieceType::BishopGeneral => Some(PieceType::RainDemon),
            PieceType::Lance => Some(PieceType::WhiteFoal),
            PieceType::BeastCadet => Some(PieceType::BeastOfficer),
            PieceType::FlyingSwallow => Some(PieceType::Rook),
            PieceType::RainDragon => Some(PieceType::GreatDragon),
            PieceType::MountainStag => Some(PieceType::GreatStag),
            PieceType::SilverGeneral => Some(PieceType::VerticalMover),
            PieceType::Rikishi => Some(PieceType::Shiten),
            PieceType::Kongou => Some(PieceType::Shiten),
            PieceType::Rasetsu => Some(PieceType::Shiten),
            PieceType::Yasha => Some(PieceType::Shiten),
            PieceType::RunningBear => Some(PieceType::FreeBear),
            PieceType::RunningTiger => Some(PieceType::FreeTiger),
            PieceType::GreatDove => Some(PieceType::WoodenDove),
            PieceType::SideSerpent => Some(PieceType::GreatShark),
            PieceType::RunningSerpent => Some(PieceType::FreeSerpent),
            PieceType::RunningPup => Some(PieceType::FreeLeopard),
            PieceType::ForestDemon => Some(PieceType::ThunderRunner),
            PieceType::FowlOfficer => Some(PieceType::Fowl),
            PieceType::Turtledove => Some(PieceType::GreatDove),
            PieceType::WhiteElephant => Some(PieceType::ElephantKing),
            PieceType::FragrantElephant => Some(PieceType::ElephantKing),
            PieceType::ReverseChariot => Some(PieceType::Whale),
            PieceType::LeftDragon => Some(PieceType::VermillionSparrow),
            PieceType::VermillionSparrow => Some(PieceType::DivineSparrow),
            PieceType::DivineSparrow => None,  // Already promoted
            PieceType::RightDragon => Some(PieceType::BlueDragon),
            PieceType::BlueDragon => Some(PieceType::DivineDragon),
            PieceType::DivineDragon => None,  // Already promoted
            PieceType::FlyingCrocodile => None,  // Already promoted
            PieceType::King => None,  // Kings don't promote
            PieceType::MixedGeneral => None,  // Already promoted
            PieceType::FrontStandard => Some(PieceType::GreatStandard),
            PieceType::Rook => Some(PieceType::DragonKing),  // Rook promotes to Dragon King
            PieceType::DragonKing => Some(PieceType::FlyingEagle),
            PieceType::CloudEagle => Some(PieceType::StrongEagle),
            PieceType::StrongEagle => None,  // Already promoted
            PieceType::StoneChariot => Some(PieceType::WalkingHeron),
            PieceType::WalkingHeron => None,  // Already promoted
            PieceType::Bishop => Some(PieceType::DragonHorse),
            PieceType::DragonHorse => Some(PieceType::HornedHawk),
            PieceType::GreatTurtle => Some(PieceType::SpiritTurtle),
            PieceType::SpiritTurtle => None,  // Already promoted
            PieceType::LittleTurtle => Some(PieceType::TreasureTurtle),
            PieceType::TreasureTurtle => None,  // Already promoted
            PieceType::Capricorn => Some(PieceType::HookMover),
            PieceType::HookMover => None,  // Already promoted
            PieceType::Kirin => Some(PieceType::GoldBird),
            PieceType::Phoenix => Some(PieceType::GoldBird),
            PieceType::FireGeneral => Some(PieceType::GreatGeneral),
            PieceType::WaterGeneral => Some(PieceType::ViceGeneral),
            PieceType::BlindDog => Some(PieceType::FierceStag),
            PieceType::FierceStag => Some(PieceType::MovingBoar),
            PieceType::MovingBoar => None,  // Already promoted
            PieceType::CrowMover => Some(PieceType::FlyingHawk),
            PieceType::FlyingHawk => None,  // Already promoted
            PieceType::FlyingGoose => Some(PieceType::SwallowsWings),
            PieceType::SwallowsWings => Some(PieceType::SwallowMover),
            PieceType::SwallowMover => None,  // Already promoted
            PieceType::CatSword => Some(PieceType::DragonHorse),
            PieceType::VerticalHorse => Some(PieceType::DragonHorse),
            PieceType::VerticalPup => Some(PieceType::LeopardKing),
            PieceType::LeopardKing => None,  // Already promoted
            PieceType::LongbowSoldier => Some(PieceType::LongbowGeneral),
            PieceType::LongbowGeneral => None,  // Already promoted
            PieceType::CannonSoldier => Some(PieceType::CannonGeneral),
            PieceType::CannonGeneral => None,  // Already promoted
            PieceType::ClimbingMonkey => Some(PieceType::FierceStag),
            PieceType::OwlMover => Some(PieceType::CloudEagle),
            PieceType::Horseman => Some(PieceType::Cavalier),
            PieceType::Tanuki => Some(PieceType::CeramicDove),
            PieceType::EarthChariot => Some(PieceType::ReedBird),
            PieceType::ReedBird => None,  // Already promoted
            PieceType::GreatMaster => None,  // Does not promote
            PieceType::GreatStandard => None,  // Does not promote
            PieceType::IronGeneral => Some(PieceType::RunningOx),
            PieceType::RunningOx => None,  // Already promoted
            PieceType::BearSoldier => Some(PieceType::StrongBear),
            PieceType::StrongBear => None,  // Already promoted
            PieceType::TileGeneral => Some(PieceType::RunningOx),
            PieceType::LeopardSoldier => Some(PieceType::RunningLeopard),
            PieceType::RunningLeopard => None,  // Already promoted
            PieceType::StoneGeneral => Some(PieceType::RunningOx),
            PieceType::BoarSoldier => Some(PieceType::RunningBoar),
            PieceType::RunningBoar => None,  // Already promoted
            PieceType::EarthGeneral => Some(PieceType::RunningOx),
            PieceType::OxSoldier => Some(PieceType::RunningOx),
            PieceType::WoodGeneral => Some(PieceType::WhiteElephant),
            PieceType::HorseSoldier => Some(PieceType::RunningHorse),
            PieceType::MountainGeneral => Some(PieceType::MountTai),
            PieceType::MountTai => None,  // Already promoted
            PieceType::RiverGeneral => Some(PieceType::HuaiRiver),
            PieceType::HuaiRiver => None,  // Already promoted
            PieceType::WindGeneral => Some(PieceType::FierceWind),
            PieceType::FierceWind => None,  // Already promoted
            PieceType::VerticalSoldier => Some(PieceType::ChariotSoldier),
            PieceType::ChariotSoldier => Some(PieceType::Shitennou),
            PieceType::Shitennou => None,  // Already promoted
            PieceType::LionDog => Some(PieceType::GreatElephant),
            PieceType::GreatElephant => None,  // Already promoted
            PieceType::RoaringDog => Some(PieceType::LionDog),
            PieceType::CrossbowSoldier => Some(PieceType::CrossbowGeneral),
            PieceType::CrossbowGeneral => None,  // Already promoted
            PieceType::FierceTiger => Some(PieceType::GreatTiger),
            PieceType::GreatTiger => None,  // Already promoted
            PieceType::VerticalLeopard => Some(PieceType::GreatLeopard),
            PieceType::GreatLeopard => None,  // Already promoted
            PieceType::SpearSoldier => Some(PieceType::SpearGeneral),
            PieceType::SpearGeneral => None,  // Already promoted
            PieceType::SwordSoldier => Some(PieceType::SwordGeneral),
            PieceType::SwordGeneral => None,  // Already promoted
            PieceType::PoisonousSerpent => Some(PieceType::HookMover),
            PieceType::FlyingDragon => Some(PieceType::DragonKing),
            PieceType::FierceEagle => Some(PieceType::FlyingEagle),
            PieceType::FierceLeopard => Some(PieceType::Bishop),
            PieceType::WaterOx => Some(PieceType::GreatBaku),
            PieceType::GreatBaku => None,  // Already promoted
            PieceType::DancingStag => Some(PieceType::SquareMover),
            PieceType::SquareMover => Some(PieceType::StrongChariot),
            PieceType::StrongChariot => None,  // Already promoted
            PieceType::SideMover => Some(PieceType::FreeBoar),
            PieceType::LeftHowlingDog => Some(PieceType::LeftDog),
            PieceType::RightHowlingDog => Some(PieceType::RightDog),
            PieceType::LeftDog => None,  // Already promoted
            PieceType::RightDog => None,  // Already promoted
            PieceType::LeftArmy => None,  // Already promoted
            PieceType::RightArmy => None,  // Already promoted
            PieceType::GreatGeneral => None,  // Already promoted
            PieceType::Tengu => None,  // Cannot promote
            PieceType::WoodenDove => None,  // Cannot promote
            PieceType::CeramicDove => None,  // Cannot promote
            PieceType::FlyingEagle => Some(PieceType::GreatEagle),
            PieceType::GreatEagle => None,  // Already promoted
            PieceType::RainDemon => None,  // Already promoted
            PieceType::KirinMaster => None,  // Does not promote
            PieceType::PhoenixMaster => None,  // Does not promote
            PieceType::CopperGeneral => Some(PieceType::HorizontalMover),
            PieceType::HorizontalMover => None,  // Already promoted
            PieceType::FireDragon => Some(PieceType::KirinMaster),
            PieceType::WaterDragon => Some(PieceType::PhoenixMaster),
            PieceType::Peacock => Some(PieceType::Tengu),
            PieceType::OldKite => Some(PieceType::Tengu),
            PieceType::RushingBird => Some(PieceType::FreeDemon),
            PieceType::FreePup => Some(PieceType::FreeDog),
            PieceType::FreeDog => None,  // Already promoted
            PieceType::WindDragon => Some(PieceType::FreeDragon),
            PieceType::FreeDragon => None,  // Already promoted
            PieceType::RunningWolf => Some(PieceType::FreeWolf),
            PieceType::FreeWolf => None,  // Already promoted
            PieceType::RunningStag => Some(PieceType::FreeStag),
            PieceType::FreeStag => None,  // Already promoted
            PieceType::GreatStag => Some(PieceType::FreeStag),
            PieceType::FowlCadet => Some(PieceType::FowlOfficer),
            PieceType::Lion => Some(PieceType::FuriousFiend),
            PieceType::FuriousFiend => None,  // Already promoted
            PieceType::GoldStag => Some(PieceType::WhiteFoal),
            PieceType::SilverRabbit => Some(PieceType::Whale),
            PieceType::SideBoar => Some(PieceType::FreeBoar),
            PieceType::FreeBoar => None,  // Already promoted
            PieceType::AngryBoar => Some(PieceType::FreeBoar),
            PieceType::FierceBear => Some(PieceType::GreatBear),
            PieceType::GreatBear => None,  // Already promoted
            PieceType::FlyingHorse => Some(PieceType::FreeKing),
            PieceType::Donkey => Some(PieceType::CeramicDove),
            PieceType::SideOx => Some(PieceType::FlyingOx),
            PieceType::VerticalWolf => Some(PieceType::RunningWolf),
            PieceType::TileChariot => Some(PieceType::RunningTile),
            PieceType::RunningTile => None,  // Already promoted
            PieceType::OldRat => Some(PieceType::JiBird),
            PieceType::JiBird => None,  // Already promoted
            PieceType::BlindBear => Some(PieceType::FlyingStag),
            PieceType::FlyingStag => None,  // Already promoted
            PieceType::SideFlyer => Some(PieceType::SideDragon),
            PieceType::OxChariot => Some(PieceType::PloddingOx),
            PieceType::PloddingOx => None,  // Already promoted
            PieceType::BlindTiger => Some(PieceType::FlyingStag),
            PieceType::BlindMonkey => Some(PieceType::FlyingStag),
            PieceType::CenterStandard => Some(PieceType::FrontStandard),  // Can promote to Front Standard
            PieceType::OxGeneral => Some(PieceType::FreeOx),
            PieceType::FreeOx => None,  // Already promoted
            PieceType::HorseGeneral => Some(PieceType::FreeHorse),
            PieceType::FreeHorse => None,  // Already promoted
            PieceType::PupGeneral => Some(PieceType::FreePup),
            PieceType::ChickenGeneral => Some(PieceType::FreeChicken),
            PieceType::FreeChicken => None,  // Already promoted
            PieceType::PigGeneral => Some(PieceType::FreePig),
            PieceType::FreePig => None,  // Already promoted
            PieceType::Knight => Some(PieceType::SideSoldier),
            PieceType::SideMonkey => Some(PieceType::SideSoldier),
            PieceType::SideSoldier => Some(PieceType::SideGeneral),  // Promotes to Side General
            PieceType::LeftChariot => Some(PieceType::LeftIronChariot),
            PieceType::LeftIronChariot => None,  // Already promoted
            PieceType::RightChariot => Some(PieceType::RightIronChariot),
            PieceType::RightIronChariot => None,  // Already promoted
            PieceType::SideGeneral => None,  // Already promoted
            PieceType::VerticalBear => Some(PieceType::FreeBear),
            PieceType::SilverChariot => Some(PieceType::GooseWing),
            PieceType::GooseWing => None,  // Already promoted
            PieceType::Daiba => Some(PieceType::KingOfTeachings),
            PieceType::KingOfTeachings => None,  // Already promoted
            PieceType::DarkSpirit => Some(PieceType::BuddhistSpirit),
            PieceType::BuddhistSpirit => None,  // Already promoted
            PieceType::GoldBird => Some(PieceType::FreeBird),
            PieceType::FreeBird => None,  // Already promoted
            PieceType::FierceOx => Some(PieceType::FlyingOx),
            PieceType::FlyingOx => Some(PieceType::FireOx),
            PieceType::FireOx => None,  // Already promoted
            PieceType::SheepSoldier => Some(PieceType::TigerSoldier),
            PieceType::TigerSoldier => None,  // Already promoted
            PieceType::RunningChariot => Some(PieceType::CannonChariot),
            PieceType::CannonChariot => None,  // Already promoted
            PieceType::CopperChariot => Some(PieceType::CopperElephant),
            PieceType::CopperElephant => None,  // Already promoted
            PieceType::CloudDragon => Some(PieceType::GreatDragon),
            PieceType::LittleStandard => Some(PieceType::RearStandard),
            PieceType::Soldier => Some(PieceType::Cavalier),
            PieceType::Cavalier => None,  // Already promoted
            PieceType::VerticalTiger => Some(PieceType::FreeTiger),
            PieceType::MountainHawk => Some(PieceType::HornedHawk),
            PieceType::HornedHawk => Some(PieceType::GreatHawk),
            PieceType::GreatHawk => None,  // Already promoted
            PieceType::FlyingCat => Some(PieceType::Rook),
            PieceType::SideWolf => Some(PieceType::FreeWolf),
            PieceType::SideDragon => Some(PieceType::RunningDragon),
            PieceType::RunningDragon => None,  // Already promoted
            PieceType::GoldenChariot => Some(PieceType::PlayfulParrot),
            PieceType::PlayfulParrot => None,  // Already promoted
            PieceType::ViceGeneral => Some(PieceType::GreatGeneral),
            PieceType::WoodlandDemon => Some(PieceType::OldPeng),
            PieceType::OldPeng => None,  // Already promoted
            PieceType::FierceDragon => Some(PieceType::GreatDragon),
            // GreatDragon promotes to PrimordialDragon
            PieceType::GreatDragon => Some(PieceType::PrimordialDragon),
            PieceType::PrimordialDragon => None,  // Already promoted
            PieceType::FreeFire => None,  // Already promoted
            PieceType::GreatWhale => None,  // Already promoted
            PieceType::MountainCrane => None,  // Already promoted
            PieceType::DivineTurtle => None,  // Already promoted
            PieceType::DivineTiger => None,  // Already promoted
            PieceType::WhiteFoal => Some(PieceType::GreatFoal),
            PieceType::GreatFoal => None,  // Already promoted
            PieceType::WoodChariot => Some(PieceType::WindSnappingTurtle),
            PieceType::WindSnappingTurtle => None,  // Already promoted
            PieceType::PengMaster => None,  // Does not promote
            PieceType::CenterMaster => None,  // Does not promote
            PieceType::BearsEyes => None,  // Already promoted
            PieceType::EasternBarbarian => Some(PieceType::Lion),
            PieceType::WesternBarbarian => Some(PieceType::LionDog),
            PieceType::SouthernBarbarian => Some(PieceType::GoldBird),
            PieceType::NorthernBarbarian => Some(PieceType::WoodenDove),
            PieceType::LionHawk => None,  // Does not promote
            PieceType::RecliningDragon => Some(PieceType::GreatDragon),
            PieceType::CoiledSerpent => Some(PieceType::CoiledDragon),
            PieceType::CoiledDragon => None,  // Already promoted
            PieceType::HuaiChicken => Some(PieceType::WizardStork),
            PieceType::WizardStork => None,  // Already promoted
            PieceType::OldMonkey => Some(PieceType::MountainWitch),
            PieceType::MountainWitch => None,  // Already promoted
            PieceType::FlyingChicken => Some(PieceType::RaidingHawk),
            PieceType::RaidingHawk => None,  // Already promoted
            PieceType::WindHorse => Some(PieceType::HeavenlyHorse),
            PieceType::HeavenlyHorse => None,  // Already promoted
            PieceType::EvilWolf => Some(PieceType::PoisonousWolf),
            PieceType::PoisonousWolf => None,  // Already promoted
            PieceType::BeastOfficer => Some(PieceType::BeastBird),
            PieceType::BeastBird => None,  // Already promoted
            // GreatStag promotes to FreeStag (handled above)
            PieceType::VerticalMover => Some(PieceType::FlyingOx),
            PieceType::FreeEagle => None,  // Does not promote
            PieceType::Shiten => None,  // Already promoted
            PieceType::FreeBear => None,  // Already promoted
            PieceType::FreeTiger => None,  // Already promoted
            PieceType::GreatShark => None,  // Already promoted
            PieceType::FreeSerpent => None,  // Already promoted
            PieceType::FreeLeopard => None,  // Already promoted
            PieceType::ThunderRunner => None,  // Already promoted
            PieceType::Fowl => None,  // Already promoted
            PieceType::ElephantKing => None,  // Already promoted
        }
    }
    
    /// Get the display symbol for this piece type (base form)
    pub fn display_symbol(&self) -> &'static str {
        match self {
            PieceType::King => "K",
            PieceType::Pawn => "P",
            PieceType::GoldGeneral => "G",
            PieceType::Dog => "D",
            PieceType::MixedGeneral => "MG",
            PieceType::GoBetween => "GB",
            PieceType::DrunkenElephant => "DE",
            PieceType::CrownPrince => "CP",
            PieceType::NeighboringKing => "NK",
            PieceType::FrontStandard => "SD",
            PieceType::Rook => "R",
            PieceType::DragonKing => "DK",
            PieceType::CloudEagle => "CE",
            PieceType::StrongEagle => "SE",
            PieceType::StoneChariot => "CI",
            PieceType::WalkingHeron => "WH",
            PieceType::Bishop => "B",
            PieceType::DragonHorse => "DH",
            PieceType::VerticalHorse => "VH",
            PieceType::VerticalPup => "VP",
            PieceType::LeopardKing => "LK",
            PieceType::LongbowSoldier => "LB",
            PieceType::LongbowGeneral => "LG",
            PieceType::CannonSoldier => "BN",
            PieceType::CannonGeneral => "CG",
            PieceType::GreatTurtle => "GT",
            PieceType::SpiritTurtle => "SP",
            PieceType::LittleTurtle => "LL",
            PieceType::TreasureTurtle => "TT",
            PieceType::Capricorn => "CA",
            PieceType::HookMover => "HM",
            PieceType::Kirin => "KR",
            PieceType::Phoenix => "PH",
            PieceType::FireGeneral => "F",
            PieceType::WaterGeneral => "WG",
            PieceType::BlindDog => "BI",
            PieceType::FierceStag => "VS",
            PieceType::MovingBoar => "MB",
            PieceType::CrowMover => "CM",
            PieceType::FlyingHawk => "",
            PieceType::FlyingGoose => "FY",
            PieceType::SwallowsWings => "SW",
            PieceType::SwallowMover => "SM",
            PieceType::CatSword => "CS",
            PieceType::ClimbingMonkey => "CM",
            PieceType::OwlMover => "OW",
            PieceType::Horseman => "HO",
            PieceType::Tanuki => "EB",
            PieceType::EarthChariot => "EC",
            PieceType::ReedBird => "RB",
            PieceType::GreatMaster => "GM",
            PieceType::GreatStandard => "GE",
            PieceType::IronGeneral => "I",
            PieceType::RunningOx => "RO",
            PieceType::BearSoldier => "BE",
            PieceType::StrongBear => "SB",
            PieceType::TileGeneral => "T",
            PieceType::LeopardSoldier => "LP",
            PieceType::RunningLeopard => "RL",
            PieceType::StoneGeneral => "SG",
            PieceType::BoarSoldier => "BS",
            PieceType::RunningBoar => "RB",
            PieceType::EarthGeneral => "EA",
            PieceType::OxSoldier => "OS",
            PieceType::WoodGeneral => "GN",
            PieceType::HorseSoldier => "HS",
            PieceType::MountainGeneral => "M",
            PieceType::MountTai => "MT",
            PieceType::RiverGeneral => "RE",
            PieceType::HuaiRiver => "HR",
            PieceType::WindGeneral => "WN",
            PieceType::FierceWind => "FW",
            PieceType::VerticalSoldier => "VR",
            PieceType::ChariotSoldier => "CH",
            PieceType::SideGeneral => "SG",
            PieceType::Shitennou => "SH",
            PieceType::GreatElephant => "GE",
            PieceType::RoaringDog => "DG",
            PieceType::CrossbowSoldier => "SC",
            PieceType::CrossbowGeneral => "CG",
            PieceType::FierceTiger => "TG",
            PieceType::GreatTiger => "GT",
            PieceType::VerticalLeopard => "VL",
            PieceType::GreatLeopard => "GL",
            PieceType::SpearSoldier => "SP",
            PieceType::SpearGeneral => "SG",
            PieceType::SwordSoldier => "SE",
            PieceType::SwordGeneral => "SW",
            PieceType::PoisonousSerpent => "PS",
            PieceType::FierceEagle => "EG",
            PieceType::FierceLeopard => "FL",
            PieceType::WaterOx => "WB",
            PieceType::GreatBaku => "GB",
            PieceType::DancingStag => "PR",
            PieceType::SquareMover => "SQ",
            PieceType::StrongChariot => "SC",
            PieceType::OldRat => "OR",
            PieceType::JiBird => "JB",
            PieceType::BlindBear => "BB",
            PieceType::FlyingStag => "FS",
            PieceType::SideFlyer => "SF",
            PieceType::OxChariot => "OC",
            PieceType::PloddingOx => "PO",
            PieceType::BlindTiger => "BT",
            PieceType::BlindMonkey => "BM",
            PieceType::SideMover => "SM",
            PieceType::LeftHowlingDog => "DL",
            PieceType::RightHowlingDog => "DR",
            PieceType::LeftDog => "LD",
            PieceType::RightDog => "RD",
            PieceType::GreatFoal => "GF",
            PieceType::WoodChariot => "WC",
            PieceType::WindSnappingTurtle => "WT",
            PieceType::PengMaster => "PE",
            PieceType::CenterMaster => "MT",
            PieceType::FierceWolf => "NT",
            PieceType::BearsEyes => "BE",
            PieceType::EasternBarbarian => "ES",
            PieceType::WesternBarbarian => "WS",
            PieceType::LionDog => "LD",
            PieceType::SouthernBarbarian => "SU",
            PieceType::NorthernBarbarian => "NB",
            PieceType::LionHawk => "LI",
            PieceType::RecliningDragon => "RD",
            PieceType::CoiledSerpent => "SN",
            PieceType::CoiledDragon => "CD",
            PieceType::HuaiChicken => "CC",
            PieceType::WizardStork => "WS",
            PieceType::OldMonkey => "OM",
            PieceType::MountainWitch => "MW",
            PieceType::FlyingChicken => "CK",
            PieceType::RaidingHawk => "RH",
            PieceType::WindHorse => "WI",
            PieceType::HeavenlyHorse => "HH",
            PieceType::EvilWolf => "EW",
            PieceType::PoisonousWolf => "PW",
            PieceType::AngryBoar => "AB",
            PieceType::FierceBear => "VB",
            PieceType::GreatBear => "GB",
            PieceType::FlyingHorse => "FH",
            PieceType::Donkey => "DO",
            PieceType::SideOx => "SX",
            PieceType::VerticalWolf => "VW",
            PieceType::TileChariot => "TC",
            PieceType::RunningTile => "RT",
            PieceType::LeftGeneral => "LG",
            PieceType::RightGeneral => "RG",
            PieceType::LeftArmy => "LA",
            PieceType::RightArmy => "",
            PieceType::RearStandard => "RS",
            PieceType::CenterStandard => "CN",
            PieceType::FreeKing => "Q",
            PieceType::GreatGeneral => "GG",
            PieceType::FreeBaku => "FB",
            PieceType::FreeDemon => "FR",
            PieceType::RunningHorse => "HR",
            PieceType::Tengu => "LO",
            PieceType::WoodenDove => "WO",
            PieceType::CeramicDove => "CD",
            PieceType::EarthDragon => "ED",
            PieceType::RainDragon => "RA",
            PieceType::LeftMountainEagle => "ML",
            PieceType::RightMountainEagle => "MR",
            PieceType::FlyingEagle => "EL",
            PieceType::FreeEagle => "FE",
            PieceType::GreatEagle => "GE",
            PieceType::FireDemon => "DM",
            PieceType::FreeFire => "FF",
            PieceType::Whale => "W",
            PieceType::GreatWhale => "GW",
            PieceType::RunningRabbit => "RR",
            PieceType::TreacherousFox => "TF",
            PieceType::MountainCrane => "MC",
            PieceType::TurtleSnake => "TS",
            PieceType::DivineTurtle => "DT",
            PieceType::WhiteTiger => "WT",
            PieceType::DivineTiger => "",
            PieceType::Lance => "L",
            PieceType::WhiteFoal => "WH",
            PieceType::BeastCadet => "BC",
            PieceType::BeastOfficer => "BO",
            PieceType::BeastBird => "BB",
            PieceType::FlyingSwallow => "FS",
            PieceType::GreatDragon => "GD",
            PieceType::PrimordialDragon => "PD",
            PieceType::MountainStag => "MS",
            PieceType::GreatStag => "GS",
            PieceType::SilverGeneral => "S",
            PieceType::VerticalMover => "VM",
            PieceType::Rikishi => "WR",
            PieceType::Kongou => "GU",
            PieceType::Rasetsu => "BD",
            PieceType::Yasha => "YA",
            PieceType::Shiten => "ST",
            PieceType::RunningBear => "BA",
            PieceType::FreeBear => "FB",
            PieceType::RunningTiger => "RT",
            PieceType::FreeTiger => "FT",
            PieceType::GreatDove => "GR",
            PieceType::SideSerpent => "SS",
            PieceType::GreatShark => "GS",
            PieceType::RunningPup => "RP",
            PieceType::FreeLeopard => "",
            PieceType::ForestDemon => "FO",
            PieceType::ThunderRunner => "TR",
            PieceType::FowlOfficer => "CO",
            PieceType::Fowl => "FW",
            PieceType::Turtledove => "TD",
            PieceType::WhiteElephant => "WE",
            PieceType::FragrantElephant => "FG",
            PieceType::ElephantKing => "EK",
            PieceType::ReverseChariot => "RV",
            PieceType::LeftDragon => "LE",
            PieceType::RightDragon => "RI",
            PieceType::VermillionSparrow => "VS",
            PieceType::DivineSparrow => "DS",
            PieceType::BlueDragon => "BL",
            PieceType::DivineDragon => "DD",
            PieceType::LeftTiger => "LT",
            PieceType::RightTiger => "TT",
            PieceType::FlyingGeneral => "RO",
            PieceType::FlyingCrocodile => "FC",
            PieceType::BishopGeneral => "BG",
            PieceType::RainDemon => "RD",
            PieceType::KirinMaster => "KM",
            PieceType::PhoenixMaster => "PM",
            PieceType::CopperGeneral => "C",
            PieceType::HorizontalMover => "",
            PieceType::FireDragon => "FI",
            PieceType::WaterDragon => "WA",
            PieceType::Peacock => "PC",
            PieceType::OldKite => "OK",
            PieceType::RushingBird => "RB",
            PieceType::FreePup => "FP",
            PieceType::FreeDog => "",
            PieceType::WindDragon => "WD",
            PieceType::FreeDragon => "",  // Free Dragon (to distinguish from FreeDog "FD")
            PieceType::RunningWolf => "RW",
            PieceType::FreeWolf => "",
            PieceType::RunningStag => "RN",
            PieceType::FreeStag => "",
            PieceType::SideDragon => "SI",
            PieceType::RunningDragon => "",  // Running Dragon (to distinguish from RainDemon "RD")
            PieceType::GoldenChariot => "GC",
            PieceType::PlayfulParrot => "PP",
            PieceType::ViceGeneral => "VG",
            PieceType::WoodlandDemon => "WL",
            PieceType::OldPeng => "OP",
            PieceType::FierceDragon => "VD",
            PieceType::RunningSerpent => "RU",
            PieceType::FreeSerpent => "FS",
            PieceType::FowlCadet => "CT",
            PieceType::Lion => "LN",
            PieceType::FuriousFiend => "FF",
            PieceType::GoldStag => "GL",
            PieceType::SilverRabbit => "SR",
            PieceType::SideBoar => "SA",
            PieceType::FreeBoar => "FB",
            PieceType::OxGeneral => "O",
            PieceType::FreeOx => "FO",
            PieceType::HorseGeneral => "H",
            PieceType::FreeHorse => "FH",
            PieceType::PupGeneral => "PG",
            PieceType::ChickenGeneral => "CG",
            PieceType::FreeChicken => "FC",
            PieceType::PigGeneral => "PI",
            PieceType::FreePig => "FPI",
            PieceType::Knight => "N",
            PieceType::SideMonkey => "MK",
            PieceType::SideSoldier => "SL",
            PieceType::LeftChariot => "LC",
            PieceType::LeftIronChariot => "",
            PieceType::RightChariot => "RC",
            PieceType::RightIronChariot => "",
            PieceType::VerticalBear => "VE",
            PieceType::SilverChariot => "SV",
            PieceType::GooseWing => "GW",
            PieceType::Daiba => "DV",
            PieceType::KingOfTeachings => "KT",
            PieceType::DarkSpirit => "DS",
            PieceType::BuddhistSpirit => "BS",
            PieceType::GoldBird => "GO",
            PieceType::FreeBird => "FB",
            PieceType::FierceOx => "VO",
            PieceType::FlyingOx => "OX",
            PieceType::FireOx => "",
            PieceType::SheepSoldier => "HE",
            PieceType::TigerSoldier => "TS",
            PieceType::RunningChariot => "RH",
            PieceType::CannonChariot => "CC",
            PieceType::CopperChariot => "CR",
            PieceType::CopperElephant => "CE",
            PieceType::CloudDragon => "CL",
            PieceType::LittleStandard => "LS",
            PieceType::Soldier => "SO",
            PieceType::Cavalier => "CA",
            PieceType::VerticalTiger => "VT",
            PieceType::MountainHawk => "MH",
            PieceType::HornedHawk => "HF",
            PieceType::GreatHawk => "GH",
            PieceType::FlyingCat => "FC",
            PieceType::SideWolf => "WF",
            PieceType::FlyingDragon => "FD",
        }
    }
    
    /// Check if this piece type is a royal piece (game ends when all are captured)
    pub fn is_royal(&self) -> bool {
        match self {
            PieceType::King | PieceType::CrownPrince => true,
            _ => false,
        }
    }

    /// Get movement configuration for this piece type
    /// Note: For pieces that have different movement when promoted (like RainDragon),
    /// use Piece::movement_config() instead which takes promotion status into account
    pub fn movement_config(&self) -> MovementConfig {
        MovementConfig::for_piece_type(*self)
    }
    
    /// Check if this piece type must promote when moving to the target rank
    /// This is piece-specific: some pieces (like pawns and dogs) must promote on the final rank
    pub fn must_promote_on_rank(&self, target_rank: u8, color: Color) -> bool {
        match self {
            PieceType::Pawn | PieceType::Dog | PieceType::Lance |PieceType::IronGeneral | PieceType::StoneGeneral | PieceType::FierceTiger=> {
                // Pawns, dogs, iron general, and stone general must promote on the final rank
                match color {
                    Color::Black => target_rank == 35,  // Final rank for Black
                    Color::White => target_rank == 0,   // Final rank for White
                }
            }
            PieceType::Knight => {
                // Knight must promote on the back 2 ranks (cannot move backwards)
                match color {
                    Color::Black => target_rank <= 1,  // Back 2 ranks for Black (0, 1)
                    Color::White => target_rank >= 34,  // Back 2 ranks for White (34, 35)
                }
            }
            _ => false,  // Other pieces don't have mandatory promotion
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Piece {
    pub piece_type: PieceType,
    pub color: Color,
    pub position: Position,
    pub is_promoted: bool,  // Whether this piece has been promoted
    #[serde(default)]
    pub base_piece_type: Option<PieceType>,  // The piece type before promotion (None if never promoted)
}

impl Piece {
    pub fn new(piece_type: PieceType, color: Color, position: Position) -> Piece {
        Piece {
            piece_type,
            color,
            position,
            is_promoted: false,
            base_piece_type: None,  // Starting pieces have no base type
        }
    }

    /// Promote this piece to its promoted form
    pub fn promote(&mut self) {
        if let Some(promoted_type) = self.piece_type.promotes_to() {
            // Store the current piece type as the base type before promoting
            if self.base_piece_type.is_none() {
                self.base_piece_type = Some(self.piece_type);
            }
            self.piece_type = promoted_type;
            self.is_promoted = true;
        }
    }

    /// Get the base piece type for display/notation purposes
    /// Returns what the piece was before promotion, or the current type if not promoted
    pub fn base_type(&self) -> PieceType {
        if let Some(base_type) = self.base_piece_type {
            // Use the stored base type if available
            base_type
        } else {
            // Not promoted or no stored base type, return current type
            self.piece_type
        }
    }

    /// Get the display symbol for this piece (base type symbol)
    /// For promoted pieces, shows the base type symbol
    /// For unpromoted pieces, shows the current type symbol
    pub fn base_symbol(&self) -> &'static str {
        self.base_type().display_symbol()
    }

    /// Check if this piece can move to the target position (basic bounds check only)
    /// This doesn't check for blocking pieces or game rules
    /// Uses the movement system to determine if the target is reachable
    pub fn can_reach(&self, target: Position, board: &Board) -> bool {
        let config = MovementConfig::for_piece(self);
        let targets = MovementGenerator::generate_targets(self, board, &config.capabilities);
        targets.contains(&target)
    }
    
    /// Check if this piece can reach the target position using BoardLike trait
    /// This version works with both Board and VirtualBoard
    pub fn can_reach_boardlike<B: crate::move_simulation::BoardLike>(&self, target: Position, board: &B) -> bool {
        let config = MovementConfig::for_piece(self);
        let targets = MovementGenerator::generate_targets(self, board, &config.capabilities);
        targets.contains(&target)
    }

    /// Get all potential target squares for this piece (without checking for blocking or other pieces)
    /// Uses the movement system to generate targets
    pub fn get_potential_targets(&self, board: &Board) -> Vec<Position> {
        let config = MovementConfig::for_piece(self);
        MovementGenerator::generate_targets(self, board, &config.capabilities)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_king_movement() {
        let board = Board::new();
        let king = Piece::new(PieceType::King, Color::Black, Position::new(10, 10).unwrap());
        
        // Should be able to move to adjacent squares
        assert!(king.can_reach(Position::new(10, 11).unwrap(), &board));
        assert!(king.can_reach(Position::new(11, 10).unwrap(), &board));
        assert!(king.can_reach(Position::new(9, 9).unwrap(), &board));
        
        // Should not be able to move to same square
        assert!(!king.can_reach(Position::new(10, 10).unwrap(), &board));
        
        // Should be able to move up to 2 squares
        assert!(king.can_reach(Position::new(12, 10).unwrap(), &board)); // 2 squares right
        assert!(king.can_reach(Position::new(10, 12).unwrap(), &board)); // 2 squares up
        assert!(king.can_reach(Position::new(12, 12).unwrap(), &board)); // 2 squares diagonally
        
        // Should not be able to move more than 2 squares
        assert!(!king.can_reach(Position::new(13, 10).unwrap(), &board));
    }

    #[test]
    fn test_pawn_movement_black() {
        let board = Board::new();
        let pawn = Piece::new(PieceType::Pawn, Color::Black, Position::new(10, 10).unwrap());
        
        // Forward move only
        assert!(pawn.can_reach(Position::new(10, 11).unwrap(), &board));
        
        // Cannot move diagonally
        assert!(!pawn.can_reach(Position::new(9, 11).unwrap(), &board));
        assert!(!pawn.can_reach(Position::new(11, 11).unwrap(), &board));
        
        // Cannot move backward
        assert!(!pawn.can_reach(Position::new(10, 9).unwrap(), &board));
        
        // Cannot move sideways
        assert!(!pawn.can_reach(Position::new(11, 10).unwrap(), &board));
    }

    #[test]
    fn test_pawn_movement_white() {
        let board = Board::new();
        let pawn = Piece::new(PieceType::Pawn, Color::White, Position::new(10, 10).unwrap());
        
        // Forward move only (decreasing rank for white)
        assert!(pawn.can_reach(Position::new(10, 9).unwrap(), &board));
        
        // Cannot move diagonally
        assert!(!pawn.can_reach(Position::new(9, 9).unwrap(), &board));
        assert!(!pawn.can_reach(Position::new(11, 9).unwrap(), &board));
        
        // Cannot move backward
        assert!(!pawn.can_reach(Position::new(10, 11).unwrap(), &board));
        
        // Cannot move sideways
        assert!(!pawn.can_reach(Position::new(11, 10).unwrap(), &board));
    }
}

