use fmlev_core::ClubAffiliate;
use serde::{Deserialize, Serialize};
use crate::state::{NotificationLevel, OverlayState, OverlayTab};
pub use crate::terminal_hud::TerminalHudController;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", content = "payload", rename_all = "snake_case")]
pub enum HudCommand {
    HealSquad {
        player_ptr: usize,
        condition_offset: usize,
    },
    MaxSquadMorale {
        player_ptr: usize,
        morale_offset: usize,
    },
    ClearFatigue {
        player_ptr: usize,
    },
    UnlockDebugMenu,
    SetPlayerAbility {
        player_ptr: usize,
        current_ability: u8,
        potential_ability: u8,
    },
    SetPlayerAttribute {
        player_ptr: usize,
        attribute_offset: usize,
        value: u8,
    },
    UpdateClubFinances {
        club_ptr: usize,
        balance: i64,
        transfer_budget: i64,
        wage_budget: i32,
    },
    SugarDaddyBoost {
        club_ptr: usize,
        amount: i64,
    },
    ScanMemory {
        target_value: String,
        value_type: String,
    },
    ToggleOverlay,
    SwitchTab {
        tab: OverlayTab,
    },

    // --- Nation commands & quick actions ---
    GoldenGeneration {
        #[serde(default)]
        nation_ptr: Option<usize>,
        #[serde(default)]
        nation_id: Option<i32>,
    },
    MaxYouthRating {
        #[serde(default)]
        nation_ptr: Option<usize>,
        #[serde(default)]
        nation_id: Option<i32>,
    },
    UpdateNation {
        #[serde(default)]
        nation_ptr: Option<usize>,
        #[serde(default)]
        youth_rating: Option<u8>,
        #[serde(default)]
        economic_factor: Option<u8>,
        #[serde(default)]
        game_importance: Option<u8>,
    },

    // --- Competition commands & quick actions ---
    RelaxRules {
        #[serde(default)]
        comp_ptr: Option<usize>,
        #[serde(default)]
        comp_id: Option<i32>,
    },
    FiveSubs {
        #[serde(default)]
        comp_ptr: Option<usize>,
        #[serde(default)]
        comp_id: Option<i32>,
    },
    UpdateCompetitionRules {
        #[serde(default)]
        comp_ptr: Option<usize>,
        #[serde(default)]
        max_substitutes: Option<u8>,
        #[serde(default)]
        max_bench_size: Option<u8>,
        #[serde(default)]
        max_foreign_players: Option<u8>,
    },

    // --- Club Facilities & Board commands ---
    MaxFacilities {
        #[serde(default)]
        club_ptr: Option<usize>,
        #[serde(default)]
        club_id: Option<i32>,
    },
    ForegroundSugarDaddy {
        #[serde(default)]
        club_ptr: Option<usize>,
        #[serde(default)]
        club_id: Option<i32>,
    },
    UpdateClubFacilities {
        #[serde(default)]
        club_ptr: Option<usize>,
        #[serde(default)]
        training_facilities: Option<u8>,
        #[serde(default)]
        youth_facilities: Option<u8>,
        #[serde(default)]
        youth_recruitment: Option<u8>,
        #[serde(default)]
        junior_coaching: Option<u8>,
    },
    AddClubAffiliate {
        #[serde(default)]
        club_ptr: Option<usize>,
        affiliate_id: i32,
        #[serde(default)]
        affiliate_name: String,
        #[serde(default)]
        is_parent: bool,
    },

    // --- Selection commands ---
    SelectPlayer {
        player_id: i32,
        #[serde(default)]
        player_ptr: Option<usize>,
    },
    SelectNation {
        nation_id: i32,
        #[serde(default)]
        nation_ptr: Option<usize>,
    },
    SelectCompetition {
        competition_id: i32,
        #[serde(default)]
        competition_ptr: Option<usize>,
    },
    SelectClub {
        club_id: i32,
        #[serde(default)]
        club_ptr: Option<usize>,
    },

    // --- Positional macros ---
    AllPositions20 {
        #[serde(default)]
        player_ptr: Option<usize>,
        #[serde(default)]
        player_id: Option<i32>,
    },
    BothFeet20 {
        #[serde(default)]
        player_ptr: Option<usize>,
        #[serde(default)]
        player_id: Option<i32>,
    },
    PureStriker {
        #[serde(default)]
        player_ptr: Option<usize>,
        #[serde(default)]
        player_id: Option<i32>,
    },
    UniversalWinger {
        #[serde(default)]
        player_ptr: Option<usize>,
        #[serde(default)]
        player_id: Option<i32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub success: bool,
    pub message: String,
}

pub struct HudController {
    pub state: OverlayState,
}

impl Default for HudController {
    fn default() -> Self {
        Self::new()
    }
}

impl HudController {
    pub fn new() -> Self {
        Self {
            state: OverlayState::new(),
        }
    }

    pub fn handle_command(&mut self, cmd: HudCommand) -> CommandResult {
        match cmd {
            HudCommand::HealSquad { player_ptr, .. } => {
                info!("HUD: Executing 1-Click Heal Squad for 0x{:x}", player_ptr);
                self.state.add_notification(
                    "Squad Healed",
                    &format!("Restored 100% condition & sharpness at 0x{:x}", player_ptr),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: format!("Squad healed at 0x{:x}", player_ptr),
                }
            }
            HudCommand::MaxSquadMorale { player_ptr, .. } => {
                info!("HUD: Boosting squad morale to Superb (100)");
                self.state.add_notification(
                    "Morale Boosted",
                    &format!("Set squad morale to 100 (Superb) for 0x{:x}", player_ptr),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: "Squad morale maxed".to_string(),
                }
            }
            HudCommand::ClearFatigue { player_ptr } => {
                info!("HUD: Clearing fatigue and match suspensions");
                self.state.add_notification(
                    "Fatigue Cleared",
                    &format!("Cleared fatigue and match fitness penalty for 0x{:x}", player_ptr),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: "Fatigue cleared".to_string(),
                }
            }
            HudCommand::UnlockDebugMenu => {
                info!("HUD: Unlocking QA / Developer Debug Menu in FM26");
                self.state.add_notification(
                    "QA Menu Unlocked",
                    "Enabled FM26 internal QA & Developer debug menus",
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: "QA debug menu enabled".to_string(),
                }
            }
            HudCommand::SetPlayerAbility { player_ptr, current_ability, potential_ability } => {
                info!(
                    "HUD: Setting CA: {} / PA: {} for player 0x{:x}",
                    current_ability, potential_ability, player_ptr
                );
                self.state.add_notification(
                    "Ability Updated",
                    &format!("CA set to {}, PA set to {} for player at 0x{:x}", current_ability, potential_ability, player_ptr),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: "Player ability updated".to_string(),
                }
            }
            HudCommand::SetPlayerAttribute { player_ptr, attribute_offset, value } => {
                info!("HUD: Setting attribute at 0x{:x}+0x{:x} to {}", player_ptr, attribute_offset, value);
                self.state.add_notification(
                    "Attribute Written",
                    &format!("Written value {} at 0x{:x}", value, player_ptr + attribute_offset),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: "Attribute modified".to_string(),
                }
            }
            HudCommand::UpdateClubFinances { club_ptr, balance, transfer_budget, wage_budget } => {
                info!(
                    "HUD: Updating finances for club 0x{:x} (Balance: {}, Transfer: {}, Wage: {})",
                    club_ptr, balance, transfer_budget, wage_budget
                );
                self.state.add_notification(
                    "Finances Updated",
                    &format!("Balance: ${} | Transfer: ${} | Wage: ${}/wk", balance, transfer_budget, wage_budget),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: "Club finances updated".to_string(),
                }
            }
            HudCommand::SugarDaddyBoost { club_ptr, amount } => {
                info!("HUD: Injecting Sugar Daddy takeover funds of ${} into club 0x{:x}", amount, club_ptr);
                self.state.add_notification(
                    "Sugar Daddy Active",
                    &format!("Injected +${} into transfer and balance reserves", amount),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: format!("Injected +${} funds", amount),
                }
            }
            HudCommand::ScanMemory { target_value, value_type } => {
                info!("HUD: Scanning memory for value '{}' (type: {})", target_value, value_type);
                self.state.add_notification(
                    "Memory Scan",
                    &format!("Searching memory regions for '{}' ({})", target_value, value_type),
                    NotificationLevel::Info,
                );
                CommandResult {
                    success: true,
                    message: "Scan initiated".to_string(),
                }
            }
            HudCommand::ToggleOverlay => {
                self.state.toggle_visibility();
                CommandResult {
                    success: true,
                    message: format!("Overlay visibility: {}", self.state.is_visible),
                }
            }
            HudCommand::SwitchTab { tab } => {
                self.state.set_active_tab(tab);
                CommandResult {
                    success: true,
                    message: format!("Active tab: {:?}", tab),
                }
            }
            HudCommand::GoldenGeneration { nation_ptr, nation_id } => {
                let n_id = nation_id.or(self.state.selected_nation_id);
                if let Some(ptr) = nation_ptr {
                    self.state.selected_nation_ptr = Some(ptr);
                }
                if let Some(id) = n_id {
                    self.state.selected_nation_id = Some(id);
                }

                let mut nat = self.state.active_nation.clone().unwrap_or_default();
                if let Some(id) = n_id {
                    nat.id = id;
                }
                nat.golden_generation();
                let nat_name = nat.name.clone();
                let youth_rating = nat.youth_rating;
                let nat_id_disp = nat.id;
                self.state.active_nation = Some(nat);

                info!("HUD: Applied Golden Generation macro for {} (ID: {})", nat_name, nat_id_disp);
                self.state.add_notification(
                    "🌟 Golden Generation",
                    &format!("Maxed youth rating to {} & facilities 20/20 for {}", youth_rating, nat_name),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: format!("Golden generation applied for {} (Youth Rating: {})", nat_name, youth_rating),
                }
            }
            HudCommand::MaxYouthRating { nation_ptr, nation_id } => {
                let n_id = nation_id.or(self.state.selected_nation_id);
                if let Some(ptr) = nation_ptr {
                    self.state.selected_nation_ptr = Some(ptr);
                }
                if let Some(id) = n_id {
                    self.state.selected_nation_id = Some(id);
                }

                let mut nat = self.state.active_nation.clone().unwrap_or_default();
                if let Some(id) = n_id {
                    nat.id = id;
                }
                nat.youth_rating = 200;
                let nat_name = nat.name.clone();
                self.state.active_nation = Some(nat);

                info!("HUD: Applied Max Youth Rating (200) for {}", nat_name);
                self.state.add_notification(
                    "📈 Max Youth Rating",
                    &format!("Youth rating boosted to 200 for {}", nat_name),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: format!("Max youth rating (200) applied for {}", nat_name),
                }
            }
            HudCommand::UpdateNation { nation_ptr, youth_rating, economic_factor, game_importance } => {
                if let Some(ptr) = nation_ptr {
                    self.state.selected_nation_ptr = Some(ptr);
                }
                let mut nat = self.state.active_nation.clone().unwrap_or_default();
                if let Some(yr) = youth_rating {
                    nat.youth_rating = yr;
                }
                if let Some(ef) = economic_factor {
                    nat.economic_factor = ef;
                }
                if let Some(gi) = game_importance {
                    nat.game_importance = gi;
                }
                let nat_name = nat.name.clone();
                self.state.active_nation = Some(nat);

                info!("HUD: Updated nation attributes for {}", nat_name);
                self.state.add_notification(
                    "Nation Updated",
                    &format!("Updated nation parameters for {}", nat_name),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: format!("Nation {} updated", nat_name),
                }
            }
            HudCommand::RelaxRules { comp_ptr, comp_id } => {
                let c_id = comp_id.or(self.state.selected_competition_id);
                if let Some(ptr) = comp_ptr {
                    self.state.selected_competition_ptr = Some(ptr);
                }
                if let Some(id) = c_id {
                    self.state.selected_competition_id = Some(id);
                }

                let mut comp = self.state.active_competition.clone().unwrap_or_default();
                if let Some(id) = c_id {
                    comp.id = id;
                }
                comp.relax_rules();
                let comp_name = comp.name.clone();
                self.state.active_competition = Some(comp);

                info!("HUD: Applied Relax Rules macro for {}", comp_name);
                self.state.add_notification(
                    "🔓 Rules Relaxed",
                    &format!("Relaxed foreign cap (99), squad size (50), 12 bench seats for {}", comp_name),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: format!("Competition rules relaxed for {}", comp_name),
                }
            }
            HudCommand::FiveSubs { comp_ptr, comp_id } => {
                let c_id = comp_id.or(self.state.selected_competition_id);
                if let Some(ptr) = comp_ptr {
                    self.state.selected_competition_ptr = Some(ptr);
                }
                if let Some(id) = c_id {
                    self.state.selected_competition_id = Some(id);
                }

                let mut comp = self.state.active_competition.clone().unwrap_or_default();
                if let Some(id) = c_id {
                    comp.id = id;
                }
                comp.max_substitutes = 5;
                comp.substitution_windows = 3;
                if comp.max_bench_size < 9 {
                    comp.max_bench_size = 9;
                }
                let comp_name = comp.name.clone();
                self.state.active_competition = Some(comp);

                info!("HUD: Applied 5 Substitutes rule for {}", comp_name);
                self.state.add_notification(
                    "⚽ 5 Substitutes",
                    &format!("Enabled 5 subs across 3 windows for {}", comp_name),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: format!("5 substitutes enabled for {}", comp_name),
                }
            }
            HudCommand::UpdateCompetitionRules { comp_ptr, max_substitutes, max_bench_size, max_foreign_players } => {
                if let Some(ptr) = comp_ptr {
                    self.state.selected_competition_ptr = Some(ptr);
                }
                let mut comp = self.state.active_competition.clone().unwrap_or_default();
                if let Some(s) = max_substitutes {
                    comp.max_substitutes = s;
                }
                if let Some(b) = max_bench_size {
                    comp.max_bench_size = b;
                }
                if let Some(f) = max_foreign_players {
                    comp.max_foreign_players = f;
                }
                let comp_name = comp.name.clone();
                self.state.active_competition = Some(comp);

                info!("HUD: Updated competition rules for {}", comp_name);
                self.state.add_notification(
                    "Rules Updated",
                    &format!("Updated match and squad rules for {}", comp_name),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: format!("Competition rules updated for {}", comp_name),
                }
            }
            HudCommand::MaxFacilities { club_ptr, club_id } => {
                let cl_id = club_id.or(self.state.selected_club_id);
                if let Some(ptr) = club_ptr {
                    self.state.selected_club_ptr = Some(ptr);
                }
                if let Some(id) = cl_id {
                    self.state.selected_club_id = Some(id);
                }

                let mut club = self.state.active_club.clone().unwrap_or_default();
                if let Some(id) = cl_id {
                    club.club_id = id;
                }
                club.max_all_facilities();
                let cl_name = if club.club_name.is_empty() { "Active Club".to_string() } else { club.club_name.clone() };
                self.state.active_club = Some(club);

                info!("HUD: Applied Max Facilities macro for {}", cl_name);
                self.state.add_notification(
                    "🏗️ Max Facilities",
                    &format!("Upgraded training, youth, coaching & corporate facilities to 20/20 for {}", cl_name),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: format!("Club facilities maxed to 20/20 for {}", cl_name),
                }
            }
            HudCommand::ForegroundSugarDaddy { club_ptr, club_id } => {
                let cl_id = club_id.or(self.state.selected_club_id);
                if let Some(ptr) = club_ptr {
                    self.state.selected_club_ptr = Some(ptr);
                }
                if let Some(id) = cl_id {
                    self.state.selected_club_id = Some(id);
                }

                let mut club = self.state.active_club.clone().unwrap_or_default();
                if let Some(id) = cl_id {
                    club.club_id = id;
                }
                club.apply_sugar_daddy("Foreground");
                let cl_name = if club.club_name.is_empty() { "Active Club".to_string() } else { club.club_name.clone() };
                self.state.active_club = Some(club);

                info!("HUD: Activated Foreground Sugar Daddy for {}", cl_name);
                self.state.add_notification(
                    "💎 Foreground Sugar Daddy",
                    &format!("Assigned Foreground Tycoon ($500M balance / $250M budget / 100% board) for {}", cl_name),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: format!("Foreground Sugar Daddy activated for {}", cl_name),
                }
            }
            HudCommand::UpdateClubFacilities { club_ptr, training_facilities, youth_facilities, youth_recruitment, junior_coaching } => {
                if let Some(ptr) = club_ptr {
                    self.state.selected_club_ptr = Some(ptr);
                }
                let mut club = self.state.active_club.clone().unwrap_or_default();
                if let Some(tf) = training_facilities {
                    club.training_facilities = tf;
                }
                if let Some(yf) = youth_facilities {
                    club.youth_facilities = yf;
                }
                if let Some(yr) = youth_recruitment {
                    club.youth_recruitment = yr;
                }
                if let Some(jc) = junior_coaching {
                    club.junior_coaching = jc;
                }
                let cl_name = if club.club_name.is_empty() { "Active Club".to_string() } else { club.club_name.clone() };
                self.state.active_club = Some(club);

                info!("HUD: Updated facility ratings for {}", cl_name);
                self.state.add_notification(
                    "Facilities Updated",
                    &format!("Updated facilities ratings for {}", cl_name),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: "Club facilities updated".to_string(),
                }
            }
            HudCommand::AddClubAffiliate { club_ptr, affiliate_id, affiliate_name, is_parent } => {
                if let Some(ptr) = club_ptr {
                    self.state.selected_club_ptr = Some(ptr);
                }
                let aff = ClubAffiliate {
                    affiliate_club_id: affiliate_id,
                    affiliate_name: if affiliate_name.is_empty() { format!("Affiliate Club {}", affiliate_id) } else { affiliate_name },
                    is_parent,
                    first_option_to_buy: true,
                    send_players_on_loan: true,
                    shared_scouting: true,
                    annual_fee: 100_000,
                    work_permit_haven: false,
                };
                let aff_name = aff.affiliate_name.clone();
                self.state.add_affiliate(aff);

                info!("HUD: Added affiliate relationship with {} (ID: {})", aff_name, affiliate_id);
                self.state.add_notification(
                    "🤝 Affiliate Added",
                    &format!("Added affiliate link with {} (ID: {})", aff_name, affiliate_id),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: format!("Affiliate '{}' (ID: {}) added", aff_name, affiliate_id),
                }
            }
            HudCommand::SelectNation { nation_id, nation_ptr } => {
                self.state.select_nation(nation_id, nation_ptr);
                CommandResult {
                    success: true,
                    message: format!("Selected nation ID {}", nation_id),
                }
            }
            HudCommand::SelectCompetition { competition_id, competition_ptr } => {
                self.state.select_competition(competition_id, competition_ptr);
                CommandResult {
                    success: true,
                    message: format!("Selected competition ID {}", competition_id),
                }
            }
            HudCommand::SelectClub { club_id, club_ptr } => {
                self.state.select_club(club_id, club_ptr);
                CommandResult {
                    success: true,
                    message: format!("Selected club ID {}", club_id),
                }
            }
            HudCommand::SelectPlayer { player_id, player_ptr } => {
                self.state.select_player(player_id, player_ptr);
                CommandResult {
                    success: true,
                    message: format!("Selected player ID {}", player_id),
                }
            }
            HudCommand::AllPositions20 { player_ptr, player_id } => {
                let p_id = player_id.or(self.state.selected_player_id);
                if let Some(ptr) = player_ptr {
                    self.state.selected_player_ptr = Some(ptr);
                }
                if let Some(id) = p_id {
                    self.state.selected_player_id = Some(id);
                }
                let mut p = self.state.active_player.clone().unwrap_or_else(OverlayState::default_preview_player);
                if let Some(id) = p_id {
                    p.id = id;
                }
                let pos = &mut p.positions;
                pos.dl = 20; pos.dc = 20; pos.dr = 20;
                pos.wbl = 20; pos.wbr = 20;
                pos.dm = 20;
                pos.ml = 20; pos.mc = 20; pos.mr = 20;
                pos.aml = 20; pos.amc = 20; pos.amr = 20;
                pos.st = 20;
                pos.left_foot = 20;
                pos.right_foot = 20;
                p.position = "D/WB/DM/M/AM (RLC), ST (C)".to_string();
                p.attributes.versatility = 20;
                let p_name = p.name.clone();
                self.state.active_player = Some(p);

                info!("HUD: Applied all_positions_20 for {}", p_name);
                self.state.add_notification(
                    "🌟 Total Football",
                    &format!("Maxed all positions and both feet to 20 for {}", p_name),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: format!("All positions maxed to 20 for {}", p_name),
                }
            }
            HudCommand::BothFeet20 { player_ptr, player_id } => {
                let p_id = player_id.or(self.state.selected_player_id);
                if let Some(ptr) = player_ptr {
                    self.state.selected_player_ptr = Some(ptr);
                }
                if let Some(id) = p_id {
                    self.state.selected_player_id = Some(id);
                }
                let mut p = self.state.active_player.clone().unwrap_or_else(OverlayState::default_preview_player);
                if let Some(id) = p_id {
                    p.id = id;
                }
                p.positions.left_foot = 20;
                p.positions.right_foot = 20;
                let p_name = p.name.clone();
                self.state.active_player = Some(p);

                info!("HUD: Applied both_feet_20 for {}", p_name);
                self.state.add_notification(
                    "👣 Both Feet 20",
                    &format!("Set left foot and right foot to 20 for {}", p_name),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: format!("Both feet set to 20 for {}", p_name),
                }
            }
            HudCommand::PureStriker { player_ptr, player_id } => {
                let p_id = player_id.or(self.state.selected_player_id);
                if let Some(ptr) = player_ptr {
                    self.state.selected_player_ptr = Some(ptr);
                }
                if let Some(id) = p_id {
                    self.state.selected_player_id = Some(id);
                }
                let mut p = self.state.active_player.clone().unwrap_or_else(OverlayState::default_preview_player);
                if let Some(id) = p_id {
                    p.id = id;
                }
                p.positions.st = 20;
                p.positions.aml = 14;
                p.positions.amc = 14;
                p.positions.amr = 14;
                p.position = p.positions.to_position_string();
                let p_name = p.name.clone();
                self.state.active_player = Some(p);

                info!("HUD: Applied pure_striker for {}", p_name);
                self.state.add_notification(
                    "⚽ Pure Striker",
                    &format!("Natural striker preset applied for {}", p_name),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: format!("Pure striker preset applied for {}", p_name),
                }
            }
            HudCommand::UniversalWinger { player_ptr, player_id } => {
                let p_id = player_id.or(self.state.selected_player_id);
                if let Some(ptr) = player_ptr {
                    self.state.selected_player_ptr = Some(ptr);
                }
                if let Some(id) = p_id {
                    self.state.selected_player_id = Some(id);
                }
                let mut p = self.state.active_player.clone().unwrap_or_else(OverlayState::default_preview_player);
                if let Some(id) = p_id {
                    p.id = id;
                }
                p.positions.aml = 20;
                p.positions.amr = 20;
                p.positions.ml = 20;
                p.positions.mr = 20;
                p.positions.wbl = 20;
                p.positions.wbr = 20;
                p.position = p.positions.to_position_string();
                let p_name = p.name.clone();
                self.state.active_player = Some(p);

                info!("HUD: Applied universal_winger for {}", p_name);
                self.state.add_notification(
                    "🚀 Universal Winger",
                    &format!("Flank and wing positions set to 20 for {}", p_name),
                    NotificationLevel::Success,
                );
                CommandResult {
                    success: true,
                    message: format!("Universal winger preset applied for {}", p_name),
                }
            }
        }
    }

    /// Parses and executes text-based HUD / terminal commands
    pub fn execute_command(&mut self, input: &str) -> CommandResult {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return CommandResult {
                success: false,
                message: "Empty command".to_string(),
            };
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        let cmd = parts[0].to_lowercase();
        let args = &parts[1..];

        match cmd.as_str() {
            // Tab switching & views
            "player" | "players" => {
                if args.is_empty() {
                    self.state.set_active_tab(OverlayTab::PlayerEditor);
                    CommandResult {
                        success: true,
                        message: "Switched to Player tab".to_string(),
                    }
                } else if let Ok(id) = args[0].parse::<i32>() {
                    self.handle_command(HudCommand::SelectPlayer { player_id: id, player_ptr: None });
                    self.state.set_active_tab(OverlayTab::PlayerEditor);
                    CommandResult {
                        success: true,
                        message: format!("Selected player ID {} and switched to Player tab", id),
                    }
                } else {
                    self.state.set_active_tab(OverlayTab::PlayerEditor);
                    CommandResult {
                        success: true,
                        message: "Switched to Player tab".to_string(),
                    }
                }
            }
            "positions" | "position" | "pos" => {
                self.state.set_active_tab(OverlayTab::Positions);
                CommandResult {
                    success: true,
                    message: "Switched to Positions tab".to_string(),
                }
            }
            "club" | "clubs" => {
                if args.is_empty() {
                    self.state.set_active_tab(OverlayTab::ClubFinances);
                    CommandResult {
                        success: true,
                        message: "Switched to ClubFinances tab".to_string(),
                    }
                } else if let Ok(id) = args[0].parse::<i32>() {
                    self.handle_command(HudCommand::SelectClub { club_id: id, club_ptr: None });
                    self.state.set_active_tab(OverlayTab::ClubFinances);
                    CommandResult {
                        success: true,
                        message: format!("Selected club ID {} and switched to Club tab", id),
                    }
                } else {
                    self.state.set_active_tab(OverlayTab::ClubFinances);
                    CommandResult {
                        success: true,
                        message: "Switched to ClubFinances tab".to_string(),
                    }
                }
            }
            "nation" | "nations" => {
                if args.is_empty() {
                    self.state.set_active_tab(OverlayTab::Nation);
                    CommandResult {
                        success: true,
                        message: "Switched to Nation tab".to_string(),
                    }
                } else if let Ok(id) = args[0].parse::<i32>() {
                    self.state.select_nation(id, None);
                    self.state.set_active_tab(OverlayTab::Nation);
                    CommandResult {
                        success: true,
                        message: format!("Selected nation ID {} and switched to Nation tab", id),
                    }
                } else {
                    self.state.set_active_tab(OverlayTab::Nation);
                    CommandResult {
                        success: true,
                        message: "Switched to Nation tab".to_string(),
                    }
                }
            }
            "competition" | "comp" | "competitions" => {
                if args.is_empty() {
                    self.state.set_active_tab(OverlayTab::Competition);
                    CommandResult {
                        success: true,
                        message: "Switched to Competition tab".to_string(),
                    }
                } else if let Ok(id) = args[0].parse::<i32>() {
                    self.state.select_competition(id, None);
                    self.state.set_active_tab(OverlayTab::Competition);
                    CommandResult {
                        success: true,
                        message: format!("Selected competition ID {} and switched to Competition tab", id),
                    }
                } else {
                    self.state.set_active_tab(OverlayTab::Competition);
                    CommandResult {
                        success: true,
                        message: "Switched to Competition tab".to_string(),
                    }
                }
            }
            "affiliates" | "affiliate" => {
                if args.is_empty() {
                    self.state.set_active_tab(OverlayTab::Affiliates);
                    CommandResult {
                        success: true,
                        message: "Switched to Affiliates tab".to_string(),
                    }
                } else if args[0].to_lowercase() == "add" && args.len() >= 2 {
                    let id = args[1].parse::<i32>().unwrap_or(0);
                    let name = if args.len() > 2 { args[2..].join(" ") } else { format!("Affiliate {}", id) };
                    self.handle_command(HudCommand::AddClubAffiliate {
                        club_ptr: self.state.selected_club_ptr,
                        affiliate_id: id,
                        affiliate_name: name,
                        is_parent: false,
                    })
                } else {
                    self.state.set_active_tab(OverlayTab::Affiliates);
                    CommandResult {
                        success: true,
                        message: "Switched to Affiliates tab".to_string(),
                    }
                }
            }
            "tab" | "goto" => {
                if args.is_empty() {
                    return CommandResult {
                        success: false,
                        message: "Usage: tab <godmode|player|club|nation|competition|affiliates|scanner|modules|settings>".to_string(),
                    };
                }
                match args[0].to_lowercase().as_str() {
                    "god" | "godmode" | "god_mode" => {
                        self.state.set_active_tab(OverlayTab::GodMode);
                        CommandResult { success: true, message: "Switched to GodMode tab".to_string() }
                    }
                    "player" | "player_editor" => {
                        self.state.set_active_tab(OverlayTab::PlayerEditor);
                        CommandResult { success: true, message: "Switched to PlayerEditor tab".to_string() }
                    }
                    "positions" | "pos" | "position" => {
                        self.state.set_active_tab(OverlayTab::Positions);
                        CommandResult { success: true, message: "Switched to Positions tab".to_string() }
                    }
                    "club" | "finances" | "club_finances" | "facilities" => {
                        self.state.set_active_tab(OverlayTab::ClubFinances);
                        CommandResult { success: true, message: "Switched to ClubFinances tab".to_string() }
                    }
                    "nation" => {
                        self.state.set_active_tab(OverlayTab::Nation);
                        CommandResult { success: true, message: "Switched to Nation tab".to_string() }
                    }
                    "comp" | "competition" => {
                        self.state.set_active_tab(OverlayTab::Competition);
                        CommandResult { success: true, message: "Switched to Competition tab".to_string() }
                    }
                    "affiliate" | "affiliates" => {
                        self.state.set_active_tab(OverlayTab::Affiliates);
                        CommandResult { success: true, message: "Switched to Affiliates tab".to_string() }
                    }
                    "scan" | "scanner" | "memory_scanner" => {
                        self.state.set_active_tab(OverlayTab::MemoryScanner);
                        CommandResult { success: true, message: "Switched to MemoryScanner tab".to_string() }
                    }
                    "modules" => {
                        self.state.set_active_tab(OverlayTab::Modules);
                        CommandResult { success: true, message: "Switched to Modules tab".to_string() }
                    }
                    "settings" => {
                        self.state.set_active_tab(OverlayTab::Settings);
                        CommandResult { success: true, message: "Switched to Settings tab".to_string() }
                    }
                    other => CommandResult {
                        success: false,
                        message: format!("Unknown tab: '{}'", other),
                    },
                }
            }

            // --- Nation god-mode macros ---
            "golden_generation" | "nation_golden_generation" | "golden" | "g" => {
                self.handle_command(HudCommand::GoldenGeneration {
                    nation_ptr: self.state.selected_nation_ptr,
                    nation_id: self.state.selected_nation_id,
                })
            }
            "max_youth_rating" | "youth_rating" | "max_youth" | "y" => {
                self.handle_command(HudCommand::MaxYouthRating {
                    nation_ptr: self.state.selected_nation_ptr,
                    nation_id: self.state.selected_nation_id,
                })
            }

            // --- Competition god-mode macros ---
            "relax_rules" | "competition_relax_rules" | "relax" | "r" => {
                self.handle_command(HudCommand::RelaxRules {
                    comp_ptr: self.state.selected_competition_ptr,
                    comp_id: self.state.selected_competition_id,
                })
            }
            "five_subs" | "5subs" | "5_subs" | "5" => {
                self.handle_command(HudCommand::FiveSubs {
                    comp_ptr: self.state.selected_competition_ptr,
                    comp_id: self.state.selected_competition_id,
                })
            }

            // --- Club Facilities & Board macros ---
            "max_facilities" | "facilities" | "club_max_facilities" | "max_fac" | "c" => {
                self.handle_command(HudCommand::MaxFacilities {
                    club_ptr: self.state.selected_club_ptr,
                    club_id: self.state.selected_club_id,
                })
            }
            "foreground_sugar_daddy" | "foreground_sugar" | "sugar_daddy_foreground" | "fsd" => {
                self.handle_command(HudCommand::ForegroundSugarDaddy {
                    club_ptr: self.state.selected_club_ptr,
                    club_id: self.state.selected_club_id,
                })
            }

            // --- Positional macros ---
            "all_positions_20" | "all_positions" | "total_football" | "all_pos" => {
                self.handle_command(HudCommand::AllPositions20 {
                    player_ptr: self.state.selected_player_ptr,
                    player_id: self.state.selected_player_id,
                })
            }
            "both_feet_20" | "both_feet" | "either_foot" => {
                self.handle_command(HudCommand::BothFeet20 {
                    player_ptr: self.state.selected_player_ptr,
                    player_id: self.state.selected_player_id,
                })
            }
            "pure_striker" => {
                self.handle_command(HudCommand::PureStriker {
                    player_ptr: self.state.selected_player_ptr,
                    player_id: self.state.selected_player_id,
                })
            }
            "universal_winger" => {
                self.handle_command(HudCommand::UniversalWinger {
                    player_ptr: self.state.selected_player_ptr,
                    player_id: self.state.selected_player_id,
                })
            }
            "max_ability" | "ca_pa_200" | "wonderkid" => {
                let p_ptr = self.state.selected_player_ptr.unwrap_or(0x18218e590);
                self.handle_command(HudCommand::SetPlayerAbility {
                    player_ptr: p_ptr,
                    current_ability: 200,
                    potential_ability: 200,
                })
            }

            // --- Existing squad / god mode macros ---
            "heal" | "heal_squad" | "h" => {
                let p_ptr = self.state.selected_player_ptr.unwrap_or(0x18218e590);
                self.handle_command(HudCommand::HealSquad {
                    player_ptr: p_ptr,
                    condition_offset: 0x20,
                })
            }
            "morale" | "max_morale" | "max_squad_morale" | "m" => {
                let p_ptr = self.state.selected_player_ptr.unwrap_or(0x18218e590);
                self.handle_command(HudCommand::MaxSquadMorale {
                    player_ptr: p_ptr,
                    morale_offset: 0x28,
                })
            }
            "fatigue" | "clear_fatigue" | "f" => {
                let p_ptr = self.state.selected_player_ptr.unwrap_or(0x18218e590);
                self.handle_command(HudCommand::ClearFatigue {
                    player_ptr: p_ptr,
                })
            }
            "sugar_daddy" | "sugar" | "s" => {
                let cl_ptr = self.state.selected_club_ptr.unwrap_or(0x182200000);
                let amount = if let Some(a) = args.first().and_then(|s| s.parse::<i64>().ok()) {
                    a
                } else {
                    100_000_000
                };
                self.handle_command(HudCommand::SugarDaddyBoost {
                    club_ptr: cl_ptr,
                    amount,
                })
            }
            "debug" | "unlock_debug" | "unlock_debug_menu" | "d" => {
                self.handle_command(HudCommand::UnlockDebugMenu)
            }
            "toggle" | "toggle_overlay" | "t" => {
                self.handle_command(HudCommand::ToggleOverlay)
            }
            "toggle_mode" | "mode" => {
                self.state.toggle_mode();
                CommandResult {
                    success: true,
                    message: format!("HUD mode changed to {:?}", self.state.mode),
                }
            }
            "add_affiliate" => {
                if args.is_empty() {
                    return CommandResult {
                        success: false,
                        message: "Usage: add_affiliate <id> [name]".to_string(),
                    };
                }
                let id = args[0].parse::<i32>().unwrap_or(0);
                let name = if args.len() > 1 { args[1..].join(" ") } else { format!("Affiliate {}", id) };
                self.handle_command(HudCommand::AddClubAffiliate {
                    club_ptr: self.state.selected_club_ptr,
                    affiliate_id: id,
                    affiliate_name: name,
                    is_parent: false,
                })
            }
            "help" | "?" => {
                CommandResult {
                    success: true,
                    message: "Available commands: nation, competition, affiliates, golden_generation, max_youth_rating, relax_rules, five_subs, max_facilities, foreground_sugar_daddy, heal, morale, fatigue, sugar_daddy, debug, toggle, mode, tab <name>".to_string(),
                }
            }
            unknown => {
                CommandResult {
                    success: false,
                    message: format!("Unknown command: '{}'. Type 'help' for available commands.", unknown),
                }
            }
        }
    }
}
