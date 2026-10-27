use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;

use fmlev_core::native_staff::NativeStaffRecord;

use super::api_error;
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

/// Read-only name search over native staff records (no screen needed).
pub async fn search_staff(State(state): State<ServerState>, Json(req): Json<StaffSearchRequest>) -> impl IntoResponse {
    let r = state.inner.read().await;
    let Some(session) = r.context_mgr.session.as_ref() else {
        return api_error(StatusCode::BAD_REQUEST, NOT_ATTACHED).into_response();
    };
    match NativeStaffRecord::search(&session.driver, &req.query, req.max.unwrap_or(20)) {
        Ok(staff) => Json(serde_json::json!({ "status": "ok", "staff": staff })).into_response(),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error).into_response(),
    }
}

/// Read-only staff record and its 22 attributes, selected by id and uid.
pub async fn read_staff_native(State(state): State<ServerState>, Json(req): Json<StaffNativeRequest>) -> impl IntoResponse {
    let r = state.inner.read().await;
    let Some(session) = r.context_mgr.session.as_ref() else {
        return api_error(StatusCode::BAD_REQUEST, NOT_ATTACHED).into_response();
    };
    let staff = match NativeStaffRecord::discover_by_index(&session.driver, req.staff_id) {
        Ok(staff) if staff.uid == req.staff_uid => staff,
        Ok(staff) => {
            return api_error(StatusCode::CONFLICT, format!("Staff {} has uid {}, not {}", staff.index, staff.uid, req.staff_uid)).into_response()
        }
        Err(error) => return api_error(StatusCode::NOT_FOUND, error).into_response(),
    };
    match staff.read_attributes(&session.driver) {
        Ok(attributes) => Json(serde_json::json!({
            "status": "ok",
            "staff": staff,
            "attributes": attributes,
            "verification": "Read-only. 22 attributes matched against 7 on-screen staff profiles; writes are not supported.",
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
    let w = state.inner.write().await;
    let Some(session) = w.context_mgr.session.as_ref() else {
        return api_error(StatusCode::BAD_REQUEST, NOT_ATTACHED).into_response();
    };
    let staff = match NativeStaffRecord::discover_by_index(&session.driver, req.staff_id) {
        Ok(staff) if staff.uid == req.staff_uid => staff,
        Ok(staff) => {
            return api_error(StatusCode::CONFLICT, format!("Staff {} has uid {}, not {}", staff.index, staff.uid, req.staff_uid)).into_response()
        }
        Err(error) => return api_error(StatusCode::NOT_FOUND, error).into_response(),
    };
    let changes: Vec<(String, u8)> = req.attributes.into_iter().collect();
    match staff.write_attributes(&session.driver, &changes) {
        Ok(changed) => {
            let attributes = staff.read_attributes(&session.driver).ok();
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
