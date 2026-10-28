pub mod context;
pub mod eligibility;
pub mod memory;
pub mod roles;
pub mod settings;
pub mod staff;

pub use context::*;
pub use eligibility::*;
pub use memory::*;
pub use roles::*;
pub use settings::*;
pub use staff::*;

use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};

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
        .route("/api/settings", get(get_settings).post(update_settings))
        .route("/api/context", get(get_context))
        .route("/api/search", get(search_entities))
        .route("/api/players/discover", post(discover_players))
        .route("/api/context/select", post(select_context))
        .route("/api/player/update", post(update_player))
        .route("/api/player/eligibility", post(player_eligibility))
        .route("/api/staff/search", post(search_staff))
        .route("/api/staff/native", post(read_staff_native))
        .route("/api/staff/update", post(update_staff))
        .route("/api/staff/information", post(read_staff_information))
        .route("/api/staff/contract", post(read_staff_contract))
        .route("/api/staff/information/update", post(staff_write_unsupported))
        .route("/api/staff/contract/update", post(staff_write_unsupported))
        .route("/api/roles", get(list_roles))
        .route("/api/player/role-preset", post(role_preset_proposal))
        .route("/api/player/preset", post(apply_player_preset))
        .route("/api/player/squad-number", post(update_squad_number))
        .route("/api/player/contract/extend", post(extend_contract))
        .route("/api/player/contract/terminate", post(terminate_contract))
        .route("/api/player/loan/terminate", post(terminate_loan))
        .route("/api/transfer/move", post(move_player))
        .route("/api/transfer/sign", post(sign_player))
        .route("/api/transfer/accelerate", post(accelerate_transfer))
        .route("/api/transfer/loan", post(transfer_not_implemented))
        .route("/api/player/familiarity", post(maximise_familiarity))
        .route("/api/club/update", post(update_club))
        .route("/api/club/money", post(update_club_money))
        .route("/api/club/native", post(read_club_native))
        .route("/api/nation/native", post(read_nation_native))
        .route("/api/competition/native", post(read_competition_native))
        .route("/api/club/preset", post(apply_club_preset))
        .route("/api/nation/update", post(update_nation))
        .route("/api/nation/preset", post(apply_nation_preset))
        .route("/api/competition/update", post(update_competition))
        .route("/api/competition/preset", post(apply_competition_preset))
        .route("/api/competition/prizes", post(competition_prizes))
        .route("/api/competition/registration", post(registration_anytime))
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
        .layer(middleware::from_fn(local_requests_only))
        .with_state(state)
}

/// True for `localhost`, `127.0.0.1` or `[::1]`, with or without scheme, port and path.
fn is_local_authority(value: &str) -> bool {
    let rest = value.split_once("://").map_or(value, |(_, rest)| rest);
    let authority = rest.split('/').next().unwrap_or("");
    let host = if authority.starts_with('[') {
        authority.split(']').next().map(|h| format!("{h}]")).unwrap_or_default()
    } else {
        authority.split(':').next().unwrap_or("").to_string()
    };
    matches!(host.to_ascii_lowercase().as_str(), "localhost" | "127.0.0.1" | "[::1]")
}

/// Every route can write FM26 memory, so only the app's own pages and local tools may call it:
/// a foreign `Origin` (another website) or `Host` (DNS rebinding) is rejected. Requests without
/// an `Origin` header (curl, scripts) are allowed because the server only listens on 127.0.0.1.
async fn local_requests_only(request: Request, next: Next) -> Response {
    let headers = request.headers();
    let foreign = |name| {
        headers
            .get(name)
            .is_some_and(|v: &axum::http::HeaderValue| !v.to_str().is_ok_and(is_local_authority))
    };
    if foreign(header::HOST) || foreign(header::ORIGIN) {
        return api_error(StatusCode::FORBIDDEN, "Requests are only accepted from the local fmlev UI").into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::is_local_authority;

    #[test]
    fn only_loopback_authorities_are_local() {
        for ok in ["localhost:41789", "http://127.0.0.1:41789", "http://localhost", "[::1]:41789", "http://LOCALHOST:41789/x"] {
            assert!(is_local_authority(ok), "{ok}");
        }
        for bad in ["evil.com", "http://evil.com:41789", "localhost.evil.com", "http://127.0.0.1.evil.com", "null", ""] {
            assert!(!is_local_authority(bad), "{bad}");
        }
    }
}
