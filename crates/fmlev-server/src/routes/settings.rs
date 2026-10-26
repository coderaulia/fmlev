use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;

use fmlev_core::settings::{self, DisplaySettings, Measurement, WagePeriod};

use super::api_error;
use crate::state::ServerState;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsRequest {
    pub currency: Option<String>,
    pub measurement: Option<Measurement>,
    pub wage_period: Option<WagePeriod>,
}

fn settings_json(current: &DisplaySettings) -> serde_json::Value {
    serde_json::json!({
        "status": "ok",
        "settings": current,
        "currencies": settings::CURRENCIES,
        "rates_date": settings::RATES_DATE,
        "weeks_per_year": settings::WEEKS_PER_YEAR,
    })
}

pub async fn get_settings(State(state): State<ServerState>) -> impl IntoResponse {
    Json(settings_json(&state.inner.read().await.settings))
}

pub async fn update_settings(
    State(state): State<ServerState>,
    Json(req): Json<SettingsRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    let mut next = w.settings.clone();
    if let Some(code) = req.currency {
        next.currency = code;
    }
    if let Some(measurement) = req.measurement {
        next.measurement = measurement;
    }
    if let Some(period) = req.wage_period {
        next.wage_period = period;
    }
    if let Err(error) = next.validate() {
        return api_error(StatusCode::BAD_REQUEST, error).into_response();
    }
    if let Some(path) = w.settings_path.clone() {
        if let Err(error) = next.save_to(&path) {
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, error).into_response();
        }
    }
    w.settings = next;
    Json(settings_json(&w.settings)).into_response()
}
