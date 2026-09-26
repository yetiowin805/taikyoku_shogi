#![allow(dead_code,unused_imports)]
mod piece {
#[derive(Debug,Clone,Copy,PartialEq,Eq,Hash)]
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
#[derive(Clone,Copy)] pub struct Piece { pub piece_type:PieceType, pub is_promoted:bool, pub base_piece_type:Option<PieceType> }
}
mod eval { use crate::piece::PieceType;
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
}
mod movement {
#[path="/tmp/search-light-results/micro/direction.rs"] pub mod direction;
#[path="/tmp/search-light-results/micro/types.rs"] pub mod types;
#[path="/tmp/search-light-results/micro/config.rs"] pub mod config;
}
use std::{collections::HashSet, hint::black_box};
use movement::{config::MovementConfig, types::{MovementCapability as Cap, BlockingMode}};
use piece::{Piece, PieceType};

#[derive(Clone, Copy)]
struct Bits([u64; 5]);
impl Bits {
    fn new(set: &HashSet<PieceType>) -> Self {
        let mut bits = [0; 5];
        for &p in set { bits[p as usize / 64] |= 1 << (p as usize % 64); }
        Self(bits)
    }
    fn contains(&self, p: PieceType) -> bool {
        self.0[p as usize / 64] & (1 << (p as usize % 64)) != 0
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Flags { two: bool, capture: bool, only_capture: bool, dirs: u8 }
fn flags(c: &MovementConfig) -> Flags {
    let mut f = Flags { two: false, capture: false, only_capture: false, dirs: 0 };
    let mut other = false;
    for cap in &c.capabilities {
        match cap {
            Cap::TwoStep { .. } => f.two = true,
            Cap::Range { directions, blocking, .. } => {
                f.dirs |= *directions;
                if *blocking == BlockingMode::Capturing { f.capture = true; } else { other = true; }
            }
            _ => (),
        }
    }
    f.only_capture = f.capture && !other;
    f
}
fn sets<'a>(cap: &'a Cap, out: &mut Vec<&'a HashSet<PieceType>>) {
    match cap {
        Cap::Range { cannot_jump_over, .. } => out.push(cannot_jump_over),
        Cap::TwoStep { first, second } => { sets(first, out); sets(second, out); }
        _ => (),
    }
}
#[repr(C)]
struct Timespec { sec: i64, ns: i64 }
unsafe extern "C" { fn clock_gettime(clock: i32, t: *mut Timespec) -> i32; }
fn cpu_ns() -> u64 {
    let mut t = Timespec { sec: 0, ns: 0 };
    assert_eq!(unsafe { clock_gettime(2, &mut t) }, 0);
    t.sec as u64 * 1_000_000_000 + t.ns as u64
}
fn measure(f: impl Fn() -> usize) -> (u64, usize) {
    let start = cpu_ns();
    let checksum = black_box(f());
    (cpu_ns() - start, checksum)
}
fn pair(name: &str, operations: usize, before: impl Fn() -> usize, after: impl Fn() -> usize) {
    assert_eq!(before(), after());
    for rep in 0..7 {
        let (a, b) = if rep % 2 == 0 { (measure(&before), measure(&after)) }
                     else { let b = measure(&after); let a = measure(&before); (a,b) };
        assert_eq!(a.1, b.1);
        println!("{{\"case\":\"{name}\",\"rep\":{rep},\"operations\":{operations},\"baseline_cpu_ns\":{},\"candidate_cpu_ns\":{},\"checksum\":{}}}", a.0, b.0, a.1);
    }
}
fn main() {
    let mut pieces: Vec<_> = eval::ALL_PIECE_TYPES.iter().map(|&t| Piece {
        piece_type: t, is_promoted: false, base_piece_type: None }).collect();
    pieces.push(Piece { piece_type: PieceType::RainDragon, is_promoted: true, base_piece_type: Some(PieceType::EarthDragon) });
    pieces.push(Piece { piece_type: PieceType::Whale, is_promoted: true, base_piece_type: Some(PieceType::ReverseChariot) });
    let configs: Vec<_> = pieces.iter().map(MovementConfig::for_piece).collect();
    let metadata: Vec<_> = configs.iter().map(|c| flags(c)).collect();
    let mut blocker_sets = Vec::new();
    for c in &configs { for cap in &c.capabilities { sets(cap, &mut blocker_sets); } }
    let bits: Vec<_> = blocker_sets.iter().map(|s| Bits::new(s)).collect();
    for (set, bit) in blocker_sets.iter().zip(&bits) {
        for &t in eval::ALL_PIECE_TYPES { assert_eq!(set.contains(&t), bit.contains(t)); }
    }
    eprintln!("validated {} configs, {} range sets against {} piece types", configs.len(), blocker_sets.len(), eval::ALL_PIECE_TYPES.len());
    let mut seed = 20260926u64;
    let mut next = || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed as usize };
    const ROUNDS: usize = 128;
    const N: usize = 4096;
    for (name, empty, positive) in [("blockers-uniform", false, false), ("blockers-half-hits", false, true), ("blockers-empty", true, false)] {
        let ids: Vec<_> = blocker_sets.iter().enumerate().filter(|(_,s)| s.is_empty() == empty).map(|(i,_)|i).collect();
        let queries: Vec<_> = (0..N).map(|i| {
            let set = ids[next() % ids.len()];
            // Sort randomized HashSet iteration to freeze the query stream.
            let piece = if positive && i%2==0 {
                let mut members: Vec<_> = blocker_sets[set].iter().copied().collect();
                members.sort_by_key(|p|*p as usize);
                members[next() % members.len()]
            } else { eval::ALL_PIECE_TYPES[next() % eval::ALL_PIECE_TYPES.len()] };
            (set, piece)
        }).collect();
        pair(name, N*ROUNDS, || {
            let mut count = 0;
            for _ in 0..ROUNDS { for &(i,p) in black_box(&queries) { count += blocker_sets[i].contains(&p) as usize; } }
            count
        }, || {
            let mut count = 0;
            for _ in 0..ROUNDS { for &(i,p) in black_box(&queries) { count += bits[i].contains(p) as usize; } }
            count
        });
    }
    let queries: Vec<_> = (0..N).map(|_|next()%configs.len()).collect();
    pair("metadata-two-step", N*ROUNDS, || {
        let mut count=0;
        for _ in 0..ROUNDS { for &i in black_box(&queries) {
            count += configs[i].capabilities.iter().any(|c|matches!(c, Cap::TwoStep{..})) as usize;
        } } count
    }, || {
        let mut count=0;
        for _ in 0..ROUNDS { for &i in black_box(&queries) { count += metadata[i].two as usize; } } count
    });
    pair("metadata-capturing", N*ROUNDS, || {
        let mut count=0;
        for _ in 0..ROUNDS { for &i in black_box(&queries) {
            count += configs[i].capabilities.iter().any(|c|matches!(c, Cap::Range{blocking:BlockingMode::Capturing,..})) as usize;
        } } count
    }, || {
        let mut count=0;
        for _ in 0..ROUNDS { for &i in black_box(&queries) { count += metadata[i].capture as usize; } } count
    });
    pair("metadata-range-mask", N*ROUNDS, || {
        let mut count=0;
        for _ in 0..ROUNDS { for &i in black_box(&queries) {
            let mut dirs=0;
            for cap in &configs[i].capabilities { if let Cap::Range{directions,..}=cap { dirs |= *directions; } }
            count += dirs as usize;
        } } count
    }, || {
        let mut count=0;
        for _ in 0..ROUNDS { for &i in black_box(&queries) { count += metadata[i].dirs as usize; } } count
    });
    for (name, density) in [("ray-dense", 3), ("ray-sparse", 16), ("ray-empty", 0)] {
        let mut lines = Vec::new();
        let mut masks = Vec::new();
        for _ in 0..64 {
            let mut line = [false; 36];
            let mut mask = 0u64;
            for (i, square) in line.iter_mut().enumerate() {
                *square = density > 0 && next() % density == 0;
                if *square { mask |= 1 << i; }
            }
            for from in 0..36 { for to in 0..36 {
                let lo = from.min(to); let hi = from.max(to);
                let clear = (lo+1..hi).all(|i| !line[i]);
                let segment = if hi <= lo+1 { 0 } else { ((1u64 << hi)-1) & !((1u64 << (lo+1))-1) };
                assert_eq!(clear, mask & segment == 0);
            } }
            lines.push(line); masks.push(mask);
        }
        let queries: Vec<_> = (0..N).map(|_|(next()%64, next()%36, next()%36)).collect();
        pair(name, N*ROUNDS, || {
            let mut count = 0;
            for _ in 0..ROUNDS { for &(i, from, to) in black_box(&queries) {
                let lo = from.min(to); let hi = from.max(to);
                count += (lo+1..hi).all(|sq| !lines[i][sq]) as usize;
            } } count
        }, || {
            let mut count = 0;
            for _ in 0..ROUNDS { for &(i, from, to) in black_box(&queries) {
                let lo = from.min(to); let hi = from.max(to);
                let segment = if hi <= lo+1 { 0 } else { ((1u64 << hi)-1) & !((1u64 << (lo+1))-1) };
                count += (masks[i] & segment == 0) as usize;
            } } count
        });
    }

}
