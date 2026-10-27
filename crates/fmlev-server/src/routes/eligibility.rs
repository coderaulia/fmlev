use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;

use fmlev_core::eligibility;

use super::api_error;
use crate::state::ServerState;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EligibilityRequest {
    pub player_id: i32,
    pub comp_index: i32,
    pub comp_uid: i32,
    /// Exact in-game message pasted by the user; only echoed into the support report.
    #[serde(default)]
    pub game_message: String,
}

/// Read-only: reports each registration gate as ok, blocked or unknown. Unmapped gates are never "ok".
pub async fn player_eligibility(
    State(state): State<ServerState>,
    Json(req): Json<EligibilityRequest>,
) -> impl IntoResponse {
    if req.game_message.chars().count() > 1000 {
        return api_error(StatusCode::BAD_REQUEST, "game_message is limited to 1000 characters").into_response();
    }
    let r = state.inner.read().await;
    let gates = match r.context_mgr.read_registration_gates(req.comp_index, req.comp_uid) {
        Ok(gates) => gates,
        Err(error) => return api_error(StatusCode::BAD_REQUEST, error).into_response(),
    };
    let reports = eligibility::evaluate(&gates);
    let report = serde_json::json!({
        "fmlev_version": env!("CARGO_PKG_VERSION"),
        "player_id": req.player_id,
        "competition": { "index": req.comp_index, "uid": req.comp_uid, "type": gates.competition_type },
        "gates": gates,
        "results": reports,
        "game_message": req.game_message,
    });
    Json(serde_json::json!({
        "status": "ok",
        "player_note": "Player-specific gates (home-grown, work permit, appearances) are not mapped and are reported as unknown.",
        "gates": gates,
        "results": reports,
        "support_report": report,
    }))
    .into_response()
}
