use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;

use fmlev_core::role_presets::{self, CaMode};

use super::api_error;
use crate::state::ServerState;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RolePresetRequest {
    pub player_id: i32,
    pub role_id: String,
    pub level: u8,
    pub ca_mode: Option<CaMode>,
}

pub async fn list_roles() -> impl IntoResponse {
    let roles: Vec<_> = role_presets::ROLES
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id, "name": r.name, "group": r.group, "positions": r.positions,
                "key": r.key, "secondary": r.secondary,
            })
        })
        .collect();
    let levels: Vec<_> = (1u8..=3)
        .map(|level| {
            let (key, secondary, position) = role_presets::level_floors(level).unwrap_or((0, 0, 0));
            serde_json::json!({
                "level": level, "key_floor": key, "secondary_floor": secondary, "position_floor": position,
                "default_ca_mode": role_presets::default_ca_mode(level),
            })
        })
        .collect();
    Json(serde_json::json!({
        "status": "ok",
        "roles": roles,
        "role_data_verified": role_presets::ROLE_DATA_VERIFIED,
        "levels": levels,
    }))
}

/// Returns a proposal (field old -> new). Nothing is written; the UI saves it through the player update path.
pub async fn role_preset_proposal(
    State(state): State<ServerState>,
    Json(req): Json<RolePresetRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    let current = w.context_mgr.get_context().player;
    let player = if current.id == req.player_id {
        current
    } else {
        match w.context_mgr.select_player(req.player_id) {
            Ok(player) => player,
            Err(error) => return api_error(StatusCode::NOT_FOUND, error).into_response(),
        }
    };
    match role_presets::propose(
        &req.role_id,
        req.level,
        req.ca_mode,
        &player.attributes,
        &player.positions,
        player.current_ability,
        player.potential_ability,
    ) {
        Ok(proposal) => Json(serde_json::json!({ "status": "ok", "player_id": player.id, "proposal": proposal })).into_response(),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error).into_response(),
    }
}
