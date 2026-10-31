use std::collections::HashMap;
use tracing::info;

use fmlev_il2cpp::OffsetsRegistry;
use fmlev_mem::find_fm_process;

use crate::error::{CoreError, Result};
use crate::models::{
    ActiveContext, ClubFinances, CompetitionDetails, NationDetails, Player,
    PlayerAttributes, PlayerPositions,
};
use crate::session::LiveSession;

pub mod default_registries;
pub mod inspector;
pub mod mutations;
pub mod prizes;
pub mod squad;
pub mod registration;
pub mod club_money;
pub mod native_sync;

#[cfg(test)]
mod tests;

pub(crate) use default_registries::build_default_registries;
pub use inspector::{decode_fm_link_name, read_typed_value, DecodedTypedValue};
use inspector::{
    build_template_live_player, unknown_clubfinances, unknown_competitiondetails,
    unknown_nationdetails,
};

pub const DEFAULT_OFFSETS_JSON: &str = include_str!("../../../../offsets/v26.0.0.json");

/// Search results returned by GameContextManager.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq)]
pub struct SearchResults {
    pub query: String,
    pub players: Vec<Player>,
    pub clubs: Vec<ClubFinances>,
    #[serde(default)]
    pub nations: Vec<NationDetails>,
    #[serde(default)]
    pub competitions: Vec<CompetitionDetails>,
}

impl SearchResults {
    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
            && self.clubs.is_empty()
            && self.nations.is_empty()
            && self.competitions.is_empty()
    }

    pub fn total_count(&self) -> usize {
        self.players.len() + self.clubs.len() + self.nations.len() + self.competitions.len()
    }
}

/// Rich, context-aware Football Manager 26 game context and entity engine.
///
/// Automatically attaches to the live FM26 process, inspects IL2CPP UI singletons
/// (PanelManager, Bindings, GlobalGameModule), syncs active PersonReference,
/// ClubReference, NationReference, and CompReference in memory, and provides user-friendly editing.
pub struct GameContextManager {
    pub context: ActiveContext,
    pub(crate) auto_attach: bool,
    pub session: Option<LiveSession>,
    pub(crate) native_player_records: HashMap<i32, crate::native_player::NativePlayerRecord>,
    pub(crate) failed_native_fingerprints: HashMap<i32, crate::live_write::NativePlayerFingerprint>,
    /// Polling skips native discovery of a followed id until this time after a failed full scan
    /// (e.g. the game shows a manager or staff profile, which is not a player record).
    pub(crate) native_discovery_retry_at: HashMap<i32, std::time::Instant>,
    pub(crate) auto_discovery_attempted: bool,
    pub registry: OffsetsRegistry,
    pub player_registry: HashMap<i32, Player>,
    pub club_registry: HashMap<i32, ClubFinances>,
    pub nation_registry: HashMap<i32, NationDetails>,
    pub competition_registry: HashMap<i32, CompetitionDetails>,
    pub live_clubs: HashMap<i32, crate::live_club::LiveClubSnapshot>,
    pub live_nations: HashMap<i32, crate::live_nation::LiveNationSnapshot>,
    pub live_competitions: HashMap<i32, crate::live_competition::LiveCompetitionSnapshot>,
    pub player_memory_bindings: HashMap<i32, usize>,
    pub club_memory_bindings: HashMap<i32, usize>,
    /// Original registration gate byte per (competition index, uid), for exact restore.
    pub registration_gates: HashMap<(i32, i32), crate::native_registration::RegistrationOriginal>,
    pub nation_memory_bindings: HashMap<i32, usize>,
    pub competition_memory_bindings: HashMap<i32, usize>,
    pub active_player_addr: Option<usize>,
    pub active_club_addr: Option<usize>,
    pub active_nation_addr: Option<usize>,
    pub active_comp_addr: Option<usize>,
    pub user_modified_players: std::collections::HashSet<i32>,
    pub user_modified_clubs: std::collections::HashSet<i32>,
    /// Clubs whose facilities were read natively; binding-cache ratings must not overwrite them.
    pub native_synced_clubs: std::collections::HashSet<i32>,
}

impl Default for GameContextManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GameContextManager {
    /// Creates a new `GameContextManager`, loading default offsets and attempting auto-attach to FM26.
    pub fn new() -> Self {
        let registry = OffsetsRegistry::load_from_json(DEFAULT_OFFSETS_JSON)
            .unwrap_or_default();
        let mut manager = Self::new_with_registry(registry);
        manager.auto_attach = true;
        let _ = manager.try_attach();
        manager
    }

    /// Creates an offline `GameContextManager` with full realistic registry populated, without auto-attaching.
    pub fn new_offline() -> Self {
        let registry = OffsetsRegistry::load_from_json(DEFAULT_OFFSETS_JSON)
            .unwrap_or_default();
        Self::new_with_registry(registry)
    }

    /// Creates a `GameContextManager` with a custom `OffsetsRegistry`.
    pub fn new_with_registry(registry: OffsetsRegistry) -> Self {
        let (player_registry, club_registry, nation_registry, competition_registry) =
            build_default_registries();

        let initial_player = player_registry.get(&101).cloned().unwrap_or_default();
        let initial_club = club_registry.get(&1001).cloned().unwrap_or_default();
        let initial_nation = nation_registry.get(&765).cloned().unwrap_or_default();
        let initial_comp = competition_registry.get(&11).cloned().unwrap_or_default();

        let context = ActiveContext {
            attached: false,
            pid: None,
            active_screen: "player".to_string(),
            sync_mode: "auto_follow".to_string(),
            player: initial_player,
            club: initial_club,
            nation: initial_nation,
            competition: initial_comp,
        };

        Self {
            context,
            auto_attach: false,
            session: None,
            native_player_records: HashMap::new(),
            failed_native_fingerprints: HashMap::new(),
            native_discovery_retry_at: HashMap::new(),
            auto_discovery_attempted: false,
            registry,
            player_registry,
            club_registry,
            nation_registry,
            competition_registry,
            live_clubs: HashMap::new(),
            live_nations: HashMap::new(),
            live_competitions: HashMap::new(),
            player_memory_bindings: HashMap::new(),
            club_memory_bindings: HashMap::new(),
            registration_gates: HashMap::new(),
            nation_memory_bindings: HashMap::new(),
            competition_memory_bindings: HashMap::new(),
            active_player_addr: None,
            active_club_addr: None,
            active_nation_addr: None,
            active_comp_addr: None,
            user_modified_players: std::collections::HashSet::new(),
            user_modified_clubs: std::collections::HashSet::new(),
            native_synced_clubs: std::collections::HashSet::new(),
        }
    }

    /// Creates a `GameContextManager` attached to an existing `LiveSession`.
    pub fn with_session(session: LiveSession) -> Self {
        let pid = session.driver.pid();
        let registry = session.registry.clone();
        let mut manager = Self::new_with_registry(registry);
        manager.context.attached = true;
        manager.context.pid = Some(pid);
        manager.session = Some(session);
        if pid != std::process::id() as i32 { manager.reset_live_state(); }
        manager
    }

    /// Discard demo data and cached references when attaching to a real game process.
    pub(crate) fn reset_live_state(&mut self) {
        self.native_player_records.clear();
        self.failed_native_fingerprints.clear();
        self.native_discovery_retry_at.clear();
        self.auto_discovery_attempted = false;
        self.live_clubs.clear();
        self.live_nations.clear();
        self.live_competitions.clear();
        self.player_registry.clear();
        self.club_registry.clear();
        self.nation_registry.clear();
        self.competition_registry.clear();
        self.player_memory_bindings.clear();
        self.club_memory_bindings.clear();
        self.nation_memory_bindings.clear();
        self.competition_memory_bindings.clear();
        self.user_modified_players.clear();
        self.user_modified_clubs.clear();
        self.active_player_addr = None;
        self.active_club_addr = None;
        self.active_nation_addr = None;
        self.active_comp_addr = None;
        self.context.player = build_template_live_player(0, String::new(), String::new());
        self.context.club = unknown_clubfinances();
        self.context.nation = unknown_nationdetails();
        self.context.competition = unknown_competitiondetails();
    }

    /// Attempts to automatically discover and attach to the running `fm.exe` process.
    pub fn try_attach(&mut self) -> Result<bool> {
        match find_fm_process() {
            Ok(proc) => {
                info!("Found FM26 process with PID: {}", proc.pid);
                self.attach_pid(proc.pid)?;
                Ok(true)
            }
            Err(_) => {
                self.context.attached = false;
                self.context.pid = None;
                self.session = None;
                Ok(false)
            }
        }
    }

    /// Attaches to a specific process ID.
    pub fn attach_pid(&mut self, pid: i32) -> Result<()> {
        let session = LiveSession::new(pid, self.registry.clone())?;
        self.context.attached = true;
        self.context.pid = Some(pid);
        self.session = Some(session);
        if pid != std::process::id() as i32 { self.reset_live_state(); }
        info!("GameContextManager attached to PID {}", pid);
        Ok(())
    }

    /// Returns the current active game context.
    pub fn get_context(&self) -> ActiveContext {
        self.context.clone()
    }

    /// Returns a reference to the active game context.
    pub fn context(&self) -> &ActiveContext {
        &self.context
    }

    /// Sets the context synchronization mode (`"auto_follow"` or `"pinned"`).
    pub fn set_sync_mode(&mut self, mode: &str) {
        self.context.sync_mode = mode.to_string();
    }

    /// Sets the active UI screen (`"player"`, `"club"`, or `"squad"`).
    pub fn set_active_screen(&mut self, screen: &str) {
        self.context.active_screen = screen.to_string();
    }

    /// Binds an in-game memory pointer to a player ID.
    pub fn bind_player_address(&mut self, player_id: i32, addr: usize) {
        self.player_memory_bindings.insert(player_id, addr);
        if self.context.player.id == player_id {
            self.active_player_addr = Some(addr);
        }
    }

    /// Binds an in-game memory pointer to a club ID.
    pub fn bind_club_address(&mut self, club_id: i32, addr: usize) {
        self.club_memory_bindings.insert(club_id, addr);
        if self.context.club.club_id == club_id {
            self.active_club_addr = Some(addr);
        }
    }

    /// Returns the bound memory address for a player if one exists.
    pub fn get_player_address(&self, player_id: i32) -> Option<usize> {
        self.player_memory_bindings.get(&player_id).copied()
    }

    /// Returns the bound memory address for a club if one exists.
    pub fn get_club_address(&self, club_id: i32) -> Option<usize> {
        self.club_memory_bindings.get(&club_id).copied()
    }

    /// Binds an in-game memory pointer to a nation ID.
    pub fn bind_nation_address(&mut self, nation_id: i32, addr: usize) {
        self.nation_memory_bindings.insert(nation_id, addr);
        if self.context.nation.id == nation_id {
            self.active_nation_addr = Some(addr);
        }
    }

    /// Returns the bound memory address for a nation if one exists.
    pub fn get_nation_address(&self, nation_id: i32) -> Option<usize> {
        self.nation_memory_bindings.get(&nation_id).copied()
    }

    /// Binds an in-game memory pointer to a competition ID.
    pub fn bind_competition_address(&mut self, comp_id: i32, addr: usize) {
        self.competition_memory_bindings.insert(comp_id, addr);
        if self.context.competition.id == comp_id {
            self.active_comp_addr = Some(addr);
        }
    }

    /// Returns the bound memory address for a competition if one exists.
    pub fn get_competition_address(&self, comp_id: i32) -> Option<usize> {
        self.competition_memory_bindings.get(&comp_id).copied()
    }

    /// Selects a player by ID, populating context and syncing from memory if mapped.
    pub fn select_player(&mut self, id: i32) -> Result<Player> {
        self.failed_native_fingerprints.remove(&id);
        self.active_player_addr = self.player_memory_bindings.get(&id).copied();
        let mut player = match self.player_registry.get(&id).cloned() {
            Some(p) => p,
            None if self.session.as_ref().is_some_and(|s| s.driver.pid() != std::process::id() as i32) => {
                build_template_live_player(id, format!("Player #{}", id), String::new())
            }
            None => return Err(CoreError::PlayerNotFound(id)),
        };

        if let Some(&addr) = self.player_memory_bindings.get(&id) {
            self.active_player_addr = Some(addr);
            if !player.user_modified && !self.user_modified_players.contains(&id) {
                if let Some(session) = &self.session {
                    if session.driver.pid() == std::process::id() as i32 {
                    if let Ok(mem_player) = Player::read(&session.driver, addr, &session.registry) {
                        player.condition = mem_player.condition;
                        player.sharpness = mem_player.sharpness;
                        player.morale = mem_player.morale;
                        if mem_player.attributes != PlayerAttributes::default()
                            && mem_player.attributes.finishing != (id as u8)
                        {
                            player.attributes = mem_player.attributes;
                        }
                    }
                    }
                }
            }
        }

        if self.session.as_ref().is_some_and(|session| session.driver.pid() != std::process::id() as i32)
            && !self.native_player_records.get(&id).is_some_and(|r| r.attributes_verified())
        {
            // Discovery is read-only and attempted on selection, not every polling tick.
            self.discover_native_player_throttled(id);
        }
        if let (Some(record), Some(session)) = (self.native_player_records.get(&id), self.session.as_ref()) {
            player.positions = record.read_positions(&session.driver)?;
            player.position = player.positions.to_position_string();
            if record.attributes_verified() {
                record.refresh_player_fields(&session.driver, &mut player)?;
            }
            if player.name.is_empty() || player.name.starts_with("Player #") {
                if let Ok(native_name) = record.name(&session.driver) {
                    player.name = native_name;
                }
            }
        } else if !self.player_registry.contains_key(&id) {
            return Err(CoreError::PlayerNotFound(id));
        }

        // The selected response and subsequent delta-save baseline must be identical.
        self.publish_selected_player(&player);
        self.context.active_screen = "player".to_string();

        if player.club_id != 0 {
            if let Some(club) = self.club_registry.get(&player.club_id).cloned() {
                self.context.club = club;
            } else if self.session.as_ref().is_some_and(|s| s.driver.pid() != std::process::id() as i32) {
                if let Ok(club) = self.select_club(player.club_id) {
                    self.context.club = club;
                }
            }
        } else if !player.club_name.is_empty() {
            if let Some(club) = self
                .club_registry
                .values()
                .find(|c| c.club_name.eq_ignore_ascii_case(&player.club_name))
                .cloned()
            {
                self.context.club = club;
            }
        }

        Ok(player)
    }

    pub(crate) fn publish_selected_player(&mut self, player: &Player) {
        self.player_registry.insert(player.id, player.clone());
        self.context.player = player.clone();
    }

    /// Selects a club by ID, populating context and syncing from memory if mapped.
    pub fn select_club(&mut self, id: i32) -> Result<ClubFinances> {
        self.active_club_addr = self.club_memory_bindings.get(&id).copied();
        let club = if self.session.as_ref().is_some_and(|s| s.driver.pid() != std::process::id() as i32) {
            match self.read_club_native(id) {
                Ok(c) => c,
                Err(_) => self.club_registry.get(&id).cloned().ok_or_else(|| CoreError::InvalidValue(format!("Club with ID {} not found", id)))?,
            }
        } else {
            let mut club = self
                .club_registry
                .get(&id)
                .cloned()
                .ok_or_else(|| CoreError::InvalidValue(format!("Club with ID {} not found", id)))?;

            if let Some(&addr) = self.club_memory_bindings.get(&id) {
                self.active_club_addr = Some(addr);
                if let Some(session) = self.session.as_ref().filter(|s| s.driver.pid() == std::process::id() as i32) {
                    if let Ok(mem_club) = ClubFinances::read(&session.driver, addr, &session.registry) {
                        club.balance = mem_club.balance;
                        club.transfer_budget = mem_club.transfer_budget;
                        club.wage_budget = mem_club.wage_budget;
                    }
                }
            }
            club
        };

        self.context.club = club.clone();
        self.context.active_screen = "club".to_string();
        Ok(club)
    }

    /// Selects a nation by ID, populating context and syncing from memory if mapped.
    pub fn select_nation(&mut self, id: i32) -> Result<NationDetails> {
        self.active_nation_addr = self.nation_memory_bindings.get(&id).copied();
        let mut nation = self
            .nation_registry
            .get(&id)
            .cloned()
            .ok_or_else(|| CoreError::InvalidValue(format!("Nation with ID {} not found", id)))?;

        if let Some(&addr) = self.nation_memory_bindings.get(&id) {
            self.active_nation_addr = Some(addr);
            if let Some(session) = self.session.as_ref().filter(|s| s.driver.pid() == std::process::id() as i32) {
                if let Ok(mem_nation) = NationDetails::read(&session.driver, addr, &session.registry) {
                    nation.reputation = mem_nation.reputation;
                    nation.youth_rating = mem_nation.youth_rating;
                    nation.economic_factor = mem_nation.economic_factor;
                    nation.state_of_development = mem_nation.state_of_development;
                    nation.fa_financial_power = mem_nation.fa_financial_power;
                    nation.game_importance = mem_nation.game_importance;
                    nation.tax_rate = mem_nation.tax_rate;
                    nation.corruption_index = mem_nation.corruption_index;
                }
            }
        }

        self.context.nation = nation.clone();
        self.context.active_screen = "nation".to_string();
        Ok(nation)
    }

    /// Selects a competition by ID, populating context and syncing from memory if mapped.
    pub fn select_competition(&mut self, id: i32) -> Result<CompetitionDetails> {
        self.active_comp_addr = self.competition_memory_bindings.get(&id).copied();
        let mut comp = self
            .competition_registry
            .get(&id)
            .cloned()
            .ok_or_else(|| CoreError::InvalidValue(format!("Competition with ID {} not found", id)))?;

        if let Some(&addr) = self.competition_memory_bindings.get(&id) {
            self.active_comp_addr = Some(addr);
            if let Some(session) = self.session.as_ref().filter(|s| s.driver.pid() == std::process::id() as i32) {
                if let Ok(mem_comp) = CompetitionDetails::read(&session.driver, addr, &session.registry) {
                    comp.reputation = mem_comp.reputation;
                    comp.max_substitutes = mem_comp.max_substitutes;
                    comp.max_bench_size = mem_comp.max_bench_size;
                    comp.substitution_windows = mem_comp.substitution_windows;
                    comp.var_active = mem_comp.var_active;
                    comp.max_squad_size = mem_comp.max_squad_size;
                    comp.max_foreign_players = mem_comp.max_foreign_players;
                    comp.min_homegrown_club = mem_comp.min_homegrown_club;
                    comp.min_homegrown_nation = mem_comp.min_homegrown_nation;
                    comp.winner_prize_money = mem_comp.winner_prize_money;
                    comp.tv_rights_pool = mem_comp.tv_rights_pool;
                    comp.yellow_cards_for_ban = mem_comp.yellow_cards_for_ban;
                }
            }
        }

        self.context.competition = comp.clone();
        self.context.active_screen = "competition".to_string();
        Ok(comp)
    }

    /// Searches live and registered players, clubs, nations, and competitions matching the query string.
    pub fn search(&self, query: &str) -> SearchResults {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return SearchResults {
                query: query.to_string(),
                players: self.all_players(),
                clubs: self.all_clubs(),
                nations: self.all_nations(),
                competitions: self.all_competitions(),
            };
        }

        let mut matched_players = Vec::new();
        for p in self.player_registry.values() {
            if p.name.to_lowercase().contains(&q)
                || p.nationality.to_lowercase().contains(&q)
                || p.position.to_lowercase().contains(&q)
                || p.club_name.to_lowercase().contains(&q)
                || p.id.to_string() == q
            {
                matched_players.push(p.clone());
            }
        }
        matched_players.sort_by_key(|p| p.id);

        let mut matched_clubs = Vec::new();
        for c in self.club_registry.values() {
            if c.club_name.to_lowercase().contains(&q)
                || c.league_name.to_lowercase().contains(&q)
                || c.club_id.to_string() == q
            {
                matched_clubs.push(c.clone());
            }
        }
        matched_clubs.sort_by_key(|c| c.club_id);

        let mut matched_nations = Vec::new();
        for n in self.nation_registry.values() {
            if n.name.to_lowercase().contains(&q)
                || n.three_letter_code.to_lowercase().contains(&q)
                || n.continent.to_lowercase().contains(&q)
                || n.id.to_string() == q
            {
                matched_nations.push(n.clone());
            }
        }
        matched_nations.sort_by_key(|n| n.id);

        let mut matched_comps = Vec::new();
        for comp in self.competition_registry.values() {
            if comp.name.to_lowercase().contains(&q)
                || comp.short_name.to_lowercase().contains(&q)
                || comp.nation_name.to_lowercase().contains(&q)
                || comp.id.to_string() == q
            {
                matched_comps.push(comp.clone());
            }
        }
        matched_comps.sort_by_key(|c| c.id);

        SearchResults {
            query: query.to_string(),
            players: matched_players,
            clubs: matched_clubs,
            nations: matched_nations,
            competitions: matched_comps,
        }
    }

    /// Returns a list of all registered nations.
    pub fn all_nations(&self) -> Vec<NationDetails> {
        let mut nations: Vec<NationDetails> = self.nation_registry.values().cloned().collect();
        nations.sort_by_key(|n| n.id);
        nations
    }

    /// Returns a list of all registered competitions.
    pub fn all_competitions(&self) -> Vec<CompetitionDetails> {
        let mut comps: Vec<CompetitionDetails> = self.competition_registry.values().cloned().collect();
        comps.sort_by_key(|c| c.id);
        comps
    }

    /// Applies a predefined quick action to the active player, squad, or club.
    ///

    /// Returns a list of all registered players.
    pub fn all_players(&self) -> Vec<Player> {
        let mut players: Vec<Player> = self.player_registry.values().cloned().collect();
        if let Some(session) = &self.session {
            for player in &mut players {
                if let Some(record) = self.native_player_records.get(&player.id) {
                    match record.read_positions(&session.driver) {
                        Ok(positions) => {
                            player.position = positions.to_position_string();
                            player.positions = positions;
                        }
                        Err(_) => {
                            player.positions = PlayerPositions::unknown();
                            player.position.clear();
                        }
                    }
                }
            }
        }
        players.sort_by_key(|p| p.id);
        players
    }

    /// Returns a list of all registered clubs.
    pub fn all_clubs(&self) -> Vec<ClubFinances> {
        let mut clubs: Vec<ClubFinances> = self.club_registry.values().cloned().collect();
        clubs.sort_by_key(|c| c.club_id);
        clubs
    }

    /// Returns a reference to the active `LiveSession` if attached.
    pub fn session(&self) -> Option<&LiveSession> {
        self.session.as_ref()
    }

    /// Returns a mutable reference to the active `LiveSession` if attached.
    pub fn session_mut(&mut self) -> Option<&mut LiveSession> {
        self.session.as_mut()
    }

    /// Returns the active `OffsetsRegistry`.
    pub fn registry(&self) -> &OffsetsRegistry {
        &self.registry
    }
}
