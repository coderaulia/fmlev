//! Club board objectives and club status: read routes and EXPERIMENTAL scalar writes (unverified in game).

use axum::{extract::State, http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Deserialize;

use fmlev_core::native_board::{self, ObjectiveEdit};
use fmlev_core::native_club::NativeClubRecord;
use fmlev_mem::LinuxMemoryDriver;

use super::{api_error, core_status};
use crate::state::ServerState;

const NOT_ATTACHED: &str = "Attach to FM26 with a save loaded first";
const BOARD_UNVERIFIED: &str =
    "Board objective editing is unverified in game. Set experimental: true and use a throwaway save.";
const STATUS_UNVERIFIED: &str =
    "Club status editing is unverified in game. Set experimental: true and use a throwaway save.";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClubBoardRequest {
    pub club_id: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClubBoardUpdateRequest {
    pub club_id: i32,
    /// Position in the objective list as returned by `/api/club/board`.
    pub objective_index: usize,
    /// Type code the caller saw at that position; a mismatch refuses the write.
    pub type_code: u8,
    pub importance: Option<u8>,
    pub value: Option<i32>,
    pub deadline_year: Option<u16>,
    pub deadline_day: Option<u16>,
    pub status_code: Option<u8>,
    #[serde(default)]
    pub experimental: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClubStatusUpdateRequest {
    pub club_id: i32,
    /// 1 professional, 2 semi-professional, 3 amateur.
    pub status: u8,
    #[serde(default)]
    pub experimental: bool,
}

async fn session_driver(state: &ServerState) -> Result<LinuxMemoryDriver, Response> {
    let r = state.inner.read().await;
    r.context_mgr
        .session
        .as_ref()
        .map(|session| session.driver)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, NOT_ATTACHED).into_response())
}

/// Cached club record lookup on a worker thread (a cold scan never runs on the async runtime or under a lock).
async fn locate_club(driver: LinuxMemoryDriver, club_id: i32) -> Result<NativeClubRecord, Response> {
    match tokio::task::spawn_blocking(move || NativeClubRecord::discover(&driver, club_id)).await {
        Ok(Ok(club)) => Ok(club),
        Ok(Err(error)) => Err(api_error(core_status(&error), error).into_response()),
        Err(error) => Err(api_error(StatusCode::INTERNAL_SERVER_ERROR, error).into_response()),
    }
}

/// Read-only board objectives of a club (labels from a community catalog, not compared with the game screen).
pub async fn read_club_board(State(state): State<ServerState>, Json(req): Json<ClubBoardRequest>) -> impl IntoResponse {
    let driver = match session_driver(&state).await {
        Ok(driver) => driver,
        Err(response) => return response,
    };
    let club = match locate_club(driver, req.club_id).await {
        Ok(club) => club,
        Err(response) => return response,
    };
    match native_board::read_board(&driver, &club) {
        Ok(report) => Json(serde_json::json!({ "status": "ok", "board": report })).into_response(),
        Err(error) => api_error(core_status(&error), error).into_response(),
    }
}

/// EXPERIMENTAL single-scalar writes on one objective (importance, target value, deadline, status code).
pub async fn update_club_board(State(state): State<ServerState>, Json(req): Json<ClubBoardUpdateRequest>) -> impl IntoResponse {
    if !req.experimental {
        return api_error(StatusCode::NOT_IMPLEMENTED, BOARD_UNVERIFIED).into_response();
    }
    let deadline = match (req.deadline_year, req.deadline_day) {
        (Some(year), Some(day)) => Some((year, day)),
        (None, None) => None,
        _ => return api_error(StatusCode::BAD_REQUEST, "deadline_year and deadline_day must be given together").into_response(),
    };
    let edit = ObjectiveEdit { importance: req.importance, value: req.value, deadline, status: req.status_code };
    if edit == ObjectiveEdit::default() {
        return api_error(StatusCode::BAD_REQUEST, "No objective field to change").into_response();
    }
    let driver = match session_driver(&state).await {
        Ok(driver) => driver,
        Err(response) => return response,
    };
    let club = match locate_club(driver, req.club_id).await {
        Ok(club) => club,
        Err(response) => return response,
    };
    let _writes = state.inner.write().await;
    match native_board::write_objective(&driver, &club, req.objective_index, req.type_code, &edit) {
        Ok(objective) => Json(serde_json::json!({
            "status": "ok",
            "message": "Objective value(s) written and read back. Reopen the board screen in FM26 to see them.",
            "objective": objective,
            "verification": "Experimental: transactional write with readback; not yet proven in game (screen, day tick, save/reload).",
        }))
        .into_response(),
        Err(error) => api_error(core_status(&error), error).into_response(),
    }
}

/// Read-only club status byte (professional / semi-professional / amateur, inferred).
pub async fn read_club_status(State(state): State<ServerState>, Json(req): Json<ClubBoardRequest>) -> impl IntoResponse {
    let driver = match session_driver(&state).await {
        Ok(driver) => driver,
        Err(response) => return response,
    };
    let club = match locate_club(driver, req.club_id).await {
        Ok(club) => club,
        Err(response) => return response,
    };
    match native_board::read_club_status(&driver, &club) {
        Ok(status) => Json(serde_json::json!({ "status": "ok", "club_status": status })).into_response(),
        Err(error) => api_error(core_status(&error), error).into_response(),
    }
}

/// EXPERIMENTAL write of the club status byte (1..=3 only, single byte with readback and rollback).
pub async fn update_club_status(State(state): State<ServerState>, Json(req): Json<ClubStatusUpdateRequest>) -> impl IntoResponse {
    if !req.experimental {
        return api_error(StatusCode::NOT_IMPLEMENTED, STATUS_UNVERIFIED).into_response();
    }
    let driver = match session_driver(&state).await {
        Ok(driver) => driver,
        Err(response) => return response,
    };
    let club = match locate_club(driver, req.club_id).await {
        Ok(club) => club,
        Err(response) => return response,
    };
    let _writes = state.inner.write().await;
    match native_board::write_club_status(&driver, &club, req.status) {
        Ok(status) => Json(serde_json::json!({
            "status": "ok",
            "message": "Club status written and read back. Reopen the club screen in FM26 to see it.",
            "club_status": status,
            "verification": "Experimental: transactional write with readback; not yet proven in game.",
        }))
        .into_response(),
        Err(error) => api_error(core_status(&error), error).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tower::ServiceExt;

    use crate::routes::create_router;
    use crate::state::{AppState, ServerState};
    use fmlev_core::GameContextManager;

    fn app() -> axum::Router {
        let inner = AppState {
            pid: None,
            exe_path: None,
            is_proton: false,
            wine_prefix: None,
            game_assembly_base: None,
            unity_player_base: None,
            fm_exe_base: None,
            is_attached: false,
            scan_results: Vec::new(),
            last_scan_type: String::new(),
            context_mgr: GameContextManager::new_offline(),
            settings: Default::default(),
            settings_path: None,
        };
        create_router(ServerState::from_inner(Arc::new(RwLock::new(inner))))
    }

    fn post(uri: &str, body: &str) -> Request<Body> {
        Request::post(uri).header("content-type", "application/json").body(Body::from(body.to_string())).unwrap()
    }

    async fn status(uri: &str, body: &str) -> u16 {
        app().oneshot(post(uri, body)).await.unwrap().status().as_u16()
    }

    #[tokio::test]
    async fn writes_need_the_experimental_flag_before_anything_else() {
        let update = r#"{"club_id":1089,"objective_index":0,"type_code":1,"importance":5}"#;
        assert_eq!(status("/api/club/board/update", update).await, 501);
        assert_eq!(status("/api/club/status/update", r#"{"club_id":1089,"status":2}"#).await, 501);
        // With the flag, a detached server is a 400, not a write.
        let flagged = r#"{"club_id":1089,"objective_index":0,"type_code":1,"importance":5,"experimental":true}"#;
        assert_eq!(status("/api/club/board/update", flagged).await, 400);
        assert_eq!(status("/api/club/status/update", r#"{"club_id":1089,"status":2,"experimental":true}"#).await, 400);
    }

    #[tokio::test]
    async fn validation_unknown_fields_and_detached_reads() {
        assert_eq!(status("/api/club/board", r#"{"club_id":1089}"#).await, 400);
        assert_eq!(status("/api/club/status", r#"{"club_id":1089}"#).await, 400);
        for (uri, body) in [
            ("/api/club/board", r#"{"club_id":1,"extra":1}"#),
            ("/api/club/status", r#"{"club_id":1,"extra":1}"#),
            ("/api/club/board/update", r#"{"club_id":1,"objective_index":0,"type_code":1,"importance":5,"experimental":true,"extra":1}"#),
            ("/api/club/status/update", r#"{"club_id":1,"status":2,"experimental":true,"extra":1}"#),
        ] {
            assert!(status(uri, body).await >= 400, "{uri}");
            assert_ne!(status(uri, body).await, 501, "{uri}");
        }
        // No field, half a deadline: refused before any memory access.
        let none = r#"{"club_id":1,"objective_index":0,"type_code":1,"experimental":true}"#;
        assert_eq!(status("/api/club/board/update", none).await, 400);
        let half = r#"{"club_id":1,"objective_index":0,"type_code":1,"deadline_year":2027,"experimental":true}"#;
        assert_eq!(status("/api/club/board/update", half).await, 400);
    }
}
