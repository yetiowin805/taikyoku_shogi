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
fn abilities(piece: &Piece, version: u32) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for cap in &MovementConfig::for_piece(piece).capabilities {
        match cap {
            Cap::Simple {
                directions,
                max_distance,
            } => {
                // Version 1 encodes Flying Crocodile's historical lateral
                // diagonal masks. Its legal movement now uses forward/backward
                // diagonals; retain the trained inputs byte-for-byte until a
                // separately versioned feature schema replaces this encoding.
                let directions = match (piece.piece_type, *max_distance) {
                    (PieceType::FlyingCrocodile, 2) if version == 1 => 0xA0,
                    (PieceType::FlyingCrocodile, 3) if version == 1 => 0x0A,
                    _ => *directions,
                };
                ordinary(&mut out, directions, *max_distance);
            }
            Cap::Range {
                directions,
                blocking: BlockingMode::NoJump,
                ..
            } => {
                if piece.piece_type == PieceType::BuddhistSpirit {
                    // Version 1's trained representation used the old extra-
                    // step template. Keep its input channel stable after the
                    // legal move correction; this is not a movement capability.
                    out.insert("two:Range { directions: 255, blocking: NoJump, cannot_jump_over: {} }:Simple { directions: 255, max_distance: 1 }".into());
                } else {
                    ordinary(&mut out, *directions, 255);
                }
            }
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
                    // Version 1 reflected only rank for White jumps. Legal
                    // jumps now rotate both axes, but the trained encoding
                    // retains that historical quirk for checkpoint compatibility.
                    let x = if version == 1 && piece.color == Color::White {
                        -x
                    } else {
                        x
                    };
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
    if version == 2 {
        let variant = match piece.piece_type {
            PieceType::Whale if piece.base_piece_type == Some(PieceType::ReverseChariot) => {
                ":reverse-chariot"
            }
            PieceType::RainDragon if piece.is_promoted => ":promoted-movement",
            _ => "",
        };
        out.insert(format!("identity:{:?}{variant}", piece.piece_type));
        if !piece.is_promoted && piece.piece_type.promotes_to().is_some() {
            out.insert("promotion-available".into());
        }
    }
    out
}
pub(super) fn key(p: &Piece) -> usize {
    (p.piece_type as usize * 4
        + usize::from(p.is_promoted)
        + 2 * usize::from(p.base_piece_type == Some(PieceType::ReverseChariot)))
        * 2
        + usize::from(p.color == Color::White)
}

#[derive(Debug)]
pub struct Schema {
    pub version: u32,
    pub channels: usize,
    pub names: Vec<String>,
    pub hash: [u8; 32],
    pub(crate) pieces: Vec<Vec<usize>>,
    pub(crate) row_classes: Vec<usize>,
}
pub fn schema() -> &'static Schema {
    static SCHEMA: OnceLock<Schema> = OnceLock::new();
    SCHEMA.get_or_init(|| build(1))
}
pub fn schema_v2() -> &'static Schema {
    static SCHEMA: OnceLock<Schema> = OnceLock::new();
    SCHEMA.get_or_init(|| build(2))
}
pub fn by_version(version: u32) -> Result<&'static Schema, String> {
    match version {
        1 => Ok(schema()),
        2 => Ok(schema_v2()),
        _ => Err(format!("unsupported NNUE feature version: {version}")),
    }
}
fn build(version: u32) -> Schema {
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
        vocab.extend(abilities(p, version));
    }
    let channels = if version == 1 { CHANNELS } else { vocab.len() };
    assert!(vocab.len() <= channels);
    let names: Vec<_> = vocab.into_iter().collect();
    let ids: BTreeMap<_, _> = names
        .iter()
        .enumerate()
        .map(|(i, s)| (s.clone(), i))
        .collect();
    let mut mappings = vec![vec![]; pieces.iter().map(key).max().unwrap() + 1];
    for p in pieces {
        mappings[key(&p)] = abilities(&p, version).iter().map(|s| ids[s]).collect();
    }
    // Includes mappings, not just channel names: movement changes invalidate old nets.
    let data = serde_json::to_vec(&(version, channels, &names, &mappings)).unwrap();
    let hash = Sha256::digest(data).into();
    // Identical ability lists select identical weight rows regardless of
    // piece identity. Preserve the original schema hash and model format.
    let mut representatives = BTreeMap::new();
    let row_classes = mappings
        .iter()
        .enumerate()
        .map(|(i, abilities)| *representatives.entry(abilities.clone()).or_insert(i))
        .collect();
    Schema {
        version,
        channels,
        row_classes,
        names,
        hash,
        pieces: mappings,
    }
}
impl Schema {
    pub fn names_hash(&self) -> String {
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&self.names).unwrap())
        )
    }
    pub fn features(&self) -> usize {
        1296 * 2 * self.channels
    }
    pub fn visit(&self, piece: &Piece, perspective: Color, mut f: impl FnMut(usize)) {
        let flipped = perspective == Color::White;
        let square = if flipped {
            1295 - piece.position.to_index()
        } else {
            piece.position.to_index()
        };
        let color = usize::from(piece.color != perspective);
        for &a in &self.pieces[key(piece)] {
            f((color * 1296 + square) * self.channels + a);
        }
    }
    pub fn active(&self, board: &Board, perspective: Color) -> Vec<usize> {
        let mut out = Vec::new();
        for c in [Color::Black, Color::White] {
            for p in board.pieces_by_color(c) {
                self.visit(p, perspective, |i| out.push(i));
            }
        }
        out.sort_unstable();
        out
    }
}
pub fn visit(piece: &Piece, perspective: Color, f: impl FnMut(usize)) {
    schema().visit(piece, perspective, f);
}
pub fn active(board: &Board, perspective: Color) -> Vec<usize> {
    schema().active(board, perspective)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(p: &Piece) -> Vec<usize> {
        let mut result = Vec::new();
        schema_v2().visit(p, Color::Black, |i| result.push(i));
        result
    }

    #[test]
    fn identity_is_bound_to_square_and_remaining_promotion_right() {
        let gold = Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        );
        let mut wolf = gold;
        wolf.piece_type = PieceType::FierceWolf;
        assert_eq!(schema().pieces[key(&gold)], schema().pieces[key(&wolf)]);
        assert_ne!(rows(&gold), rows(&wolf));
        let mut a = Board::new();
        let mut b = Board::new();
        wolf.position = Position::new(11, 10).unwrap();
        a.place_piece(gold);
        a.place_piece(wolf);
        // Swapping equal movement types must change the position encoding.
        let mut moved_gold = gold;
        moved_gold.position = Position::new(11, 10).unwrap();
        wolf.position = gold.position;
        b.place_piece(moved_gold);
        b.place_piece(wolf);
        assert_eq!(
            schema().active(&a, Color::Black),
            schema().active(&b, Color::Black)
        );
        assert_ne!(
            schema_v2().active(&a, Color::Black),
            schema_v2().active(&b, Color::Black)
        );
        let rook = Piece::new(PieceType::Rook, Color::Black, gold.position);
        let mut promoted = rook;
        promoted.is_promoted = true;
        promoted.base_piece_type = Some(PieceType::FlyingSwallow);
        let expected = schema_v2()
            .names
            .iter()
            .position(|n| n == "promotion-available")
            .unwrap();
        let native = rows(&rook);
        let used = rows(&promoted);
        let difference: Vec<_> = native
            .iter()
            .filter(|i| !used.contains(i))
            .map(|i| i % schema_v2().channels)
            .collect();
        assert_eq!(difference, vec![expected]);
    }

    #[test]
    fn movement_variants_and_color_rotation_are_explicit() {
        for pt in [PieceType::Whale, PieceType::RainDragon] {
            let native = Piece::new(pt, Color::Black, Position::from_index(40).unwrap());
            let mut promoted = native;
            promoted.is_promoted = true;
            promoted.base_piece_type = Some(if pt == PieceType::Whale {
                PieceType::ReverseChariot
            } else {
                PieceType::EarthDragon
            });
            let identity = |p: &Piece| {
                schema_v2().pieces[key(p)]
                    .iter()
                    .filter_map(|i| {
                        let name = &schema_v2().names[*i];
                        name.starts_with("identity:").then_some(name)
                    })
                    .collect::<Vec<_>>()
            };
            assert_ne!(identity(&native), identity(&promoted));
        }
        for &pt in crate::eval::ALL_PIECE_TYPES {
            for promoted in [false, true] {
                let mut black = Piece::new(pt, Color::Black, Position::from_index(40).unwrap());
                black.is_promoted = promoted;
                let mut white = black;
                white.color = Color::White;
                white.position = Position::from_index(1295 - 40).unwrap();
                let mut mirrored = Vec::new();
                schema_v2().visit(&white, Color::White, |i| mirrored.push(i));
                assert_eq!(rows(&black), mirrored, "{pt:?}, promoted={promoted}");
            }
        }
    }
}
