//! Club bank loans and sponsor deals: native reads, and EXPERIMENTAL scalar edits (501 without `experimental: true`).
//! Nothing here resizes a list; see `fmlev_core::native_club_loans` and `native_club_sponsors`.

use axum::{extract::State, http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Deserialize;

use fmlev_core::native_club::NativeClubRecord;
use fmlev_core::native_club_loans::LoanEdit;
use fmlev_core::native_club_sponsors::SponsorEdit;
use fmlev_mem::LinuxMemoryDriver;

use super::{api_error, core_status};
use crate::state::ServerState;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClubDebtsRequest {
    pub club_id: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClubLoanUpdateRequest {
    pub club_id: i32,
    pub loan_index: usize,
    /// Address from the last read; the edit is refused when the record at that index changed.
    pub loan_address: Option<String>,
    pub original_debt: Option<u32>,
    pub monthly_repayment: Option<u32>,
    pub monthly_interest: Option<u32>,
    pub conditional_repayment: Option<u32>,
    pub start_day: Option<u16>,
    pub start_year: Option<u16>,
    pub end_day: Option<u16>,
    pub end_year: Option<u16>,
    pub loan_type: Option<u8>,
    pub include_in_ffp: Option<bool>,
    pub interest_only: Option<bool>,
    /// Zeroes the amounts (the loan stays in the list).
    #[serde(default)]
    pub clear: bool,
    #[serde(default)]
    pub experimental: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClubSponsorUpdateRequest {
    pub club_id: i32,
    pub sponsor_index: usize,
    pub sponsor_address: Option<String>,
    pub annual_value: Option<u32>,
    pub total_value: Option<u32>,
    #[serde(default)]
    pub experimental: bool,
}

const NOT_ATTACHED: &str = "Attach to FM26 with a save loaded first";
const UNVERIFIED: &str = "Experimental: transactional scalar write with readback; unverified in game (no screen check, day tick or save/reload yet). Use a throwaway save.";

async fn session_driver(state: &ServerState) -> Result<LinuxMemoryDriver, Response> {
    let r = state.inner.read().await;
    r.context_mgr
        .session
        .as_ref()
        .map(|session| session.driver)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, NOT_ATTACHED).into_response())
}

/// Finds the validated native club on a worker thread (cached; a miss scans memory), without holding the global lock.
async fn locate_club(driver: LinuxMemoryDriver, club_id: i32) -> Result<NativeClubRecord, Response> {
    match tokio::task::spawn_blocking(move || NativeClubRecord::discover(&driver, club_id)).await {
        Ok(Ok(record)) => Ok(record),
        Ok(Err(error)) => Err(api_error(core_status(&error), error).into_response()),
        Err(error) => Err(api_error(StatusCode::INTERNAL_SERVER_ERROR, error).into_response()),
    }
}

fn gate(experimental: bool, what: &str) -> Option<Response> {
    (!experimental).then(|| {
        api_error(StatusCode::NOT_IMPLEMENTED, format!("{what} editing is unverified in game. Set experimental: true and use a throwaway save.")).into_response()
    })
}

pub async fn read_club_loans(State(state): State<ServerState>, Json(req): Json<ClubDebtsRequest>) -> impl IntoResponse {
    let driver = match session_driver(&state).await {
        Ok(driver) => driver,
        Err(response) => return response,
    };
    let record = match locate_club(driver, req.club_id).await {
        Ok(record) => record,
        Err(response) => return response,
    };
    match tokio::task::spawn_blocking(move || record.read_loans(&driver)).await {
        Ok(Ok(loans)) => Json(serde_json::json!({
            "status": "success", "club_id": req.club_id, "loans": loans,
            "verification": "Read from native memory; unverified against the game's Finances screen. Loan type codes are not mapped.",
        }))
        .into_response(),
        Ok(Err(error)) => api_error(core_status(&error), error).into_response(),
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}

pub async fn update_club_loan(State(state): State<ServerState>, Json(req): Json<ClubLoanUpdateRequest>) -> impl IntoResponse {
    if let Some(response) = gate(req.experimental, "Club loan") {
        return response;
    }
    let driver = match session_driver(&state).await {
        Ok(driver) => driver,
        Err(response) => return response,
    };
    let record = match locate_club(driver, req.club_id).await {
        Ok(record) => record,
        Err(response) => return response,
    };
    let edit = LoanEdit {
        original_debt: req.original_debt,
        monthly_repayment: req.monthly_repayment,
        monthly_interest: req.monthly_interest,
        conditional_repayment: req.conditional_repayment,
        start_day: req.start_day,
        start_year: req.start_year,
        end_day: req.end_day,
        end_year: req.end_year,
        loan_type: req.loan_type,
        include_in_ffp: req.include_in_ffp,
        interest_only: req.interest_only,
        clear: req.clear,
    };
    let (index, address) = (req.loan_index, req.loan_address.clone());
    let _writes = state.inner.write().await;
    match tokio::task::spawn_blocking(move || record.write_loan(&driver, index, address.as_deref(), &edit)).await {
        Ok(Ok(loan)) => Json(serde_json::json!({
            "status": "ok", "club_id": req.club_id, "loan": loan,
            "message": "Loan written and read back. Reopen Club > Finances in FM26 to see it.",
            "verification": UNVERIFIED,
        }))
        .into_response(),
        Ok(Err(error)) => api_error(core_status(&error), error).into_response(),
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}

pub async fn read_club_sponsors(State(state): State<ServerState>, Json(req): Json<ClubDebtsRequest>) -> impl IntoResponse {
    let driver = match session_driver(&state).await {
        Ok(driver) => driver,
        Err(response) => return response,
    };
    let record = match locate_club(driver, req.club_id).await {
        Ok(record) => record,
        Err(response) => return response,
    };
    match tokio::task::spawn_blocking(move || record.read_sponsors(&driver)).await {
        Ok(Ok(list)) => Json(serde_json::json!({
            "status": "success", "club_id": req.club_id, "list_present": list.list_present, "sponsors": list.sponsors,
            "verification": "Read from native memory; unverified against the game's Sponsors screen. Sponsor names and type labels are not mapped (labels are candidates only).",
        }))
        .into_response(),
        Ok(Err(error)) => api_error(core_status(&error), error).into_response(),
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}

pub async fn update_club_sponsor(State(state): State<ServerState>, Json(req): Json<ClubSponsorUpdateRequest>) -> impl IntoResponse {
    if let Some(response) = gate(req.experimental, "Club sponsor") {
        return response;
    }
    let driver = match session_driver(&state).await {
        Ok(driver) => driver,
        Err(response) => return response,
    };
    let record = match locate_club(driver, req.club_id).await {
        Ok(record) => record,
        Err(response) => return response,
    };
    let edit = SponsorEdit { annual_value: req.annual_value, total_value: req.total_value };
    let (index, address) = (req.sponsor_index, req.sponsor_address.clone());
    let _writes = state.inner.write().await;
    match tokio::task::spawn_blocking(move || record.write_sponsor(&driver, index, address.as_deref(), &edit)).await {
        Ok(Ok(sponsor)) => Json(serde_json::json!({
            "status": "ok", "club_id": req.club_id, "sponsor": sponsor,
            "message": "Sponsor value written and read back. Reopen Club > Finances in FM26 to see it.",
            "verification": UNVERIFIED,
        }))
        .into_response(),
        Ok(Err(error)) => api_error(core_status(&error), error).into_response(),
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}
