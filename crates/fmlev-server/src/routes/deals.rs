//! Transfer Activity: read-only deal lists by club or player, and experimental fee / join-date edits.
//! Reads scan on a worker thread (no state lock held); the edit re-reads the deal at its address first.

use axum::{extract::State, http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Deserialize;

use fmlev_core::native_deals::{self, DealUpdate, NativeDeal};
use fmlev_mem::LinuxMemoryDriver;

use super::{api_error, core_status};
use crate::state::ServerState;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DealsClubRequest {
    pub club_id: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DealsPlayerRequest {
    pub player_id: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DealJoinDate {
    pub year: u16,
    /// 1-based day of year.
    pub day: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DealUpdateRequest {
    /// Address of the deal object, as returned by the read routes.
    pub deal_address: u64,
    pub player_id: i32,
    /// Raw GBP headline fee (+0x58).
    pub fee_gbp: Option<i64>,
    pub join_date: Option<DealJoinDate>,
    /// Unverified in game; callers must opt in explicitly.
    #[serde(default)]
    pub experimental: bool,
}

async fn session_driver(state: &ServerState) -> Result<LinuxMemoryDriver, Response> {
    let r = state.inner.read().await;
    r.context_mgr
        .session
        .as_ref()
        .map(|session| session.driver)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "Attach to FM26 with a save loaded first").into_response())
}

fn direction(deal: &NativeDeal, club_id: i32) -> &'static str {
    let buying = deal.buying_team.club_id == Some(club_id);
    let selling = deal.selling_team.as_ref().is_some_and(|t| t.club_id == Some(club_id));
    match (buying, selling) {
        (true, true) => "internal",
        (true, false) => "in",
        _ => "out",
    }
}

/// Labelled statuses first, then by join date (deals without one last).
fn sort_deals(deals: &mut [NativeDeal]) {
    deals.sort_by(|a, b| {
        let key = |d: &NativeDeal| (d.status_label.is_none(), d.join_date.as_ref().map_or((u16::MAX, u16::MAX), |j| (j.year, j.day)), d.address);
        key(a).cmp(&key(b))
    });
}

async fn scan(driver: LinuxMemoryDriver) -> Result<native_deals::DealScan, Response> {
    match tokio::task::spawn_blocking(move || native_deals::scan_process(&driver)).await {
        Ok(Ok(scan)) => Ok(scan),
        Ok(Err(error)) => Err(api_error(core_status(&error), error).into_response()),
        Err(error) => Err(api_error(StatusCode::INTERNAL_SERVER_ERROR, error).into_response()),
    }
}

/// Read-only: every deal where the club is the buyer or the seller.
pub async fn deals_for_club(State(state): State<ServerState>, Json(req): Json<DealsClubRequest>) -> impl IntoResponse {
    let driver = match session_driver(&state).await {
        Ok(driver) => driver,
        Err(response) => return response,
    };
    let scan = match scan(driver).await {
        Ok(scan) => scan,
        Err(response) => return response,
    };
    let mut deals: Vec<NativeDeal> = scan.deals.into_iter().filter(|d| d.involves_club(req.club_id)).collect();
    sort_deals(&mut deals);
    let rows: Vec<_> = deals.iter().map(|d| serde_json::json!({ "direction": direction(d, req.club_id), "deal": d })).collect();
    Json(serde_json::json!({
        "status": "ok", "club_id": req.club_id, "deals": rows,
        "scanned": { "candidates": scan.candidates, "rejected": scan.rejected, "rejected_reasons": scan.rejected_reasons },
    }))
    .into_response()
}

/// Read-only: every deal for one player id (staff and hybrid deals have no player id and never match).
pub async fn deals_for_player(State(state): State<ServerState>, Json(req): Json<DealsPlayerRequest>) -> impl IntoResponse {
    let driver = match session_driver(&state).await {
        Ok(driver) => driver,
        Err(response) => return response,
    };
    let scan = match scan(driver).await {
        Ok(scan) => scan,
        Err(response) => return response,
    };
    let mut deals: Vec<NativeDeal> = scan.deals.into_iter().filter(|d| d.player.id == Some(req.player_id)).collect();
    sort_deals(&mut deals);
    Json(serde_json::json!({
        "status": "ok", "player_id": req.player_id, "deals": deals,
        "scanned": { "candidates": scan.candidates, "rejected": scan.rejected, "rejected_reasons": scan.rejected_reasons },
    }))
    .into_response()
}

/// EXPERIMENTAL (unverified in game): changes the headline fee and/or join date of an existing deal. Status,
/// teams and clauses are never written.
pub async fn update_deal(State(state): State<ServerState>, Json(req): Json<DealUpdateRequest>) -> impl IntoResponse {
    if !req.experimental {
        return api_error(StatusCode::NOT_IMPLEMENTED, "Editing transfer deals is experimental and unverified in game: send experimental: true to opt in").into_response();
    }
    let driver = match session_driver(&state).await {
        Ok(driver) => driver,
        Err(response) => return response,
    };
    let Ok(address) = usize::try_from(req.deal_address) else {
        return api_error(StatusCode::BAD_REQUEST, "deal_address is out of range").into_response();
    };
    let update = DealUpdate { fee_gbp: req.fee_gbp, join_date: req.join_date.map(|d| (d.year, d.day)) };
    let player_id = req.player_id;
    match tokio::task::spawn_blocking(move || native_deals::update_deal(&driver, address, player_id, update)).await {
        Ok(Ok(deal)) => Json(serde_json::json!({
            "status": "ok", "experimental": true, "deal": deal,
            "message": "Deal updated in memory. Unverified in game: check Transfer Activity after the next day tick.",
        }))
        .into_response(),
        Ok(Err(error)) => api_error(core_status(&error), error).into_response(),
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}
