//! Fischer-style opening starts for Texel training pools.
//!
//! Mirrored rank shuffles (with Left/Right file anchors) plus independent
//! powerful-piece and royal ablations. See AGENTS.md / training CLI help.

use crate::board_position::BoardPosition;
use crate::game_state::GameState;
use crate::piece::{Color, Piece, PieceType};
use crate::position::Position;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

/// Probability each Black home rank (0..=11) is shuffled.
pub const RANK_SHUFFLE_P: f64 = 0.10;
/// P(remove exactly 2 powerful) = 0.05; P(exactly 1) = 0.50; else 0.
pub const POWERFUL_TWO_P: f64 = 0.05;
pub const POWERFUL_ONE_P: f64 = 0.50;
/// Per-player chance to remove one royal (King or CrownPrince, 50/50).
pub const ROYAL_REMOVE_P: f64 = 0.20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemovedPiece {
    pub color: Color,
    pub piece_type: PieceType,
    pub file: u8,
    pub rank: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StartRecipe {
    pub seed: u64,
    pub shuffled_black_ranks: Vec<u8>,
    pub removed: Vec<RemovedPiece>,
}

#[derive(Debug, Clone)]
pub struct FischerGenOptions {
    pub rank_shuffle_p: f64,
    pub apply_powerful: bool,
    pub apply_royal: bool,
}

impl Default for FischerGenOptions {
    fn default() -> Self {
        Self {
            rank_shuffle_p: RANK_SHUFFLE_P,
            apply_powerful: true,
            apply_royal: true,
        }
    }
}

/// Capturing-range + two-move-range opening types (~15 instances / side).
pub fn is_powerful_opening_piece(pt: PieceType) -> bool {
    matches!(
        pt,
        PieceType::GreatGeneral
            | PieceType::ViceGeneral
            | PieceType::BishopGeneral
            | PieceType::FlyingGeneral
            | PieceType::FierceDragon
            | PieceType::Tengu
            | PieceType::Capricorn
            | PieceType::HookMover
            | PieceType::Peacock
            | PieceType::FreeEagle
    )
}

/// Left/Right wing pieces stay on their opening files during shuffles.
pub fn is_file_tied(pt: PieceType) -> bool {
    matches!(
        pt,
        PieceType::LeftGeneral
            | PieceType::RightGeneral
            | PieceType::LeftMountainEagle
            | PieceType::RightMountainEagle
            | PieceType::LeftTiger
            | PieceType::RightTiger
            | PieceType::LeftDragon
            | PieceType::RightDragon
            | PieceType::LeftHowlingDog
            | PieceType::RightHowlingDog
            | PieceType::LeftChariot
            | PieceType::RightChariot
    )
}

#[allow(dead_code)] // used by unit tests
fn count_powerful(state: &GameState, color: Color) -> usize {
    state
        .get_board()
        .pieces_by_color(color)
        .iter()
        .filter(|p| is_powerful_opening_piece(p.piece_type))
        .count()
}

/// Apply mirrored Fischer rank shuffles for Black ranks 0..=11.
pub fn apply_mirrored_rank_shuffles(
    state: &mut GameState,
    rng: &mut StdRng,
    p: f64,
    recipe: &mut StartRecipe,
) {
    for black_rank in 0u8..=11 {
        if rng.gen::<f64>() >= p {
            continue;
        }
        if shuffle_one_mirrored_rank(state, black_rank, rng) {
            recipe.shuffled_black_ranks.push(black_rank);
        }
    }
}

/// Shuffle free files on `black_rank` and conjugate White rank `35 - black_rank`.
/// Returns false if there was nothing to shuffle.
fn shuffle_one_mirrored_rank(state: &mut GameState, black_rank: u8, rng: &mut StdRng) -> bool {
    let white_rank = 35u8.saturating_sub(black_rank);
    let board = state.get_board();

    let mut black_anchor = [false; 36];
    for file in 0u8..36 {
        let Some(pos) = Position::new(file, black_rank) else {
            continue;
        };
        if let Some(p) = board.get_piece(pos) {
            if p.color == Color::Black && is_file_tied(p.piece_type) {
                black_anchor[file as usize] = true;
            }
        }
    }

    let free: Vec<u8> = (0u8..36).filter(|&f| !black_anchor[f as usize]).collect();
    if free.len() < 2 {
        return false;
    }

    let mut dest = free.clone();
    dest.shuffle(rng);
    if dest == free {
        // Extremely rare; force a non-identity swap when possible.
        dest.swap(0, 1);
    }

    let mut map = [0u8; 36];
    for f in 0u8..36 {
        map[f as usize] = f;
    }
    for (i, &f_from) in free.iter().enumerate() {
        map[f_from as usize] = dest[i];
    }

    let mut reloc: Vec<(Position, Position, Piece)> = Vec::new();

    for &file in &free {
        let Some(from) = Position::new(file, black_rank) else {
            continue;
        };
        if let Some(p) = board.get_piece(from) {
            if p.color == Color::Black {
                let new_f = map[file as usize];
                let Some(to) = Position::new(new_f, black_rank) else {
                    continue;
                };
                if to != from {
                    reloc.push((from, to, p));
                }
            }
        }
    }

    for file in 0u8..36 {
        // White free iff conjugate Black file is free.
        if black_anchor[(35 - file) as usize] {
            continue;
        }
        let Some(from) = Position::new(file, white_rank) else {
            continue;
        };
        if let Some(p) = board.get_piece(from) {
            if p.color == Color::White {
                let black_from = 35 - file;
                let black_to = map[black_from as usize];
                let new_f = 35 - black_to;
                let Some(to) = Position::new(new_f, white_rank) else {
                    continue;
                };
                if to != from {
                    reloc.push((from, to, p));
                }
            }
        }
    }

    if reloc.is_empty() {
        return false;
    }

    for (from, _, _) in &reloc {
        state.remove_piece(*from);
    }
    for (_, to, mut piece) in reloc {
        piece.position = to;
        state.place_piece(piece);
    }
    true
}

fn powerful_remove_count(rng: &mut StdRng) -> usize {
    let u: f64 = rng.gen();
    if u < POWERFUL_TWO_P {
        2
    } else if u < POWERFUL_TWO_P + POWERFUL_ONE_P {
        1
    } else {
        0
    }
}

pub fn apply_powerful_ablation(
    state: &mut GameState,
    color: Color,
    rng: &mut StdRng,
    recipe: &mut StartRecipe,
) {
    let n = powerful_remove_count(rng);
    if n == 0 {
        return;
    }
    let mut cands: Vec<Piece> = state
        .get_board()
        .pieces_by_color(color)
        .iter()
        .copied()
        .filter(|p| is_powerful_opening_piece(p.piece_type))
        .collect();
    if cands.is_empty() {
        return;
    }
    cands.shuffle(rng);
    let take = n.min(cands.len());
    for p in cands.into_iter().take(take) {
        if state.remove_piece(p.position).is_some() {
            recipe.removed.push(RemovedPiece {
                color,
                piece_type: p.piece_type,
                file: p.position.file,
                rank: p.position.rank,
            });
        }
    }
}

pub fn apply_royal_ablation(
    state: &mut GameState,
    color: Color,
    rng: &mut StdRng,
    recipe: &mut StartRecipe,
) {
    if rng.gen::<f64>() >= ROYAL_REMOVE_P {
        return;
    }
    let royals: Vec<Piece> = state
        .get_board()
        .pieces_by_color(color)
        .iter()
        .copied()
        .filter(|p| p.piece_type.is_royal())
        .collect();
    if royals.len() < 2 {
        // Never leave zero royals; with one left, skip.
        return;
    }
    let prefer_king = rng.gen_bool(0.5);
    let target = if prefer_king {
        royals
            .iter()
            .find(|p| p.piece_type == PieceType::King)
            .or_else(|| royals.first())
    } else {
        royals
            .iter()
            .find(|p| p.piece_type == PieceType::CrownPrince)
            .or_else(|| royals.first())
    };
    let Some(p) = target.copied() else {
        return;
    };
    if state.remove_piece(p.position).is_some() {
        recipe.removed.push(RemovedPiece {
            color,
            piece_type: p.piece_type,
            file: p.position.file,
            rank: p.position.rank,
        });
    }
}

/// Opening setup → mirrored shuffles → powerful/royal ablations.
pub fn generate_fischer_start(seed: u64) -> (BoardPosition, StartRecipe) {
    generate_fischer_start_with(seed, &FischerGenOptions::default())
}

pub fn generate_fischer_start_with(
    seed: u64,
    opts: &FischerGenOptions,
) -> (BoardPosition, StartRecipe) {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut state = GameState::new();
    state.setup_initial_position();
    let mut recipe = StartRecipe {
        seed,
        shuffled_black_ranks: Vec::new(),
        removed: Vec::new(),
    };

    apply_mirrored_rank_shuffles(&mut state, &mut rng, opts.rank_shuffle_p, &mut recipe);

    if opts.apply_powerful {
        apply_powerful_ablation(&mut state, Color::Black, &mut rng, &mut recipe);
        apply_powerful_ablation(&mut state, Color::White, &mut rng, &mut recipe);
    }
    if opts.apply_royal {
        apply_royal_ablation(&mut state, Color::Black, &mut rng, &mut recipe);
        apply_royal_ablation(&mut state, Color::White, &mut rng, &mut recipe);
    }

    (BoardPosition::from_state(&state), recipe)
}

impl StartRecipe {
    pub fn save_path(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize StartRecipe: {}", e))?;
        std::fs::write(path, json).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
    }
}

/// True if `path` is a Fischer recipe sidecar (`*.recipe.json`).
pub fn is_recipe_path(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|s| s.to_str())
        .map(|n| n.ends_with(".recipe.json"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opening_powerful_count(color: Color) -> usize {
        let mut state = GameState::new();
        state.setup_initial_position();
        count_powerful(&state, color)
    }

    fn assert_color_mirror(state: &GameState) {
        let board = state.get_board();
        for p in board.pieces_by_color(Color::Black) {
            let mf = 35 - p.position.file;
            let mr = 35 - p.position.rank;
            let Some(mpos) = Position::new(mf, mr) else {
                panic!("bad mirror pos");
            };
            let Some(w) = board.get_piece(mpos) else {
                panic!(
                    "missing white mirror of {:?} at {:?}",
                    p.piece_type, p.position
                );
            };
            assert_eq!(w.color, Color::White);
            assert_eq!(
                w.piece_type, p.piece_type,
                "type mismatch at black {:?} mirrored",
                p.position
            );
        }
        assert_eq!(
            board.pieces_by_color(Color::Black).len(),
            board.pieces_by_color(Color::White).len()
        );
    }

    #[test]
    fn opening_has_fifteen_powerful_per_side() {
        assert_eq!(opening_powerful_count(Color::Black), 15);
        assert_eq!(opening_powerful_count(Color::White), 15);
    }

    #[test]
    fn forced_shuffle_preserves_mirror_and_file_tied() {
        let mut opening = GameState::new();
        opening.setup_initial_position();
        let tied_before: Vec<_> = opening
            .get_board()
            .pieces_by_color(Color::Black)
            .iter()
            .filter(|p| is_file_tied(p.piece_type))
            .map(|p| (p.piece_type, p.position))
            .collect();

        let opts = FischerGenOptions {
            rank_shuffle_p: 1.0,
            apply_powerful: false,
            apply_royal: false,
        };
        let (pos, recipe) = generate_fischer_start_with(42, &opts);
        assert!(
            !recipe.shuffled_black_ranks.is_empty(),
            "expected some ranks shuffled"
        );
        let state = pos.to_state();
        assert_color_mirror(&state);

        for (pt, old_pos) in tied_before {
            let still = state
                .get_board()
                .pieces_by_color(Color::Black)
                .iter()
                .find(|p| p.piece_type == pt && p.position.file == old_pos.file);
            assert!(
                still.is_some(),
                "file-tied {:?} should stay on file {}",
                pt,
                old_pos.file
            );
            assert_eq!(still.unwrap().position.rank, old_pos.rank);
        }
    }

    #[test]
    fn ablations_never_zero_royals_and_seed_deterministic() {
        for seed in [1u64, 7, 99, 12345, 999_999] {
            let (a, ra) = generate_fischer_start(seed);
            let (b, rb) = generate_fischer_start(seed);
            assert_eq!(a, b);
            assert_eq!(ra, rb);
            let state = a.to_state();
            for color in [Color::Black, Color::White] {
                let n = state
                    .get_board()
                    .pieces_by_color(color)
                    .iter()
                    .filter(|p| p.piece_type.is_royal())
                    .count();
                assert!(n >= 1, "seed {seed} color {:?} has {n} royals", color);
            }
        }
    }

    #[test]
    fn powerful_removal_counts_match_recipe() {
        // Many seeds: removed powerful count per color ≤ 2 and matches recipe.
        for seed in 0u64..40 {
            let (pos, recipe) = generate_fischer_start(seed);
            let state = pos.to_state();
            for color in [Color::Black, Color::White] {
                let removed = recipe
                    .removed
                    .iter()
                    .filter(|r| r.color == color && is_powerful_opening_piece(r.piece_type))
                    .count();
                assert!(removed <= 2);
                let left = count_powerful(&state, color);
                assert_eq!(left + removed, 15);
            }
        }
    }
}
