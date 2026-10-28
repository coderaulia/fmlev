use axum::{extract::State, http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Deserialize;

use fmlev_core::native_staff::{NativeStaffRecord, MAX_SEARCH_RESULTS};
use fmlev_mem::LinuxMemoryDriver;

use super::{api_error, search};
use crate::state::ServerState;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaffSearchRequest {
    pub query: String,
    pub max: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaffNativeRequest {
    pub staff_id: i32,
    pub staff_uid: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaffUpdateRequest {
    pub staff_id: i32,
    pub staff_uid: i32,
    /// Attribute field -> display value 1..=20.
    pub attributes: std::collections::BTreeMap<String, u8>,
    /// Staff writes are not proven in game yet; callers must opt in explicitly.
    #[serde(default)]
    pub experimental: bool,
}

const NOT_ATTACHED: &str = "Attach to FM26 with a save loaded first";

/// Copies the session driver out of the global state, so no lock is held during a scan.
async fn session_driver(state: &ServerState) -> Result<LinuxMemoryDriver, Response> {
    let r = state.inner.read().await;
    r.context_mgr
        .session
        .as_ref()
        .map(|session| session.driver)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, NOT_ATTACHED).into_response())
}

/// Index hit revalidated by identity, else a full scan on a worker thread (no lock held).
async fn locate_staff(state: &ServerState, driver: LinuxMemoryDriver, id: i32, uid: i32) -> Result<NativeStaffRecord, Response> {
    if let Some(staff) = search::indexed_staff(state, &driver, id, uid) {
        return Ok(staff);
    }
    match tokio::task::spawn_blocking(move || NativeStaffRecord::discover_by_index(&driver, id)).await {
        Ok(Ok(staff)) if staff.uid == uid => Ok(staff),
        Ok(Ok(staff)) => Err(api_error(StatusCode::CONFLICT, format!("Staff {} has uid {}, not {}", staff.index, staff.uid, uid)).into_response()),
        Ok(Err(error)) => Err(api_error(StatusCode::NOT_FOUND, error).into_response()),
        Err(error) => Err(api_error(StatusCode::INTERNAL_SERVER_ERROR, error).into_response()),
    }
}

/// Read-only name search over native staff records (no screen needed). Uses the search index when it
/// is ready; otherwise scans on a worker thread.
pub async fn search_staff(State(state): State<ServerState>, Json(req): Json<StaffSearchRequest>) -> impl IntoResponse {
    let driver = match session_driver(&state).await {
        Ok(driver) => driver,
        Err(response) => return response,
    };
    let max = req.max.unwrap_or(20).clamp(1, MAX_SEARCH_RESULTS);
    if req.query.trim().chars().count() >= 2 {
        if let Some((staff, clubs)) = search::indexed_staff_search(&state, &driver, req.query.trim(), max) {
            return Json(serde_json::json!({ "status": "ok", "staff": staff, "clubs": clubs, "source": "index" })).into_response();
        }
    }
    let query = req.query.clone();
    match tokio::task::spawn_blocking(move || NativeStaffRecord::search(&driver, &query, max)).await {
        Ok(Ok(staff)) => Json(serde_json::json!({ "status": "ok", "staff": staff, "source": "scan" })).into_response(),
        Ok(Err(error)) => api_error(StatusCode::BAD_REQUEST, error).into_response(),
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaffOpenRequest {}

/// Read-only: the staff profile currently open in FM26, found from the game's UI bindings and then
/// resolved and validated natively (identity by id and uid). 404 when none is open, 409 when several
/// are bound, so the caller never guesses.
pub async fn open_staff(State(state): State<ServerState>, Json(_): Json<StaffOpenRequest>) -> impl IntoResponse {
    let (driver, ids) = {
        let r = state.inner.read().await;
        let Some(session) = r.context_mgr.session.as_ref() else {
            return api_error(StatusCode::BAD_REQUEST, NOT_ATTACHED).into_response();
        };
        (session.driver, fmlev_core::open_staff::read_open_staff_ids(session))
    };
    let id = match ids.as_slice() {
        [] => return api_error(StatusCode::NOT_FOUND, "No staff profile is open in FM26 (open one in the game, then try again)").into_response(),
        [id] => *id,
        many => return api_error(StatusCode::CONFLICT, format!("Several staff profiles are bound in FM26 ({many:?}); close the extra ones")).into_response(),
    };
    let from_index = search::indexed_staff_uid(&state, &driver, id);
    let staff = match from_index {
        Some(uid) => locate_staff(&state, driver, id, uid).await,
        None => match tokio::task::spawn_blocking(move || NativeStaffRecord::discover_by_index(&driver, id)).await {
            Ok(Ok(staff)) => Ok(staff),
            Ok(Err(error)) => Err(api_error(StatusCode::NOT_FOUND, error).into_response()),
            Err(error) => Err(api_error(StatusCode::INTERNAL_SERVER_ERROR, error).into_response()),
        },
    };
    match staff {
        Ok(staff) => Json(serde_json::json!({ "status": "ok", "staff": staff, "source": "ui_binding" })).into_response(),
        Err(response) => response,
    }
}

/// Read-only staff record and its 22 attributes, selected by id and uid.
pub async fn read_staff_native(State(state): State<ServerState>, Json(req): Json<StaffNativeRequest>) -> impl IntoResponse {
    let driver = match session_driver(&state).await {
        Ok(driver) => driver,
        Err(response) => return response,
    };
    let staff = match locate_staff(&state, driver, req.staff_id, req.staff_uid).await {
        Ok(staff) => staff,
        Err(response) => return response,
    };
    match staff.read_attributes(&driver) {
        Ok(attributes) => Json(serde_json::json!({
            "status": "ok",
            "staff": staff,
            "attributes": attributes,
            "verification": "Reads: 22 attributes matched against 7 on-screen staff profiles. Edits: experimental, readback-checked, not yet proven after a day tick and save/reload.",
        }))
        .into_response(),
        Err(error) => api_error(StatusCode::CONFLICT, error).into_response(),
    }
}

/// EXPERIMENTAL transactional staff attribute write (readback + rollback). Refused with 501 unless
/// `experimental` is set, because the write has not passed the in-game evidence ladder.
pub async fn update_staff(State(state): State<ServerState>, Json(req): Json<StaffUpdateRequest>) -> impl IntoResponse {
    if !req.experimental {
        return api_error(
            StatusCode::NOT_IMPLEMENTED,
            "Staff editing is unverified in game. Set experimental: true and use a throwaway save.",
        )
        .into_response();
    }
    let driver = match session_driver(&state).await {
        Ok(driver) => driver,
        Err(response) => return response,
    };
    let staff = match locate_staff(&state, driver, req.staff_id, req.staff_uid).await {
        Ok(staff) => staff,
        Err(response) => return response,
    };
    // The write lock serializes writes; the transaction re-checks identity before writing.
    let _writes = state.inner.write().await;
    let changes: Vec<(String, u8)> = req.attributes.into_iter().collect();
    match staff.write_attributes(&driver, &changes) {
        Ok(changed) => {
            let attributes = staff.read_attributes(&driver).ok();
            Json(serde_json::json!({
                "status": "ok",
                "message": format!("{} staff attribute(s) written and read back. Reopen the staff profile in FM26 to see them.", changed.len()),
                "changed": changed,
                "attributes": attributes,
                "verification": "Experimental: transactional write with readback; not yet proven after a day tick and save/reload.",
            }))
            .into_response()
        }
        Err(error) => api_error(StatusCode::BAD_REQUEST, error).into_response(),
    }
}

/// Read-only Information tab: identity plus the extra fields. Unverified offsets return `unavailable`.
pub async fn read_staff_information(State(state): State<ServerState>, Json(req): Json<StaffNativeRequest>) -> impl IntoResponse {
    let driver = match session_driver(&state).await {
        Ok(driver) => driver,
        Err(response) => return response,
    };
    let staff = match locate_staff(&state, driver, req.staff_id, req.staff_uid).await {
        Ok(staff) => staff,
        Err(response) => return response,
    };
    match staff.read_information(&driver) {
        Ok(fields) => Json(serde_json::json!({
            "status": "ok",
            "staff": staff,
            "fields": fields,
            "verification": "Identity (name, id, uid) is validated by RTTI vtables and duplicate uid. Other fields stay unavailable until verified in game.",
        }))
        .into_response(),
        Err(error) => api_error(StatusCode::CONFLICT, error).into_response(),
    }
}

/// Read-only Contract tab. The staff contract link is not mapped yet, so every field is `unavailable`.
pub async fn read_staff_contract(State(state): State<ServerState>, Json(req): Json<StaffNativeRequest>) -> impl IntoResponse {
    let driver = match session_driver(&state).await {
        Ok(driver) => driver,
        Err(response) => return response,
    };
    let staff = match locate_staff(&state, driver, req.staff_id, req.staff_uid).await {
        Ok(staff) => staff,
        Err(response) => return response,
    };
    match staff.read_contract(&driver) {
        Ok(contract) => Json(serde_json::json!({ "status": "ok", "staff": staff, "contract": contract })).into_response(),
        Err(error) => api_error(StatusCode::CONFLICT, error).into_response(),
    }
}

/// Staff information/contract writes have no verified layout, so they are never attempted.
pub async fn staff_write_unsupported() -> impl IntoResponse {
    api_error(
        StatusCode::NOT_IMPLEMENTED,
        "Staff information and contract writes are not supported: the layout is unverified (read-only discovery only).",
    )
}
