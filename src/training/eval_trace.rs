//! Quiet-biased eval trajectories: swing metrics, jump attribution, interesting ranking.

use crate::eval::{EvalCheckpoint, EvalWeights, ALL_PIECE_TYPES};
use crate::game_history::GameResult;
use crate::game_state::GameState;
use crate::piece::{Color, PieceType};
use crate::position::Position;
use crate::search::{search, SearchConfig};
use crate::training::featurize::move_was_capture;
use crate::training::paths::{self, ensure_data_dirs};
use crate::training::record::{AgentSpec, GameRecordV2, GameStart};
use crate::eval::is_big_piece;
use crate::training::worker::replay_to_ply;
use crate::game_history::MoveRecord;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_QUIET_STRIDE: usize = 16;
pub const DEFAULT_TOP_N: usize = 30;
pub const DEFAULT_EVAL_CLIP: f32 = 50_000.0;
pub const DEFAULT_OUT_DIR: &str = "data/derived/eval_traces";
pub const DEFAULT_SEARCH_DEPTH: u32 = 2;
pub const DEFAULT_SEARCH_STRIDE: usize = 5;
pub const TRACE_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct EvalTraceConfig {
    pub games_dir: PathBuf,
    pub out_dir: PathBuf,
    pub model_path: PathBuf,
    pub quiet_stride: usize,
    pub top_n: usize,
    pub eval_clip: f32,
    /// Zero eval noise for stable traces.
    pub zero_noise: bool,
    pub max_games: Option<usize>,
    /// Depth-2 (default) probes every `search_stride` plies on top-N games.
    pub search_depth: u32,
    pub search_stride: usize,
    pub skip_search: bool,
}

impl Default for EvalTraceConfig {
    fn default() -> Self {
        Self {
            games_dir: PathBuf::from(paths::RAW_GAMES),
            out_dir: PathBuf::from(DEFAULT_OUT_DIR),
            model_path: PathBuf::from("models/ab-seed.json"),
            quiet_stride: DEFAULT_QUIET_STRIDE,
            top_n: DEFAULT_TOP_N,
            eval_clip: DEFAULT_EVAL_CLIP,
            zero_noise: true,
            max_games: None,
            search_depth: DEFAULT_SEARCH_DEPTH,
            search_stride: DEFAULT_SEARCH_STRIDE,
            skip_search: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracePoint {
    pub ply: usize,
    pub quiet: bool,
    pub capture_into: bool,
    pub royal_change: bool,
    /// Promotion that created a two-mover / range-capturer (e.g. FreeKing→GG).
    #[serde(default)]
    pub loud_promotion: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loud_promotion_to: Option<String>,
    pub turn: String,
    pub eval: f32,
    pub eval_clipped: f32,
    pub material_only: f32,
    pub black_royals: usize,
    pub white_royals: usize,
    pub black_pieces: usize,
    pub white_pieces: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JumpAttribution {
    pub from_ply: usize,
    pub to_ply: usize,
    pub delta_eval: f32,
    pub delta_material: f32,
    pub residual: f32,
    pub black_royals: (usize, usize),
    pub white_royals: (usize, usize),
    pub top_piece_deltas: Vec<(String, i32)>,
    pub capture_plies_between: Vec<usize>,
    #[serde(default)]
    pub loud_promo_plies_between: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameMetrics {
    pub max_drawdown: f32,
    pub max_runup: f32,
    pub sign_flips: usize,
    pub late_reversal: bool,
    pub result_disagreement: f32,
    pub volatility: f32,
    pub interestingness: f32,
    pub n_quiet: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchProbe {
    pub ply: usize,
    pub turn: String,
    /// Depth-N score with Black's model, Black-absolute (positive = Black better).
    pub depth_black_model: f32,
    /// Depth-N score with White's model, Black-absolute.
    pub depth_white_model: f32,
    pub best_move_black_model: Option<String>,
    pub best_move_white_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameTrace {
    pub format_version: u32,
    pub game_id: String,
    pub game_path: String,
    pub result: String,
    pub model: String,
    pub points: Vec<TracePoint>,
    pub jumps: Vec<JumpAttribution>,
    pub metrics: GameMetrics,
    #[serde(default)]
    pub search_probes: Vec<SearchProbe>,
    /// Suggested plies for GUI scrubbing (big static jumps + search extrema).
    #[serde(default)]
    pub focus_plies: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryRow {
    pub game_id: String,
    pub game_path: String,
    pub result: String,
    pub metrics: GameMetrics,
    pub top_jump: Option<JumpAttribution>,
    pub trace_path: String,
}

fn initial_state(start: &GameStart) -> GameState {
    match start {
        GameStart::Opening => {
            let mut state = GameState::new();
            state.setup_initial_position();
            state
        }
        GameStart::Position { position } => position.to_state(),
    }
}

fn result_str(r: &Option<GameResult>) -> String {
    match r {
        Some(GameResult::BlackWins) => "black_wins".into(),
        Some(GameResult::WhiteWins) => "white_wins".into(),
        Some(GameResult::Draw) => "draw".into(),
        None => "unknown".into(),
    }
}

fn winner_sign(r: &Option<GameResult>) -> Option<f32> {
    match r {
        Some(GameResult::BlackWins) => Some(1.0),
        Some(GameResult::WhiteWins) => Some(-1.0),
        _ => None,
    }
}

fn clip(v: f32, cap: f32) -> f32 {
    v.clamp(-cap, cap)
}

fn count_royals(state: &GameState, color: Color) -> usize {
    state
        .get_board()
        .get_pieces_by_color(color)
        .iter()
        .filter(|p| p.piece_type.is_royal())
        .count()
}

fn material_only_black(state: &GameState, weights: &EvalWeights) -> f32 {
    let board = state.get_board();
    let mut s = 0.0f32;
    for p in board.get_pieces_by_color(Color::Black) {
        s += weights.piece_value(p.piece_type);
    }
    for p in board.get_pieces_by_color(Color::White) {
        s -= weights.piece_value(p.piece_type);
    }
    s
}

fn piece_counts(state: &GameState) -> Vec<i32> {
    let board = state.get_board();
    let mut c = vec![0i32; ALL_PIECE_TYPES.len()];
    for (i, &pt) in ALL_PIECE_TYPES.iter().enumerate() {
        for p in board.get_pieces_by_color(Color::Black) {
            if p.piece_type == pt {
                c[i] += 1;
            }
        }
        for p in board.get_pieces_by_color(Color::White) {
            if p.piece_type == pt {
                c[i] -= 1;
            }
        }
    }
    c
}

fn snapshot_point(
    state: &GameState,
    ply: usize,
    weights: &EvalWeights,
    clip_cap: f32,
    quiet: bool,
    capture_into: bool,
    royal_change: bool,
    loud_promotion: bool,
    loud_promotion_to: Option<String>,
) -> TracePoint {
    let eval = crate::eval::evaluate_absolute_black(state.get_board(), weights, ply) as f32;
    TracePoint {
        ply,
        quiet,
        capture_into,
        royal_change,
        loud_promotion,
        loud_promotion_to,
        turn: format!("{:?}", state.get_current_turn()),
        eval,
        eval_clipped: clip(eval, clip_cap),
        material_only: material_only_black(state, weights),
        black_royals: count_royals(state, Color::Black),
        white_royals: count_royals(state, Color::White),
        black_pieces: state.get_board().get_pieces_by_color(Color::Black).len(),
        white_pieces: state.get_board().get_pieces_by_color(Color::White).len(),
    }
}

/// True when this recorded move promotes into a two-mover / range-capturer.
pub fn move_is_loud_promotion(state_before: &GameState, mr: &MoveRecord) -> Option<PieceType> {
    if !mr.promoted {
        return None;
    }
    let from = Position::new(mr.from_file, mr.from_rank)?;
    let piece = state_before.get_board().get_piece(from)?;
    let to = piece.piece_type.promotes_to()?;
    if is_big_piece(to) {
        Some(to)
    } else {
        None
    }
}

/// Select sample plies: quiet stride + forced events (royals, loud promos).
/// Captures are not primary samples (unless also a forced event).
pub fn select_quiet_plies(
    capture_into: &[bool],
    royal_change: &[bool],
    loud_promotion: &[bool],
    quiet_stride: usize,
) -> Vec<usize> {
    let n = capture_into.len();
    if n == 0 {
        return Vec::new();
    }
    let stride = quiet_stride.max(1);
    let mut out = Vec::new();
    let mut last_quiet = 0usize;
    for ply in 0..n {
        let is_capture = capture_into[ply];
        let is_event = royal_change[ply] || loud_promotion.get(ply).copied().unwrap_or(false);
        let quiet_ok = !is_capture && (ply == 0 || ply - last_quiet >= stride);
        if ply == 0 || ply + 1 == n || is_event || quiet_ok {
            out.push(ply);
            if !is_capture || is_event {
                last_quiet = ply;
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

pub fn compute_metrics(
    quiet: &[(usize, f32)],
    result: &Option<GameResult>,
) -> (GameMetrics, f32, f32) {
    // returns metrics plus raw max_dd and vol for z-scoring later
    if quiet.is_empty() {
        return (
            GameMetrics {
                max_drawdown: 0.0,
                max_runup: 0.0,
                sign_flips: 0,
                late_reversal: false,
                result_disagreement: 0.0,
                volatility: 0.0,
                interestingness: 0.0,
                n_quiet: 0,
            },
            0.0,
            0.0,
        );
    }
    let vals: Vec<f32> = quiet.iter().map(|(_, e)| *e).collect();
    let mut peak = vals[0];
    let mut trough = vals[0];
    let mut max_dd = 0.0f32;
    let mut max_ru = 0.0f32;
    for &v in &vals {
        peak = peak.max(v);
        trough = trough.min(v);
        max_dd = max_dd.max(peak - v);
        max_ru = max_ru.max(v - trough);
    }
    let mut flips = 0usize;
    for w in vals.windows(2) {
        if w[0] == 0.0 || w[1] == 0.0 {
            continue;
        }
        if w[0].signum() != w[1].signum() {
            flips += 1;
        }
    }
    let n = vals.len();
    let mid = n / 2;
    let early_med = median(&vals[..mid.max(1)]);
    let late_start = (n * 3) / 4;
    let late_med = median(&vals[late_start.min(n - 1)..]);
    let late_reversal = early_med != 0.0
        && late_med != 0.0
        && early_med.signum() != late_med.signum();

    let mut disagree = 0.0f32;
    if let Some(ws) = winner_sign(result) {
        let mut bad = 0usize;
        let mut tot = 0usize;
        for &v in &vals {
            if v.abs() < 1e-3 {
                continue;
            }
            tot += 1;
            if v.signum() != ws {
                bad += 1;
            }
        }
        disagree = if tot == 0 {
            0.0
        } else {
            bad as f32 / tot as f32
        };
    } else {
        // Draws: down-weight disagreement term by leaving it near 0
        disagree = 0.0;
    }

    let mut deltas = Vec::new();
    for w in vals.windows(2) {
        deltas.push((w[1] - w[0]).abs());
    }
    let volatility = if deltas.is_empty() {
        0.0
    } else {
        median(&deltas)
    };

    let metrics = GameMetrics {
        max_drawdown: max_dd,
        max_runup: max_ru,
        sign_flips: flips,
        late_reversal,
        result_disagreement: disagree,
        volatility,
        interestingness: 0.0, // filled after z-score
        n_quiet: n,
    };
    (metrics, max_dd.max(max_ru), volatility)
}

fn median(xs: &[f32]) -> f32 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let m = v.len() / 2;
    if v.len() % 2 == 0 {
        (v[m - 1] + v[m]) * 0.5
    } else {
        v[m]
    }
}

fn mean_std(xs: &[f32]) -> (f32, f32) {
    if xs.is_empty() {
        return (0.0, 1.0);
    }
    let mean = xs.iter().sum::<f32>() / xs.len() as f32;
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / xs.len() as f32;
    let std = var.sqrt().max(1e-3);
    (mean, std)
}

fn z(x: f32, mean: f32, std: f32) -> f32 {
    (x - mean) / std
}

pub fn assign_interestingness(rows: &mut [SummaryRow]) {
    let dds: Vec<f32> = rows
        .iter()
        .map(|r| r.metrics.max_drawdown.max(r.metrics.max_runup))
        .collect();
    let revs: Vec<f32> = rows
        .iter()
        .map(|r| if r.metrics.late_reversal { 1.0 } else { 0.0 })
        .collect();
    let dis: Vec<f32> = rows
        .iter()
        .map(|r| r.metrics.result_disagreement)
        .collect();
    let vols: Vec<f32> = rows.iter().map(|r| r.metrics.volatility).collect();
    let (md, sd) = mean_std(&dds);
    let (mr, sr) = mean_std(&revs);
    let (mdi, sdi) = mean_std(&dis);
    let (mv, sv) = mean_std(&vols);
    for r in rows.iter_mut() {
        let dd = r.metrics.max_drawdown.max(r.metrics.max_runup);
        let rev = if r.metrics.late_reversal { 1.0 } else { 0.0 };
        r.metrics.interestingness = z(dd, md, sd)
            + z(rev, mr, sr)
            + z(r.metrics.result_disagreement, mdi, sdi)
            + 0.5 * z(r.metrics.volatility, mv, sv);
    }
    rows.sort_by(|a, b| {
        b.metrics
            .interestingness
            .partial_cmp(&a.metrics.interestingness)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

fn top_piece_deltas(before: &[i32], after: &[i32], k: usize) -> Vec<(String, i32)> {
    let mut deltas: Vec<(PieceType, i32)> = ALL_PIECE_TYPES
        .iter()
        .enumerate()
        .map(|(i, &pt)| (pt, after[i] - before[i]))
        .filter(|(_, d)| *d != 0)
        .collect();
    deltas.sort_by_key(|(_, d)| -d.abs());
    deltas
        .into_iter()
        .take(k)
        .map(|(pt, d)| (format!("{:?}", pt), d))
        .collect()
}

fn attribute_jumps(
    quiet_idx: &[usize],
    points_by_ply: &[Option<TracePoint>],
    capture_into: &[bool],
    loud_promotion: &[bool],
    counts_by_ply: &[Vec<i32>],
) -> Vec<JumpAttribution> {
    let mut jumps = Vec::new();
    for w in quiet_idx.windows(2) {
        let a = w[0];
        let b = w[1];
        let Some(pa) = points_by_ply[a].as_ref() else {
            continue;
        };
        let Some(pb) = points_by_ply[b].as_ref() else {
            continue;
        };
        let delta = pb.eval_clipped - pa.eval_clipped;
        let delta_mat = pb.material_only - pa.material_only;
        let mut caps = Vec::new();
        let mut promos = Vec::new();
        for ply in (a + 1)..=b {
            if ply < capture_into.len() && capture_into[ply] {
                caps.push(ply);
            }
            if ply < loud_promotion.len() && loud_promotion[ply] {
                promos.push(ply);
            }
        }
        jumps.push(JumpAttribution {
            from_ply: a,
            to_ply: b,
            delta_eval: delta,
            delta_material: delta_mat,
            residual: delta - delta_mat,
            black_royals: (pa.black_royals, pb.black_royals),
            white_royals: (pa.white_royals, pb.white_royals),
            top_piece_deltas: top_piece_deltas(&counts_by_ply[a], &counts_by_ply[b], 5),
            capture_plies_between: caps,
            loud_promo_plies_between: promos,
        });
    }
    jumps.sort_by(|x, y| {
        y.delta_eval
            .abs()
            .partial_cmp(&x.delta_eval.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    jumps
}

/// Trace a single game. Returns None if replay fails or game has no moves.
pub fn trace_game(
    record: &GameRecordV2,
    game_path: &Path,
    weights: &EvalWeights,
    cfg: &EvalTraceConfig,
) -> Result<Option<GameTrace>, String> {
    let n_moves = record.moves.len();
    let n_plies = n_moves + 1; // ply 0 = start, ply n = after all moves
    if n_plies < 2 {
        return Ok(None);
    }

    let mut capture_into = vec![false; n_plies];
    let mut royal_change = vec![false; n_plies];
    let mut loud_promotion = vec![false; n_plies];
    let mut loud_promo_to: Vec<Option<String>> = vec![None; n_plies];
    let mut counts_by_ply = vec![Vec::new(); n_plies];
    let mut points_by_ply: Vec<Option<TracePoint>> = vec![None; n_plies];

    let mut state = initial_state(&record.start);
    let mut prev_br = count_royals(&state, Color::Black);
    let mut prev_wr = count_royals(&state, Color::White);
    counts_by_ply[0] = piece_counts(&state);
    points_by_ply[0] = Some(snapshot_point(
        &state, 0, weights, cfg.eval_clip, true, false, false, false, None,
    ));

    for (i, mr) in record.moves.iter().enumerate() {
        let mv = crate::game_history::GameHistory::record_to_move(mr)?;
        let cap = move_was_capture(&state, &mv);
        let loud_to = move_is_loud_promotion(&state, mr);
        let turn_before = state.get_current_turn();
        let _ = state.make_move(mv);
        if state.get_current_turn() == turn_before {
            return Err(format!(
                "eval-trace replay failed at move {} in {}",
                mr.move_number, record.game_id
            ));
        }
        let ply = i + 1;
        capture_into[ply] = cap;
        if let Some(pt) = loud_to {
            loud_promotion[ply] = true;
            loud_promo_to[ply] = Some(format!("{:?}", pt));
        }
        let br = count_royals(&state, Color::Black);
        let wr = count_royals(&state, Color::White);
        royal_change[ply] = br != prev_br || wr != prev_wr;
        prev_br = br;
        prev_wr = wr;
        counts_by_ply[ply] = piece_counts(&state);
        points_by_ply[ply] = Some(snapshot_point(
            &state,
            ply,
            weights,
            cfg.eval_clip,
            false,
            cap,
            royal_change[ply],
            loud_promotion[ply],
            loud_promo_to[ply].clone(),
        ));
    }

    let quiet_plies =
        select_quiet_plies(&capture_into, &royal_change, &loud_promotion, cfg.quiet_stride);
    for &ply in &quiet_plies {
        if let Some(p) = points_by_ply[ply].as_mut() {
            p.quiet = true;
        }
    }

    let quiet_series: Vec<(usize, f32)> = quiet_plies
        .iter()
        .filter_map(|&ply| {
            points_by_ply[ply]
                .as_ref()
                .map(|p| (ply, p.eval_clipped))
        })
        .collect();

    let (mut metrics, _, _) = compute_metrics(&quiet_series, &record.result);
    let jumps = attribute_jumps(
        &quiet_plies,
        &points_by_ply,
        &capture_into,
        &loud_promotion,
        &counts_by_ply,
    );

    // Keep only quiet (+ markers already in select) points in output series
    let points: Vec<TracePoint> = quiet_plies
        .iter()
        .filter_map(|&ply| points_by_ply[ply].clone())
        .collect();

    metrics.n_quiet = points.len();

    Ok(Some(GameTrace {
        format_version: TRACE_FORMAT_VERSION,
        game_id: record.game_id.clone(),
        game_path: game_path.display().to_string(),
        result: result_str(&record.result),
        model: cfg.model_path.display().to_string(),
        points,
        jumps,
        metrics,
        search_probes: Vec::new(),
        focus_plies: Vec::new(),
    }))
}

fn resolve_agent_weights(
    spec: &AgentSpec,
    fallback: &EvalWeights,
    zero_noise: bool,
) -> EvalWeights {
    let mut w = if let Some(path) = &spec.model {
        if Path::new(path).is_file() {
            EvalCheckpoint::load_path(path)
                .map(|cp| cp.weights)
                .unwrap_or_else(|_| fallback.clone())
        } else {
            fallback.clone()
        }
    } else {
        fallback.clone()
    };
    if zero_noise {
        w.noise_scale = 0.0;
    }
    w
}

fn stm_score_to_black_abs(score: i32, turn: Color) -> f32 {
    match turn {
        Color::Black => score as f32,
        Color::White => -(score as f32),
    }
}

fn move_label(mv: &crate::game_state::Move) -> String {
    format!(
        "{}{}{}{}",
        mv.from.file, mv.from.rank, mv.to.file, mv.to.rank
    )
}

/// Depth-N probes every `stride` plies (0, stride, 2*stride, …, last).
pub fn search_probe_plies(n_plies: usize, stride: usize) -> Vec<usize> {
    let stride = stride.max(1);
    if n_plies == 0 {
        return Vec::new();
    }
    let mut out: Vec<usize> = (0..n_plies).step_by(stride).collect();
    let last = n_plies - 1;
    if out.last() != Some(&last) {
        out.push(last);
    }
    out
}

fn enrich_with_search(
    record: &GameRecordV2,
    trace: &mut GameTrace,
    scan_weights: &EvalWeights,
    cfg: &EvalTraceConfig,
) -> Result<(), String> {
    let n_plies = record.moves.len() + 1;
    if n_plies < 2 {
        return Ok(());
    }
    let black_w = resolve_agent_weights(&record.black, scan_weights, cfg.zero_noise);
    let white_w = resolve_agent_weights(&record.white, scan_weights, cfg.zero_noise);
    let search_cfg = SearchConfig {
        depth: cfg.search_depth,
        max_time_ms: None,
        collect_trace: false,
        quiescence_depth: 0, // keep probes cheap/consistent
        ..SearchConfig::default()
    };

    let plies = search_probe_plies(n_plies, cfg.search_stride);
    let mut probes = Vec::with_capacity(plies.len());
    for &ply in &plies {
        let state = replay_to_ply(record, ply)?;
        if state.get_winner().is_some() {
            continue;
        }
        let turn = state.get_current_turn();
        let rb = search(&state, &black_w, &search_cfg);
        let rw = search(&state, &white_w, &search_cfg);
        probes.push(SearchProbe {
            ply,
            turn: format!("{:?}", turn),
            depth_black_model: stm_score_to_black_abs(rb.score, turn),
            depth_white_model: stm_score_to_black_abs(rw.score, turn),
            best_move_black_model: rb.best_move.as_ref().map(move_label),
            best_move_white_model: rw.best_move.as_ref().map(move_label),
        });
    }
    trace.search_probes = probes;
    trace.focus_plies = build_focus_plies(trace);
    Ok(())
}

fn build_focus_plies(trace: &GameTrace) -> Vec<usize> {
    let mut focus = Vec::new();
    for j in trace.jumps.iter().take(5) {
        focus.push(j.to_ply);
    }
    // Largest |Δ| between consecutive search probes
    let mut best_d = 0.0f32;
    let mut best_ply = None;
    for w in trace.search_probes.windows(2) {
        let d = (w[1].depth_black_model - w[0].depth_black_model).abs();
        if d > best_d {
            best_d = d;
            best_ply = Some(w[1].ply);
        }
    }
    if let Some(p) = best_ply {
        focus.push(p);
    }
    focus.sort_unstable();
    focus.dedup();
    focus
}

fn load_weights(cfg: &EvalTraceConfig) -> Result<EvalWeights, String> {
    let mut w = if cfg.model_path.is_file() {
        EvalCheckpoint::load_path(&cfg.model_path)?.weights
    } else if cfg.model_path == Path::new("models/ab-seed.json")
        || cfg.model_path.file_name().and_then(|s| s.to_str()) == Some("ab-seed.json")
    {
        // Fall back to hand seed if file missing (analysis still works).
        EvalWeights::seed()
    } else {
        return Err(format!("missing model {}", cfg.model_path.display()));
    };
    if cfg.zero_noise {
        w.noise_scale = 0.0;
    }
    Ok(w)
}

fn list_game_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let rd = fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    for ent in rd {
        let ent = ent.map_err(|e| e.to_string())?;
        let p = ent.path();
        if p.extension().and_then(|s| s.to_str()) == Some("json") {
            // Skip nested partial/ by only taking files directly in dir unless recursive walk
            files.push(p);
        }
    }
    // Also scan non-partial if games are flat; skip anything under partial/
    files.retain(|p| !p.components().any(|c| c.as_os_str() == "partial"));
    files.sort();
    Ok(files)
}

fn write_interesting_md(path: &Path, rows: &[SummaryRow], top_n: usize) -> Result<(), String> {
    let mut out = String::new();
    out.push_str("# Interesting eval trajectories\n\n");
    out.push_str("| Rank | Game | Result | I | MaxDD/RU | LateRev | Disagree | Vol | Top jump |\n");
    out.push_str("|---:|---|---|---:|---:|---|---:|---:|---|\n");
    for (i, r) in rows.iter().take(top_n).enumerate() {
        let jump = r
            .top_jump
            .as_ref()
            .map(|j| {
                let promo = if j.loud_promo_plies_between.is_empty() {
                    String::new()
                } else {
                    format!(" loud@{:?}", j.loud_promo_plies_between)
                };
                format!(
                    "ply {}→{} Δe={:.0} mat={:.0} {:?}{}",
                    j.from_ply,
                    j.to_ply,
                    j.delta_eval,
                    j.delta_material,
                    j.top_piece_deltas
                        .iter()
                        .take(2)
                        .map(|(n, d)| format!("{n}:{d}"))
                        .collect::<Vec<_>>(),
                    promo
                )
            })
            .unwrap_or_else(|| "—".into());
        out.push_str(&format!(
            "| {} | `{}` | {} | {:.2} | {:.0} | {} | {:.2} | {:.0} | {} |\n",
            i + 1,
            r.game_id,
            r.result,
            r.metrics.interestingness,
            r.metrics.max_drawdown.max(r.metrics.max_runup),
            r.metrics.late_reversal,
            r.metrics.result_disagreement,
            r.metrics.volatility,
            jump
        ));
    }
    out.push('\n');
    out.push_str("Open each game's JSON for quiet series + `search_probes` (depth-N every 5 plies) and `focus_plies`.\n");
    out.push_str("Scrub the GUI to those plies first.\n");
    for (i, r) in rows.iter().take(top_n).enumerate() {
        let p = Path::new(&r.trace_path);
        if let Ok(s) = fs::read_to_string(p) {
            if let Ok(t) = serde_json::from_str::<GameTrace>(&s) {
                if !t.focus_plies.is_empty() {
                    out.push_str(&format!(
                        "- #{} `{}` focus_plies: {:?}\n",
                        i + 1,
                        r.game_id,
                        t.focus_plies
                    ));
                }
            }
        }
    }
    fs::write(path, out).map_err(|e| format!("write {}: {e}", path.display()))
}

/// Scan a games directory, write traces + summary + interesting.md.
pub fn run_eval_trace(cfg: &EvalTraceConfig) -> Result<(usize, PathBuf), String> {
    ensure_data_dirs()?;
    fs::create_dir_all(&cfg.out_dir)
        .map_err(|e| format!("create {}: {e}", cfg.out_dir.display()))?;
    let weights = load_weights(cfg)?;
    let files = list_game_files(&cfg.games_dir)?;
    let limit = cfg.max_games.unwrap_or(usize::MAX);

    let mut summaries = Vec::new();
    let mut n_ok = 0usize;
    for path in files.into_iter().take(limit) {
        let record = match GameRecordV2::load_path(&path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("eval-trace skip {}: {e}", path.display());
                continue;
            }
        };
        if record.abort_reason.is_some() {
            continue;
        }
        match trace_game(&record, &path, &weights, cfg) {
            Ok(Some(mut trace)) => {
                let trace_path = cfg.out_dir.join(format!("{}.json", record.game_id));
                // interestingness filled later
                let top_jump = trace.jumps.first().cloned();
                let row = SummaryRow {
                    game_id: record.game_id.clone(),
                    game_path: path.display().to_string(),
                    result: trace.result.clone(),
                    metrics: trace.metrics.clone(),
                    top_jump,
                    trace_path: trace_path.display().to_string(),
                };
                summaries.push(row);
                // write after metrics finalized
                let _ = &mut trace;
                n_ok += 1;
                // stash trace for rewrite — keep in memory only metrics path; write after I assigned
                fs::write(
                    &trace_path,
                    serde_json::to_string_pretty(&trace).map_err(|e| e.to_string())?,
                )
                .map_err(|e| format!("write {}: {e}", trace_path.display()))?;
            }
            Ok(None) => {}
            Err(e) => eprintln!("eval-trace fail {}: {e}", path.display()),
        }
    }

    assign_interestingness(&mut summaries);

    // Rewrite metrics.interestingness; depth-search enrich top-N.
    for (rank, r) in summaries.iter().enumerate() {
        let p = Path::new(&r.trace_path);
        let Ok(s) = fs::read_to_string(p) else {
            continue;
        };
        let Ok(mut t) = serde_json::from_str::<GameTrace>(&s) else {
            continue;
        };
        t.metrics = r.metrics.clone();
        if !cfg.skip_search && rank < cfg.top_n {
            match GameRecordV2::load_path(Path::new(&r.game_path)) {
                Ok(rec) => {
                    eprintln!(
                        "eval-trace search enrich rank={} {} (depth={} stride={})",
                        rank + 1,
                        r.game_id,
                        cfg.search_depth,
                        cfg.search_stride
                    );
                    if let Err(e) = enrich_with_search(&rec, &mut t, &weights, cfg) {
                        eprintln!("eval-trace search enrich fail {}: {e}", r.game_id);
                    }
                }
                Err(e) => eprintln!("eval-trace reload {}: {e}", r.game_path),
            }
        }
        let _ = fs::write(p, serde_json::to_string_pretty(&t).unwrap_or(s));
    }

    let summary_path = cfg.out_dir.join("summary.jsonl");
    let mut lines = String::new();
    for r in &summaries {
        lines.push_str(&serde_json::to_string(r).map_err(|e| e.to_string())?);
        lines.push('\n');
    }
    fs::write(&summary_path, lines).map_err(|e| format!("write {}: {e}", summary_path.display()))?;

    let md_path = cfg.out_dir.join("interesting.md");
    write_interesting_md(&md_path, &summaries, cfg.top_n)?;

    Ok((n_ok, md_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_history::GameResult;

    fn synth_quiet_series_collapse() -> Vec<(usize, f32)> {
        // Black ahead then collapses
        vec![
            (0, 100.0),
            (16, 120.0),
            (32, 80.0),
            (48, -50.0),
            (64, -200.0),
        ]
    }

    #[test]
    fn select_quiet_skips_capture_mid_burst() {
        // plies 0..8; captures into 2,3 then quiet
        let cap = vec![false, false, true, true, false, false, false, false, false];
        let royal = vec![false; 9];
        let loud = vec![false; 9];
        let plies = select_quiet_plies(&cap, &royal, &loud, 3);
        assert!(plies.contains(&0));
        assert!(plies.contains(&8));
        assert!(!plies.contains(&2));
        assert!(!plies.contains(&3));
    }

    #[test]
    fn select_quiet_keeps_loud_promotion() {
        let cap = vec![false; 10];
        let royal = vec![false; 10];
        let mut loud = vec![false; 10];
        loud[4] = true;
        let plies = select_quiet_plies(&cap, &royal, &loud, 16);
        assert!(plies.contains(&4));
    }

    #[test]
    fn free_king_promotes_to_great_general_is_loud() {
        assert!(is_big_piece(PieceType::GreatGeneral));
        assert_eq!(
            PieceType::FreeKing.promotes_to(),
            Some(PieceType::GreatGeneral)
        );
        let non_big_into_big = [
            PieceType::FreeKing,
            PieceType::FireGeneral,
            PieceType::WaterGeneral,
            PieceType::PoisonousSerpent,
            PieceType::EasternBarbarian,
            PieceType::OldKite,
            PieceType::DarkSpirit,
        ];
        for pt in non_big_into_big {
            let to = pt.promotes_to().expect("promotes");
            assert!(
                is_big_piece(to),
                "{pt:?} → {to:?} should be loud promotion target"
            );
            assert!(!is_big_piece(pt), "{pt:?} should not already be big");
        }
        // Any promote-into-big counts as loud (includes big→big upgrades).
        let loud: Vec<_> = ALL_PIECE_TYPES
            .iter()
            .filter_map(|pt| {
                let to = pt.promotes_to()?;
                is_big_piece(to).then_some((*pt, to))
            })
            .collect();
        assert!(
            loud.iter().any(|(f, t)| *f == PieceType::FreeKing
                && *t == PieceType::GreatGeneral)
        );
        assert!(!loud.is_empty());
        // Keep this list in sync with scale_sample::is_big_piece for docs/debugging.
        let expected_new_big = [
            (PieceType::DarkSpirit, PieceType::BuddhistSpirit),
            (PieceType::FireGeneral, PieceType::GreatGeneral),
            (PieceType::FreeKing, PieceType::GreatGeneral),
            (PieceType::PoisonousSerpent, PieceType::HookMover),
            (PieceType::EasternBarbarian, PieceType::Lion),
            (PieceType::OldKite, PieceType::Tengu),
            (PieceType::WaterGeneral, PieceType::ViceGeneral),
        ];
        for (from, to) in expected_new_big {
            assert!(
                loud.contains(&(from, to)),
                "missing loud promo {from:?}→{to:?} in {loud:?}"
            );
        }
    }

    #[test]
    fn metrics_detect_drawdown_and_reversal() {
        let series = synth_quiet_series_collapse();
        let (m, _, _) = compute_metrics(&series, &Some(GameResult::WhiteWins));
        assert!(m.max_drawdown > 200.0);
        assert!(m.late_reversal);
        assert!(m.result_disagreement > 0.0); // early black-favoring samples disagree with white win
        assert!(m.sign_flips >= 1);
    }

    #[test]
    fn mate_clip_limits_interestingness_input() {
        let series = vec![(0, 10.0), (10, 20.0), (20, 1_000_000.0)];
        let clipped: Vec<_> = series
            .iter()
            .map(|(p, e)| (*p, clip(*e, 50_000.0)))
            .collect();
        let (m, _, _) = compute_metrics(&clipped, &Some(GameResult::BlackWins));
        assert!(m.max_runup <= 50_000.0 + 20.0);
    }

    #[test]
    fn interestingness_ranks_swingy_higher() {
        let mut rows = vec![
            SummaryRow {
                game_id: "calm".into(),
                game_path: String::new(),
                result: "black_wins".into(),
                metrics: GameMetrics {
                    max_drawdown: 10.0,
                    max_runup: 10.0,
                    sign_flips: 0,
                    late_reversal: false,
                    result_disagreement: 0.0,
                    volatility: 1.0,
                    interestingness: 0.0,
                    n_quiet: 5,
                },
                top_jump: None,
                trace_path: String::new(),
            },
            SummaryRow {
                game_id: "wild".into(),
                game_path: String::new(),
                result: "white_wins".into(),
                metrics: GameMetrics {
                    max_drawdown: 5000.0,
                    max_runup: 100.0,
                    sign_flips: 3,
                    late_reversal: true,
                    result_disagreement: 0.6,
                    volatility: 400.0,
                    interestingness: 0.0,
                    n_quiet: 5,
                },
                top_jump: None,
                trace_path: String::new(),
            },
        ];
        assign_interestingness(&mut rows);
        assert_eq!(rows[0].game_id, "wild");
    }

    #[test]
    fn search_probe_plies_every_five() {
        assert_eq!(search_probe_plies(1, 5), vec![0]);
        assert_eq!(search_probe_plies(11, 5), vec![0, 5, 10]);
        assert_eq!(search_probe_plies(12, 5), vec![0, 5, 10, 11]);
    }

    #[test]
    fn stm_to_black_abs_flips_for_white() {
        assert_eq!(stm_score_to_black_abs(100, Color::Black), 100.0);
        assert_eq!(stm_score_to_black_abs(100, Color::White), -100.0);
    }
}
