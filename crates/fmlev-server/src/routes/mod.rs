pub mod context;
pub mod memory;

pub use context::*;
pub use memory::*;

use axum::{
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use tower_http::cors::CorsLayer;

use crate::state::ServerState;

pub(crate) fn api_error(
    status: StatusCode,
    error: impl std::fmt::Display,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({ "status": "error", "message": error.to_string() })),
    )
}

pub fn create_router(state: ServerState) -> Router {
    Router::new()
        .route("/", get(serve_ui))
        .route("/api/status", get(get_status))
        .route("/api/context", get(get_context))
        .route("/api/search", get(search_entities))
        .route("/api/players/discover", post(discover_players))
        .route("/api/context/select", post(select_context))
        .route("/api/player/update", post(update_player))
        .route("/api/player/preset", post(apply_player_preset))
        .route("/api/club/update", post(update_club))
        .route("/api/club/preset", post(apply_club_preset))
        .route("/api/nation/update", post(update_nation))
        .route("/api/nation/preset", post(apply_nation_preset))
        .route("/api/competition/update", post(update_competition))
        .route("/api/competition/preset", post(apply_competition_preset))
        .route("/api/club/affiliate/add", post(add_club_affiliate))
        .route("/api/club/affiliate/remove", post(remove_club_affiliate))
        .route("/api/attach", post(attach_process))
        .route("/api/maps", get(get_maps))
        .route("/api/mem/read", post(read_memory))
        .route("/api/mem/write", post(write_memory))
        .route("/api/mem/scan", post(scan_memory))
        .route("/api/mem/scan/next", post(scan_memory_next))
        .route("/api/actions/heal", post(heal_action))
        .route("/api/actions/finances", post(finances_action))
        .route("/api/actions/godmode", post(godmode_action))
        .route("/api/offsets", get(get_offsets))
        .layer(CorsLayer::permissive())
        .with_state(state)
}
