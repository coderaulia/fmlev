//! Competition extras (COMP detail fields, native prize table, league table) and awards.
//!
//! Reads are screen-independent and run on a worker thread with no state lock held. Every write is
//! experimental: it returns 501 unless the body says `experimental: true`, rejects unknown fields and
//! goes through a transactional native write (pre-read, write, readback, rollback).

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;

use fmlev_core::native_award::{self, AwardEdits};
use fmlev_core::native_competition_extra::{self as extra, ExtraEdits};
use fmlev_core::CoreError;
use fmlev_mem::LinuxMemoryDriver;

use super::{api_error, core_status};
use crate::state::ServerState;

const NOT_ATTACHED: &str = "Attach to FM26 with a save loaded first";
const GATE: &str = "This edit is unverified in game. Set experimental: true and use a throwaway save.";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompetitionKey {
    pub comp_index: i32,
    pub comp_uid: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompetitionDetailsUpdate {
    pub comp_index: i32,
    pub comp_uid: i32,
    pub original_reputation: Option<u16>,
    pub trophy_type: Option<u8>,
    pub lowest_category: Option<u8>,
    pub pitch_min_length: Option<u16>,
    pub pitch_min_width: Option<u16>,
    pub pitch_max_length: Option<u16>,
    pub pitch_max_width: Option<u16>,
    #[serde(default)]
    pub experimental: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePrizeUpdate {
    pub comp_index: i32,
    pub comp_uid: i32,
    /// Required when the competition has several league stages.
    pub stage_index: Option<usize>,
    /// One raw-GBP amount per position, position 1 first.
    pub prizes: Vec<i32>,
    #[serde(default)]
    pub experimental: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AwardKey {
    pub index: i32,
    pub uid: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AwardUpdate {
    pub index: i32,
    pub uid: i32,
    pub reputation_candidate_a: Option<u8>,
    pub reputation_candidate_b: Option<u8>,
    pub name: Option<String>,
    pub short_name: Option<String>,
    #[serde(default)]
    pub experimental: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AwardSearchQuery {
    pub q: Option<String>,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

/// Copies the session driver out of the global state so no lock is held during a scan.
async fn session_driver(state: &ServerState) -> Result<LinuxMemoryDriver, Response> {
    let r = state.inner.read().await;
    r.context_mgr.session.as_ref().map(|s| s.driver).ok_or_else(|| api_error(StatusCode::BAD_REQUEST, NOT_ATTACHED).into_response())
}

fn failure(error: &CoreError) -> Response {
    api_error(core_status(error), error).into_response()
}

fn gate() -> Response {
    api_error(StatusCode::NOT_IMPLEMENTED, GATE).into_response()
}

/// Runs a blocking native job off the async runtime and the state lock.
async fn blocking<T: Send + 'static>(job: impl FnOnce() -> Result<T, CoreError> + Send + 'static) -> Result<T, Response> {
    match tokio::task::spawn_blocking(job).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(failure(&error)),
        Err(error) => Err(api_error(StatusCode::INTERNAL_SERVER_ERROR, error).into_response()),
    }
}

pub async fn read_competition_details(State(state): State<ServerState>, Json(key): Json<CompetitionKey>) -> Response {
    let driver = match session_driver(&state).await {
        Ok(d) => d,
        Err(r) => return r,
    };
    match blocking(move || {
        let record = extra::locate_competition(&driver, key.comp_index, key.comp_uid)?;
        extra::read_extras(&driver, &record)
    })
    .await
    {
        Ok(details) => Json(serde_json::json!({
            "status": "ok",
            "details": details,
            "verification": "Read live on one build against 1,227 competitions (value ranges and the Premier League 1050 x 680 pitch). Not compared with an on-screen editor field; edits unverified in game."
        }))
        .into_response(),
        Err(r) => r,
    }
}

pub async fn update_competition_details(State(state): State<ServerState>, Json(req): Json<CompetitionDetailsUpdate>) -> Response {
    if !req.experimental {
        return gate();
    }
    let driver = match session_driver(&state).await {
        Ok(d) => d,
        Err(r) => return r,
    };
    let edits = ExtraEdits {
        original_reputation: req.original_reputation,
        trophy_type: req.trophy_type,
        lowest_category: req.lowest_category,
        pitch_min_length: req.pitch_min_length,
        pitch_min_width: req.pitch_min_width,
        pitch_max_length: req.pitch_max_length,
        pitch_max_width: req.pitch_max_width,
    };
    let _writes = state.inner.write().await;
    let (index, uid) = (req.comp_index, req.comp_uid);
    match blocking(move || {
        let record = extra::locate_competition(&driver, index, uid)?;
        let changed = extra::write_extras(&driver, &record, &edits)?;
        Ok((changed, extra::read_extras(&driver, &record)?))
    })
    .await
    {
        Ok((changed, details)) => Json(serde_json::json!({
            "status": "ok",
            "message": format!("{} competition field(s) written and read back. Reopen the competition in FM26.", changed.len()),
            "changed": changed,
            "details": details,
            "verification": "Experimental: transactional write with readback; not proven in game."
        }))
        .into_response(),
        Err(r) => r,
    }
}

/// Screen-independent prize table(s) for the competition's league stage(s).
pub async fn read_native_prizes(State(state): State<ServerState>, Json(key): Json<CompetitionKey>) -> Response {
    let driver = match session_driver(&state).await {
        Ok(d) => d,
        Err(r) => return r,
    };
    match blocking(move || {
        let record = extra::locate_competition(&driver, key.comp_index, key.comp_uid)?;
        extra::read_native_prizes(&driver, &record)
    })
    .await
    {
        Ok(tables) => Json(serde_json::json!({
            "status": "ok",
            "tables": tables,
            "verification": "Same i32 array the Rules-page value search finds, checked live for the Premier League, Bundesliga and Serie A. Leagues without a native stage chain (for example La Liga, the Indonesian Super League) are refused. Season-end payout source unconfirmed."
        }))
        .into_response(),
        Err(r) => r,
    }
}

pub async fn update_native_prizes(State(state): State<ServerState>, Json(req): Json<NativePrizeUpdate>) -> Response {
    if !req.experimental {
        return gate();
    }
    let driver = match session_driver(&state).await {
        Ok(d) => d,
        Err(r) => return r,
    };
    let _writes = state.inner.write().await;
    match blocking(move || {
        let record = extra::locate_competition(&driver, req.comp_index, req.comp_uid)?;
        extra::write_native_prizes(&driver, &record, req.stage_index, &req.prizes)
    })
    .await
    {
        Ok(table) => Json(serde_json::json!({
            "status": "ok",
            "message": "Prize table written and read back. Reopen Competition > Rules to see it; the season-end payout source is unconfirmed.",
            "table": table
        }))
        .into_response(),
        Err(r) => r,
    }
}

pub async fn read_competition_table(State(state): State<ServerState>, Json(key): Json<CompetitionKey>) -> Response {
    let driver = match session_driver(&state).await {
        Ok(d) => d,
        Err(r) => return r,
    };
    match blocking(move || {
        let record = extra::locate_competition(&driver, key.comp_index, key.comp_uid)?;
        extra::read_league_tables(&driver, &record)
    })
    .await
    {
        Ok(tables) => Json(serde_json::json!({
            "status": "ok",
            "tables": tables,
            "verification": "Read-only. Row order is the game's standings order; the row layout is heuristic (points = 3 x won + drawn held for every Premier League row). Not compared with the on-screen table."
        }))
        .into_response(),
        Err(r) => r,
    }
}

pub async fn search_awards(State(state): State<ServerState>, Query(query): Query<AwardSearchQuery>) -> Response {
    let driver = match session_driver(&state).await {
        Ok(d) => d,
        Err(r) => return r,
    };
    let (offset, limit) = (query.offset.unwrap_or(0), query.limit.unwrap_or(25).clamp(1, 100));
    let text = query.q.unwrap_or_default();
    match blocking(move || native_award::search_awards(&driver, &text, offset, limit)).await {
        Ok((total, awards)) => Json(serde_json::json!({ "status": "ok", "total": total, "offset": offset, "awards": awards })).into_response(),
        Err(r) => r,
    }
}

pub async fn read_award(State(state): State<ServerState>, Json(key): Json<AwardKey>) -> Response {
    let driver = match session_driver(&state).await {
        Ok(d) => d,
        Err(r) => return r,
    };
    match blocking(move || {
        let address = native_award::locate_award(&driver, key.index, key.uid)?;
        native_award::read_award(&driver, address)
    })
    .await
    {
        Ok(award) => Json(serde_json::json!({
            "status": "ok",
            "award": award,
            "verification": "Read live on one build (3,884 awards). The Studio-lead reputation/type offsets did not match this build and are not exposed; the two reputation candidates (+0x91/+0x92) have unproven meaning."
        }))
        .into_response(),
        Err(r) => r,
    }
}

pub async fn update_award(State(state): State<ServerState>, Json(req): Json<AwardUpdate>) -> Response {
    if !req.experimental {
        return gate();
    }
    let driver = match session_driver(&state).await {
        Ok(d) => d,
        Err(r) => return r,
    };
    let edits = AwardEdits { reputation_candidate_a: req.reputation_candidate_a, reputation_candidate_b: req.reputation_candidate_b, name: req.name, short_name: req.short_name };
    let _writes = state.inner.write().await;
    let (index, uid) = (req.index, req.uid);
    match blocking(move || {
        let address = native_award::locate_award(&driver, index, uid)?;
        let changed = native_award::write_award(&driver, address, &edits)?;
        Ok((changed, native_award::read_award(&driver, address)?))
    })
    .await
    {
        Ok((changed, award)) => Json(serde_json::json!({
            "status": "ok",
            "message": format!("{} award field(s) written and read back.", changed.len()),
            "changed": changed,
            "award": award,
            "verification": "Experimental: transactional write with readback; not proven in game. Names are rewritten in place within their existing length."
        }))
        .into_response(),
        Err(r) => r,
    }
}

#[cfg(test)]
mod tests {
    use super::super::create_router;
    use crate::state::{AppState, ServerState};
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use fmlev_core::GameContextManager;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tower::ServiceExt;

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

    async fn call(app: &axum::Router, method: &str, uri: &str, body: &str) -> (StatusCode, String) {
        let req = Request::builder().method(method).uri(uri).header("content-type", "application/json").body(Body::from(body.to_owned())).unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        (status, String::from_utf8(to_bytes(res.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap())
    }

    #[tokio::test]
    async fn writes_need_the_experimental_flag_and_unknown_fields_are_rejected() {
        let app = app();
        for (uri, body) in [
            ("/api/competition/details/update", r#"{"comp_index":7,"comp_uid":11,"trophy_type":2}"#),
            ("/api/competition/prizes/native/update", r#"{"comp_index":7,"comp_uid":11,"prizes":[1,2]}"#),
            ("/api/awards/update", r#"{"index":0,"uid":102406,"name":"X"}"#),
        ] {
            let (status, message) = call(&app, "POST", uri, body).await;
            assert_eq!(status, StatusCode::NOT_IMPLEMENTED, "{uri}: {message}");
            assert!(message.contains("experimental"), "{message}");
        }
        for (uri, body) in [
            ("/api/competition/details", r#"{"comp_index":7,"comp_uid":11,"extra":1}"#),
            ("/api/competition/details/update", r#"{"comp_index":7,"comp_uid":11,"experimental":true,"foot":1}"#),
            ("/api/competition/prizes/native", r#"{"comp_index":7}"#),
            ("/api/competition/table", r#"{"comp_index":7,"comp_uid":11,"x":0}"#),
            ("/api/awards/get", r#"{"index":0,"uid":1,"y":2}"#),
            ("/api/awards/update", r#"{"index":0,"uid":1,"experimental":true,"reputation":5}"#),
        ] {
            let (status, _) = call(&app, "POST", uri, body).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{uri}");
        }
    }

    #[tokio::test]
    async fn reads_and_experimental_writes_need_an_attached_game() {
        let app = app();
        for (uri, body) in [
            ("/api/competition/details", r#"{"comp_index":7,"comp_uid":11}"#),
            ("/api/competition/details/update", r#"{"comp_index":7,"comp_uid":11,"trophy_type":2,"experimental":true}"#),
            ("/api/competition/prizes/native", r#"{"comp_index":7,"comp_uid":11}"#),
            ("/api/competition/prizes/native/update", r#"{"comp_index":7,"comp_uid":11,"prizes":[1],"experimental":true}"#),
            ("/api/competition/table", r#"{"comp_index":7,"comp_uid":11}"#),
            ("/api/awards/get", r#"{"index":0,"uid":102406}"#),
            ("/api/awards/update", r#"{"index":0,"uid":102406,"name":"X","experimental":true}"#),
        ] {
            let (status, message) = call(&app, "POST", uri, body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {message}");
            assert!(message.contains("Attach"), "{message}");
        }
        let (status, message) = call(&app, "GET", "/api/awards?q=golden", "").await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{message}");
        let (status, _) = call(&app, "GET", "/api/awards?bogus=1", "").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
