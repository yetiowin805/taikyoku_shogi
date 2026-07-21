use crate::game_history::{GameHistory, GameRecord, MoveRecord, MoveRecordData};
use crate::game_state::{GameState, Move};
use crate::move_simulation::MoveDelta;
use crate::piece::{Color, Piece, PieceType};
use crate::position::Position;
use std::io::{self, BufRead, Write};

pub struct DebugTool {
    // Game data
    game_record: Option<GameRecord>,
    current_move_index: usize,  // 0 = initial, 1 = after move 1, etc.
    
    // Current position state
    game_state: GameState,  // Current position (either from game or modified)
    is_modified: bool,  // Whether current state differs from game state
    
    // Undo/redo stacks for navigation
    undo_stack: Vec<MoveDelta>,  // Moves we can undo
    redo_stack: Vec<MoveDelta>,  // Moves we can redo
    
    // Cached game state at current_move_index (for reset)
    cached_game_state: Option<GameState>,
    
    // Game history for loading games
    game_history: GameHistory,
}

impl DebugTool {
    pub fn new() -> Self {
        let mut game_state = GameState::new();
        game_state.setup_initial_position();
        
        DebugTool {
            game_record: None,
            current_move_index: 0,
            game_state,
            is_modified: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            cached_game_state: None,
            game_history: GameHistory::new("games"),
        }
    }
    
    /// Load a game from JSON file
    pub fn load_game(&mut self, filename: &str) -> Result<(), String> {
        let game_record = self.game_history.load_game(filename)?;
        
        // Reset to initial position
        self.game_state = GameState::new();
        self.game_state.setup_initial_position();
        self.current_move_index = 0;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.is_modified = false;
        self.cached_game_state = Some(self.clone_game_state());
        self.game_record = Some(game_record);
        
        Ok(())
    }
    
    /// Create a copy of the current game state
    fn clone_game_state(&self) -> GameState {
        // Create new GameState and copy board
        let mut new_state = GameState::new();
        let board = self.game_state.clone_board();
        
        // Place all pieces from the board
        for piece in board.get_pieces_by_color(Color::Black) {
            new_state.place_piece(piece);
        }
        for piece in board.get_pieces_by_color(Color::White) {
            new_state.place_piece(piece);
        }
        
        // Set turn
        new_state.set_current_turn(self.game_state.get_current_turn());
        
        new_state
    }
    
    /// Show current board position
    pub fn show_board(&self) -> String {
        self.game_history.format_board(&self.game_state)
    }
    
    /// List all pieces, optionally filtered by color
    pub fn list_pieces(&self, color: Option<Color>) -> Vec<Piece> {
        let board = self.game_state.get_board();
        if let Some(c) = color {
            board.get_pieces_by_color(c)
        } else {
            let mut all_pieces = board.get_pieces_by_color(Color::Black);
            all_pieces.extend(board.get_pieces_by_color(Color::White));
            all_pieces
        }
    }
    
    /// Get legal moves, optionally for a specific piece
    pub fn get_legal_moves(&self, position: Option<Position>) -> Vec<Move> {
        if let Some(pos) = position {
            // Get moves for specific piece
            if let Some(piece) = self.game_state.get_board().get_piece(pos) {
                if piece.color == self.game_state.get_current_turn() {
                    let pieces = vec![piece];
                    self.game_state.generate_legal_moves_for_pieces(&pieces)
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            // Get all legal moves for current player
            self.game_state.generate_legal_moves()
        }
    }
    
    /// Check if a player is in check
    pub fn is_in_check(&self, color: Color) -> bool {
        let board = self.game_state.get_board();
        let pieces = board.get_pieces_by_color(color);
        
        // Find royal pieces (King, CrownPrince, GreatGeneral)
        for piece in pieces {
            if Self::is_royal_piece(piece.piece_type) {
                if board.is_position_attacked_by_color_for_check(piece.position, color.opposite()) {
                    return true;
                }
            }
        }
        
        false
    }
    
    /// Get all pieces attacking a position
    pub fn get_attackers(&self, position: Position) -> Vec<Piece> {
        let board = self.game_state.get_board();
        let attacker_color = self.game_state.get_current_turn().opposite();
        let all_attackers = board.get_pieces_by_color(attacker_color);
        
        let mut attackers = Vec::new();
        for piece in all_attackers {
            if piece.can_reach_boardlike(position, board) {
                attackers.push(piece);
            }
        }
        
        attackers
    }
    
    /// Check if a piece type is royal (King, CrownPrince, GreatGeneral)
    fn is_royal_piece(piece_type: PieceType) -> bool {
        matches!(piece_type, 
            PieceType::King | 
            PieceType::CrownPrince | 
            PieceType::GreatGeneral
        )
    }
    
    /// Convert shogi-style file (1-36, rightmost=1) to internal file (0-35, leftmost=0)
    fn shogi_to_internal_file(file: u8) -> Result<u8, String> {
        if file < 1 || file > 36 {
            return Err(format!("File must be between 1 and 36, got {}", file));
        }
        Ok(36 - file)
    }
    
    /// Convert shogi-style rank (1-36, top=1) to internal rank (0-35, bottom=0)
    fn shogi_to_internal_rank(rank: u8) -> Result<u8, String> {
        if rank < 1 || rank > 36 {
            return Err(format!("Rank must be between 1 and 36, got {}", rank));
        }
        Ok(36 - rank)
    }
    
    /// Convert internal file (0-35, leftmost=0) to shogi-style file (1-36, rightmost=1)
    fn internal_to_shogi_file(file: u8) -> u8 {
        36 - file
    }
    
    /// Convert internal rank (0-35, bottom=0) to shogi-style rank (1-36, top=1)
    fn internal_to_shogi_rank(rank: u8) -> u8 {
        36 - rank
    }
    
    /// Count how many displayed moves a MoveRecord takes up
    fn count_displayed_moves_for_record(&self, mv: &MoveRecord) -> usize {
        match &mv.data {
            MoveRecordData::TwoStep { .. } => {
                // Two-step moves are displayed as 2 moves
                2
            }
            MoveRecordData::FreeEagle { path } => {
                // Analyze Free Eagle path to determine if it's displayed as multiple moves
                if path.len() < 2 {
                    return 1;
                }
                
                // Reconstruct board state at this move to analyze path
                // We need to check if the path has captures or direction changes
                let analysis = self.analyze_free_eagle_path(path);
                if analysis.0 {
                    // Displayed as multiple moves - count positions to display
                    let mut positions_to_display = std::collections::HashSet::new();
                    for capture_pos in &analysis.1 {
                        positions_to_display.insert(*capture_pos);
                    }
                    for idx in &analysis.2 {
                        if *idx < path.len() {
                            positions_to_display.insert(path[*idx]);
                        }
                    }
                    // Always show final position
                    if let Some(to) = path.last() {
                        positions_to_display.insert(*to);
                    }
                    positions_to_display.len().max(1)
                } else {
                    // Displayed as single move
                    1
                }
            }
            MoveRecordData::Standard => {
                // Regular moves are displayed as 1 move
                1
            }
        }
    }
    
    /// Analyze Free Eagle path to determine display characteristics
    /// Returns (should_display_as_multiple, capture_positions, direction_change_indices)
    fn analyze_free_eagle_path(&self, path: &[Position]) -> (bool, Vec<Position>, Vec<usize>) {
        if path.len() < 2 {
            return (false, Vec::new(), Vec::new());
        }

        let mut capture_positions = Vec::new();
        let mut direction_change_indices = Vec::new();
        let mut prev_direction: Option<(i8, i8)> = None;
        let returns_to_origin = path[0] == path[path.len() - 1];

        // Check each step in the path
        for i in 1..path.len() {
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

        // Should display as multiple if: returns to origin, has direction changes
        // Note: We can't check captures without board state, but direction changes are enough
        let should_display_as_multiple = returns_to_origin 
            || !direction_change_indices.is_empty();

        (should_display_as_multiple, capture_positions, direction_change_indices)
    }
    
    /// Compute displayed move number for a given MoveRecord index
    /// Returns the displayed move number (1-indexed, where 1 is after the first move)
    fn compute_displayed_move_number(&self, move_record_index: usize) -> Result<usize, String> {
        let game_record = self.game_record.as_ref()
            .ok_or_else(|| "No game loaded".to_string())?;
        
        if move_record_index == 0 {
            return Ok(0); // Initial position
        }
        
        if move_record_index > game_record.moves.len() {
            return Err(format!("Move record index {} is out of range (max: {})", 
                move_record_index, game_record.moves.len()));
        }
        
        let mut displayed_count = 0;
        for i in 0..move_record_index {
            let mv = &game_record.moves[i];
            displayed_count += self.count_displayed_moves_for_record(mv);
        }
        
        Ok(displayed_count)
    }
    
    /// Find MoveRecord index from displayed move number
    fn find_move_record_index(&self, displayed_move_num: usize) -> Result<usize, String> {
        let game_record = self.game_record.as_ref()
            .ok_or_else(|| "No game loaded".to_string())?;
        
        if displayed_move_num == 0 {
            return Ok(0);
        }
        
        let mut displayed_count = 0;
        for (i, mv) in game_record.moves.iter().enumerate() {
            let moves_for_this_record = self.count_displayed_moves_for_record(mv);
            if displayed_count + moves_for_this_record >= displayed_move_num {
                return Ok(i + 1); // +1 because we want the index after applying this move
            }
            displayed_count += moves_for_this_record;
        }
        
        Err(format!("Displayed move number {} is out of range (max: {})", 
            displayed_move_num, displayed_count))
    }
    
    /// Apply a move and compute the delta for undo
    fn apply_move_with_delta(&mut self, mv: Move) -> Result<MoveDelta, String> {
        // Snapshot state before move - get copies of pieces
        let from = mv.from;
        let to = mv.to;
        let promoted = mv.promoted;
        
        let piece_before = self.game_state.get_board().get_piece(from);
        let piece_at_dest_before = self.game_state.get_board().get_piece(to);
        
        // Get path positions for capturing range moves
        let path_positions = crate::path_utils::get_path_positions(from, to);
        let mut pieces_in_path_before = Vec::new();
        for pos in &path_positions {
            if *pos != from && *pos != to {
                if let Some(piece) = self.game_state.get_board().get_piece(*pos) {
                    pieces_in_path_before.push((*pos, piece));
                }
            }
        }
        
        // Track promotion state and make copies
        let old_piece_type = piece_before.map(|p| p.piece_type);
        let piece_before_copy = piece_before;
        let piece_at_dest_before_copy = piece_at_dest_before;
        
        // Apply the move
        let turn_before = self.game_state.get_current_turn();
        let move_result = self.game_state.make_move(mv);
        let turn_after = self.game_state.get_current_turn();
        
        // Check if move succeeded
        let move_succeeded = move_result.is_some() || (turn_before != turn_after);
        if !move_succeeded {
            return Err("Move failed".to_string());
        }
        
        // Build delta
        let mut delta = MoveDelta::new();
        
        // Track piece movement
        if let Some(piece) = piece_before_copy {
            let piece_after = self.game_state.get_board().get_piece(to);
            if let Some(p) = piece_after {
                delta.piece_moved = Some((from, to, piece));
                
                // Track promotion
                if promoted {
                    if let Some(old_type) = old_piece_type {
                        delta.piece_promoted = Some((to, old_type, p.piece_type));
                    }
                }
            }
        }
        
        // Track pieces removed (captures in path and at destination)
        for (pos, piece) in pieces_in_path_before {
            if self.game_state.get_board().get_piece(pos).is_none() {
                delta.pieces_removed.push((pos, piece));
            }
        }
        
        if let Some(captured) = piece_at_dest_before_copy {
            // Check if it was actually captured (not the same piece that moved)
            if captured.position != from {
                delta.pieces_removed.push((to, captured));
            }
        }
        
        Ok(delta)
    }
    
    /// Undo a move delta (reverse the changes)
    fn undo_delta(&mut self, delta: &MoveDelta) -> Result<(), String> {
        let board = self.game_state.get_board_mut();
        
        // Reverse promotion first
        if let Some((pos, old_type, _)) = delta.piece_promoted {
            if let Some(mut piece) = board.get_piece(pos) {
                // Demote the piece - restore to old type
                piece.piece_type = old_type;
                piece.is_promoted = false;
                board.remove_piece(pos);
                board.place_piece(piece);
            }
        }
        
        // Restore removed pieces (in reverse order to maintain correct state)
        for (pos, piece) in delta.pieces_removed.iter().rev() {
            board.place_piece(*piece);
        }
        
        // Move piece back
        if let Some((from, to, original_piece)) = delta.piece_moved {
            // Remove piece from destination
            board.remove_piece(to);
            // Place it back at original position
            board.place_piece(original_piece);
        }
        
        // Reverse turn
        self.game_state.set_current_turn(self.game_state.get_current_turn().opposite());
        
        Ok(())
    }
    
    /// Convert MoveRecord to Move
    fn move_record_to_move(mv: &MoveRecord) -> Result<Move, String> {
        let from = Position::new(mv.from_file, mv.from_rank)
            .ok_or_else(|| format!("Invalid from position: ({}, {})", mv.from_file, mv.from_rank))?;
        let to = Position::new(mv.to_file, mv.to_rank)
            .ok_or_else(|| format!("Invalid to position: ({}, {})", mv.to_file, mv.to_rank))?;
        
        let move_obj = match &mv.data {
            MoveRecordData::Standard => {
                Move::new_with_promotion(from, to, mv.promoted)
            }
            MoveRecordData::TwoStep { intermediate_file, intermediate_rank } => {
                let intermediate = Position::new(*intermediate_file, *intermediate_rank)
                    .ok_or_else(|| format!("Invalid intermediate position: ({}, {})", intermediate_file, intermediate_rank))?;
                Move::new_two_step_with_promotion(from, intermediate, to, mv.promoted)
            }
            MoveRecordData::FreeEagle { path } => {
                Move::new_free_eagle(from, to, path.clone())
            }
        };
        
        Ok(move_obj)
    }
    
    /// Move forward n displayed moves
    pub fn forward(&mut self, n: usize) -> Result<(), String> {
        // Extract move records to apply first to avoid borrow checker issues
        let move_records_to_apply: Vec<MoveRecord> = {
            let game_record = self.game_record.as_ref()
                .ok_or_else(|| "No game loaded".to_string())?;
            
            // Convert displayed move count to MoveRecord count
            // We need to find how many MoveRecords to apply to get n displayed moves
            let mut displayed_count = 0;
            let mut move_records_to_apply = Vec::new();
            
            for i in self.current_move_index..game_record.moves.len() {
                if displayed_count >= n {
                    break;
                }
                let mv = &game_record.moves[i];
                displayed_count += self.count_displayed_moves_for_record(mv);
                move_records_to_apply.push(mv.clone());
            }
            
            if displayed_count < n {
                let current_displayed = self.compute_displayed_move_number(self.current_move_index)?;
                let max_displayed = self.compute_displayed_move_number(game_record.moves.len())?;
                return Err(format!("Cannot move forward {} displayed moves: only {} available (at move {})", 
                    n, max_displayed - current_displayed, current_displayed));
            }
            
            move_records_to_apply
        };
        
        // Apply the move records
        for move_record in move_records_to_apply {
            let mv = Self::move_record_to_move(&move_record)?;
            
            // Set turn to match move record
            self.game_state.set_current_turn(move_record.color);
            
            let delta = self.apply_move_with_delta(mv)?;
            self.undo_stack.push(delta);
            self.redo_stack.clear();
            self.current_move_index += 1;
        }
        
        // Cache state if at game position
        if !self.is_modified {
            self.cached_game_state = Some(self.clone_game_state());
        }
        
        Ok(())
    }
    
    /// Move backward n moves
    pub fn back(&mut self, n: usize) -> Result<(), String> {
        if n > self.undo_stack.len() {
            return Err(format!("Cannot move back {} moves: only {} moves in undo stack", 
                n, self.undo_stack.len()));
        }
        
        for _ in 0..n {
            if let Some(delta) = self.undo_stack.pop() {
                self.undo_delta(&delta)?;
                // Store delta in redo stack for potential redo
                self.redo_stack.push(delta);
                self.current_move_index -= 1;
            } else {
                break;
            }
        }
        
        // Cache state if at game position
        if !self.is_modified {
            self.cached_game_state = Some(self.clone_game_state());
        }
        
        Ok(())
    }
    
    /// Jump to a specific displayed move number
    pub fn goto_move(&mut self, displayed_move_num: usize) -> Result<(), String> {
        // Convert displayed move number to MoveRecord index
        let target_move_record_index = self.find_move_record_index(displayed_move_num)?;
        
        // If jumping far, rebuild from initial
        let distance = if target_move_record_index > self.current_move_index {
            target_move_record_index - self.current_move_index
        } else {
            self.current_move_index - target_move_record_index
        };
        
        if distance > 10 {
            // Rebuild from initial
            self.rebuild_to_move_index(target_move_record_index)?;
        } else {
            // Use undo/redo for small jumps
            if target_move_record_index > self.current_move_index {
                let current_displayed = self.compute_displayed_move_number(self.current_move_index)?;
                self.forward(displayed_move_num - current_displayed)?;
            } else if target_move_record_index < self.current_move_index {
                let current_displayed = self.compute_displayed_move_number(self.current_move_index)?;
                self.back(current_displayed - displayed_move_num)?;
            }
        }
        
        Ok(())
    }
    
    /// Rebuild position to a specific move index
    fn rebuild_to_move_index(&mut self, target_index: usize) -> Result<(), String> {
        // Reset to initial position
        self.game_state = GameState::new();
        self.game_state.setup_initial_position();
        self.current_move_index = 0;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.is_modified = false;
        
        // Apply moves up to target
        if target_index > 0 {
            self.forward(target_index)?;
        } else {
            self.cached_game_state = Some(self.clone_game_state());
        }
        
        Ok(())
    }
    
    /// Run the REPL loop
    pub fn run(&mut self) {
        let stdin = io::stdin();
        let mut stdout = io::stdout();
        
        println!("Taikyoku Shogi Debug Tool");
        println!("Type 'help' for commands, 'quit' to exit");
        
        loop {
            print!("debug> ");
            stdout.flush().unwrap();
            
            let mut line = String::new();
            if stdin.lock().read_line(&mut line).is_err() || line.is_empty() {
                break;
            }
            
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            
            match self.process_command(line) {
                Ok(output) => {
                    if !output.is_empty() {
                        println!("{}", output);
                    }
                }
                Err(e) => {
                    println!("Error: {}", e);
                }
            }
        }
    }
    
    /// Process a command and return output or error
    fn process_command(&mut self, command: &str) -> Result<String, String> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(String::new());
        }
        
        match parts[0] {
            "help" => Ok(self.help_text()),
            "quit" | "exit" => {
                std::process::exit(0);
            }
            "load" => {
                if parts.len() < 2 {
                    return Err("Usage: load <filename>".to_string());
                }
                self.load_game(parts[1])?;
                Ok(format!("Loaded game: {}", parts[1]))
            }
            "forward" | "f" => {
                let n = if parts.len() > 1 {
                    parts[1].parse().map_err(|_| "Invalid number".to_string())?
                } else {
                    1
                };
                self.forward(n)?;
                let displayed_move = self.compute_displayed_move_number(self.current_move_index)?;
                Ok(format!("Moved forward {} displayed move(s). Now at displayed move {}", n, displayed_move))
            }
            "back" | "b" => {
                let n = if parts.len() > 1 {
                    parts[1].parse().map_err(|_| "Invalid number".to_string())?
                } else {
                    1
                };
                let current_displayed = self.compute_displayed_move_number(self.current_move_index)?;
                // For back, we need to work in displayed moves, but back() works in MoveRecords
                // We'll need to convert, but for now let's just undo MoveRecords and show the result
                self.back(n)?;
                let displayed_move = self.compute_displayed_move_number(self.current_move_index)?;
                Ok(format!("Moved back {} move record(s). Now at displayed move {}", n, displayed_move))
            }
            "goto" | "g" => {
                if parts.len() < 2 {
                    return Err("Usage: goto <displayed_move_number>".to_string());
                }
                let displayed_move_num = parts[1].parse().map_err(|_| "Invalid move number".to_string())?;
                self.goto_move(displayed_move_num)?;
                Ok(format!("Jumped to displayed move {}", displayed_move_num))
            }
            "info" => {
                let game_info = if let Some(record) = &self.game_record {
                    let max_displayed = self.compute_displayed_move_number(record.moves.len())?;
                    format!("Game loaded: {} move records ({} displayed moves)", record.moves.len(), max_displayed)
                } else {
                    "No game loaded".to_string()
                };
                let displayed_move = self.compute_displayed_move_number(self.current_move_index)?;
                Ok(format!(
                    "Current displayed move: {}\nCurrent move record index: {}\nTurn: {:?}\nUndo stack: {} moves\nRedo stack: {} moves\n{}",
                    displayed_move,
                    self.current_move_index,
                    self.game_state.get_current_turn(),
                    self.undo_stack.len(),
                    self.redo_stack.len(),
                    game_info
                ))
            }
            "board" => Ok(self.show_board()),
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
                let pieces = self.list_pieces(color);
                let mut output = String::new();
                for piece in pieces {
                    let shogi_file = Self::internal_to_shogi_file(piece.position.file);
                    let shogi_rank = Self::internal_to_shogi_rank(piece.position.rank);
                    output.push_str(&format!("{:?} at file {}, rank {}\n", piece.piece_type, shogi_file, shogi_rank));
                }
                Ok(output)
            }
            "piece" => {
                if parts.len() < 3 {
                    return Err("Usage: piece <file> <rank> (shogi-style: 1-36, file 1=rightmost, rank 1=top)".to_string());
                }
                let shogi_file = parts[1].parse().map_err(|_| "Invalid file".to_string())?;
                let shogi_rank = parts[2].parse().map_err(|_| "Invalid rank".to_string())?;
                let file = Self::shogi_to_internal_file(shogi_file)?;
                let rank = Self::shogi_to_internal_rank(shogi_rank)?;
                let pos = Position::new(file, rank)
                    .ok_or_else(|| "Invalid position".to_string())?;
                if let Some(piece) = self.game_state.get_board().get_piece(pos) {
                    Ok(format!("{:?} {:?} at file {}, rank {}", piece.color, piece.piece_type, shogi_file, shogi_rank))
                } else {
                    Ok(format!("No piece at file {}, rank {}", shogi_file, shogi_rank))
                }
            }
            "moves" => {
                if parts.len() >= 3 {
                    let shogi_file = parts[1].parse().map_err(|_| "Invalid file".to_string())?;
                    let shogi_rank = parts[2].parse().map_err(|_| "Invalid rank".to_string())?;
                    let file = Self::shogi_to_internal_file(shogi_file)?;
                    let rank = Self::shogi_to_internal_rank(shogi_rank)?;
                    let pos = Position::new(file, rank)
                        .ok_or_else(|| "Invalid position".to_string())?;
                    let moves = self.get_legal_moves(Some(pos));
                    let mut output = format!("Legal moves for piece at file {}, rank {}:\n", shogi_file, shogi_rank);
                    for mv in moves {
                        let from_file = Self::internal_to_shogi_file(mv.from.file);
                        let from_rank = Self::internal_to_shogi_rank(mv.from.rank);
                        let to_file = Self::internal_to_shogi_file(mv.to.file);
                        let to_rank = Self::internal_to_shogi_rank(mv.to.rank);
                        output.push_str(&format!("  file {}, rank {} -> file {}, rank {}\n", from_file, from_rank, to_file, to_rank));
                    }
                    Ok(output)
                } else {
                    let moves = self.get_legal_moves(None);
                    let mut output = format!("All legal moves ({}):\n", moves.len());
                    for mv in moves {
                        let from_file = Self::internal_to_shogi_file(mv.from.file);
                        let from_rank = Self::internal_to_shogi_rank(mv.from.rank);
                        let to_file = Self::internal_to_shogi_file(mv.to.file);
                        let to_rank = Self::internal_to_shogi_rank(mv.to.rank);
                        output.push_str(&format!("  file {}, rank {} -> file {}, rank {}\n", from_file, from_rank, to_file, to_rank));
                    }
                    Ok(output)
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
                let in_check = self.is_in_check(color);
                Ok(format!("{:?} is {} check", color, if in_check { "in" } else { "not in" }))
            }
            "attacked" => {
                if parts.len() < 3 {
                    return Err("Usage: attacked <file> <rank> (shogi-style: 1-36, file 1=rightmost, rank 1=top)".to_string());
                }
                let shogi_file = parts[1].parse().map_err(|_| "Invalid file".to_string())?;
                let shogi_rank = parts[2].parse().map_err(|_| "Invalid rank".to_string())?;
                let file = Self::shogi_to_internal_file(shogi_file)?;
                let rank = Self::shogi_to_internal_rank(shogi_rank)?;
                let pos = Position::new(file, rank)
                    .ok_or_else(|| "Invalid position".to_string())?;
                let attackers = self.get_attackers(pos);
                let mut output = format!("Pieces attacking file {}, rank {}:\n", shogi_file, shogi_rank);
                if attackers.is_empty() {
                    output.push_str("  (none)");
                } else {
                    for attacker in attackers {
                        let attacker_file = Self::internal_to_shogi_file(attacker.position.file);
                        let attacker_rank = Self::internal_to_shogi_rank(attacker.position.rank);
                        output.push_str(&format!("  {:?} {:?} at file {}, rank {}\n", attacker.color, attacker.piece_type, attacker_file, attacker_rank));
                    }
                }
                Ok(output)
            }
            _ => Err(format!("Unknown command: {}. Type 'help' for available commands", parts[0]))
        }
    }
    
    fn help_text(&self) -> String {
        r#"Available commands:
  Navigation:
    load <file>          - Load game from JSON file
    forward [n] / f [n]  - Move forward n displayed moves (default: 1)
    back [n] / b [n]     - Move backward n move records (default: 1)
    goto <n> / g <n>     - Jump to displayed move number n
    info                 - Show current position info
    
  Display:
    board                - Display current board
    pieces [color]       - List all pieces (optionally filter by color)
    piece <file> <rank>  - Show piece at position (shogi-style: 1-36)
    
  Queries:
    moves [<file> <rank>] - Show legal moves (all or for piece at position)
                           Positions use shogi-style: 1-36 (file 1=rightmost, rank 1=top)
    check [color]        - Check if player is in check (default: current turn)
    attacked <file> <rank> - Show all pieces attacking this position (shogi-style: 1-36)
    
  Utility:
    help                 - Show this help
    quit / exit          - Exit debug tool
    
Note: Positions use shogi-style numbering:
  - Files: 1-36 (1 = rightmost, 36 = leftmost)
  - Ranks: 1-36 (1 = top, 36 = bottom)
"#.to_string()
    }
}

