use crate::board::Board;
use crate::piece::{Piece, PieceType, Color};
use crate::position::Position;
use crate::movement::MovementConfig;
use crate::path_utils;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MoveData {
    Standard,
    TwoStep { intermediate: Position },
    FreeEagle { path: Vec<Position> },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Move {
    pub from: Position,
    pub to: Position,
    pub promoted: bool,  // Whether this move resulted in promotion
    pub data: MoveData,
}

impl Move {
    pub fn new(from: Position, to: Position) -> Move {
        Move { from, to, promoted: false, data: MoveData::Standard }
    }

    pub fn new_with_promotion(from: Position, to: Position, promoted: bool) -> Move {
        Move { from, to, promoted, data: MoveData::Standard }
    }
    
    pub fn new_two_step(from: Position, intermediate: Position, to: Position) -> Move {
        Move { from, to, promoted: false, data: MoveData::TwoStep { intermediate } }
    }
    
    pub fn new_two_step_with_promotion(from: Position, intermediate: Position, to: Position, promoted: bool) -> Move {
        Move { from, to, promoted, data: MoveData::TwoStep { intermediate } }
    }
    
    pub fn new_free_eagle(from: Position, to: Position, path: Vec<Position>) -> Move {
        Move { from, to, promoted: false, data: MoveData::FreeEagle { path } }
    }
    
    // Helper methods for accessing enum data
    pub fn intermediate(&self) -> Option<Position> {
        match &self.data {
            MoveData::TwoStep { intermediate } => Some(*intermediate),
            _ => None,
        }
    }
    
    pub fn free_eagle_path(&self) -> Option<&Vec<Position>> {
        match &self.data {
            MoveData::FreeEagle { path } => Some(path),
            _ => None,
        }
    }
    
    pub fn is_two_step(&self) -> bool {
        matches!(self.data, MoveData::TwoStep { .. })
    }
    
    pub fn is_free_eagle(&self) -> bool {
        matches!(self.data, MoveData::FreeEagle { .. })
    }
}

/// Undo token for [`GameState::make_move_for_search`] / [`GameState::unmake_move_for_search`].
#[derive(Debug, Clone)]
pub struct SearchUndo {
    #[allow(dead_code)]
    from: Position,
    final_to: Position,
    original_mover: Piece,
    removed: Vec<(Position, Piece)>,
    prev_turn: Color,
    prev_draw: u32,
}

#[derive(Clone)]
pub struct GameState {
    board: Board,
    current_turn: Color,
    move_history: Vec<Move>,
    turns_without_capture_or_promotion: u32,  // Counter for draw by 500-move rule
}

enum ApplyOutcome {
    Failed,
    Ok {
        intermediate: Option<Position>,
        final_to: Position,
    },
}

impl GameState {
    pub fn new() -> GameState {
        GameState {
            board: Board::new(),
            current_turn: Color::Black,
            move_history: Vec::new(),
            turns_without_capture_or_promotion: 0,
        }
    }

    pub fn get_board(&self) -> &Board {
        &self.board
    }

    pub fn get_board_mut(&mut self) -> &mut Board {
        &mut self.board
    }

    pub fn get_current_turn(&self) -> Color {
        self.current_turn
    }

    pub fn set_current_turn(&mut self, turn: Color) {
        self.current_turn = turn;
    }

    /// Place a piece on the board (for initial setup)
    pub fn place_piece(&mut self, piece: Piece) {
        self.board.place_piece(piece);
    }

    /// Remove a piece from the board, if any
    pub fn remove_piece(&mut self, pos: Position) -> Option<Piece> {
        let piece = self.board.get_piece(pos);
        if piece.is_some() {
            self.board.remove_piece(pos);
        }
        piece
    }

    /// Clear all pieces from the board (keeps turn / history / draw counter)
    pub fn clear_board(&mut self) {
        self.board = Board::new();
    }

    pub fn get_turns_without_capture_or_promotion(&self) -> u32 {
        self.turns_without_capture_or_promotion
    }

    pub fn set_turns_without_capture_or_promotion(&mut self, turns: u32) {
        self.turns_without_capture_or_promotion = turns;
    }

    /// Replace board and turn from a snapshot, clearing in-engine move history
    pub fn restore_position(&mut self, board: Board, turn: Color, draw_counter: u32) {
        self.board = board;
        self.current_turn = turn;
        self.move_history.clear();
        self.turns_without_capture_or_promotion = draw_counter;
    }

    /// Mirror a position across both axes (for White setup)
    /// Black position (file, rank) -> White position (35 - file, 35 - rank)
    fn mirror_position(file: u8, rank: u8) -> (u8, u8) {
        (35 - file, 35 - rank)
    }

    /// Place a piece for Black and its mirrored version for White
    /// This ensures White's setup is an exact mirror of Black's
    fn place_piece_mirrored(&mut self, piece_type: PieceType, file: u8, rank: u8) {
        // Place Black piece
        let black_piece = Piece::new(piece_type, Color::Black, Position::new(file, rank).unwrap());
        self.place_piece(black_piece);
        
        // Place White piece at mirrored position
        let (white_file, white_rank) = Self::mirror_position(file, rank);
        let white_piece = Piece::new(piece_type, Color::White, Position::new(white_file, white_rank).unwrap());
        self.place_piece(white_piece);
    }

    /// Place pieces from a list, automatically mirroring for White
    /// Takes a vector of (file, rank, Option<PieceType>) for Black positions
    fn place_pieces_mirrored(&mut self, pieces: Vec<(u8, u8, Option<PieceType>)>) {
        for (file, rank, piece_type_opt) in pieces {
            if let Some(piece_type) = piece_type_opt {
                self.place_piece_mirrored(piece_type, file, rank);
            }
        }
    }

    /// Set up the initial board position with kings and pawns
    /// - Kings: file 16 (17th from left), on back ranks (rank 0 for Black, rank 35 for White)
    /// - Pawns: on rank 10 for Black, rank 25 for White, all files
    pub fn setup_initial_position(&mut self) {
        // Clear the board first
        self.board = Board::new();
        
        // Place back rank pieces in order (files 0-35 for Black, automatically mirrored for White)
        // Note: This includes King, CrownPrince, GoldGenerals, LeftGeneral, RightGeneral, and RearStandards
        // Order: L, TS, RR, W, DM, ML, LO, BC, HR, FR, ED, CD, FB, Q, RS, LG, G, K, CP, G, RG, RS, Q, FB, WO, ED, FR, HR, BC, LO, MR, DM, W, RR, WT, L
        let back_rank: Vec<(u8, u8, Option<PieceType>)> = vec![
            (0, 0, Some(PieceType::Lance)),           // L
            (1, 0, Some(PieceType::TurtleSnake)),     // TS
            (2, 0, Some(PieceType::RunningRabbit)),   // RR
            (3, 0, Some(PieceType::Whale)),           // W
            (4, 0, Some(PieceType::FireDemon)),       // DM
            (5, 0, Some(PieceType::LeftMountainEagle)), // ML
            (6, 0, Some(PieceType::Tengu)),           // LO
            (7, 0, Some(PieceType::BeastCadet)),      // BC
            (8, 0, Some(PieceType::RunningHorse)),    // HR
            (9, 0, Some(PieceType::FreeDemon)),       // FR
            (10, 0, Some(PieceType::EarthDragon)),    // ED
            (11, 0, Some(PieceType::CeramicDove)),    // CD
            (12, 0, Some(PieceType::FreeBaku)),       // FB
            (13, 0, Some(PieceType::FreeKing)),       // Q
            (14, 0, Some(PieceType::RearStandard)),   // RS
            (15, 0, Some(PieceType::LeftGeneral)),    // LG
            (16, 0, Some(PieceType::GoldGeneral)),    // G
            (17, 0, Some(PieceType::King)),            // K
            (18, 0, Some(PieceType::CrownPrince)),    // CP
            (19, 0, Some(PieceType::GoldGeneral)),    // G
            (20, 0, Some(PieceType::RightGeneral)),   // RG
            (21, 0, Some(PieceType::RearStandard)),   // RS
            (22, 0, Some(PieceType::FreeKing)),       // Q
            (23, 0, Some(PieceType::FreeBaku)),       // FB
            (24, 0, Some(PieceType::WoodenDove)),     // WO
            (25, 0, Some(PieceType::EarthDragon)),    // ED
            (26, 0, Some(PieceType::FreeDemon)),      // FR
            (27, 0, Some(PieceType::RunningHorse)),   // HR
            (28, 0, Some(PieceType::BeastCadet)),      // BC
            (29, 0, Some(PieceType::Tengu)),          // LO
            (30, 0, Some(PieceType::RightMountainEagle)), // MR
            (31, 0, Some(PieceType::FireDemon)),      // DM
            (32, 0, Some(PieceType::Whale)),          // W
            (33, 0, Some(PieceType::RunningRabbit)),  // RR
            (34, 0, Some(PieceType::WhiteTiger)),     // WT
            (35, 0, Some(PieceType::Lance)),          // L
        ];
        
        self.place_pieces_mirrored(back_rank);
        
        // Place 2nd rank pieces in order (files 0-35 for Black, automatically mirrored for White)
        // Order: RV, WE, TD, FS, CO, RA, FO, MS, RP, RU, SS, GR, RT, BA, BD, WR, S, NK, DE, S, GU, YA, BA, RT, GR, SS, RU, RP, MS, FO, RA, CO, FS, TD, FG, RV
        let second_rank: Vec<(u8, u8, Option<PieceType>)> = vec![
            (0, 1, Some(PieceType::ReverseChariot)),  // RV
            (1, 1, Some(PieceType::WhiteElephant)),  // WE
            (2, 1, Some(PieceType::Turtledove)),  // TD
            (3, 1, Some(PieceType::FlyingSwallow)),  // FS
            (4, 1, Some(PieceType::FowlOfficer)),  // CO
            (5, 1, Some(PieceType::RainDragon)),  // RA
            (6, 1, Some(PieceType::ForestDemon)),  // FO
            (7, 1, Some(PieceType::MountainStag)),  // MS
            (8, 1, Some(PieceType::RunningPup)),  // RP
            (9, 1, Some(PieceType::RunningSerpent)),  // RU
            (10, 1, Some(PieceType::SideSerpent)),  // SS
            (11, 1, Some(PieceType::GreatDove)),  // GR
            (12, 1, Some(PieceType::RunningTiger)),  // RT
            (13, 1, Some(PieceType::RunningBear)),  // BA
            (14, 1, Some(PieceType::Rasetsu)),  // BD
            (15, 1, Some(PieceType::Rikishi)),  // WR
            (16, 1, Some(PieceType::SilverGeneral)),  // S
            (17, 1, Some(PieceType::NeighboringKing)),  // NK
            (18, 1, Some(PieceType::DrunkenElephant)),  // DE
            (19, 1, Some(PieceType::SilverGeneral)),  // S
            (20, 1, Some(PieceType::Kongou)),  // GU
            (21, 1, Some(PieceType::Yasha)),  // YA
            (22, 1, Some(PieceType::RunningBear)),  // BA
            (23, 1, Some(PieceType::RunningTiger)),  // RT
            (24, 1, Some(PieceType::GreatDove)),  // GR
            (25, 1, Some(PieceType::SideSerpent)),  // SS
            (26, 1, Some(PieceType::RunningSerpent)),  // RU
            (27, 1, Some(PieceType::RunningPup)),  // RP
            (28, 1, Some(PieceType::MountainStag)),  // MS
            (29, 1, Some(PieceType::ForestDemon)),  // FO
            (30, 1, Some(PieceType::RainDragon)),  // RA
            (31, 1, Some(PieceType::FowlOfficer)),  // CO
            (32, 1, Some(PieceType::FlyingSwallow)),  // FS
            (33, 1, Some(PieceType::Turtledove)),  // TD
            (34, 1, Some(PieceType::FragrantElephant)),  // FG
            (35, 1, Some(PieceType::ReverseChariot)),  // RV
        ];
        
        self.place_pieces_mirrored(second_rank);
        
        // Place 3rd rank pieces in order (files 0-35 for Black, automatically mirrored for White)
        // Order: GC, SI, RN, RW, BG, RO, LT, LE, BO, WD, FP, RB, OK, PC, WA, FI, C, KM, PM, C, FI, WA, PC, OK, RB, FP, WD, BO, RI, TT, RO, BG, RW, RN, SI, GC
        let third_rank: Vec<(u8, u8, Option<PieceType>)> = vec![
            (0, 2, Some(PieceType::GoldenChariot)),  // GC
            (1, 2, Some(PieceType::SideDragon)),  // SI
            (2, 2, Some(PieceType::RunningStag)),  // RN
            (3, 2, Some(PieceType::RunningWolf)),  // RW
            (4, 2, Some(PieceType::BishopGeneral)),  // BG
            (5, 2, Some(PieceType::FlyingGeneral)),  // RO
            (6, 2, Some(PieceType::LeftTiger)),  // LT
            (7, 2, Some(PieceType::LeftDragon)),  // LE
            (8, 2, Some(PieceType::BeastOfficer)),  // BO
            (9, 2, Some(PieceType::WindDragon)),  // WD
            (10, 2, Some(PieceType::FreePup)),  // FP
            (11, 2, Some(PieceType::RushingBird)),  // RB
            (12, 2, Some(PieceType::OldKite)),  // OK
            (13, 2, Some(PieceType::Peacock)),  // PC
            (14, 2, Some(PieceType::WaterDragon)),  // WA
            (15, 2, Some(PieceType::FireDragon)),  // FI
            (16, 2, Some(PieceType::CopperGeneral)),  // C
            (17, 2, Some(PieceType::KirinMaster)),  // KM
            (18, 2, Some(PieceType::PhoenixMaster)),  // PM
            (19, 2, Some(PieceType::CopperGeneral)),  // C
            (20, 2, Some(PieceType::FireDragon)),  // FI
            (21, 2, Some(PieceType::WaterDragon)),  // WA
            (22, 2, Some(PieceType::Peacock)),  // PC
            (23, 2, Some(PieceType::OldKite)),  // OK
            (24, 2, Some(PieceType::RushingBird)),  // RB
            (25, 2, Some(PieceType::FreePup)),  // FP
            (26, 2, Some(PieceType::WindDragon)),  // WD
            (27, 2, Some(PieceType::BeastOfficer)),  // BO
            (28, 2, Some(PieceType::RightDragon)),  // RI
            (29, 2, Some(PieceType::RightTiger)),  // TT
            (30, 2, Some(PieceType::FlyingGeneral)),  // RO
            (31, 2, Some(PieceType::BishopGeneral)),  // BG
            (32, 2, Some(PieceType::RunningWolf)),  // RW
            (33, 2, Some(PieceType::RunningStag)),  // RN
            (34, 2, Some(PieceType::SideDragon)),  // SI
            (35, 2, Some(PieceType::GoldenChariot)),  // GC
        ];
        
        self.place_pieces_mirrored(third_rank);
        
        // Place 4th rank pieces in order (files 0-35 for Black, automatically mirrored for White)
        // Order: SV, VE, N, PI, CG, PG, H, O, CN, SA, SR, GL, LN, CT, GS, VD, WL, GG, VG, WL, VD, GS, CT, LN, GL, SR, SA, CN, O, H, PG, CG, PI, N, VE, SV
        let fourth_rank: Vec<(u8, u8, Option<PieceType>)> = vec![
            (0, 3, Some(PieceType::SilverChariot)),   // SV - Silver Chariot
            (1, 3, Some(PieceType::VerticalBear)),   // VE - Vertical Bear
            (2, 3, Some(PieceType::Knight)),  // N - Knight
            (3, 3, Some(PieceType::PigGeneral)),   // PI - Pig General
            (4, 3, Some(PieceType::ChickenGeneral)),   // CG - Chicken General
            (5, 3, Some(PieceType::PupGeneral)),   // PG - Pup General
            (6, 3, Some(PieceType::HorseGeneral)),   // H - Horse General
            (7, 3, Some(PieceType::OxGeneral)),   // O - Ox General
            (8, 3, Some(PieceType::CenterStandard)),   // CN - Center Standard
            (9, 3, Some(PieceType::SideBoar)),   // SA - Side Boar
            (10, 3, Some(PieceType::SilverRabbit)), // SR - Silver Rabbit
            (11, 3, Some(PieceType::GoldStag)), // GL - Gold Stag
            (12, 3, Some(PieceType::Lion)), // LN - Lion
            (13, 3, Some(PieceType::FowlCadet)), // CT - Fowl Cadet
            (14, 3, Some(PieceType::GreatStag)), // GS - GreatStag
            (15, 3, Some(PieceType::FierceDragon)),  // VD - Fierce Dragon
            (16, 3, Some(PieceType::WoodlandDemon)),  // WL - Woodland Demon
            (17, 3, Some(PieceType::GreatGeneral)), // GG - GreatGeneral
            (18, 3, Some(PieceType::ViceGeneral)), // VG - ViceGeneral
            (19, 3, Some(PieceType::WoodlandDemon)),  // WL - Woodland Demon
            (20, 3, Some(PieceType::FierceDragon)), // VD - Fierce Dragon
            (21, 3, Some(PieceType::GreatStag)), // GS - GreatStag
            (22, 3, Some(PieceType::FowlCadet)),  // CT - Fowl Cadet
            (23, 3, Some(PieceType::Lion)),  // LN - Lion
            (24, 3, Some(PieceType::GoldStag)),  // GL - Gold Stag
            (25, 3, Some(PieceType::SilverRabbit)),  // SR - Silver Rabbit
            (26, 3, Some(PieceType::SideBoar)),  // SA - Side Boar
            (27, 3, Some(PieceType::CenterStandard)),  // CN - Center Standard
            (28, 3, Some(PieceType::OxGeneral)),  // O - Ox General
            (29, 3, Some(PieceType::HorseGeneral)),  // H - Horse General
            (30, 3, Some(PieceType::PupGeneral)),  // PG - Pup General
            (31, 3, Some(PieceType::ChickenGeneral)),  // CG - Chicken General
            (32, 3, Some(PieceType::PigGeneral)),  // PI - Pig General
            (33, 3, Some(PieceType::Knight)), // N - Knight
            (34, 3, Some(PieceType::VerticalBear)),  // VE - Vertical Bear
            (35, 3, Some(PieceType::SilverChariot)),  // SV - Silver Chariot
        ];
        self.place_pieces_mirrored(fourth_rank);
        
        // Place 5th rank pieces in order (files 0-35 for Black, automatically mirrored for White)
        // Order: CI, CE, B, R, WF, FC, MF, VT, SO, LS, CL, CR, RH, HE, VO, GD, GO, DV, DS, GO, GD, VO, HE, RH, CR, CL, LS, SO, VT, MF, FC, WF, R, B, CE, CI
        let fifth_rank: Vec<(u8, u8, Option<PieceType>)> = vec![
            (0, 4, Some(PieceType::StoneChariot)),   // CI - Stone Chariot
            (1, 4, Some(PieceType::CloudEagle)),   // CE - Cloud Eagle
            (2, 4, Some(PieceType::Bishop)),   // B - Bishop
            (3, 4, Some(PieceType::Rook)),   // R - Rook
            (4, 4, Some(PieceType::SideWolf)),   // WF - Side Wolf
            (5, 4, Some(PieceType::FlyingCat)),   // FC - Flying Cat
            (6, 4, Some(PieceType::MountainHawk)),   // MH - Mountain Hawk
            (7, 4, Some(PieceType::VerticalTiger)),   // VT - Vertical Tiger
            (8, 4, Some(PieceType::Soldier)),   // SO - Soldier
            (9, 4, Some(PieceType::LittleStandard)),   // LS - Little Standard
            (10, 4, Some(PieceType::CloudDragon)),  // CL - Cloud Dragon
            (11, 4, Some(PieceType::CopperChariot)),  // CR - Copper Chariot
            (12, 4, Some(PieceType::RunningChariot)),  // RH - Running Chariot
            (13, 4, Some(PieceType::SheepSoldier)),  // HE - Sheep Soldier
            (14, 4, Some(PieceType::FierceOx)),  // VO - Fierce Ox
            (15, 4, Some(PieceType::GreatDragon)),  // GD - Great Dragon
            (16, 4, Some(PieceType::GoldBird)),  // GO - Gold Bird
            (17, 4, Some(PieceType::Daiba)),  // DV - Daiba
            (18, 4, Some(PieceType::DarkSpirit)),  // DS - Dark Spirit
            (19, 4, Some(PieceType::GoldBird)),  // GO - Gold Bird (mirrored)
            (20, 4, Some(PieceType::GreatDragon)),  // GD - Great Dragon (mirrored)
            (21, 4, Some(PieceType::FierceOx)),  // VO - Fierce Ox (mirrored)
            (22, 4, Some(PieceType::SheepSoldier)),  // HE - Sheep Soldier (mirrored)
            (23, 4, Some(PieceType::RunningChariot)),  // RH - Running Chariot (mirrored)
            (24, 4, Some(PieceType::CopperChariot)),  // CR - Copper Chariot (mirrored)
            (25, 4, Some(PieceType::CloudDragon)),  // CL - Cloud Dragon (mirrored)
            (26, 4, Some(PieceType::LittleStandard)),  // LS - Little Standard (mirrored)
            (27, 4, Some(PieceType::Soldier)),  // SO - Soldier (mirrored)
            (28, 4, Some(PieceType::VerticalTiger)),  // VT - Vertical Tiger (mirrored)
            (29, 4, Some(PieceType::MountainHawk)),  // MH - Mountain Hawk (mirrored)
            (30, 4, Some(PieceType::FlyingCat)),  // FC - Flying Cat (mirrored)
            (31, 4, Some(PieceType::SideWolf)),  // WF - Side Wolf (mirrored)
            (32, 4, Some(PieceType::Rook)),  // R - Rook (mirrored)
            (33, 4, Some(PieceType::Bishop)),  // B - Bishop (mirrored)
            (34, 4, Some(PieceType::CloudEagle)),  // CE - Cloud Eagle (mirrored)
            (35, 4, Some(PieceType::StoneChariot)),  // CI - Stone Chariot (mirrored)
        ];
        self.place_pieces_mirrored(fifth_rank);
        
        // Place 6th rank pieces in order (files 0-35 for Black, automatically mirrored for White)
        // Order: WC, WH, DL, SM, PR, WB, FL, EG, FD, PS, FY, ST, BI, WG, F, KR, CA, GT, LL, HM, PH, F, WG, BI, ST, FY, PS, FD, EG, FL, WB, PR, SM, DR, WH, WC
        let sixth_rank: Vec<(u8, u8, Option<PieceType>)> = vec![
            (0, 5, Some(PieceType::WoodChariot)),   // WC - Wood Chariot
            (1, 5, Some(PieceType::WhiteFoal)),   // WH - White Foal
            (2, 5, Some(PieceType::LeftHowlingDog)),   // DL - Left Howling Dog
            (3, 5, Some(PieceType::SideMover)),   // SM - Side Mover
            (4, 5, Some(PieceType::DancingStag)),   // PR - Dancing Stag
            (5, 5, Some(PieceType::WaterOx)),   // WB - Water Ox
            (6, 5, Some(PieceType::FierceLeopard)),   // FL - Fierce Leopard
            (7, 5, Some(PieceType::FierceEagle)),   // EG - Fierce Eagle
            (8, 5, Some(PieceType::FlyingDragon)),   // FD - Flying Dragon
            (9, 5, Some(PieceType::PoisonousSerpent)),   // PS - Poisonous Serpent
            (10, 5, Some(PieceType::FlyingGoose)),  // FY - Flying Goose
            (11, 5, Some(PieceType::CrowMover)),  // ST - Crow Mover
            (12, 5, Some(PieceType::BlindDog)),  // BI - Blind Dog
            (13, 5, Some(PieceType::WaterGeneral)),  // WG - Water General
            (14, 5, Some(PieceType::FireGeneral)),  // F - Fire General
            (15, 5, Some(PieceType::Kirin)),  // KR - Kirin
            (16, 5, Some(PieceType::Capricorn)),  // CA - Capricorn
            (17, 5, Some(PieceType::GreatTurtle)),  // GT - Great Turtle
            (18, 5, Some(PieceType::LittleTurtle)),  // LL - Little Turtle
            (19, 5, Some(PieceType::HookMover)),  // HM - Hook Mover
            (20, 5, Some(PieceType::Phoenix)),  // PH - Phoenix
            (21, 5, Some(PieceType::FireGeneral)),  // F - Fire General (mirrored)
            (22, 5, Some(PieceType::WaterGeneral)),  // WG - Water General (mirrored)
            (23, 5, Some(PieceType::BlindDog)),  // BI - Blind Dog
            (24, 5, Some(PieceType::CrowMover)),  // ST - Crow Mover
            (25, 5, Some(PieceType::FlyingGoose)),  // FY - Flying Goose
            (26, 5, Some(PieceType::PoisonousSerpent)),  // PS - Poisonous Serpent
            (27, 5, Some(PieceType::FlyingDragon)),  // FD - Flying Dragon
            (28, 5, Some(PieceType::FierceEagle)),  // EG - Fierce Eagle
            (29, 5, Some(PieceType::FierceLeopard)),  // FL - Fierce Leopard
            (30, 5, Some(PieceType::WaterOx)),  // WB - Water Ox
            (31, 5, Some(PieceType::DancingStag)),  // PR - Dancing Stag
            (32, 5, Some(PieceType::SideMover)),  // SM - Side Mover
            (33, 5, Some(PieceType::RightHowlingDog)),  // DR - Right Howling Dog
            (34, 5, Some(PieceType::WhiteFoal)),  // WH - White Foal
            (35, 5, Some(PieceType::WoodChariot)),  // WC - Wood Chariot (mirrored)
        ];
        self.place_pieces_mirrored(sixth_rank);
        
        // Place 7th rank pieces in order (files 0-35 for Black, automatically mirrored for White)
        // Order: TC, VW, SX, DO, FH, VB, AB, EW, WI, CK, OM, CC, WS, ES, VS, NT, TF, PE, MT, TF, NT, VS, SU, NB, CC, OM, CK, WI, EW, AB, VB, FH, DO, SX, VW, TC
        let seventh_rank: Vec<(u8, u8, Option<PieceType>)> = vec![
            (0, 6, Some(PieceType::TileChariot)),  // TC - Tile Chariot
            (1, 6, Some(PieceType::VerticalWolf)),  // VW - Vertical Wolf
            (2, 6, Some(PieceType::SideOx)),  // SX - Side Ox
            (3, 6, Some(PieceType::Donkey)),  // DO - Donkey
            (4, 6, Some(PieceType::FlyingHorse)),  // FH - Flying Horse
            (5, 6, Some(PieceType::FierceBear)),  // VB - Fierce Bear
            (6, 6, Some(PieceType::AngryBoar)),  // AB - Angry Boar
            (7, 6, Some(PieceType::EvilWolf)),  // EW - Evil Wolf
            (8, 6, Some(PieceType::WindHorse)),  // WI - Wind Horse
            (9, 6, Some(PieceType::FlyingChicken)),  // CK - Flying Chicken
            (10, 6, Some(PieceType::OldMonkey)),  // OM - Old Monkey
            (11, 6, Some(PieceType::HuaiChicken)),  // CC - Huai Chicken
            (12, 6, Some(PieceType::WesternBarbarian)),  // WS - Western Barbarian
            (13, 6, Some(PieceType::EasternBarbarian)),  // ES - Eastern Barbarian
            (14, 6, Some(PieceType::FierceStag)),  // VS - Fierce Stag
            (15, 6, Some(PieceType::FierceWolf)),  // NT - Fierce Wolf
            (16, 6, Some(PieceType::TreacherousFox)),  // TF - Treacherous Fox
            (17, 6, Some(PieceType::PengMaster)),  // PE - Peng Master
            (18, 6, Some(PieceType::CenterMaster)),  // MT - Center Master
            (19, 6, Some(PieceType::TreacherousFox)),  // TF - Treacherous Fox
            (20, 6, Some(PieceType::FierceWolf)),  // NT - Fierce Wolf
            (21, 6, Some(PieceType::FierceStag)),  // VS - Fierce Stag
            (22, 6, Some(PieceType::SouthernBarbarian)),  // SU - Southern Barbarian
            (23, 6, Some(PieceType::NorthernBarbarian)),  // NB - Northern Barbarian
            (24, 6, Some(PieceType::HuaiChicken)),  // CC - Huai Chicken
            (25, 6, Some(PieceType::OldMonkey)),  // OM - Old Monkey
            (26, 6, Some(PieceType::FlyingChicken)),  // CK - Flying Chicken
            (27, 6, Some(PieceType::WindHorse)),  // WI - Wind Horse
            (28, 6, Some(PieceType::EvilWolf)),  // EW - Evil Wolf
            (29, 6, Some(PieceType::AngryBoar)),  // AB - Angry Boar
            (30, 6, Some(PieceType::FierceBear)),  // VB - Fierce Bear
            (31, 6, Some(PieceType::FlyingHorse)),  // FH - Flying Horse
            (32, 6, Some(PieceType::Donkey)),  // DO - Donkey
            (33, 6, Some(PieceType::SideOx)),  // SX - Side Ox
            (34, 6, Some(PieceType::VerticalWolf)),  // VW - Vertical Wolf
            (35, 6, Some(PieceType::TileChariot)),  // TC - Tile Chariot
        ];
        self.place_pieces_mirrored(seventh_rank);
        
        // Place 8th rank pieces in order (files 0-35 for Black, automatically mirrored for White)
        // Order: EC, BL, EB, HO, OW, CM, CS, SW, BM, BT, OC, SF, BB, OR, SQ, SN, RD, LI, FE, RD, SN, SQ, OR, BB, SF, OC, BT, BM, SW, CS, CM, OW, HO, EB, VI, EC
        let eighth_rank: Vec<(u8, u8, Option<PieceType>)> = vec![
            (0, 7, Some(PieceType::EarthChariot)),  // EC - Earth Chariot
            (1, 7, Some(PieceType::BlueDragon)),   // BL - Blue Dragon
            (2, 7, Some(PieceType::Tanuki)),  // EB - Tanuki
            (3, 7, Some(PieceType::Horseman)),  // HO - Horseman
            (4, 7, Some(PieceType::OwlMover)),  // OW - Owl Mover
            (5, 7, Some(PieceType::ClimbingMonkey)),  // CM - Climbing Monkey
            (6, 7, Some(PieceType::CatSword)),  // CS - Cat Sword
            (7, 7, Some(PieceType::SwallowsWings)),   // SW - Swallow's Wings
            (8, 7, Some(PieceType::BlindMonkey)),  // BM - Blind Monkey
            (9, 7, Some(PieceType::BlindTiger)),   // BT - Blind Tiger
            (10, 7, Some(PieceType::OxChariot)),  // OC - Ox Chariot
            (11, 7, Some(PieceType::SideFlyer)),  // SF - Side Flyer
            (12, 7, Some(PieceType::BlindBear)),  // BB - Blind Bear
            (13, 7, Some(PieceType::OldRat)),  // OR - Old Rat
            (14, 7, Some(PieceType::SquareMover)),  // SQ - Square Mover
            (15, 7, Some(PieceType::CoiledSerpent)),  // SN - Coiled Serpent
            (16, 7, Some(PieceType::RecliningDragon)),  // RD - Reclining Dragon
            (17, 7, Some(PieceType::LionHawk)),  // LI - Lion Hawk
            (18, 7, Some(PieceType::FreeEagle)),  // FE - Free Eagle
            (19, 7, Some(PieceType::RecliningDragon)),  // RD - Reclining Dragon
            (20, 7, Some(PieceType::CoiledSerpent)),  // SN - Coiled Serpent
            (21, 7, Some(PieceType::SquareMover)),  // SQ - Square Mover
            (22, 7, Some(PieceType::OldRat)),  // OR - Old Rat
            (23, 7, Some(PieceType::BlindBear)),  // BB - Blind Bear
            (24, 7, Some(PieceType::SideFlyer)),  // SF - Side Flyer
            (25, 7, Some(PieceType::OxChariot)),  // OC - Ox Chariot
            (26, 7, Some(PieceType::BlindTiger)),  // BT - Blind Tiger
            (27, 7, Some(PieceType::BlindMonkey)),  // BM - Blind Monkey
            (28, 7, Some(PieceType::SwallowsWings)),  // SW - Swallow's Wings
            (29, 7, Some(PieceType::CatSword)),  // CS - Cat Sword
            (30, 7, Some(PieceType::ClimbingMonkey)),  // CM - Climbing Monkey
            (31, 7, Some(PieceType::OwlMover)),  // OW - Owl Mover
            (32, 7, Some(PieceType::Horseman)),  // HO - Horseman
            (33, 7, Some(PieceType::Tanuki)),  // EB - Tanuki
            (34, 7, Some(PieceType::VermillionSparrow)),  // VI - Vermillion Sparrow
            (35, 7, Some(PieceType::EarthChariot)),  // EC - Earth Chariot
        ];
        self.place_pieces_mirrored(eighth_rank);
        
        // Place 9th rank pieces in order (files 0-35 for Black, automatically mirrored for White)
        // Order: CH, SL, VR, WN, RE, M, SD, HS, GN, OS, EA, BS, SG, LP, T, BE, I, GM, GE, I, BE, T, LP, SG, BS, EA, OS, GN, HS, SD, M, RE, WN, VR, SL, CH
        let ninth_rank: Vec<(u8, u8, Option<PieceType>)> = vec![
            (0, 8, Some(PieceType::ChariotSoldier)),   // CH - chariot soldier
            (1, 8, Some(PieceType::SideSoldier)),  // SL - Side Soldier
            (2, 8, Some(PieceType::VerticalSoldier)),  // VR - Vertical Soldier
            (3, 8, Some(PieceType::WindGeneral)),  // WN - Wind General
            (4, 8, Some(PieceType::RiverGeneral)),  // RE - River General
            (5, 8, Some(PieceType::MountainGeneral)),  // M - Mountain General
            (6, 8, Some(PieceType::FrontStandard)),  // SD - Front Standard
            (7, 8, Some(PieceType::HorseSoldier)),  // HS - Horse Soldier
            (8, 8, Some(PieceType::WoodGeneral)),  // GN - Wood General
            (9, 8, Some(PieceType::OxSoldier)),  // OS - Ox Soldier
            (10, 8, Some(PieceType::EarthGeneral)),  // EA - Earth General
            (11, 8, Some(PieceType::BoarSoldier)),  // BS - Boar Soldier
            (12, 8, Some(PieceType::StoneGeneral)),  // SG - Stone General
            (13, 8, Some(PieceType::LeopardSoldier)),  // LP - Leopard Soldier
            (14, 8, Some(PieceType::TileGeneral)),  // T - Tile General
            (15, 8, Some(PieceType::BearSoldier)),  // BE - Bear Soldier
            (16, 8, Some(PieceType::IronGeneral)),  // I - Iron General
            (17, 8, Some(PieceType::GreatMaster)),  // GM - Great Master
            (18, 8, Some(PieceType::GreatStandard)),  // GE - Great Standard
            (19, 8, Some(PieceType::IronGeneral)),  // I - Iron General
            (20, 8, Some(PieceType::BearSoldier)),  // BE - Bear Soldier
            (21, 8, Some(PieceType::TileGeneral)),  // T - Tile General
            (22, 8, Some(PieceType::LeopardSoldier)),  // LP - Leopard Soldier
            (23, 8, Some(PieceType::StoneGeneral)),  // SG - Stone General
            (24, 8, Some(PieceType::SideGeneral)),  // BS - Side General
            (25, 8, Some(PieceType::EarthGeneral)),  // EA - Earth General
            (26, 8, Some(PieceType::OxSoldier)),  // OS - Ox Soldier
            (27, 8, Some(PieceType::WoodGeneral)),  // GN - Wood General
            (28, 8, Some(PieceType::HorseSoldier)),  // HS - Horse Soldier
            (29, 8, Some(PieceType::FrontStandard)),  // SD - Front Standard
            (30, 8, Some(PieceType::MountainGeneral)),  // M - Mountain General
            (31, 8, Some(PieceType::RiverGeneral)),  // RE - River General
            (32, 8, Some(PieceType::WindGeneral)),  // WN - Wind General
            (33, 8, Some(PieceType::VerticalSoldier)),  // VR - Vertical Soldier
            (34, 8, Some(PieceType::SideSoldier)),  // SL - Side Soldier
            (35, 8, Some(PieceType::ChariotSoldier)),  // CH - Chariot Soldier
        ];
        self.place_pieces_mirrored(ninth_rank);

        // Tenth rank (rank 9, 0-indexed) - Final rank with pieces
        let tenth_rank = vec![
            (0, 9, Some(PieceType::LeftChariot)),  // LC - Left Chariot
            (1, 9, Some(PieceType::SideMonkey)),  // MK - Side Monkey
            (2, 9, Some(PieceType::VerticalMover)),   // VM - Vertical Mover
            (3, 9, Some(PieceType::FlyingOx)),   // OX - Flying Ox
            (4, 9, Some(PieceType::LongbowSoldier)),  // LB - Longbow Soldier
            (5, 9, Some(PieceType::VerticalPup)),  // VP - Vertical Pup
            (6, 9, Some(PieceType::VerticalHorse)),  // VH - Vertical Horse
            (7, 9, Some(PieceType::CannonSoldier)),  // BN - Cannon Soldier
            (8, 9, Some(PieceType::DragonHorse)),  // DH - Dragon Horse
            (9, 9, Some(PieceType::DragonKing)),  // DK - Dragon King
            (10, 9, Some(PieceType::SwordSoldier)),  // SE - Sword Soldier
            (11, 9, Some(PieceType::HornedHawk)),  // HF - Horned Hawk
            (12, 9, Some(PieceType::FlyingEagle)),  // EL - Flying Eagle
            (13, 9, Some(PieceType::SpearSoldier)),  // SP - Spear Soldier
            (14, 9, Some(PieceType::VerticalLeopard)),  // VL - Vertical Leopard
            (15, 9, Some(PieceType::FierceTiger)),  // TG - Fierce Tiger
            (16, 9, Some(PieceType::CrossbowSoldier)),  // SC - Crossbow Soldier
            (17, 9, Some(PieceType::LionDog)),  // LD - Lion Dog
            (18, 9, Some(PieceType::RoaringDog)),  // DG - Roaring Dog
            (19, 9, Some(PieceType::CrossbowSoldier)),  // SC - Crossbow Soldier
            (20, 9, Some(PieceType::FierceTiger)),  // TG - Fierce Tiger
            (21, 9, Some(PieceType::VerticalLeopard)),  // VL - Vertical Leopard
            (22, 9, Some(PieceType::SpearSoldier)),  // SP - Spear Soldier
            (23, 9, Some(PieceType::FlyingEagle)),  // EL - Flying Eagle
            (24, 9, Some(PieceType::HornedHawk)),  // HF - Horned Hawk
            (25, 9, Some(PieceType::SwordSoldier)),  // SE - Sword Soldier
            (26, 9, Some(PieceType::DragonKing)),  // DK - Dragon King
            (27, 9, Some(PieceType::DragonHorse)),  // DH - Dragon Horse
            (28, 9, Some(PieceType::CannonSoldier)),  // BN - Cannon Soldier
            (29, 9, Some(PieceType::VerticalHorse)),  // VH - Vertical Horse
            (30, 9, Some(PieceType::VerticalPup)),  // VP - Vertical Pup
            (31, 9, Some(PieceType::LongbowSoldier)),  // LB - Longbow Soldier
            (32, 9, Some(PieceType::FlyingOx)),  // OX - Flying Ox
            (33, 9, Some(PieceType::VerticalMover)),  // VM - Vertical Mover
            (34, 9, Some(PieceType::SideMonkey)),  // MK - Side Monkey
            (35, 9, Some(PieceType::RightChariot)),  // RC - Right Chariot
        ];
        self.place_pieces_mirrored(tenth_rank);
        
        // Place pawns (mirrored automatically)
        // Black pawns: rank 10, all files (0-35)
        for file in 0..36 {
            self.place_piece_mirrored(PieceType::Pawn, file, 10);
        }
        
        // Place dogs (mirrored automatically)
        // Black dogs: rank 11, files 5, 14, 21, 30
        let dog_files = [5, 14, 21, 30];
        for file in dog_files.iter() {
            self.place_piece_mirrored(PieceType::Dog, *file, 11);
        }
        
        // Place go-betweens (mirrored automatically)
        // Black go-betweens: rank 11, files 10 and 25
        let go_between_files = [10, 25];
        for file in go_between_files.iter() {
            self.place_piece_mirrored(PieceType::GoBetween, *file, 11);
        }
    }

    /// Generate all legal moves for the current player
    pub fn generate_legal_moves(&self) -> Vec<Move> {
        // Use iterator to avoid cloning pieces
        let pieces: Vec<Piece> = self.board.iter_pieces_by_color(self.current_turn).collect();
        self.generate_legal_moves_for_pieces(&pieces)
    }

    /// Generate legal moves only for the specified pieces
    /// This allows filtering pieces before generating moves for performance optimization
    pub fn generate_legal_moves_for_pieces(&self, pieces: &[Piece]) -> Vec<Move> {
        let mut moves = Vec::new();

        for piece in pieces {
            // Special handling for Free Eagle
            if piece.piece_type == PieceType::FreeEagle {
                moves.extend(self.generate_free_eagle_moves(piece));
                continue;
            }
            
            let config = MovementConfig::for_piece(piece);
            
            // Check if this piece has two-step movement capabilities
            let has_two_step = config.capabilities.iter().any(|cap| {
                matches!(cap, crate::movement::types::MovementCapability::TwoStep { .. })
            });
            
            if has_two_step {
                // Generate moves for two-step capabilities separately to track intermediates
                for capability in &config.capabilities {
                    if let crate::movement::types::MovementCapability::TwoStep { first, second } = capability {
                        // Generate first step targets
                        let first_cap = vec![first.as_ref().clone()];
                        let first_targets = crate::movement::MovementGenerator::generate_targets(piece, &self.board, &first_cap);
                        
                        // Generate single-step moves using just the first step
                        for target in &first_targets {
                            if self.is_legal_move_assuming_reachable(piece, piece.position, *target, false) {
                                let can_promote = self.can_promote(piece, piece.position, *target);
                                
                                if !can_promote {
                                    moves.push(Move::new_with_promotion(piece.position, *target, false));
                                } else {
                                    let must_promote = piece.piece_type.must_promote_on_rank(target.rank, piece.color);
                                    if must_promote {
                                        moves.push(Move::new_with_promotion(piece.position, *target, true));
                                    } else {
                                        moves.push(Move::new_with_promotion(piece.position, *target, true));
                                        moves.push(Move::new_with_promotion(piece.position, *target, false));
                                    }
                                }
                            }
                        }
                        
                        // For each first step target, generate second moves from that position
                        for intermediate in first_targets {
                            // Use optimized check for first step: assumes intermediate is reachable
                            if !self.is_legal_move_assuming_reachable(piece, piece.position, intermediate, false) {
                                continue;
                            }
                            
                            // Create a temporary piece at the intermediate position
                            let mut temp_piece = *piece;
                            temp_piece.position = intermediate;
                            
                            // Generate second step targets from intermediate position
                            let second_cap = vec![second.as_ref().clone()];
                            let second_targets = crate::movement::MovementGenerator::generate_targets(&temp_piece, &self.board, &second_cap);
                            
                            for target in second_targets {
                                // Use optimized check for second step: assumes target is reachable (we generated it)
                                if !self.is_legal_move_assuming_reachable(&temp_piece, intermediate, target, false) {
                                    continue;
                                }
                                
                                // Check if this move can promote
                                // For two-step moves, can promote if start, intermediate, OR end is in promotion zone
                                let can_promote = self.can_promote(piece, piece.position, target) 
                                    || self.can_promote(piece, piece.position, intermediate);
                                
                                if !can_promote {
                                    moves.push(Move::new_two_step(piece.position, intermediate, target));
                                } else {
                                    // For two-step moves, check if promotion is mandatory based on final destination
                                    let must_promote = piece.piece_type.must_promote_on_rank(target.rank, piece.color);
                                    if must_promote {
                                        moves.push(Move::new_two_step_with_promotion(piece.position, intermediate, target, true));
                                    } else {
                                        moves.push(Move::new_two_step_with_promotion(piece.position, intermediate, target, true));
                                        moves.push(Move::new_two_step_with_promotion(piece.position, intermediate, target, false));
                                    }
                                }
                            }
                        }
                    }
                }
                
                // Generate moves for non-two-step capabilities
                for capability in &config.capabilities {
                    if !matches!(capability, crate::movement::types::MovementCapability::TwoStep { .. }) {
                        let cap_vec = vec![capability.clone()];
                        let potential_targets = crate::movement::MovementGenerator::generate_targets(piece, &self.board, &cap_vec);
                        
                        for target in potential_targets {
                            // Use optimized check: assumes target is reachable (we generated it)
                            // Skip check detection for now (can be added later if needed)
                            if self.is_legal_move_assuming_reachable(piece, piece.position, target, false) {
                                let can_promote = self.can_promote(piece, piece.position, target);
                                
                                if !can_promote {
                                    moves.push(Move::new_with_promotion(piece.position, target, false));
                                } else {
                                    let must_promote = piece.piece_type.must_promote_on_rank(target.rank, piece.color);
                                    if must_promote {
                                        moves.push(Move::new_with_promotion(piece.position, target, true));
                                    } else {
                                        moves.push(Move::new_with_promotion(piece.position, target, true));
                                        moves.push(Move::new_with_promotion(piece.position, target, false));
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                // No two-step capabilities - use the simple approach
                let potential_targets = piece.get_potential_targets(&self.board);
                
                for target in potential_targets {
                    // Use optimized check: assumes target is reachable (we generated it)
                    // Skip check detection for now (can be added later if needed)
                    if self.is_legal_move_assuming_reachable(piece, piece.position, target, false) {
                        let can_promote = self.can_promote(piece, piece.position, target);
                        
                        if !can_promote {
                            moves.push(Move::new_with_promotion(piece.position, target, false));
                        } else {
                            let must_promote = piece.piece_type.must_promote_on_rank(target.rank, piece.color);
                            if must_promote {
                                moves.push(Move::new_with_promotion(piece.position, target, true));
                            } else {
                                moves.push(Move::new_with_promotion(piece.position, target, true));
                                moves.push(Move::new_with_promotion(piece.position, target, false));
                            }
                        }
                    }
                }
            }
        }

        moves
    }

    /// Check if a move is legal
    fn is_legal_move(&self, from: Position, to: Position) -> bool {
        // Get the piece making the move
        let Some(piece) = self.board.get_piece(from) else {
            return false;
        };

        // Must be the current player's piece
        if piece.color != self.current_turn {
            return false;
        }

        // Check if piece can reach the target (basic movement pattern)
        if !piece.can_reach(to, &self.board) {
            return false;
        }

        // Cannot capture own pieces (skip this check for return-to-start moves)
        if from != to {
            if let Some(target_piece) = self.board.get_piece(to) {
                if target_piece.color == piece.color {
                    return false;
                }
            }
        }

        // Movement-specific rules (pawn forward-only, dog capture rules, etc.) are handled
        // by the movement generator, so we don't need explicit checks here

        true
    }

    /// Optimized legality check that assumes:
    /// - The piece at `from` exists and belongs to the current player
    /// - The piece can reach `to` (target was generated from movement generator)
    /// Only checks: not capturing own piece, and optionally if move leaves king in check
    /// Uses VirtualBoard for efficient check detection
    fn is_legal_move_assuming_reachable(&self, piece: &Piece, from: Position, to: Position, check_for_check: bool) -> bool {
        // Cannot capture own pieces (skip this check for return-to-start moves)
        if from != to {
            if let Some(target_piece) = self.board.get_piece(to) {
                if target_piece.color == piece.color {
                    return false;
                }
            }
        }

        // Optionally check if move leaves own king in check
        if check_for_check {
            use crate::move_simulation::{simulate_move, BoardLike};
            use crate::game_state::Move;
            
            // Create a move for simulation
            let mv = Move::new_with_promotion(from, to, false);
            let virtual_board = simulate_move(&self.board, &mv, piece);
            
            // Check if any royal piece is under attack after the move
            let royal_pieces = self.board.get_pieces_by_color(piece.color);
            for royal_piece in royal_pieces {
                if royal_piece.piece_type.is_royal() {
                    // Check if this royal piece would be under attack after the move
                    // If the royal piece moved, check its new position; otherwise check its current position
                    let check_pos = if royal_piece.position == from {
                        to  // Royal piece moved
                    } else {
                        royal_piece.position  // Royal piece didn't move
                    };
                    
                    if virtual_board.is_position_attacked_by_color_for_check(check_pos, piece.color.opposite()) {
                        return false;  // Move leaves king in check
                    }
                }
            }
        }

        true
    }


    /// Check if a piece can promote (starts OR ends in promotion zone)
    fn can_promote(&self, piece: &Piece, from: Position, to: Position) -> bool {
        // Already promoted pieces cannot promote again
        if piece.is_promoted {
            return false;
        }
        
        // Only pieces that can promote (pawns, dogs, go-betweens, crown prince, etc.)
        if piece.piece_type.promotes_to().is_none() {
            return false;
        }
        
        // Promotion zone is opponent's 11th rank
        // For Black: ranks 25-35 (opponent's 11th rank)
        // For White: ranks 0-10 (opponent's 11th rank)
        match piece.color {
            Color::Black => {
                // Can promote if starts OR ends in promotion zone (rank >= 25)
                from.rank >= 25 || to.rank >= 25
            }
            Color::White => {
                // Can promote if starts OR ends in promotion zone (rank <= 10)
                from.rank <= 10 || to.rank <= 10
            }
        }
    }
    

    /// Make a move (assumes move is legal - caller should validate)
    /// Executes a move and returns the intermediate position if this was a two-step move, None otherwise
    pub fn make_move(&mut self, mv: Move) -> Option<Position> {
        match self.apply_move(mv, true, true, None) {
            ApplyOutcome::Failed => None,
            ApplyOutcome::Ok { intermediate, .. } => intermediate,
        }
    }

    /// Apply a move produced by `generate_legal_moves` during search.
    ///
    /// Skips reachability re-validation and does not append to `move_history`
    /// (eval noise should use an explicit ply). Board updates, captures,
    /// promotions, and turn flip match `make_move`.
    ///
    /// Returns a [`SearchUndo`] token for [`unmake_move_for_search`].
    pub fn make_move_for_search(&mut self, mv: Move) -> Option<SearchUndo> {
        let prev_turn = self.current_turn;
        let prev_draw = self.turns_without_capture_or_promotion;
        let original_mover = self.board.get_piece(mv.from)?;
        let from = mv.from;
        let mut removed = Vec::new();
        let final_to = match self.apply_move(mv, false, false, Some(&mut removed)) {
            ApplyOutcome::Failed => return None,
            ApplyOutcome::Ok { final_to, .. } => final_to,
        };
        Some(SearchUndo {
            from,
            final_to,
            original_mover,
            removed,
            prev_turn,
            prev_draw,
        })
    }

    /// Reverse a move applied by [`make_move_for_search`].
    pub fn unmake_move_for_search(&mut self, undo: SearchUndo) {
        self.board.remove_piece(undo.final_to);
        self.board.place_piece(undo.original_mover);
        for (_pos, piece) in undo.removed {
            self.board.place_piece(piece);
        }
        self.current_turn = undo.prev_turn;
        self.turns_without_capture_or_promotion = undo.prev_draw;
    }

    fn apply_move(
        &mut self,
        mut mv: Move,
        validate: bool,
        record_history: bool,
        mut removed_out: Option<&mut Vec<(Position, Piece)>>,
    ) -> ApplyOutcome {
        let Some(piece) = self.board.get_piece(mv.from) else {
            return ApplyOutcome::Failed;
        };

        if piece.piece_type == PieceType::FreeEagle && !mv.is_free_eagle() {
            let free_eagle_moves = self.generate_free_eagle_moves(&piece);
            if let Some(matching_move) = free_eagle_moves.iter().find(|m| m.to == mv.to) {
                if let Some(path) = matching_move.free_eagle_path() {
                    mv.data = MoveData::FreeEagle { path: path.clone() };
                }
            } else {
                return ApplyOutcome::Failed;
            }
        } else if validate && !self.is_legal_move(mv.from, mv.to) {
            return ApplyOutcome::Failed;
        }

        if let Some(intermediate) = mv.intermediate() {
            if validate {
                if !self.is_legal_move(mv.from, intermediate) {
                    return ApplyOutcome::Failed;
                }
                let mut temp_piece = piece;
                temp_piece.position = intermediate;
                if !temp_piece.can_reach(mv.to, &self.board) {
                    return ApplyOutcome::Failed;
                }
                if let Some(target_piece) = self.board.get_piece(mv.to) {
                    if target_piece.color == piece.color {
                        return ApplyOutcome::Failed;
                    }
                }
            }

            let first_move = Move::new(mv.from, intermediate);
            let (first_success, first_capture, _) =
                self.execute_single_move(first_move, record_history, removed_out.as_deref_mut());
            if !first_success {
                return ApplyOutcome::Failed;
            }

            let second_move = Move::new_with_promotion(intermediate, mv.to, mv.promoted);
            let (second_success, second_capture, second_promotion) =
                self.execute_single_move(second_move, record_history, removed_out.as_deref_mut());
            if !second_success {
                self.board.move_piece(intermediate, mv.from);
                return ApplyOutcome::Failed;
            }

            if first_capture || second_capture || second_promotion {
                self.turns_without_capture_or_promotion = 0;
            } else {
                self.turns_without_capture_or_promotion += 1;
            }

            self.current_turn = self.current_turn.opposite();
            return ApplyOutcome::Ok {
                intermediate: Some(intermediate),
                final_to: mv.to,
            };
        }

        if let Some(path) = mv.free_eagle_path() {
            let mut had_capture = false;
            let mut had_promotion = false;

            for i in 1..path.len() {
                let from_pos = path[i - 1];
                let to_pos = path[i];

                let Some(current_piece) = self.board.get_piece(from_pos) else {
                    return ApplyOutcome::Failed;
                };

                if let Some(target_piece) = self.board.get_piece(to_pos) {
                    if target_piece.color == current_piece.color {
                        return ApplyOutcome::Failed;
                    }
                    had_capture = true;
                    if let Some(out) = removed_out.as_deref_mut() {
                        out.push((to_pos, target_piece));
                    }
                    self.board.remove_piece(to_pos);
                }

                self.board.move_piece(from_pos, to_pos);

                if i == path.len() - 1 && mv.promoted {
                    if let Some(mut p) = self.board.get_piece(to_pos) {
                        if p.piece_type.promotes_to().is_some() {
                            p.promote();
                            self.board.remove_piece(to_pos);
                            self.board.place_piece(p);
                            had_promotion = true;
                        }
                    }
                }
            }

            if record_history {
                for i in 1..path.len() {
                    let pos = path[i];
                    if i < path.len() - 1 || path[i] != mv.to {
                        self.move_history.push(Move::new(path[i - 1], pos));
                    }
                }
                if let Some(last_pos) = path.last() {
                    if *last_pos != mv.to {
                        self.move_history.push(Move::new(*last_pos, mv.to));
                    }
                }
            }

            if had_capture || had_promotion {
                self.turns_without_capture_or_promotion = 0;
            } else {
                self.turns_without_capture_or_promotion += 1;
            }

            self.current_turn = self.current_turn.opposite();
            let final_to = path.last().copied().unwrap_or(mv.to);
            return ApplyOutcome::Ok {
                intermediate: None,
                final_to,
            };
        }

        let (execute_result, had_capture, had_promotion) =
            self.execute_single_move(mv.clone(), record_history, removed_out.as_deref_mut());

        if !execute_result {
            return ApplyOutcome::Failed;
        }

        if had_capture || had_promotion {
            self.turns_without_capture_or_promotion = 0;
        } else {
            self.turns_without_capture_or_promotion += 1;
        }
        self.current_turn = self.current_turn.opposite();
        ApplyOutcome::Ok {
            intermediate: None,
            final_to: mv.to,
        }
    }
    
    /// Execute a single move (helper for make_move)
    /// Does not change the turn
    /// Returns (success, had_capture, had_promotion)
    fn execute_single_move(
        &mut self,
        mv: Move,
        record_history: bool,
        mut removed_out: Option<&mut Vec<(Position, Piece)>>,
    ) -> (bool, bool, bool) {
        // Get the piece making the move
        let Some(piece) = self.board.get_piece(mv.from) else {
            return (false, false, false);
        };

        // Check if there's a capture at the destination
        let had_capture_dest = self.board.get_piece(mv.to).is_some();
        
        // Defensive check: cannot capture friendly piece
        if had_capture_dest {
            if let Some(target_piece) = self.board.get_piece(mv.to) {
                if target_piece.color == piece.color {
                    return (false, false, false); // Cannot capture friendly piece
                }
                if let Some(out) = removed_out.as_deref_mut() {
                    out.push((mv.to, target_piece));
                }
            }
        }

        // Check if this piece uses capturing range movement
        let config = MovementConfig::for_piece(&piece);
        let uses_capturing = config.capabilities.iter().any(|cap| {
            if let crate::movement::MovementCapability::Range { blocking, .. } = cap {
                *blocking == crate::movement::BlockingMode::Capturing
            } else {
                false
            }
        });

        // Track if any captures occurred in the path
        let mut had_capture_path = false;

        // If using capturing range movement, capture all pieces in the path
        if uses_capturing {
            let path_positions = path_utils::get_path_positions(mv.from, mv.to);
            // Remove all pieces in the path (excluding start and destination)
            // Start is excluded because that's where the moving piece is
            // Destination is excluded because move_piece will handle it
            for pos in path_positions {
                if pos != mv.from && pos != mv.to {
                    if let Some(removed) = self.board.remove_piece(pos) {
                        had_capture_path = true;
                        if let Some(out) = removed_out.as_deref_mut() {
                            out.push((pos, removed));
                        }
                    }
                }
            }
        }

        // Move the piece (destination capture already recorded above)
        self.board.move_piece(mv.from, mv.to);

        // Verify the piece was actually moved
        if self.board.get_piece(mv.from).is_some() {
            // Piece is still at from - move failed
            return (false, false, false);
        }
        if self.board.get_piece(mv.to).is_none() {
            // Piece is not at destination - move failed
            return (false, false, false);
        }

        // Handle promotion if the move indicates promotion
        let had_promotion = mv.promoted;
        if mv.promoted {
            if let Some(mut piece) = self.board.get_piece(mv.to) {
                piece.promote();
                // Update the piece on the board
                self.board.remove_piece(mv.to);
                self.board.place_piece(piece);
            }
        }

        if record_history {
            self.move_history.push(mv);
        }
        let had_capture = had_capture_dest || had_capture_path;
        (true, had_capture, had_promotion)
    }
    


    /// Get move history
    pub fn get_move_history(&self) -> &Vec<Move> {
        &self.move_history
    }

    /// Get a copy of the board state (for analysis)
    pub fn clone_board(&self) -> Board {
        self.board.clone()
    }
    
    /// Create a temporary GameState with the opponent's turn set
    /// Used for attack detection - same board state but opponent's perspective
    pub fn with_opponent_turn(&self) -> GameState {
        GameState {
            board: self.board.clone(),
            current_turn: self.current_turn.opposite(),
            move_history: Vec::new(), // Don't copy history for attack checking
            turns_without_capture_or_promotion: self.turns_without_capture_or_promotion, // Preserve counter
        }
    }

    /// Check if a player has lost (all their royal pieces are captured)
    /// A player loses when ALL their royal pieces (King and Crown Prince) are captured
    pub fn has_lost(&self, color: Color) -> bool {
        !self
            .board
            .iter_pieces_by_color(color)
            .any(|p| p.piece_type.is_royal())
    }
    
    /// Check if a piece type is King, CrownPrince, GreatGeneral, or can promote into one of these
    /// Pieces that count: King, CrownPrince, GreatGeneral, DrunkenElephant (promotes to CrownPrince), FreeKing (promotes to GreatGeneral)
    fn is_king_crownprince_or_greatgeneral(piece_type: PieceType) -> bool {
        matches!(piece_type,
            PieceType::King |
            PieceType::CrownPrince |
            PieceType::GreatGeneral |
            PieceType::DrunkenElephant |  // Promotes to CrownPrince
            PieceType::FreeKing            // Promotes to GreatGeneral
        )
    }
    
    /// Check if the game should be adjudicated as a draw by 500-move rule
    /// Draw occurs when no capture or promotion has happened for 500 consecutive turns
    pub fn is_draw_by_500_move_rule(&self) -> bool {
        self.turns_without_capture_or_promotion >= 500
    }
    
    /// Check if the game should be adjudicated as a draw
    /// Draw occurs when only Kings, Crown Princes, and Great Generals (including pieces that promote into these) remain
    /// Note: This only applies if there are pieces on the board (empty board is a win, not a draw)
    pub fn is_draw_by_insufficient_material(&self) -> bool {
        let black_pieces = self.board.get_pieces_by_color(Color::Black);
        let white_pieces = self.board.get_pieces_by_color(Color::White);
        
        // If either side has no pieces, it's not a draw (game would have ended already)
        if black_pieces.is_empty() || white_pieces.is_empty() {
            return false;
        }
        
        // Check if all pieces are King, CrownPrince, GreatGeneral, or can promote into one of these
        black_pieces.iter().all(|piece| Self::is_king_crownprince_or_greatgeneral(piece.piece_type)) &&
        white_pieces.iter().all(|piece| Self::is_king_crownprince_or_greatgeneral(piece.piece_type))
    }
    
    /// Check if the game is over (one player has lost, or draw by insufficient material)
    /// Returns Some(Color) if that color has won, None if game continues
    pub fn get_winner(&self) -> Option<Color> {
        if self.has_lost(Color::Black) {
            Some(Color::White)
        } else if self.has_lost(Color::White) {
            Some(Color::Black)
        } else {
            None
        }
    }
    
    /// Generate Free Eagle moves with full paths
    pub fn generate_free_eagle_moves(&self, piece: &Piece) -> Vec<Move> {
        use crate::movement::direction::Direction;
        
        let mut moves = Vec::new();
        
        // Forward diagonals for this color
        let forward_diagonals = match piece.color {
            Color::Black => vec![Direction::NE, Direction::NW],
            Color::White => vec![Direction::SE, Direction::SW],
        };
        
        // Other directions (orthogonal and backward diagonals)
        let other_directions = match piece.color {
            Color::Black => vec![Direction::N, Direction::S, Direction::E, Direction::W, Direction::SE, Direction::SW],
            Color::White => vec![Direction::N, Direction::S, Direction::E, Direction::W, Direction::NE, Direction::NW],
        };
        
        // Pattern 1: Forward diagonal multi-move (up to 4 spaces)
        for direction in &forward_diagonals {
            let (file_delta, rank_delta) = direction.to_offset();
            let mut path = vec![piece.position];
            let mut current = piece.position;
            
            for distance in 1..=4 {
                let Some(next) = current.offset(file_delta, rank_delta) else {
                    break;
                };
                
                if let Some(p) = self.board.get_piece(next) {
                    if p.color == piece.color {
                        break; // Blocked by friendly
                    }
                    // Enemy piece - capture and continue
                    path.push(next);
                    current = next;
                    // Generate move to this position
                    if self.is_legal_move(piece.position, next) {
                        moves.push(Move::new_free_eagle(piece.position, next, path.clone()));
                    }
                } else {
                    // Empty square
                    path.push(next);
                    current = next;
                    // Generate move to this position
                    if self.is_legal_move(piece.position, next) {
                        moves.push(Move::new_free_eagle(piece.position, next, path.clone()));
                    }
                }
            }
        }
        
        // Pattern 2: Other directions multi-move (up to 3 spaces)
        for direction in &other_directions {
            let (file_delta, rank_delta) = direction.to_offset();
            let mut path = vec![piece.position];
            let mut current = piece.position;
            
            for distance in 1..=3 {
                let Some(next) = current.offset(file_delta, rank_delta) else {
                    break;
                };
                
                if let Some(p) = self.board.get_piece(next) {
                    if p.color == piece.color {
                        break; // Blocked by friendly
                    }
                    // Enemy piece - capture and continue
                    path.push(next);
                    current = next;
                    // Generate move to this position
                    if self.is_legal_move(piece.position, next) {
                        moves.push(Move::new_free_eagle(piece.position, next, path.clone()));
                    }
                } else {
                    // Empty square
                    path.push(next);
                    current = next;
                    // Generate move to this position
                    if self.is_legal_move(piece.position, next) {
                        moves.push(Move::new_free_eagle(piece.position, next, path.clone()));
                    }
                }
            }
        }
        
        // Pattern 3: Forward diagonal special (3 forward + 1 back) - only if capture on 3rd space
        for direction in &forward_diagonals {
            let (file_delta, rank_delta) = direction.to_offset();
            let back_delta = (-file_delta, -rank_delta);
            
            // Build forward path
            let mut forward_path = vec![piece.position];
            let mut pos3 = piece.position;
            let mut valid = true;
            
            for _ in 0..3 {
                let Some(next) = pos3.offset(file_delta, rank_delta) else {
                    valid = false;
                    break;
                };
                if let Some(p) = self.board.get_piece(next) {
                    if p.color == piece.color {
                        valid = false;
                        break;
                    }
                }
                forward_path.push(next);
                pos3 = next;
            }
            
            if !valid {
                continue;
            }
            
            // Check if there's a capture on the 3rd space
            if let Some(p) = self.board.get_piece(pos3) {
                if p.color != piece.color {
                    // Pattern is valid - move 1 space back
                    if let Some(final_pos) = pos3.offset(back_delta.0, back_delta.1) {
                        if let Some(p) = self.board.get_piece(final_pos) {
                            if p.color == piece.color {
                                continue; // Cannot land on friendly
                            }
                        }
                        
                        let mut path = forward_path.clone();
                        path.push(final_pos);
                        
                        if self.is_legal_move(piece.position, final_pos) {
                            moves.push(Move::new_free_eagle(piece.position, final_pos, path));
                        }
                    }
                }
            }
        }
        
        // Pattern 4: Any direction special (2 forward + 1 back) - only if capture on 2nd space
        let all_directions = Direction::all();
        for direction in all_directions {
            let (file_delta, rank_delta) = direction.to_offset();
            let back_delta = (-file_delta, -rank_delta);
            
            // Build forward path
            let mut forward_path = vec![piece.position];
            let mut pos2 = piece.position;
            let mut valid = true;
            
            for _ in 0..2 {
                let Some(next) = pos2.offset(file_delta, rank_delta) else {
                    valid = false;
                    break;
                };
                if let Some(p) = self.board.get_piece(next) {
                    if p.color == piece.color {
                        valid = false;
                        break;
                    }
                }
                forward_path.push(next);
                pos2 = next;
            }
            
            if !valid {
                continue;
            }
            
            // Check if there's a capture on the 2nd space
            if let Some(p) = self.board.get_piece(pos2) {
                if p.color != piece.color {
                    // Pattern is valid - move 1 space back
                    if let Some(final_pos) = pos2.offset(back_delta.0, back_delta.1) {
                        if let Some(p) = self.board.get_piece(final_pos) {
                            if p.color == piece.color {
                                continue; // Cannot land on friendly
                            }
                        }
                        
                        let mut path = forward_path.clone();
                        path.push(final_pos);
                        
                        if self.is_legal_move(piece.position, final_pos) {
                            moves.push(Move::new_free_eagle(piece.position, final_pos, path));
                        }
                    }
                }
            }
        }
        
        // Pattern 5: Stay in place while capturing enemy 1 space away in any direction
        if let Some(capture_pos) = crate::movement::MovementGenerator::check_pattern5(piece, &self.board) {
            // There's an enemy piece 1 space away - can capture while staying in place
            // Path: [start, capture_pos, start]
            let path = vec![piece.position, capture_pos, piece.position];
            if self.is_legal_move(piece.position, piece.position) {
                moves.push(Move::new_free_eagle(piece.position, piece.position, path));
            }
        }
        
        // Pattern 6: Stay in place while capturing one or two enemies on first or second space along forward diagonals
        // Only generate if there's a capture on the 2nd space (otherwise Pattern 5 covers it)
        if let Some((pos1, pos2)) = crate::movement::MovementGenerator::check_pattern6(piece, &self.board) {
            let mut path = vec![piece.position];
            
            // Include pos1 in path (even if empty or has enemy)
            if let Some(pos1) = pos1 {
                path.push(pos1);
            }
            
            // Include pos2 (which has the capture)
            path.push(pos2);
            
            // Return to start
            path.push(piece.position);
            
            if self.is_legal_move(piece.position, piece.position) {
                moves.push(Move::new_free_eagle(piece.position, piece.position, path));
            }
        }
        
        // Also generate standard range moves (Pattern 0)
        let config = MovementConfig::for_piece(piece);
        for capability in &config.capabilities {
            if let crate::movement::types::MovementCapability::Range { .. } = capability {
                let cap_vec = vec![capability.clone()];
                let potential_targets = crate::movement::MovementGenerator::generate_targets(piece, &self.board, &cap_vec);
                
                for target in potential_targets {
                    if self.is_legal_move(piece.position, target) {
                        // Standard range move - no path needed
                        moves.push(Move::new(piece.position, target));
                    }
                }
            }
        }
        
        moves
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::piece::PieceType;

    #[test]
    fn test_initial_state() {
        let state = GameState::new();
        assert_eq!(state.get_current_turn(), Color::Black);
        assert_eq!(state.generate_legal_moves().len(), 0); // Empty board
    }

    #[test]
    fn test_king_move_generation() {
        let mut state = GameState::new();
        let king = Piece::new(PieceType::King, Color::Black, Position::new(10, 10).unwrap());
        state.place_piece(king);
        
        let moves = state.generate_legal_moves();
        // King can move up to 2 squares in 8 directions
        // In center of board: 24 squares (8 directions * 2 steps + 8 intermediate + 8 immediate)
        // Actually, it's all squares within 2 steps: 5x5 - 1 = 24 squares
        assert!(moves.len() >= 8); // At least 8 immediate adjacent squares
        assert!(moves.len() <= 24); // At most 24 squares (5x5 grid minus center)
        // All moves should not be promoted (kings don't promote)
        for mv in moves {
            assert!(!mv.promoted);
        }
    }

    #[test]
    fn test_pawn_move_generation() {
        let mut state = GameState::new();
        let pawn = Piece::new(PieceType::Pawn, Color::Black, Position::new(10, 10).unwrap());
        state.place_piece(pawn);
        
        let moves = state.generate_legal_moves();
        // Pawn can move forward (1) or diagonally (2), but only if legal
        assert!(moves.len() >= 1); // At least forward move
    }

    #[test]
    fn test_pawn_cannot_move_diagonally_to_empty() {
        let mut state = GameState::new();
        let pawn = Piece::new(PieceType::Pawn, Color::Black, Position::new(10, 10).unwrap());
        state.place_piece(pawn);
        
        let moves = state.generate_legal_moves();
        // Pawns can only move forward (same file)
        for mv in moves {
            assert_eq!(mv.from.file, mv.to.file, "Pawn must move forward (same file)");
            // Pawn should move forward (increasing rank for Black)
            assert!(mv.to.rank > mv.from.rank, "Pawn must move forward");
        }
    }

    #[test]
    fn test_initial_position_setup() {
        let mut state = GameState::new();
        state.setup_initial_position();
        
        // Check kings are in place
        assert!(state.board.get_piece(Position::new(17, 0).unwrap()).is_some());
        assert_eq!(state.board.get_piece(Position::new(17, 0).unwrap()).unwrap().piece_type, PieceType::King);
        assert_eq!(state.board.get_piece(Position::new(17, 0).unwrap()).unwrap().color, Color::Black);
        
        assert!(state.board.get_piece(Position::new(18, 35).unwrap()).is_some());
        assert_eq!(state.board.get_piece(Position::new(18, 35).unwrap()).unwrap().piece_type, PieceType::King);
        assert_eq!(state.board.get_piece(Position::new(18, 35).unwrap()).unwrap().color, Color::White);
        
        // Check pawns are in place
        for file in 0..36 {
            assert!(state.board.get_piece(Position::new(file, 10).unwrap()).is_some());
            assert_eq!(state.board.get_piece(Position::new(file, 10).unwrap()).unwrap().piece_type, PieceType::Pawn);
            assert_eq!(state.board.get_piece(Position::new(file, 10).unwrap()).unwrap().color, Color::Black);
            
            assert!(state.board.get_piece(Position::new(file, 25).unwrap()).is_some());
            assert_eq!(state.board.get_piece(Position::new(file, 25).unwrap()).unwrap().piece_type, PieceType::Pawn);
            assert_eq!(state.board.get_piece(Position::new(file, 25).unwrap()).unwrap().color, Color::White);
        }
        
        // Check that there are legal moves from initial position
        let moves = state.generate_legal_moves();
        assert!(moves.len() > 0);
    }

    #[test]
    fn test_king_path_blocking() {
        let mut state = GameState::new();
        let king = Piece::new(PieceType::King, Color::Black, Position::new(10, 10).unwrap());
        state.place_piece(king);
        
        // Place a blocking piece one square away
        let blocker = Piece::new(PieceType::Pawn, Color::White, Position::new(12, 10).unwrap());
        state.place_piece(blocker);
        
        let moves = state.generate_legal_moves();
        // King should be able to move to the blocking piece (capture) but not beyond
        // Should be able to move to (11, 10) and (12, 10) but not (13, 10)
        let can_capture_blocker = moves.iter().any(|m| m.to == Position::new(12, 10).unwrap());
        let cannot_jump_over = !moves.iter().any(|m| m.to == Position::new(13, 10).unwrap());
        
        assert!(can_capture_blocker);
        assert!(cannot_jump_over);
    }

    #[test]
    fn test_pawn_promotion() {
        let mut state = GameState::new();
        // Place a black pawn near promotion zone (rank 24, moving to rank 25)
        let pawn = Piece::new(PieceType::Pawn, Color::Black, Position::new(10, 24).unwrap());
        state.place_piece(pawn);
        
        let moves = state.generate_legal_moves();
        // Should have a move to rank 25 that is promoted
        let promotion_move = moves.iter().find(|m| m.to.rank == 25);
        assert!(promotion_move.is_some());
        assert!(promotion_move.unwrap().promoted);
        
        // Place a white pawn near promotion zone (rank 11, moving to rank 10)
        let mut state2 = GameState::new();
        // Make a dummy move to switch to White's turn
        let dummy_piece = Piece::new(PieceType::King, Color::Black, Position::new(0, 0).unwrap());
        state2.place_piece(dummy_piece);
        let dummy_move = Move::new(Position::new(0, 0).unwrap(), Position::new(0, 1).unwrap());
        state2.make_move(dummy_move); // This switches to White's turn
        
        let pawn2 = Piece::new(PieceType::Pawn, Color::White, Position::new(10, 11).unwrap());
        state2.place_piece(pawn2);
        
        let moves2 = state2.generate_legal_moves();
        // Should have a move to rank 10 that is promoted
        let promotion_move2 = moves2.iter().find(|m| m.to.rank == 10);
        assert!(promotion_move2.is_some());
        assert!(promotion_move2.unwrap().promoted);
    }

    #[test]
    fn test_capturing_range_movement() {
        let mut state = GameState::new();
        
        // Place a Great General (promoted Free King with capturing range movement)
        let great_general = Piece::new(PieceType::GreatGeneral, Color::Black, Position::new(10, 10).unwrap());
        state.place_piece(great_general);
        
        // Place enemy pieces in the path (diagonal)
        let enemy1 = Piece::new(PieceType::Pawn, Color::White, Position::new(11, 11).unwrap());
        state.place_piece(enemy1);
        let enemy2 = Piece::new(PieceType::Pawn, Color::White, Position::new(12, 12).unwrap());
        state.place_piece(enemy2);
        let enemy3 = Piece::new(PieceType::Pawn, Color::White, Position::new(13, 13).unwrap());
        state.place_piece(enemy3);
        
        // Place a friendly piece in the path (should be captured, but movement continues)
        let friendly = Piece::new(PieceType::Pawn, Color::Black, Position::new(14, 14).unwrap());
        state.place_piece(friendly);
        
        // Place an enemy piece at the destination (after the friendly piece)
        let enemy_dest = Piece::new(PieceType::Pawn, Color::White, Position::new(15, 15).unwrap());
        state.place_piece(enemy_dest);
        
        // Test that we cannot land on the friendly piece
        let mv1 = Move::new_with_promotion(Position::new(10, 10).unwrap(), Position::new(14, 14).unwrap(), false);
        // This should fail because we can't land on a friendly piece
        assert!(state.make_move(mv1).is_none());
        
        // But we can move past it to the enemy piece
        let mv = Move::new_with_promotion(Position::new(10, 10).unwrap(), Position::new(15, 15).unwrap(), false);
        // make_move returns None for regular moves (Some(intermediate) only for two-step moves)
        assert!(state.make_move(mv).is_none());
        
        // Verify all pieces in the path are gone (including the friendly piece)
        assert!(state.board.is_empty(Position::new(11, 11).unwrap()), "Enemy piece 1 should be captured");
        assert!(state.board.is_empty(Position::new(12, 12).unwrap()), "Enemy piece 2 should be captured");
        assert!(state.board.is_empty(Position::new(13, 13).unwrap()), "Enemy piece 3 should be captured");
        assert!(state.board.is_empty(Position::new(14, 14).unwrap()), "Friendly piece should be captured");
        
        // Verify the Great General is at the destination (destination is not empty, it has the Great General)
        let moved_piece = state.board.get_piece(Position::new(15, 15).unwrap());
        assert!(moved_piece.is_some(), "Great General should be at destination");
        assert_eq!(moved_piece.unwrap().piece_type, PieceType::GreatGeneral);
        assert_eq!(moved_piece.unwrap().color, Color::Black);
        
        // Verify the original position is now empty
        assert!(state.board.is_empty(Position::new(10, 10).unwrap()), "Original position should be empty");
    }
}

