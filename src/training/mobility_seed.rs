//! Mobility Monte Carlo seed values for piece material.

use crate::board_position::BoardPosition;
use crate::eval::{EvalCheckpoint, EvalWeights, ALL_PIECE_TYPES};
use crate::game_state::GameState;
use crate::movement::{BlockingMode, MovementCapability, MovementConfig};
use crate::piece::{Color, Piece, PieceType};
use crate::position::Position;
use crate::training::paths;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobilityStats {
    pub piece_type: String,
    pub samples: usize,
    pub mean_mobility: f64,
    pub mean_attacks: f64,
    pub suggested_value: f32,
}

#[derive(Debug, Clone)]
pub struct MobilitySeedConfig {
    pub samples_per_piece: usize,
    pub seed: u64,
    /// Optional start positions to sample from; if empty, uses opening after random clears.
    pub starts: Vec<BoardPosition>,
    pub out_model: String,
}

impl Default for MobilitySeedConfig {
    fn default() -> Self {
        Self {
            samples_per_piece: 32,
            seed: 1,
            starts: Vec::new(),
            out_model: "models/ab-mobility-seed.json".to_string(),
        }
    }
}

fn is_capturer(pt: PieceType) -> bool {
    let cfg = MovementConfig::for_piece_type(pt);
    cfg.capabilities.iter().any(|cap| {
        matches!(
            cap,
            MovementCapability::Range {
                blocking: BlockingMode::Capturing,
                ..
            }
        )
    })
}

fn occupancy(state: &GameState) -> usize {
    let b = state.get_board();
    b.get_pieces_by_color(Color::Black).len() + b.get_pieces_by_color(Color::White).len()
}

fn random_empty_square(state: &GameState, rng: &mut StdRng) -> Option<Position> {
    let mut empties = Vec::new();
    for file in 0..36u8 {
        for rank in 0..36u8 {
            if let Some(pos) = Position::new(file, rank) {
                if state.get_board().get_piece(pos).is_none() {
                    empties.push(pos);
                }
            }
        }
    }
    if empties.is_empty() {
        None
    } else {
        Some(empties[rng.gen_range(0..empties.len())])
    }
}

fn sample_base_position(cfg: &MobilitySeedConfig, rng: &mut StdRng) -> GameState {
    if !cfg.starts.is_empty() {
        let idx = rng.gen_range(0..cfg.starts.len());
        return cfg.starts[idx].to_state();
    }
    let mut state = GameState::new();
    state.setup_initial_position();
    // Lightly thin the board for mobility diversity.
    let remove_n = rng.gen_range(20..80);
    for _ in 0..remove_n {
        let board = state.get_board();
        let all: Vec<_> = board
            .get_pieces_by_color(Color::Black)
            .iter()
            .chain(board.get_pieces_by_color(Color::White).iter())
            .copied()
            .collect();
        if all.is_empty() {
            break;
        }
        let p = all[rng.gen_range(0..all.len())];
        if p.piece_type.is_royal() {
            continue;
        }
        state.remove_piece(p.position);
    }
    state
}

fn measure_mobility(state: &GameState, pt: PieceType, pos: Position, color: Color) -> (usize, usize) {
    let mut probe = state.clone();
    // Clear landing square if occupied (should be empty).
    probe.remove_piece(pos);
    let piece = Piece::new(pt, color, pos);
    probe.place_piece(piece);
    probe.set_current_turn(color);
    let moves = probe.generate_legal_moves();
    let from_moves: Vec<_> = moves.into_iter().filter(|m| m.from == pos).collect();
    let mobility = from_moves.len();
    let mut attacks = 0usize;
    let board = probe.get_board();
    for mv in &from_moves {
        if board.get_piece(mv.to).map(|p| p.color != color).unwrap_or(false) {
            attacks += 1;
        }
        // Path clears count as attacks along the path.
        for p in crate::path_utils::get_path_positions(mv.from, mv.to) {
            if p == mv.from || p == mv.to {
                continue;
            }
            if board.get_piece(p).map(|x| x.color != color).unwrap_or(false) {
                attacks += 1;
            }
        }
    }
    (mobility, attacks)
}

/// Estimate mobility-based seed values; capturers keep explicit overrides / seed defaults.
pub fn run_mobility_seed(cfg: &MobilitySeedConfig) -> Result<(EvalCheckpoint, Vec<MobilityStats>), String> {
    let mut rng = StdRng::seed_from_u64(cfg.seed);
    let mut weights = EvalWeights::seed();
    let mut stats = Vec::new();

    for &pt in ALL_PIECE_TYPES {
        if is_capturer(pt) || pt.is_royal() {
            stats.push(MobilityStats {
                piece_type: format!("{:?}", pt),
                samples: 0,
                mean_mobility: 0.0,
                mean_attacks: 0.0,
                suggested_value: weights.piece_value(pt),
            });
            continue;
        }

        let mut mob_sum = 0.0f64;
        let mut atk_sum = 0.0f64;
        let mut n = 0usize;
        for _ in 0..cfg.samples_per_piece {
            let base = sample_base_position(cfg, &mut rng);
            let Some(pos) = random_empty_square(&base, &mut rng) else {
                continue;
            };
            let color = if rng.gen() { Color::Black } else { Color::White };
            let (m, a) = measure_mobility(&base, pt, pos, color);
            // Density-normalize lightly.
            let dens = (occupancy(&base) as f64 / 720.0).clamp(0.1, 1.0);
            mob_sum += m as f64 / dens;
            atk_sum += a as f64 / dens;
            n += 1;
        }
        let mean_m = if n > 0 { mob_sum / n as f64 } else { 0.0 };
        let mean_a = if n > 0 { atk_sum / n as f64 } else { 0.0 };
        // Map mobility to a material-ish scale (pawn ~0.5 at low mobility).
        let suggested = ((mean_m * 0.35 + mean_a * 0.5) as f32).clamp(0.25, 200.0);
        weights.piece.insert(pt, suggested);
        stats.push(MobilityStats {
            piece_type: format!("{:?}", pt),
            samples: n,
            mean_mobility: mean_m,
            mean_attacks: mean_a,
            suggested_value: suggested,
        });
    }

    weights.rebuild_piece_value_table();
    let mut cp = EvalCheckpoint::seed("ab-mobility-seed");
    cp.weights = weights;
    cp.name = "ab-mobility-seed".into();

    if let Some(parent) = Path::new(&cfg.out_model).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create {}: {}", parent.display(), e))?;
    }
    cp.save_path(&cfg.out_model)
        .map_err(|e| format!("save model: {}", e))?;

    let _ = paths::ensure_data_dirs();
    Ok((cp, stats))
}
