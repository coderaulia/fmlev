use super::*;
use axum::{
    body::{to_bytes, Body},
    extract::{Query, State},
    http::{Request, StatusCode},
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

use fmlev_core::{
    ClubAffiliate, CompetitionDetails, GameContextManager, NationDetails, Player,
};

fn offline_app_state() -> AppState {
    AppState {
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
    }
}

#[tokio::test]
async fn partial_attribute_http_update_preserves_other_player_data() {
    let state = ServerState {
        inner: Arc::new(RwLock::new(offline_app_state())),
    };
    let original = state.inner.read().await.context_mgr.get_context().player;
    let app = create_router(state);
    let response = app
        .oneshot(
            Request::post("/api/player/update")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "id": original.id, "attributes": {"passing": 15},
                        "changed_fields": ["attributes.passing"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    let actual: Player = serde_json::from_value(body["player"].clone()).unwrap();
    assert_eq!(actual.attributes.passing, 15);
    assert_eq!(actual.name, original.name);
    assert_eq!(actual.current_ability, original.current_ability);
    assert_eq!(actual.contract, original.contract);
}

#[test]
fn test_parse_hex_or_dec_address() {
    assert_eq!(parse_hex_or_dec_address("0x18218e590"), Some(0x18218e590));
    assert_eq!(
        parse_hex_or_dec_address("0X7FF418290000"),
        Some(0x7ff418290000)
    );
    assert_eq!(parse_hex_or_dec_address("18218e590"), Some(0x18218e590));
    assert_eq!(parse_hex_or_dec_address("4096"), Some(4096));
    assert_eq!(parse_hex_or_dec_address("\"0x1000\""), Some(0x1000));
    assert_eq!(parse_hex_or_dec_address("invalid_address"), None);
}

#[test]
fn test_offsets_json_embed() {
    let parsed: serde_json::Value =
        serde_json::from_str(DEFAULT_OFFSETS_JSON).expect("valid json");
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
    let state = offline_app_state();
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
    let state = offline_app_state();
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
        inner: Arc::new(RwLock::new(offline_app_state())),
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
        inner: Arc::new(RwLock::new(offline_app_state())),
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
        inner: Arc::new(RwLock::new(offline_app_state())),
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
    assert_eq!(
        state.inner.read().await.context_mgr.context.nation.youth_rating,
        200
    );

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
        inner: Arc::new(RwLock::new(offline_app_state())),
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
    assert_eq!(
        state
            .inner
            .read()
            .await
            .context_mgr
            .context
            .competition
            .max_substitutes,
        5
    );

    // 4. Preset boost_prize_pool
    let req = CompetitionPresetRequest {
        preset: "boost_prize_pool".to_string(),
        competition_id: None,
    };
    let res = apply_competition_preset(State(state.clone()), Json(req)).await.into_response();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(
        state
            .inner
            .read()
            .await
            .context_mgr
            .context
            .competition
            .winner_prize_money
            >= 150_000_000
    );
}

#[tokio::test]
async fn test_club_affiliates_add_and_remove() {
    let state = ServerState {
        inner: Arc::new(RwLock::new(offline_app_state())),
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
    let state = ServerState {
        inner: Arc::new(RwLock::new(offline_app_state())),
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
    }))
    .unwrap();
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
    }))
    .unwrap();
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
    }))
    .unwrap();
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
    }))
    .unwrap();
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
    let state = ServerState {
        inner: Arc::new(RwLock::new(offline_app_state())),
    };
    let app = create_router(state.clone());

    // 1. Select player 73 (Zulkifli Lukmansyah)
    let select_body = serde_json::to_string(&serde_json::json!({
        "player_id": 73
    }))
    .unwrap();
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

#[tokio::test]
async fn failed_selection_and_presets_never_report_success() {
    let state = ServerState {
        inner: Arc::new(RwLock::new(offline_app_state())),
    };
    let original = state.inner.read().await.context_mgr.context.player.clone();
    let app = create_router(state.clone());
    for (route, body) in [
        ("/api/context/select", serde_json::json!({"player_id": -999})),
        (
            "/api/player/preset",
            serde_json::json!({"player_id": -999, "preset": "boost_all_2"}),
        ),
        (
            "/api/club/preset",
            serde_json::json!({"club_id": -999, "preset": "morale_squad"}),
        ),
        (
            "/api/nation/preset",
            serde_json::json!({"nation_id": -999, "preset": "golden_generation"}),
        ),
        (
            "/api/competition/preset",
            serde_json::json!({"competition_id": -999, "preset": "five_subs"}),
        ),
        (
            "/api/club/preset",
            serde_json::json!({"preset": "unlock_ige"}),
        ),
        (
            "/api/actions/godmode",
            serde_json::json!({"action": "unlock_debug_menu"}),
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::post(route)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(!response.status().is_success(), "{route} must propagate failure");
        let bytes = axum::body::to_bytes(response.into_body(), 65536).await.unwrap();
        let data: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(data["status"], "error");
    }
    assert_eq!(
        state.inner.read().await.context_mgr.context.player.attributes,
        original.attributes
    );
}

#[tokio::test]
async fn live_preset_errors_are_propagated_without_registry_changes() {
    let mut app = offline_app_state();
    let session = fmlev_core::LiveSession::with_bases(
        std::process::id() as i32,
        fmlev_mem::ModuleBases {
            game_assembly_base: Some(0x10000),
            ..Default::default()
        },
        fmlev_il2cpp::OffsetsRegistry::default(),
    );
    app.context_mgr.session = Some(session);
    let original = app.context_mgr.context.nation.youth_rating;
    let state = ServerState {
        inner: Arc::new(RwLock::new(app)),
    };
    let response = apply_nation_preset(
        State(state.clone()),
        Json(NationPresetRequest {
            preset: "max_youth_rating".into(),
            nation_id: None,
        }),
    )
    .await
    .into_response();
    assert!(!response.status().is_success());
    assert_eq!(
        state.inner.read().await.context_mgr.context.nation.youth_rating,
        original
    );
}

#[test]
fn memory_write_values_reject_truncation_and_malformed_bytes() {
    for (kind, value) in [
        ("u8", serde_json::json!(256)),
        ("u16", serde_json::json!(-1)),
        ("i32", serde_json::json!(2147483648_i64)),
        ("u64", serde_json::json!("123")),
        ("bytes", serde_json::json!([1, 256])),
        ("bytes", serde_json::json!("01 ZZ 02")),
    ] {
        assert!(!valid_memory_value(kind, &value));
    }
    assert!(valid_memory_value("u8", &serde_json::json!(0)));
    assert!(valid_memory_value("bytes", &serde_json::json!("00 ff 0x12")));
}

#[tokio::test]
async fn memory_routes_round_trip_values_and_reject_invalid_writes() {
    let mut app_state = offline_app_state();
    app_state.pid = Some(std::process::id() as i32);
    let app = create_router(ServerState {
        inner: Arc::new(RwLock::new(app_state)),
    });
    let mut buffer = [0u8; 32];
    let address = format!("0x{:x}", buffer.as_mut_ptr() as usize);
    for (kind, value) in [
        ("u8", serde_json::json!(255)),
        ("u16", serde_json::json!(65535)),
        ("u32", serde_json::json!(4294967295_u64)),
        ("u64", serde_json::json!(u64::MAX)),
        ("i32", serde_json::json!(-123)),
        ("i64", serde_json::json!(-123456789012_i64)),
        ("f32", serde_json::json!(1.25)),
    ] {
        let body = serde_json::json!({"address": address, "type": kind, "value": value});
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/mem/write")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "write {kind}");
        let body = serde_json::json!({"address": address, "type": kind});
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/mem/read")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "read {kind}");
        let bytes = axum::body::to_bytes(response.into_body(), 65536).await.unwrap();
        let actual: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(actual["value"], value, "round trip {kind}");
    }
    let previous = buffer;
    let body = serde_json::json!({"address": address, "type": "bytes", "value": "01 ZZ 02"});
    let response = app
        .oneshot(
            Request::post("/api/mem/write")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(buffer, previous);
}

#[tokio::test]
async fn test_contract_and_transfer_routes() {
    let app_state = offline_app_state();
    let app = create_router(ServerState {
        inner: Arc::new(RwLock::new(app_state)),
    });

    // 1. Sign free agent with offline state should fail gracefully (no live session)
    let body = serde_json::json!({ "player_id": 1, "target_club_id": 1 });
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/transfer/sign")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // 1b. Accelerate transfer with offline state should fail gracefully
    let accelerate_body = serde_json::json!({ "player_id": 1, "instant": true });
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/transfer/accelerate")
                .header("content-type", "application/json")
                .body(Body::from(accelerate_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // 2. Terminate contract with offline state should fail gracefully (no live session)
    let body = serde_json::json!({ "player_id": 1 });
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/player/contract/terminate")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // 3. Move player with offline state should fail gracefully (no live session)
    let body = serde_json::json!({ "player_id": 1, "target_club_id": 100 });
    let response = app
        .oneshot(
            Request::post("/api/transfer/move")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
