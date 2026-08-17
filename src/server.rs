//! Local HTTP API for the GUI workbench.
use crate::alphabeta_player::AlphaBetaPlayer;
use crate::debug_tool::DebugTool;
use crate::piece::Color;
use crate::player::Player;
use crate::search::search_info_from_result;
use crate::session_api::CommandResult;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

pub struct AppShared {
    pub tool: Mutex<DebugTool>,
    /// Active AB search abort flag; reachable without waiting on `tool`.
    pub stop: Mutex<Option<Arc<AtomicBool>>>,
    /// PID of a spawned historical `think-loop` (kill on stop).
    pub ext_pid: Mutex<Option<u32>>,
}

pub type AppState = Arc<AppShared>;

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
    pub intermediate_file: Option<u8>,
    pub intermediate_rank: Option<u8>,
}

#[derive(Deserialize)]
pub struct AgentBody {
    #[serde(default = "default_mi")]
    pub agent: String,
    pub depth: Option<u32>,
    pub model: Option<String>,
    pub max_time_ms: Option<u64>,
    pub quiescence_depth: Option<u32>,
    pub engine: Option<String>,
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
            engine: self.engine.clone(),
        }
    }
}

#[derive(Deserialize)]
pub struct SaveBody {
    pub filename: Option<String>,
}

async fn request_stop(state: &AppState) {
    if let Some(flag) = state.stop.lock().await.as_ref() {
        flag.store(true, Ordering::Relaxed);
    }
    if let Some(pid) = *state.ext_pid.lock().await {
        kill_pid(pid);
    }
}

fn kill_pid(pid: u32) {
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .status();
}

async fn install_stop(state: &AppState) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let mut slot = state.stop.lock().await;
    if let Some(old) = slot.as_ref() {
        old.store(true, Ordering::Relaxed);
    }
    *slot = Some(Arc::clone(&flag));
    flag
}

async fn clear_stop_if(state: &AppState, flag: &Arc<AtomicBool>) {
    let mut slot = state.stop.lock().await;
    if slot.as_ref().is_some_and(|s| Arc::ptr_eq(s, flag)) {
        *slot = None;
    }
}

fn is_ab_agent(name: &str) -> bool {
    matches!(name, "ab" | "search")
}

async fn api_state(State(state): State<AppState>) -> Json<CommandResult> {
    let tool = state.tool.lock().await;
    Json(tool.ok_result("ok"))
}

async fn api_new(State(state): State<AppState>) -> Json<CommandResult> {
    request_stop(&state).await;
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
    request_stop(&state).await;
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
    request_stop(&state).await;
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
    request_stop(&state).await;
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
    request_stop(&state).await;
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
    request_stop(&state).await;
    let mut tool = state.tool.lock().await;
    match tool.apply_human_move(
        body.from_file,
        body.from_rank,
        body.to_file,
        body.to_rank,
        body.promote,
        body.path_index,
        body.intermediate_file,
        body.intermediate_rank,
    ) {
        Ok(msg) => Json(tool.ok_result(msg)),
        Err(e) => Json(tool.err_result(e)),
    }
}

async fn api_stop_search(State(state): State<AppState>) -> Json<CommandResult> {
    request_stop(&state).await;
    let tool = state.tool.lock().await;
    Json(tool.ok_result("search stop requested"))
}

/// Run AB off the tool mutex. `apply` is `/play`; suggest never applies.
async fn run_ab(state: &AppState, opts: crate::player::AgentOptions, apply: bool) -> Json<CommandResult> {
    if opts.engine.is_some() {
        return run_external_ab(state, opts, apply).await;
    }
    let stop = install_stop(state).await;
    let (game, ply, turn, player, side) = {
        let tool = state.tool.lock().await;
        let player = AlphaBetaPlayer::from_options(&opts).with_stop(Arc::clone(&stop));
        let turn = tool.game_state_ref().get_current_turn();
        let side = match turn {
            Color::Black => "Black",
            Color::White => "White",
        };
        (
            tool.game_state_ref().clone(),
            tool.cursor_ply(),
            turn,
            player,
            side.to_string(),
        )
    };
    let depth = player.config().depth;
    let result = match tokio::task::spawn_blocking(move || player.analyze(&game)).await {
        Ok(r) => r,
        Err(e) => {
            clear_stop_if(state, &stop).await;
            let tool = state.tool.lock().await;
            return Json(tool.err_result(format!("search task: {e}")));
        }
    };
    let user_stop = stop.load(Ordering::Relaxed);
    clear_stop_if(state, &stop).await;

    let mut tool = state.tool.lock().await;
    let stale = tool.cursor_ply() != ply || tool.game_state_ref().get_current_turn() != turn;
    let mut info = search_info_from_result("ab", &side, depth, &result);
    if user_stop || result.aborted {
        info.aborted = true;
    }

    if user_stop || stale {
        return Json(tool.ok_result_with_search("search aborted", info));
    }
    if !apply {
        let msg = match &info.best_move {
            Some(label) => format!(
                "ab suggests: {} (eval {} → search {}, nodes {})",
                label, info.static_eval, info.score, info.nodes
            ),
            None => "ab has no legal moves".to_string(),
        };
        return Json(tool.ok_result_with_search(msg, info));
    }
    let Some(mv) = result.best_move else {
        return Json(tool.err_result("ab has no legal moves"));
    };
    match tool.apply_live_move_pub(mv) {
        Ok(msg) => {
            let msg = format!(
                "ab: {} (eval {} → search {}, nodes {})",
                msg, info.static_eval, info.score, info.nodes
            );
            Json(tool.ok_result_with_search(msg, info))
        }
        Err(e) => Json(tool.err_result(e)),
    }
}

async fn run_external_ab(
    state: &AppState,
    opts: crate::player::AgentOptions,
    apply: bool,
) -> Json<CommandResult> {
    let engine = opts.engine.clone().expect("engine");
    if !std::path::Path::new(&engine).is_file() {
        let tool = state.tool.lock().await;
        return Json(tool.err_result(format!(
            "missing historical binary {engine} — run ./deploy/freeze_history.sh"
        )));
    }
    let stop = install_stop(state).await;
    let spec = crate::training::record::AgentSpec {
        name: "ab".into(),
        depth: opts.depth,
        model: opts.model.clone(),
        max_time_ms: opts.max_time_ms,
        quiescence_depth: opts.quiescence_depth,
        engine: Some(engine.clone()),
    };
    let player = match crate::external_player::ExternalAbPlayer::spawn(&engine, &spec) {
        Ok(p) => p,
        Err(e) => {
            let tool = state.tool.lock().await;
            return Json(tool.err_result(e));
        }
    };
    if let Some(pid) = player.pid() {
        *state.ext_pid.lock().await = Some(pid);
    }
    let (game, ply, turn) = {
        let tool = state.tool.lock().await;
        (
            tool.game_state_ref().clone(),
            tool.cursor_ply(),
            tool.game_state_ref().get_current_turn(),
        )
    };
    let result = tokio::task::spawn_blocking(move || player.choose_move(&game)).await;
    let stopped = stop.load(Ordering::Relaxed);
    *state.ext_pid.lock().await = None;
    clear_stop_if(state, &stop).await;
    let mut tool = state.tool.lock().await;
    let stale = tool.cursor_ply() != ply || tool.game_state_ref().get_current_turn() != turn;
    let mv = match result {
        Ok(m) => m,
        Err(e) => return Json(tool.err_result(format!("external search: {e}"))),
    };
    if stopped || stale {
        return Json(tool.ok_result("search aborted"));
    }
    let Some(mv) = mv else {
        return Json(tool.err_result("historical engine has no legal moves"));
    };
    let label = crate::debug_tool::DebugTool::format_move_public(&mv);
    if !apply {
        return Json(tool.ok_result(format!("ab-ext suggests: {label}")));
    }
    match tool.apply_live_move_pub(mv) {
        Ok(msg) => Json(tool.ok_result(format!("ab-ext: {msg}"))),
        Err(e) => Json(tool.err_result(e)),
    }
}

async fn api_suggest(
    State(state): State<AppState>,
    Json(body): Json<AgentBody>,
) -> Json<CommandResult> {
    if is_ab_agent(&body.agent) {
        return run_ab(&state, body.options(), false).await;
    }
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
    if is_ab_agent(&body.agent) {
        return run_ab(&state, body.options(), true).await;
    }
    let mut tool = state.tool.lock().await;
    match tool.play_agent_with_options(&body.agent, &body.options()) {
        Ok((msg, Some(search))) => Json(tool.ok_result_with_search(msg, search)),
        Ok((msg, None)) => Json(tool.ok_result(msg)),
        Err(e) => Json(tool.err_result(e)),
    }
}

async fn api_list_models() -> impl IntoResponse {
    match crate::training::history::list_gui_agents() {
        Ok(agents) => {
            let models: Vec<String> = agents.iter().map(|a| a.id.clone()).collect();
            (
                StatusCode::OK,
                Json(serde_json::json!({ "ok": true, "models": models, "agents": agents })),
            )
                .into_response()
        }
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
        .route("/stop-search", post(api_stop_search))
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
    let state: AppState = Arc::new(AppShared {
        tool: Mutex::new(DebugTool::new()),
        stop: Mutex::new(None),
        ext_pid: Mutex::new(None),
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
