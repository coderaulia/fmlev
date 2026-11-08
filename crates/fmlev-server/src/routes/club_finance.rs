//! Club transfer embargo and financial statements (native finance object).
//! Reads are read-only; the embargo update is EXPERIMENTAL (501 without `experimental: true`).

use axum::{extract::State, http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Deserialize;

use fmlev_core::native_club::NativeClubRecord;
use fmlev_core::native_club_finance::{read_embargo, read_statements, write_embargo, EmbargoEdit};
use fmlev_mem::LinuxMemoryDriver;

use super::api_error;
use crate::state::ServerState;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClubFinanceReadRequest {
    pub club_id: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClubEmbargoUpdateRequest {
    pub club_id: i32,
    #[serde(flatten)]
    pub edit: EmbargoEdit,
    #[serde(default)]
    pub experimental: bool,
}

async fn driver_and_record(state: &ServerState, club_id: i32) -> Result<(LinuxMemoryDriver, NativeClubRecord), Response> {
    // Locate on a worker thread without the global lock (the locator cache keeps repeat calls cheap).
    super::search::warm(state, &[super::search::Warm::Club(club_id)]).await;
    let Some(driver) = ({
        let r = state.inner.read().await;
        r.context_mgr.session.as_ref().map(|session| session.driver)
    }) else {
        return Err(api_error(StatusCode::BAD_REQUEST, "Attach to FM26 with a save loaded first").into_response());
    };
    match tokio::task::spawn_blocking(move || NativeClubRecord::discover(&driver, club_id).map(|record| (driver, record))).await {
        Ok(Ok(found)) => Ok(found),
        Ok(Err(error)) => Err(api_error(StatusCode::NOT_FOUND, error).into_response()),
        Err(error) => Err(api_error(StatusCode::INTERNAL_SERVER_ERROR, error).into_response()),
    }
}

/// Read-only: transfer embargo dates, age restriction and flags of a club.
pub async fn read_club_embargo_native(State(state): State<ServerState>, Json(req): Json<ClubFinanceReadRequest>) -> impl IntoResponse {
    let (driver, record) = match driver_and_record(&state, req.club_id).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    match read_embargo(&driver, &record) {
        Ok(embargo) => Json(serde_json::json!({ "status": "ok", "embargo": embargo })).into_response(),
        Err(error) => api_error(StatusCode::CONFLICT, error).into_response(),
    }
}

/// EXPERIMENTAL scalar embargo edit (dates, age restriction, flag byte) with readback and rollback.
pub async fn update_club_embargo(State(state): State<ServerState>, Json(req): Json<ClubEmbargoUpdateRequest>) -> impl IntoResponse {
    if !req.experimental {
        return api_error(
            StatusCode::NOT_IMPLEMENTED,
            "Embargo editing is unverified in game. Set experimental: true and use a throwaway save.",
        )
        .into_response();
    }
    let (driver, record) = match driver_and_record(&state, req.club_id).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    // The write lock serializes writes; the transaction re-checks identity before writing.
    let _writes = state.inner.write().await;
    match write_embargo(&driver, &record, &req.edit) {
        Ok(embargo) => Json(serde_json::json!({
            "status": "ok",
            "message": "Embargo written and read back. Reopen the club page in FM26 to see it.",
            "embargo": embargo,
            "verification": "Experimental: transactional write with readback; not proven in game (no screen check, day tick or save/reload).",
        }))
        .into_response(),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error).into_response(),
    }
}

/// Read-only: this/last month and this/last season income and expenditure of a club (raw GBP).
pub async fn read_club_statements(State(state): State<ServerState>, Json(req): Json<ClubFinanceReadRequest>) -> impl IntoResponse {
    let (driver, record) = match driver_and_record(&state, req.club_id).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    match read_statements(&driver, &record) {
        Ok(statements) => Json(serde_json::json!({ "status": "ok", "statements": statements })).into_response(),
        Err(error) => api_error(StatusCode::CONFLICT, error).into_response(),
    }
}
