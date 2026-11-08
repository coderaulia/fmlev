use axum::{extract::State, http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Deserialize;

use super::{api_error, core_status};
use crate::state::ServerState;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadMorale {
    player_id: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateMorale {
    player_id: i32,
    player_uid: u32,
    expected: u8,
    value: u8,
}

async fn morale(state: ServerState, id: i32, edit: Option<(u32, u8, u8)>) -> Response {
    if id <= 0 {
        return api_error(StatusCode::BAD_REQUEST, "A positive player ID is required").into_response();
    }
    super::search::warm(&state, &[super::search::Warm::Player(id)]).await;
    let mut writes = state.inner.write().await;
    match writes.context_mgr.player_morale(id, edit) {
        Ok((uid, value)) => Json(serde_json::json!({ "player_id": id, "player_uid": uid, "value": value })).into_response(),
        Err(error) => api_error(core_status(&error), error).into_response(),
    }
}

pub async fn read(State(state): State<ServerState>, Json(req): Json<ReadMorale>) -> Response {
    morale(state, req.player_id, None).await
}

pub async fn update(State(state): State<ServerState>, Json(req): Json<UpdateMorale>) -> Response {
    if req.player_uid == 0 || !(1..=20).contains(&req.value) || !(1..=20).contains(&req.expected) {
        return api_error(StatusCode::BAD_REQUEST, "A player UID and morale values from 1 to 20 are required").into_response();
    }
    morale(state, req.player_id, Some((req.player_uid, req.expected, req.value))).await
}
