use crate::move_simulation::{BoardAfterStep, BoardLike};
use crate::movement::direction::{direction_iter, Direction, DirectionSet};
use crate::movement::types::{BlockingMode, MovementCapability};
#[cfg(test)]
use crate::path_utils;
use crate::piece::{Color, Piece};
use crate::position::Position;

pub struct MovementGenerator;

impl MovementGenerator {
    /// Generate all possible target positions for a piece given its movement capabilities
    pub fn generate_targets<B: BoardLike>(
        piece: &Piece,
        board: &B,
        capabilities: &[MovementCapability],
    ) -> Vec<Position> {
        Self::generate_targets_filtered(piece, board, capabilities, false)
    }

    /// Like [`generate_targets`], but when `captures_only` is set only returns
    /// landings that capture an enemy (or capturing-range path clears).
    pub fn generate_targets_filtered<B: BoardLike>(
        piece: &Piece,
        board: &B,
        capabilities: &[MovementCapability],
        captures_only: bool,
    ) -> Vec<Position> {
        let mut targets = Vec::new();

        for capability in capabilities {
            Self::append_for_capability(piece, board, capability, captures_only, &mut targets);
        }

        // Remove duplicates
        targets.sort();
        targets.dedup();
        targets
    }

    /// Landing squares for a single capability (empty + enemy). Used by eval
    /// to count a two-mover's first-leg Range walk without the second scan.
    pub fn capability_landings<B: BoardLike>(
        piece: &Piece,
        board: &B,
        capability: &MovementCapability,
    ) -> Vec<Position> {
        Self::generate_for_capability(piece, board, capability, false)
    }

    /// Count first-leg landings without allocating a target list for Range walks.
    /// Other capabilities preserve their existing raw landing-list semantics.
    pub fn capability_landing_count<B: BoardLike>(
        piece: &Piece,
        board: &B,
        capability: &MovementCapability,
    ) -> usize {
        if matches!(capability, MovementCapability::Range { .. }) {
            let mut count = 0;
            Self::visit_range_landings(piece, board, capability, false, |_| count += 1);
            count
        } else {
            Self::capability_landings(piece, board, capability).len()
        }
    }

    /// Like [`capability_landings`], optionally restricted to capturing landings.
    /// Does not sort/dedup (single capability).
    pub fn capability_landings_filtered<B: BoardLike>(
        piece: &Piece,
        board: &B,
        capability: &MovementCapability,
        captures_only: bool,
    ) -> Vec<Position> {
        Self::generate_for_capability(piece, board, capability, captures_only)
    }

    /// Directed probe: can `piece` reach `target` without materializing full fan-out.
    ///
    /// Simple / Range / Jumping walk only the relevant ray or offset. TwoStep,
    /// ConditionalDiagonalJump, and FreeEagle fall back to full generation.
    pub fn can_reach_target<B: BoardLike>(
        piece: &Piece,
        board: &B,
        capabilities: &[MovementCapability],
        target: Position,
    ) -> bool {
        if piece.position == target {
            return false;
        }
        for capability in capabilities {
            if Self::capability_reaches(piece, board, capability, target) {
                return true;
            }
        }
        false
    }

    /// True when any capability needs full fan-out (two-step / FE / conditional jump).
    pub fn needs_full_gen_for_victim_hits(capabilities: &[MovementCapability]) -> bool {
        capabilities.iter().any(|cap| {
            matches!(
                cap,
                MovementCapability::TwoStep { .. }
                    | MovementCapability::ConditionalDiagonalJump { .. }
                    | MovementCapability::FreeEagleMultiMove { .. }
            )
        })
    }

    /// Landing squares for standard (non-multi-leg) captures that take an enemy on `victim`.
    ///
    /// Dest captures plus Capturing-range path-clears through `victim`. Callers that need
    /// TwoStep / FreeEagle / conditional-jump must fall back to full CapturesOnly.
    pub fn capture_landings_hitting_target<B: BoardLike>(
        piece: &Piece,
        board: &B,
        capabilities: &[MovementCapability],
        victim: Position,
    ) -> Vec<Position> {
        let victim_is_enemy = board
            .get_piece(victim)
            .is_some_and(|p| p.color != piece.color);
        if !victim_is_enemy || piece.position == victim {
            return Vec::new();
        }

        let mut landings = Vec::new();
        for capability in capabilities {
            match capability {
                MovementCapability::Simple { .. } | MovementCapability::Jumping { .. } => {
                    if Self::capability_reaches(piece, board, capability, victim) {
                        landings.push(victim);
                    }
                }
                MovementCapability::Range {
                    directions,
                    blocking,
                    cannot_jump_over,
                } => match blocking {
                    BlockingMode::NoJump | BlockingMode::Jump => {
                        if Self::capability_reaches(piece, board, capability, victim) {
                            landings.push(victim);
                        }
                    }
                    BlockingMode::Capturing => {
                        let Some(dir) =
                            crate::attack_utils::get_direction_toward(piece.position, victim)
                        else {
                            continue;
                        };
                        let adjusted = Self::adjust_directions_for_color(*directions, piece.color);
                        if !crate::movement::direction::direction_set_contains(adjusted, dir) {
                            continue;
                        }
                        let (file_delta, rank_delta) = dir.to_offset();
                        let mut distance = 1u8;
                        let mut seen_victim = false;
                        let mut ray_has_enemy_capture = false;
                        while let Some(pos) = piece
                            .position
                            .offset(file_delta * distance as i8, rank_delta * distance as i8)
                        {
                            let target_piece = board.get_piece(pos);
                            let is_empty = target_piece.is_none();
                            let is_friendly = target_piece
                                .map(|p| p.color == piece.color)
                                .unwrap_or(false);
                            let is_enemy = target_piece
                                .map(|p| p.color != piece.color)
                                .unwrap_or(false);
                            if let Some(piece_in_path) = target_piece {
                                if cannot_jump_over.contains(&piece_in_path.piece_type) {
                                    break;
                                }
                            }
                            if is_enemy {
                                ray_has_enemy_capture = true;
                                if pos == victim {
                                    seen_victim = true;
                                }
                            }
                            if !is_friendly
                                && seen_victim
                                && (is_enemy || (is_empty && ray_has_enemy_capture))
                            {
                                landings.push(pos);
                            }
                            distance = match distance.checked_add(1) {
                                Some(d) => d,
                                None => break,
                            };
                        }
                    }
                },
                MovementCapability::TwoStep { .. }
                | MovementCapability::ConditionalDiagonalJump { .. }
                | MovementCapability::FreeEagleMultiMove { .. } => {}
            }
        }
        landings.sort();
        landings.dedup();
        landings
    }

    fn capability_reaches<B: BoardLike>(
        piece: &Piece,
        board: &B,
        capability: &MovementCapability,
        target: Position,
    ) -> bool {
        match capability {
            MovementCapability::Simple {
                directions,
                max_distance,
            } => {
                let Some(dir) = crate::attack_utils::get_direction_toward(piece.position, target)
                else {
                    return false;
                };
                let adjusted = Self::adjust_directions_for_color(*directions, piece.color);
                if !crate::movement::direction::direction_set_contains(adjusted, dir) {
                    return false;
                }
                let (file_delta, rank_delta) = dir.to_offset();
                let file_diff = target.file() as i8 - piece.position.file() as i8;
                let rank_diff = target.rank() as i8 - piece.position.rank() as i8;
                let dist = file_diff.abs().max(rank_diff.abs()) as u8;
                if dist == 0 || dist > *max_distance {
                    return false;
                }
                // Same landing rules as generate_simple along this ray only.
                for distance in 1..=dist {
                    let Some(pos) = piece
                        .position
                        .offset(file_delta * distance as i8, rank_delta * distance as i8)
                    else {
                        return false;
                    };
                    if pos == target {
                        if let Some(tp) = board.get_piece(pos) {
                            return tp.color != piece.color;
                        }
                        return true;
                    }
                    if let Some(tp) = board.get_piece(pos) {
                        // Occupied before target blocks further (enemy would have been target).
                        let _ = tp;
                        return false;
                    }
                }
                false
            }
            MovementCapability::Range {
                directions,
                blocking,
                cannot_jump_over,
            } => {
                if let Some(dir) = crate::attack_utils::get_direction_toward(piece.position, target)
                {
                    let adjusted = Self::adjust_directions_for_color(*directions, piece.color);
                    if !crate::movement::direction::direction_set_contains(adjusted, dir) {
                        return false;
                    }
                    // One-direction range walk (same rules as generate_range).
                    let (file_delta, rank_delta) = dir.to_offset();
                    let mut distance = 1u8;
                    let mut ray_has_enemy_capture = false;
                    loop {
                        let Some(pos) = piece
                            .position
                            .offset(file_delta * distance as i8, rank_delta * distance as i8)
                        else {
                            return false;
                        };
                        let target_piece = board.get_piece(pos);
                        let is_empty = target_piece.is_none();
                        let is_friendly = target_piece
                            .map(|p| p.color == piece.color)
                            .unwrap_or(false);
                        let is_enemy = target_piece
                            .map(|p| p.color != piece.color)
                            .unwrap_or(false);
                        let mut lands = false;
                        match blocking {
                            BlockingMode::NoJump => {
                                if is_enemy || is_empty {
                                    lands = true;
                                }
                                if pos == target {
                                    return lands;
                                }
                                if !is_empty {
                                    return false;
                                }
                            }
                            BlockingMode::Jump => {
                                if is_enemy || !is_friendly {
                                    lands = true;
                                }
                                if pos == target {
                                    return lands;
                                }
                            }
                            BlockingMode::Capturing => {
                                if let Some(piece_in_path) = target_piece {
                                    if cannot_jump_over.contains(&piece_in_path.piece_type) {
                                        return false;
                                    }
                                }
                                if is_enemy {
                                    ray_has_enemy_capture = true;
                                }
                                if !is_friendly {
                                    lands = true;
                                }
                                if pos == target {
                                    return lands;
                                }
                            }
                        }
                        let _ = (lands, ray_has_enemy_capture);
                        distance = distance.saturating_add(1);
                        if distance > 64 {
                            return false;
                        }
                    }
                } else {
                    false
                }
            }
            MovementCapability::Jumping { offsets } => {
                let df = target.file() as i8 - piece.position.file() as i8;
                let dr = target.rank() as i8 - piece.position.rank() as i8;
                let dr = if piece.color == Color::White { -dr } else { dr };
                offsets.contains(&(df, dr))
                    && !board
                        .get_piece(target)
                        .is_some_and(|p| p.color == piece.color)
            }
            MovementCapability::TwoStep { .. }
            | MovementCapability::ConditionalDiagonalJump { .. }
            | MovementCapability::FreeEagleMultiMove { .. } => {
                Self::generate_for_capability(piece, board, capability, false).contains(&target)
            }
        }
    }

    /// Generate targets for a single movement capability
    fn generate_for_capability<B: BoardLike>(
        piece: &Piece,
        board: &B,
        capability: &MovementCapability,
        captures_only: bool,
    ) -> Vec<Position> {
        let mut targets = Vec::new();
        Self::append_for_capability(piece, board, capability, captures_only, &mut targets);
        targets
    }

    /// Common capabilities append directly to the caller's final target buffer.
    /// Multi-leg capabilities retain their own intermediate lists and semantics.
    fn append_for_capability<B: BoardLike>(
        piece: &Piece,
        board: &B,
        capability: &MovementCapability,
        captures_only: bool,
        targets: &mut Vec<Position>,
    ) {
        match capability {
            MovementCapability::Simple {
                directions,
                max_distance,
            } => Self::generate_simple_into(
                piece,
                board,
                *directions,
                *max_distance,
                captures_only,
                targets,
            ),
            MovementCapability::Range { .. } => {
                Self::visit_range_landings(piece, board, capability, captures_only, |target| {
                    targets.push(target);
                });
            }
            MovementCapability::Jumping { offsets } => {
                Self::generate_jumping_into(piece, board, offsets, captures_only, targets);
            }
            MovementCapability::TwoStep { first, second } => {
                targets.extend(Self::generate_two_step(
                    piece,
                    board,
                    first,
                    second,
                    captures_only,
                ));
            }
            MovementCapability::ConditionalDiagonalJump {
                directions,
                base_jump,
                conditional_jumps,
            } => targets.extend(Self::generate_conditional_diagonal_jump(
                piece,
                board,
                *directions,
                *base_jump,
                conditional_jumps,
                captures_only,
            )),
            MovementCapability::FreeEagleMultiMove {
                max_distance_forward_diagonal,
                max_distance_other,
            } => targets.extend(Self::generate_free_eagle_multi_move(
                piece,
                board,
                *max_distance_forward_diagonal,
                *max_distance_other,
                captures_only,
            )),
        }
    }

    /// Adjust directions for piece color (for pawn and gold general)
    /// Black moves "up" (increasing rank), White moves "down" (decreasing rank)
    pub(crate) fn adjust_directions_for_color(
        directions: DirectionSet,
        color: Color,
    ) -> DirectionSet {
        if color == Color::Black {
            // No adjustment needed for Black (N is forward)
            directions
        } else {
            // For White, flip directions: N<->S, NE<->SW, E<->W, SE<->NW
            let mut adjusted = 0u8;
            if directions & Direction::N.to_bit() != 0 {
                adjusted |= Direction::S.to_bit();
            }
            if directions & Direction::S.to_bit() != 0 {
                adjusted |= Direction::N.to_bit();
            }
            if directions & Direction::NE.to_bit() != 0 {
                adjusted |= Direction::SW.to_bit();
            }
            if directions & Direction::SW.to_bit() != 0 {
                adjusted |= Direction::NE.to_bit();
            }
            if directions & Direction::E.to_bit() != 0 {
                adjusted |= Direction::E.to_bit();
            } // E stays E
            if directions & Direction::W.to_bit() != 0 {
                adjusted |= Direction::W.to_bit();
            } // W stays W
            if directions & Direction::SE.to_bit() != 0 {
                adjusted |= Direction::NW.to_bit();
            }
            if directions & Direction::NW.to_bit() != 0 {
                adjusted |= Direction::SE.to_bit();
            }
            adjusted
        }
    }

    /// Generate targets for simple movement
    #[cfg(test)]
    fn generate_simple<B: BoardLike>(
        piece: &Piece,
        board: &B,
        directions: DirectionSet,
        max_distance: u8,
        captures_only: bool,
    ) -> Vec<Position> {
        let mut targets = Vec::new();
        Self::generate_simple_into(
            piece,
            board,
            directions,
            max_distance,
            captures_only,
            &mut targets,
        );
        targets
    }

    fn generate_simple_into<B: BoardLike>(
        piece: &Piece,
        board: &B,
        directions: DirectionSet,
        max_distance: u8,
        captures_only: bool,
        targets: &mut Vec<Position>,
    ) {
        // Adjust directions for color (for pawn and gold general)
        let adjusted_directions = Self::adjust_directions_for_color(directions, piece.color);

        for direction in direction_iter(adjusted_directions) {
            let (file_delta, rank_delta) = direction.to_offset();

            for distance in 1..=max_distance {
                let file_offset = file_delta * distance as i8;
                let rank_offset = rank_delta * distance as i8;

                let Some(target) = piece.position.offset(file_offset, rank_offset) else {
                    break; // Out of bounds, stop in this direction
                };

                // Check if target has a friendly piece - cannot land on friendly
                if let Some(target_piece) = board.get_piece(target) {
                    if target_piece.color == piece.color {
                        // Can't land on friendly piece - stop in this direction
                        break;
                    }
                    targets.push(target); // enemy
                                          // Preserve the legacy raw list: on the next in-bounds step,
                                          // its path scan emitted this enemy a second time, then stopped.
                                          // Downstream deduplication/order must not change in a speed fix.
                    if distance < max_distance && target.offset(file_delta, rank_delta).is_some() {
                        targets.push(target);
                    }
                    break;
                } else if !captures_only {
                    targets.push(target);
                }
            }
        }
    }

    /// Visit raw Range landings in generation order. Counting and collecting
    /// share the same blocking/capture rules without requiring a temporary Vec.
    fn visit_range_landings<B: BoardLike>(
        piece: &Piece,
        board: &B,
        capability: &MovementCapability,
        captures_only: bool,
        mut visit: impl FnMut(Position),
    ) {
        let MovementCapability::Range {
            directions,
            blocking,
            cannot_jump_over,
        } = capability
        else {
            unreachable!("Range visitor requires a Range capability");
        };

        // Adjust directions for color (for pieces that need it)
        let adjusted_directions = Self::adjust_directions_for_color(*directions, piece.color);

        for direction in direction_iter(adjusted_directions) {
            let (file_delta, rank_delta) = direction.to_offset();
            let mut distance = 1;
            let mut ray_has_enemy_capture = false;

            loop {
                let file_offset = file_delta * distance as i8;
                let rank_offset = rank_delta * distance as i8;

                let Some(target) = piece.position.offset(file_offset, rank_offset) else {
                    break; // Out of bounds
                };

                let target_piece = board.get_piece(target);
                let is_empty = target_piece.is_none();
                let is_friendly = target_piece
                    .map(|p| p.color == piece.color)
                    .unwrap_or(false);
                let is_enemy = target_piece
                    .map(|p| p.color != piece.color)
                    .unwrap_or(false);

                match blocking {
                    BlockingMode::NoJump => {
                        if is_enemy || (is_empty && !captures_only) {
                            visit(target);
                        }
                        // Stop at first piece (whether we can capture it or not)
                        if !is_empty {
                            break;
                        }
                    }
                    BlockingMode::Jump => {
                        if is_enemy || (!is_friendly && !captures_only) {
                            visit(target);
                        }
                    }
                    BlockingMode::Capturing => {
                        if let Some(piece_in_path) = target_piece {
                            if cannot_jump_over.contains(&piece_in_path.piece_type) {
                                break;
                            }
                        }
                        if is_enemy {
                            ray_has_enemy_capture = true;
                        }
                        if !is_friendly
                            && (!captures_only || is_enemy || (is_empty && ray_has_enemy_capture))
                        {
                            visit(target);
                        }
                    }
                }

                distance += 1;
            }
        }
    }

    /// Generate targets for jumping movement
    /// Offsets are relative to Black's perspective (N is up, S is down)
    /// For White pieces, offsets are automatically flipped (rank_delta is negated)
    #[cfg(test)]
    fn generate_jumping<B: BoardLike>(
        piece: &Piece,
        board: &B,
        offsets: &[(i8, i8)],
        captures_only: bool,
    ) -> Vec<Position> {
        let mut targets = Vec::new();
        Self::generate_jumping_into(piece, board, offsets, captures_only, &mut targets);
        targets
    }

    fn generate_jumping_into<B: BoardLike>(
        piece: &Piece,
        board: &B,
        offsets: &[(i8, i8)],
        captures_only: bool,
        targets: &mut Vec<Position>,
    ) {
        for &(file_delta, rank_delta) in offsets {
            // Adjust rank_delta for White (flip vertically)
            let adjusted_rank_delta = if piece.color == Color::White {
                -rank_delta
            } else {
                rank_delta
            };

            if let Some(target) = piece.position.offset(file_delta, adjusted_rank_delta) {
                // Jumping moves are never blocked, but can't land on friendly pieces
                if let Some(target_piece) = board.get_piece(target) {
                    if target_piece.color != piece.color {
                        targets.push(target);
                    }
                } else if !captures_only {
                    targets.push(target);
                }
            }
        }
    }

    /// Generate targets for two-step movement
    fn generate_two_step<B: BoardLike>(
        piece: &Piece,
        board: &B,
        first: &MovementCapability,
        second: &MovementCapability,
        captures_only: bool,
    ) -> Vec<Position> {
        let mut targets = Vec::new();

        // Intermediates need full first-leg targets even for captures_only.
        let first_targets = Self::generate_for_capability(piece, board, first, false);

        if captures_only {
            targets.extend(first_targets.iter().copied().filter(|pos| {
                board
                    .get_piece(*pos)
                    .is_some_and(|p| p.color != piece.color)
            }));
        } else {
            targets.extend(first_targets.iter().copied());
        }

        for intermediate_pos in first_targets {
            let mut temp_piece = *piece;
            temp_piece.position = intermediate_pos;
            let after_first = BoardAfterStep::new(board, *piece, intermediate_pos);
            let inter_captures = board
                .get_piece(intermediate_pos)
                .is_some_and(|p| p.color != piece.color);
            let second_targets = Self::generate_for_capability(
                &temp_piece,
                &after_first,
                second,
                captures_only && !inter_captures,
            );
            if captures_only {
                for t in second_targets {
                    let dest_captures = after_first
                        .get_piece(t)
                        .is_some_and(|p| p.color != piece.color);
                    if inter_captures || dest_captures {
                        targets.push(t);
                    }
                }
            } else {
                targets.extend(second_targets);
            }
        }

        targets
    }

    /// Generate targets for conditional diagonal jump movement
    /// This handles patterns like the Wooden Dove where:
    /// - Can jump base_jump spaces normally (jumping over anything)
    /// - Can jump conditional_jumps distances if the squares from 3 up to the landing square are empty
    fn generate_conditional_diagonal_jump<B: BoardLike>(
        piece: &Piece,
        board: &B,
        directions: DirectionSet,
        base_jump: u8,
        conditional_jumps: &[u8],
        captures_only: bool,
    ) -> Vec<Position> {
        let mut targets = Vec::new();

        // Adjust directions for color (though diagonal directions shouldn't need adjustment)
        let adjusted_directions = Self::adjust_directions_for_color(directions, piece.color);

        // Only process diagonal directions
        let diagonal_directions = vec![Direction::NE, Direction::SE, Direction::SW, Direction::NW];

        for direction in diagonal_directions {
            // Check if this direction is allowed
            if (adjusted_directions & direction.to_bit()) == 0 {
                continue;
            }

            let (file_delta, rank_delta) = direction.to_offset();

            // Generate base jump (normal jump, can jump over anything)
            if let Some(target) = piece
                .position
                .offset(file_delta * base_jump as i8, rank_delta * base_jump as i8)
            {
                // Can land on empty or enemy square
                if let Some(target_piece) = board.get_piece(target) {
                    if target_piece.color != piece.color {
                        targets.push(target); // Can capture enemy
                    }
                } else {
                    targets.push(target); // Empty square
                }
            }

            // Check each conditional jump distance individually
            // The rule is: can only jump over pieces that would be jumped in a 3-space jump (positions 1-2)
            // But positions 1-2 don't need to have pieces - they just can't have pieces beyond position 2
            // For 4-space jump: position 3 must be empty
            // For 5-space jump: positions 3 and 4 must be empty
            for &jump_distance in conditional_jumps {
                // Check that position 3 is empty (required for both 4 and 5 space jumps)
                let pos3 = 3;
                let pos3_empty = if let Some(pos) = piece
                    .position
                    .offset(file_delta * pos3 as i8, rank_delta * pos3 as i8)
                {
                    board.get_piece(pos).is_none()
                } else {
                    false // Out of bounds
                };

                if !pos3_empty {
                    continue; // Position 3 has a piece, can't do this jump distance
                }

                // For 5-space jump, also check that position 4 is empty
                if jump_distance == 5 {
                    let pos4 = 4;
                    let pos4_empty = if let Some(pos) = piece
                        .position
                        .offset(file_delta * pos4 as i8, rank_delta * pos4 as i8)
                    {
                        board.get_piece(pos).is_none()
                    } else {
                        false // Out of bounds
                    };

                    if !pos4_empty {
                        continue; // Position 4 has a piece, can't do 5-space jump
                    }
                }

                // Check if we can land at the target position
                if let Some(target) = piece.position.offset(
                    file_delta * jump_distance as i8,
                    rank_delta * jump_distance as i8,
                ) {
                    if let Some(target_piece) = board.get_piece(target) {
                        if target_piece.color != piece.color {
                            targets.push(target); // Can capture enemy
                        }
                        // Can't land on friendly piece
                    } else if !captures_only {
                        targets.push(target); // Empty square
                    }
                }
            }
        }

        targets
    }

    /// Generate Free Eagle multi-move patterns
    /// Returns positions that can be reached via multi-move patterns
    /// Note: The actual Move objects with paths will be created in game_state.rs
    fn generate_free_eagle_multi_move<B: BoardLike>(
        piece: &Piece,
        board: &B,
        max_distance_forward_diagonal: u8,
        max_distance_other: u8,
        captures_only: bool,
    ) -> Vec<Position> {
        let mut targets = Vec::new();

        // Forward diagonals for this color
        let forward_diagonals = match piece.color {
            Color::Black => vec![Direction::NE, Direction::NW],
            Color::White => vec![Direction::SE, Direction::SW],
        };

        // Other directions (orthogonal and backward diagonals)
        let other_directions = match piece.color {
            Color::Black => vec![
                Direction::N,
                Direction::S,
                Direction::E,
                Direction::W,
                Direction::SE,
                Direction::SW,
            ],
            Color::White => vec![
                Direction::N,
                Direction::S,
                Direction::E,
                Direction::W,
                Direction::NE,
                Direction::NW,
            ],
        };

        // Pattern 1 & 2: Multi-move patterns
        // Generate all possible final destinations for forward diagonals (up to max_distance_forward_diagonal)
        for direction in &forward_diagonals {
            let (file_delta, rank_delta) = direction.to_offset();
            let mut path = vec![piece.position];
            let mut current = piece.position;

            for _ in 1..=max_distance_forward_diagonal {
                let Some(next) = current.offset(file_delta, rank_delta) else {
                    break; // Out of bounds
                };

                if let Some(p) = board.get_piece(next) {
                    if p.color == piece.color {
                        break; // Blocked by friendly piece
                    }
                    // Enemy piece - can capture and continue
                    path.push(next);
                    current = next;
                    targets.push(next);
                } else {
                    path.push(next);
                    current = next;
                    if !captures_only {
                        targets.push(next);
                    }
                }
            }
        }

        // Generate all possible final destinations for other directions (up to max_distance_other)
        for direction in other_directions {
            let (file_delta, rank_delta) = direction.to_offset();
            let mut current = piece.position;

            for _ in 1..=max_distance_other {
                let Some(next) = current.offset(file_delta, rank_delta) else {
                    break; // Out of bounds
                };

                if let Some(p) = board.get_piece(next) {
                    if p.color == piece.color {
                        break; // Blocked by friendly piece
                    }
                    // Enemy piece - can capture and continue
                    current = next;
                    targets.push(next);
                } else {
                    // Empty square
                    current = next;
                    if !captures_only {
                        targets.push(next);
                    }
                }
            }
        }

        // Pattern 3: Special forward diagonal (3 forward + 1 back) - only if capture on 3rd space
        for direction in &forward_diagonals {
            let (file_delta, rank_delta) = direction.to_offset();
            let back_delta = (-file_delta, -rank_delta);

            // Move 3 spaces forward
            let mut pos3 = piece.position;
            let mut valid = true;
            for _ in 0..3 {
                let Some(next) = pos3.offset(file_delta, rank_delta) else {
                    valid = false;
                    break;
                };
                if let Some(p) = board.get_piece(next) {
                    if p.color == piece.color {
                        valid = false;
                        break;
                    }
                }
                pos3 = next;
            }

            if !valid {
                continue;
            }

            // Check if there's a capture on the 3rd space
            if let Some(p) = board.get_piece(pos3) {
                if p.color != piece.color {
                    // There's an enemy piece on the 3rd space - pattern is valid
                    // Move 1 space back
                    if let Some(final_pos) = pos3.offset(back_delta.0, back_delta.1) {
                        if let Some(p) = board.get_piece(final_pos) {
                            if p.color != piece.color {
                                targets.push(final_pos); // Can capture on backward step
                            }
                        } else {
                            targets.push(final_pos); // Empty square
                        }
                    }
                }
            }
        }

        // Pattern 4: Special any direction (2 forward + 1 back) - only if capture on 2nd space
        let all_directions = Direction::all();
        for direction in all_directions {
            let (file_delta, rank_delta) = direction.to_offset();
            let back_delta = (-file_delta, -rank_delta);

            // Move 2 spaces forward
            let mut pos2 = piece.position;
            let mut valid = true;
            for _ in 0..2 {
                let Some(next) = pos2.offset(file_delta, rank_delta) else {
                    valid = false;
                    break;
                };
                if let Some(p) = board.get_piece(next) {
                    if p.color == piece.color {
                        valid = false;
                        break;
                    }
                }
                pos2 = next;
            }

            if !valid {
                continue;
            }

            // Check if there's a capture on the 2nd space
            if let Some(p) = board.get_piece(pos2) {
                if p.color != piece.color {
                    // There's an enemy piece on the 2nd space - pattern is valid
                    // Move 1 space back
                    if let Some(final_pos) = pos2.offset(back_delta.0, back_delta.1) {
                        if let Some(p) = board.get_piece(final_pos) {
                            if p.color != piece.color {
                                targets.push(final_pos); // Can capture on backward step
                            }
                        } else {
                            targets.push(final_pos); // Empty square
                        }
                    }
                }
            }
        }

        // Pattern 5: Stay in place while capturing enemy 1 space away in any direction
        if Self::check_pattern5(piece, board).is_some() {
            targets.push(piece.position);
        }

        // Pattern 6: Stay in place while capturing on 2nd space along forward diagonal
        if Self::check_pattern6(piece, board).is_some() {
            targets.push(piece.position);
        }

        // Remove duplicates
        targets.sort();
        targets.dedup();
        targets
    }

    /// Check if Pattern 5 is valid (enemy 1 space away in any direction)
    /// Returns Some(capture_pos) if valid, None otherwise
    pub fn check_pattern5<B: BoardLike>(piece: &Piece, board: &B) -> Option<Position> {
        use crate::movement::direction::Direction;

        for direction in Direction::all() {
            let (file_delta, rank_delta) = direction.to_offset();

            if let Some(capture_pos) = piece.position.offset(file_delta, rank_delta) {
                if let Some(target_piece) = board.get_piece(capture_pos) {
                    if target_piece.color != piece.color {
                        return Some(capture_pos);
                    }
                }
            }
        }
        None
    }

    /// Check if Pattern 6 is valid (capture on 2nd space along forward diagonal)
    /// Returns Some((pos1, pos2)) if valid, None otherwise
    /// pos1 may be None if the first position is empty
    pub fn check_pattern6<B: BoardLike>(
        piece: &Piece,
        board: &B,
    ) -> Option<(Option<Position>, Position)> {
        use crate::movement::direction::Direction;

        let forward_diagonals = match piece.color {
            Color::Black => vec![Direction::NE, Direction::NW],
            Color::White => vec![Direction::SE, Direction::SW],
        };

        for direction in &forward_diagonals {
            let (file_delta, rank_delta) = direction.to_offset();

            // Check position 1 space away
            let pos1 = piece.position.offset(file_delta, rank_delta);

            // Check position 2 spaces away (only if we can reach it)
            if let Some(pos1) = pos1 {
                // Check if position 1 is empty or has an enemy (can jump over enemy)
                let can_reach_pos2 = if let Some(p) = board.get_piece(pos1) {
                    p.color != piece.color // Can jump over enemy at pos1
                } else {
                    true // Empty at pos1, can continue
                };

                if can_reach_pos2 {
                    if let Some(pos2) = pos1.offset(file_delta, rank_delta) {
                        if let Some(p) = board.get_piece(pos2) {
                            if p.color != piece.color {
                                return Some((Some(pos1), pos2));
                            }
                        }
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod linear_simple_tests {
    use super::*;
    use crate::{board::Board, piece::PieceType};

    #[test]
    fn range_counts_and_raw_order_cover_forward_diagonal_and_edge_blockers() {
        use crate::movement::types::PieceTypeSet;

        for color in [Color::Black, Color::White] {
            let enemy = color.opposite();
            for (file, rank) in [(18, 18), (0, 0), (35, 35), (1, 34)] {
                let piece = Piece::new(PieceType::Rook, color, Position::new(file, rank).unwrap());
                for (direction, mut df, mut dr) in [
                    (Direction::N, 0i8, 1i8),
                    (Direction::NE, 1, 1),
                    (Direction::SE, 1, -1),
                    (Direction::E, 1, 0),
                ] {
                    // Preserve the generator's legacy convention: White reverses
                    // these diagonal/forward rays, while E remains E.
                    if color == Color::White && direction != Direction::E {
                        df = -df;
                        dr = -dr;
                    }
                    let ray: Vec<_> = (1..36)
                        .filter_map(|distance| piece.position.offset(df * distance, dr * distance))
                        .collect();
                    for first in [
                        None,
                        Some((PieceType::Pawn, color)),
                        Some((PieceType::Pawn, enemy)),
                        Some((PieceType::King, color)),
                        Some((PieceType::King, enemy)),
                    ] {
                        let mut board = Board::new();
                        board.place_piece(piece);
                        let occupants: Vec<_> = first
                            .map(|(kind, occupant)| (1, kind, occupant))
                            .into_iter()
                            .chain([(3, PieceType::Pawn, enemy), (5, PieceType::Pawn, color)])
                            .filter(|(distance, _, _)| *distance <= ray.len())
                            .collect();
                        for &(distance, kind, occupant) in &occupants {
                            board.place_piece(Piece::new(kind, occupant, ray[distance - 1]));
                        }
                        for prohibit_king in [false, true] {
                            let mut prohibited = PieceTypeSet::new();
                            if prohibit_king {
                                prohibited.insert(PieceType::King);
                            }
                            let first_occupied =
                                occupants.first().map(|p| p.0).unwrap_or(usize::MAX);
                            let first_enemy = occupants
                                .iter()
                                .find(|p| p.2 == enemy)
                                .map(|p| p.0)
                                .unwrap_or(usize::MAX);
                            let first_prohibited = occupants
                                .iter()
                                .find(|p| prohibit_king && p.1 == PieceType::King)
                                .map(|p| p.0)
                                .unwrap_or(usize::MAX);
                            for blocking in [
                                BlockingMode::NoJump,
                                BlockingMode::Jump,
                                BlockingMode::Capturing,
                            ] {
                                let capability = MovementCapability::Range {
                                    directions: direction.to_bit(),
                                    blocking,
                                    cannot_jump_over: prohibited,
                                };
                                for captures in [false, true] {
                                    // Independently specify eligibility by distance
                                    // and the known occupancy layout, without a ray
                                    // walk that shares the generator's control flow.
                                    let expected: Vec<_> = ray
                                        .iter()
                                        .copied()
                                        .enumerate()
                                        .filter_map(|(i, pos)| {
                                            let distance = i + 1;
                                            let occupant =
                                                occupants.iter().find(|p| p.0 == distance);
                                            let friendly = occupant.is_some_and(|p| p.2 == color);
                                            let enemy_here = occupant.is_some_and(|p| p.2 == enemy);
                                            let allowed = !friendly
                                                && match blocking {
                                                    BlockingMode::NoJump => {
                                                        distance <= first_occupied
                                                            && (!captures || enemy_here)
                                                    }
                                                    BlockingMode::Jump => !captures || enemy_here,
                                                    BlockingMode::Capturing => {
                                                        distance < first_prohibited
                                                            && (!captures
                                                                || distance >= first_enemy)
                                                    }
                                                };
                                            allowed.then_some(pos)
                                        })
                                        .collect();
                                    assert_eq!(
                                        MovementGenerator::capability_landings_filtered(&piece, &board, &capability, captures),
                                        expected,
                                        "{piece:?}, {direction:?}, {first:?}, prohibit={prohibit_king}, {blocking:?}, captures={captures}"
                                    );
                                    if !captures {
                                        assert_eq!(
                                            MovementGenerator::capability_landing_count(
                                                &piece,
                                                &board,
                                                &capability
                                            ),
                                            expected.len()
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn capability_append_preserves_prefix_and_mixed_fallback_union() {
        use crate::movement::types::PieceTypeSet;

        for color in [Color::Black, Color::White] {
            let mut board = Board::new();
            let piece = Piece::new(PieceType::FreeEagle, color, Position::new(18, 18).unwrap());
            board.place_piece(piece);
            for (file, rank, occupant) in [
                (19, 18, color.opposite()),
                (18, 19, color),
                (20, 20, color.opposite()),
            ] {
                board.place_piece(Piece::new(
                    PieceType::Pawn,
                    occupant,
                    Position::new(file, rank).unwrap(),
                ));
            }
            let ordinary = vec![
                MovementCapability::Simple {
                    directions: 0,
                    max_distance: 1,
                },
                MovementCapability::Jumping {
                    offsets: vec![(-5, 0)],
                },
                MovementCapability::Simple {
                    directions: Direction::E.to_bit(),
                    max_distance: 2,
                },
                MovementCapability::Range {
                    directions: Direction::E.to_bit(),
                    blocking: BlockingMode::NoJump,
                    cannot_jump_over: PieceTypeSet::new(),
                },
                MovementCapability::Jumping {
                    offsets: vec![(1, 0), (-5, 0)],
                },
            ];
            for fallback in [
                MovementCapability::TwoStep {
                    first: Box::new(MovementCapability::Simple {
                        directions: Direction::W.to_bit(),
                        max_distance: 1,
                    }),
                    second: Box::new(MovementCapability::Simple {
                        directions: Direction::N.to_bit(),
                        max_distance: 1,
                    }),
                },
                MovementCapability::FreeEagleMultiMove {
                    max_distance_forward_diagonal: 4,
                    max_distance_other: 3,
                },
                MovementCapability::ConditionalDiagonalJump {
                    directions: Direction::NE.to_bit(),
                    base_jump: 3,
                    conditional_jumps: vec![4, 5],
                },
            ] {
                let mut capabilities = ordinary.clone();
                capabilities.push(fallback);
                for captures in [false, true] {
                    let mut expected = Vec::new();
                    for capability in &capabilities {
                        let raw = MovementGenerator::capability_landings_filtered(
                            &piece, &board, capability, captures,
                        );
                        let sentinel = Position::new(35, 35).unwrap();
                        let mut appended = vec![sentinel];
                        MovementGenerator::append_for_capability(
                            &piece,
                            &board,
                            capability,
                            captures,
                            &mut appended,
                        );
                        assert_eq!(appended.first(), Some(&sentinel));
                        assert_eq!(&appended[1..], raw.as_slice());
                        expected.extend(raw);
                    }
                    expected.sort();
                    expected.dedup();
                    let actual = MovementGenerator::generate_targets_filtered(
                        &piece,
                        &board,
                        &capabilities,
                        captures,
                    );
                    assert_eq!(actual, expected, "{color:?}, captures={captures}");
                    let overlapping_capture = Position::new(19, 18).unwrap();
                    assert_eq!(
                        actual.iter().filter(|&&p| p == overlapping_capture).count(),
                        1
                    );
                    if !captures {
                        assert!(actual.contains(&Position::new(13, 18).unwrap()));
                    }
                }
            }
        }
    }

    #[test]
    fn range_visits_and_counts_preserve_blockers_and_capture_sweeps() {
        use crate::movement::types::PieceTypeSet;

        // A friendly pawn is passed by jumping/capturing ranges, an enemy pawn
        // starts a capture sweep, and the king stops only the capturing range.
        for color in [Color::Black, Color::White] {
            let enemy = if color == Color::Black {
                Color::White
            } else {
                Color::Black
            };
            let mut board = Board::new();
            let piece = Piece::new(PieceType::Rook, color, Position::new(0, 0).unwrap());
            for (file, kind, occupant) in [
                (2, PieceType::Pawn, color),
                (4, PieceType::Pawn, enemy),
                (6, PieceType::King, enemy),
            ] {
                board.place_piece(Piece::new(kind, occupant, Position::new(file, 0).unwrap()));
            }
            let mut blockers = PieceTypeSet::new();
            blockers.insert(PieceType::King);
            for blocking in [
                BlockingMode::NoJump,
                BlockingMode::Jump,
                BlockingMode::Capturing,
            ] {
                let capability = MovementCapability::Range {
                    directions: Direction::E.to_bit(),
                    blocking,
                    cannot_jump_over: blockers,
                };
                for captures in [false, true] {
                    let files: Vec<u8> = match (blocking, captures) {
                        (BlockingMode::NoJump, false) => vec![1],
                        (BlockingMode::NoJump, true) => vec![],
                        (BlockingMode::Jump, false) => (1..36).filter(|file| *file != 2).collect(),
                        (BlockingMode::Jump, true) => vec![4, 6],
                        (BlockingMode::Capturing, false) => vec![1, 3, 4, 5],
                        (BlockingMode::Capturing, true) => vec![4, 5],
                    };
                    let expected: Vec<_> = files
                        .into_iter()
                        .map(|file| Position::new(file, 0).unwrap())
                        .collect();
                    assert_eq!(
                        MovementGenerator::capability_landings_filtered(
                            &piece,
                            &board,
                            &capability,
                            captures
                        ),
                        expected,
                        "{color:?} {blocking:?} captures={captures}"
                    );
                    if !captures {
                        assert_eq!(
                            MovementGenerator::capability_landing_count(
                                &piece,
                                &board,
                                &capability
                            ),
                            expected.len()
                        );
                    }
                }
            }
        }
    }

    // Original repeated-path implementation, including its duplicate capture.
    fn reference(
        piece: &Piece,
        board: &Board,
        directions: DirectionSet,
        max: u8,
        captures: bool,
    ) -> Vec<Position> {
        let mut result = Vec::new();
        for direction in direction_iter(MovementGenerator::adjust_directions_for_color(
            directions,
            piece.color,
        )) {
            let (df, dr) = direction.to_offset();
            for distance in 1..=max {
                let Some(to) = piece
                    .position
                    .offset(df * distance as i8, dr * distance as i8)
                else {
                    break;
                };
                if max > 1 && !path_utils::is_path_clear_for_boardlike(board, piece.position, to) {
                    for pos in path_utils::path_positions(piece.position, to) {
                        if let Some(blocker) = board.get_piece(pos) {
                            if blocker.color != piece.color {
                                result.push(pos);
                            }
                            break;
                        }
                    }
                    break;
                }
                if let Some(occupant) = board.get_piece(to) {
                    if occupant.color == piece.color {
                        break;
                    }
                    result.push(to);
                } else if !captures {
                    result.push(to);
                }
            }
        }
        result
    }

    #[test]
    fn direct_reach_matches_original_generated_landings() {
        let jumps = [
            vec![],
            vec![(1, 2), (-1, 2)],
            vec![(2, 2), (-3, -3)],
            vec![(0, 0), (35, 0), (-35, 0), (0, 35), (0, -35)],
            (-5..=5)
                .flat_map(|x| (-5..=5).map(move |y| (x, y)))
                .collect(),
        ];
        for pattern in 0..3 {
            let mut board = Board::new();
            for i in 0..1296 {
                if pattern > 0 && (i * 13 + i / 36) % (pattern * 11) == 0 {
                    let pos = Position::new((i % 36) as u8, (i / 36) as u8).unwrap();
                    board.place_piece(Piece::new(
                        PieceType::Pawn,
                        if i % 3 == 0 {
                            Color::Black
                        } else {
                            Color::White
                        },
                        pos,
                    ));
                }
            }
            for (file, rank) in [(0, 0), (35, 35), (17, 17), (1, 34)] {
                for color in [Color::Black, Color::White] {
                    let piece =
                        Piece::new(PieceType::King, color, Position::new(file, rank).unwrap());
                    let mut cases = Vec::new();
                    for directions in [1, 5, 0xaa, 0xff] {
                        for max in [1, 2, 5, 36] {
                            cases.push((
                                MovementCapability::Simple {
                                    directions,
                                    max_distance: max,
                                },
                                reference(&piece, &board, directions, max, false),
                            ));
                        }
                    }
                    for offsets in &jumps {
                        cases.push((
                            MovementCapability::Jumping {
                                offsets: offsets.clone(),
                            },
                            MovementGenerator::generate_jumping(&piece, &board, offsets, false),
                        ));
                    }
                    for (cap, expected) in cases {
                        for i in 0..1296 {
                            let to = Position::new((i % 36) as u8, (i / 36) as u8).unwrap();
                            assert_eq!(
                                MovementGenerator::capability_reaches(&piece, &board, &cap, to),
                                expected.contains(&to),
                                "{piece:?} {cap:?} -> {to:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn simple_raw_order_matches_legacy_paths_and_blockers() {
        for pattern in 0..4 {
            let mut board = Board::new();
            for i in 0..1296 {
                if pattern > 0 && (i * 17 + i / 36) % (pattern * 7) == 0 {
                    let pos = Position::new((i % 36) as u8, (i / 36) as u8).unwrap();
                    let color = if i % 3 == 0 {
                        Color::Black
                    } else {
                        Color::White
                    };
                    board.place_piece(Piece::new(PieceType::Pawn, color, pos));
                }
            }
            for (file, rank) in [(0, 0), (35, 35), (17, 17), (1, 34)] {
                for color in [Color::Black, Color::White] {
                    let piece =
                        Piece::new(PieceType::King, color, Position::new(file, rank).unwrap());
                    for mask in 0..=255 {
                        for max in [0, 1, 2, 5, 36] {
                            for captures in [false, true] {
                                assert_eq!(MovementGenerator::generate_simple(&piece, &board, mask, max, captures),
                                    reference(&piece, &board, mask, max, captures),
                                    "pattern={pattern} from={:?} color={color:?} mask={mask} max={max} captures={captures}", piece.position);
                            }
                        }
                    }
                }
            }
        }
    }
}
