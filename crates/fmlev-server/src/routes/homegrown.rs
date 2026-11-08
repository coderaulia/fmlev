//! "Trained at club / nation" (homegrown) status of a player, from the relationship records.
//! Read: `POST /api/player/homegrown`. Write (experimental, allocates memory in the game process when the
//! relationship vector is full; throwaway save only): `POST /api/player/homegrown/set`.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;

use fmlev_core::native_catalog::RttiCatalog;
use fmlev_core::native_club::NativeClubRecord;
use fmlev_core::native_nation::NativeNationRecord;
use fmlev_core::native_player::NativePlayerRecord;
use fmlev_core::native_relationships as rel;
use fmlev_mem::{parse_process_maps, LinuxMemoryDriver};

use super::{api_error, core_status};
use crate::state::ServerState;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HomegrownReadRequest {
    pub player_id: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HomegrownSetRequest {
    pub player_id: i32,
    /// Native club id to mark as "trained at club".
    #[serde(default)]
    pub club_id: Option<i32>,
    /// Native nation id to mark as "trained at nation".
    #[serde(default)]
    pub nation_id: Option<i32>,
    /// Explicit flag: use the club of the player's parent contract. Never implied.
    #[serde(default)]
    pub club_current: bool,
    /// Explicit flag: use the player's first nationality. Never implied.
    #[serde(default)]
    pub nation_current: bool,
    #[serde(default)]
    pub experimental: bool,
}

async fn attached_driver(state: &ServerState) -> Option<LinuxMemoryDriver> {
    let r = state.inner.read().await;
    r.context_mgr.session.as_ref().map(|session| session.driver)
}

fn load(driver: &LinuxMemoryDriver, player_id: i32) -> fmlev_core::Result<(RttiCatalog, usize)> {
    let catalog = RttiCatalog::discover(driver, &parse_process_maps(driver.pid())?)?;
    let player = NativePlayerRecord::discover_by_index(driver, player_id)?;
    Ok((catalog, player.address()))
}

fn target_json(target: &rel::TargetRef) -> serde_json::Value {
    serde_json::json!({ "id": target.id, "name": target.name, "address": format!("0x{:x}", target.address) })
}

fn summary_json(
    driver: &LinuxMemoryDriver,
    catalog: &RttiCatalog,
    record: usize,
    summary: &rel::HomegrownSummary,
) -> serde_json::Value {
    let describe = |kind: rel::HomegrownKind, address: Option<usize>| address.map(|a| target_json(&rel::describe_target(driver, kind, a)));
    serde_json::json!({
        "status": "success",
        "clubs": summary.clubs.iter().map(target_json).collect::<Vec<_>>(),
        "nations": summary.nations.iter().map(target_json).collect::<Vec<_>>(),
        "unresolved_targets": summary.unresolved_targets,
        "relation_count": summary.relation_count,
        "spare_slots": summary.spare_slots,
        "current_club": describe(rel::HomegrownKind::Club, rel::current_club(driver, catalog, record)),
        "first_nationality": describe(rel::HomegrownKind::Nation, rel::first_nationality(driver, catalog, record)),
        "layout_note": "Relationship records at person+0x78 (category 1/72 = trained at club, 8/70 = trained at nation); layout read live on two players, heuristic.",
    })
}

/// Read-only. Lists the clubs and nations the game records as "trained at" for the player.
pub async fn read_homegrown(State(state): State<ServerState>, Json(req): Json<HomegrownReadRequest>) -> impl IntoResponse {
    let Some(driver) = attached_driver(&state).await else {
        return api_error(StatusCode::BAD_REQUEST, "Attach to FM26 first");
    };
    let result = tokio::task::spawn_blocking(move || {
        let (catalog, record) = load(&driver, req.player_id)?;
        let summary = rel::homegrown_summary(&driver, &catalog, record)?;
        Ok::<_, fmlev_core::CoreError>(summary_json(&driver, &catalog, record, &summary))
    })
    .await;
    match result {
        Ok(Ok(body)) => (StatusCode::OK, Json(body)),
        Ok(Err(error)) => api_error(core_status(&error), error),
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

/// EXPERIMENTAL write: appends the missing homegrown records. When the player's relationship vector is
/// full, a new array is allocated inside the game process (see `native_relationships` for the risk).
pub async fn set_homegrown(State(state): State<ServerState>, Json(req): Json<HomegrownSetRequest>) -> impl IntoResponse {
    if !req.experimental {
        return api_error(
            StatusCode::NOT_IMPLEMENTED,
            "Homegrown edits are experimental: they may allocate memory inside the game process. Send experimental: true on a throwaway save.",
        );
    }
    let club_wanted = req.club_id.is_some() || req.club_current;
    let nation_wanted = req.nation_id.is_some() || req.nation_current;
    if !club_wanted && !nation_wanted {
        return api_error(StatusCode::BAD_REQUEST, "Give club_id / nation_id, or club_current / nation_current: nothing to set");
    }
    if (req.club_id.is_some() && req.club_current) || (req.nation_id.is_some() && req.nation_current) {
        return api_error(StatusCode::BAD_REQUEST, "Use either an explicit id or the current flag for the club (and the nation), not both");
    }
    if req.club_id.is_some_and(|id| id <= 0) || req.nation_id.is_some_and(|id| id <= 0) {
        return api_error(StatusCode::BAD_REQUEST, "club_id and nation_id must be positive native ids");
    }
    let Some(driver) = attached_driver(&state).await else {
        return api_error(StatusCode::BAD_REQUEST, "Attach to FM26 first");
    };
    let result = tokio::task::spawn_blocking(move || {
        let (catalog, record) = load(&driver, req.player_id)?;
        let club = if let Some(id) = req.club_id {
            Some(NativeClubRecord::discover(&driver, id)?.club)
        } else if req.club_current {
            Some(rel::current_club(&driver, &catalog, record).ok_or_else(|| fmlev_core::CoreError::InvalidValue("The player has no resolvable current club".into()))?)
        } else {
            None
        };
        let nation = if let Some(id) = req.nation_id {
            Some(NativeNationRecord::discover(&driver, id)?.nation)
        } else if req.nation_current {
            Some(rel::first_nationality(&driver, &catalog, record).ok_or_else(|| fmlev_core::CoreError::InvalidValue("The player has no resolvable first nationality".into()))?)
        } else {
            None
        };
        let allocator = rel::PtraceAllocator { pid: driver.pid() };
        let report = rel::ensure_homegrown(&driver, &catalog, record, club, nation, &allocator)?;
        let summary = rel::homegrown_summary(&driver, &catalog, record)?;
        Ok::<_, fmlev_core::CoreError>((report, summary_json(&driver, &catalog, record, &summary)))
    })
    .await;
    match result {
        Ok(Ok((report, mut body))) => {
            body["report"] = serde_json::to_value(&report).unwrap_or_default();
            body["message"] = serde_json::Value::from(if report.new_array.is_some() {
                "Homegrown status written into a new array allocated in the game. Throwaway save only; FM26 shows it after reopening the player."
            } else {
                "Homegrown status written. FM26 shows it after reopening the player."
            });
            (StatusCode::OK, Json(body))
        }
        Ok(Err(error)) => api_error(core_status(&error), error),
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}
