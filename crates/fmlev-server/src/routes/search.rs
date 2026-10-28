//! Global search over the background index of the loaded database (plan G).
//!
//! The index is built on a worker thread by RTTI enumeration (`fmlev_core::native_catalog`) and kept in
//! `ServerState::search`, apart from the global state lock, so queries never wait for a scan. Without a
//! ready index for the attached process, search falls back to the in-memory registries.

use std::time::Instant;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::{json, Value};

use fmlev_core::native_staff::NativeStaffRecord;
use fmlev_core::search_index::{EntityKind, IndexEntry, SearchIndex, DEFAULT_LIMIT, MAX_LIMIT};
use fmlev_core::{GameContextManager, SearchResults};
use fmlev_mem::LinuxMemoryDriver;

use super::api_error;
use crate::state::{SearchQuery, SearchState, ServerState};

/// Driver of an attached game process; `None` offline or for the in-process test session.
pub(crate) fn live_driver(mgr: &GameContextManager) -> Option<LinuxMemoryDriver> {
    mgr.is_live_session().then(|| mgr.session.as_ref().map(|session| session.driver)).flatten()
}

pub(crate) fn status_json(search: &SearchState) -> Value {
    let state = if search.building {
        "building"
    } else if search.error.is_some() {
        "error"
    } else if search.index.is_some() {
        "ready"
    } else {
        "idle"
    };
    let index = search.index.as_ref();
    let counts = index.map_or_else(
        || EntityKind::ALL.iter().map(|kind| (kind.plural().to_string(), json!(0))).collect(),
        |index| index.counts().into_iter().map(|(kind, n)| (kind.to_string(), json!(n))).collect::<serde_json::Map<_, _>>(),
    );
    json!({
        "state": state,
        "stage": search.stage,
        "counts": counts,
        "built_at": index.map(|i| i.built_at_unix),
        "duration_ms": index.map(|i| i.duration_ms),
        "error": search.error,
        "pid": index.map(|i| i.pid).or(search.attempted_pid),
        "duplicates_dropped": index.map_or(0, |i| i.duplicates_dropped),
    })
}

/// Starts a background build for `driver`'s process. Returns false when one is already running.
pub(crate) fn start_index_build(state: &ServerState, driver: LinuxMemoryDriver) -> bool {
    {
        let mut search = state.search_write();
        if search.building {
            return false;
        }
        search.building = true;
        search.stage = Some("scanning");
        search.error = None;
        search.attempted_pid = Some(driver.pid());
    }
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        let started = Instant::now();
        let result = fmlev_core::native_catalog::enumerate_process(&driver);
        let mut search = state.search_write();
        search.building = false;
        search.stage = None;
        match result {
            Ok(found) => {
                let elapsed = started.elapsed().as_millis() as u64;
                tracing::info!(
                    "Search index built for PID {}: {} entries, {} candidates, {} duplicates dropped, {} ms",
                    driver.pid(), found.entries.len(), found.candidates, found.duplicates_dropped, elapsed
                );
                search.index = Some(SearchIndex::new(driver.pid(), found.entries, found.duplicates_dropped, elapsed));
            }
            Err(error) => search.error = Some(error.to_string()),
        }
    });
    true
}

/// Automatic build: once per attached process; an index of another process is dropped.
pub(crate) fn ensure_index(state: &ServerState, driver: Option<LinuxMemoryDriver>) {
    let Some(driver) = driver else { return };
    check_stale(state, &driver);
    {
        let mut search = state.search_write();
        if search.index.as_ref().is_some_and(|index| index.pid != driver.pid()) {
            search.index = None;
        }
        if search.building || search.index.is_some() || search.attempted_pid == Some(driver.pid()) {
            return;
        }
    }
    start_index_build(state, driver);
}

const STALE_CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(10);
const STALE_SAMPLE: usize = 12;

/// Spot-checks a few entries per kind against the live process (at most every 10 s). A save loaded in
/// the same process leaves the old records behind, so any identity change schedules a rebuild.
pub(crate) fn check_stale(state: &ServerState, driver: &LinuxMemoryDriver) {
    let sample: Vec<IndexEntry> = {
        let mut search = state.search_write();
        if search.building || search.last_checked.is_some_and(|at| at.elapsed() < STALE_CHECK_EVERY) {
            return;
        }
        search.last_checked = Some(Instant::now());
        let Some(index) = search.index.as_ref().filter(|index| index.pid == driver.pid()) else { return };
        EntityKind::ALL
            .iter()
            .flat_map(|&kind| {
                let entries = index.entries(kind);
                let step = (entries.len() / STALE_SAMPLE).max(1);
                entries.iter().step_by(step).take(STALE_SAMPLE).cloned().collect::<Vec<_>>()
            })
            .collect()
    };
    if sample.iter().any(|entry| !fmlev_core::native_catalog::entry_still_valid(driver, entry)) {
        tracing::info!("Search index is stale (record identity changed); rebuilding");
        start_index_build(state, *driver);
    }
}

/// Drops an index entry whose record failed to open (moved or gone after a reload).
pub(crate) fn drop_stale(state: &ServerState, kind: EntityKind, id: i32) {
    if let Some(index) = state.search_write().index.as_mut() {
        index.remove(kind, id);
    }
}

/// Staff record from the index, revalidated by identity; a stale entry is dropped.
pub(crate) fn indexed_staff(state: &ServerState, driver: &LinuxMemoryDriver, id: i32, uid: i32) -> Option<NativeStaffRecord> {
    let entry = {
        let search = state.search_read();
        let index = search.index.as_ref().filter(|index| index.pid == driver.pid())?;
        index.get(EntityKind::Staff, id).filter(|entry| entry.uid as i32 == uid)?.clone()
    };
    let staff = NativeStaffRecord { pid: driver.pid(), record: entry.address, index: id, uid, name: entry.name };
    if staff.validate(driver).is_ok() {
        Some(staff)
    } else {
        drop_stale(state, EntityKind::Staff, id);
        None
    }
}

/// The uid the ready index holds for a staff id, if any (the caller still revalidates the record).
pub(crate) fn indexed_staff_uid(state: &ServerState, driver: &LinuxMemoryDriver, id: i32) -> Option<i32> {
    let search = state.search_read();
    let index = search.index.as_ref().filter(|index| index.pid == driver.pid())?;
    index.get(EntityKind::Staff, id).map(|entry| entry.uid as i32)
}

/// Staff search over the index (name or club), revalidating each hit. Returns the records and their
/// validated club names by index. `None` when no ready index exists.
pub(crate) fn indexed_staff_search(
    state: &ServerState,
    driver: &LinuxMemoryDriver,
    query: &str,
    max: usize,
) -> Option<(Vec<NativeStaffRecord>, std::collections::BTreeMap<i32, String>)> {
    let hits = {
        let search = state.search_read();
        let index = search.index.as_ref().filter(|index| index.pid == driver.pid())?;
        index.query(query, Some(EntityKind::Staff), max, 0).remove(&EntityKind::Staff).unwrap_or_default().hits
    };
    let clubs = hits.iter().filter_map(|entry| entry.detail.clone().map(|club| (entry.id, club))).collect();
    let staff = hits.into_iter().filter_map(|entry| indexed_staff(state, driver, entry.id, entry.uid as i32)).collect();
    Some((staff, clubs))
}

pub async fn search_status(State(state): State<ServerState>) -> impl IntoResponse {
    let driver = live_driver(&state.inner.read().await.context_mgr);
    ensure_index(&state, driver);
    Json(status_json(&state.search_read()))
}

pub async fn search_rebuild(State(state): State<ServerState>) -> impl IntoResponse {
    let Some(driver) = live_driver(&state.inner.read().await.context_mgr) else {
        return api_error(StatusCode::BAD_REQUEST, "Attach to FM26 with a save loaded first");
    };
    if start_index_build(&state, driver) {
        (StatusCode::ACCEPTED, Json(json!({ "status": "started" })))
    } else {
        api_error(StatusCode::CONFLICT, "The search index is already being built")
    }
}

fn kind_filter(kind: Option<&str>) -> Result<Option<EntityKind>, String> {
    match kind.map(str::trim) {
        None | Some("") | Some("all") => Ok(None),
        Some(kind) => EntityKind::from_plural(kind)
            .map(Some)
            .ok_or_else(|| format!("Unknown kind '{kind}'; use all, players, staff, clubs, nations or competitions")),
    }
}

fn page<T: Clone>(items: &[T], offset: usize, limit: usize) -> Vec<T> {
    items.iter().skip(offset).take(limit).cloned().collect()
}

/// Index hits shaped like the registry objects the UI already renders; registry data enriches a hit
/// with the same id (live registries are cleared on attach, so ids are native).
fn index_results(mgr: &GameContextManager, pages: &std::collections::BTreeMap<EntityKind, fmlev_core::search_index::KindPage>) -> serde_json::Map<String, Value> {
    let hits = |kind| pages.get(&kind).map_or(&[][..], |page| page.hits.as_slice());
    let mut out = serde_json::Map::new();
    out.insert("players".into(), hits(EntityKind::Player).iter().map(|e: &IndexEntry| match mgr.player_registry.get(&e.id) {
        Some(player) => {
            let mut value = to_value(player);
            if player.club_name.is_empty() {
                value["club_name"] = json!(e.detail.clone().unwrap_or_default());
            }
            value["uid"] = json!(e.uid);
            value["is_live"] = json!(true);
            value
        }
        None => json!({
            "id": e.id, "uid": e.uid, "name": e.name, "club_name": e.detail.clone().unwrap_or_default(),
            "nationality": "", "position": "", "current_ability": 0, "potential_ability": 0,
            "is_live": true, "source": "index",
        }),
    }).collect());
    out.insert("staff".into(), hits(EntityKind::Staff).iter().map(|e| json!({
        "index": e.id, "uid": e.uid, "name": e.name, "record": format!("{:#x}", e.address),
    })).collect());
    out.insert("clubs".into(), hits(EntityKind::Club).iter().map(|e| match mgr.club_registry.get(&e.id) {
        Some(club) => to_value(club),
        None => json!({ "club_id": e.id, "uid": e.uid, "club_name": e.name, "league_name": e.detail.clone().unwrap_or_default(), "reputation": 0 }),
    }).collect());
    out.insert("nations".into(), hits(EntityKind::Nation).iter().map(|e| match mgr.nation_registry.get(&e.id) {
        Some(nation) => to_value(nation),
        None => json!({ "id": e.id, "uid": e.uid, "name": e.name, "three_letter_code": "", "continent": "", "reputation": 0, "youth_rating": null }),
    }).collect());
    out.insert("competitions".into(), hits(EntityKind::Competition).iter().map(|e| match mgr.competition_registry.get(&e.id) {
        Some(comp) => to_value(comp),
        None => json!({ "id": e.id, "uid": e.uid, "name": e.name, "short_name": "", "nation_name": e.detail.clone().unwrap_or_default(), "reputation": 0 }),
    }).collect());
    out
}

fn to_value<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn registry_results(results: &SearchResults, kind: Option<EntityKind>, offset: usize, limit: usize) -> (serde_json::Map<String, Value>, serde_json::Map<String, Value>) {
    let wanted = |k: EntityKind| kind.is_none_or(|kind| kind == k);
    let mut out = serde_json::Map::new();
    let mut totals = serde_json::Map::new();
    macro_rules! section {
        ($kind:expr, $items:expr) => {{
            let items = if wanted($kind) { $items.as_slice() } else { &[] };
            totals.insert($kind.plural().into(), json!(items.len()));
            out.insert($kind.plural().into(), serde_json::to_value(page(items, offset, limit)).unwrap_or(Value::Null));
        }};
    }
    section!(EntityKind::Player, results.players);
    section!(EntityKind::Club, results.clubs);
    section!(EntityKind::Nation, results.nations);
    section!(EntityKind::Competition, results.competitions);
    (out, totals)
}

/// `GET /api/search?q=&kind=&limit=&offset=`. Never scans memory: the index is built in the background.
pub async fn search_entities(State(state): State<ServerState>, Query(query): Query<SearchQuery>) -> impl IntoResponse {
    let kind = match kind_filter(query.kind.as_deref()) {
        Ok(kind) => kind,
        Err(error) => return api_error(StatusCode::BAD_REQUEST, error).into_response(),
    };
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = query.offset.unwrap_or(0);
    let r = state.inner.read().await;
    let mgr = &r.context_mgr;
    let driver = live_driver(mgr);
    ensure_index(&state, driver);
    let search = state.search_read();
    let status = status_json(&search);
    let ready = search.index.as_ref().filter(|index| driver.is_some_and(|driver| driver.pid() == index.pid));

    let (mut body, totals, source) = match ready {
        Some(index) => {
            let pages = index.query(&query.q, kind, limit, offset);
            let totals: serde_json::Map<String, Value> =
                EntityKind::ALL.iter().map(|k| (k.plural().to_string(), json!(pages.get(k).map_or(0, |p| p.total)))).collect();
            (index_results(mgr, &pages), totals, "index")
        }
        None => {
            let (body, totals) = registry_results(&mgr.search(&query.q), kind, offset, limit);
            (body, totals, "registry")
        }
    };
    let first = |key: &str| body.get(key).and_then(|v| v.as_array()).and_then(|a| a.first()).cloned().unwrap_or(Value::Null);
    let (nation, competition) = (first("nations"), first("competitions"));
    body.insert("query".into(), json!(query.q));
    body.insert("total_players".into(), totals.get("players").cloned().unwrap_or(json!(0)));
    body.insert("totals".into(), Value::Object(totals));
    body.insert("source".into(), json!(source));
    body.insert("index".into(), status);
    body.insert("discovery_error".into(), Value::Null);
    body.insert("nation".into(), nation);
    body.insert("competition".into(), competition);
    Json(Value::Object(body)).into_response()
}

/// Lets a nation or competition found only by the index be selected (registry entry on demand).
pub(crate) fn prepare_selection(state: &ServerState, mgr: &mut GameContextManager, nation_id: Option<i32>, competition_id: Option<i32>) {
    let search = state.search_read();
    let Some(index) = search.index.as_ref() else { return };
    if let Some(entry) = nation_id.and_then(|id| index.get(EntityKind::Nation, id)) {
        mgr.ensure_live_nation(entry.id, &entry.name);
    }
    if let Some(entry) = competition_id.and_then(|id| index.get(EntityKind::Competition, id)) {
        mgr.ensure_live_competition(entry.id, &entry.name, entry.detail.as_deref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_filter_accepts_known_kinds_only() {
        assert_eq!(kind_filter(None), Ok(None));
        assert_eq!(kind_filter(Some("all")), Ok(None));
        assert_eq!(kind_filter(Some("staff")), Ok(Some(EntityKind::Staff)));
        assert!(kind_filter(Some("teams")).is_err());
    }

    #[test]
    fn status_reports_each_state() {
        let mut search = SearchState::default();
        assert_eq!(status_json(&search)["state"], "idle");
        search.building = true;
        search.stage = Some("scanning");
        assert_eq!(status_json(&search)["state"], "building");
        assert_eq!(status_json(&search)["stage"], "scanning");
        search.building = false;
        search.error = Some("game_plugin.dll is not mapped".into());
        assert_eq!(status_json(&search)["state"], "error");
        search.error = None;
        search.index = Some(SearchIndex::new(
            7,
            vec![IndexEntry::new(EntityKind::Nation, 1, 11, "Spain".into(), None, 0x1000)],
            3,
            1200,
        ));
        let status = status_json(&search);
        assert_eq!(status["state"], "ready");
        assert_eq!(status["counts"]["nations"], 1);
        assert_eq!(status["counts"]["players"], 0);
        assert_eq!(status["pid"], 7);
        assert_eq!(status["duplicates_dropped"], 3);
        assert_eq!(status["duration_ms"], 1200);
    }
}

/// A record to locate before a route takes the global state lock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Warm {
    Player(i32),
    Club(i32),
    Nation(i32),
    Competition(i32),
    CompetitionRecord(i32, i32),
}

/// Locates native records on a worker thread without holding the global lock, so the in-lock call that
/// follows finds them in the locator cache (a validated identity check) instead of scanning all of game
/// memory under the lock. Failures are ignored: the locked call reports its own error.
pub(crate) async fn warm(state: &ServerState, targets: &[Warm]) {
    use fmlev_core::native_club::NativeClubRecord;
    use fmlev_core::native_competition::NativeCompetitionRecord;
    use fmlev_core::native_nation::NativeNationRecord;
    use fmlev_core::native_player::NativePlayerRecord;
    if targets.is_empty() {
        return;
    }
    let Some(driver) = ({
        let r = state.inner.read().await;
        r.context_mgr.session.as_ref().map(|session| session.driver)
    }) else {
        return;
    };
    let targets = targets.to_vec();
    let _ = tokio::task::spawn_blocking(move || {
        for target in targets {
            match target {
                Warm::Player(id) => drop(NativePlayerRecord::discover_by_index(&driver, id)),
                Warm::Club(id) => drop(NativeClubRecord::discover(&driver, id)),
                Warm::Nation(id) => drop(NativeNationRecord::discover(&driver, id)),
                Warm::Competition(id) => drop(NativeCompetitionRecord::discover(&driver, id)),
                Warm::CompetitionRecord(index, uid) => drop(fmlev_core::native_registration::competition_record(&driver, index, uid)),
            }
        }
    })
    .await;
}
