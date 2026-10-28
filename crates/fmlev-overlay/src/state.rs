use serde::{Deserialize, Serialize};
use fmlev_core::{ClubAffiliate, ClubFinances, CompetitionDetails, NationDetails, Player};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OverlayDisplayMode {
    Full,
    #[default]
    CompactOverlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OverlayTab {
    #[default]
    GodMode,
    PlayerEditor,
    Positions,
    ClubFinances,
    Nation,
    Competition,
    Affiliates,
    MemoryScanner,
    Modules,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NotificationLevel {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HudNotification {
    pub id: u64,
    pub title: String,
    pub message: String,
    pub level: NotificationLevel,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayState {
    pub is_visible: bool,
    pub mode: OverlayDisplayMode,
    pub active_tab: OverlayTab,
    pub target_pid: Option<i32>,
    pub game_assembly_base: Option<usize>,
    pub unity_player_base: Option<usize>,
    pub wine_prefix: Option<String>,
    pub status_message: String,
    pub selected_player_id: Option<i32>,
    pub selected_player_ptr: Option<usize>,
    pub selected_club_id: Option<i32>,
    pub selected_club_ptr: Option<usize>,
    pub selected_nation_id: Option<i32>,
    pub selected_nation_ptr: Option<usize>,
    pub selected_competition_id: Option<i32>,
    pub selected_competition_ptr: Option<usize>,
    #[serde(default)]
    pub active_player: Option<Player>,
    #[serde(default)]
    pub active_nation: Option<NationDetails>,
    #[serde(default)]
    pub active_competition: Option<CompetitionDetails>,
    #[serde(default)]
    pub active_club: Option<ClubFinances>,
    #[serde(default)]
    pub active_affiliates: Vec<ClubAffiliate>,
    pub notifications: Vec<HudNotification>,
    pub next_notification_id: u64,
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            is_visible: true,
            mode: OverlayDisplayMode::CompactOverlay,
            active_tab: OverlayTab::GodMode,
            target_pid: None,
            game_assembly_base: None,
            unity_player_base: None,
            wine_prefix: None,
            status_message: "Ready to hook Football Manager 26 on Linux".to_string(),
            selected_player_id: None,
            selected_player_ptr: None,
            selected_club_id: None,
            selected_club_ptr: None,
            selected_nation_id: None,
            selected_nation_ptr: None,
            selected_competition_id: None,
            selected_competition_ptr: None,
            active_player: None,
            active_nation: None,
            active_competition: None,
            active_club: None,
            active_affiliates: Vec::new(),
            notifications: Vec::new(),
            next_notification_id: 1,
        }
    }
}

impl OverlayState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle_visibility(&mut self) {
        self.is_visible = !self.is_visible;
    }

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            OverlayDisplayMode::Full => OverlayDisplayMode::CompactOverlay,
            OverlayDisplayMode::CompactOverlay => OverlayDisplayMode::Full,
        };
    }

    pub fn set_active_tab(&mut self, tab: OverlayTab) {
        self.active_tab = tab;
    }

    pub fn update_connection(
        &mut self,
        pid: i32,
        game_assembly_base: Option<usize>,
        unity_player_base: Option<usize>,
        wine_prefix: Option<String>,
    ) {
        self.target_pid = Some(pid);
        self.game_assembly_base = game_assembly_base;
        self.unity_player_base = unity_player_base;
        self.wine_prefix = wine_prefix;

        let base_str = game_assembly_base
            .map(|b| format!("0x{:x}", b))
            .unwrap_or_else(|| "N/A".to_string());

        self.status_message = format!("Attached to FM26 (PID: {}) | Base: {}", pid, base_str);
        self.add_notification(
            "Game Attached",
            &format!("Successfully hooked FM26 PID {} with GameAssembly at {}", pid, base_str),
            NotificationLevel::Success,
        );
    }

    pub fn disconnect(&mut self) {
        self.target_pid = None;
        self.game_assembly_base = None;
        self.unity_player_base = None;
        self.wine_prefix = None;
        self.status_message = "Disconnected from Football Manager 26".to_string();
        self.add_notification(
            "Disconnected",
            "Detached from game process",
            NotificationLevel::Warning,
        );
    }

    pub fn add_notification(&mut self, title: &str, message: &str, level: NotificationLevel) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let notif = HudNotification {
            id: self.next_notification_id,
            title: title.to_string(),
            message: message.to_string(),
            level,
            timestamp_ms: now,
        };
        self.next_notification_id += 1;

        self.notifications.push(notif);
        // Retain only latest 20 notifications
        if self.notifications.len() > 20 {
            self.notifications.remove(0);
        }
    }

    pub fn clear_notifications(&mut self) {
        self.notifications.clear();
    }

    pub fn select_player(&mut self, id: i32, ptr: Option<usize>) {
        self.selected_player_id = Some(id);
        self.selected_player_ptr = ptr;
        let mut p = self.active_player.clone().unwrap_or_else(Self::default_preview_player);
        p.id = id;
        self.active_player = Some(p);
        self.add_notification(
            "Player Selected",
            &format!("Active player set to ID {}", id),
            NotificationLevel::Info,
        );
    }

    pub fn set_active_player(&mut self, player: Player) {
        self.selected_player_id = Some(player.id);
        self.active_player = Some(player);
    }

    pub fn default_preview_player() -> Player {
        Player {
            id: 8000,
            name: "Erling Haaland".to_string(),
            club_id: 102,
            current_ability: 195,
            potential_ability: 200,
            condition: 10000,
            sharpness: 10000,
            morale: 100,
            age: 24,
            nationality: "Norway".to_string(),
            position: "ST (C)".to_string(),
            positions: fmlev_core::PlayerPositions {
                st: 20,
                amc: 12,
                aml: 10,
                amr: 10,
                mc: 10,
                left_foot: 20,
                right_foot: 15,
                ..Default::default()
            },
            club_name: "Manchester City".to_string(),
            morale_label: "Superb".to_string(),
            is_injured: false,
            is_suspended: false,
            contract: fmlev_core::PlayerContract::default(),
            attributes: fmlev_core::PlayerAttributes {
                finishing: 20,
                pace: 19,
                acceleration: 18,
                dribbling: 15,
                passing: 14,
                tackling: 7,
                decisions: 17,
                composure: 18,
                determination: 19,
                work_rate: 16,
                ..Default::default()
            },
            user_modified: false,
            preferred_moves: None,
        }
    }

    pub fn select_nation(&mut self, id: i32, ptr: Option<usize>) {
        self.selected_nation_id = Some(id);
        self.selected_nation_ptr = ptr;
        let mut nat = self.active_nation.clone().unwrap_or_default();
        nat.id = id;
        self.active_nation = Some(nat);
        self.add_notification(
            "Nation Selected",
            &format!("Active nation set to ID {}", id),
            NotificationLevel::Info,
        );
    }

    pub fn select_competition(&mut self, id: i32, ptr: Option<usize>) {
        self.selected_competition_id = Some(id);
        self.selected_competition_ptr = ptr;
        let mut comp = self.active_competition.clone().unwrap_or_default();
        comp.id = id;
        self.active_competition = Some(comp);
        self.add_notification(
            "Competition Selected",
            &format!("Active competition set to ID {}", id),
            NotificationLevel::Info,
        );
    }

    pub fn select_club(&mut self, id: i32, ptr: Option<usize>) {
        self.selected_club_id = Some(id);
        self.selected_club_ptr = ptr;
        let mut club = self.active_club.clone().unwrap_or_default();
        club.club_id = id;
        self.active_club = Some(club);
        self.add_notification(
            "Club Selected",
            &format!("Active club set to ID {}", id),
            NotificationLevel::Info,
        );
    }

    pub fn set_active_nation(&mut self, nation: NationDetails) {
        self.selected_nation_id = Some(nation.id);
        self.active_nation = Some(nation);
    }

    pub fn set_active_competition(&mut self, comp: CompetitionDetails) {
        self.selected_competition_id = Some(comp.id);
        self.active_competition = Some(comp);
    }

    pub fn set_active_club(&mut self, club: ClubFinances) {
        self.selected_club_id = Some(club.club_id);
        self.active_affiliates = club.affiliates.clone();
        self.active_club = Some(club);
    }

    pub fn add_affiliate(&mut self, affiliate: ClubAffiliate) {
        if let Some(ref mut club) = self.active_club {
            club.affiliates.push(affiliate.clone());
        }
        self.active_affiliates.push(affiliate);
    }
}
