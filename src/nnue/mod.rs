//! Versioned, content-verified NNUE residual on a fixed material baseline.
pub mod features;
mod storage;
mod packed;
pub(crate) use storage::Snapshot;
use crate::{
    board::Board,
    piece::{Color, Piece},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::Path,
    sync::{Arc, Mutex, OnceLock, Weak},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    pub file: String,
    pub sha256: String,
    pub width: usize,
    pub feature_hash: String,
}
pub struct Network {
    pub width: usize,
    pub sha256: String,
    weights: Vec<i16>,
    bias: Vec<i32>,
    h1: Vec<i8>,
    b1: Vec<i32>,
    h2: Vec<i8>,
    b2: Vec<i32>,
    out: Vec<f32>,
    out_bias: f32,
}
impl std::fmt::Debug for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NNUE")
            .field("width", &self.width)
            .field("sha256", &self.sha256)
            .finish()
    }
}
fn bytes<const N: usize>(r: &mut impl Read) -> Result<[u8; N], String> {
    let mut b = [0; N];
    r.read_exact(&mut b).map_err(|e| e.to_string())?;
    Ok(b)
}
fn u32le(r: &mut impl Read) -> Result<u32, String> {
    Ok(u32::from_le_bytes(bytes(r)?))
}
fn i32s(r: &mut impl Read, n: usize) -> Result<Vec<i32>, String> {
    (0..n).map(|_| Ok(i32::from_le_bytes(bytes(r)?))).collect()
}
fn i8s(r: &mut impl Read, n: usize) -> Result<Vec<i8>, String> {
    let mut v = vec![0; n];
    r.read_exact(&mut v).map_err(|e| e.to_string())?;
    Ok(v.into_iter().map(|b| b as i8).collect())
}
fn f32s(r: &mut impl Read, n: usize) -> Result<Vec<f32>, String> {
    (0..n)
        .map(|_| {
            let f = f32::from_le_bytes(bytes(r)?);
            if !f.is_finite() {
                Err("nonfinite NNUE parameter".into())
            } else {
                Ok(f)
            }
        })
        .collect()
}
pub fn hash_file(path: &Path) -> Result<String, String> {
    let mut r = BufReader::new(File::open(path).map_err(|e| e.to_string())?);
    let mut h = Sha256::new();
    let mut b = [0; 65536];
    loop {
        let n = r.read(&mut b).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}
impl Network {
    pub fn load(checkpoint: &Path, d: &Descriptor) -> Result<Arc<Self>, String> {
        if d.width < 32 || d.width > 4096 || d.width % 32 != 0 {
            return Err("invalid NNUE width".into());
        }
        if d.feature_hash
            != format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&features::schema().names).unwrap())
            )
        {
            return Err("NNUE descriptor feature names differ".into());
        }
        let path = checkpoint
            .parent()
            .unwrap_or(Path::new("."))
            .join(&d.file)
            .canonicalize()
            .map_err(|e| format!("NNUE blob {}: {e}", d.file))?;
        static CACHE: OnceLock<Mutex<HashMap<(std::path::PathBuf, String), Weak<Network>>>> =
            OnceLock::new();
        let mut cache = CACHE
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| "NNUE cache poisoned")?;
        cache.retain(|_, v| v.strong_count() > 0);
        let key = (path.clone(), d.sha256.clone());
        if let Some(n) = cache.get(&key).and_then(Weak::upgrade) {
            return Ok(n);
        }
        // Hash and parse the same open file: an atomic path replacement cannot
        // mix one checkpoint's identity with another checkpoint's bytes.
        let mut r = BufReader::new(File::open(&path).map_err(|e| e.to_string())?);
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let n = r.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        if format!("{:x}", hasher.finalize()) != d.sha256 {
            return Err("NNUE content hash mismatch".into());
        }
        r.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        if &bytes::<8>(&mut r)? != b"TKNNUE01" {
            return Err("invalid NNUE magic".into());
        }
        if u32le(&mut r)? != 1
            || u32le(&mut r)? as usize != d.width
            || u32le(&mut r)? as usize != features::CHANNELS
        {
            return Err("unsupported NNUE header".into());
        }
        if bytes::<32>(&mut r)? != features::schema().hash {
            return Err("NNUE feature schema mismatch".into());
        }
        if u32le(&mut r)? != 4096 || u32le(&mut r)? != 64 || u32le(&mut r)? != 1 {
            return Err("unsupported NNUE quantization or baseline".into());
        }
        let w = d.width;
        let expected = 64
            + features::FEATURES * w * 2
            + w * 4
            + 2 * w * 32
            + 32 * 4
            + 32 * 32
            + 32 * 4
            + 33 * 4;
        if r.get_ref().metadata().map_err(|e| e.to_string())?.len() != expected as u64 {
            return Err("NNUE byte length mismatch".into());
        }
        let mut weights = vec![0i16; features::FEATURES * w];
        let mut buf = [0u8; 8192];
        for chunk in weights.chunks_mut(4096) {
            let n = chunk.len() * 2;
            r.read_exact(&mut buf[..n]).map_err(|e| e.to_string())?;
            for (v, b) in chunk.iter_mut().zip(buf[..n].chunks_exact(2)) {
                *v = i16::from_le_bytes([b[0], b[1]]);
            }
        }
        let net = Arc::new(Self {
            width: w,
            sha256: d.sha256.clone(),
            weights,
            bias: i32s(&mut r, w)?,
            h1: i8s(&mut r, 2 * w * 32)?,
            b1: i32s(&mut r, 32)?,
            h2: i8s(&mut r, 32 * 32)?,
            b2: i32s(&mut r, 32)?,
            out: f32s(&mut r, 32)?,
            out_bias: f32s(&mut r, 1)?[0],
        });
        // Accumulator arithmetic stays bounded even on a completely occupied board.
        if net.weights.iter().any(|&x| i32::from(x).abs() > 2048)
            || net.bias.iter().any(|x| x.unsigned_abs() > 1_000_000)
            || net
                .b1
                .iter()
                .chain(&net.b2)
                .any(|x| x.unsigned_abs() > 100_000_000)
        {
            return Err("NNUE quantized parameter out of bounds".into());
        }
        cache.insert(key, Arc::downgrade(&net));
        Ok(net)
    }
}
#[derive(Debug, Clone)]
pub struct Accumulator {
    pub net: Arc<Network>,
    sums: Vec<i32>,
    storage: storage::Storage,
}
thread_local! {static INPUT:std::cell::RefCell<Vec<u8>>=const{std::cell::RefCell::new(Vec::new())};}
impl Accumulator {
    pub fn new(net: Arc<Network>, board: &Board) -> Self {
        let mut sums = net.bias.clone();
        sums.extend_from_slice(&net.bias);
        let mut a = Self { net, sums, storage: storage::Storage::default() };
        for c in [Color::Black, Color::White] {
            for p in board.pieces_by_color(c) {
                a.change(p, 1);
            }
        }
        a
    }
    pub fn matches(&self, net: &Arc<Network>) -> bool {
        Arc::ptr_eq(&self.net, net)
    }
    pub fn change(&mut self, p: &Piece, sign: i32) {
        self.change_cached(p, sign);
    }
    pub fn residual(&self, stm: Color) -> i32 {
        let w = self.net.width;
        let first = usize::from(stm == Color::White);
        INPUT.with(|input| {
            let mut x = input.borrow_mut();
            x.resize(2 * w, 0);
            for side in 0..2 {
                for (d, &a) in x[side * w..(side + 1) * w]
                    .iter_mut()
                    .zip(&self.sums[(first ^ side) * w..((first ^ side) + 1) * w])
                {
                    *d = ((i64::from(a) * 127 + 2048) / 4096).clamp(0, 127) as u8;
                }
            }
            let mut h = [0i32; 32];
            for (j, y) in h.iter_mut().enumerate() {
                let sum = packed::dot(&x, &self.net.h1[j * 2 * w..(j + 1) * 2 * w]);
                *y = ((sum + self.net.b1[j] + 32) / 64).clamp(0, 127);
            }
            let mut result = self.net.out_bias;
            for j in 0..32 {
                let sum: i32 = h
                    .iter()
                    .zip(&self.net.h2[j * 32..(j + 1) * 32])
                    .map(|(&a, &b)| a * i32::from(b))
                    .sum();
                result += ((sum + self.net.b2[j] + 32) / 64).clamp(0, 127) as f32 / 127.
                    * self.net.out[j];
            }
            (result * 1000.).round().clamp(-100_000., 100_000.) as i32
        })
    }
    pub fn matches_rebuild(&self, board: &Board) -> bool {
        self.sums == Self::new(self.net.clone(), board).sums
    }
}
pub fn material(board: &Board, weights: &crate::eval::EvalWeights) -> f32 {
    let mut sums = [0f32; 2];
    for (i, c) in [Color::Black, Color::White].into_iter().enumerate() {
        for p in board.pieces_by_color(c) {
            sums[i] += crate::eval::material_piece_value(p, weights);
        }
    }
    sums[0] - sums[1]
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        eval::{self, EvalWeights},
        game_state::{GameState, Move},
        piece::PieceType,
        position::Position,
        search::{search, SearchConfig},
    };
    pub(crate) fn net(width: usize, tag: &str) -> Arc<Network> {
        Arc::new(Network {
            width,
            sha256: tag.into(),
            weights: (0..features::FEATURES * width)
                .map(|i| (i % 13) as i16 - 6)
                .collect(),
            bias: vec![2048; width],
            h1: (0..2 * width * 32).map(|i| (i % 7) as i8 - 3).collect(),
            b1: vec![1024; 32],
            h2: vec![1; 1024],
            b2: vec![1024; 32],
            out: vec![0.02; 32],
            out_bias: -0.1,
        })
    }
    fn state() -> GameState {
        let mut s = GameState::new();
        for (pt, c, x, y) in [
            (PieceType::King, Color::Black, 0, 0),
            (PieceType::King, Color::White, 35, 35),
            (PieceType::GreatGeneral, Color::Black, 5, 5),
            (PieceType::Pawn, Color::Black, 5, 6),
            (PieceType::Pawn, Color::White, 5, 7),
            (PieceType::Pawn, Color::White, 5, 8),
        ] {
            s.place_piece(Piece::new(pt, c, Position::new(x, y).unwrap()));
        }
        s
    }
    #[test]
    fn deltas_promotions_null_clone_and_model_switch() {
        let first_net = net(32, "a");
        let mut board = Board::new();
        for (i, &pt) in eval::ALL_PIECE_TYPES.iter().enumerate() {
            board.place_piece(Piece::new(
                pt,
                if i % 2 == 0 {
                    Color::Black
                } else {
                    Color::White
                },
                Position::from_index(i).unwrap(),
            ));
        }
        let mut acc = Accumulator::new(first_net.clone(), &board);
        for c in [Color::Black, Color::White] {
            for old in board.pieces_by_color(c).to_vec() {
                let mut new = old;
                new.promote();
                acc.change(&old, -1);
                board.remove_piece(old.position);
                acc.change(&new, 1);
                board.place_piece(new);
                assert!(acc.matches_rebuild(&board));
            }
        }
        let mut s = state();
        let mut w = EvalWeights::seed();
        w.nnue_runtime = Some(first_net.clone());
        let _bind = eval::bind_search_weights(&w);
        s.ensure_eval_inc(&w);
        let old = s.nnue.clone().unwrap();
        let u = s
            .make_move_for_search(Move::new(
                Position::new(5, 5).unwrap(),
                Position::new(5, 8).unwrap(),
            ))
            .unwrap();
        assert!(s.nnue.as_ref().unwrap().matches_rebuild(s.get_board()));
        let clone = s.clone();
        assert!(clone
            .nnue
            .as_ref()
            .unwrap()
            .matches_rebuild(clone.get_board()));
        s.unmake_move_for_search(u);
        assert_eq!(old.sums, s.nnue.as_ref().unwrap().sums);
        s.set_current_turn(Color::White);
        assert!(s.nnue.as_ref().unwrap().matches_rebuild(s.get_board()));
        s.set_current_turn(Color::Black);
        w.nnue_runtime = Some(net(64, "b"));
        s.ensure_eval_inc(&w);
        assert_eq!(s.nnue.as_ref().unwrap().net.width, 64);
        w.nnue_runtime = None;
        s.ensure_eval_inc(&w);
        assert!(s.nnue.is_none());
    }
    #[test]
    fn score_replaces_handcrafted_terms_but_preserves_rules_and_timeout() {
        let mut s = state();
        let mut w = EvalWeights::seed();
        w.noise_scale = 0.;
        w.nnue_runtime = Some(net(32, "eval"));
        s.ensure_eval_inc(&w);
        let expected = material(s.get_board(), &w).round() as i32
            + s.nnue.as_ref().unwrap().residual(Color::Black);
        assert_eq!(eval::evaluate(&s, &w), expected);
        w.two_mover_align_k = 999.;
        w.lr_flight_k = 999.;
        assert_eq!(eval::evaluate(&s, &w), expected);
        let cfg = SearchConfig {
            depth: 8,
            max_time_ms: Some(2),
            collect_trace: false,
            ..Default::default()
        };
        let _ = search(&s, &w, &cfg);
        assert!(s.nnue.as_ref().unwrap().matches_rebuild(s.get_board()));
        s.remove_piece(Position::new(35, 35).unwrap());
        assert_eq!(eval::evaluate(&s, &w), w.mate_score);
        assert!(s.nnue.is_none());
    }
    #[test]
    fn undo_after_model_rebinding_invalidates_the_old_snapshot() {
        for initially_bound in [false, true] {
            let mut s = state();
            let mut w = EvalWeights::seed();
            if initially_bound {
                w.nnue_runtime = Some(net(32, "before"));
                s.ensure_eval_inc(&w);
            }
            let undo = s.make_move_for_search(Move::new(
                Position::new(5, 5).unwrap(), Position::new(5, 8).unwrap(),
            )).unwrap();
            w.nnue_runtime = Some(net(64, "after"));
            s.ensure_eval_inc(&w);
            s.unmake_move_for_search(undo);
            assert!(s.nnue.is_none());
            let score = eval::evaluate(&s, &w);
            s.ensure_eval_inc(&w);
            assert!(s.nnue.as_ref().unwrap().matches_rebuild(s.get_board()));
            assert_eq!(score, eval::evaluate(&s, &w));
        }
    }
    #[test]
    fn schema_stable_and_color_rotation_equivalent() {
        assert_eq!(features::schema().names.len(), 85);
        let mut a = Board::new();
        let mut b = Board::new();
        for (i, &pt) in [PieceType::King, PieceType::Pawn, PieceType::Rook]
            .iter()
            .enumerate()
        {
            let c = if i % 2 == 0 {
                Color::Black
            } else {
                Color::White
            };
            a.place_piece(Piece::new(pt, c, Position::from_index(i).unwrap()));
            b.place_piece(Piece::new(
                pt,
                c.opposite(),
                Position::from_index(1295 - i).unwrap(),
            ));
        }
        assert_eq!(
            features::active(&a, Color::Black),
            features::active(&b, Color::White)
        );
        // Asymmetric jumping pieces intentionally do NOT obey that rotation in
        // the current rules implementation (White's jump file is not flipped).
        let p = Piece::new(
            PieceType::LeftMountainEagle,
            Color::Black,
            Position::new(10, 10).unwrap(),
        );
        let q = Piece::new(
            PieceType::LeftMountainEagle,
            Color::White,
            Position::new(25, 25).unwrap(),
        );
        let mut x = Vec::new();
        let mut y = Vec::new();
        features::visit(&p, Color::Black, |i| x.push(i));
        features::visit(&q, Color::White, |i| y.push(i));
        assert_ne!(x, y);
    }
    #[test]
    fn checkpoint_hash_length_and_schema_validation() {
        use std::io::{BufWriter, Write};
        let dir = std::env::temp_dir().join(format!("taikyoku-nnue-load-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("net.bin");
        let n = net(32, "fixture");
        let mut f = BufWriter::new(File::create(&path).unwrap());
        f.write_all(b"TKNNUE01").unwrap();
        for v in [1u32, 32, 92] {
            f.write_all(&v.to_le_bytes()).unwrap();
        }
        f.write_all(&features::schema().hash).unwrap();
        for v in [4096u32, 64, 1] {
            f.write_all(&v.to_le_bytes()).unwrap();
        }
        for v in &n.weights {
            f.write_all(&v.to_le_bytes()).unwrap();
        }
        for v in &n.bias {
            f.write_all(&v.to_le_bytes()).unwrap();
        }
        for (w, b) in [(&n.h1, &n.b1), (&n.h2, &n.b2)] {
            f.write_all(&w.iter().map(|&x| x as u8).collect::<Vec<_>>())
                .unwrap();
            for v in b {
                f.write_all(&v.to_le_bytes()).unwrap();
            }
        }
        for v in n.out.iter().chain(std::iter::once(&n.out_bias)) {
            f.write_all(&v.to_le_bytes()).unwrap();
        }
        f.flush().unwrap();
        drop(f);
        let d = Descriptor {
            file: "net.bin".into(),
            sha256: hash_file(&path).unwrap(),
            width: 32,
            feature_hash: format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&features::schema().names).unwrap())
            ),
        };
        let loaded = Network::load(&dir.join("model.json"), &d).unwrap();
        assert_eq!(loaded.weights, n.weights);
        drop(loaded);
        let mut wrong = d.clone();
        wrong.sha256 = "wrong".into();
        assert!(Network::load(&dir.join("model.json"), &wrong)
            .unwrap_err()
            .contains("hash mismatch"));
        wrong = d.clone();
        wrong.feature_hash = "wrong".into();
        assert!(Network::load(&dir.join("model.json"), &wrong).is_err());
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(&[0]).unwrap();
        drop(f);
        wrong = d.clone();
        wrong.sha256 = hash_file(&path).unwrap();
        assert!(Network::load(&dir.join("model.json"), &wrong)
            .unwrap_err()
            .contains("length mismatch"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
