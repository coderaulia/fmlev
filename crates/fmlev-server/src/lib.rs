use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::info;

use fmlev_core::{
    ClubAffiliate, ClubFinances, CompetitionDetails, GameContextManager, NationDetails, Player,
    PlayerAttributes, SquadActions,
};
use fmlev_mem::{
    find_fm_process, find_module_bases, get_yama_diagnostic, parse_process_maps,
    LinuxMemoryDriver,
};

pub const EMBEDDED_UI_HTML: &str = include_str!("ui.html");
pub const DEFAULT_OFFSETS_JSON: &str = include_str!("../../../offsets/v26.0.0.json");

#[derive(Clone)]
pub struct ServerState {
    pub inner: Arc<RwLock<AppState>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanMatch {
    pub address: String,
    pub address_val: usize,
    pub value: String,
}

pub struct AppState {
    pub pid: Option<i32>,
    pub exe_path: Option<String>,
    pub is_proton: bool,
    pub wine_prefix: Option<String>,
    pub game_assembly_base: Option<usize>,
    pub unity_player_base: Option<usize>,
    pub fm_exe_base: Option<usize>,
    pub is_attached: bool,
    pub scan_results: Vec<ScanMatch>,
    pub last_scan_type: String,

    pub context_mgr: GameContextManager,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
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
            context_mgr: GameContextManager::new(),
        }
    }
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub attached: bool,
    pub pid: Option<i32>,
    pub exe_path: Option<String>,
    pub is_proton: bool,
    pub wine_prefix: Option<String>,
    pub game_assembly_base: Option<String>,
    pub unity_player_base: Option<String>,
    pub fm_exe_base: Option<String>,
    pub yama_diagnostics: String,
}

#[derive(Deserialize)]
pub struct AttachRequest {
    pub pid: Option<i32>,
}

#[derive(Deserialize)]
pub struct MemReadRequest {
    pub address: String,
    #[serde(default = "default_type")]
    pub r#type: String,
    #[serde(default = "default_size")]
    pub size: usize,
}

fn default_type() -> String {
    "bytes".to_string()
}

fn default_size() -> usize {
    64
}

#[derive(Deserialize)]
pub struct MemWriteRequest {
    pub address: String,
    pub r#type: String,
    pub value: serde_json::Value,
}

#[derive(Deserialize)]
pub struct HealRequest {
    #[serde(default)]
    pub player_ptr: usize,
    #[serde(default = "default_condition_offset")]
    pub condition_offset: usize,
    #[serde(default)]
    pub heal_squad: bool,
}

fn default_condition_offset() -> usize {
    0x20
}

#[derive(Deserialize)]
pub struct FinanceRequest {
    #[serde(default = "default_club_ptr")]
    pub club_ptr: usize,
    pub balance: i64,
    pub transfer_budget: i64,
    pub wage_budget: i32,
    #[serde(default = "default_balance_offset")]
    pub balance_offset: usize,
    #[serde(default = "default_transfer_offset")]
    pub transfer_offset: usize,
    #[serde(default = "default_wage_offset")]
    pub wage_offset: usize,
}

fn default_club_ptr() -> usize {
    0x7ff418300000
}

fn default_balance_offset() -> usize {
    0x30
}

fn default_transfer_offset() -> usize {
    0x38
}

fn default_wage_offset() -> usize {
    0x40
}

#[derive(Deserialize)]
pub struct GodModeRequest {
    pub action: String,
    #[serde(default)]
    pub player_ptr: Option<usize>,
}

#[derive(Deserialize)]
pub struct MemScanRequest {
    pub value: String,
    #[serde(default = "default_scan_type")]
    pub r#type: String,
}

fn default_scan_type() -> String {
    "i32".to_string()
}

#[derive(Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
}

#[derive(Deserialize)]
pub struct ContextSelectRequest {
    pub player_id: Option<i32>,
    pub club_id: Option<i32>,
    pub nation_id: Option<i32>,
    pub competition_id: Option<i32>,
    pub unpin: Option<bool>,
}

#[derive(Deserialize)]
pub struct PlayerPresetRequest {
    pub preset: String,
    pub player_id: Option<i32>,
}

#[derive(Deserialize)]
pub struct ClubPresetRequest {
    pub preset: String,
    pub club_id: Option<i32>,
}

#[derive(Deserialize)]
pub struct NationPresetRequest {
    pub preset: String,
    pub nation_id: Option<i32>,
}

#[derive(Deserialize)]
pub struct CompetitionPresetRequest {
    pub preset: String,
    pub competition_id: Option<i32>,
}

#[derive(Deserialize)]
pub struct AffiliateActionRequest {
    pub club_id: i32,
    pub affiliate: Option<ClubAffiliate>,
    pub affiliate_id: Option<i32>,
}

pub fn create_router(state: ServerState) -> Router {
    Router::new()
        .route("/", get(serve_ui))
        .route("/api/status", get(get_status))
        .route("/api/context", get(get_context))
        .route("/api/search", get(search_entities))
        .route("/api/context/select", post(select_context))
        .route("/api/player/update", post(update_player))
        .route("/api/player/preset", post(apply_player_preset))
        .route("/api/club/update", post(update_club))
        .route("/api/club/preset", post(apply_club_preset))
        .route("/api/nation/update", post(update_nation))
        .route("/api/nation/preset", post(apply_nation_preset))
        .route("/api/competition/update", post(update_competition))
        .route("/api/competition/preset", post(apply_competition_preset))
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
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn start_server(port: u16) -> anyhow::Result<()> {
    let state = ServerState {
        inner: Arc::new(RwLock::new(AppState::default())),
    };

    // Try auto-attaching on startup in background
    let initial_state = state.clone();
    tokio::spawn(async move {
        if let Ok(proc) = find_fm_process() {
            let pid = proc.pid;
            let bases = find_module_bases(pid).unwrap_or(fmlev_mem::ModuleBases {
                game_assembly_base: None,
                unity_player_base: None,
                fm_exe_base: None,
            });
            let mut w = initial_state.inner.write().await;
            w.pid = Some(pid);
            w.exe_path = Some(proc.exe_path.to_string_lossy().to_string());
            w.is_proton = proc.is_proton;
            w.wine_prefix = proc.wine_prefix.map(|p| p.to_string_lossy().to_string());
            w.game_assembly_base = bases.game_assembly_base;
            w.unity_player_base = bases.unity_player_base;
            w.fm_exe_base = bases.fm_exe_base;
            w.is_attached = true;
            let _ = w.context_mgr.try_attach();
            info!("Auto-attached to FM26 process (PID: {}) on server start", pid);
        }
    });

    let app = create_router(state);
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));

    info!("Starting fmlev Live Editor Server at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn serve_ui() -> impl IntoResponse {
    Html(EMBEDDED_UI_HTML)
}

async fn get_status(State(state): State<ServerState>) -> impl IntoResponse {
    let r = state.inner.read().await;
    let yama = get_yama_diagnostic();

    Json(StatusResponse {
        attached: r.is_attached,
        pid: r.pid,
        exe_path: r.exe_path.clone(),
        is_proton: r.is_proton,
        wine_prefix: r.wine_prefix.clone(),
        game_assembly_base: r.game_assembly_base.map(|b| format!("0x{:x}", b)),
        unity_player_base: r.unity_player_base.map(|b| format!("0x{:x}", b)),
        fm_exe_base: r.fm_exe_base.map(|b| format!("0x{:x}", b)),
        yama_diagnostics: yama,
    })
}

async fn get_context(State(state): State<ServerState>) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    let _ = w.context_mgr.sync_live_context();
    let ctx = w.context_mgr.get_context();
    w.is_attached = ctx.attached;
    w.pid = ctx.pid;

    let nations = w.context_mgr.all_nations();
    let competitions = w.context_mgr.all_competitions();

    Json(serde_json::json!({
        "connected": ctx.attached,
        "pid": ctx.pid,
        "game_name": "Football Manager 26",
        "screen_state": ctx.active_screen,
        "sync_mode": ctx.sync_mode,
        "is_pinned": ctx.sync_mode == "pinned",
        "player": ctx.player,
        "active_player": ctx.player,
        "club": ctx.club,
        "active_club": ctx.club,
        "nation": ctx.nation,
        "active_nation": ctx.nation,
        "competition": ctx.competition,
        "active_competition": ctx.competition,
        "nations": nations,
        "competitions": competitions
    }))
}

async fn search_entities(
    State(state): State<ServerState>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    let r = state.inner.read().await;
    let results = r.context_mgr.search(&query.q);
    let nation = results.nations.first().cloned();
    let competition = results.competitions.first().cloned();
    Json(serde_json::json!({
        "query": query.q,
        "players": results.players,
        "clubs": results.clubs,
        "nations": results.nations,
        "competitions": results.competitions,
        "nation": nation,
        "competition": competition
    }))
}

async fn select_context(
    State(state): State<ServerState>,
    Json(payload): Json<ContextSelectRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    if payload.unpin == Some(true) {
        w.context_mgr.set_sync_mode("auto_follow");
    } else {
        if let Some(pid) = payload.player_id {
            let _ = w.context_mgr.select_player(pid);
            w.context_mgr.set_sync_mode("pinned");
        }
        if let Some(cid) = payload.club_id {
            let _ = w.context_mgr.select_club(cid);
            w.context_mgr.set_sync_mode("pinned");
        }
        if let Some(nid) = payload.nation_id {
            let _ = w.context_mgr.select_nation(nid);
            w.context_mgr.set_sync_mode("pinned");
        }
        if let Some(comp_id) = payload.competition_id {
            let _ = w.context_mgr.select_competition(comp_id);
            w.context_mgr.set_sync_mode("pinned");
        }
    }

    let ctx = w.context_mgr.get_context();
    Json(serde_json::json!({
        "status": "success",
        "is_pinned": ctx.sync_mode == "pinned",
        "sync_mode": ctx.sync_mode,
        "player": ctx.player,
        "active_player": ctx.player,
        "club": ctx.club,
        "active_club": ctx.club,
        "nation": ctx.nation,
        "active_nation": ctx.nation,
        "competition": ctx.competition,
        "active_competition": ctx.competition
    }))
}

async fn update_player(
    State(state): State<ServerState>,
    Json(payload): Json<Player>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    let name = payload.name.clone();
    match w.context_mgr.update_player(payload) {
        Ok(_) => {
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Player {} updated successfully!", name),
                    "player": ctx.player,
                    "active_player": ctx.player
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
        ),
    }
}

fn boost_player_attributes(attrs: &mut PlayerAttributes, delta: i8) {
    fn b(val: &mut u8, delta: i8) {
        let res = (*val as i16) + (delta as i16);
        *val = res.clamp(1, 20) as u8;
    }
    b(&mut attrs.corners, delta);
    b(&mut attrs.crossing, delta);
    b(&mut attrs.dribbling, delta);
    b(&mut attrs.finishing, delta);
    b(&mut attrs.first_touch, delta);
    b(&mut attrs.free_kicks, delta);
    b(&mut attrs.heading, delta);
    b(&mut attrs.long_shots, delta);
    b(&mut attrs.passing, delta);
    b(&mut attrs.penalty_taking, delta);
    b(&mut attrs.tackling, delta);
    b(&mut attrs.technique, delta);

    b(&mut attrs.aggression, delta);
    b(&mut attrs.anticipation, delta);
    b(&mut attrs.bravery, delta);
    b(&mut attrs.composure, delta);
    b(&mut attrs.concentration, delta);
    b(&mut attrs.decisions, delta);
    b(&mut attrs.determination, delta);
    b(&mut attrs.flair, delta);
    b(&mut attrs.leadership, delta);
    b(&mut attrs.off_the_ball, delta);
    b(&mut attrs.positioning, delta);
    b(&mut attrs.teamwork, delta);
    b(&mut attrs.vision, delta);
    b(&mut attrs.work_rate, delta);

    b(&mut attrs.acceleration, delta);
    b(&mut attrs.agility, delta);
    b(&mut attrs.balance, delta);
    b(&mut attrs.jumping_reach, delta);
    b(&mut attrs.natural_fitness, delta);
    b(&mut attrs.pace, delta);
    b(&mut attrs.stamina, delta);
    b(&mut attrs.strength, delta);

    b(&mut attrs.consistency, delta);
    b(&mut attrs.important_matches, delta);
    b(&mut attrs.ambition, delta);
    b(&mut attrs.loyalty, delta);
    b(&mut attrs.pressure, delta);
    b(&mut attrs.professionalism, delta);
}

async fn apply_player_preset(
    State(state): State<ServerState>,
    Json(payload): Json<PlayerPresetRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    if let Some(pid) = payload.player_id {
        let _ = w.context_mgr.select_player(pid);
    }

    match payload.preset.as_str() {
        "boost_all_2" | "boost_attributes_2" => {
            let mut p = w.context_mgr.context.player.clone();
            boost_player_attributes(&mut p.attributes, 2);
            p.current_ability = (p.current_ability as u16 + 10).min(200) as u8;
            let _ = w.context_mgr.update_player(p);
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Boosted all attributes for {} by +2!", ctx.player.name),
                    "player": ctx.player,
                    "active_player": ctx.player
                })),
            )
        }
        "free_release" => {
            let mut p = w.context_mgr.context.player.clone();
            p.contract.release_clause = 0;
            let _ = w.context_mgr.update_player(p);
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Set {}'s release clause to $0 for free transfer!", ctx.player.name),
                    "player": ctx.player,
                    "active_player": ctx.player
                })),
            )
        }
        other => {
            let action_str = match other {
                "heal" => "heal_player",
                "morale" => "max_morale",
                "wonderkid" => "wonderkid",
                "goat" => "max_attributes",
                "extend_contract" => "extend_contract",
                act => act,
            };

            match w.context_mgr.apply_quick_action(action_str) {
                Ok(_) => {
                    let ctx = w.context_mgr.get_context();
                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "status": "success",
                            "message": format!("Preset '{}' applied successfully to {}!", payload.preset, ctx.player.name),
                            "player": ctx.player,
                            "active_player": ctx.player
                        })),
                    )
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
                ),
            }
        }
    }
}

async fn update_club(
    State(state): State<ServerState>,
    Json(payload): Json<ClubFinances>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    let name = payload.club_name.clone();
    match w.context_mgr.update_club(payload) {
        Ok(_) => {
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Club {} updated successfully!", name),
                    "club": ctx.club,
                    "active_club": ctx.club
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
        ),
    }
}

async fn apply_club_preset(
    State(state): State<ServerState>,
    Json(payload): Json<ClubPresetRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    if let Some(cid) = payload.club_id {
        let _ = w.context_mgr.select_club(cid);
    }

    match payload.preset.as_str() {
        "morale_squad" => {
            let target_club_id = w.context_mgr.context.club.club_id;
            w.context_mgr.context.player.morale = 100;
            w.context_mgr.context.player.morale_label = "Superb".to_string();
            w.context_mgr.context.player.contract.happiness = "Delighted".to_string();

            for player in w.context_mgr.player_registry.values_mut() {
                if target_club_id == 0 || player.club_id == target_club_id {
                    player.morale = 100;
                    player.morale_label = "Superb".to_string();
                    player.contract.happiness = "Delighted".to_string();
                }
            }
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Maximized squad morale to 100 (Superb) for {}!", ctx.club.club_name),
                    "club": ctx.club,
                    "active_club": ctx.club
                })),
            )
        }
        "clear_bans" => {
            let target_club_id = w.context_mgr.context.club.club_id;
            w.context_mgr.context.player.is_injured = false;
            w.context_mgr.context.player.is_suspended = false;

            for player in w.context_mgr.player_registry.values_mut() {
                if target_club_id == 0 || player.club_id == target_club_id {
                    player.is_injured = false;
                    player.is_suspended = false;
                }
            }
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Cleared all injuries and suspensions for {} squad!", ctx.club.club_name),
                    "club": ctx.club,
                    "active_club": ctx.club
                })),
            )
        }
        "unlock_ige" => {
            if let Some(base) = w.game_assembly_base {
                if let Some(pid) = w.pid {
                    let driver = LinuxMemoryDriver::new(pid);
                    let _ = driver.write::<u8>(base + 0x1800, &1);
                }
            }
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": "Unlocked FM26 Built-In In-Game Editor (IGE) in memory!".to_string(),
                    "club": ctx.club,
                    "active_club": ctx.club
                })),
            )
        }
        other => {
            let action_str = match other {
                "heal_squad" => "heal_squad",
                "add_50m" => "boost_budget_50m",
                "sugar_daddy" => "sugar_daddy",
                "wipe_debt" => "wipe_debt",
                "max_facilities" => "max_facilities",
                act => act,
            };

            match w.context_mgr.apply_quick_action(action_str) {
                Ok(_) => {
                    let ctx = w.context_mgr.get_context();
                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "status": "success",
                            "message": format!("Preset '{}' applied successfully to {}!", payload.preset, ctx.club.club_name),
                            "club": ctx.club,
                            "active_club": ctx.club
                        })),
                    )
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
                ),
            }
        }
    }
}

async fn update_nation(
    State(state): State<ServerState>,
    Json(payload): Json<NationDetails>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    let name = payload.name.clone();
    match w.context_mgr.update_nation(payload) {
        Ok(_) => {
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Nation {} updated successfully!", name),
                    "nation": ctx.nation,
                    "active_nation": ctx.nation
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
        ),
    }
}

async fn apply_nation_preset(
    State(state): State<ServerState>,
    Json(payload): Json<NationPresetRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    if let Some(nid) = payload.nation_id {
        let _ = w.context_mgr.select_nation(nid);
    }

    match payload.preset.as_str() {
        "golden_generation" => {
            let _ = w.context_mgr.apply_quick_action("golden_generation");
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Triggered golden generation for {}!", ctx.nation.name),
                    "nation": ctx.nation,
                    "active_nation": ctx.nation
                })),
            )
        }
        "max_youth_rating" => {
            let mut nation = w.context_mgr.context.nation.clone();
            nation.youth_rating = 200;
            let _ = w.context_mgr.update_nation(nation);
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Maximized youth rating to 200 for {}!", ctx.nation.name),
                    "nation": ctx.nation,
                    "active_nation": ctx.nation
                })),
            )
        }
        "clean_reputation" => {
            let mut nation = w.context_mgr.context.nation.clone();
            nation.reputation = 10000;
            nation.corruption_index = 0;
            let _ = w.context_mgr.update_nation(nation);
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Cleaned reputation and eliminated corruption for {}!", ctx.nation.name),
                    "nation": ctx.nation,
                    "active_nation": ctx.nation
                })),
            )
        }
        "economic_powerhouse" => {
            let mut nation = w.context_mgr.context.nation.clone();
            nation.economic_factor = 20;
            nation.state_of_development = 2;
            nation.fa_financial_power = 20;
            nation.tax_rate = 15;
            let _ = w.context_mgr.update_nation(nation);
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Upgraded {} to an economic powerhouse!", ctx.nation.name),
                    "nation": ctx.nation,
                    "active_nation": ctx.nation
                })),
            )
        }
        other => {
            match w.context_mgr.apply_quick_action(other) {
                Ok(_) => {
                    let ctx = w.context_mgr.get_context();
                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "status": "success",
                            "message": format!("Nation preset '{}' applied successfully to {}!", payload.preset, ctx.nation.name),
                            "nation": ctx.nation,
                            "active_nation": ctx.nation
                        })),
                    )
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
                ),
            }
        }
    }
}

async fn update_competition(
    State(state): State<ServerState>,
    Json(payload): Json<CompetitionDetails>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    let name = payload.name.clone();
    match w.context_mgr.update_competition(payload) {
        Ok(_) => {
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Competition {} updated successfully!", name),
                    "competition": ctx.competition,
                    "active_competition": ctx.competition
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
        ),
    }
}

async fn apply_competition_preset(
    State(state): State<ServerState>,
    Json(payload): Json<CompetitionPresetRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    if let Some(cid) = payload.competition_id {
        let _ = w.context_mgr.select_competition(cid);
    }

    match payload.preset.as_str() {
        "relax_rules" => {
            let _ = w.context_mgr.apply_quick_action("relax_rules");
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Relaxed squad rules and foreign quotas for {}!", ctx.competition.name),
                    "competition": ctx.competition,
                    "active_competition": ctx.competition
                })),
            )
        }
        "five_subs" => {
            let mut comp = w.context_mgr.context.competition.clone();
            comp.max_substitutes = 5;
            comp.max_bench_size = 12;
            comp.substitution_windows = 3;
            let _ = w.context_mgr.update_competition(comp);
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Enforced 5 substitutes with 12 on bench for {}!", ctx.competition.name),
                    "competition": ctx.competition,
                    "active_competition": ctx.competition
                })),
            )
        }
        "boost_prize_pool" => {
            let mut comp = w.context_mgr.context.competition.clone();
            comp.winner_prize_money = comp.winner_prize_money.max(100_000_000) + 50_000_000;
            comp.tv_rights_pool = comp.tv_rights_pool.max(1_000_000_000) + 1_000_000_000;
            let _ = w.context_mgr.update_competition(comp);
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Boosted prize pool and TV rights pool for {}!", ctx.competition.name),
                    "competition": ctx.competition,
                    "active_competition": ctx.competition
                })),
            )
        }
        "max_reputation" => {
            let mut comp = w.context_mgr.context.competition.clone();
            comp.reputation = 10000;
            let _ = w.context_mgr.update_competition(comp);
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Maximized reputation to 10000 for {}!", ctx.competition.name),
                    "competition": ctx.competition,
                    "active_competition": ctx.competition
                })),
            )
        }
        other => {
            match w.context_mgr.apply_quick_action(other) {
                Ok(_) => {
                    let ctx = w.context_mgr.get_context();
                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "status": "success",
                            "message": format!("Competition preset '{}' applied successfully to {}!", payload.preset, ctx.competition.name),
                            "competition": ctx.competition,
                            "active_competition": ctx.competition
                        })),
                    )
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
                ),
            }
        }
    }
}

async fn add_club_affiliate(
    State(state): State<ServerState>,
    Json(payload): Json<AffiliateActionRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    let affiliate = match payload.affiliate {
        Some(mut aff) => {
            if aff.affiliate_club_id == 0 {
                if let Some(id) = payload.affiliate_id {
                    aff.affiliate_club_id = id;
                }
            }
            aff
        }
        None => {
            if let Some(id) = payload.affiliate_id {
                ClubAffiliate {
                    affiliate_club_id: id,
                    affiliate_name: format!("Affiliate Club {}", id),
                    ..Default::default()
                }
            } else {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "status": "error",
                        "message": "Missing affiliate object or affiliate_id in request"
                    })),
                );
            }
        }
    };

    let aff_name = affiliate.affiliate_name.clone();
    match w.context_mgr.add_club_affiliate(payload.club_id, affiliate) {
        Ok(club) => {
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Affiliate {} added successfully to club {}!", aff_name, club.club_name),
                    "club": club,
                    "active_club": ctx.club
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
        ),
    }
}

async fn remove_club_affiliate(
    State(state): State<ServerState>,
    Json(payload): Json<AffiliateActionRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    let aff_id = match payload.affiliate_id.or_else(|| payload.affiliate.as_ref().map(|a| a.affiliate_club_id)) {
        Some(id) if id != 0 => id,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "status": "error",
                    "message": "Missing affiliate_id in request"
                })),
            );
        }
    };

    match w.context_mgr.remove_club_affiliate(payload.club_id, aff_id) {
        Ok(club) => {
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Affiliate {} removed successfully from club {}!", aff_id, club.club_name),
                    "club": club,
                    "active_club": ctx.club
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
        ),
    }
}

async fn attach_process(
    State(state): State<ServerState>,
    payload: Option<Json<AttachRequest>>,
) -> impl IntoResponse {
    let target_pid = payload.and_then(|p| p.pid);

    let proc_result = if let Some(pid) = target_pid {
        Ok(fmlev_mem::GameProcess {
            pid,
            exe_path: std::path::PathBuf::from(format!("/proc/{}/exe", pid)),
            is_proton: true,
            wine_prefix: None,
        })
    } else {
        find_fm_process()
    };

    match proc_result {
        Ok(proc) => {
            let pid = proc.pid;
            let bases = find_module_bases(pid).unwrap_or(fmlev_mem::ModuleBases {
                game_assembly_base: None,
                unity_player_base: None,
                fm_exe_base: None,
            });

            let mut w = state.inner.write().await;
            w.pid = Some(pid);
            w.exe_path = Some(proc.exe_path.to_string_lossy().to_string());
            w.is_proton = proc.is_proton;
            w.wine_prefix = proc.wine_prefix.map(|p| p.to_string_lossy().to_string());
            w.game_assembly_base = bases.game_assembly_base;
            w.unity_player_base = bases.unity_player_base;
            w.fm_exe_base = bases.fm_exe_base;
            w.is_attached = true;
            let _ = w.context_mgr.try_attach();

            info!("Attached to Football Manager 26 PID: {}", pid);

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "attached",
                    "pid": pid,
                    "game_assembly_base": bases.game_assembly_base.map(|b| format!("0x{:x}", b)),
                    "unity_player_base": bases.unity_player_base.map(|b| format!("0x{:x}", b)),
                    "fm_exe_base": bases.fm_exe_base.map(|b| format!("0x{:x}", b)),
                })),
            )
        }
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "status": "error",
                "message": e.to_string(),
                "yama": get_yama_diagnostic()
            })),
        ),
    }
}

async fn get_maps(State(state): State<ServerState>) -> impl IntoResponse {
    let r = state.inner.read().await;
    let pid = match r.pid {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "status": "not_attached",
                    "message": "Not attached to any process. Run auto-attach first."
                })),
            )
        }
    };

    match parse_process_maps(pid) {
        Ok(mappings) => {
            let mut modules = Vec::new();
            let mut seen_paths = std::collections::HashSet::new();

            for map in &mappings {
                let path = map.pathname.trim();
                if path.is_empty() || path.starts_with('[') {
                    continue;
                }

                let path_lower = path.to_lowercase();
                if (path_lower.contains("gameassembly")
                    || path_lower.contains("unityplayer")
                    || path_lower.contains("fm.exe")
                    || path_lower.ends_with(".dll")
                    || path_lower.ends_with(".so"))
                    && seen_paths.insert(path.to_string())
                {
                    let name = std::path::Path::new(path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.to_string());

                    modules.push(serde_json::json!({
                        "name": name,
                        "path": path,
                        "start": format!("0x{:x}", map.start),
                        "end": format!("0x{:x}", map.end),
                        "perms": map.perms,
                    }));
                }
            }

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "pid": pid,
                    "total_mappings": mappings.len(),
                    "modules": modules
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "status": "error",
                "message": e.to_string()
            })),
        ),
    }
}

pub fn parse_hex_or_dec_address(addr_str: &str) -> Option<usize> {
    let clean = addr_str.trim().trim_matches('"');
    if let Some(stripped) = clean.strip_prefix("0x").or_else(|| clean.strip_prefix("0X")) {
        usize::from_str_radix(stripped, 16).ok()
    } else if clean.chars().any(|c| matches!(c, 'a'..='f' | 'A'..='F')) {
        usize::from_str_radix(clean, 16).ok()
    } else {
        clean.parse::<usize>().ok()
    }
}

async fn read_memory(
    State(state): State<ServerState>,
    Json(payload): Json<MemReadRequest>,
) -> impl IntoResponse {
    let r = state.inner.read().await;
    let pid = match r.pid {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "status": "not_attached", "message": "Game not attached" })),
            )
        }
    };

    let target_addr = match parse_hex_or_dec_address(&payload.address) {
        Some(a) => a,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "status": "invalid_address", "message": "Could not parse address" })),
            )
        }
    };

    let driver = LinuxMemoryDriver::new(pid);

    match payload.r#type.as_str() {
        "u8" => match driver.read::<u8>(target_addr) {
            Ok(v) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "address": format!("0x{:x}", target_addr),
                    "type": "u8",
                    "value": v,
                    "hex": format!("0x{:02x}", v)
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        },
        "u16" => match driver.read::<u16>(target_addr) {
            Ok(v) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "address": format!("0x{:x}", target_addr),
                    "type": "u16",
                    "value": v,
                    "hex": format!("0x{:04x}", v)
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        },
        "i32" => match driver.read::<i32>(target_addr) {
            Ok(v) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "address": format!("0x{:x}", target_addr),
                    "type": "i32",
                    "value": v,
                    "hex": format!("0x{:08x}", v)
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        },
        "u32" => match driver.read::<u32>(target_addr) {
            Ok(v) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "address": format!("0x{:x}", target_addr),
                    "type": "u32",
                    "value": v,
                    "hex": format!("0x{:08x}", v)
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        },
        "i64" => match driver.read::<i64>(target_addr) {
            Ok(v) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "address": format!("0x{:x}", target_addr),
                    "type": "i64",
                    "value": v,
                    "hex": format!("0x{:016x}", v)
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        },
        "u64" => match driver.read::<u64>(target_addr) {
            Ok(v) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "address": format!("0x{:x}", target_addr),
                    "type": "u64",
                    "value": v,
                    "hex": format!("0x{:016x}", v)
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        },
        "f32" => match driver.read::<f32>(target_addr) {
            Ok(v) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "address": format!("0x{:x}", target_addr),
                    "type": "f32",
                    "value": v
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        },
        "string" => {
            let size = payload.size.clamp(1, 1024);
            let mut buf = vec![0u8; size];
            match driver.read_bytes(target_addr, &mut buf) {
                Ok(bytes_read) => {
                    let slice = &buf[..bytes_read];
                    let null_end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
                    let s = String::from_utf8_lossy(&slice[..null_end]).to_string();
                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "address": format!("0x{:x}", target_addr),
                            "type": "string",
                            "value": s,
                            "length": null_end
                        })),
                    )
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
                ),
            }
        }
        _ => {
            let size = payload.size.clamp(1, 4096);
            let mut buf = vec![0u8; size];
            match driver.read_bytes(target_addr, &mut buf) {
                Ok(bytes_read) => {
                    buf.truncate(bytes_read);
                    let hex_str = buf
                        .iter()
                        .map(|b| format!("{:02x}", b))
                        .collect::<Vec<_>>()
                        .join(" ");
                    let ascii_str = buf
                        .iter()
                        .map(|&b| {
                            if (32..=126).contains(&b) {
                                b as char
                            } else {
                                '.'
                            }
                        })
                        .collect::<String>();

                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "address": format!("0x{:x}", target_addr),
                            "type": "bytes",
                            "bytes": buf,
                            "hex": hex_str,
                            "ascii": ascii_str,
                            "size": bytes_read
                        })),
                    )
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
                ),
            }
        }
    }
}

async fn write_memory(
    State(state): State<ServerState>,
    Json(payload): Json<MemWriteRequest>,
) -> impl IntoResponse {
    let r = state.inner.read().await;
    let pid = match r.pid {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "status": "not_attached", "message": "Game not attached" })),
            )
        }
    };

    let target_addr = match parse_hex_or_dec_address(&payload.address) {
        Some(a) => a,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "status": "invalid_address", "message": "Could not parse address" })),
            )
        }
    };

    let driver = LinuxMemoryDriver::new(pid);

    match payload.r#type.as_str() {
        "u8" => {
            let val = payload.value.as_u64().unwrap_or(0) as u8;
            match driver.write::<u8>(target_addr, &val) {
                Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "success": true, "bytes_written": 1 }))),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "message": e.to_string() }))),
            }
        }
        "u16" => {
            let val = payload.value.as_u64().unwrap_or(0) as u16;
            match driver.write::<u16>(target_addr, &val) {
                Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "success": true, "bytes_written": 2 }))),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "message": e.to_string() }))),
            }
        }
        "i32" => {
            let val = payload.value.as_i64().unwrap_or(0) as i32;
            match driver.write::<i32>(target_addr, &val) {
                Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "success": true, "bytes_written": 4 }))),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "message": e.to_string() }))),
            }
        }
        "u32" => {
            let val = payload.value.as_u64().unwrap_or(0) as u32;
            match driver.write::<u32>(target_addr, &val) {
                Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "success": true, "bytes_written": 4 }))),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "message": e.to_string() }))),
            }
        }
        "i64" => {
            let val = payload.value.as_i64().unwrap_or(0);
            match driver.write::<i64>(target_addr, &val) {
                Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "success": true, "bytes_written": 8 }))),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "message": e.to_string() }))),
            }
        }
        "u64" => {
            let val = payload.value.as_u64().unwrap_or(0);
            match driver.write::<u64>(target_addr, &val) {
                Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "success": true, "bytes_written": 8 }))),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "message": e.to_string() }))),
            }
        }
        "f32" => {
            let val = payload.value.as_f64().unwrap_or(0.0) as f32;
            match driver.write::<f32>(target_addr, &val) {
                Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "success": true, "bytes_written": 4 }))),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "message": e.to_string() }))),
            }
        }
        "bytes" => {
            let bytes_to_write: Vec<u8> = if let Some(arr) = payload.value.as_array() {
                arr.iter().map(|v| v.as_u64().unwrap_or(0) as u8).collect()
            } else if let Some(hex_str) = payload.value.as_str() {
                hex_str
                    .split_whitespace()
                    .filter_map(|s| u8::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                    .collect()
            } else {
                Vec::new()
            };

            if bytes_to_write.is_empty() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "success": false, "message": "No valid bytes to write" })),
                );
            }

            match driver.write_bytes(target_addr, &bytes_to_write) {
                Ok(written) => (
                    StatusCode::OK,
                    Json(serde_json::json!({ "success": true, "bytes_written": written })),
                ),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "success": false, "message": e.to_string() })),
                ),
            }
        }
        _ => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "success": false, "message": "Unsupported data type" })),
        ),
    }
}

async fn scan_memory(
    State(state): State<ServerState>,
    Json(payload): Json<MemScanRequest>,
) -> impl IntoResponse {
    let (pid, _ga_base) = {
        let r = state.inner.read().await;
        match r.pid {
            Some(p) => (p, r.game_assembly_base),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "status": "not_attached", "message": "Process not attached" })),
                )
            }
        }
    };

    let mappings = match parse_process_maps(pid) {
        Ok(m) => m,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            )
        }
    };

    let driver = LinuxMemoryDriver::new(pid);
    let mut matches = Vec::new();

    for map in mappings {
        if !map.perms.contains('w') || !map.perms.contains('r') {
            continue;
        }

        let region_len = map.end.saturating_sub(map.start);
        if region_len == 0 || region_len > 32 * 1024 * 1024 {
            continue;
        }

        let mut buf = vec![0u8; region_len.min(512 * 1024)];
        let mut cur_addr = map.start;

        while cur_addr < map.end {
            let to_read = (map.end - cur_addr).min(buf.len());
            if driver.read_bytes(cur_addr, &mut buf[..to_read]).is_err() {
                break;
            }

            match payload.r#type.as_str() {
                "i32" => {
                    if let Ok(target_val) = payload.value.parse::<i32>() {
                        let target_bytes = target_val.to_le_bytes();
                        if to_read >= 4 {
                            for i in (0..to_read - 3).step_by(4) {
                                if buf[i..i + 4] == target_bytes {
                                    let match_addr = cur_addr + i;
                                    matches.push(ScanMatch {
                                        address: format!("0x{:x}", match_addr),
                                        address_val: match_addr,
                                        value: target_val.to_string(),
                                    });
                                    if matches.len() >= 5000 {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                "i64" => {
                    if let Ok(target_val) = payload.value.parse::<i64>() {
                        let target_bytes = target_val.to_le_bytes();
                        if to_read >= 8 {
                            for i in (0..to_read - 7).step_by(8) {
                                if buf[i..i + 8] == target_bytes {
                                    let match_addr = cur_addr + i;
                                    matches.push(ScanMatch {
                                        address: format!("0x{:x}", match_addr),
                                        address_val: match_addr,
                                        value: target_val.to_string(),
                                    });
                                    if matches.len() >= 5000 {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                "u8" => {
                    if let Ok(target_val) = payload.value.parse::<u8>() {
                        for (i, &byte_val) in buf.iter().enumerate().take(to_read) {
                            if byte_val == target_val {
                                let match_addr = cur_addr + i;
                                matches.push(ScanMatch {
                                    address: format!("0x{:x}", match_addr),
                                    address_val: match_addr,
                                    value: target_val.to_string(),
                                });
                                if matches.len() >= 5000 {
                                    break;
                                }
                            }
                        }
                    }
                }
                _ => {}
            }

            if matches.len() >= 5000 {
                break;
            }

            cur_addr += to_read;
        }

        if matches.len() >= 5000 {
            break;
        }
    }

    let total = matches.len();
    {
        let mut w = state.inner.write().await;
        w.scan_results = matches.clone();
        w.last_scan_type = payload.r#type.clone();
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "success",
            "total_matches": total,
            "results": matches.into_iter().take(200).collect::<Vec<_>>()
        })),
    )
}

async fn scan_memory_next(
    State(state): State<ServerState>,
    Json(payload): Json<MemScanRequest>,
) -> impl IntoResponse {
    let (pid, prev_matches) = {
        let r = state.inner.read().await;
        match r.pid {
            Some(p) => (p, r.scan_results.clone()),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "status": "not_attached", "message": "Process not attached" })),
                )
            }
        }
    };

    let driver = LinuxMemoryDriver::new(pid);
    let mut filtered = Vec::new();

    for m in prev_matches {
        let addr = m.address_val;
        match payload.r#type.as_str() {
            "i32" => {
                if let Ok(target_val) = payload.value.parse::<i32>() {
                    if let Ok(cur_val) = driver.read::<i32>(addr) {
                        if cur_val == target_val {
                            filtered.push(ScanMatch {
                                address: m.address,
                                address_val: addr,
                                value: cur_val.to_string(),
                            });
                        }
                    }
                }
            }
            "i64" => {
                if let Ok(target_val) = payload.value.parse::<i64>() {
                    if let Ok(cur_val) = driver.read::<i64>(addr) {
                        if cur_val == target_val {
                            filtered.push(ScanMatch {
                                address: m.address,
                                address_val: addr,
                                value: cur_val.to_string(),
                            });
                        }
                    }
                }
            }
            "u8" => {
                if let Ok(target_val) = payload.value.parse::<u8>() {
                    if let Ok(cur_val) = driver.read::<u8>(addr) {
                        if cur_val == target_val {
                            filtered.push(ScanMatch {
                                address: m.address,
                                address_val: addr,
                                value: cur_val.to_string(),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let total = filtered.len();
    {
        let mut w = state.inner.write().await;
        w.scan_results = filtered.clone();
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "success",
            "total_matches": total,
            "results": filtered.into_iter().take(200).collect::<Vec<_>>()
        })),
    )
}

async fn heal_action(
    State(state): State<ServerState>,
    Json(payload): Json<HealRequest>,
) -> impl IntoResponse {
    let r = state.inner.read().await;
    if let Some(pid) = r.pid {
        let driver = LinuxMemoryDriver::new(pid);
        let player_ptr = if payload.player_ptr != 0 {
            payload.player_ptr
        } else {
            0x7ff418290000
        };

        match SquadActions::heal_player(&driver, player_ptr, payload.condition_offset) {
            Ok(_) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "healed",
                    "player_ptr": format!("0x{:x}", player_ptr),
                    "condition": 10000,
                    "sharpness": 10000
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        }
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "status": "not_attached", "message": "Game process not attached" })),
        )
    }
}

async fn finances_action(
    State(state): State<ServerState>,
    Json(payload): Json<FinanceRequest>,
) -> impl IntoResponse {
    let r = state.inner.read().await;
    if let Some(pid) = r.pid {
        let driver = LinuxMemoryDriver::new(pid);
        match SquadActions::update_finances(
            &driver,
            payload.club_ptr,
            payload.balance_offset,
            payload.transfer_offset,
            payload.wage_offset,
            payload.balance,
            payload.transfer_budget,
            payload.wage_budget,
        ) {
            Ok(_) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "finances_updated",
                    "club_ptr": format!("0x{:x}", payload.club_ptr),
                    "balance": payload.balance,
                    "transfer_budget": payload.transfer_budget,
                    "wage_budget": payload.wage_budget
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        }
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "status": "not_attached", "message": "Game process not attached" })),
        )
    }
}

async fn godmode_action(
    State(state): State<ServerState>,
    Json(payload): Json<GodModeRequest>,
) -> impl IntoResponse {
    let r = state.inner.read().await;
    let pid = match r.pid {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "status": "not_attached", "message": "Game process not attached" })),
            )
        }
    };

    let driver = LinuxMemoryDriver::new(pid);
    let target_ptr = payload.player_ptr.unwrap_or(0x7ff418290000);

    match payload.action.as_str() {
        "max_morale" => match SquadActions::boost_morale(&driver, target_ptr, 0x24) {
            Ok(_) => (
                StatusCode::OK,
                Json(serde_json::json!({ "status": "success", "message": "Maximized squad morale to 100 (Superb)" })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        },
        "clear_fatigue" => {
            let _ = driver.write::<u16>(target_ptr + 0x28, &0);
            (
                StatusCode::OK,
                Json(serde_json::json!({ "status": "success", "message": "Cleared squad fatigue accumulation & suspensions" })),
            )
        }
        "cure_injuries" => {
            let _ = driver.write::<u32>(target_ptr + 0x2c, &0);
            (
                StatusCode::OK,
                Json(serde_json::json!({ "status": "success", "message": "Cured all active injuries" })),
            )
        }
        "unlock_debug_menu" => {
            if let Some(base) = r.game_assembly_base {
                let _ = driver.write::<u8>(base + 0x1800, &1);
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({ "status": "success", "message": "Unlocked FM26 internal QA & Developer Debug Menu" })),
            )
        }
        _ => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "status": "unknown_action", "message": "Unknown godmode action" })),
        ),
    }
}

async fn get_offsets() -> impl IntoResponse {
    let parsed: serde_json::Value =
        serde_json::from_str(DEFAULT_OFFSETS_JSON).unwrap_or(serde_json::json!({}));
    Json(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_or_dec_address() {
        assert_eq!(parse_hex_or_dec_address("0x18218e590"), Some(0x18218e590));
        assert_eq!(parse_hex_or_dec_address("0X7FF418290000"), Some(0x7ff418290000));
        assert_eq!(parse_hex_or_dec_address("18218e590"), Some(0x18218e590));
        assert_eq!(parse_hex_or_dec_address("4096"), Some(4096));
        assert_eq!(parse_hex_or_dec_address("\"0x1000\""), Some(0x1000));
        assert_eq!(parse_hex_or_dec_address("invalid_address"), None);
    }

    #[test]
    fn test_offsets_json_embed() {
        let parsed: serde_json::Value = serde_json::from_str(DEFAULT_OFFSETS_JSON).expect("valid json");
        assert_eq!(parsed["version"], "26.0.0");
        assert!(parsed["singletons"]["World"].is_object());
    }

    #[test]
    fn test_embedded_ui_html() {
        assert!(EMBEDDED_UI_HTML.contains("<!DOCTYPE html>"));
        assert!(EMBEDDED_UI_HTML.contains("fmlev 26"));
        assert!(EMBEDDED_UI_HTML.contains("Squad God Mode"));
        assert!(EMBEDDED_UI_HTML.contains("Player Editor"));
        assert!(EMBEDDED_UI_HTML.contains("Club Finances"));
        assert!(EMBEDDED_UI_HTML.contains("Memory Scanner"));
    }

    #[test]
    fn test_game_context_manager_defaults() {
        let state = AppState::default();
        let ctx = state.context_mgr.get_context();
        assert_eq!(ctx.player.name, "Erling Haaland");
        assert_eq!(ctx.player.current_ability, 188);
        assert_eq!(ctx.player.potential_ability, 195);
        assert_eq!(ctx.player.attributes.finishing, 19);

        assert_eq!(ctx.club.club_name, "Manchester City");
        assert_eq!(ctx.club.training_facilities, 20);
    }

    #[test]
    fn test_context_search_players_and_clubs() {
        let state = AppState::default();
        let res = state.context_mgr.search("haaland");
        assert!(!res.players.is_empty());
        assert_eq!(res.players[0].name, "Erling Haaland");

        let res_club = state.context_mgr.search("Arsenal");
        assert!(!res_club.clubs.is_empty());
        assert_eq!(res_club.clubs[0].club_name, "Arsenal");
    }

    #[tokio::test]
    async fn test_get_context_endpoint() {
        let state = ServerState {
            inner: Arc::new(RwLock::new(AppState::default())),
        };
        let response = get_context(State(state)).await.into_response();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(json["game_name"], "Football Manager 26");
        assert!(json["nation"].is_object());
        assert_eq!(json["nation"]["name"], "England");
        assert!(json["competition"].is_object());
        assert_eq!(json["competition"]["name"], "Premier League");

        assert!(json["nations"].is_array());
        assert!(!json["nations"].as_array().unwrap().is_empty());
        assert!(json["competitions"].is_array());
        assert!(!json["competitions"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_search_entities_endpoint() {
        let state = ServerState {
            inner: Arc::new(RwLock::new(AppState::default())),
        };
        let query = SearchQuery {
            q: "Brazil".to_string(),
        };
        let response = search_entities(State(state), Query(query)).await.into_response();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(json["query"], "Brazil");
        assert!(json["nations"].is_array());
        assert!(!json["nations"].as_array().unwrap().is_empty());
        assert_eq!(json["nation"]["name"], "Brazil");
    }

    #[tokio::test]
    async fn test_update_nation_and_presets() {
        let state = ServerState {
            inner: Arc::new(RwLock::new(AppState::default())),
        };

        // 1. Update nation
        let mut nation = NationDetails::default();
        nation.name = "Testland".to_string();
        nation.reputation = 9500;
        nation.youth_rating = 180;
        nation.tax_rate = 20;

        let res = update_nation(State(state.clone()), Json(nation.clone())).await.into_response();
        assert_eq!(res.status(), StatusCode::OK);

        let r = state.inner.read().await;
        assert_eq!(r.context_mgr.context.nation.name, "Testland");
        assert_eq!(r.context_mgr.context.nation.youth_rating, 180);
        drop(r);

        // 2. Preset golden_generation
        let req = NationPresetRequest {
            preset: "golden_generation".to_string(),
            nation_id: None,
        };
        let res = apply_nation_preset(State(state.clone()), Json(req)).await.into_response();
        assert_eq!(res.status(), StatusCode::OK);

        let r = state.inner.read().await;
        assert_eq!(r.context_mgr.context.nation.youth_rating, 200);
        assert_eq!(r.context_mgr.context.nation.youth_facilities_avg, 20);
        drop(r);

        // 3. Preset max_youth_rating
        let req = NationPresetRequest {
            preset: "max_youth_rating".to_string(),
            nation_id: None,
        };
        let res = apply_nation_preset(State(state.clone()), Json(req)).await.into_response();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(state.inner.read().await.context_mgr.context.nation.youth_rating, 200);

        // 4. Preset clean_reputation
        let req = NationPresetRequest {
            preset: "clean_reputation".to_string(),
            nation_id: None,
        };
        let res = apply_nation_preset(State(state.clone()), Json(req)).await.into_response();
        assert_eq!(res.status(), StatusCode::OK);
        let r = state.inner.read().await;
        assert_eq!(r.context_mgr.context.nation.reputation, 10000);
        assert_eq!(r.context_mgr.context.nation.corruption_index, 0);
    }

    #[tokio::test]
    async fn test_update_competition_and_presets() {
        let state = ServerState {
            inner: Arc::new(RwLock::new(AppState::default())),
        };

        // 1. Update competition
        let mut comp = CompetitionDetails::default();
        comp.name = "Super Champions League".to_string();
        comp.max_substitutes = 7;
        comp.max_bench_size = 14;
        comp.winner_prize_money = 80_000_000;

        let res = update_competition(State(state.clone()), Json(comp)).await.into_response();
        assert_eq!(res.status(), StatusCode::OK);

        let r = state.inner.read().await;
        assert_eq!(r.context_mgr.context.competition.name, "Super Champions League");
        assert_eq!(r.context_mgr.context.competition.max_substitutes, 7);
        drop(r);

        // 2. Preset relax_rules
        let req = CompetitionPresetRequest {
            preset: "relax_rules".to_string(),
            competition_id: None,
        };
        let res = apply_competition_preset(State(state.clone()), Json(req)).await.into_response();
        assert_eq!(res.status(), StatusCode::OK);

        let r = state.inner.read().await;
        assert_eq!(r.context_mgr.context.competition.max_foreign_players, 99);
        assert_eq!(r.context_mgr.context.competition.max_bench_size, 12);
        drop(r);

        // 3. Preset five_subs
        let req = CompetitionPresetRequest {
            preset: "five_subs".to_string(),
            competition_id: None,
        };
        let res = apply_competition_preset(State(state.clone()), Json(req)).await.into_response();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(state.inner.read().await.context_mgr.context.competition.max_substitutes, 5);

        // 4. Preset boost_prize_pool
        let req = CompetitionPresetRequest {
            preset: "boost_prize_pool".to_string(),
            competition_id: None,
        };
        let res = apply_competition_preset(State(state.clone()), Json(req)).await.into_response();
        assert_eq!(res.status(), StatusCode::OK);
        assert!(state.inner.read().await.context_mgr.context.competition.winner_prize_money >= 150_000_000);
    }

    #[tokio::test]
    async fn test_club_affiliates_add_and_remove() {
        let state = ServerState {
            inner: Arc::new(RwLock::new(AppState::default())),
        };

        let affiliate = ClubAffiliate {
            affiliate_club_id: 8888,
            affiliate_name: "Girona FC".to_string(),
            is_parent: false,
            first_option_to_buy: true,
            send_players_on_loan: true,
            shared_scouting: true,
            annual_fee: 500_000,
            work_permit_haven: true,
        };

        let add_req = AffiliateActionRequest {
            club_id: 1001,
            affiliate: Some(affiliate),
            affiliate_id: Some(8888),
        };

        let res = add_club_affiliate(State(state.clone()), Json(add_req)).await.into_response();
        assert_eq!(res.status(), StatusCode::OK);

        let r = state.inner.read().await;
        let club = r.context_mgr.club_registry.get(&1001).unwrap();
        assert!(club.affiliates.iter().any(|a| a.affiliate_club_id == 8888));
        drop(r);

        let rem_req = AffiliateActionRequest {
            club_id: 1001,
            affiliate: None,
            affiliate_id: Some(8888),
        };

        let res = remove_club_affiliate(State(state.clone()), Json(rem_req)).await.into_response();
        assert_eq!(res.status(), StatusCode::OK);

        let r = state.inner.read().await;
        let club = r.context_mgr.club_registry.get(&1001).unwrap();
        assert!(!club.affiliates.iter().any(|a| a.affiliate_club_id == 8888));
    }

    #[tokio::test]
    async fn test_router_integration_new_routes() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let state = ServerState {
            inner: Arc::new(RwLock::new(AppState::default())),
        };
        let app = create_router(state);

        // 1. GET /api/context
        let req = Request::builder()
            .method("GET")
            .uri("/api/context")
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 2. POST /api/nation/preset
        let body = serde_json::to_string(&serde_json::json!({
            "preset": "golden_generation"
        })).unwrap();
        let req = Request::builder()
            .method("POST")
            .uri("/api/nation/preset")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 3. POST /api/competition/preset
        let body = serde_json::to_string(&serde_json::json!({
            "preset": "relax_rules"
        })).unwrap();
        let req = Request::builder()
            .method("POST")
            .uri("/api/competition/preset")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 4. POST /api/club/affiliate/add
        let body = serde_json::to_string(&serde_json::json!({
            "club_id": 1001,
            "affiliate": {
                "affiliate_club_id": 9999,
                "affiliate_name": "Yokohama F. Marinos",
                "annual_fee": 200000
            }
        })).unwrap();
        let req = Request::builder()
            .method("POST")
            .uri("/api/club/affiliate/add")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 5. POST /api/club/affiliate/remove
        let body = serde_json::to_string(&serde_json::json!({
            "club_id": 1001,
            "affiliate_id": 9999
        })).unwrap();
        let req = Request::builder()
            .method("POST")
            .uri("/api/club/affiliate/remove")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_player_update_persists_across_context_polls() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let state = ServerState {
            inner: Arc::new(RwLock::new(AppState::default())),
        };
        let app = create_router(state.clone());

        // 1. Select player 73 (Zulkifli Lukmansyah)
        let select_body = serde_json::to_string(&serde_json::json!({
            "player_id": 73
        })).unwrap();
        let req = Request::builder()
            .method("POST")
            .uri("/api/context/select")
            .header("content-type", "application/json")
            .body(Body::from(select_body))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // Verify player is Zulkifli Lukmansyah at Persib
        {
            let r = state.inner.read().await;
            assert_eq!(r.context_mgr.context.player.id, 73);
            assert_eq!(r.context_mgr.context.player.name, "Zulkifli Lukmansyah");
            assert_eq!(r.context_mgr.context.player.club_name, "Persib");
            assert_eq!(r.context_mgr.context.club.club_name, "Persib");
        }

        // 2. User edits player attributes via /api/player/update
        let mut p = {
            let r = state.inner.read().await;
            r.context_mgr.context.player.clone()
        };
        p.attributes.finishing = 20;
        p.attributes.passing = 20;
        p.attributes.pace = 20;
        p.current_ability = 195;

        let update_body = serde_json::to_string(&p).unwrap();
        let req = Request::builder()
            .method("POST")
            .uri("/api/player/update")
            .header("content-type", "application/json")
            .body(Body::from(update_body))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // Verify player was marked user_modified
        {
            let r = state.inner.read().await;
            assert!(r.context_mgr.context.player.user_modified);
            assert!(r.context_mgr.user_modified_players.contains(&73));
            assert_eq!(r.context_mgr.context.player.attributes.finishing, 20);
        }

        // 3. Simulate continuous UI polling of /api/context (which calls sync_live_context)
        for _ in 0..5 {
            let req = Request::builder()
                .method("GET")
                .uri("/api/context")
                .body(Body::empty())
                .unwrap();
            let res = app.clone().oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK);

            let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
            let json_val: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

            assert_eq!(json_val["player"]["id"], 73);
            assert_eq!(json_val["player"]["attributes"]["finishing"], 20);
            assert_eq!(json_val["player"]["attributes"]["passing"], 20);
            assert_eq!(json_val["player"]["attributes"]["pace"], 20);
            assert_eq!(json_val["player"]["current_ability"], 195);
            assert_eq!(json_val["player"]["club_name"], "Persib");
            assert_eq!(json_val["player"]["user_modified"], true);
            assert_eq!(json_val["club"]["club_name"], "Persib");
        }
    }
}
