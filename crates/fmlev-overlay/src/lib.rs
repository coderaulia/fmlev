pub mod command;
pub mod error;
pub mod launcher;
pub mod state;
pub mod terminal_hud;

pub use command::{CommandResult, HudCommand, HudController};
pub use error::{OverlayError, Result};
pub use launcher::{BrowserCandidate, OverlayLauncher};
pub use state::{
    HudNotification, NotificationLevel, OverlayDisplayMode, OverlayState, OverlayTab,
};
pub use terminal_hud::{TerminalHud, TerminalHudController};

pub struct OverlayRenderer {
    pub state: OverlayState,
}

impl Default for OverlayRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl OverlayRenderer {
    pub fn new() -> Self {
        Self {
            state: OverlayState::new(),
        }
    }

    pub fn toggle_visibility(&mut self) {
        self.state.toggle_visibility();
    }

    pub fn set_active_tab(&mut self, tab: OverlayTab) {
        self.state.set_active_tab(tab);
    }

    pub fn update_connection(&mut self, pid: i32, game_assembly_base: usize) {
        self.state.update_connection(pid, Some(game_assembly_base), None, None);
    }

    pub fn render_terminal(&self) -> String {
        TerminalHud::render(&self.state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlay_state_and_mode_toggle() {
        let mut state = OverlayState::new();
        assert_eq!(state.mode, OverlayDisplayMode::CompactOverlay);
        state.toggle_mode();
        assert_eq!(state.mode, OverlayDisplayMode::Full);
        state.toggle_mode();
        assert_eq!(state.mode, OverlayDisplayMode::CompactOverlay);

        state.update_connection(1234, Some(0x7fff1000), Some(0x7fff5000), Some("/path/to/pfx".into()));
        assert_eq!(state.target_pid, Some(1234));
        assert_eq!(state.game_assembly_base, Some(0x7fff1000));
        assert!(!state.notifications.is_empty());

        let hud_output = TerminalHud::render(&state);
        assert!(hud_output.contains("1234"));
        assert!(hud_output.contains("0x7fff1000"));
    }

    #[test]
    fn test_hud_controller_commands() {
        let mut controller = HudController::new();
        let res = controller.handle_command(HudCommand::HealSquad {
            player_ptr: 0x18218e590,
            condition_offset: 0x20,
        });
        assert!(res.success);
        assert!(res.message.contains("0x18218e590"));

        let res2 = controller.handle_command(HudCommand::SwitchTab {
            tab: OverlayTab::ClubFinances,
        });
        assert!(res2.success);
        assert_eq!(controller.state.active_tab, OverlayTab::ClubFinances);
    }

    #[test]
    fn test_nation_quick_actions() {
        let mut controller = TerminalHudController::new();
        controller.handle_command(HudCommand::SelectNation {
            nation_id: 765,
            nation_ptr: Some(0x183000000),
        });
        assert_eq!(controller.state.selected_nation_id, Some(765));

        // Test golden_generation
        let res = controller.handle_command(HudCommand::GoldenGeneration {
            nation_ptr: None,
            nation_id: None,
        });
        assert!(res.success);
        let active_nat = controller.state.active_nation.as_ref().unwrap();
        assert_eq!(active_nat.youth_rating, 200);
        assert_eq!(active_nat.youth_facilities_avg, 20);
        assert_eq!(active_nat.fa_financial_power, 20);

        // Test max_youth_rating
        let res2 = controller.handle_command(HudCommand::MaxYouthRating {
            nation_ptr: None,
            nation_id: None,
        });
        assert!(res2.success);
        assert_eq!(controller.state.active_nation.as_ref().unwrap().youth_rating, 200);
    }

    #[test]
    fn test_competition_quick_actions() {
        let mut controller = TerminalHudController::new();
        controller.handle_command(HudCommand::SelectCompetition {
            competition_id: 11,
            competition_ptr: Some(0x184000000),
        });
        assert_eq!(controller.state.selected_competition_id, Some(11));

        // Test relax_rules
        let res = controller.handle_command(HudCommand::RelaxRules {
            comp_ptr: None,
            comp_id: None,
        });
        assert!(res.success);
        let active_comp = controller.state.active_competition.as_ref().unwrap();
        assert_eq!(active_comp.max_foreign_players, 99);
        assert_eq!(active_comp.max_squad_size, 50);
        assert_eq!(active_comp.min_homegrown_club, 0);
        assert_eq!(active_comp.min_homegrown_nation, 0);
        assert_eq!(active_comp.max_bench_size, 12);

        // Test five_subs
        let res2 = controller.handle_command(HudCommand::FiveSubs {
            comp_ptr: None,
            comp_id: None,
        });
        assert!(res2.success);
        let comp2 = controller.state.active_competition.as_ref().unwrap();
        assert_eq!(comp2.max_substitutes, 5);
        assert_eq!(comp2.substitution_windows, 3);
        assert!(comp2.max_bench_size >= 9);
    }

    #[test]
    fn test_club_facilities_and_board_commands() {
        let mut controller = TerminalHudController::new();
        controller.handle_command(HudCommand::SelectClub {
            club_id: 102,
            club_ptr: Some(0x185000000),
        });
        assert_eq!(controller.state.selected_club_id, Some(102));

        // Test max_facilities
        let res = controller.handle_command(HudCommand::MaxFacilities {
            club_ptr: None,
            club_id: None,
        });
        assert!(res.success);
        let active_club = controller.state.active_club.as_ref().unwrap();
        assert_eq!(active_club.training_facilities, 20);
        assert_eq!(active_club.youth_facilities, 20);
        assert_eq!(active_club.youth_recruitment, 20);
        assert_eq!(active_club.junior_coaching, 20);
        assert_eq!(active_club.data_analysis_facilities, 20);
        assert_eq!(active_club.corporate_facilities, 20);

        // Test foreground_sugar_daddy
        let res2 = controller.handle_command(HudCommand::ForegroundSugarDaddy {
            club_ptr: None,
            club_id: None,
        });
        assert!(res2.success);
        let club2 = controller.state.active_club.as_ref().unwrap();
        assert_eq!(club2.sugar_daddy_type, "Foreground");
        assert!(club2.balance >= 500_000_000);
        assert!(club2.transfer_budget >= 250_000_000);
        assert_eq!(club2.board_patience, 20);
        assert_eq!(club2.transfer_retention_pct, 100);

        // Test add_affiliate
        let res3 = controller.handle_command(HudCommand::AddClubAffiliate {
            club_ptr: None,
            affiliate_id: 1032,
            affiliate_name: "Girona FC".to_string(),
            is_parent: false,
        });
        assert!(res3.success);
        assert_eq!(controller.state.active_affiliates.len(), 1);
        assert_eq!(controller.state.active_affiliates[0].affiliate_name, "Girona FC");
    }

    #[test]
    fn test_terminal_hud_controller_commands_parsing() {
        let mut controller = TerminalHudController::new();

        // 1. Navigation string commands
        let res_nat = controller.execute_command("nation");
        assert!(res_nat.success);
        assert_eq!(controller.state.active_tab, OverlayTab::Nation);

        let res_comp = controller.execute_command("competition");
        assert!(res_comp.success);
        assert_eq!(controller.state.active_tab, OverlayTab::Competition);

        let res_aff = controller.execute_command("affiliates");
        assert!(res_aff.success);
        assert_eq!(controller.state.active_tab, OverlayTab::Affiliates);

        // 2. God-mode macros string commands
        let res_gold = controller.execute_command("golden_generation");
        assert!(res_gold.success);
        assert_eq!(controller.state.active_nation.as_ref().unwrap().youth_rating, 200);

        let res_youth = controller.execute_command("max_youth_rating");
        assert!(res_youth.success);

        let res_relax = controller.execute_command("relax_rules");
        assert!(res_relax.success);
        assert_eq!(controller.state.active_competition.as_ref().unwrap().max_foreign_players, 99);

        let res_subs = controller.execute_command("five_subs");
        assert!(res_subs.success);
        assert_eq!(controller.state.active_competition.as_ref().unwrap().max_substitutes, 5);

        let res_fac = controller.execute_command("max_facilities");
        assert!(res_fac.success);
        assert_eq!(controller.state.active_club.as_ref().unwrap().training_facilities, 20);

        let res_fsd = controller.execute_command("foreground_sugar_daddy");
        assert!(res_fsd.success);
        assert_eq!(controller.state.active_club.as_ref().unwrap().sugar_daddy_type, "Foreground");

        // 3. Affiliate add string command
        let res_add_aff = controller.execute_command("affiliates add 4120 Lommel SK");
        assert!(res_add_aff.success);
        assert_eq!(controller.state.active_affiliates.last().unwrap().affiliate_name, "Lommel SK");
    }

    #[test]
    fn test_terminal_hud_render_all_tabs() {
        let mut state = OverlayState::new();

        // 1. Rebranded banner check
        let banner_out = TerminalHud::render(&state);
        assert!(banner_out.contains("FMLE by Vanaila - Football Manager 26 Live HUD"));

        // 2. Render Player tab (Decoupled from club finances)
        state.set_active_tab(OverlayTab::PlayerEditor);
        let out_player = TerminalHud::render(&state);
        assert!(out_player.contains("Player Details & Attributes"));
        assert!(out_player.contains("Team:"));
        assert!(out_player.contains("Manchester City"));
        assert!(out_player.contains("Finishing:"));
        assert!(out_player.contains("CA:"));
        // Ensure no club finances in player tab!
        assert!(!out_player.contains("Balance: $"));
        assert!(!out_player.contains("Transfer Budget:"));

        // 3. Render Positions tab
        state.set_active_tab(OverlayTab::Positions);
        let out_pos = TerminalHud::render(&state);
        assert!(out_pos.contains("Player Positional Versatility & Footedness"));
        assert!(out_pos.contains("Positions: ST:"));
        assert!(out_pos.contains("Feet: Left Foot:"));
        assert!(out_pos.contains("all_positions_20"));
        assert!(out_pos.contains("both_feet_20"));

        // 4. Render Club tab (Dedicated facilities & ownership)
        state.set_active_tab(OverlayTab::ClubFinances);
        let out_club = TerminalHud::render(&state);
        assert!(out_club.contains("Club Facilities & Ownership"));
        assert!(out_club.contains("Balance: $"));
        assert!(out_club.contains("max_facilities"));
        assert!(out_club.contains("foreground_sugar_daddy"));

        // 5. Render Nation tab
        state.set_active_tab(OverlayTab::Nation);
        let out_nat = TerminalHud::render(&state);
        assert!(out_nat.contains("Nation Editor"));
        assert!(out_nat.contains("golden_generation"));
        assert!(out_nat.contains("max_youth_rating"));

        // 6. Render Competition tab
        state.set_active_tab(OverlayTab::Competition);
        let out_comp = TerminalHud::render(&state);
        assert!(out_comp.contains("Competition & Match Rules"));
        assert!(out_comp.contains("relax_rules"));
        assert!(out_comp.contains("five_subs"));

        // 7. Render Affiliates tab
        state.set_active_tab(OverlayTab::Affiliates);
        let out_aff = TerminalHud::render(&state);
        assert!(out_aff.contains("Club Affiliate Network"));

        // 8. Render GodMode tab
        state.set_active_tab(OverlayTab::GodMode);
        let out_god = TerminalHud::render(&state);
        assert!(out_god.contains("Heal Squad"));
        assert!(out_god.contains("Golden Generation"));
        assert!(out_god.contains("Relax Competition Rules"));
    }

    #[test]
    fn test_player_and_position_macros() {
        let mut controller = TerminalHudController::new();

        // Switch to player tab and select player
        let res_p = controller.execute_command("player 8000");
        assert!(res_p.success);
        assert_eq!(controller.state.active_tab, OverlayTab::PlayerEditor);
        assert_eq!(controller.state.selected_player_id, Some(8000));

        // Switch to positions tab
        let res_pos = controller.execute_command("positions");
        assert!(res_pos.success);
        assert_eq!(controller.state.active_tab, OverlayTab::Positions);

        // Test all_positions_20 macro
        let res_all = controller.execute_command("all_positions_20");
        assert!(res_all.success);
        let p = controller.state.active_player.as_ref().unwrap();
        assert_eq!(p.positions.st, 20);
        assert_eq!(p.positions.dc, 20);
        assert_eq!(p.positions.left_foot, 20);
        assert_eq!(p.positions.right_foot, 20);

        // Test pure_striker macro
        let res_striker = controller.execute_command("pure_striker");
        assert!(res_striker.success);
        let p2 = controller.state.active_player.as_ref().unwrap();
        assert_eq!(p2.positions.st, 20);
        assert_eq!(p2.positions.aml, 14);

        // Test both_feet_20 macro
        let res_feet = controller.execute_command("both_feet_20");
        assert!(res_feet.success);
        let p3 = controller.state.active_player.as_ref().unwrap();
        assert_eq!(p3.positions.left_foot, 20);
        assert_eq!(p3.positions.right_foot, 20);
    }

    #[test]
    fn test_hud_command_json_roundtrip() {
        let cmd = HudCommand::GoldenGeneration {
            nation_ptr: Some(0x180000000),
            nation_id: Some(759),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("golden_generation"));

        let deserialized: HudCommand = serde_json::from_str(&json).unwrap();
        match deserialized {
            HudCommand::GoldenGeneration { nation_ptr, nation_id } => {
                assert_eq!(nation_ptr, Some(0x180000000));
                assert_eq!(nation_id, Some(759));
            }
            _ => panic!("Expected GoldenGeneration variant"),
        }
    }
}
