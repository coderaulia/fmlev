use super::inspector::*;
use super::*;
use crate::models::*;
use crate::session::LiveSession;

    #[test]
    fn explicit_passing_edit_preserves_newer_unedited_fields() {
        let mut manager = GameContextManager::new_offline();
        let previous = manager.get_context().player;
        let mut stale = previous.clone();
        stale.attributes.passing = 19;
        stale.name = "stale browser name".into();
        stale.current_ability = 123;
        stale.contract.wage_per_week = 1;
        manager.update_player_fields(stale, Some(&["attributes.passing".into()])).unwrap();
        let actual = manager.get_context().player;
        assert_eq!(actual.attributes.passing, 19);
        assert_eq!(actual.name, previous.name);
        assert_eq!(actual.current_ability, previous.current_ability);
        assert_eq!(actual.contract, previous.contract);
    }

    #[test]
    fn unsupported_edits_report_exact_fields_before_any_write() {
        let previous = Player::default();
        let mut requested = previous.clone();
        requested.attributes.passing = 20;
        requested.positions.dr = 20;
        assert!(unsupported_live_player_changes(&previous, &requested).is_empty());
        requested.current_ability = 100;
        requested.contract.wage_per_week = 123;
        assert!(unsupported_live_player_changes(&previous, &requested).is_empty());
        // Squad status and the release clause reach the native writer, which validates them itself.
        requested.contract.release_clause = 123;
        requested.contract.squad_status = "Star Player".into();
        assert!(unsupported_live_player_changes(&previous, &requested).is_empty());
        // Happiness is derived by the game and has no native write path.
        requested.contract.happiness = "Delighted".into();
        requested.name = "Another name".into();
        assert_eq!(unsupported_live_player_changes(&previous, &requested), vec!["contract.happiness", "name"]);
    }

    #[test]
    fn unknown_edit_fields_and_identity_edits_are_rejected() {
        let mut manager = GameContextManager::new_offline();
        let previous = manager.get_context().player;
        for field in ["id", "user_modified", "attributes.unknown", "attributes.passing.extra"] {
            assert!(manager.update_player_fields(previous.clone(), Some(&[field.into()])).is_err());
        }
    }

    #[test]
    fn selected_native_read_becomes_the_immediate_delta_save_baseline() {
        let mut manager = GameContextManager::new_offline();
        let mut selected = manager.player_registry[&101].clone();
        selected.positions.wbl = 20; // Stale UI binding before the native selection read.
        manager.player_registry.insert(selected.id, selected.clone());
        selected.positions.wbl = 18; // Verified native selection read.
        selected.position = selected.positions.to_position_string();
        manager.publish_selected_player(&selected);
        assert_eq!(manager.context.player.positions.wbl, 18);
        assert_eq!(manager.player_registry[&selected.id], selected);

        // The user edits immediately, before polling has a chance to update the registry.
        let mut requested = manager.context.player.positions.clone();
        requested.wbl = 20;
        let mut native = selected.positions.clone();
        apply_changed_positions(&manager.player_registry[&selected.id].positions, &requested, &mut native);
        assert_eq!(native.wbl, 20);
    }

    #[test]
    fn native_position_delta_preserves_untouched_values_from_stale_ui_bindings() {
        let mut previous = PlayerPositions::default();
        previous.wbl = 20;
        previous.dl = 1;
        let mut requested = previous.clone();
        requested.dl = 15;
        requested.wbr = 14;
        requested.left_foot = 15;
        let mut native = previous.clone();
        native.wbl = 18; // The old UI binding cache incorrectly displayed 20.
        apply_changed_positions(&previous, &requested, &mut native);
        assert_eq!((native.dl, native.wbr, native.left_foot), (15, 14, 15));
        assert_eq!(native.wbl, 18);
        assert_eq!(native.right_foot, previous.right_foot);
    }

    #[test]
    fn native_edit_gate_allows_only_positions_and_derived_label() {
        let previous = build_template_live_player(65304, "Zulkifli Lukmansyah".into(), "D (R)".into());
        let mut requested = previous.clone();
        requested.positions.dl = 15;
        requested.positions.wbr = 14;
        requested.positions.left_foot = 15;
        requested.position = "D (RL)".into();
        requested.user_modified = true;
        assert!(is_positions_only_edit(&previous, &requested));
        let mut json = serde_json::to_value(&requested).unwrap();
        for (field, value) in [
            ("id", serde_json::json!(38141)),
            ("name", serde_json::json!("Another player")),
            ("current_ability", serde_json::json!(199)),
            ("potential_ability", serde_json::json!(200)),
            ("condition", serde_json::json!(10000)),
            ("age", serde_json::json!(18)),
            ("club_id", serde_json::json!(7001)),
            ("is_injured", serde_json::json!(true)),
            ("attributes", serde_json::json!({"finishing":20})),
            ("contract", serde_json::json!({"wage_per_week":75000})),
        ] {
            let original = json[field].clone();
            json[field] = value;
            let unsupported: Player = serde_json::from_value(json.clone()).unwrap();
            assert!(!is_positions_only_edit(&previous, &unsupported), "field {field}");
            json[field] = original;
        }
    }

    #[test]
    fn footedness_read_skips_removed_slots_and_out_of_range_ratings() {
        let driver = fmlev_mem::LinuxMemoryDriver::new(std::process::id() as i32);
        let mut left = [0u8; 40];
        let mut right = [0u8; 40];
        left[0x18..0x20].copy_from_slice(&264i64.to_ne_bytes());
        right[0x18..0x20].copy_from_slice(&20i64.to_ne_bytes());
        let mut entries = [0u8; 0x20 + 2 * 24];
        entries[0x28..0x2c].copy_from_slice(&0x32342020u32.to_ne_bytes());
        entries[0x30..0x38].copy_from_slice(&(left.as_ptr() as usize).to_ne_bytes());
        entries[0x38..0x3c].copy_from_slice(&(-1i32).to_ne_bytes());
        entries[0x40..0x44].copy_from_slice(&0x32332020u32.to_ne_bytes());
        entries[0x48..0x50].copy_from_slice(&(right.as_ptr() as usize).to_ne_bytes());
        let mut dynamic_ref = [0u8; 40];
        dynamic_ref[0x18..0x20].copy_from_slice(&(entries.as_ptr() as usize).to_ne_bytes());
        dynamic_ref[0x20..0x24].copy_from_slice(&2i32.to_ne_bytes());
        let mut typed_value = [0u8; 40];
        typed_value[0x18..0x20].copy_from_slice(&(dynamic_ref.as_ptr() as usize).to_ne_bytes());
        let mut positions = PlayerPositions::unknown();
        extract_footedness(&driver, typed_value.as_ptr() as usize, &mut positions);
        assert_eq!(positions.left_foot, 0);
        assert_eq!(positions.right_foot, 0);
        left[0x18..0x20].copy_from_slice(&8i64.to_ne_bytes());
        entries[0x38..0x3c].copy_from_slice(&0i32.to_ne_bytes());
        extract_footedness(&driver, typed_value.as_ptr() as usize, &mut positions);
        assert_eq!(positions.left_foot, 8);
        assert_eq!(positions.right_foot, 20);
    }

    #[test]
    fn offline_context_does_not_attach_during_polling() {
        let mut manager = GameContextManager::new_offline();
        manager.sync_live_context().unwrap();
        assert!(manager.session.is_none());
        assert!(!manager.context.attached);
    }

    #[test]
    fn unsupported_live_updates_leave_every_entity_unchanged() {
        let session = LiveSession::with_bases(
            std::process::id() as i32,
            fmlev_mem::ModuleBases { game_assembly_base: Some(1), ..Default::default() },
            OffsetsRegistry::default(),
        );
        let mut manager = GameContextManager::with_session(session);
        let before = manager.get_context();
        let mut player = before.player.clone();
        player.positions.dl = 15;
        assert!(manager.update_player(player).is_err());
        assert!(manager.update_club(before.club.clone()).is_err());
        assert!(manager.update_nation(before.nation.clone()).is_err());
        assert!(manager.update_competition(before.competition.clone()).is_err());
        for action in ["heal_player", "all_positions", "both_feet", "max_facilities", "golden_generation", "relax_rules"] {
            assert!(manager.apply_quick_action(action).is_err());
        }
        assert_eq!(manager.get_context(), before);
        assert!(manager.user_modified_players.is_empty());
        assert!(manager.user_modified_clubs.is_empty());
    }

    #[test]
    fn failed_record_write_does_not_poison_cache_or_modified_flags() {
        let session = LiveSession::new(std::process::id() as i32, OffsetsRegistry::default()).unwrap();
        let mut manager = GameContextManager::with_session(session);
        manager.bind_player_address(101, 1);
        let before = manager.get_context();
        let mut changed = before.player.clone();
        changed.condition = 123;
        assert!(manager.update_player(changed).is_err());
        assert_eq!(manager.get_context(), before);
        assert!(manager.user_modified_players.is_empty());
        assert!(manager.apply_quick_action("heal_player").is_err());
        assert_eq!(manager.get_context(), before);
        assert!(manager.user_modified_players.is_empty());
    }

    #[test]
    fn update_uses_requested_entity_binding_and_requires_one() {
        let session = LiveSession::new(std::process::id() as i32, OffsetsRegistry::default()).unwrap();
        let mut manager = GameContextManager::with_session(session);
        let mut active = [0u8; 128];
        let mut target = [0u8; 128];
        manager.bind_player_address(101, active.as_mut_ptr() as usize);
        let mut player = manager.player_registry[&102].clone();
        player.condition = 4321;
        assert!(manager.update_player(player.clone()).is_err());
        assert!(active.iter().all(|v| *v == 0));
        manager.bind_player_address(102, target.as_mut_ptr() as usize);
        manager.update_player(player).unwrap();
        assert_eq!(u16::from_ne_bytes(target[0x10..0x12].try_into().unwrap()), 4321);
        assert!(active.iter().all(|v| *v == 0));
        manager.select_player(103).unwrap();
        assert_eq!(manager.active_player_addr, None);
    }

    #[test]
    fn attaching_real_session_removes_demo_entities_and_values() {
        let session = LiveSession::with_bases(-1, fmlev_mem::ModuleBases::default(), OffsetsRegistry::default());
        let manager = GameContextManager::with_session(session);
        assert!(manager.player_registry.is_empty());
        assert!(manager.club_registry.is_empty());
        assert!(manager.nation_registry.is_empty());
        assert!(manager.competition_registry.is_empty());
        assert_eq!(manager.context.player.id, 0);
        assert_eq!(manager.context.club.club_id, 0);
        assert_eq!(manager.context.nation.id, 0);
        assert_eq!(manager.context.competition.id, 0);
        assert_eq!(manager.context.player.condition, 0);
        assert_eq!(manager.context.player.contract.wage_per_week, 0);
        assert_eq!(manager.context.player.positions, PlayerPositions::unknown());
    }

    #[test]
    fn live_position_label_does_not_fabricate_numeric_ratings() {
        let player = build_template_live_player(65304, "Zulkifli Lukmansyah".into(), "D (R)".into());
        assert_eq!(player.position, "D (R)");
        assert_eq!(player.positions, PlayerPositions::unknown());
        assert_eq!(player.condition, 0);
        assert_eq!(player.sharpness, 0);
        assert_eq!(player.morale, 0);
        assert!(player.morale_label.is_empty());
        assert_eq!(player.contract.wage_per_week, 0);
        assert_eq!(player.contract.expiry_year, 0);
        assert!(player.contract.squad_status.is_empty());
        assert!(player.contract.happiness.is_empty());
    }

    #[test]
    fn test_context_manager_offline_init() {
        let manager = GameContextManager::new_offline();
        let ctx = manager.get_context();

        assert!(!ctx.attached);
        assert_eq!(ctx.active_screen, "player");
        assert_eq!(ctx.sync_mode, "auto_follow");
        assert_eq!(ctx.player.name, "Erling Haaland");
        assert_eq!(ctx.club.club_name, "Manchester City");
        assert_eq!(manager.all_players().len(), 11);
        assert_eq!(manager.all_clubs().len(), 9);
    }

    #[test]
    fn test_context_manager_search() {
        let manager = GameContextManager::new_offline();

        let res_haaland = manager.search("Haaland");
        assert_eq!(res_haaland.players.len(), 1);
        assert_eq!(res_haaland.players[0].name, "Erling Haaland");

        let res_norway = manager.search("Norway");
        assert_eq!(res_norway.players.len(), 2); // Haaland + Ødegaard

        let res_real = manager.search("Real Madrid");
        assert_eq!(res_real.clubs.len(), 1);
        assert_eq!(res_real.clubs[0].club_name, "Real Madrid");
        assert_eq!(res_real.players.len(), 3); // Mbappé, Bellingham, Courtois

        let res_empty = manager.search("NonExistentNameXYZ");
        assert!(res_empty.is_empty());
        assert_eq!(res_empty.total_count(), 0);
    }

    #[test]
    fn test_context_manager_select_player_and_club() {
        let mut manager = GameContextManager::new_offline();

        let mbappe = manager.select_player(102).unwrap();
        assert_eq!(mbappe.name, "Kylian Mbappé");
        assert_eq!(manager.context().player.name, "Kylian Mbappé");
        assert_eq!(manager.context().club.club_name, "Real Madrid");
        assert_eq!(manager.context().active_screen, "player");

        let bayern = manager.select_club(1006).unwrap();
        assert_eq!(bayern.club_name, "Bayern München");
        assert_eq!(manager.context().club.club_name, "Bayern München");
        assert_eq!(manager.context().active_screen, "club");

        let err = manager.select_player(99999);
        assert!(err.is_err());
    }

    #[test]
    fn test_context_manager_update_player_and_club() {
        let mut manager = GameContextManager::new_offline();

        let mut player = manager.context().player.clone();
        player.current_ability = 199;
        player.attributes.finishing = 20;
        player.contract.wage_per_week = 550_000;

        manager.update_player(player).unwrap();
        assert_eq!(manager.context().player.current_ability, 199);
        assert_eq!(manager.context().player.attributes.finishing, 20);
        assert_eq!(manager.context().player.contract.wage_per_week, 550_000);

        let mut club = manager.context().club.clone();
        club.balance = 500_000_000;
        club.transfer_budget = 300_000_000;

        manager.update_club(club).unwrap();
        assert_eq!(manager.context().club.balance, 500_000_000);
        assert_eq!(manager.context().club.transfer_budget, 300_000_000);
    }

    #[test]
    fn test_context_manager_quick_actions() {
        let mut manager = GameContextManager::new_offline();

        // 1. Heal Player
        manager.context.player.condition = 4500;
        manager.context.player.is_injured = true;
        manager.apply_quick_action("heal_player").unwrap();
        assert_eq!(manager.context().player.condition, 10000);
        assert_eq!(manager.context().player.sharpness, 10000);
        assert!(!manager.context().player.is_injured);

        // 2. Max Morale
        manager.context.player.morale = 30;
        manager.apply_quick_action("max_morale").unwrap();
        assert_eq!(manager.context().player.morale, 100);
        assert_eq!(manager.context().player.morale_label, "Superb");
        assert_eq!(manager.context().player.contract.happiness, "Delighted");

        // 3. Wonderkid
        manager.context.player.age = 24;
        manager.context.player.potential_ability = 150;
        manager.apply_quick_action("wonderkid").unwrap();
        assert_eq!(manager.context().player.potential_ability, 200);
        assert!(manager.context().player.current_ability >= 165);
        assert_eq!(manager.context().player.age, 18);
        assert_eq!(manager.context().player.attributes.ambition, 20);

        // 4. Max Attributes
        manager.apply_quick_action("max_attributes").unwrap();
        assert_eq!(manager.context().player.current_ability, 200);
        assert_eq!(manager.context().player.potential_ability, 200);
        assert_eq!(manager.context().player.attributes.finishing, 20);
        assert_eq!(manager.context().player.attributes.passing, 20);
        assert_eq!(manager.context().player.attributes.pace, 20);
        assert_eq!(manager.context().player.attributes.consistency, 20);

        // 5. Extend Contract
        let orig_year = manager.context().player.contract.expiry_year;
        manager.apply_quick_action("extend_contract").unwrap();
        assert_eq!(manager.context().player.contract.expiry_year, orig_year + 5);
        assert_eq!(manager.context().player.contract.squad_status, "Star Player");

        // 6. Boost Budget
        let orig_bal = manager.context().club.balance;
        manager.apply_quick_action("boost_budget_50m").unwrap();
        assert_eq!(manager.context().club.balance, orig_bal + 50_000_000);

        // 7. Sugar Daddy
        manager.apply_quick_action("sugar_daddy").unwrap();
        assert_eq!(manager.context().club.balance, 1_000_000_000);
        assert_eq!(manager.context().club.transfer_budget, 500_000_000);
        assert_eq!(manager.context().club.reputation, 10000);

        // 8. Wipe Debt
        manager.context.club.balance = -10_000_000;
        manager.apply_quick_action("wipe_debt").unwrap();
        assert!(manager.context().club.balance >= 150_000_000);

        // 9. Max Facilities
        manager.apply_quick_action("max_facilities").unwrap();
        assert_eq!(manager.context().club.training_facilities, 20);
        assert_eq!(manager.context().club.youth_facilities, 20);
        assert_eq!(manager.context().club.youth_recruitment, 20);
        assert_eq!(manager.context().club.junior_coaching, 20);

        // 10. Heal Squad
        manager.apply_quick_action("heal_squad").unwrap();
        assert_eq!(manager.context().player.condition, 10000);
    }

    #[test]
    fn test_context_manager_memory_bound_write() {
        let my_pid = std::process::id() as i32;
        let session = LiveSession::new(my_pid, OffsetsRegistry::default()).unwrap();
        let mut manager = GameContextManager::with_session(session);

        // Create fake player memory buffer
        let mut fake_player_memory = vec![0u8; 128];
        let fake_addr = fake_player_memory.as_mut_ptr() as usize;

        manager.bind_player_address(101, fake_addr);
        assert_eq!(manager.get_player_address(101), Some(fake_addr));

        // Quick action writes directly to memory
        manager.apply_quick_action("heal_player").unwrap();

        let condition = u16::from_ne_bytes(fake_player_memory[0x10..0x12].try_into().unwrap());
        let sharpness = u16::from_ne_bytes(fake_player_memory[0x12..0x14].try_into().unwrap());
        assert_eq!(condition, 10000);
        assert_eq!(sharpness, 10000);
    }

    #[test]
    fn test_context_manager_serialization_and_backward_compatibility() {
        let manager = GameContextManager::new_offline();
        let ctx = manager.get_context();

        // Roundtrip serialization
        let json_str = serde_json::to_string(&ctx).unwrap();
        let deserialized: ActiveContext = serde_json::from_str(&json_str).unwrap();
        assert_eq!(ctx.player.name, deserialized.player.name);
        assert_eq!(ctx.player.contract.wage_per_week, deserialized.player.contract.wage_per_week);
        assert_eq!(ctx.club.club_name, deserialized.club.club_name);

        // Backward compatibility: old Player JSON without the enriched fields
        let legacy_player_json = r#"{
            "id": 42,
            "name": "Legacy Legend",
            "club_id": 999,
            "current_ability": 150,
            "potential_ability": 170,
            "condition": 9500,
            "sharpness": 9600,
            "morale": 80,
            "attributes": {
                "finishing": 14,
                "passing": 15
            }
        }"#;

        let player: Player = serde_json::from_str(legacy_player_json).unwrap();
        assert_eq!(player.id, 42);
        assert_eq!(player.name, "Legacy Legend");
        assert_eq!(player.age, 0); // default
        assert_eq!(player.contract.wage_per_week, 50_000); // default contract
        assert_eq!(player.attributes.consistency, 0); // default hidden attribute
    }

    #[test]
    fn test_context_manager_sync_mode_and_errors() {
        let mut manager = GameContextManager::new_offline();

        manager.set_sync_mode("pinned");
        assert_eq!(manager.context().sync_mode, "pinned");

        manager.set_active_screen("squad");
        assert_eq!(manager.context().active_screen, "squad");

        // Invalid quick action
        let bad_action = manager.apply_quick_action("non_existent_cheat_code");
        assert!(bad_action.is_err());

        // Invalid club selection
        let bad_club = manager.select_club(999999);
        assert!(bad_club.is_err());
    }

    #[test]
    fn test_context_manager_nation_competition_and_affiliates() {
        let mut manager = GameContextManager::new_offline();

        // 1. Test Nation selection & golden generation
        let nation = manager.select_nation(759).expect("Should select Brazil");
        assert_eq!(nation.name, "Brazil");
        assert_eq!(nation.youth_rating, 165);

        manager.apply_quick_action("golden_generation").unwrap();
        assert_eq!(manager.context().nation.youth_rating, 200);
        assert_eq!(manager.context().nation.youth_facilities_avg, 20);

        // 2. Test Competition selection & relax rules
        let comp = manager.select_competition(11).expect("Should select EPL");
        assert_eq!(comp.name, "Premier League");
        assert_eq!(comp.max_foreign_players, 17);

        manager.apply_quick_action("relax_rules").unwrap();
        assert_eq!(manager.context().competition.max_foreign_players, 99);
        assert_eq!(manager.context().competition.max_bench_size, 12);

        // 3. Test Club facilities, board, and affiliates
        let club = manager.select_club(1001).expect("Should select Man City");
        assert_eq!(club.sugar_daddy_type, "Foreground");
        assert_eq!(club.affiliates.len(), 2);

        let new_affiliate = ClubAffiliate {
            affiliate_club_id: 3001,
            affiliate_name: "Palermo FC".to_string(),
            is_parent: false,
            first_option_to_buy: true,
            send_players_on_loan: true,
            shared_scouting: true,
            annual_fee: 300_000,
            work_permit_haven: true,
        };
        let updated_club = manager.add_club_affiliate(1001, new_affiliate).unwrap();
        assert_eq!(updated_club.affiliates.len(), 3);

        let removed_club = manager.remove_club_affiliate(1001, 3001).unwrap();
        assert_eq!(removed_club.affiliates.len(), 2);

        // 4. Test Search across 4 categories
        let search_res = manager.search("Brazil");
        assert!(!search_res.nations.is_empty());
        assert_eq!(search_res.nations[0].name, "Brazil");

        let search_comp = manager.search("Champions League");
        assert!(!search_comp.competitions.is_empty());
        assert_eq!(search_comp.competitions[0].short_name, "UCL");
    }

    #[test]
    fn test_zulkifli_lukmansyah_persib_and_position() {
        let mut manager = GameContextManager::new_offline();
        let player = manager.select_player(73).expect("Player 73 should exist");
        assert_eq!(player.name, "Zulkifli Lukmansyah");
        assert_eq!(player.nationality, "Indonesia");
        assert_eq!(player.club_name, "Persib");
        assert_eq!(player.club_id, 7001);
        assert_eq!(player.position, "D (R)");
        assert_eq!(player.positions.dr, 20);
        assert_eq!(player.positions.dl, 1);
        assert_eq!(player.positions.dc, 1);

        // Check active context club
        assert_eq!(manager.context().club.club_id, 7001);
        assert_eq!(manager.context().club.club_name, "Persib");
    }

    #[test]
    fn test_user_edits_persist_across_sync_live_context() {
        let mut manager = GameContextManager::new_offline();
        manager.set_sync_mode("pinned");

        // 1. Select player 73 (Persib)
        let mut player = manager.select_player(73).unwrap();
        assert_eq!(player.club_name, "Persib");
        assert_eq!(manager.context().club.club_name, "Persib");

        // 2. Perform user modifications
        player.attributes.finishing = 20;
        player.attributes.passing = 20;
        player.attributes.tackling = 19;
        player.current_ability = 190;
        player.potential_ability = 199;
        player.condition = 9999;
        player.sharpness = 9999;
        player.contract.wage_per_week = 75_000;

        manager.update_player(player).unwrap();

        assert!(manager.context().player.user_modified);
        assert!(manager.user_modified_players.contains(&73));
        assert_eq!(manager.context().player.attributes.finishing, 20);
        assert_eq!(manager.context().player.attributes.passing, 20);
        assert_eq!(manager.context().player.attributes.tackling, 19);

        // 3. Run sync_live_context multiple times (simulating UI 1s polling)
        for _ in 0..5 {
            let res = manager.sync_live_context();
            assert!(res.is_ok());

            // Assert that modifications remain intact and do NOT revert
            let current = manager.context().player.clone();
            assert_eq!(current.id, 73);
            assert_eq!(current.attributes.finishing, 20);
            assert_eq!(current.attributes.passing, 20);
            assert_eq!(current.attributes.tackling, 19);
            assert_eq!(current.current_ability, 190);
            assert_eq!(current.potential_ability, 199);
            assert_eq!(current.condition, 9999);
            assert_eq!(current.sharpness, 9999);
            assert_eq!(current.contract.wage_per_week, 75_000);
            assert_eq!(current.club_name, "Persib");
            assert_eq!(manager.context().club.club_name, "Persib");
            assert!(current.user_modified);
        }
    }

    #[test]
    fn test_quick_action_marks_user_modified_and_persists() {
        let mut manager = GameContextManager::new_offline();
        manager.set_sync_mode("pinned");
        manager.select_player(101).unwrap();

        manager.apply_quick_action("max_attributes").unwrap();
        assert!(manager.context().player.user_modified);
        assert!(manager.user_modified_players.contains(&101));
        assert_eq!(manager.context().player.attributes.finishing, 20);

        for _ in 0..5 {
            manager.sync_live_context().unwrap();
            assert_eq!(manager.context().player.attributes.finishing, 20);
            assert!(manager.context().player.user_modified);
        }
    }

    #[test]
    fn test_build_template_live_player_has_no_synthetic_defaults() {
        let p = build_template_live_player(38141, "Layvin Kurzawa".to_string(), "D/WB (L)".to_string());
        assert_eq!(p.id, 38141);
        assert_eq!(p.name, "Layvin Kurzawa");
        assert_eq!(p.position, "D/WB (L)");
        assert_eq!(p.club_id, 0);
        assert_eq!(p.club_name, "");
        assert_eq!(p.nationality, "");
        assert_eq!(p.age, 0);
        assert_eq!(p.current_ability, 0);
        assert_eq!(p.potential_ability, 0);
        assert_eq!(p.attributes, PlayerAttributes::default());
    }

    #[test]
    fn test_merge_player_attributes_and_apply_attribute_by_name() {
        let mut dst = PlayerAttributes::default();
        dst.finishing = 15;
        dst.vision = 10;

        let mut src = PlayerAttributes::default();
        apply_attribute_by_name(&mut src, "Vision", 12);
        apply_attribute_by_name(&mut src, "Natural Fitness", 10);
        apply_attribute_by_name(&mut src, "Free Kick Taking", 13);
        apply_attribute_by_name(&mut src, "Off The Ball", 15);

        merge_player_attributes(&mut dst, &src);
        assert_eq!(dst.finishing, 15); // preserved because src.finishing == 0
        assert_eq!(dst.vision, 12); // updated from src
        assert_eq!(dst.natural_fitness, 10);
        assert_eq!(dst.free_kicks, 13);
        assert_eq!(dst.off_the_ball, 15);
    }

    #[test]
    fn test_read_typed_value_and_extract_binding_number() {
        let my_pid = std::process::id() as i32;
        let driver = fmlev_mem::LinuxMemoryDriver::new(my_pid);

        // Simulate NumericTypedValue in memory: +0x18 = union (8 bytes), +0x20 = NumberKind (i32)
        let mut fake_tv = [0u8; 40];
        let tv_addr = fake_tv.as_mut_ptr() as usize;

        // 1. SignedWhole = 85010869 (UniqueId > 0x10000)
        fake_tv[0x18..0x20].copy_from_slice(&85010869i64.to_ne_bytes());
        fake_tv[0x20..0x24].copy_from_slice(&0i32.to_ne_bytes());
        assert_eq!(extract_binding_number(&driver, tv_addr), Some(85010869));

        // 2. SignedWhole = 32 (Age)
        fake_tv[0x18..0x20].copy_from_slice(&32i64.to_ne_bytes());
        fake_tv[0x20..0x24].copy_from_slice(&0i32.to_ne_bytes());
        assert_eq!(extract_binding_number(&driver, tv_addr), Some(32));

        // 3. FloatingPoint = 14.0
        fake_tv[0x18..0x20].copy_from_slice(&14.0f64.to_ne_bytes());
        fake_tv[0x20..0x24].copy_from_slice(&2i32.to_ne_bytes());
        assert_eq!(extract_binding_number(&driver, tv_addr), Some(14));
    }

    #[test]
    fn failed_native_discovery_is_not_repeated_on_every_poll() {
        // Without a session discovery fails; the failure must set a retry time that later calls respect.
        let mut manager = GameContextManager::new_offline();
        manager.discover_native_player_throttled(110396);
        let first = *manager.native_discovery_retry_at.get(&110396).expect("failure records a retry time");
        assert!(first > std::time::Instant::now());
        manager.discover_native_player_throttled(110396);
        assert_eq!(manager.native_discovery_retry_at.get(&110396), Some(&first), "no new attempt inside the window");
        manager.native_discovery_retry_at.insert(110396, std::time::Instant::now());
        manager.discover_native_player_throttled(110396);
        assert!(manager.native_discovery_retry_at[&110396] > first, "a due retry runs again and pushes the time");
    }
