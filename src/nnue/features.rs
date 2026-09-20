//! Versioned movement-ability features shared by inference and data export.
use crate::{
    board::Board,
    movement::{BlockingMode, MovementCapability as Cap, MovementConfig},
    piece::{Color, Piece, PieceType},
    position::Position,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};
pub const CHANNELS: usize = 92;
pub const FEATURES: usize = 36 * 36 * 2 * CHANNELS;
fn ordinary(out: &mut BTreeSet<String>, dirs: u8, distance: u8) {
    assert!([1, 2, 3, 4, 5, 7, 255].contains(&distance));
    for d in 0..8 {
        if dirs & (1 << d) != 0 {
            out.insert(format!("ordinary:{distance}:{d}"));
        }
    }
}
fn abilities(piece: &Piece) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for cap in &MovementConfig::for_piece(piece).capabilities {
        match cap {
            Cap::Simple {
                directions,
                max_distance,
            } => ordinary(&mut out, *directions, *max_distance),
            Cap::Range {
                directions,
                blocking: BlockingMode::NoJump,
                ..
            } => ordinary(&mut out, *directions, 255),
            Cap::Range {
                directions,
                blocking: BlockingMode::Jump,
                ..
            } => {
                ordinary(&mut out, *directions, 255);
                out.insert(format!("jump-ray:{directions}"));
            }
            Cap::Range {
                directions,
                cannot_jump_over,
                ..
            } => {
                let mut blockers: Vec<_> =
                    cannot_jump_over.iter().map(|p| format!("{p:?}")).collect();
                blockers.sort();
                out.insert(format!("capture-ray:{directions}:{blockers:?}"));
            }
            Cap::Jumping { offsets } => {
                for &(x, y) in offsets {
                    // The actual engine flips only rank for White jumps, unlike
                    // its 180-degree ordinary directions. Preserve that quirk
                    // in the piece-relative ability encoding.
                    let x = if piece.color == Color::White { -x } else { x };
                    let (ax, ay) = (x.abs(), y.abs());
                    if ax.max(ay) == 1 {
                        let d = [
                            (0, 1),
                            (1, 1),
                            (1, 0),
                            (1, -1),
                            (0, -1),
                            (-1, -1),
                            (-1, 0),
                            (-1, 1),
                        ]
                        .iter()
                        .position(|&p| p == (x, y))
                        .unwrap();
                        ordinary(&mut out, 1 << d, 1);
                    } else {
                        let key = if ax == 1 && ay == 2 {
                            format!("knight:forward:{}", y.signum())
                        } else if ax == 2 && ay == 1 {
                            "knight:sideways".to_string()
                        } else if y == 0 {
                            format!("jump:side:{ax}")
                        } else if ax == 3 && ay == 3 {
                            format!("jump:diagonal3:{}", y.signum())
                        } else {
                            format!("jump:{x}:{y}")
                        };
                        out.insert(key);
                    }
                }
            }
            // These templates contain only Simple/Range legs with stable Debug output.
            Cap::TwoStep { first, second } => {
                out.insert(format!("two:{first:?}:{second:?}"));
            }
            Cap::ConditionalDiagonalJump { .. } => {
                out.insert("wooden-dove".into());
            }
            Cap::FreeEagleMultiMove { .. } => {
                out.insert("free-eagle".into());
            }
        }
    }
    out.insert("occupancy".into());
    if piece.piece_type.is_royal() {
        out.insert("royal".into());
    }
    out
}
fn key(p: &Piece) -> usize {
    (p.piece_type as usize * 4
        + usize::from(p.is_promoted)
        + 2 * usize::from(p.base_piece_type == Some(PieceType::ReverseChariot)))
        * 2
        + usize::from(p.color == Color::White)
}

#[derive(Debug)]
pub struct Schema {
    pub names: Vec<String>,
    pub hash: [u8; 32],
    pieces: Vec<Vec<usize>>,
}
pub fn schema() -> &'static Schema {
    static SCHEMA: OnceLock<Schema> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        let mut pieces = Vec::new();
        for &pt in crate::eval::ALL_PIECE_TYPES {
            for promoted in [false, true] {
                for reverse in [false, true] {
                    let mut p = Piece::new(pt, Color::Black, Position::from_index(0).unwrap());
                    p.is_promoted = promoted;
                    if reverse {
                        p.base_piece_type = Some(PieceType::ReverseChariot);
                    }
                    pieces.push(p);
                    p.color = Color::White;
                    pieces.push(p);
                }
            }
        }
        let mut vocab = BTreeSet::new();
        for p in &pieces {
            vocab.extend(abilities(p));
        }
        assert!(vocab.len() <= CHANNELS);
        let names: Vec<_> = vocab.into_iter().collect();
        let ids: BTreeMap<_, _> = names
            .iter()
            .enumerate()
            .map(|(i, s)| (s.clone(), i))
            .collect();
        let mut mappings = vec![vec![]; pieces.iter().map(key).max().unwrap() + 1];
        for p in pieces {
            mappings[key(&p)] = abilities(&p).iter().map(|s| ids[s]).collect();
        }
        // Includes mappings, not just channel names: movement changes invalidate old nets.
        let data = serde_json::to_vec(&(1, CHANNELS, &names, &mappings)).unwrap();
        let hash = Sha256::digest(data).into();
        Schema {
            names,
            hash,
            pieces: mappings,
        }
    })
}
pub fn visit(piece: &Piece, perspective: Color, mut f: impl FnMut(usize)) {
    let flipped = perspective == Color::White;
    let square = if flipped {
        1295 - piece.position.to_index()
    } else {
        piece.position.to_index()
    };
    let color = usize::from(piece.color != perspective);
    for &a in &schema().pieces[key(piece)] {
        f((color * 1296 + square) * CHANNELS + a);
    }
}
pub fn active(board: &Board, perspective: Color) -> Vec<usize> {
    let mut out = Vec::new();
    for c in [Color::Black, Color::White] {
        for p in board.pieces_by_color(c) {
            visit(p, perspective, |i| out.push(i));
        }
    }
    out.sort_unstable();
    out
}
