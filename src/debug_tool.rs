use crate::board::Board;
use crate::game_history::{GameHistory, GameRecord, GameResult, MoveRecord};
use crate::game_state::{GameState, Move};
use crate::piece::{Color, Piece, PieceType};
use crate::player::player_by_name;
use crate::position::Position;
use std::io::{self, BufRead, Write};
use std::time::{SystemTime, UNIX_EPOCH};

/// Snapshot of a position used when the session diverges via edits.
#[derive(Clone)]
struct SetupSnapshot {
    board: Board,
    turn: Color,
    draw_counter: u32,
}

/// Interactive replay / edit / branch harness.
///
/// Navigation rebuilds from the initial position or a setup snapshot by replaying
/// MoveRecords — no MoveDelta undo (keeps Free Eagle / two-step trustworthy).
pub struct DebugTool {
    trunk: Option<GameRecord>,
    /// Index into trunk where divergence starts (trunk prefix length when no setup).
    branch_index: usize,
    setup: Option<SetupSnapshot>,
    branch: Vec<MoveRecord>,
    /// Ply into `effective_moves()` (0 = start position).
    cursor: usize,
    game_state: GameState,
    game_history: GameHistory,
}

impl DebugTool {
    pub fn new() -> Self {
        let mut game_state = GameState::new();
        game_state.setup_initial_position();

        DebugTool {
            trunk: None,
            branch_index: 0,
            setup: None,
            branch: Vec::new(),
            cursor: 0,
            game_state,
            game_history: GameHistory::new("games"),
        }
    }

    /// Effective timeline: trunk prefix + branch, or only branch when a setup exists.
    fn effective_moves(&self) -> Vec<MoveRecord> {
        if self.setup.is_some() {
            return self.branch.clone();
        }

        let mut moves = Vec::new();
        if let Some(trunk) = &self.trunk {
            let end = self.branch_index.min(trunk.moves.len());
            moves.extend_from_slice(&trunk.moves[..end]);
        }
        moves.extend(self.branch.iter().cloned());
        moves
    }

    fn timeline_len(&self) -> usize {
        if self.setup.is_some() {
            self.branch.len()
        } else {
            let trunk_prefix = self
                .trunk
                .as_ref()
                .map(|t| self.branch_index.min(t.moves.len()))
                .unwrap_or(0);
            trunk_prefix + self.branch.len()
        }
    }

    /// Rebuild live state to the given ply by replaying MoveRecords.
    pub fn rebuild_to(&mut self, cursor: usize) -> Result<(), String> {
        let moves = self.effective_moves();
        let cursor = cursor.min(moves.len());

        if let Some(setup) = &self.setup {
            self.game_state = GameState::new();
            self.game_state
                .restore_position(setup.board.clone(), setup.turn, setup.draw_counter);
        } else {
            self.game_state = GameState::new();
            self.game_state.setup_initial_position();
        }

        for (i, record) in moves[..cursor].iter().enumerate() {
            let mv = GameHistory::record_to_move(record)?;
            self.game_state.set_current_turn(record.color);
            let turn_before = self.game_state.get_current_turn();
            let result = self.game_state.make_move(mv);
            let turn_after = self.game_state.get_current_turn();
            let ok = result.is_some() || turn_before != turn_after;
            if !ok {
                return Err(format!(
                    "Failed to replay move record {} at ply {}",
                    i + 1,
                    i + 1
                ));
            }
        }

        self.cursor = cursor;
        Ok(())
    }

    /// Prepare the timeline so the next applied move appends at `cursor`.
    /// Does not wipe prior branch moves when already at the tip of the line.
    fn ensure_branch_point(&mut self) {
        if self.setup.is_some() {
            // Setup-based timeline is just `branch`; drop moves after cursor.
            if self.cursor < self.branch.len() {
                self.branch.truncate(self.cursor);
            }
            return;
        }

        // Scrubbed back into the trunk prefix — diverge here and drop any branch.
        if self.cursor < self.branch_index {
            self.branch_index = self.cursor;
            self.branch.clear();
            return;
        }

        // Cursor is on the branch segment (or at its tip).
        let into_branch = self.cursor - self.branch_index;
        if into_branch < self.branch.len() {
            // Scrubbed back within the branch — drop future branch moves.
            self.branch.truncate(into_branch);
        }
        // else: already at tip (into_branch == branch.len()) — keep branch as-is.
    }

    /// Snapshot current board as a setup root (after edits). Clears forward branch.
    fn diverge_via_edit(&mut self) {
        self.setup = Some(SetupSnapshot {
            board: self.game_state.clone_board(),
            turn: self.game_state.get_current_turn(),
            draw_counter: self.game_state.get_turns_without_capture_or_promotion(),
        });
        // Trunk becomes reference-only; timeline is setup + branch.
        self.branch.clear();
        self.branch_index = self.cursor; // remember where we left the trunk
        self.cursor = 0;
    }

    pub fn load_game(&mut self, filename: &str) -> Result<(), String> {
        let filename = filename.strip_prefix("games/").unwrap_or(filename);
        let game_record = self.game_history.load_game(filename)?;
        let len = game_record.moves.len();

        self.trunk = Some(game_record);
        self.branch_index = len;
        self.setup = None;
        self.branch.clear();
        self.rebuild_to(0)?;
        Ok(())
    }

    pub fn forward(&mut self, n: usize) -> Result<(), String> {
        let target = (self.cursor + n).min(self.timeline_len());
        self.rebuild_to(target)
    }

    pub fn back(&mut self, n: usize) -> Result<(), String> {
        let target = self.cursor.saturating_sub(n);
        self.rebuild_to(target)
    }

    pub fn goto_move(&mut self, ply: usize) -> Result<(), String> {
        if ply > self.timeline_len() {
            return Err(format!(
                "Ply {} out of range (max {})",
                ply,
                self.timeline_len()
            ));
        }
        self.rebuild_to(ply)
    }

    fn shogi_to_internal_file(file: u8) -> Result<u8, String> {
        if !(1..=36).contains(&file) {
            return Err(format!("File must be between 1 and 36, got {}", file));
        }
        Ok(36 - file)
    }

    fn shogi_to_internal_rank(rank: u8) -> Result<u8, String> {
        if !(1..=36).contains(&rank) {
            return Err(format!("Rank must be between 1 and 36, got {}", rank));
        }
        Ok(36 - rank)
    }

    fn internal_to_shogi_file(file: u8) -> u8 {
        36 - file
    }

    fn internal_to_shogi_rank(rank: u8) -> u8 {
        36 - rank
    }

    fn parse_shogi_pos(file_s: &str, rank_s: &str) -> Result<Position, String> {
        let shogi_file: u8 = file_s.parse().map_err(|_| "Invalid file".to_string())?;
        let shogi_rank: u8 = rank_s.parse().map_err(|_| "Invalid rank".to_string())?;
        let file = Self::shogi_to_internal_file(shogi_file)?;
        let rank = Self::shogi_to_internal_rank(shogi_rank)?;
        Position::new(file, rank).ok_or_else(|| "Invalid position".to_string())
    }

    fn format_move(mv: &Move) -> String {
        let from_f = Self::internal_to_shogi_file(mv.from.file);
        let from_r = Self::internal_to_shogi_rank(mv.from.rank);
        let to_f = Self::internal_to_shogi_file(mv.to.file);
        let to_r = Self::internal_to_shogi_rank(mv.to.rank);
        let promo = if mv.promoted { " promote" } else { "" };
        let extra = match &mv.data {
            crate::game_state::MoveData::TwoStep { intermediate } => {
                format!(
                    " via {},{}",
                    Self::internal_to_shogi_file(intermediate.file),
                    Self::internal_to_shogi_rank(intermediate.rank)
                )
            }
            crate::game_state::MoveData::FreeEagle { path } => {
                format!(" free-eagle path_len={}", path.len())
            }
            crate::game_state::MoveData::Standard => String::new(),
        };
        format!(
            "{},{} -> {},{}{}{}",
            from_f, from_r, to_f, to_r, promo, extra
        )
    }

    fn is_in_check(&self, color: Color) -> bool {
        let board = self.game_state.get_board();
        for piece in board.get_pieces_by_color(color) {
            if piece.piece_type.is_royal()
                && board.is_position_attacked_by_color_for_check(piece.position, color.opposite())
            {
                return true;
            }
        }
        false
    }

    fn get_attackers(&self, position: Position, by_color: Option<Color>) -> Vec<Piece> {
        let board = self.game_state.get_board();
        let mut attackers = Vec::new();
        let colors: Vec<Color> = match by_color {
            Some(c) => vec![c],
            None => vec![Color::Black, Color::White],
        };
        for color in colors {
            for piece in board.get_pieces_by_color(color) {
                if piece.can_reach_boardlike(position, board) {
                    attackers.push(piece);
                }
            }
        }
        attackers
    }

    fn status_text(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!(
            "Cursor ply: {} / {}",
            self.cursor,
            self.timeline_len()
        ));
        lines.push(format!("Side to move: {:?}", self.game_state.get_current_turn()));
        if self.setup.is_some() {
            lines.push("Mode: branched from edited setup".to_string());
        } else if !self.branch.is_empty() || self.trunk.as_ref().map(|t| self.branch_index < t.moves.len()).unwrap_or(false) {
            lines.push(format!(
                "Mode: branched (trunk prefix {}, branch {} moves)",
                self.branch_index,
                self.branch.len()
            ));
        } else if self.trunk.is_some() {
            lines.push("Mode: replaying trunk".to_string());
        } else {
            lines.push("Mode: initial / scratch".to_string());
        }

        if let Some(w) = self.game_state.get_winner() {
            lines.push(format!("Winner: {:?} (all opponent royals captured)", w));
        }
        if self.game_state.is_draw_by_500_move_rule() {
            lines.push("Draw: 500-move rule".to_string());
        }
        if self.game_state.is_draw_by_insufficient_material() {
            lines.push("Draw: insufficient material".to_string());
        }
        lines.push(format!(
            "Black in check: {}",
            self.is_in_check(Color::Black)
        ));
        lines.push(format!(
            "White in check: {}",
            self.is_in_check(Color::White)
        ));
        lines.push(format!(
            "Legal moves: {}",
            self.game_state.generate_legal_moves().len()
        ));
        lines.join("\n")
    }

    /// Apply a live move onto the branch at the current cursor.
    fn apply_live_move(&mut self, mv: Move) -> Result<String, String> {
        self.ensure_branch_point();

        let color = self.game_state.get_current_turn();
        let move_number = self.cursor + 1;
        let turn_before = color;
        let applied = self.game_state.make_move(mv.clone());
        let turn_after = self.game_state.get_current_turn();
        if applied.is_none() && turn_before == turn_after {
            return Err("Move failed to apply".to_string());
        }

        let record = GameHistory::move_to_record(&mv, color, move_number);
        // ensure_branch_point left us at the tip of the branch segment.
        self.branch.push(record);
        self.cursor += 1;

        // Re-sync from rebuild for reliability
        let target = self.cursor;
        self.rebuild_to(target)?;
        Ok(format!("Applied: {}", Self::format_move(&mv)))
    }

    fn find_matching_moves(
        &self,
        from: Position,
        to: Position,
        promote: Option<bool>,
    ) -> Vec<Move> {
        self.game_state
            .generate_legal_moves()
            .into_iter()
            .filter(|mv| {
                mv.from == from
                    && mv.to == to
                    && promote.map(|p| mv.promoted == p).unwrap_or(true)
            })
            .collect()
    }

    fn build_save_record(&self) -> GameRecord {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut moves = self.effective_moves();
        // Renumber
        for (i, mv) in moves.iter_mut().enumerate() {
            mv.move_number = i + 1;
        }

        let result = match self.game_state.get_winner() {
            Some(Color::Black) => Some(GameResult::BlackWins),
            Some(Color::White) => Some(GameResult::WhiteWins),
            None if self.game_state.is_draw_by_500_move_rule()
                || self.game_state.is_draw_by_insufficient_material() =>
            {
                Some(GameResult::Draw)
            }
            None => None,
        };

        GameRecord {
            timestamp,
            moves,
            result,
        }
    }

    pub fn game_state_ref(&self) -> &GameState {
        &self.game_state
    }

    pub fn cursor_ply(&self) -> usize {
        self.cursor
    }

    pub fn timeline_length(&self) -> usize {
        self.timeline_len()
    }

    pub fn check_color(&self, color: Color) -> bool {
        self.is_in_check(color)
    }

    pub fn status_summary(&self) -> String {
        self.status_text()
    }

    pub fn to_shogi_file(file: u8) -> u8 {
        Self::internal_to_shogi_file(file)
    }

    pub fn to_shogi_rank(rank: u8) -> u8 {
        Self::internal_to_shogi_rank(rank)
    }

    pub fn parse_shogi_position(&self, file: u8, rank: u8) -> Result<Position, String> {
        let f = Self::shogi_to_internal_file(file)?;
        let r = Self::shogi_to_internal_rank(rank)?;
        Position::new(f, r).ok_or_else(|| "Invalid position".to_string())
    }

    pub fn format_move_public(mv: &Move) -> String {
        Self::format_move(mv)
    }

    pub fn find_matching_moves_pub(
        &self,
        from: Position,
        to: Position,
        promote: Option<bool>,
    ) -> Vec<Move> {
        self.find_matching_moves(from, to, promote)
    }

    pub fn apply_live_move_pub(&mut self, mv: Move) -> Result<String, String> {
        self.apply_live_move(mv)
    }

    pub fn list_games_pub(&self) -> Result<Vec<String>, String> {
        self.game_history.list_games()
    }

    pub fn save_current(&self, filename: Option<&str>) -> Result<String, String> {
        let mut warning = String::new();
        if self.setup.is_some() {
            warning = " (warning: edited setup — branch moves only)".to_string();
        }
        let record = self.build_save_record();
        let path = self.game_history.save_game(&record, filename)?;
        Ok(format!("{}{}", path, warning))
    }

    /// Start a fresh game from the initial setup (clears trunk/branch).
    pub fn new_game(&mut self) {
        *self = DebugTool::new();
    }

    pub fn run(&mut self) {
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        println!("Taikyoku Shogi Debug Tool");
        println!("Type 'help' for commands, 'quit' to exit");

        loop {
            print!("debug> ");
            let _ = stdout.flush();

            let mut line = String::new();
            if stdin.lock().read_line(&mut line).is_err() || line.is_empty() {
                break;
            }

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            match self.process_command(line) {
                Ok(Some(output)) => {
                    if output == "__QUIT__" {
                        break;
                    }
                    if !output.is_empty() {
                        println!("{}", output);
                    }
                }
                Ok(None) => {}
                Err(e) => println!("Error: {}", e),
            }
        }
    }

    fn process_command(&mut self, command: &str) -> Result<Option<String>, String> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(Some(String::new()));
        }

        match parts[0] {
            "help" => Ok(Some(self.help_text())),
            "quit" | "exit" => Ok(Some("__QUIT__".to_string())),

            "load" => {
                if parts.len() < 2 {
                    return Err("Usage: load <filename>".to_string());
                }
                self.load_game(parts[1])?;
                Ok(Some(format!(
                    "Loaded {}. Timeline length: {} plies. At ply 0.",
                    parts[1],
                    self.timeline_len()
                )))
            }

            "forward" | "f" => {
                let n = if parts.len() > 1 {
                    parts[1].parse().map_err(|_| "Invalid number".to_string())?
                } else {
                    1usize
                };
                self.forward(n)?;
                Ok(Some(format!(
                    "Forward {}. Now at ply {} / {}",
                    n,
                    self.cursor,
                    self.timeline_len()
                )))
            }

            "back" | "b" => {
                let n = if parts.len() > 1 {
                    parts[1].parse().map_err(|_| "Invalid number".to_string())?
                } else {
                    1usize
                };
                self.back(n)?;
                Ok(Some(format!(
                    "Back {}. Now at ply {} / {}",
                    n,
                    self.cursor,
                    self.timeline_len()
                )))
            }

            "goto" | "g" => {
                if parts.len() < 2 {
                    return Err("Usage: goto <ply>".to_string());
                }
                let ply: usize = parts[1].parse().map_err(|_| "Invalid ply".to_string())?;
                self.goto_move(ply)?;
                Ok(Some(format!(
                    "At ply {} / {}",
                    self.cursor,
                    self.timeline_len()
                )))
            }

            "info" => {
                let trunk_info = if let Some(t) = &self.trunk {
                    format!(
                        "Trunk: {} move records (branch_index={})",
                        t.moves.len(),
                        self.branch_index
                    )
                } else {
                    "Trunk: none".to_string()
                };
                Ok(Some(format!(
                    "{}\n{}\nSetup: {}\nBranch moves: {}",
                    self.status_text(),
                    trunk_info,
                    if self.setup.is_some() { "yes" } else { "no" },
                    self.branch.len()
                )))
            }

            "status" => Ok(Some(self.status_text())),

            "board" => Ok(Some(self.game_history.format_board(&self.game_state))),

            "pieces" => {
                let color = if parts.len() > 1 {
                    match parts[1] {
                        "black" => Some(Color::Black),
                        "white" => Some(Color::White),
                        _ => return Err("Invalid color. Use 'black' or 'white'".to_string()),
                    }
                } else {
                    None
                };
                let board = self.game_state.get_board();
                let pieces = match color {
                    Some(c) => board.get_pieces_by_color(c),
                    None => {
                        let mut all = board.get_pieces_by_color(Color::Black);
                        all.extend(board.get_pieces_by_color(Color::White));
                        all
                    }
                };
                let mut output = String::new();
                for piece in pieces {
                    let f = Self::internal_to_shogi_file(piece.position.file);
                    let r = Self::internal_to_shogi_rank(piece.position.rank);
                    output.push_str(&format!(
                        "{:?} {:?} at {},{}{}\n",
                        piece.color,
                        piece.piece_type,
                        f,
                        r,
                        if piece.is_promoted { " (promoted)" } else { "" }
                    ));
                }
                Ok(Some(output))
            }

            "piece" => {
                if parts.len() < 3 {
                    return Err("Usage: piece <file> <rank>".to_string());
                }
                let pos = Self::parse_shogi_pos(parts[1], parts[2])?;
                let f: u8 = parts[1].parse().unwrap();
                let r: u8 = parts[2].parse().unwrap();
                if let Some(piece) = self.game_state.get_board().get_piece(pos) {
                    Ok(Some(format!(
                        "{:?} {:?} at {},{}{}",
                        piece.color,
                        piece.piece_type,
                        f,
                        r,
                        if piece.is_promoted { " (promoted)" } else { "" }
                    )))
                } else {
                    Ok(Some(format!("No piece at {},{}", f, r)))
                }
            }

            "moves" => {
                if parts.len() >= 3 {
                    let pos = Self::parse_shogi_pos(parts[1], parts[2])?;
                    let moves = if let Some(piece) = self.game_state.get_board().get_piece(pos) {
                        if piece.color == self.game_state.get_current_turn() {
                            self.game_state.generate_legal_moves_for_pieces(&[piece])
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    };
                    let mut output = format!(
                        "Legal moves from {},{} ({}):\n",
                        parts[1],
                        parts[2],
                        moves.len()
                    );
                    for (i, mv) in moves.iter().enumerate() {
                        output.push_str(&format!("  [{}] {}\n", i, Self::format_move(mv)));
                    }
                    Ok(Some(output))
                } else {
                    let moves = self.game_state.generate_legal_moves();
                    let mut output = format!("All legal moves ({}):\n", moves.len());
                    for (i, mv) in moves.iter().enumerate() {
                        output.push_str(&format!("  [{}] {}\n", i, Self::format_move(mv)));
                    }
                    Ok(Some(output))
                }
            }

            "check" => {
                let color = if parts.len() > 1 {
                    match parts[1] {
                        "black" => Color::Black,
                        "white" => Color::White,
                        _ => self.game_state.get_current_turn(),
                    }
                } else {
                    self.game_state.get_current_turn()
                };
                Ok(Some(format!(
                    "{:?} is {} check",
                    color,
                    if self.is_in_check(color) {
                        "in"
                    } else {
                        "not in"
                    }
                )))
            }

            "attacked" => {
                if parts.len() < 3 {
                    return Err(
                        "Usage: attacked <file> <rank> [black|white]".to_string(),
                    );
                }
                let pos = Self::parse_shogi_pos(parts[1], parts[2])?;
                let by_color = if parts.len() > 3 {
                    match parts[3] {
                        "black" => Some(Color::Black),
                        "white" => Some(Color::White),
                        _ => {
                            return Err("Color must be black or white".to_string());
                        }
                    }
                } else {
                    None
                };
                let attackers = self.get_attackers(pos, by_color);
                let mut output = format!(
                    "Pieces attacking {},{}:\n",
                    parts[1], parts[2]
                );
                if attackers.is_empty() {
                    output.push_str("  (none)");
                } else {
                    for a in attackers {
                        let af = Self::internal_to_shogi_file(a.position.file);
                        let ar = Self::internal_to_shogi_rank(a.position.rank);
                        output.push_str(&format!(
                            "  {:?} {:?} at {},{}\n",
                            a.color, a.piece_type, af, ar
                        ));
                    }
                }
                Ok(Some(output))
            }

            "turn" => {
                if parts.len() < 2 {
                    return Err("Usage: turn black|white".to_string());
                }
                let turn = match parts[1] {
                    "black" => Color::Black,
                    "white" => Color::White,
                    _ => return Err("Use black or white".to_string()),
                };
                self.game_state.set_current_turn(turn);
                self.diverge_via_edit();
                Ok(Some(format!(
                    "Turn set to {:?}. Session branched from edited setup.",
                    turn
                )))
            }

            "remove" => {
                if parts.len() < 3 {
                    return Err("Usage: remove <file> <rank>".to_string());
                }
                let pos = Self::parse_shogi_pos(parts[1], parts[2])?;
                match self.game_state.remove_piece(pos) {
                    Some(piece) => {
                        self.diverge_via_edit();
                        Ok(Some(format!(
                            "Removed {:?} {:?}. Session branched from edited setup.",
                            piece.color, piece.piece_type
                        )))
                    }
                    None => Err("No piece at that square".to_string()),
                }
            }

            "place" => {
                // place <color> <PieceType> <file> <rank> [promoted]
                if parts.len() < 5 {
                    return Err(
                        "Usage: place <black|white> <PieceType> <file> <rank> [promoted]"
                            .to_string(),
                    );
                }
                let color = match parts[1] {
                    "black" => Color::Black,
                    "white" => Color::White,
                    _ => return Err("Color must be black or white".to_string()),
                };
                let piece_type = PieceType::from_name(parts[2]).ok_or_else(|| {
                    format!(
                        "Unknown PieceType '{}'. Use serde names e.g. King, Pawn, FreeEagle",
                        parts[2]
                    )
                })?;
                let pos = Self::parse_shogi_pos(parts[3], parts[4])?;
                let promoted = parts.get(5).map(|s| *s == "promoted").unwrap_or(false);
                let mut piece = Piece::new(piece_type, color, pos);
                if promoted {
                    piece.promote();
                    if !piece.is_promoted {
                        // Force flag if type has no promotion chain but user asked
                        piece.is_promoted = true;
                    }
                }
                self.game_state.place_piece(piece);
                self.diverge_via_edit();
                Ok(Some(format!(
                    "Placed {:?} {:?} at {},{}. Session branched from edited setup.",
                    color, piece_type, parts[3], parts[4]
                )))
            }

            "clear" => {
                self.game_state.clear_board();
                self.diverge_via_edit();
                Ok(Some(
                    "Board cleared. Session branched from edited setup.".to_string(),
                ))
            }

            "reset" => {
                self.setup = None;
                self.branch.clear();
                if let Some(trunk) = &self.trunk {
                    self.branch_index = trunk.moves.len();
                } else {
                    self.branch_index = 0;
                    self.trunk = None;
                }
                self.rebuild_to(0)?;
                Ok(Some(
                    "Reset to start of trunk (or initial position).".to_string(),
                ))
            }

            "move" => {
                // move <ff> <fr> <tf> <tr> [promote|n|path_index]
                if parts.len() < 5 {
                    return Err(
                        "Usage: move <from_file> <from_rank> <to_file> <to_rank> [promote|path_index]"
                            .to_string(),
                    );
                }
                let from = Self::parse_shogi_pos(parts[1], parts[2])?;
                let to = Self::parse_shogi_pos(parts[3], parts[4])?;
                let mut promote: Option<bool> = None;
                let mut path_index: Option<usize> = None;
                if let Some(extra) = parts.get(5) {
                    if *extra == "promote" || *extra == "+" {
                        promote = Some(true);
                    } else if *extra == "n" || *extra == "nopromote" {
                        promote = Some(false);
                    } else if let Ok(i) = extra.parse::<usize>() {
                        path_index = Some(i);
                    } else {
                        return Err(
                            "Optional arg must be promote, n, or a path index".to_string(),
                        );
                    }
                }
                if let Some(extra) = parts.get(6) {
                    if let Ok(i) = extra.parse::<usize>() {
                        path_index = Some(i);
                    }
                }

                let matches = self.find_matching_moves(from, to, promote);
                if matches.is_empty() {
                    return Err("No legal move matches those squares".to_string());
                }
                let chosen = if matches.len() == 1 {
                    matches[0].clone()
                } else if let Some(i) = path_index {
                    matches.get(i).cloned().ok_or_else(|| {
                        format!(
                            "Path index {} out of range (0..{})",
                            i,
                            matches.len() - 1
                        )
                    })?
                } else {
                    let mut msg = format!(
                        "{} matching moves; re-run with path index:\n",
                        matches.len()
                    );
                    for (i, mv) in matches.iter().enumerate() {
                        msg.push_str(&format!("  [{}] {}\n", i, Self::format_move(mv)));
                    }
                    return Err(msg);
                };
                Ok(Some(self.apply_live_move(chosen)?))
            }

            "suggest" => {
                let name = parts.get(1).copied().unwrap_or("mi");
                let player = player_by_name(name)?;
                match player.choose_move(&self.game_state) {
                    Some(mv) => Ok(Some(format!(
                        "{} suggests: {}",
                        player.name(),
                        Self::format_move(&mv)
                    ))),
                    None => Ok(Some(format!("{} has no legal moves", player.name()))),
                }
            }

            "play" => {
                let name = parts.get(1).copied().unwrap_or("mi");
                let player = player_by_name(name)?;
                let pname = player.name();
                match player.choose_move(&self.game_state) {
                    Some(mv) => {
                        let msg = self.apply_live_move(mv)?;
                        Ok(Some(format!("{}: {}", pname, msg)))
                    }
                    None => Err(format!("{} has no legal moves", pname)),
                }
            }

            "save" => {
                let filename = parts.get(1).copied();
                // If setup exists, saved game is only the branch moves from an edited
                // position — note that load expects full-game-from-initial. For MVP we
                // still save the effective move list; if setup-only, warn the user.
                let mut warning = String::new();
                if self.setup.is_some() {
                    warning = "\nNote: session has an edited setup; saved file contains branch moves only and may not replay from the standard initial position.".to_string();
                }
                let record = self.build_save_record();
                let path = self.game_history.save_game(&record, filename)?;
                Ok(Some(format!("Saved to {}{}", path, warning)))
            }

            _ => Err(format!(
                "Unknown command: {}. Type 'help' for available commands",
                parts[0]
            )),
        }
    }

    fn help_text(&self) -> String {
        r#"Available commands (plies = MoveRecords in the effective timeline):

  Navigation:
    load <file>           - Load game JSON from games/
    forward [n] / f [n]   - Move forward n plies (default 1)
    back [n] / b [n]      - Move backward n plies (default 1)
    goto <n> / g <n>      - Jump to ply n (0 = start)
    info / status         - Session and position summary

  Display / engine:
    board                 - Show board
    pieces [black|white]  - List pieces
    piece <f> <r>         - Piece at shogi square
    moves [<f> <r>]       - Legal moves (all or from square)
    check [black|white]   - Royal check (King/CrownPrince)
    attacked <f> <r> [black|white] - Attackers of a square

  Edit (branches from current position via setup snapshot):
    turn black|white
    remove <f> <r>
    place <black|white> <PieceType> <f> <r> [promoted]
    clear                 - Empty the board
    reset                 - Back to trunk start / initial

  Branch / agents:
    move <ff> <fr> <tf> <tr> [promote|n] [path_index]
    suggest [mi|random|royal] - Show agent choice
    play [mi|random|royal]    - Apply agent choice
    save [filename]           - Write effective timeline JSON

  Utility:
    help / quit

Coordinates: shogi-style file/rank 1-36 (file 1=rightmost, rank 1=top).
PieceType names: King, Pawn, FreeEagle, ... (serde/Debug names).
"#
        .to_string()
    }
}

impl Default for DebugTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rebuild_to_idempotent_on_initial() {
        let mut tool = DebugTool::new();
        tool.rebuild_to(0).unwrap();
        let board1 = tool.game_state.clone_board();
        tool.rebuild_to(0).unwrap();
        let board2 = tool.game_state.clone_board();
        assert_eq!(
            board1.get_pieces_by_color(Color::Black).len(),
            board2.get_pieces_by_color(Color::Black).len()
        );
        assert_eq!(tool.cursor, 0);
    }

    #[test]
    fn test_play_mi_extends_branch() {
        let mut tool = DebugTool::new();
        assert_eq!(tool.timeline_len(), 0);
        let out = tool.process_command("play mi").unwrap().unwrap();
        assert!(out.contains("Applied") || out.contains("mi:"));
        assert_eq!(tool.cursor, 1);
        assert_eq!(tool.branch.len(), 1);
        tool.back(1).unwrap();
        assert_eq!(tool.cursor, 0);
        tool.forward(1).unwrap();
        assert_eq!(tool.cursor, 1);
    }

    #[test]
    fn test_successive_ai_moves_accumulate_from_current_position() {
        let mut tool = DebugTool::new();
        tool.play_agent("mi").unwrap();
        tool.play_agent("mi").unwrap();
        tool.play_agent("mi").unwrap();
        assert_eq!(tool.cursor_ply(), 3);
        assert_eq!(tool.branch.len(), 3);
        assert_eq!(tool.timeline_length(), 3);

        // Board should reflect all three moves, not only the last from the start.
        let black = tool
            .game_state_ref()
            .get_board()
            .get_pieces_by_color(Color::Black)
            .len();
        let white = tool
            .game_state_ref()
            .get_board()
            .get_pieces_by_color(Color::White)
            .len();
        assert!(black + white > 0);

        // Rebuild from scratch to the same ply must match live piece counts.
        let live_black = black;
        tool.rebuild_to(3).unwrap();
        assert_eq!(
            tool.game_state_ref()
                .get_board()
                .get_pieces_by_color(Color::Black)
                .len(),
            live_black
        );
    }

    #[test]
    fn test_ai_move_after_scrub_into_loaded_prefix() {
        let mut tool = DebugTool::new();
        // Synthesize a short trunk by playing then saving via branch as trunk-like state:
        tool.play_agent("mi").unwrap();
        tool.play_agent("mi").unwrap();
        tool.play_agent("mi").unwrap();
        tool.play_agent("mi").unwrap();
        assert_eq!(tool.cursor_ply(), 4);

        // Scrub back to ply 2 and play from there — should diverge and continue.
        tool.goto_move(2).unwrap();
        assert_eq!(tool.cursor_ply(), 2);
        tool.play_agent("mi").unwrap();
        assert_eq!(tool.cursor_ply(), 3);
        assert_eq!(tool.timeline_length(), 3);
    }

    #[test]
    fn test_gamestate_clone_round_trip() {
        let mut state = GameState::new();
        state.setup_initial_position();
        let cloned = state.clone();
        assert_eq!(
            state.get_board().get_pieces_by_color(Color::Black).len(),
            cloned.get_board().get_pieces_by_color(Color::Black).len()
        );
        assert_eq!(state.get_current_turn(), cloned.get_current_turn());
    }
}
