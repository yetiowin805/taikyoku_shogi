//! Local HTTP API for the GUI workbench.
use crate::debug_tool::DebugTool;
use crate::session_api::CommandResult;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex as StdMutex,
};
use std::time::Instant;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

pub struct ServerState {
    tool: Mutex<DebugTool>,
    analysis: Mutex<Option<AnalysisJob>>,
    next_analysis_id: AtomicU64,
}

pub type AppState = Arc<ServerState>;

struct AnalysisJob {
    id: u64,
    cancel: Arc<AtomicBool>,
    view: Arc<StdMutex<AnalysisView>>,
}

#[derive(Clone, Serialize)]
pub struct AnalysisView {
    pub ok: bool,
    pub job_id: u64,
    pub running: bool,
    pub message: String,
    pub elapsed_ms: u64,
    pub search: Option<crate::search::SearchInfo>,
}

#[derive(Deserialize)]
pub struct LoadBody {
    pub filename: String,
}

#[derive(Deserialize)]
pub struct GotoBody {
    pub ply: usize,
}

#[derive(Deserialize)]
pub struct StepBody {
    #[serde(default = "default_one")]
    pub n: usize,
}

fn default_one() -> usize {
    1
}

#[derive(Deserialize)]
pub struct MovesQuery {
    pub file: Option<u8>,
    pub rank: Option<u8>,
}

#[derive(Deserialize)]
pub struct MoveBody {
    pub from_file: u8,
    pub from_rank: u8,
    pub to_file: u8,
    pub to_rank: u8,
    pub promote: Option<bool>,
    pub path_index: Option<usize>,
}

#[derive(Deserialize)]
pub struct AgentBody {
    #[serde(default = "default_mi")]
    pub agent: String,
    pub depth: Option<u32>,
    pub model: Option<String>,
    pub max_time_ms: Option<u64>,
    pub quiescence_depth: Option<u32>,
    pub cpu_percent: Option<u8>,
}

fn default_mi() -> String {
    "mi".to_string()
}

impl AgentBody {
    fn options(&self) -> crate::player::AgentOptions {
        crate::player::AgentOptions {
            depth: self.depth,
            model: self.model.clone(),
            max_time_ms: self.max_time_ms,
            quiescence_depth: self.quiescence_depth,
            cpu_percent: self.cpu_percent,
            cancel: None,
        }
    }
}

#[derive(Deserialize)]
pub struct AnalysisQuery {
    pub job_id: u64,
}

#[derive(Deserialize)]
pub struct AnalysisStopBody {
    pub job_id: u64,
}

#[derive(Deserialize)]
pub struct SaveBody {
    pub filename: Option<String>,
}

async fn cancel_current_analysis(state: &AppState) {
    if let Some(job) = state.analysis.lock().await.as_ref() {
        job.cancel.store(true, Ordering::Relaxed);
    }
}

async fn api_state(State(state): State<AppState>) -> Json<CommandResult> {
    let tool = state.tool.lock().await;
    Json(tool.ok_result("ok"))
}

async fn api_new(State(state): State<AppState>) -> Json<CommandResult> {
    cancel_current_analysis(&state).await;
    let mut tool = state.tool.lock().await;
    tool.new_game();
    Json(tool.ok_result("New game started"))
}

async fn api_list(State(state): State<AppState>) -> impl IntoResponse {
    let tool = state.tool.lock().await;
    match tool.list_games_pub() {
        Ok(games) => (StatusCode::OK, Json(serde_json::json!({ "ok": true, "games": games }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "ok": false, "message": e })),
        )
            .into_response(),
    }
}

async fn api_load(
    State(state): State<AppState>,
    Json(body): Json<LoadBody>,
) -> Json<CommandResult> {
    cancel_current_analysis(&state).await;
    let mut tool = state.tool.lock().await;
    match tool.load_game(&body.filename) {
        Ok(()) => Json(tool.ok_result(format!("Loaded {}", body.filename))),
        Err(e) => Json(tool.err_result(e)),
    }
}

async fn api_goto(
    State(state): State<AppState>,
    Json(body): Json<GotoBody>,
) -> Json<CommandResult> {
    cancel_current_analysis(&state).await;
    let mut tool = state.tool.lock().await;
    match tool.goto_move(body.ply) {
        Ok(()) => Json(tool.ok_result(format!("At ply {}", body.ply))),
        Err(e) => Json(tool.err_result(e)),
    }
}

async fn api_forward(
    State(state): State<AppState>,
    Json(body): Json<StepBody>,
) -> Json<CommandResult> {
    cancel_current_analysis(&state).await;
    let mut tool = state.tool.lock().await;
    match tool.forward(body.n) {
        Ok(()) => Json(tool.ok_result(format!("Forward {}", body.n))),
        Err(e) => Json(tool.err_result(e)),
    }
}

async fn api_back(
    State(state): State<AppState>,
    Json(body): Json<StepBody>,
) -> Json<CommandResult> {
    cancel_current_analysis(&state).await;
    let mut tool = state.tool.lock().await;
    match tool.back(body.n) {
        Ok(()) => Json(tool.ok_result(format!("Back {}", body.n))),
        Err(e) => Json(tool.err_result(e)),
    }
}

async fn api_moves(
    State(state): State<AppState>,
    Query(q): Query<MovesQuery>,
) -> Json<CommandResult> {
    let tool = state.tool.lock().await;
    let from = match (q.file, q.rank) {
        (Some(f), Some(r)) => Some((f, r)),
        (None, None) => None,
        _ => {
            return Json(tool.err_result("Provide both file and rank, or neither"));
        }
    };
    match tool.legal_moves_dto(from) {
        Ok(moves) => {
            let n = moves.len();
            Json(tool.ok_result_with_moves(format!("{} legal moves", n), moves))
        }
        Err(e) => Json(tool.err_result(e)),
    }
}

async fn api_move(
    State(state): State<AppState>,
    Json(body): Json<MoveBody>,
) -> Json<CommandResult> {
    cancel_current_analysis(&state).await;
    let mut tool = state.tool.lock().await;
    match tool.apply_human_move(
        body.from_file,
        body.from_rank,
        body.to_file,
        body.to_rank,
        body.promote,
        body.path_index,
    ) {
        Ok(msg) => Json(tool.ok_result(msg)),
        Err(e) => Json(tool.err_result(e)),
    }
}

async fn api_suggest(
    State(state): State<AppState>,
    Json(body): Json<AgentBody>,
) -> Json<CommandResult> {
    let tool = state.tool.lock().await;
    match tool.suggest_agent_with_options(&body.agent, &body.options()) {
        Ok((msg, Some(search))) => Json(tool.ok_result_with_search(msg, search)),
        Ok((msg, None)) => Json(tool.ok_result(msg)),
        Err(e) => Json(tool.err_result(e)),
    }
}

async fn api_play_agent(
    State(state): State<AppState>,
    Json(body): Json<AgentBody>,
) -> Json<CommandResult> {
    cancel_current_analysis(&state).await;
    let mut tool = state.tool.lock().await;
    match tool.play_agent_with_options(&body.agent, &body.options()) {
        Ok((msg, Some(search))) => Json(tool.ok_result_with_search(msg, search)),
        Ok((msg, None)) => Json(tool.ok_result(msg)),
        Err(e) => Json(tool.err_result(e)),
    }
}

async fn api_list_models() -> impl IntoResponse {
    match crate::eval::list_model_files("models") {
        Ok(models) => (StatusCode::OK, Json(serde_json::json!({ "ok": true, "models": models }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "ok": false, "message": e })),
        )
            .into_response(),
    }
}

async fn api_save(
    State(state): State<AppState>,
    Json(body): Json<SaveBody>,
) -> Json<CommandResult> {
    let tool = state.tool.lock().await;
    match tool.save_current(body.filename.as_deref()) {
        Ok(msg) => Json(tool.ok_result(msg)),
        Err(e) => Json(tool.err_result(e)),
    }
}

async fn api_analysis_start(
    State(state): State<AppState>,
    Json(body): Json<AgentBody>,
) -> impl IntoResponse {
    cancel_current_analysis(&state).await;
    let game_state = {
        let tool = state.tool.lock().await;
        tool.game_state_ref().clone()
    };
    let side = match game_state.get_current_turn() {
        crate::piece::Color::Black => "Black".to_string(),
        crate::piece::Color::White => "White".to_string(),
    };
    let job_id = state.next_analysis_id.fetch_add(1, Ordering::Relaxed);
    let cancel = Arc::new(AtomicBool::new(false));
    let view = Arc::new(StdMutex::new(AnalysisView {
        ok: true,
        job_id,
        running: true,
        message: "Loading engine…".into(),
        elapsed_ms: 0,
        search: None,
    }));
    *state.analysis.lock().await = Some(AnalysisJob {
        id: job_id,
        cancel: Arc::clone(&cancel),
        view: Arc::clone(&view),
    });

    let mut options = body.options();
    options.cancel = Some(Arc::clone(&cancel));
    tokio::task::spawn_blocking(move || {
        let started = Instant::now();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let player = crate::alphabeta_player::AlphaBetaPlayer::from_options(&options);
            let search_started = Instant::now();
            let weights = player.weights().clone();
            let config = player.config().clone();
            let requested_depth = config.depth;
            let progress_weights = weights.clone();
            let progress_view = Arc::clone(&view);
            let progress_state = game_state.clone();
            let progress_side = side.clone();
            let mut progress = move |depth,
                                     score,
                                     best_move: &crate::game_state::Move,
                                     nodes,
                                     root_lines: &[(crate::game_state::Move, i32)]| {
                let search = crate::search::search_info_from_iteration(
                    "ab",
                    &progress_side,
                    &progress_state,
                    &progress_weights,
                    requested_depth,
                    depth,
                    score,
                    best_move,
                    nodes,
                    root_lines,
                );
                let mut current = progress_view.lock().unwrap_or_else(|e| e.into_inner());
                current.elapsed_ms = search_started.elapsed().as_millis() as u64;
                current.message = format!("Depth {depth} complete · deepening");
                current.search = Some(search);
            };
            let result = crate::search::search_with_progress(
                &game_state,
                &weights,
                &config,
                &mut progress,
            );
            if cancel.load(Ordering::Relaxed) {
                let mut current = view.lock().unwrap_or_else(|e| e.into_inner());
                current.running = false;
                current.message = "Analysis paused".into();
                current.elapsed_ms = search_started.elapsed().as_millis() as u64;
                return;
            }
            let final_search = result.best_move.as_ref().map_or_else(
                || {
                    crate::search::search_info_from_result(
                        "ab",
                        &side,
                        requested_depth,
                        &result,
                    )
                },
                |best_move| {
                    crate::search::search_info_from_iteration(
                        "ab",
                        &side,
                        &game_state,
                        &weights,
                        requested_depth,
                        result.completed_depth,
                        result.score,
                        best_move,
                        result.nodes,
                        &result.root_lines,
                    )
                },
            );
            let mut current = view.lock().unwrap_or_else(|e| e.into_inner());
            current.running = false;
            current.elapsed_ms = search_started.elapsed().as_millis() as u64;
            current.message = format!("Depth {} complete", final_search.depth);
            current.search = Some(final_search);
        }));
        if outcome.is_err() {
            let mut current = view.lock().unwrap_or_else(|e| e.into_inner());
            current.ok = false;
            current.running = false;
            current.elapsed_ms = started.elapsed().as_millis() as u64;
            current.message = "Engine analysis failed while loading or searching".into();
        }
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "ok": true, "job_id": job_id })),
    )
}

async fn api_analysis_state(
    State(state): State<AppState>,
    Query(query): Query<AnalysisQuery>,
) -> impl IntoResponse {
    let analysis = state.analysis.lock().await;
    let Some(job) = analysis.as_ref().filter(|job| job.id == query.job_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "ok": false, "message": "Analysis job not found" })),
        )
            .into_response();
    };
    let view = job.view.lock().unwrap_or_else(|e| e.into_inner()).clone();
    (StatusCode::OK, Json(serde_json::json!(view))).into_response()
}

async fn api_analysis_stop(
    State(state): State<AppState>,
    Json(body): Json<AnalysisStopBody>,
) -> impl IntoResponse {
    let analysis = state.analysis.lock().await;
    let Some(job) = analysis.as_ref().filter(|job| job.id == body.job_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "ok": false, "message": "Analysis job not found" })),
        )
            .into_response();
    };
    job.cancel.store(true, Ordering::Relaxed);
    let mut view = job.view.lock().unwrap_or_else(|e| e.into_inner());
    view.running = false;
    view.message = "Stopping analysis…".into();
    (
        StatusCode::OK,
        Json(serde_json::json!({ "ok": true, "job_id": body.job_id })),
    )
        .into_response()
}

/// Read `data/run/status.json` written by `worker daemon` (or 404 if absent).
async fn api_training_status() -> impl IntoResponse {
    let path = crate::training::paths::status_path();
    match crate::training::run_status::RunStatus::load_path(&path) {
        Ok(status) => (StatusCode::OK, Json(serde_json::json!({ "ok": true, "status": status }))).into_response(),
        Err(e) => {
            let missing = !path.exists();
            let code = if missing {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                code,
                Json(serde_json::json!({
                    "ok": false,
                    "message": e,
                    "path": path.display().to_string(),
                })),
            )
                .into_response()
        }
    }
}

pub fn app_router(state: AppState, static_dir: Option<PathBuf>) -> Router {
    let api = Router::new()
        .route("/state", get(api_state))
        .route("/new", post(api_new))
        .route("/list", get(api_list))
        .route("/load", post(api_load))
        .route("/goto", post(api_goto))
        .route("/forward", post(api_forward))
        .route("/back", post(api_back))
        .route("/moves", get(api_moves))
        .route("/move", post(api_move))
        .route("/suggest", post(api_suggest))
        .route("/play", post(api_play_agent))
        .route("/analysis/start", post(api_analysis_start))
        .route("/analysis/state", get(api_analysis_state))
        .route("/analysis/stop", post(api_analysis_stop))
        .route("/save", post(api_save))
        .route("/models", get(api_list_models))
        .route("/training/status", get(api_training_status))
        .with_state(state);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let mut router = Router::new().nest("/api", api).layer(cors);

    if let Some(dir) = static_dir {
        if dir.exists() {
            let index = dir.join("index.html");
            router = router.fallback_service(
                ServeDir::new(dir).not_found_service(ServeFile::new(index)),
            );
        }
    }

    router
}

pub async fn serve(addr: SocketAddr, static_dir: Option<PathBuf>) -> Result<(), String> {
    let state: AppState = Arc::new(ServerState {
        tool: Mutex::new(DebugTool::new()),
        analysis: Mutex::new(None),
        next_analysis_id: AtomicU64::new(1),
    });
    let app = app_router(state, static_dir);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("bind {}: {}", addr, e))?;
    println!("Taikyoku GUI server listening on http://{}", addr);
    axum::serve(listener, app)
        .await
        .map_err(|e| format!("server error: {}", e))?;
    Ok(())
}
