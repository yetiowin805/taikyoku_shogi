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
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

pub type AppState = Arc<Mutex<DebugTool>>;

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
        }
    }
}

#[derive(Deserialize)]
pub struct SaveBody {
    pub filename: Option<String>,
}

async fn api_state(State(state): State<AppState>) -> Json<CommandResult> {
    let tool = state.lock().await;
    Json(tool.ok_result("ok"))
}

async fn api_new(State(state): State<AppState>) -> Json<CommandResult> {
    let mut tool = state.lock().await;
    tool.new_game();
    Json(tool.ok_result("New game started"))
}

async fn api_list(State(state): State<AppState>) -> impl IntoResponse {
    let tool = state.lock().await;
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
    let mut tool = state.lock().await;
    match tool.load_game(&body.filename) {
        Ok(()) => Json(tool.ok_result(format!("Loaded {}", body.filename))),
        Err(e) => Json(tool.err_result(e)),
    }
}

async fn api_goto(
    State(state): State<AppState>,
    Json(body): Json<GotoBody>,
) -> Json<CommandResult> {
    let mut tool = state.lock().await;
    match tool.goto_move(body.ply) {
        Ok(()) => Json(tool.ok_result(format!("At ply {}", body.ply))),
        Err(e) => Json(tool.err_result(e)),
    }
}

async fn api_forward(
    State(state): State<AppState>,
    Json(body): Json<StepBody>,
) -> Json<CommandResult> {
    let mut tool = state.lock().await;
    match tool.forward(body.n) {
        Ok(()) => Json(tool.ok_result(format!("Forward {}", body.n))),
        Err(e) => Json(tool.err_result(e)),
    }
}

async fn api_back(
    State(state): State<AppState>,
    Json(body): Json<StepBody>,
) -> Json<CommandResult> {
    let mut tool = state.lock().await;
    match tool.back(body.n) {
        Ok(()) => Json(tool.ok_result(format!("Back {}", body.n))),
        Err(e) => Json(tool.err_result(e)),
    }
}

async fn api_moves(
    State(state): State<AppState>,
    Query(q): Query<MovesQuery>,
) -> Json<CommandResult> {
    let tool = state.lock().await;
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
    let mut tool = state.lock().await;
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
    let tool = state.lock().await;
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
    let mut tool = state.lock().await;
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
    let tool = state.lock().await;
    match tool.save_current(body.filename.as_deref()) {
        Ok(msg) => Json(tool.ok_result(msg)),
        Err(e) => Json(tool.err_result(e)),
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
        .route("/save", post(api_save))
        .route("/models", get(api_list_models))
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
    let state: AppState = Arc::new(Mutex::new(DebugTool::new()));
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
