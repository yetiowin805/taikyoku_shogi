use crate::game_state::{GameState, Move, MoveData};
use crate::piece::{Color, Piece};
use crate::position::Position;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameRecord {
    pub timestamp: u64,
    pub moves: Vec<MoveRecord>,
    pub result: Option<GameResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MoveRecordData {
    Standard,
    TwoStep { intermediate_file: u8, intermediate_rank: u8 },
    FreeEagle { path: Vec<Position> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveRecord {
    pub move_number: usize,
    pub color: Color,
    pub from_file: u8,
    pub from_rank: u8,
    pub to_file: u8,
    pub to_rank: u8,
    pub promoted: bool,
    pub data: MoveRecordData,
    /// Black-absolute search score for the chosen line (AB games).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eval: Option<i32>,
    /// Black-absolute stand-pat at the root before the move (AB games).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub static_eval: Option<i32>,
    /// Nodes searched when choosing this move (AB games).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nodes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameResult {
    BlackWins,
    WhiteWins,
    Draw,
}

pub struct GameHistory {
    current_game: Option<GameRecord>,
    games_dir: String,
}

impl GameHistory {
    pub fn new(games_dir: &str) -> GameHistory {
        // Create games directory if it doesn't exist
        if !Path::new(games_dir).exists() {
            fs::create_dir_all(games_dir).expect("Failed to create games directory");
        }

        GameHistory {
            current_game: None,
            games_dir: games_dir.to_string(),
        }
    }

    /// Start recording a new game
    pub fn start_game(&mut self) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        self.current_game = Some(GameRecord {
            timestamp,
            moves: Vec::new(),
            result: None,
        });
    }

    /// Record a move
    pub fn record_move(&mut self, mv: &Move, color: Color, move_number: usize, promoted: bool, intermediate: Option<Position>) {
        if let Some(ref mut game) = self.current_game {
            // Convert MoveData to MoveRecordData
            let data = match &mv.data {
                MoveData::Standard => {
                    // Use intermediate parameter if provided (for backwards compatibility)
                    if let Some(inter) = intermediate {
                        MoveRecordData::TwoStep { 
                            intermediate_file: inter.file, 
                            intermediate_rank: inter.rank 
                        }
                    } else {
                        MoveRecordData::Standard
                    }
                }
                MoveData::TwoStep { intermediate } => {
                    MoveRecordData::TwoStep { 
                        intermediate_file: intermediate.file, 
                        intermediate_rank: intermediate.rank 
                    }
                }
                MoveData::FreeEagle { path } => {
                    MoveRecordData::FreeEagle { path: path.clone() }
                }
            };
            
            game.moves.push(MoveRecord {
                move_number,
                color,
                from_file: mv.from.file,
                from_rank: mv.from.rank,
                to_file: mv.to.file,
                to_rank: mv.to.rank,
                promoted,
                data,
                eval: None,
                static_eval: None,
                nodes: None,
            });
        }
    }

    /// End the game and save it
    pub fn end_game(&mut self, result: GameResult) -> Result<String, String> {
        let mut game = self
            .current_game
            .take()
            .ok_or_else(|| "No game in progress".to_string())?;
        game.result = Some(result);
        self.save_game(&game, None)
    }

    /// Save an arbitrary game record (e.g. a branched debug session).
    /// If `filename` is None, uses `game_<timestamp>.json` under the games directory.
    pub fn save_game(&self, game: &GameRecord, filename: Option<&str>) -> Result<String, String> {
        let path = match filename {
            Some(name) => {
                let name = name.strip_prefix("games/").unwrap_or(name);
                format!("{}/{}", self.games_dir, name)
            }
            None => {
                let timestamp = if game.timestamp > 0 {
                    game.timestamp
                } else {
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                };
                format!("{}/game_{}.json", self.games_dir, timestamp)
            }
        };

        let json = serde_json::to_string_pretty(game)
            .map_err(|e| format!("Failed to serialize game: {}", e))?;

        fs::write(&path, json)
            .map_err(|e| format!("Failed to write game file: {}", e))?;

        Ok(path)
    }

    /// Convert a live Move into a MoveRecord for persistence
    pub fn move_to_record(mv: &Move, color: Color, move_number: usize) -> MoveRecord {
        let data = match &mv.data {
            MoveData::Standard => MoveRecordData::Standard,
            MoveData::TwoStep { intermediate } => MoveRecordData::TwoStep {
                intermediate_file: intermediate.file,
                intermediate_rank: intermediate.rank,
            },
            MoveData::FreeEagle { path } => MoveRecordData::FreeEagle {
                path: path.clone(),
            },
        };

        MoveRecord {
            move_number,
            color,
            from_file: mv.from.file,
            from_rank: mv.from.rank,
            to_file: mv.to.file,
            to_rank: mv.to.rank,
            promoted: mv.promoted,
            data,
            eval: None,
            static_eval: None,
            nodes: None,
        }
    }

    /// Like [`Self::move_to_record`], attaching optional AB search telemetry.
    pub fn move_to_record_with_eval(
        mv: &Move,
        color: Color,
        move_number: usize,
        eval: Option<i32>,
        static_eval: Option<i32>,
        nodes: Option<u64>,
    ) -> MoveRecord {
        let mut rec = Self::move_to_record(mv, color, move_number);
        rec.eval = eval;
        rec.static_eval = static_eval;
        rec.nodes = nodes;
        rec
    }

    /// Convert a MoveRecord back into a live Move
    pub fn record_to_move(mv: &MoveRecord) -> Result<Move, String> {
        let from = Position::new(mv.from_file, mv.from_rank)
            .ok_or_else(|| format!("Invalid from position: ({}, {})", mv.from_file, mv.from_rank))?;
        let to = Position::new(mv.to_file, mv.to_rank)
            .ok_or_else(|| format!("Invalid to position: ({}, {})", mv.to_file, mv.to_rank))?;

        Ok(match &mv.data {
            MoveRecordData::Standard => Move::new_with_promotion(from, to, mv.promoted),
            MoveRecordData::TwoStep {
                intermediate_file,
                intermediate_rank,
            } => {
                let intermediate = Position::new(*intermediate_file, *intermediate_rank)
                    .ok_or_else(|| {
                        format!(
                            "Invalid intermediate position: ({}, {})",
                            intermediate_file, intermediate_rank
                        )
                    })?;
                Move::new_two_step_with_promotion(from, intermediate, to, mv.promoted)
            }
            MoveRecordData::FreeEagle { path } => {
                let mut move_obj = Move::new_free_eagle(from, to, path.clone());
                move_obj.promoted = mv.promoted;
                move_obj
            }
        })
    }

    /// List JSON games under the primary directory (relative paths, recursive).
    pub fn list_games(&self) -> Result<Vec<String>, String> {
        Self::list_game_rel_paths(&self.games_dir, false)
    }

    /// List loadable games with directory prefixes (recursive under known roots).
    ///
    /// Roots: legacy `games/`, `data/raw/games/` (all nested `.json` except metadata),
    /// and `data/raw/tourney/` (`slot*.json` only).
    pub fn list_games_all(&self) -> Result<Vec<String>, String> {
        let mut games = Vec::new();
        for rel in Self::list_game_rel_paths(&self.games_dir, false)? {
            games.push(format!("{}/{}", self.games_dir.trim_end_matches('/'), rel));
        }
        for rel in Self::list_game_rel_paths("data/raw/games", false)? {
            games.push(format!("data/raw/games/{rel}"));
        }
        for rel in Self::list_game_rel_paths("data/raw/tourney", true)? {
            games.push(format!("data/raw/tourney/{rel}"));
        }
        games.sort();
        games.reverse();
        Ok(games)
    }

    /// Relative paths of game JSON files under `dir` (recursive).
    ///
    /// When `tourney_slots_only`, only `slot*.json` are included (skips state/elo/…).
    pub fn list_game_rel_paths(dir: &str, tourney_slots_only: bool) -> Result<Vec<String>, String> {
        let root = Path::new(dir);
        if !root.is_dir() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        Self::walk_game_json(root, root, tourney_slots_only, &mut out)?;
        out.sort();
        out.reverse();
        Ok(out)
    }

    fn walk_game_json(
        root: &Path,
        dir: &Path,
        tourney_slots_only: bool,
        out: &mut Vec<String>,
    ) -> Result<(), String> {
        let entries = fs::read_dir(dir)
            .map_err(|e| format!("Failed to read games directory {}: {}", dir.display(), e))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let path = entry.path();
            if path.is_dir() {
                Self::walk_game_json(root, &path, tourney_slots_only, out)?;
                continue;
            }
            if !Self::is_listable_game_json(&path, tourney_slots_only) {
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
        Ok(())
    }

    fn is_listable_game_json(path: &Path, tourney_slots_only: bool) -> bool {
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            return false;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return false;
        };
        // Tourney / training metadata that is not a GameRecord.
        const SKIP: &[&str] = &[
            "state.json",
            "elo.json",
            "ratings.json",
            "manifest.json",
            "grid.json",
            "samples.json",
            "status.json",
        ];
        if SKIP.iter().any(|s| name.eq_ignore_ascii_case(s)) {
            return false;
        }
        if tourney_slots_only {
            return name.starts_with("slot") && name.ends_with(".json");
        }
        true
    }

    /// Resolve a game path: absolute/relative with `/`, else under `games_dir`.
    pub fn resolve_game_path(&self, filename: &str) -> std::path::PathBuf {
        let filename = filename
            .strip_prefix("games/")
            .unwrap_or(filename);
        let p = Path::new(filename);
        if p.is_absolute() || filename.contains('/') {
            p.to_path_buf()
        } else {
            Path::new(&self.games_dir).join(filename)
        }
    }

    /// Load a legacy v1 game by filename (extra JSON fields ignored by serde).
    pub fn load_game(&self, filename: &str) -> Result<GameRecord, String> {
        let path = self.resolve_game_path(filename);
        let contents = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read game file {}: {}", path.display(), e))?;

        let game: GameRecord = serde_json::from_str(&contents)
            .map_err(|e| format!("Failed to parse game file: {}", e))?;

        Ok(game)
    }

    /// Load raw JSON from a game path (v1 or v2).
    pub fn read_game_file(&self, filename: &str) -> Result<(std::path::PathBuf, String), String> {
        let path = self.resolve_game_path(filename);
        let contents = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read game file {}: {}", path.display(), e))?;
        Ok((path, contents))
    }

    /// Format board as a visual string representation
    /// Uses flipped coordinates: files (rightmost=1, leftmost=36), ranks (top=1一, bottom=36三十六)
    pub fn format_board(&self, state: &GameState) -> String {
        let board = state.get_board();
        let mut result = String::new();
        
        // Header with file numbers (flipped: rightmost is 1, leftmost is 36, showing every 6th for readability)
        for file_display in (1..=36).rev() {
            // Match the exact spacing pattern used in the board
            // Each file column is 3 characters wide
            if file_display % 6 == 1 || file_display == 36 {
                // Show the file number, centered in its 3-character column
                result.push_str(&format!("{:^3}", file_display));
            } else {
                // Empty space for files we don't show
                result.push_str("   ");
            }
            // Add spacing every 6 files (same as board)
            if file_display % 6 == 0 && file_display < 36 {
                result.push(' ');
            }
        }
        result.push('\n');
        
        // Board rows (top rank is 1一, bottom rank is 36三十六)
        // Iterate from top (rank 35 in 0-indexed = rank 1一 in display) to bottom (rank 0 in 0-indexed = rank 36三十六 in display)
        for rank_display in 1..=36 {
            // rank_display is the displayed rank (1-36, top to bottom)
            // Convert to 0-indexed internal rank: rank 1 (top) -> rank 35, rank 36 (bottom) -> rank 0
            let rank_internal = 36 - rank_display;
            
            // Files from right (file 1) to left (file 36)
            for file_display in 1..=36 {
                // file_display is the displayed file (1-36, rightmost to leftmost)
                // Convert to 0-indexed internal file: file 1 (rightmost) -> file 35, file 0 (leftmost) -> file 35
                let file_internal = file_display - 1;
                
                let pos = Position::new(file_internal, rank_internal).unwrap();
                if let Some(piece) = board.get_piece(pos) {
                    let symbol = self.piece_to_symbol(&piece);
                    result.push_str(&symbol);
                } else {
                    result.push_str("...");
                }
                // Add spacing every 6 files for readability
                if file_display % 6 == 0 && file_display < 36 {
                    result.push(' ');
                }
            }
            // Right side rank number with Japanese numeral
            let rank_jp = self.to_japanese_numeral(rank_display);
            result.push_str(&format!("  {}\n", rank_jp));
        }
        
        // Footer with file numbers (same as header)
        for file_display in (1..=36).rev() {
            // Match the exact spacing pattern used in the board
            // Each file column is 3 characters wide
            if file_display % 6 == 1 || file_display == 36 {
                // Show the file number, centered in its 3-character column
                result.push_str(&format!("{:^3}", file_display));
            } else {
                // Empty space for files we don't show
                result.push_str("   ");
            }
            // Add spacing every 6 files (same as board)
            if file_display % 6 == 0 && file_display < 36 {
                result.push(' ');
            }
        }
        result.push('\n');
        
        result
    }

    /// Convert a piece to a 3-character symbol (2 chars + promoted indicator)
    /// For promoted pieces, shows the base piece type (e.g., "P+" for promoted pawn, not "G+")
    /// Convert a number to Japanese/Chinese numerals
    fn to_japanese_numeral(&self, n: u8) -> String {
        if n == 0 {
            return "零".to_string();
        }
        
        let digits = [
            "", "一", "二", "三", "四", "五", "六", "七", "八", "九"
        ];
        
        let mut result = String::new();
        let n = n as usize;
        
        if n >= 30 {
            let tens = n / 10;
            let ones = n % 10;
            result.push_str(digits[tens]);
            result.push_str("十");
            if ones > 0 {
                result.push_str(digits[ones]);
            }
        } else if n >= 20 {
            result.push_str("二");
            result.push_str("十");
            let ones = n % 10;
            if ones > 0 {
                result.push_str(digits[ones]);
            }
        } else if n >= 10 {
            result.push_str("十");
            let ones = n % 10;
            if ones > 0 {
                result.push_str(digits[ones]);
            }
        } else {
            result.push_str(digits[n]);
        }
        
        result
    }
    
    /// Flip file number for shogi notation (rightmost is 1, leftmost is 36)
    /// Takes 0-indexed file (0-35) and returns 1-indexed flipped file (1-36)
    /// File 0 (leftmost) becomes 36, file 35 (rightmost) becomes 1
    fn flip_file(&self, file: u8) -> u8 {
        36 - file  // This already gives 1-indexed: file 0 -> 36, file 35 -> 1
    }
    
    /// Flip rank number for shogi notation (top is 1, bottom is 36)
    /// Takes 0-indexed rank (0-35) and returns 1-indexed flipped rank (1-36)
    /// Rank 0 (bottom) becomes 36, rank 35 (top) becomes 1
    fn flip_rank(&self, rank: u8) -> u8 {
        36 - rank  // This already gives 1-indexed: rank 0 -> 36, rank 35 -> 1
    }
    
    fn piece_to_symbol(&self, piece: &Piece) -> String {
        // Get the base piece type for display (unpromoted form)
        // For promoted pieces, show what they were before promotion
        // For unpromoted pieces, show the piece type itself
        // Use the base_type() method which uses stored base_piece_type if available
        let display_type = piece.base_type();
        
        let base = display_type.display_symbol();
        
        // Use uppercase for Black, lowercase for White
        let symbol = if piece.color == Color::Black {
            base.to_string()
        } else {
            base.to_lowercase()
        };
        
        // Add "+" at the start if promoted, pad to 3 characters
        if piece.is_promoted {
            format!("+{:<2}", symbol)  // + at start, left-align symbol to 2 chars
        } else {
            format!("{:<3}", symbol)  // Left-align symbol to 3 chars
        }
    }

    /// Analyze a Free Eagle path to determine if it should be displayed as multiple moves
    /// Returns: (should_display_as_multiple, capture_positions, direction_change_indices)
    fn analyze_free_eagle_path(
        &self,
        path: &[Position],
        board: &crate::board::Board,
    ) -> (bool, Vec<Position>, Vec<usize>) {
        if path.len() < 2 {
            return (false, Vec::new(), Vec::new());
        }

        let mut capture_positions = Vec::new();
        let mut direction_change_indices = Vec::new();
        let mut prev_direction: Option<(i8, i8)> = None;
        let returns_to_origin = path[0] == path[path.len() - 1];

        // Check each step in the path
        for i in 1..path.len() {
            // Check for capture at this position (before the move)
            if board.get_piece(path[i]).is_some() {
                capture_positions.push(path[i]);
            }

            // Calculate direction for this step
            let file_delta = path[i].file as i16 - path[i - 1].file as i16;
            let rank_delta = path[i].rank as i16 - path[i - 1].rank as i16;
            
            // Normalize direction to sign only
            let direction = (
                if file_delta == 0 { 0 } else if file_delta > 0 { 1 } else { -1 },
                if rank_delta == 0 { 0 } else if rank_delta > 0 { 1 } else { -1 },
            );

            // Check if direction changed
            if let Some(prev_dir) = prev_direction {
                if prev_dir != direction {
                    direction_change_indices.push(i);
                }
            }
            prev_direction = Some(direction);
        }

        // Should display as multiple if: returns to origin, has captures, or has direction changes
        let should_display_as_multiple = returns_to_origin 
            || !capture_positions.is_empty() 
            || !direction_change_indices.is_empty();

        (should_display_as_multiple, capture_positions, direction_change_indices)
    }

    /// Get a formatted string representation of a game
    pub fn format_game(&self, game: &GameRecord) -> String {
        let mut result = format!("Game from {}\n", game.timestamp);
        result.push_str(&format!("Moves: {}\n", game.moves.len()));
        
        if let Some(ref game_result) = game.result {
            result.push_str(&format!("Result: {:?}\n", game_result));
        }
        
        // Show initial board
        result.push_str("\n=== Initial Position ===\n");
        let mut initial_state = GameState::new();
        initial_state.setup_initial_position();
        result.push_str(&self.format_board(&initial_state));
        
        // Show board after each move (or every N moves for long games)
        let show_every = if game.moves.len() > 50 { 2 } else { 1 };
        
        result.push_str("\n=== Move list ===\n");
        let mut current_state = GameState::new();
        current_state.setup_initial_position();
        
        let mut idx = 0;
        while idx < game.moves.len() {
            let mv = &game.moves[idx];
            let color_str = match mv.color {
                Color::Black => "Black",
                Color::White => "White",
            };
            
            // Get piece type before applying the move (for shogi-style notation)
            let from = Position::new(mv.from_file, mv.from_rank).unwrap();
            let to = Position::new(mv.to_file, mv.to_rank).unwrap();
            let piece = current_state.get_board().get_piece(from);
            
            // Track how many moves will be displayed for this MoveRecord
            let mut moves_displayed_this_record = 0;
            
            // Analyze Free Eagle path before applying move (if applicable)
            let free_eagle_analysis = if let MoveRecordData::FreeEagle { path } = &mv.data {
                Some(self.analyze_free_eagle_path(path, current_state.get_board()))
            } else {
                None
            };
            
            
            if let Some(p) = piece {
                // Check if this piece type has two-step movement capability
                let config = crate::movement::MovementConfig::for_piece(&p);
                let has_two_step = config.capabilities.iter().any(|cap| {
                    matches!(cap, crate::movement::types::MovementCapability::TwoStep { .. })
                });
                
                if has_two_step {
                    // Check if this is a two-step move by checking stored intermediate position
                    if let MoveRecordData::TwoStep { intermediate_file: inter_file, intermediate_rank: inter_rank } = &mv.data {
                        // This is a two-step move - format as two lines showing the two steps
                        let intermediate = Position::new(*inter_file, *inter_rank).unwrap();
                        let piece_symbol = {
                            let symbol = p.base_symbol();
                            if p.is_promoted {
                                format!("+{}", symbol)
                            } else {
                                symbol.to_string()
                            }
                        };
                        
                        // Format first step: from -> intermediate
                        let inter_file_flipped = self.flip_file(intermediate.file);
                        let inter_rank_flipped = self.flip_rank(intermediate.rank);
                        let from_file_flipped = self.flip_file(mv.from_file);
                        let from_rank_flipped = self.flip_rank(mv.from_rank);
                        let inter_rank_jp = self.to_japanese_numeral(inter_rank_flipped);
                        let from_rank_jp = self.to_japanese_numeral(from_rank_flipped);
                        
                        result.push_str(&format!(
                            "{}. {}: {}{}{}{}{}{}\n",
                            mv.move_number, color_str,
                            inter_file_flipped, inter_rank_jp,  // Destination (intermediate)
                            piece_symbol.clone(),                  // Piece type
                            from_file_flipped, from_rank_jp,  // Source
                            ""  // No promotion on first step
                        ));
                        moves_displayed_this_record += 1;
                        
                        // Format second step: intermediate -> to (with next move number)
                        let to_file_flipped = self.flip_file(mv.to_file);
                        let to_rank_flipped = self.flip_rank(mv.to_rank);
                        let to_rank_jp = self.to_japanese_numeral(to_rank_flipped);
                        
                        result.push_str(&format!(
                            "{}. {}: {}{}{}{}{}{}\n",
                            mv.move_number + 1, color_str,  // Next move number for second step
                            to_file_flipped, to_rank_jp,  // Destination (final)
                            piece_symbol,                  // Piece type
                            inter_file_flipped, inter_rank_jp,  // Source (intermediate)
                            if mv.promoted { "成" } else { "" }  // Promotion indicator
                        ));
                        moves_displayed_this_record += 1;
                    } else {
                        // Single-step move - format normally
                        let piece_symbol = {
                            let symbol = p.base_symbol();
                            if p.is_promoted {
                                format!("+{}", symbol)
                            } else {
                                symbol.to_string()
                            }
                        };
                        let to_file_flipped = self.flip_file(mv.to_file);
                        let from_file_flipped = self.flip_file(mv.from_file);
                        let to_rank_flipped = self.flip_rank(mv.to_rank);
                        let from_rank_flipped = self.flip_rank(mv.from_rank);
                        let to_rank_jp = self.to_japanese_numeral(to_rank_flipped);
                        let from_rank_jp = self.to_japanese_numeral(from_rank_flipped);
                        
                        result.push_str(&format!(
                            "{}. {}: {}{}{}{}{}{}\n",
                            mv.move_number, color_str,
                            to_file_flipped, to_rank_jp,
                            piece_symbol,
                            from_file_flipped, from_rank_jp,
                            if mv.promoted { "成" } else { "" }
                        ));
                        moves_displayed_this_record += 1;
                    }
                } else {
                    // Check if this is a Free Eagle move with a path
                    if let MoveRecordData::FreeEagle { path } = &mv.data {
                        if let Some((should_display_as_multiple, ref capture_positions, ref direction_change_indices)) = 
                            free_eagle_analysis {
                            
                            let piece_symbol = {
                                let symbol = p.base_symbol();
                                if p.is_promoted {
                                    format!("+{}", symbol)
                                } else {
                                    symbol.to_string()
                                }
                            };
                            
                            if should_display_as_multiple {
                                // Display as multiple moves - show each step with capture or direction change
                                let mut current_move_number = mv.move_number;
                                
                                // Create a set of positions that should be displayed
                                let mut positions_to_display = std::collections::HashSet::new();
                                for capture_pos in capture_positions {
                                    positions_to_display.insert(*capture_pos);
                                }
                                for idx in direction_change_indices {
                                    if *idx < path.len() {
                                        positions_to_display.insert(path[*idx]);
                                    }
                                }
                                // Always show final position
                                positions_to_display.insert(to);
                                
                                // Display each step
                                for i in 1..path.len() {
                                    let step_to = path[i];
                                    
                                    // Display this step if it's a capture, direction change, or final position
                                    if positions_to_display.contains(&step_to) {
                                        let step_from = if i > 0 { path[i - 1] } else { from };
                                        
                                        let to_file_flipped = self.flip_file(step_to.file);
                                        let to_rank_flipped = self.flip_rank(step_to.rank);
                                        let from_file_flipped = self.flip_file(step_from.file);
                                        let from_rank_flipped = self.flip_rank(step_from.rank);
                                        let to_rank_jp = self.to_japanese_numeral(to_rank_flipped);
                                        let from_rank_jp = self.to_japanese_numeral(from_rank_flipped);
                                        
                                        // Only show promotion on final step
                                        let promotion_str = if step_to == to && mv.promoted { "成" } else { "" };
                                        
                                        result.push_str(&format!(
                                            "{}. {}: {}{}{}{}{}{}\n",
                                            current_move_number, color_str,
                                            to_file_flipped, to_rank_jp,
                                            piece_symbol.clone(),
                                            from_file_flipped, from_rank_jp,
                                            promotion_str
                                        ));
                                        
                                        current_move_number += 1;
                                        moves_displayed_this_record += 1;
                                    }
                                }
                                } else {
                                // Display as single move
                                let to_file_flipped = self.flip_file(mv.to_file);
                                let from_file_flipped = self.flip_file(mv.from_file);
                                let to_rank_flipped = self.flip_rank(mv.to_rank);
                                let from_rank_flipped = self.flip_rank(mv.from_rank);
                                let to_rank_jp = self.to_japanese_numeral(to_rank_flipped);
                                let from_rank_jp = self.to_japanese_numeral(from_rank_flipped);
                                
                                result.push_str(&format!(
                                    "{}. {}: {}{}{}{}{}{}\n",
                                    mv.move_number, color_str,
                                    to_file_flipped, to_rank_jp,
                                    piece_symbol,
                                    from_file_flipped, from_rank_jp,
                                    if mv.promoted { "成" } else { "" }
                                ));
                                moves_displayed_this_record += 1;
                            }
                        }
                    } else {
                        // Regular move (not Tengu, not Free Eagle with path)
                        let piece_symbol = {
                            let symbol = p.base_symbol();
                            if p.is_promoted {
                                format!("+{}", symbol)
                            } else {
                                symbol.to_string()
                            }
                        };
                        
                        let to_file_flipped = self.flip_file(mv.to_file);
                        let from_file_flipped = self.flip_file(mv.from_file);
                        let to_rank_flipped = self.flip_rank(mv.to_rank);
                        let from_rank_flipped = self.flip_rank(mv.from_rank);
                        let to_rank_jp = self.to_japanese_numeral(to_rank_flipped);
                        let from_rank_jp = self.to_japanese_numeral(from_rank_flipped);
                        
                        result.push_str(&format!(
                            "{}. {}: {}{}{}{}{}{}\n",
                            mv.move_number, color_str,
                            to_file_flipped, to_rank_jp,
                            piece_symbol,
                            from_file_flipped, from_rank_jp,
                            if mv.promoted { "成" } else { "" }
                        ));
                        moves_displayed_this_record += 1;
                    }
                }
            } else {
                // Fallback: piece not found
                let to_file_flipped = self.flip_file(mv.to_file);
                let from_file_flipped = self.flip_file(mv.from_file);
                let to_rank_flipped = self.flip_rank(mv.to_rank);
                let from_rank_flipped = self.flip_rank(mv.from_rank);
                let to_rank_jp = self.to_japanese_numeral(to_rank_flipped);
                let from_rank_jp = self.to_japanese_numeral(from_rank_flipped);
                
                result.push_str(&format!(
                    "{}. {}: {}{}?{}{}{}\n",
                    mv.move_number, color_str,
                    to_file_flipped, to_rank_jp,
                    from_file_flipped, from_rank_jp,
                    if mv.promoted { "成" } else { "" }
                ));
            }
            
            // Apply move to current state
            // Create Move from MoveRecordData
            let move_obj = match &mv.data {
                MoveRecordData::Standard => {
                    Move::new_with_promotion(from, to, mv.promoted)
                }
                MoveRecordData::TwoStep { intermediate_file, intermediate_rank } => {
                    let intermediate = Position::new(*intermediate_file, *intermediate_rank).unwrap();
                    Move::new_two_step_with_promotion(from, intermediate, to, mv.promoted)
                }
                MoveRecordData::FreeEagle { path } => {
                    Move::new_free_eagle(from, to, path.clone())
                }
            };
            let turn_before = current_state.get_current_turn();
            let move_result = current_state.make_move(move_obj);
            let turn_after = current_state.get_current_turn();
            
            // Check if move succeeded:
            // - Two-step moves return Some(intermediate)
            // - Single moves return None but turn changes
            // - Failed moves return None and turn doesn't change
            let move_succeeded = move_result.is_some() || (turn_before != turn_after);
            
            if !move_succeeded {
                // Move failed - this shouldn't happen in a valid game, but log it
                result.push_str(&format!("WARNING: Move {} failed to apply!\n", mv.move_number));
            }
            
            // Show board periodically
            // For two-step moves, show after the second step (move_number + 1)
            // For Free Eagle multi-moves, show after the last displayed step
            let display_move_number = if matches!(mv.data, MoveRecordData::TwoStep { .. }) {
                mv.move_number + 1
            } else if let Some((should_display_as_multiple, _, _)) = free_eagle_analysis {
                if should_display_as_multiple {
                    mv.move_number + moves_displayed_this_record - 1
                } else {
                    mv.move_number
                }
            } else {
                mv.move_number
            };
            if (idx + 1) % show_every == 0 || idx == game.moves.len() - 1 {
                result.push_str(&format!("\n=== After move {} ===\n", display_move_number));
                result.push_str(&self.format_board(&current_state));
            }
            
            idx += 1;
        }
        
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_root(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!("taikyoku-games-{label}-{nanos}"));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn list_game_rel_paths_recurses_and_skips_metadata() {
        let root = tmp_root("recurse");
        fs::create_dir_all(root.join("nested/deep")).unwrap();
        fs::write(root.join("top.json"), "{}").unwrap();
        fs::write(root.join("nested/deep/leaf.json"), "{}").unwrap();
        fs::write(root.join("nested/state.json"), "{}").unwrap();
        fs::write(root.join("nested/elo.json"), "{}").unwrap();

        let paths = GameHistory::list_game_rel_paths(root.to_str().unwrap(), false).unwrap();
        assert!(paths.iter().any(|p| p == "top.json"));
        assert!(paths.iter().any(|p| p == "nested/deep/leaf.json"));
        assert!(!paths.iter().any(|p| p.contains("state.json")));
        assert!(!paths.iter().any(|p| p.contains("elo.json")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn list_tourney_paths_only_slot_json() {
        let root = tmp_root("tourney");
        fs::create_dir_all(root.join("run-a")).unwrap();
        fs::write(root.join("run-a/slot0001-a-vs-b-a-black.json"), "{}").unwrap();
        fs::write(root.join("run-a/state.json"), "{}").unwrap();
        fs::write(root.join("run-a/ratings.json"), "{}").unwrap();
        fs::write(root.join("run-a/other.json"), "{}").unwrap();

        let paths = GameHistory::list_game_rel_paths(root.to_str().unwrap(), true).unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("slot0001-a-vs-b-a-black.json"));
        let _ = fs::remove_dir_all(&root);
    }
}

