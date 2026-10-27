use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    Json,
};

use fmlev_core::{
    ClubAffiliate, ClubFinances, CompetitionDetails, NationDetails, PlayerAttributes,
};
use fmlev_mem::get_yama_diagnostic;

use crate::state::{
    AffiliateActionRequest, ClubPresetRequest, CompetitionPresetRequest, ContextSelectRequest,
    NationPresetRequest, PlayerPresetRequest, PlayerUpdateRequest, SearchQuery, ServerState,
    StatusResponse,
};
use crate::embedded_ui_html;
use super::api_error;

pub async fn serve_ui() -> impl IntoResponse {
    Html(embedded_ui_html())
}

pub async fn get_status(State(state): State<ServerState>) -> impl IntoResponse {
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

pub async fn get_context(State(state): State<ServerState>) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    let sync_error = w.context_mgr.sync_live_context().err().map(|error| error.to_string());
    let ctx = w.context_mgr.get_context();
    w.is_attached = ctx.attached;
    w.pid = ctx.pid;

    let nations = w.context_mgr.all_nations();
    let competitions = w.context_mgr.all_competitions();

    Json(serde_json::json!({
        "connected": ctx.attached,
        "sync_error": sync_error,
        "pid": ctx.pid,
        "game_name": "Football Manager 26",
        "screen_state": ctx.active_screen,
        "sync_mode": ctx.sync_mode,
        "is_pinned": ctx.sync_mode == "pinned",
        "player": ctx.player,
        "active_player": ctx.player,
        "player_writable_fields": w.context_mgr.player_writable_fields(),
        "club": ctx.club,
        "active_club": ctx.club,
        "nation": ctx.nation,
        "active_nation": ctx.nation,
        "competition": ctx.competition,
        "active_competition": ctx.competition,
        "nations": nations,
        "competitions": competitions,
        "live_clubs": w.context_mgr.live_clubs,
        "live_nations": w.context_mgr.live_nations,
        "live_competitions": w.context_mgr.live_competitions
    }))
}

pub async fn search_entities(
    State(state): State<ServerState>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    let mut r = state.inner.write().await;
    let discovery_error = if query.q.trim().chars().count() >= 2 {
        r.context_mgr.maybe_auto_discover_players().and_then(|result| result.err().map(|error| error.to_string()))
    } else { None };
    let mut results = r.context_mgr.search(&query.q);
    let total_players = results.players.len();
    // Native discovery can add an entire loaded database. Keep dropdown responses bounded.
    results.players.truncate(100);
    let nation = results.nations.first().cloned();
    let competition = results.competitions.first().cloned();
    Json(serde_json::json!({
        "query": query.q,
        "players": results.players,
        "total_players": total_players,
        "discovery_error": discovery_error,
        "clubs": results.clubs,
        "nations": results.nations,
        "competitions": results.competitions,
        "nation": nation,
        "competition": competition
    }))
}

pub async fn discover_players(State(state): State<ServerState>) -> impl IntoResponse {
    // A full process scan is blocking; leave it off the async request executor.
    match tokio::task::spawn_blocking(move || {
        state.inner.blocking_write().context_mgr.discover_loaded_players()
    }).await {
        Ok(Ok(count)) => (StatusCode::OK, Json(serde_json::json!({
            "status": "success", "count": count,
            "message": format!("Indexed {count} loaded native players. Search by name to edit their verified attributes, positions and feet.")
        }))),
        Ok(Err(error)) => api_error(StatusCode::BAD_REQUEST, error),
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

pub async fn select_context(
    State(state): State<ServerState>,
    Json(payload): Json<ContextSelectRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    let targets = [payload.player_id, payload.club_id, payload.nation_id, payload.competition_id]
        .iter().filter(|target| target.is_some()).count();
    if (payload.unpin == Some(true) && targets != 0) || (payload.unpin != Some(true) && targets != 1) {
        return api_error(StatusCode::BAD_REQUEST, "Select exactly one entity, or unpin without an entity");
    }
    if payload.unpin == Some(true) {
        w.context_mgr.set_sync_mode("auto_follow");
    } else {
        if let Some(pid) = payload.player_id {
            if let Err(error) = w.context_mgr.select_player(pid) {
                return api_error(StatusCode::BAD_REQUEST, error);
            }
            w.context_mgr.set_sync_mode("pinned");
        }
        if let Some(cid) = payload.club_id {
            if let Err(error) = w.context_mgr.select_club(cid) {
                return api_error(StatusCode::BAD_REQUEST, error);
            }
            w.context_mgr.set_sync_mode("pinned");
        }
        if let Some(nid) = payload.nation_id {
            if let Err(error) = w.context_mgr.select_nation(nid) {
                return api_error(StatusCode::BAD_REQUEST, error);
            }
            w.context_mgr.set_sync_mode("pinned");
        }
        if let Some(comp_id) = payload.competition_id {
            if let Err(error) = w.context_mgr.select_competition(comp_id) {
                return api_error(StatusCode::BAD_REQUEST, error);
            }
            w.context_mgr.set_sync_mode("pinned");
        }
    }

    let ctx = w.context_mgr.get_context();
    (StatusCode::OK, Json(serde_json::json!({
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
    })))
}


pub async fn update_player(
    State(state): State<ServerState>,
    Json(payload): Json<PlayerUpdateRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    let name = payload.player.name.clone();
    match w.context_mgr.update_player_fields(payload.player, payload.changed_fields.as_deref()) {
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

pub async fn apply_player_preset(
    State(state): State<ServerState>,
    Json(payload): Json<PlayerPresetRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    if let Some(pid) = payload.player_id {
        if let Err(error) = w.context_mgr.select_player(pid) {
            return api_error(StatusCode::BAD_REQUEST, error);
        }
    }

    match payload.preset.as_str() {
        "boost_all_2" | "boost_attributes_2" => {
            let result = if w.context_mgr.player_writable_fields().is_some() {
                w.context_mgr.apply_quick_action("boost_all_2")
            } else {
                let mut p = w.context_mgr.context.player.clone();
                boost_player_attributes(&mut p.attributes, 2);
                p.current_ability = (p.current_ability as u16 + 10).min(200) as u8;
                w.context_mgr.update_player(p)
            };
            if let Err(error) = result {
                return api_error(StatusCode::BAD_REQUEST, error);
            }
            let ctx = w.context_mgr.get_context();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Boosted writable attributes for {} by +2!", ctx.player.name),
                    "player": ctx.player,
                    "active_player": ctx.player
                })),
            )
        }
        "free_release" => {
            let mut p = w.context_mgr.context.player.clone();
            p.contract.release_clause = 0;
            if let Err(error) = w.context_mgr.update_player(p) {
                return api_error(StatusCode::BAD_REQUEST, error);
            }
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
                "remove_promises" | "clear_promises" => "remove_promises",
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

pub async fn update_club(
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

pub async fn apply_club_preset(
    State(state): State<ServerState>,
    Json(payload): Json<ClubPresetRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    if let Some(cid) = payload.club_id {
        if let Err(error) = w.context_mgr.select_club(cid) {
            return api_error(StatusCode::BAD_REQUEST, error);
        }
    }

    match payload.preset.as_str() {
        "morale_squad" => {
            if w.context_mgr.session.is_some() {
                return api_error(StatusCode::NOT_IMPLEMENTED,
                    "Squad writes require verified native player bindings and are unavailable for this live session");
            }
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
            if w.context_mgr.session.is_some() {
                return api_error(StatusCode::NOT_IMPLEMENTED,
                    "Squad writes require verified native player bindings and are unavailable for this live session");
            }
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
        "unlock_ige" => api_error(
            StatusCode::NOT_IMPLEMENTED,
            "The in-game editor unlock has no verified memory binding and is unavailable",
        ),
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

pub async fn update_nation(
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

pub async fn apply_nation_preset(
    State(state): State<ServerState>,
    Json(payload): Json<NationPresetRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    if let Some(nid) = payload.nation_id {
        if let Err(error) = w.context_mgr.select_nation(nid) {
            return api_error(StatusCode::BAD_REQUEST, error);
        }
    }

    match payload.preset.as_str() {
        "golden_generation" => {
            if let Err(error) = w.context_mgr.apply_quick_action("golden_generation") {
                return api_error(StatusCode::BAD_REQUEST, error);
            }
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
            if let Err(error) = w.context_mgr.update_nation(nation) {
                return api_error(StatusCode::BAD_REQUEST, error);
            }
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
            if let Err(error) = w.context_mgr.update_nation(nation) {
                return api_error(StatusCode::BAD_REQUEST, error);
            }
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
            if let Err(error) = w.context_mgr.update_nation(nation) {
                return api_error(StatusCode::BAD_REQUEST, error);
            }
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

pub async fn update_competition(
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

pub async fn apply_competition_preset(
    State(state): State<ServerState>,
    Json(payload): Json<CompetitionPresetRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    if let Some(cid) = payload.competition_id {
        if let Err(error) = w.context_mgr.select_competition(cid) {
            return api_error(StatusCode::BAD_REQUEST, error);
        }
    }

    match payload.preset.as_str() {
        "relax_rules" => {
            if let Err(error) = w.context_mgr.apply_quick_action("relax_rules") {
                return api_error(StatusCode::BAD_REQUEST, error);
            }
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
            if let Err(error) = w.context_mgr.update_competition(comp) {
                return api_error(StatusCode::BAD_REQUEST, error);
            }
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
            if let Err(error) = w.context_mgr.update_competition(comp) {
                return api_error(StatusCode::BAD_REQUEST, error);
            }
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
            if let Err(error) = w.context_mgr.update_competition(comp) {
                return api_error(StatusCode::BAD_REQUEST, error);
            }
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

pub async fn add_club_affiliate(
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

pub async fn remove_club_affiliate(
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

/// Reads or writes the live league prize table (display-verified; payout source unconfirmed).
pub async fn competition_prizes(
    State(state): State<ServerState>,
    Json(payload): Json<crate::state::PrizeRequest>,
) -> impl IntoResponse {
    let writing = payload.prizes.is_some();
    let mut w = state.inner.write().await;
    match w.context_mgr.competition_prizes(
        payload.teams,
        payload.first_prize,
        payload.last_prize,
        payload.tolerance,
        payload.prizes
    ) {
        Ok(prizes) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "success",
                "message": if writing { "Prize money written. Reopen Competition > Rules to see it; season-end payout is unconfirmed." } else { "Prize money read." },
                "prizes": prizes
            })),
        ),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}

/// Sets a player's native squad number; a teammate wearing it is swapped.
pub async fn update_squad_number(
    State(state): State<ServerState>,
    Json(payload): Json<crate::state::SquadNumberRequest>,
) -> impl IntoResponse {
    let number = payload.number.filter(|n| *n != 0);
    let mut w = state.inner.write().await;
    match w.context_mgr.update_squad_number(payload.player_id, number) {
        Ok(swapped) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "success",
                "message": match (number, swapped) {
                    (_, Some(other)) => format!("Squad number saved; swapped with player {other}. FM26 shows it after the next day."),
                    (Some(_), None) => "Squad number saved. FM26 shows it after the next day.".to_string(),
                    (None, None) => "Squad number cleared. FM26 shows it after the next day.".to_string(),
                },
                "swapped_with": swapped
            })),
        ),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}

/// Extends a player's native contract expiry (default 1 year).
pub async fn extend_contract(
    State(state): State<ServerState>,
    Json(payload): Json<crate::state::ContractExtendRequest>,
) -> impl IntoResponse {
    let years = payload.years.unwrap_or(1);
    let mut w = state.inner.write().await;
    match w.context_mgr.extend_contract(payload.player_id, years) {
        Ok((old, new)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "success",
                "message": format!("Contract extended from {old} to {new}. FM26 shows it after the next day."),
                "old_year": old,
                "new_year": new
            })),
        ),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}

/// Terminates a player's contract in native memory, detaching contract, team, and agreement links,
/// and removing the player from team squad vectors. Releases the player as a free agent.
pub async fn terminate_contract(
    State(state): State<ServerState>,
    Json(payload): Json<crate::state::ContractTerminateRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    match w.context_mgr.terminate_contract(payload.player_id) {
        Ok(cleared_teams) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "success",
                "message": format!("Player contract terminated and detached from {} squad vector(s). Player is now a free agent.", cleared_teams.len()),
                "cleared_teams": cleared_teams
            })),
        ),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}

/// Terminates an active loan, returning the player from the loan club to their parent club.
pub async fn terminate_loan(
    State(state): State<ServerState>,
    Json(payload): Json<crate::state::LoanTerminateRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    match w.context_mgr.terminate_loan(payload.player_id) {
        Ok((loan_team, parent_team)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "success",
                "message": "Loan terminated. Player returned to parent club squad.",
                "loan_team": format!("0x{:x}", loan_team),
                "parent_team": format!("0x{:x}", parent_team),
            })),
        ),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}

/// Moves a contracted player to a destination club or team.
pub async fn move_player(
    State(state): State<ServerState>,
    Json(payload): Json<crate::state::PlayerMoveRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    match w.context_mgr.move_player(payload.player_id, payload.target_club_id, payload.target_team_address) {
        Ok((from_team, to_team)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "success",
                "message": "Player moved successfully to target team squad.",
                "from_team": format!("0x{:x}", from_team),
                "to_team": format!("0x{:x}", to_team),
            })),
        ),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}

/// Signs an unemployed free agent to a target club squad.
pub async fn sign_player(
    State(state): State<ServerState>,
    Json(payload): Json<crate::state::PlayerSignRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    match w.context_mgr.sign_free_agent(
        payload.player_id,
        payload.target_club_id,
        payload.target_team_address,
        payload.wage_gbp,
        payload.expiry_year,
    ) {
        Ok(team_ptr) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "success",
                "message": "Player signed successfully to target club squad.",
                "target_team": format!("0x{:x}", team_ptr),
            })),
        ),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}

/// Accelerates the scheduled join date of an arranged pending transfer or completes it instantly.
pub async fn accelerate_transfer(
    State(state): State<ServerState>,
    Json(payload): Json<crate::state::TransferAccelerateRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    if payload.instant {
        match w.context_mgr.complete_pending_transfer(payload.player_id) {
            Ok((from_team, to_team)) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": "Pending transfer completed immediately. Player moved to destination squad.",
                    "from_team": format!("0x{:x}", from_team),
                    "to_team": format!("0x{:x}", to_team),
                })),
            ),
            Err(error) => api_error(StatusCode::BAD_REQUEST, error),
        }
    } else {
        match w.context_mgr.accelerate_pending_transfer(payload.player_id, payload.new_join_date) {
            Ok((old_date, new_date)) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": "Pending transfer join date accelerated.",
                    "old_join_date": format!("0x{:x}", old_date),
                    "new_join_date": format!("0x{:x}", new_date),
                })),
            ),
            Err(error) => api_error(StatusCode::BAD_REQUEST, error),
        }
    }
}

/// Loan agreement creation is not implemented: loans require complex multi-club loan contract nodes.
pub async fn transfer_not_implemented() -> impl IntoResponse {
    api_error(
        StatusCode::NOT_IMPLEMENTED,
        fmlev_core::CoreError::InvalidValue("Creating new loan agreements via raw memory writes is not supported. Use Move Player or Sign Player instead.".into()),
    )
}

/// Writes the active club's native balance and/or transfer budget (Club > Finances must be open).
pub async fn update_club_money(
    State(state): State<ServerState>,
    Json(payload): Json<crate::state::ClubMoneyRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    if let Err(error) = w.context_mgr.sync_live_context() {
        return api_error(StatusCode::BAD_REQUEST, error);
    }
    match w.context_mgr.update_club_money(payload.club_id, payload.balance, payload.transfer_budget) {
        Ok(club) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "success",
                "message": if payload.balance.is_none() && payload.transfer_budget.is_none() { "Club money read from the game." } else { "Club money saved. Reopen Club > Finances to see it." },
                "club": club
            })),
        ),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}

/// Reads the verified native club values (read-only) so the UI loads real numbers.
pub async fn read_club_native(
    State(state): State<ServerState>,
    Json(payload): Json<crate::state::ClubNativeRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    match w.context_mgr.read_club_native(payload.club_id) {
        Ok(club) => (StatusCode::OK, Json(serde_json::json!({ "status": "success", "club": club }))),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}

pub async fn read_nation_native(
    State(state): State<ServerState>,
    Json(payload): Json<crate::state::NationNativeRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    match w.context_mgr.read_nation_native(payload.nation_id) {
        Ok(nation) => (StatusCode::OK, Json(serde_json::json!({ "status": "success", "nation": nation }))),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}

pub async fn read_competition_native(
    State(state): State<ServerState>,
    Json(payload): Json<crate::state::CompetitionNativeRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    match w.context_mgr.read_competition_native(payload.competition_id) {
        Ok(comp) => (StatusCode::OK, Json(serde_json::json!({ "status": "success", "competition": comp }))),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}

/// Sets tactical familiarity to the maximum for a player or his squad (native; same as the game's action).
pub async fn maximise_familiarity(
    State(state): State<ServerState>,
    Json(payload): Json<crate::state::FamiliarityRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    match w.context_mgr.maximise_familiarity(payload.player_id, payload.squad) {
        Ok((players, changed)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "success",
                "message": format!("Tactical familiarity maximised for {players} player(s); {changed} values changed. Reopen the familiarity screen to see it."),
                "players": players,
                "changed": changed
            })),
        ),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}

/// Allows or restores squad registration outside the window for one competition.
pub async fn registration_anytime(
    State(state): State<ServerState>,
    Json(payload): Json<crate::state::RegistrationRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    match w.context_mgr.set_registration_anytime(payload.comp_index, payload.comp_uid, payload.enabled) {
        Ok(result) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "success",
                "message": if payload.enabled { "Registration allowed anytime. Close and reopen the Registration screen." } else { "Registration rules restored. Close and reopen the Registration screen." },
                "manager_flag": result.manager_flag,
                "squad_number_flag": result.squad_number_flag,
                "competition_gate": result.competition_gate
            })),
        ),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}
