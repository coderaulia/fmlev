use std::collections::HashMap;
use tracing::info;

use fmlev_il2cpp::OffsetsRegistry;
use fmlev_mem::find_fm_process;

use crate::error::{CoreError, Result};
use crate::models::{
    ActiveContext, ClubAffiliate, ClubFinances, CompetitionDetails, NationDetails, Player,
    PlayerAttributes, PlayerContract, PlayerPositions,
};
use crate::session::LiveSession;

pub const DEFAULT_OFFSETS_JSON: &str = include_str!("../../../offsets/v26.0.0.json");

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
    auto_attach: bool,
    pub session: Option<LiveSession>,
    native_player_records: HashMap<i32, crate::native_player::NativePlayerRecord>,
    failed_native_fingerprints: HashMap<i32, crate::live_write::NativePlayerFingerprint>,
    pub registry: OffsetsRegistry,
    pub player_registry: HashMap<i32, Player>,
    pub club_registry: HashMap<i32, ClubFinances>,
    pub nation_registry: HashMap<i32, NationDetails>,
    pub competition_registry: HashMap<i32, CompetitionDetails>,
    pub player_memory_bindings: HashMap<i32, usize>,
    pub club_memory_bindings: HashMap<i32, usize>,
    pub nation_memory_bindings: HashMap<i32, usize>,
    pub competition_memory_bindings: HashMap<i32, usize>,
    pub active_player_addr: Option<usize>,
    pub active_club_addr: Option<usize>,
    pub active_nation_addr: Option<usize>,
    pub active_comp_addr: Option<usize>,
    pub user_modified_players: std::collections::HashSet<i32>,
    pub user_modified_clubs: std::collections::HashSet<i32>,
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
            registry,
            player_registry,
            club_registry,
            nation_registry,
            competition_registry,
            player_memory_bindings: HashMap::new(),
            club_memory_bindings: HashMap::new(),
            nation_memory_bindings: HashMap::new(),
            competition_memory_bindings: HashMap::new(),
            active_player_addr: None,
            active_club_addr: None,
            active_nation_addr: None,
            active_comp_addr: None,
            user_modified_players: std::collections::HashSet::new(),
            user_modified_clubs: std::collections::HashSet::new(),
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
    fn reset_live_state(&mut self) {
        self.native_player_records.clear();
        self.failed_native_fingerprints.clear();
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
        let mut player = self
            .player_registry
            .get(&id)
            .cloned()
            .ok_or(CoreError::PlayerNotFound(id))?;

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
            && !self.native_player_records.contains_key(&id)
        {
            // Discovery is read-only and attempted on selection, not every polling tick.
            // An unopened profile may not yet expose the complete validation fingerprint.
            let _ = self.verified_native_player(id);
        }
        if let (Some(record), Some(session)) = (self.native_player_records.get(&id), self.session.as_ref()) {
            player.positions = record.read_positions(&session.driver)?;
            player.position = player.positions.to_position_string();
            if record.attributes_verified() { record.refresh_player_fields(&session.driver, &mut player)?; }
        }

        // The selected response and subsequent delta-save baseline must be identical.
        self.publish_selected_player(&player);
        self.context.active_screen = "player".to_string();

        if player.club_id != 0 {
            if let Some(club) = self.club_registry.get(&player.club_id).cloned() {
                self.context.club = club;
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

    fn publish_selected_player(&mut self, player: &Player) {
        self.player_registry.insert(player.id, player.clone());
        self.context.player = player.clone();
    }

    /// Selects a club by ID, populating context and syncing from memory if mapped.
    pub fn select_club(&mut self, id: i32) -> Result<ClubFinances> {
        self.active_club_addr = self.club_memory_bindings.get(&id).copied();
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

    /// The configured entity offsets describe test records, not FM26's native records.
    /// Managed PersonReference/ClubReference objects must never be treated as those records.
    fn ensure_record_writes_supported(&self) -> Result<()> {
        if let Some(session) = &self.session {
            if session.driver.pid() != std::process::id() as i32
                || session.bases.game_assembly_base.is_some()
            {
                return Err(CoreError::InvalidValue(
                    "Live writes are unavailable: the authoritative FM26 native record layout has not been verified. UI binding values are read-only and do not persist simulation edits.".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Updates local state only after the explicitly bound record write succeeds.
    /// Merge only explicit UI edits into the latest server snapshot. A stale name,
    /// condition, contract, or recommendation must not hitchhike on an attribute Save.
    pub fn update_player_fields(&mut self, player: Player, fields: Option<&[String]>) -> Result<()> {
        let Some(fields) = fields else { return self.update_player(player); };
        let previous = self.player_registry.get(&player.id).ok_or(CoreError::PlayerNotFound(player.id))?;
        let mut target = serde_json::to_value(previous).unwrap();
        let source = serde_json::to_value(&player).unwrap();
        for field in fields {
            let parts: Vec<_> = field.split('.').collect();
            if parts.len() > 2 || matches!(parts[0], "id" | "user_modified") {
                return Err(CoreError::InvalidValue(format!("Invalid player edit field: {field}")));
            }
            let mut dst = &mut target;
            let mut src = &source;
            for part in &parts {
                dst = dst.get_mut(*part).ok_or_else(|| CoreError::InvalidValue(format!("Unknown player edit field: {field}")))?;
                src = src.get(*part).ok_or_else(|| CoreError::InvalidValue(format!("Missing player edit field: {field}")))?;
            }
            *dst = src.clone();
        }
        let merged = serde_json::from_value(target).map_err(|error| CoreError::InvalidValue(error.to_string()))?;
        self.update_player(merged)
    }

    /// None denotes offline fixtures; a live session advertises only validated fields.
    pub fn player_writable_fields(&self) -> Option<Vec<String>> {
        let session = self.session.as_ref()?;
        if session.driver.pid() == std::process::id() as i32 && session.bases.game_assembly_base.is_none() { return None; }
        let mut fields = Vec::new();
        if let Some(record) = self.native_player_records.get(&self.context.player.id) {
            fields.push("positions".to_string());
            if record.attributes_verified() {
                fields.extend(crate::native_player::native_attributes().map(|field| format!("attributes.{}", field.name)));
                fields.extend(["current_ability", "potential_ability"].map(str::to_string));
                if record.has_contract(&session.driver) { fields.extend(["contract.wage_per_week", "contract.expiry_year"].map(str::to_string)); }
            }
        }
        Some(fields)
    }

    pub fn discover_loaded_players(&mut self) -> Result<usize> {
        let seed = self.verified_native_player(self.context.player.id)?;
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live game session".into()))?;
        let records = seed.discover_loaded(&session.driver)?;
        for record in records {
            let id = record.index();
            let name = record.name(&session.driver)?;
            let player = self.player_registry.entry(id).or_insert_with(|| build_template_live_player(id, name, String::new()));
            player.positions = record.read_positions(&session.driver)?;
            player.position = player.positions.to_position_string();
            record.refresh_attributes(&session.driver, &mut player.attributes)?;
            self.native_player_records.insert(id, record);
            self.failed_native_fingerprints.remove(&id);
        }
        Ok(self.native_player_records.len())
    }

    pub fn update_player(&mut self, mut player: Player) -> Result<()> {
        if self.session.as_ref().is_some_and(|session|
            session.driver.pid() != std::process::id() as i32 || session.bases.game_assembly_base.is_some())
        {
            return self.update_live_player_positions(player);
        }
        self.ensure_record_writes_supported()?;
        let player_id = player.id;
        if let Some(session) = &self.session {
            let addr = self.player_memory_bindings.get(&player_id).copied()
                .ok_or_else(|| CoreError::InvalidValue(format!("No writable record bound for player {}", player_id)))?;
            player.write(&session.driver, addr, &session.registry)?;
        }
        player.user_modified = true;
        self.user_modified_players.insert(player_id);
        self.context.player = player.clone();
        self.player_registry.insert(player_id, player);
        self.active_player_addr = self.player_memory_bindings.get(&player_id).copied();
        Ok(())
    }

    fn verified_native_player(&mut self, id: i32) -> Result<crate::native_player::NativePlayerRecord> {
        if let Some(mut record) = self.native_player_records.get(&id).cloned() {
            if !record.attributes_verified() {
                if let (Some(session), Some(player)) = (self.session.as_ref(), self.player_registry.get(&id)) {
                    let _ = record.verify_attributes(&session.driver, &player.attributes);
                    self.native_player_records.insert(id, record.clone());
                }
            }
            return Ok(record);
        }
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;
        let subsystem = resolve_binding_subsystem(session)
            .ok_or_else(|| CoreError::InvalidValue("Live player bindings are unavailable; open this player's profile in FM26".into()))?;
        let fingerprint = crate::live_write::read_person_native_fingerprint(&session.driver, subsystem, id)?;
        if self.failed_native_fingerprints.get(&id) == Some(&fingerprint) {
            return Err(CoreError::InvalidValue("Native player could not be verified for these bindings; reopen the profile or reconnect before retrying".into()));
        }
        let mut record = match crate::native_player::NativePlayerRecord::discover(
            &session.driver, id, fingerprint.unique_id, fingerprint.aptitudes,
            fingerprint.left_foot, fingerprint.right_foot,
        ) {
            Ok(record) => record,
            Err(error) => {
                self.failed_native_fingerprints.insert(id, fingerprint);
                return Err(error);
            }
        };
        self.failed_native_fingerprints.remove(&id);
        if let Some(player) = self.player_registry.get(&id) {
            let _ = record.verify_attributes(&session.driver, &player.attributes);
        }
        self.native_player_records.insert(id, record.clone());
        Ok(record)
    }

    fn update_live_player_positions(&mut self, player: Player) -> Result<()> {
        self.save_live_player_positions(player, None)
    }

    fn save_live_player_positions(&mut self, mut player: Player, preset: Option<PlayerPositions>) -> Result<()> {
        let previous = self.player_registry.get(&player.id)
            .ok_or(CoreError::PlayerNotFound(player.id))?.clone();
        let unsupported = unsupported_live_player_changes(&previous, &player);
        if !unsupported.is_empty() {
            return Err(CoreError::InvalidValue(format!(
                "No verified native write path for: {}. No fields were written. Native player attributes, positions, feet, CA/PA and validated wage/expiry fields can be saved.", unsupported.join(", ")
            )));
        }
        let record = self.verified_native_player(player.id)?;
        let session = self.session.as_ref().unwrap();
        if let Some(preset) = preset { player.positions = preset; }
        record.write_player_edits(&session.driver, &previous, &player)?;
        player.positions = record.read_positions(&session.driver)?;
        if record.attributes_verified() { record.refresh_player_fields(&session.driver, &mut player)?; }
        player.position = player.positions.to_position_string();
        // Native edits remain live: subsequent polling must never freeze the cached values.
        player.user_modified = false;
        self.user_modified_players.remove(&player.id);
        self.player_registry.insert(player.id, player.clone());
        self.context.player = player;
        self.active_player_addr = None;
        Ok(())
    }

    pub fn update_club(&mut self, club: ClubFinances) -> Result<()> {
        self.ensure_record_writes_supported()?;
        let club_id = club.club_id;
        if let Some(session) = &self.session {
            let addr = self.club_memory_bindings.get(&club_id).copied()
                .ok_or_else(|| CoreError::InvalidValue(format!("No writable record bound for club {}", club_id)))?;
            club.write(&session.driver, addr, &session.registry)?;
        }
        self.user_modified_clubs.insert(club_id);
        self.context.club = club.clone();
        self.club_registry.insert(club_id, club);
        self.active_club_addr = self.club_memory_bindings.get(&club_id).copied();
        Ok(())
    }

    pub fn update_nation(&mut self, nation: NationDetails) -> Result<()> {
        self.ensure_record_writes_supported()?;
        let nation_id = nation.id;
        if let Some(session) = &self.session {
            let addr = self.nation_memory_bindings.get(&nation_id).copied()
                .ok_or_else(|| CoreError::InvalidValue(format!("No writable record bound for nation {}", nation_id)))?;
            nation.write(&session.driver, addr, &session.registry)?;
        }
        self.context.nation = nation.clone();
        self.nation_registry.insert(nation_id, nation);
        self.active_nation_addr = self.nation_memory_bindings.get(&nation_id).copied();
        Ok(())
    }

    pub fn update_competition(&mut self, comp: CompetitionDetails) -> Result<()> {
        self.ensure_record_writes_supported()?;
        let comp_id = comp.id;
        if let Some(session) = &self.session {
            let addr = self.competition_memory_bindings.get(&comp_id).copied()
                .ok_or_else(|| CoreError::InvalidValue(format!("No writable record bound for competition {}", comp_id)))?;
            comp.write(&session.driver, addr, &session.registry)?;
        }
        self.context.competition = comp.clone();
        self.competition_registry.insert(comp_id, comp);
        self.active_comp_addr = self.competition_memory_bindings.get(&comp_id).copied();
        Ok(())
    }

    /// Adds or replaces an affiliate club for a given club ID.
    pub fn add_club_affiliate(&mut self, club_id: i32, affiliate: ClubAffiliate) -> Result<ClubFinances> {
        let mut club = self
            .club_registry
            .get(&club_id)
            .cloned()
            .ok_or_else(|| CoreError::InvalidValue(format!("Club with ID {} not found", club_id)))?;

        club.affiliates.retain(|a| a.affiliate_club_id != affiliate.affiliate_club_id);
        club.affiliates.push(affiliate);

        self.update_club(club.clone())?;
        Ok(club)
    }

    /// Removes an affiliate club relationship.
    pub fn remove_club_affiliate(&mut self, club_id: i32, affiliate_id: i32) -> Result<ClubFinances> {
        let mut club = self
            .club_registry
            .get(&club_id)
            .cloned()
            .ok_or_else(|| CoreError::InvalidValue(format!("Club with ID {} not found", club_id)))?;

        club.affiliates.retain(|a| a.affiliate_club_id != affiliate_id);
        self.update_club(club.clone())?;
        Ok(club)
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
    /// Supported actions:
    /// - `"heal_player"`: Restores condition & sharpness to 100%, clears injury.
    /// - `"max_morale"`: Sets morale to 100 ("Superb"), happiness to "Delighted".
    /// - `"wonderkid"`: PA 200, CA >= 165, age <= 18, high determination/ambition.
    /// - `"max_attributes"`: All 20s for technical, mental, physical, goalkeeping, hidden attributes.
    /// - `"extend_contract"`: Adds 5 years, sets status to "Star Player", happiness "Delighted".
    /// - `"heal_squad"`: Heals all players in the active club.
    /// - `"boost_budget_50m"`: Adds 50M balance & transfer budget, 500k wage budget.
    /// - `"sugar_daddy"`: 1B balance, 500M transfer budget, 10M wage, 10k reputation.
    /// - `"wipe_debt"`: Clears debt, guarantees healthy balance and wage coverage.
    /// - `"max_facilities"`: Sets training, youth facilities, recruitment, coaching to 20.
    pub fn apply_quick_action(&mut self, action_name: &str) -> Result<()> {
        let normalized = action_name.trim().to_lowercase().replace([' ', '-'], "_");
        if self.session.as_ref().is_some_and(|session|
            session.driver.pid() != std::process::id() as i32 || session.bases.game_assembly_base.is_some())
            && matches!(normalized.as_str(), "max_outfield_attributes" | "boost_all_2" | "max_attributes" | "max_stats" | "goat")
        {
            let record = self.verified_native_player(self.context.player.id)?;
            let mut player = self.context.player.clone();
            record.refresh_player_fields(&self.session.as_ref().unwrap().driver, &mut player)?;
            player.positions = record.read_positions(&self.session.as_ref().unwrap().driver)?;
            player.position = player.positions.to_position_string();
            // Presets compare against fresh native values, including fields that
            // happen to equal an old cached target but have changed in the game.
            self.player_registry.insert(player.id, player.clone());
            for field in crate::native_player::native_attributes() {
                if normalized == "max_outfield_attributes" && !crate::native_player::NATIVE_OUTFIELD_ATTRIBUTES.iter().any(|f| f.name == field.name) { continue; }
                let value = if matches!(field.name, "injury_proneness" | "eccentricity" | "dirtiness" | "controversy") { 1 }
                    else if normalized == "boost_all_2" { (field.get)(&player.attributes).saturating_add(2).min(20) } else { 20 };
                (field.set)(&mut player.attributes, value);
            }
            if matches!(normalized.as_str(), "max_attributes" | "max_stats" | "goat") {
                player.current_ability = 200;
                player.potential_ability = 200;
            }
            return self.save_live_player_positions(player, None);
        }
        if self.session.as_ref().is_some_and(|session|
            session.driver.pid() != std::process::id() as i32 || session.bases.game_assembly_base.is_some())
            && matches!(normalized.as_str(),
                "all_positions_20" | "all_positions" | "total_football"
                | "both_feet_20" | "either_foot" | "both_feet"
                | "pure_striker" | "universal_winger" | "midfield_maestro"
                | "complete_defender" | "goalkeeper")
        {
            // Reuse the preset calculations in an isolated offline proposal.
            // The native path writes only its advertised positions/feet fields.
            let record = self.verified_native_player(self.context.player.id)?;
            let native_positions = record.read_positions(&self.session.as_ref().unwrap().driver)?;
            if let Some(previous) = self.player_registry.get_mut(&self.context.player.id) {
                previous.positions = native_positions.clone();
                previous.position = native_positions.to_position_string();
            }
            let mut proposal = Self::new_offline();
            proposal.context.player = self.context.player.clone();
            proposal.context.player.positions = native_positions;
            proposal.apply_quick_action_inner(&normalized)?;
            let mut requested = self.context.player.clone();
            requested.positions = proposal.context.player.positions;
            requested.position = requested.positions.to_position_string();
            let positions = requested.positions.clone();
            return self.save_live_player_positions(requested, Some(positions));
        }
        self.ensure_record_writes_supported()?;
        let saved_context = self.context.clone();
        let saved_players = self.player_registry.clone();
        let saved_clubs = self.club_registry.clone();
        let saved_nations = self.nation_registry.clone();
        let saved_competitions = self.competition_registry.clone();
        let saved_modified_players = self.user_modified_players.clone();
        let saved_modified_clubs = self.user_modified_clubs.clone();
        let result = self.apply_quick_action_inner(action_name);
        if result.is_err() {
            self.context = saved_context;
            self.player_registry = saved_players;
            self.club_registry = saved_clubs;
            self.nation_registry = saved_nations;
            self.competition_registry = saved_competitions;
            self.user_modified_players = saved_modified_players;
            self.user_modified_clubs = saved_modified_clubs;
        }
        result
    }

    fn apply_quick_action_inner(&mut self, action_name: &str) -> Result<()> {
        let normalized = action_name.trim().to_lowercase().replace([' ', '-'], "_");
        let is_player_action = matches!(
            normalized.as_str(),
            "heal_player" | "heal" | "max_morale" | "morale" | "boost_morale" | "wonderkid"
                | "max_attributes" | "max_stats" | "goat" | "extend_contract" | "contract"
                | "all_positions_20" | "all_positions" | "total_football"
                | "both_feet_20" | "either_foot" | "both_feet"
                | "pure_striker" | "universal_winger" | "midfield_maestro" | "complete_defender" | "goalkeeper"
                | "heal_squad"
        );
        if is_player_action {
            self.context.player.user_modified = true;
            self.user_modified_players.insert(self.context.player.id);
        }

        let is_club_action = matches!(
            normalized.as_str(),
            "boost_budget_50m" | "boost_budget" | "50m" | "sugar_daddy" | "wipe_debt" | "clear_debt"
                | "max_facilities" | "facilities" | "club_max_facilities"
        );
        if is_club_action {
            self.user_modified_clubs.insert(self.context.club.club_id);
        }

        if self.session.is_some() {
            let bound = if is_player_action {
                self.player_memory_bindings.contains_key(&self.context.player.id)
            } else if is_club_action {
                self.club_memory_bindings.contains_key(&self.context.club.club_id)
            } else if matches!(normalized.as_str(), "golden_generation" | "nation_golden_generation") {
                self.nation_memory_bindings.contains_key(&self.context.nation.id)
            } else if matches!(normalized.as_str(), "relax_rules" | "competition_relax_rules") {
                self.competition_memory_bindings.contains_key(&self.context.competition.id)
            } else { true };
            if !bound {
                return Err(CoreError::InvalidValue("No writable record bound for this action".to_string()));
            }
        }

        match normalized.as_str() {
            "heal_player" | "heal" => {
                self.context.player.condition = 10000;
                self.context.player.sharpness = 10000;
                self.context.player.is_injured = false;
                self.player_registry
                    .insert(self.context.player.id, self.context.player.clone());

                if let Some(session) = &self.session {
                    let bound_addr = self
                        .active_player_addr
                        .or_else(|| self.player_memory_bindings.get(&self.context.player.id).copied());
                    if let Some(addr) = bound_addr {
                        session.heal_player(addr, None)?;
                    }
                }
                info!("Applied quick action: heal_player (Player ID: {})", self.context.player.id);
            }
            "max_morale" | "morale" | "boost_morale" => {
                self.context.player.morale = 100;
                self.context.player.morale_label = "Superb".to_string();
                self.context.player.contract.happiness = "Delighted".to_string();
                self.player_registry
                    .insert(self.context.player.id, self.context.player.clone());

                if let Some(session) = &self.session {
                    let bound_addr = self
                        .active_player_addr
                        .or_else(|| self.player_memory_bindings.get(&self.context.player.id).copied());
                    if let Some(addr) = bound_addr {
                        session.boost_morale(addr, None)?;
                    }
                }
                info!("Applied quick action: max_morale (Player ID: {})", self.context.player.id);
            }
            "wonderkid" => {
                if self.context.player.current_ability < 165 {
                    self.context.player.current_ability = 165;
                }
                self.context.player.potential_ability = 200;
                if self.context.player.age > 18 {
                    self.context.player.age = 18;
                }
                self.context.player.contract.squad_status = "Breakthrough Prospect".to_string();
                self.context.player.attributes.ambition = 20;
                self.context.player.attributes.professionalism = 20;
                self.context.player.attributes.determination = 20;
                self.context.player.attributes.natural_fitness = 18;
                self.player_registry
                    .insert(self.context.player.id, self.context.player.clone());

                if let Some(session) = &self.session {
                    let bound_addr = self
                        .active_player_addr
                        .or_else(|| self.player_memory_bindings.get(&self.context.player.id).copied());
                    if let Some(addr) = bound_addr {
                        self.context.player.write(&session.driver, addr, &session.registry)?;
                    }
                }
                info!("Applied quick action: wonderkid (Player ID: {})", self.context.player.id);
            }
            "max_outfield_attributes" => {
                for field in crate::native_player::native_attributes() { (field.set)(&mut self.context.player.attributes, 20); }
                self.player_registry.insert(self.context.player.id, self.context.player.clone());
            }
            "max_attributes" | "max_stats" | "goat" => {
                self.context.player.current_ability = 200;
                self.context.player.potential_ability = 200;
                let a = &mut self.context.player.attributes;
                a.corners = 20; a.crossing = 20; a.dribbling = 20; a.finishing = 20;
                a.first_touch = 20; a.free_kicks = 20; a.heading = 20; a.long_shots = 20;
                a.passing = 20; a.penalty_taking = 20; a.tackling = 20; a.technique = 20;
                a.aggression = 20; a.anticipation = 20; a.bravery = 20; a.composure = 20;
                a.concentration = 20; a.decisions = 20; a.determination = 20; a.flair = 20;
                a.leadership = 20; a.off_the_ball = 20; a.positioning = 20; a.teamwork = 20;
                a.vision = 20; a.work_rate = 20;
                a.acceleration = 20; a.agility = 20; a.balance = 20; a.jumping_reach = 20;
                a.natural_fitness = 20; a.pace = 20; a.stamina = 20; a.strength = 20;
                a.consistency = 20; a.important_matches = 20; a.injury_proneness = 1;
                a.versatility = 20; a.adaptability = 20; a.ambition = 20; a.loyalty = 20;
                a.pressure = 20; a.professionalism = 20; a.sportsmanship = 20; a.temperament = 20;
                a.aerial_reach = 20; a.command_of_area = 20; a.communication = 20;
                a.eccentricity = 1; a.handling = 20; a.kicking = 20; a.one_on_ones = 20;
                a.reflexes = 20; a.rushing_out = 20; a.throwing = 20;

                self.player_registry
                    .insert(self.context.player.id, self.context.player.clone());

                if let Some(session) = &self.session {
                    let bound_addr = self
                        .active_player_addr
                        .or_else(|| self.player_memory_bindings.get(&self.context.player.id).copied());
                    if let Some(addr) = bound_addr {
                        self.context.player.write(&session.driver, addr, &session.registry)?;
                    }
                }
                info!("Applied quick action: max_attributes (Player ID: {})", self.context.player.id);
            }
            "extend_contract" | "contract" => {
                let current_year = self.context.player.contract.expiry_year;
                self.context.player.contract.expiry_year = current_year.max(2026) + 5;
                self.context.player.contract.squad_status = "Star Player".to_string();
                self.context.player.contract.happiness = "Delighted".to_string();
                self.context.player.contract.release_clause = 0;
                self.player_registry
                    .insert(self.context.player.id, self.context.player.clone());

                if let Some(session) = &self.session {
                    let bound_addr = self
                        .active_player_addr
                        .or_else(|| self.player_memory_bindings.get(&self.context.player.id).copied());
                    if let Some(addr) = bound_addr {
                        self.context.player.write(&session.driver, addr, &session.registry)?;
                    }
                }
                info!("Applied quick action: extend_contract (Player ID: {})", self.context.player.id);
            }
            "heal_squad" => {
                let target_club_id = self.context.club.club_id;
                self.context.player.condition = 10000;
                self.context.player.sharpness = 10000;
                self.context.player.is_injured = false;
                self.context.player.is_suspended = false;

                for player in self.player_registry.values_mut() {
                    if target_club_id == 0 || player.club_id == target_club_id {
                        player.condition = 10000;
                        player.sharpness = 10000;
                        player.is_injured = false;
                        player.is_suspended = false;
                    }
                }

                if let Some(session) = &self.session {
                    for (pid, &addr) in &self.player_memory_bindings {
                        if let Some(player) = self.player_registry.get(pid) {
                            if target_club_id == 0 || player.club_id == target_club_id {
                                session.heal_player(addr, None)?;
                            }
                        }
                    }
                }
                info!("Applied quick action: heal_squad for club ID {}", target_club_id);
            }
            "boost_budget_50m" | "boost_budget" | "50m" => {
                self.context.club.balance += 50_000_000;
                self.context.club.transfer_budget += 50_000_000;
                self.context.club.wage_budget += 500_000;
                self.club_registry
                    .insert(self.context.club.club_id, self.context.club.clone());

                if let Some(session) = &self.session {
                    let bound_addr = self
                        .active_club_addr
                        .or_else(|| self.club_memory_bindings.get(&self.context.club.club_id).copied());
                    if let Some(addr) = bound_addr {
                        self.context.club.write(&session.driver, addr, &session.registry)?;
                    }
                }
                info!("Applied quick action: boost_budget_50m for club ID {}", self.context.club.club_id);
            }
            "sugar_daddy" => {
                self.context.club.balance = 1_000_000_000;
                self.context.club.transfer_budget = 500_000_000;
                self.context.club.wage_budget = 10_000_000;
                self.context.club.reputation = 10000;
                self.club_registry
                    .insert(self.context.club.club_id, self.context.club.clone());

                if let Some(session) = &self.session {
                    let bound_addr = self
                        .active_club_addr
                        .or_else(|| self.club_memory_bindings.get(&self.context.club.club_id).copied());
                    if let Some(addr) = bound_addr {
                        self.context.club.write(&session.driver, addr, &session.registry)?;
                    }
                }
                info!("Applied quick action: sugar_daddy for club ID {}", self.context.club.club_id);
            }
            "wipe_debt" | "clear_debt" => {
                if self.context.club.balance < 150_000_000 {
                    self.context.club.balance = 150_000_000;
                }
                if self.context.club.wage_used > self.context.club.wage_budget {
                    self.context.club.wage_budget = self.context.club.wage_used + 500_000;
                }
                self.club_registry
                    .insert(self.context.club.club_id, self.context.club.clone());

                if let Some(session) = &self.session {
                    let bound_addr = self
                        .active_club_addr
                        .or_else(|| self.club_memory_bindings.get(&self.context.club.club_id).copied());
                    if let Some(addr) = bound_addr {
                        self.context.club.write(&session.driver, addr, &session.registry)?;
                    }
                }
                info!("Applied quick action: wipe_debt for club ID {}", self.context.club.club_id);
            }
            "max_facilities" | "facilities" | "club_max_facilities" => {
                self.context.club.max_all_facilities();
                self.club_registry
                    .insert(self.context.club.club_id, self.context.club.clone());

                if let Some(session) = &self.session {
                    let bound_addr = self
                        .active_club_addr
                        .or_else(|| self.club_memory_bindings.get(&self.context.club.club_id).copied());
                    if let Some(addr) = bound_addr {
                        self.context.club.write(&session.driver, addr, &session.registry)?;
                    }
                }
                info!("Applied quick action: max_facilities for club ID {}", self.context.club.club_id);
            }
            "golden_generation" | "nation_golden_generation" => {
                self.context.nation.golden_generation();
                self.nation_registry
                    .insert(self.context.nation.id, self.context.nation.clone());

                if let Some(session) = &self.session {
                    let bound_addr = self
                        .active_nation_addr
                        .or_else(|| self.nation_memory_bindings.get(&self.context.nation.id).copied());
                    if let Some(addr) = bound_addr {
                        self.context.nation.write(&session.driver, addr, &session.registry)?;
                    }
                }
                info!("Applied quick action: golden_generation for nation ID {}", self.context.nation.id);
            }
            "relax_rules" | "competition_relax_rules" => {
                self.context.competition.relax_rules();
                self.competition_registry
                    .insert(self.context.competition.id, self.context.competition.clone());

                if let Some(session) = &self.session {
                    let bound_addr = self
                        .active_comp_addr
                        .or_else(|| self.competition_memory_bindings.get(&self.context.competition.id).copied());
                    if let Some(addr) = bound_addr {
                        self.context.competition.write(&session.driver, addr, &session.registry)?;
                    }
                }
                info!("Applied quick action: relax_rules for competition ID {}", self.context.competition.id);
            }
            "all_positions_20" | "all_positions" | "total_football" => {
                let pos = &mut self.context.player.positions;
                pos.dl = 20; pos.dc = 20; pos.dr = 20;
                pos.wbl = 20; pos.wbr = 20;
                pos.dm = 20;
                pos.ml = 20; pos.mc = 20; pos.mr = 20;
                pos.aml = 20; pos.amc = 20; pos.amr = 20;
                pos.st = 20;
                pos.left_foot = 20;
                pos.right_foot = 20;
                self.context.player.position = "D/WB/DM/M/AM (RLC), ST (C)".to_string();
                self.context.player.attributes.versatility = 20;
                self.player_registry
                    .insert(self.context.player.id, self.context.player.clone());
                info!("Applied quick action: all_positions_20 (Player ID: {})", self.context.player.id);
            }
            "both_feet_20" | "either_foot" | "both_feet" => {
                self.context.player.positions.left_foot = 20;
                self.context.player.positions.right_foot = 20;
                self.player_registry
                    .insert(self.context.player.id, self.context.player.clone());
                info!("Applied quick action: both_feet_20 (Player ID: {})", self.context.player.id);
            }
            "pure_striker" => {
                let pos = &mut self.context.player.positions;
                pos.st = 20;
                pos.aml = 14; pos.amc = 14; pos.amr = 14;
                self.context.player.position = pos.to_position_string();
                self.player_registry
                    .insert(self.context.player.id, self.context.player.clone());
                info!("Applied quick action: pure_striker (Player ID: {})", self.context.player.id);
            }
            "universal_winger" => {
                let pos = &mut self.context.player.positions;
                pos.aml = 20; pos.amr = 20; pos.ml = 20; pos.mr = 20; pos.wbl = 20; pos.wbr = 20;
                self.context.player.position = pos.to_position_string();
                self.player_registry
                    .insert(self.context.player.id, self.context.player.clone());
                info!("Applied quick action: universal_winger (Player ID: {})", self.context.player.id);
            }
            "midfield_maestro" => {
                let pos = &mut self.context.player.positions;
                pos.mc = 20; pos.dm = 20; pos.amc = 20; pos.ml = 16; pos.mr = 16;
                self.context.player.position = pos.to_position_string();
                self.player_registry
                    .insert(self.context.player.id, self.context.player.clone());
                info!("Applied quick action: midfield_maestro (Player ID: {})", self.context.player.id);
            }
            "complete_defender" => {
                let pos = &mut self.context.player.positions;
                pos.dc = 20; pos.dl = 20; pos.dr = 20; pos.wbl = 20; pos.wbr = 20; pos.dm = 16;
                self.context.player.position = pos.to_position_string();
                self.player_registry
                    .insert(self.context.player.id, self.context.player.clone());
                info!("Applied quick action: complete_defender (Player ID: {})", self.context.player.id);
            }
            "goalkeeper" => {
                let left = self.context.player.positions.left_foot;
                let right = self.context.player.positions.right_foot;
                let mut pos = PlayerPositions::default();
                pos.gk = 20;
                pos.mc = 1;
                pos.left_foot = left;
                pos.right_foot = right;
                self.context.player.positions = pos;
                self.context.player.position = "GK".to_string();
                self.player_registry
                    .insert(self.context.player.id, self.context.player.clone());
                info!("Applied quick action: goalkeeper (Player ID: {})", self.context.player.id);
            }
            _ => {
                return Err(CoreError::InvalidValue(format!("Unknown quick action: '{}'", action_name)));
            }
        }
        Ok(())
    }

    /// Reads live IL2CPP singletons (PanelManager, Bindings, GlobalGameModule) to detect active
    /// in-game screen/context changes and syncs bound memory references.
    pub fn sync_live_context(&mut self) -> Result<()> {
        if self.session.is_none() {
            if !self.auto_attach { return Ok(()); }
            let _ = self.try_attach();
            if self.session.is_none() {
                return Ok(());
            }
        }

        let session = self.session.as_ref().unwrap();
        let base = match session.bases.game_assembly_base {
            Some(b) => b,
            None => return Ok(()),
        };

        let mut detected_screen: Option<String> = None;
        let mut detected_player_id: Option<(i32, usize)> = None;
        let mut detected_club_id: Option<(i32, usize)> = None;

        // 1. Inspect PanelManager singleton
        if let Ok(panel_mgr_singleton) = session.registry.get_singleton("PanelManager") {
            if let Ok(panel_mgr_ptr) = fmlev_il2cpp::read_static_singleton(&session.driver, base, panel_mgr_singleton) {
                if panel_mgr_ptr != 0 {
                    let state_offset = session
                        .registry
                        .get_field_offset("PanelManager", "state")
                        .unwrap_or(0xe8);
                    if let Ok(panel_state) = session.driver.read::<i32>(panel_mgr_ptr + state_offset) {
                        match panel_state {
                            1 => detected_screen = Some("player".to_string()),
                            2 => detected_screen = Some("club".to_string()),
                            3 => detected_screen = Some("squad".to_string()),
                            _ => {}
                        }
                    }
                }
            }
        }

        // 2. Inspect Bindings singleton to detect active PersonReference / ClubReference
        if let Ok(bindings_singleton) = session.registry.get_singleton("Bindings") {
            if let Ok(bindings_ptr) = fmlev_il2cpp::read_static_singleton(&session.driver, base, bindings_singleton) {
                if bindings_ptr != 0 {
                    let ctx_mapping_offset = session
                        .registry
                        .get_field_offset("Bindings", "m_contextMapping")
                        .unwrap_or(0x88);
                    if let Ok(active_ref_ptr) = session.driver.read_ptr(bindings_ptr + ctx_mapping_offset) {
                        if active_ref_ptr != 0 {
                            let id_offset = session
                                .registry
                                .get_field_offset("PersonReference", "m_idHash")
                                .unwrap_or(0x18);
                            if let Ok(id) = session.driver.read::<i32>(active_ref_ptr + id_offset) {
                                if id > 0 {
                                    if self.player_registry.contains_key(&id) {
                                        detected_player_id = Some((id, active_ref_ptr));
                                    } else if self.club_registry.contains_key(&id) {
                                        detected_club_id = Some((id, active_ref_ptr));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. Inspect GlobalGameModule singleton
        if let Ok(global_module_singleton) = session.registry.get_singleton("GlobalGameModule") {
            let _ = fmlev_il2cpp::read_static_singleton(&session.driver, base, global_module_singleton);
        }

        // 3b. Inspect PanelHistoryManagerModule & GameSubsystems (BindingSubsystem) for real-time FM26 UI bindings
        let mut live_discovered_players: Vec<Player> = Vec::new();
        let mut panel_history_person_id: Option<i32> = None;
        let mut panel_history_club_id: Option<i32> = None;

        if let Ok(phmm_singleton) = session.registry.get_singleton("PanelHistoryManagerModule") {
            if let Ok(phmm_ptr) = fmlev_il2cpp::read_static_singleton(&session.driver, base, phmm_singleton) {
                if phmm_ptr > 0x10000 {
                    if let Ok(phm_ptr) = session.driver.read_ptr(phmm_ptr + 0x40) {
                        if phm_ptr > 0x10000 {
                            let hist_list = session.driver.read_ptr(phm_ptr + 0x10).unwrap_or(0);
                            let curr_idx = session.driver.read::<i32>(phm_ptr + 0x18).unwrap_or(-1);
                            if hist_list > 0x10000 {
                                let hist_items = session.driver.read_ptr(hist_list + 0x10).unwrap_or(0);
                                let hist_size = session.driver.read::<i32>(hist_list + 0x18).unwrap_or(0);
                                if hist_items > 0x10000 && curr_idx >= 0 && curr_idx < hist_size && hist_size <= 256 {
                                    let node_addr = hist_items + 0x20 + (curr_idx as usize) * 0x48;
                                    if let Ok(name_ptr) = session.driver.read_ptr(node_addr + 0x10) {
                                        if let Ok(panel_name) = fmlev_il2cpp::Il2CppString::read(&session.driver, name_ptr) {
                                            let lower = panel_name.to_lowercase();
                                            if lower.contains("player") || lower.contains("person") {
                                                detected_screen = Some("player".to_string());
                                            } else if lower.contains("club") || lower.contains("finance") {
                                                detected_screen = Some("club".to_string());
                                            } else if lower.contains("squad") || lower.contains("tactic") {
                                                detected_screen = Some("squad".to_string());
                                            }
                                        }
                                    }
                                    if let Ok(args_list) = session.driver.read_ptr(node_addr + 0x18) {
                                        if args_list > 0x10000 {
                                            let arg_items = session.driver.read_ptr(args_list + 0x10).unwrap_or(0);
                                            let arg_count = session.driver.read::<i32>(args_list + 0x18).unwrap_or(0);
                                            if arg_items > 0x10000 && arg_count > 0 && arg_count <= 16 {
                                                for j in 0..(arg_count as usize) {
                                                    let arg_addr = arg_items + 0x20 + j * 0x30;
                                                    if let Ok(val_tv_ptr) = session.driver.read_ptr(arg_addr + 0x08) {
                                                        if val_tv_ptr > 0x10000 {
                                                            if let Ok(bound_obj) = session.driver.read_ptr(val_tv_ptr + 0x18) {
                                                                if bound_obj > 0x10000 {
                                                                    let m_idx = session.driver.read::<i32>(bound_obj + 0x20).unwrap_or(0);
                                                                    let t_typ = session.driver.read::<i32>(bound_obj + 0x24).unwrap_or(0);
                                                                    if t_typ == 1 && m_idx > 0 {
                                                                        panel_history_person_id = Some(m_idx);
                                                                    } else if t_typ == 3 && m_idx > 0 {
                                                                        panel_history_club_id = Some(m_idx);
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Inspect GameSubsystems -> BindingSubsystem (index 11) for live bound players in the current FM26 session
        if let Ok(gs_singleton) = session.registry.get_singleton("GameSubsystems") {
            if let Ok(gs_sf) = fmlev_il2cpp::read_static_singleton(&session.driver, base, gs_singleton) {
                let sub_list = if gs_sf > 0x10000 {
                    session.driver.read_ptr(gs_sf).unwrap_or(0)
                } else {
                    0
                };
                if sub_list > 0x10000 {
                    let sub_items = session.driver.read_ptr(sub_list + 0x10).unwrap_or(0);
                    let sub_count = session.driver.read::<i32>(sub_list + 0x18).unwrap_or(0);
                    if sub_items > 0x10000 && sub_count > 11 {
                        let b_sub = session.driver.read_ptr(sub_items + 0x20 + 11 * 8).unwrap_or(0);
                        if b_sub > 0x10000 {
                            live_discovered_players = extract_live_bound_players(&session.driver, b_sub);
                        }
                    }
                }
            }
        }

        // 4. Memory read active player if bound
        let mem_player_opt = if let Some(addr) = self.active_player_addr.filter(|_| session.driver.pid() == std::process::id() as i32) {
            Player::read(&session.driver, addr, &session.registry).ok()
        } else {
            None
        };

        // 5. Memory read active club if bound
        let mem_club_opt = if let Some(addr) = self.active_club_addr.filter(|_| session.driver.pid() == std::process::id() as i32) {
            ClubFinances::read(&session.driver, addr, &session.registry).ok()
        } else {
            None
        };

        // Now session borrow is finished! Merge any live discovered players from FM26 BindingSubsystem:
        let mut first_live_id: Option<i32> = None;
        let mut active_profile_live_id: Option<i32> = None;
        for lp in live_discovered_players {
            if lp.club_id > 0 && !lp.club_name.is_empty() {
                let club = self.club_registry.entry(lp.club_id).or_insert_with(unknown_clubfinances);
                club.club_id = lp.club_id;
                club.club_name = lp.club_name.clone();
            }
            if first_live_id.is_none() {
                first_live_id = Some(lp.id);
            }
            if active_profile_live_id.is_none() && lp.attributes != PlayerAttributes::default() {
                active_profile_live_id = Some(lp.id);
            }
            self.player_registry.entry(lp.id).and_modify(|existing| {
                if existing.user_modified || self.user_modified_players.contains(&lp.id) {
                    return;
                }
                if !lp.name.is_empty() && !lp.name.starts_with("Player #") {
                    existing.name = lp.name.clone();
                }
                if !lp.position.is_empty() {
                    existing.position = lp.position.clone();
                }
                if lp.positions != PlayerPositions::default() {
                    existing.positions = lp.positions.clone();
                } else if !lp.position.is_empty() && existing.positions == PlayerPositions::default() {
                    existing.positions = PlayerPositions::from_position_string(&lp.position);
                }
                if lp.age > 0 {
                    existing.age = lp.age;
                }
                if lp.condition > 0 {
                    existing.condition = lp.condition;
                }
                if lp.sharpness > 0 {
                    existing.sharpness = lp.sharpness;
                }
                if lp.morale > 0 {
                    existing.morale = lp.morale;
                }
                merge_player_attributes(&mut existing.attributes, &lp.attributes);
                if lp.current_ability > 0 {
                    existing.current_ability = lp.current_ability;
                }
                if lp.potential_ability > 0 {
                    existing.potential_ability = lp.potential_ability;
                }
                if !lp.club_name.is_empty() {
                    existing.club_name = lp.club_name.clone();
                }
                if lp.club_id > 0 {
                    existing.club_id = lp.club_id;
                }
                if !lp.nationality.is_empty() {
                    existing.nationality = lp.nationality.clone();
                }
            }).or_insert(lp);
        }
        if active_profile_live_id.is_some() {
            first_live_id = active_profile_live_id;
        }

        // Apply state updates to self:
        if self.context.sync_mode == "auto_follow" {
            if let Some(screen) = detected_screen {
                self.context.active_screen = screen;
            }
            if let Some(pid) = panel_history_person_id {
                if !self.player_registry.contains_key(&pid) {
                    let p = build_template_live_player(pid, format!("Player #{}", pid), String::new());
                    self.player_registry.insert(pid, p);
                }
                if self.context.player.id != pid {
                    let _ = self.select_player(pid);
                }
            } else if let Some((id, addr)) = detected_player_id {
                self.player_memory_bindings.insert(id, addr);
                if self.context.player.id != id {
                    let _ = self.select_player(id);
                }
            } else if let Some(cid) = panel_history_club_id {
                if self.club_registry.contains_key(&cid) {
                    if self.context.club.club_id != cid {
                        let _ = self.select_club(cid);
                    }
                }
            } else if let Some((id, addr)) = detected_club_id {
                self.club_memory_bindings.insert(id, addr);
                if self.context.club.club_id != id {
                    let _ = self.select_club(id);
                }
            } else if let Some(live_id) = first_live_id {
                // If still on default offline player 101 and live FM26 squad players exist, switch to live squad player!
                if (self.context.player.id == 0 || self.context.player.id == 101)
                    && !self.context.player.user_modified
                    && !self.user_modified_players.contains(&101)
                    && self.context.player.id != live_id
                {
                    let _ = self.select_player(live_id);
                }
            }
        }

        // Keep active player in sync with registry if not user-modified
        if !self.context.player.user_modified && !self.user_modified_players.contains(&self.context.player.id) {
            if let Some(reg_player) = self.player_registry.get(&self.context.player.id) {
                if !reg_player.name.is_empty() && !reg_player.name.starts_with("Player #") {
                    self.context.player.name = reg_player.name.clone();
                }
                if reg_player.age > 0 {
                    self.context.player.age = reg_player.age;
                }
                if reg_player.condition > 0 {
                    self.context.player.condition = reg_player.condition;
                }
                if reg_player.sharpness > 0 {
                    self.context.player.sharpness = reg_player.sharpness;
                }
                if reg_player.morale > 0 {
                    self.context.player.morale = reg_player.morale;
                }
                merge_player_attributes(&mut self.context.player.attributes, &reg_player.attributes);
                if !reg_player.position.is_empty() {
                    self.context.player.position = reg_player.position.clone();
                    self.context.player.positions = reg_player.positions.clone();
                }
                if !reg_player.club_name.is_empty() {
                    self.context.player.club_name = reg_player.club_name.clone();
                }
                if reg_player.club_id > 0 {
                    self.context.player.club_id = reg_player.club_id;
                }
                if !reg_player.nationality.is_empty() {
                    self.context.player.nationality = reg_player.nationality.clone();
                }
                if reg_player.current_ability > 0 {
                    self.context.player.current_ability = reg_player.current_ability;
                }
                if reg_player.potential_ability > 0 {
                    self.context.player.potential_ability = reg_player.potential_ability;
                }
            }
        }

        if let Some(mem_player) = mem_player_opt {
            if !self.context.player.user_modified && !self.user_modified_players.contains(&self.context.player.id) {
                self.context.player.current_ability = mem_player.current_ability;
                self.context.player.potential_ability = mem_player.potential_ability;
                self.context.player.condition = mem_player.condition;
                self.context.player.sharpness = mem_player.sharpness;
                self.context.player.morale = mem_player.morale;
                if mem_player.attributes != PlayerAttributes::default()
                    && mem_player.attributes.finishing != (self.context.player.id as u8)
                {
                    self.context.player.attributes = mem_player.attributes;
                }
            }
        }

        if let Some(mem_club) = mem_club_opt {
            if !self.user_modified_clubs.contains(&self.context.club.club_id) {
                self.context.club.balance = mem_club.balance;
                self.context.club.transfer_budget = mem_club.transfer_budget;
                self.context.club.wage_budget = mem_club.wage_budget;
            }
        }

        if self.context.player.id > 0
            && self.session.as_ref().is_some_and(|session| session.driver.pid() != std::process::id() as i32)
            && !self.native_player_records.contains_key(&self.context.player.id)
        {
            // Retry when a newly opened profile gains a complete/changed fingerprint.
            // Failed complete fingerprints suppress repeated full process scans.
            let _ = self.verified_native_player(self.context.player.id);
        }
        if self.native_player_records.get(&self.context.player.id).is_some_and(|r| !r.attributes_verified()) {
            let _ = self.verified_native_player(self.context.player.id);
        }
        if let Some(record) = self.native_player_records.get(&self.context.player.id) {
            match record.read_positions(&self.session.as_ref().unwrap().driver) {
                Ok(positions) => {
                    self.context.player.position = positions.to_position_string();
                    self.context.player.positions = positions;
                    if record.attributes_verified() {
                        record.refresh_player_fields(&self.session.as_ref().unwrap().driver, &mut self.context.player)?;
                    }
                    self.player_registry.insert(self.context.player.id, self.context.player.clone());
                }
                Err(error) => {
                    self.native_player_records.remove(&self.context.player.id);
                    self.context.player.positions = PlayerPositions::unknown();
                    self.context.player.position.clear();
                    self.player_registry.insert(self.context.player.id, self.context.player.clone());
                    return Err(error);
                }
            }
        }

        Ok(())
    }

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

/// Builds realistic initial player, club, nation, and competition registries for FM26.
fn build_default_registries() -> (
    HashMap<i32, Player>,
    HashMap<i32, ClubFinances>,
    HashMap<i32, NationDetails>,
    HashMap<i32, CompetitionDetails>,
) {
    let mut clubs = HashMap::new();
    let mut players = HashMap::new();
    let mut nations = HashMap::new();
    let mut competitions = HashMap::new();

    // -------------------------------------------------------------
    // Clubs
    // -------------------------------------------------------------
    let club_list = vec![
        ClubFinances {
            club_id: 1001,
            club_name: "Manchester City".to_string(),
            league_name: "Premier League".to_string(),
            balance: 220_000_000,
            transfer_budget: 115_000_000,
            wage_budget: 4_200_000,
            wage_used: 3_850_000,
            reputation: 9450,
            training_facilities: 20,
            youth_facilities: 20,
            youth_recruitment: 19,
            junior_coaching: 19,
            data_analysis_facilities: 20,
            corporate_facilities: 20,
            stadium_name: "Etihad Stadium".to_string(),
            stadium_capacity: 53_400,
            stadium_expansion_cap: 62_000,
            sugar_daddy_type: "Foreground".to_string(),
            board_patience: 18,
            fan_confidence: 95,
            board_confidence: 96,
            transfer_retention_pct: 100,
            max_wage_allowance: 600_000,
            affiliates: vec![
                ClubAffiliate {
                    affiliate_club_id: 2001,
                    affiliate_name: "Girona FC".to_string(),
                    is_parent: false,
                    first_option_to_buy: true,
                    send_players_on_loan: true,
                    shared_scouting: true,
                    annual_fee: 500_000,
                    work_permit_haven: true,
                },
                ClubAffiliate {
                    affiliate_club_id: 2002,
                    affiliate_name: "New York City FC".to_string(),
                    is_parent: false,
                    first_option_to_buy: true,
                    send_players_on_loan: true,
                    shared_scouting: true,
                    annual_fee: 250_000,
                    work_permit_haven: false,
                },
            ],
        },
        ClubFinances {
            club_id: 1002,
            club_name: "Real Madrid".to_string(),
            league_name: "La Liga".to_string(),
            balance: 280_000_000,
            transfer_budget: 150_000_000,
            wage_budget: 5_100_000,
            wage_used: 4_700_000,
            reputation: 9800,
            training_facilities: 20,
            youth_facilities: 19,
            youth_recruitment: 20,
            junior_coaching: 20,
            data_analysis_facilities: 19,
            corporate_facilities: 20,
            stadium_name: "Santiago Bernabéu".to_string(),
            stadium_capacity: 84_744,
            stadium_expansion_cap: 85_000,
            sugar_daddy_type: "Underwriter".to_string(),
            board_patience: 14,
            fan_confidence: 92,
            board_confidence: 90,
            transfer_retention_pct: 100,
            max_wage_allowance: 750_000,
            affiliates: vec![
                ClubAffiliate {
                    affiliate_club_id: 2003,
                    affiliate_name: "Real Madrid Castilla".to_string(),
                    is_parent: false,
                    first_option_to_buy: true,
                    send_players_on_loan: true,
                    shared_scouting: true,
                    annual_fee: 0,
                    work_permit_haven: false,
                },
            ],
        },
        ClubFinances {
            club_id: 1003,
            club_name: "Arsenal".to_string(),
            league_name: "Premier League".to_string(),
            balance: 140_000_000,
            transfer_budget: 75_000_000,
            wage_budget: 3_600_000,
            wage_used: 3_200_000,
            reputation: 8900,
            training_facilities: 19,
            youth_facilities: 18,
            youth_recruitment: 18,
            junior_coaching: 18,
            data_analysis_facilities: 18,
            corporate_facilities: 18,
            stadium_name: "Emirates Stadium".to_string(),
            stadium_capacity: 60_704,
            stadium_expansion_cap: 62_000,
            sugar_daddy_type: "None".to_string(),
            board_patience: 16,
            fan_confidence: 88,
            board_confidence: 89,
            transfer_retention_pct: 100,
            max_wage_allowance: 400_000,
            affiliates: vec![
                ClubAffiliate {
                    affiliate_club_id: 2004,
                    affiliate_name: "Colorado Rapids".to_string(),
                    is_parent: false,
                    first_option_to_buy: false,
                    send_players_on_loan: false,
                    shared_scouting: true,
                    annual_fee: 150_000,
                    work_permit_haven: false,
                },
            ],
        },
        ClubFinances {
            club_id: 1004,
            club_name: "Liverpool".to_string(),
            league_name: "Premier League".to_string(),
            balance: 160_000_000,
            transfer_budget: 80_000_000,
            wage_budget: 3_800_000,
            wage_used: 3_500_000,
            reputation: 9100,
            training_facilities: 19,
            youth_facilities: 19,
            youth_recruitment: 18,
            junior_coaching: 18,
            data_analysis_facilities: 19,
            corporate_facilities: 18,
            stadium_name: "Anfield".to_string(),
            stadium_capacity: 61_276,
            stadium_expansion_cap: 62_000,
            sugar_daddy_type: "None".to_string(),
            board_patience: 15,
            fan_confidence: 90,
            board_confidence: 91,
            transfer_retention_pct: 100,
            max_wage_allowance: 450_000,
            affiliates: Vec::new(),
        },
        ClubFinances {
            club_id: 1005,
            club_name: "FC Barcelona".to_string(),
            league_name: "La Liga".to_string(),
            balance: 45_000_000,
            transfer_budget: 30_000_000,
            wage_budget: 4_500_000,
            wage_used: 4_400_000,
            reputation: 9200,
            training_facilities: 19,
            youth_facilities: 20,
            youth_recruitment: 19,
            junior_coaching: 20,
            data_analysis_facilities: 18,
            corporate_facilities: 19,
            stadium_name: "Spotify Camp Nou".to_string(),
            stadium_capacity: 105_000,
            stadium_expansion_cap: 105_000,
            sugar_daddy_type: "None".to_string(),
            board_patience: 12,
            fan_confidence: 84,
            board_confidence: 80,
            transfer_retention_pct: 70,
            max_wage_allowance: 500_000,
            affiliates: vec![
                ClubAffiliate {
                    affiliate_club_id: 2005,
                    affiliate_name: "Barça Atlètic".to_string(),
                    is_parent: false,
                    first_option_to_buy: true,
                    send_players_on_loan: true,
                    shared_scouting: true,
                    annual_fee: 0,
                    work_permit_haven: false,
                },
            ],
        },
        ClubFinances {
            club_id: 1006,
            club_name: "Bayern München".to_string(),
            league_name: "Bundesliga".to_string(),
            balance: 210_000_000,
            transfer_budget: 100_000_000,
            wage_budget: 4_300_000,
            wage_used: 3_900_000,
            reputation: 9350,
            training_facilities: 20,
            youth_facilities: 19,
            youth_recruitment: 18,
            junior_coaching: 19,
            data_analysis_facilities: 19,
            corporate_facilities: 20,
            stadium_name: "Allianz Arena".to_string(),
            stadium_capacity: 75_024,
            stadium_expansion_cap: 78_000,
            sugar_daddy_type: "None".to_string(),
            board_patience: 17,
            fan_confidence: 93,
            board_confidence: 95,
            transfer_retention_pct: 100,
            max_wage_allowance: 550_000,
            affiliates: Vec::new(),
        },
        ClubFinances {
            club_id: 1007,
            club_name: "Paris Saint-Germain".to_string(),
            league_name: "Ligue 1".to_string(),
            balance: 250_000_000,
            transfer_budget: 130_000_000,
            wage_budget: 4_800_000,
            wage_used: 4_400_000,
            reputation: 9000,
            training_facilities: 20,
            youth_facilities: 19,
            youth_recruitment: 19,
            junior_coaching: 19,
            data_analysis_facilities: 19,
            corporate_facilities: 20,
            stadium_name: "Parc des Princes".to_string(),
            stadium_capacity: 48_583,
            stadium_expansion_cap: 60_000,
            sugar_daddy_type: "Foreground".to_string(),
            board_patience: 15,
            fan_confidence: 86,
            board_confidence: 88,
            transfer_retention_pct: 100,
            max_wage_allowance: 650_000,
            affiliates: Vec::new(),
        },
        ClubFinances {
            club_id: 1008,
            club_name: "Inter Milan".to_string(),
            league_name: "Serie A".to_string(),
            balance: 60_000_000,
            transfer_budget: 40_000_000,
            wage_budget: 2_900_000,
            wage_used: 2_700_000,
            reputation: 8700,
            training_facilities: 18,
            youth_facilities: 17,
            youth_recruitment: 17,
            junior_coaching: 17,
            data_analysis_facilities: 17,
            corporate_facilities: 18,
            stadium_name: "San Siro".to_string(),
            stadium_capacity: 75_817,
            stadium_expansion_cap: 76_000,
            sugar_daddy_type: "None".to_string(),
            board_patience: 15,
            fan_confidence: 88,
            board_confidence: 87,
            transfer_retention_pct: 85,
            max_wage_allowance: 350_000,
            affiliates: Vec::new(),
        },
        ClubFinances {
            club_id: 7001,
            club_name: "Persib".to_string(),
            league_name: "Liga 1 Indonesia".to_string(),
            balance: 15_000_000,
            transfer_budget: 8_000_000,
            wage_budget: 120_000,
            wage_used: 95_000,
            reputation: 6200,
            training_facilities: 14,
            youth_facilities: 13,
            youth_recruitment: 15,
            junior_coaching: 14,
            data_analysis_facilities: 12,
            corporate_facilities: 13,
            stadium_name: "Gelora Bandung Lautan Api".to_string(),
            stadium_capacity: 38_000,
            stadium_expansion_cap: 40_000,
            sugar_daddy_type: "Underwriter".to_string(),
            board_patience: 15,
            fan_confidence: 95,
            board_confidence: 92,
            transfer_retention_pct: 100,
            max_wage_allowance: 25_000,
            affiliates: Vec::new(),
        },
    ];

    for c in club_list {
        clubs.insert(c.club_id, c);
    }

    // -------------------------------------------------------------
    // Nations
    // -------------------------------------------------------------
    let nation_list = vec![
        NationDetails {
            id: 765,
            name: "England".to_string(),
            three_letter_code: "ENG".to_string(),
            continent: "Europe".to_string(),
            reputation: 8800,
            youth_rating: 145,
            economic_factor: 19,
            state_of_development: 2,
            fa_financial_power: 18,
            game_importance: 20,
            tax_rate: 45,
            corruption_index: 2,
            youth_facilities_avg: 17,
            youth_coaching_avg: 16,
            youth_recruitment_avg: 17,
        },
        NationDetails {
            id: 759,
            name: "Brazil".to_string(),
            three_letter_code: "BRA".to_string(),
            continent: "South America".to_string(),
            reputation: 9600,
            youth_rating: 165,
            economic_factor: 13,
            state_of_development: 1,
            fa_financial_power: 14,
            game_importance: 20,
            tax_rate: 27,
            corruption_index: 10,
            youth_facilities_avg: 14,
            youth_coaching_avg: 15,
            youth_recruitment_avg: 18,
        },
        NationDetails {
            id: 769,
            name: "France".to_string(),
            three_letter_code: "FRA".to_string(),
            continent: "Europe".to_string(),
            reputation: 9500,
            youth_rating: 155,
            economic_factor: 18,
            state_of_development: 2,
            fa_financial_power: 17,
            game_importance: 18,
            tax_rate: 48,
            corruption_index: 3,
            youth_facilities_avg: 18,
            youth_coaching_avg: 18,
            youth_recruitment_avg: 18,
        },
        NationDetails {
            id: 753,
            name: "Argentina".to_string(),
            three_letter_code: "ARG".to_string(),
            continent: "South America".to_string(),
            reputation: 9400,
            youth_rating: 150,
            economic_factor: 11,
            state_of_development: 1,
            fa_financial_power: 12,
            game_importance: 20,
            tax_rate: 35,
            corruption_index: 12,
            youth_facilities_avg: 13,
            youth_coaching_avg: 14,
            youth_recruitment_avg: 17,
        },
        NationDetails {
            id: 771,
            name: "Germany".to_string(),
            three_letter_code: "GER".to_string(),
            continent: "Europe".to_string(),
            reputation: 9100,
            youth_rating: 140,
            economic_factor: 20,
            state_of_development: 2,
            fa_financial_power: 19,
            game_importance: 19,
            tax_rate: 42,
            corruption_index: 1,
            youth_facilities_avg: 19,
            youth_coaching_avg: 18,
            youth_recruitment_avg: 18,
        },
        NationDetails {
            id: 796,
            name: "Spain".to_string(),
            three_letter_code: "ESP".to_string(),
            continent: "Europe".to_string(),
            reputation: 9300,
            youth_rating: 150,
            economic_factor: 15,
            state_of_development: 2,
            fa_financial_power: 16,
            game_importance: 20,
            tax_rate: 47,
            corruption_index: 4,
            youth_facilities_avg: 18,
            youth_coaching_avg: 18,
            youth_recruitment_avg: 18,
        },
        NationDetails {
            id: 781,
            name: "Italy".to_string(),
            three_letter_code: "ITA".to_string(),
            continent: "Europe".to_string(),
            reputation: 8900,
            youth_rating: 135,
            economic_factor: 16,
            state_of_development: 2,
            fa_financial_power: 15,
            game_importance: 19,
            tax_rate: 43,
            corruption_index: 8,
            youth_facilities_avg: 16,
            youth_coaching_avg: 16,
            youth_recruitment_avg: 16,
        },
        NationDetails {
            id: 115,
            name: "Japan".to_string(),
            three_letter_code: "JPN".to_string(),
            continent: "Asia".to_string(),
            reputation: 7800,
            youth_rating: 115,
            economic_factor: 18,
            state_of_development: 2,
            fa_financial_power: 16,
            game_importance: 14,
            tax_rate: 30,
            corruption_index: 1,
            youth_facilities_avg: 16,
            youth_coaching_avg: 15,
            youth_recruitment_avg: 15,
        },
        NationDetails {
            id: 110,
            name: "Indonesia".to_string(),
            three_letter_code: "IDN".to_string(),
            continent: "Asia".to_string(),
            reputation: 5400,
            youth_rating: 85,
            economic_factor: 11,
            state_of_development: 1,
            fa_financial_power: 11,
            game_importance: 18,
            tax_rate: 22,
            corruption_index: 14,
            youth_facilities_avg: 9,
            youth_coaching_avg: 9,
            youth_recruitment_avg: 10,
        },
    ];

    for n in nation_list {
        nations.insert(n.id, n);
    }

    // -------------------------------------------------------------
    // Competitions
    // -------------------------------------------------------------
    let comp_list = vec![
        CompetitionDetails {
            id: 11,
            name: "Premier League".to_string(),
            short_name: "EPL".to_string(),
            nation_name: "England".to_string(),
            nation_id: 765,
            reputation: 9200,
            max_substitutes: 5,
            max_bench_size: 9,
            substitution_windows: 3,
            var_active: true,
            max_squad_size: 25,
            max_foreign_players: 17,
            min_homegrown_club: 4,
            min_homegrown_nation: 8,
            winner_prize_money: 44_000_000,
            tv_rights_pool: 2_400_000_000,
            yellow_cards_for_ban: 5,
        },
        CompetitionDetails {
            id: 1301444,
            name: "UEFA Champions League".to_string(),
            short_name: "UCL".to_string(),
            nation_name: "Europe".to_string(),
            nation_id: 0,
            reputation: 9800,
            max_substitutes: 5,
            max_bench_size: 12,
            substitution_windows: 3,
            var_active: true,
            max_squad_size: 25,
            max_foreign_players: 17,
            min_homegrown_club: 4,
            min_homegrown_nation: 8,
            winner_prize_money: 85_000_000,
            tv_rights_pool: 3_200_000_000,
            yellow_cards_for_ban: 3,
        },
        CompetitionDetails {
            id: 67,
            name: "La Liga".to_string(),
            short_name: "LaLiga".to_string(),
            nation_name: "Spain".to_string(),
            nation_id: 796,
            reputation: 8900,
            max_substitutes: 5,
            max_bench_size: 9,
            substitution_windows: 3,
            var_active: true,
            max_squad_size: 25,
            max_foreign_players: 3,
            min_homegrown_club: 4,
            min_homegrown_nation: 8,
            winner_prize_money: 38_000_000,
            tv_rights_pool: 1_600_000_000,
            yellow_cards_for_ban: 5,
        },
        CompetitionDetails {
            id: 45,
            name: "Serie A".to_string(),
            short_name: "Serie A".to_string(),
            nation_name: "Italy".to_string(),
            nation_id: 781,
            reputation: 8600,
            max_substitutes: 5,
            max_bench_size: 9,
            substitution_windows: 3,
            var_active: true,
            max_squad_size: 25,
            max_foreign_players: 17,
            min_homegrown_club: 4,
            min_homegrown_nation: 8,
            winner_prize_money: 28_000_000,
            tv_rights_pool: 1_100_000_000,
            yellow_cards_for_ban: 5,
        },
        CompetitionDetails {
            id: 32,
            name: "Bundesliga".to_string(),
            short_name: "BL".to_string(),
            nation_name: "Germany".to_string(),
            nation_id: 771,
            reputation: 8700,
            max_substitutes: 5,
            max_bench_size: 9,
            substitution_windows: 3,
            var_active: true,
            max_squad_size: 25,
            max_foreign_players: 99,
            min_homegrown_club: 4,
            min_homegrown_nation: 8,
            winner_prize_money: 32_000_000,
            tv_rights_pool: 1_300_000_000,
            yellow_cards_for_ban: 5,
        },
        CompetitionDetails {
            id: 1301394,
            name: "FIFA World Cup".to_string(),
            short_name: "WC".to_string(),
            nation_name: "World".to_string(),
            nation_id: 0,
            reputation: 10000,
            max_substitutes: 5,
            max_bench_size: 15,
            substitution_windows: 3,
            var_active: true,
            max_squad_size: 26,
            max_foreign_players: 0,
            min_homegrown_club: 0,
            min_homegrown_nation: 0,
            winner_prize_money: 100_000_000,
            tv_rights_pool: 4_500_000_000,
            yellow_cards_for_ban: 2,
        },
    ];

    for comp in comp_list {
        competitions.insert(comp.id, comp);
    }

    // -------------------------------------------------------------
    // Players
    // -------------------------------------------------------------
    let player_list = vec![
        Player {
            id: 101,
            name: "Erling Haaland".to_string(),
            club_id: 1001,
            club_name: "Manchester City".to_string(),
            age: 24,
            nationality: "Norway".to_string(),
            position: "ST (C)".to_string(),
            current_ability: 188,
            potential_ability: 195,
            condition: 9800,
            sharpness: 9900,
            morale: 95,
            morale_label: "Superb".to_string(),
            is_injured: false,
            is_suspended: false,
            contract: PlayerContract {
                wage_per_week: 400_000,
                expiry_year: 2030,
                release_clause: 200_000_000,
                squad_status: "Star Player".to_string(),
                happiness: "Delighted".to_string(),
            },
            attributes: PlayerAttributes {
                finishing: 19,
                dribbling: 15,
                passing: 13,
                tackling: 9,
                pace: 19,
                acceleration: 18,
                stamina: 17,
                strength: 19,
                heading: 18,
                penalty_taking: 19,
                first_touch: 16,
                technique: 16,
                anticipation: 17,
                composure: 18,
                decisions: 16,
                determination: 19,
                off_the_ball: 18,
                work_rate: 17,
                jumping_reach: 18,
                natural_fitness: 18,
                consistency: 18,
                important_matches: 17,
                injury_proneness: 8,
                versatility: 12,
                adaptability: 16,
                ambition: 19,
                loyalty: 14,
                pressure: 18,
                professionalism: 19,
                sportsmanship: 15,
                temperament: 16,
                ..Default::default()
            },
            ..Default::default()
        },
        Player {
            id: 102,
            name: "Kylian Mbappé".to_string(),
            club_id: 1002,
            club_name: "Real Madrid".to_string(),
            age: 25,
            nationality: "France".to_string(),
            position: "ST (C), AM (L)".to_string(),
            current_ability: 189,
            potential_ability: 196,
            condition: 9700,
            sharpness: 9800,
            morale: 92,
            morale_label: "Very Good".to_string(),
            is_injured: false,
            is_suspended: false,
            contract: PlayerContract {
                wage_per_week: 600_000,
                expiry_year: 2029,
                release_clause: 1_000_000_000,
                squad_status: "Star Player".to_string(),
                happiness: "Delighted".to_string(),
            },
            attributes: PlayerAttributes {
                finishing: 18,
                dribbling: 19,
                passing: 15,
                tackling: 7,
                pace: 20,
                acceleration: 20,
                stamina: 16,
                technique: 19,
                flair: 19,
                composure: 17,
                agility: 18,
                balance: 16,
                anticipation: 17,
                decisions: 16,
                determination: 17,
                off_the_ball: 19,
                consistency: 18,
                important_matches: 19,
                injury_proneness: 7,
                versatility: 15,
                adaptability: 17,
                ambition: 20,
                loyalty: 13,
                pressure: 19,
                professionalism: 18,
                sportsmanship: 14,
                temperament: 15,
                ..Default::default()
            },
            ..Default::default()
        },
        Player {
            id: 103,
            name: "Jude Bellingham".to_string(),
            club_id: 1002,
            club_name: "Real Madrid".to_string(),
            age: 21,
            nationality: "England".to_string(),
            position: "AM (C), M (C)".to_string(),
            current_ability: 182,
            potential_ability: 192,
            condition: 9900,
            sharpness: 9900,
            morale: 98,
            morale_label: "Superb".to_string(),
            is_injured: false,
            is_suspended: false,
            contract: PlayerContract {
                wage_per_week: 350_000,
                expiry_year: 2031,
                release_clause: 1_000_000_000,
                squad_status: "Star Player".to_string(),
                happiness: "Delighted".to_string(),
            },
            attributes: PlayerAttributes {
                finishing: 16,
                dribbling: 16,
                passing: 16,
                tackling: 15,
                pace: 16,
                acceleration: 16,
                stamina: 18,
                work_rate: 19,
                technique: 17,
                determination: 19,
                composure: 18,
                leadership: 17,
                bravery: 17,
                decisions: 16,
                vision: 16,
                consistency: 18,
                important_matches: 19,
                injury_proneness: 6,
                versatility: 18,
                adaptability: 19,
                ambition: 19,
                loyalty: 16,
                pressure: 19,
                professionalism: 19,
                sportsmanship: 16,
                temperament: 17,
                ..Default::default()
            },
            ..Default::default()
        },
        Player {
            id: 104,
            name: "Bukayo Saka".to_string(),
            club_id: 1003,
            club_name: "Arsenal".to_string(),
            age: 23,
            nationality: "England".to_string(),
            position: "AM (R), M (R)".to_string(),
            current_ability: 174,
            potential_ability: 185,
            condition: 9600,
            sharpness: 9700,
            morale: 90,
            morale_label: "Very Good".to_string(),
            is_injured: false,
            is_suspended: false,
            contract: PlayerContract {
                wage_per_week: 290_000,
                expiry_year: 2028,
                release_clause: 0,
                squad_status: "Star Player".to_string(),
                happiness: "Happy".to_string(),
            },
            attributes: PlayerAttributes {
                dribbling: 17,
                crossing: 16,
                passing: 16,
                technique: 17,
                finishing: 15,
                pace: 16,
                acceleration: 17,
                stamina: 17,
                work_rate: 18,
                consistency: 17,
                important_matches: 17,
                injury_proneness: 8,
                versatility: 16,
                adaptability: 16,
                ambition: 17,
                loyalty: 19,
                pressure: 18,
                professionalism: 19,
                sportsmanship: 18,
                temperament: 18,
                ..Default::default()
            },
            ..Default::default()
        },
        Player {
            id: 105,
            name: "Kevin De Bruyne".to_string(),
            club_id: 1001,
            club_name: "Manchester City".to_string(),
            age: 33,
            nationality: "Belgium".to_string(),
            position: "AM (C), M (C)".to_string(),
            current_ability: 184,
            potential_ability: 188,
            condition: 9200,
            sharpness: 9400,
            morale: 88,
            morale_label: "Good".to_string(),
            is_injured: false,
            is_suspended: false,
            contract: PlayerContract {
                wage_per_week: 380_000,
                expiry_year: 2026,
                release_clause: 0,
                squad_status: "Star Player".to_string(),
                happiness: "Content".to_string(),
            },
            attributes: PlayerAttributes {
                passing: 20,
                vision: 20,
                crossing: 19,
                technique: 19,
                decisions: 19,
                composure: 18,
                long_shots: 18,
                first_touch: 18,
                dribbling: 15,
                pace: 13,
                acceleration: 13,
                stamina: 15,
                consistency: 17,
                important_matches: 18,
                injury_proneness: 14,
                versatility: 16,
                adaptability: 16,
                ambition: 18,
                loyalty: 17,
                pressure: 18,
                professionalism: 18,
                sportsmanship: 16,
                temperament: 16,
                ..Default::default()
            },
            ..Default::default()
        },
        Player {
            id: 106,
            name: "Lamine Yamal".to_string(),
            club_id: 1005,
            club_name: "FC Barcelona".to_string(),
            age: 17,
            nationality: "Spain".to_string(),
            position: "AM (R), ST (C)".to_string(),
            current_ability: 165,
            potential_ability: 198,
            condition: 9800,
            sharpness: 9600,
            morale: 96,
            morale_label: "Superb".to_string(),
            is_injured: false,
            is_suspended: false,
            contract: PlayerContract {
                wage_per_week: 80_000,
                expiry_year: 2030,
                release_clause: 1_000_000_000,
                squad_status: "Important Player".to_string(),
                happiness: "Delighted".to_string(),
            },
            attributes: PlayerAttributes {
                flair: 19,
                dribbling: 18,
                technique: 18,
                vision: 17,
                passing: 16,
                crossing: 16,
                finishing: 15,
                agility: 17,
                acceleration: 16,
                pace: 15,
                stamina: 14,
                consistency: 16,
                important_matches: 18,
                injury_proneness: 6,
                versatility: 16,
                adaptability: 17,
                ambition: 19,
                loyalty: 18,
                pressure: 19,
                professionalism: 18,
                sportsmanship: 17,
                temperament: 17,
                ..Default::default()
            },
            ..Default::default()
        },
        Player {
            id: 107,
            name: "Thibaut Courtois".to_string(),
            club_id: 1002,
            club_name: "Real Madrid".to_string(),
            age: 32,
            nationality: "Belgium".to_string(),
            position: "GK".to_string(),
            current_ability: 178,
            potential_ability: 182,
            condition: 9500,
            sharpness: 9700,
            morale: 91,
            morale_label: "Very Good".to_string(),
            is_injured: false,
            is_suspended: false,
            contract: PlayerContract {
                wage_per_week: 280_000,
                expiry_year: 2027,
                release_clause: 0,
                squad_status: "Star Player".to_string(),
                happiness: "Happy".to_string(),
            },
            attributes: PlayerAttributes {
                reflexes: 19,
                handling: 18,
                aerial_reach: 19,
                command_of_area: 18,
                one_on_ones: 18,
                communication: 17,
                kicking: 15,
                throwing: 16,
                rushing_out: 14,
                eccentricity: 3,
                agility: 15,
                jumping_reach: 19,
                balance: 15,
                consistency: 18,
                important_matches: 19,
                injury_proneness: 10,
                versatility: 10,
                adaptability: 16,
                ambition: 17,
                loyalty: 16,
                pressure: 19,
                professionalism: 18,
                sportsmanship: 15,
                temperament: 17,
                ..Default::default()
            },
            ..Default::default()
        },
        Player {
            id: 108,
            name: "Martin Ødegaard".to_string(),
            club_id: 1003,
            club_name: "Arsenal".to_string(),
            age: 26,
            nationality: "Norway".to_string(),
            position: "AM (C), M (C)".to_string(),
            current_ability: 176,
            potential_ability: 182,
            condition: 9700,
            sharpness: 9800,
            morale: 94,
            morale_label: "Superb".to_string(),
            is_injured: false,
            is_suspended: false,
            contract: PlayerContract {
                wage_per_week: 250_000,
                expiry_year: 2028,
                release_clause: 0,
                squad_status: "Star Player".to_string(),
                happiness: "Delighted".to_string(),
            },
            attributes: PlayerAttributes {
                passing: 18,
                vision: 19,
                technique: 18,
                dribbling: 17,
                decisions: 17,
                leadership: 18,
                teamwork: 19,
                work_rate: 18,
                first_touch: 18,
                finishing: 15,
                stamina: 17,
                pace: 14,
                acceleration: 15,
                consistency: 17,
                important_matches: 17,
                injury_proneness: 7,
                versatility: 15,
                adaptability: 17,
                ambition: 17,
                loyalty: 18,
                pressure: 18,
                professionalism: 20,
                sportsmanship: 18,
                temperament: 18,
                ..Default::default()
            },
            ..Default::default()
        },
        Player {
            id: 109,
            name: "Harry Kane".to_string(),
            club_id: 1006,
            club_name: "Bayern München".to_string(),
            age: 31,
            nationality: "England".to_string(),
            position: "ST (C)".to_string(),
            current_ability: 183,
            potential_ability: 187,
            condition: 9700,
            sharpness: 9800,
            morale: 93,
            morale_label: "Superb".to_string(),
            is_injured: false,
            is_suspended: false,
            contract: PlayerContract {
                wage_per_week: 410_000,
                expiry_year: 2028,
                release_clause: 0,
                squad_status: "Star Player".to_string(),
                happiness: "Delighted".to_string(),
            },
            attributes: PlayerAttributes {
                finishing: 19,
                passing: 18,
                penalty_taking: 20,
                composure: 19,
                decisions: 18,
                vision: 18,
                heading: 17,
                strength: 16,
                stamina: 16,
                first_touch: 18,
                technique: 18,
                pace: 13,
                acceleration: 12,
                consistency: 18,
                important_matches: 17,
                injury_proneness: 9,
                versatility: 15,
                adaptability: 18,
                ambition: 18,
                loyalty: 17,
                pressure: 18,
                professionalism: 19,
                sportsmanship: 17,
                temperament: 18,
                ..Default::default()
            },
            ..Default::default()
        },
        Player {
            id: 110,
            name: "Mohamed Salah".to_string(),
            club_id: 1004,
            club_name: "Liverpool".to_string(),
            age: 32,
            nationality: "Egypt".to_string(),
            position: "AM (R), ST (C)".to_string(),
            current_ability: 180,
            potential_ability: 185,
            condition: 9600,
            sharpness: 9700,
            morale: 90,
            morale_label: "Very Good".to_string(),
            is_injured: false,
            is_suspended: false,
            contract: PlayerContract {
                wage_per_week: 350_000,
                expiry_year: 2026,
                release_clause: 0,
                squad_status: "Star Player".to_string(),
                happiness: "Happy".to_string(),
            },
            attributes: PlayerAttributes {
                finishing: 18,
                dribbling: 17,
                pace: 17,
                acceleration: 17,
                composure: 17,
                off_the_ball: 18,
                passing: 15,
                technique: 17,
                stamina: 17,
                natural_fitness: 18,
                consistency: 18,
                important_matches: 18,
                injury_proneness: 5,
                versatility: 15,
                adaptability: 18,
                ambition: 19,
                loyalty: 17,
                pressure: 18,
                professionalism: 19,
                sportsmanship: 16,
                temperament: 17,
                ..Default::default()
            },
            ..Default::default()
        },
        Player {
            id: 73,
            name: "Zulkifli Lukmansyah".to_string(),
            club_id: 7001,
            club_name: "Persib".to_string(),
            age: 19,
            nationality: "Indonesia".to_string(),
            position: "D (R)".to_string(),
            positions: PlayerPositions::from_position_string("D (R)"),
            current_ability: 138,
            potential_ability: 165,
            condition: 10000,
            sharpness: 9800,
            morale: 95,
            morale_label: "Superb".to_string(),
            is_injured: false,
            is_suspended: false,
            contract: PlayerContract {
                wage_per_week: 5_000,
                expiry_year: 2028,
                release_clause: 0,
                squad_status: "Regular Starter".to_string(),
                happiness: "Delighted".to_string(),
            },
            attributes: PlayerAttributes {
                tackling: 14,
                pace: 16,
                acceleration: 15,
                crossing: 13,
                stamina: 15,
                work_rate: 16,
                determination: 16,
                teamwork: 15,
                positioning: 14,
                anticipation: 14,
                dribbling: 12,
                passing: 12,
                decisions: 13,
                natural_fitness: 16,
                consistency: 15,
                important_matches: 15,
                injury_proneness: 5,
                versatility: 14,
                adaptability: 15,
                ambition: 16,
                loyalty: 17,
                pressure: 15,
                professionalism: 16,
                sportsmanship: 15,
                temperament: 15,
                ..Default::default()
            },
            user_modified: false,
            ..Default::default()
        },
    ];

    for mut p in player_list {
        p.positions = PlayerPositions::from_position_string(&p.position);
        players.insert(p.id, p);
    }

    (players, clubs, nations, competitions)
}

/// Apply the user's changed fields to fresh authoritative values, preserving untouched native ratings.
#[cfg(test)]
fn apply_changed_positions(previous: &PlayerPositions, requested: &PlayerPositions, native: &mut PlayerPositions) {
    macro_rules! patch {
        ($($field:ident),+ $(,)?) => { $(if requested.$field != previous.$field { native.$field = requested.$field; })+ };
    }
    patch!(gk, dl, dc, dr, wbl, wbr, dm, ml, mc, mr, aml, amc, amr, st, left_foot, right_foot);
}

fn unsupported_live_player_changes(previous: &Player, requested: &Player) -> Vec<String> {
    let mut comparable = requested.clone();
    comparable.positions = previous.positions.clone();
    if requested.positions != previous.positions { comparable.position = previous.position.clone(); }
    comparable.user_modified = previous.user_modified;
    comparable.current_ability = previous.current_ability;
    comparable.potential_ability = previous.potential_ability;
    comparable.contract.wage_per_week = previous.contract.wage_per_week;
    comparable.contract.expiry_year = previous.contract.expiry_year;
    for field in crate::native_player::native_attributes() {
        (field.set)(&mut comparable.attributes, (field.get)(&previous.attributes));
    }
    fn differences(path: &str, old: &serde_json::Value, new: &serde_json::Value, output: &mut Vec<String>) {
        if old == new { return; }
        if let (Some(old), Some(new)) = (old.as_object(), new.as_object()) {
            for (key, value) in new {
                let path = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
                differences(&path, &old[key], value, output);
            }
        } else { output.push(path.to_string()); }
    }
    let mut fields = Vec::new();
    differences("", &serde_json::to_value(previous).unwrap(), &serde_json::to_value(comparable).unwrap(), &mut fields);
    fields
}

#[cfg(test)]
fn is_positions_only_edit(previous: &Player, requested: &Player) -> bool {
    let mut comparable = requested.clone();
    comparable.positions = previous.positions.clone();
    comparable.position = previous.position.clone();
    comparable.user_modified = previous.user_modified;
    comparable == *previous
}

fn resolve_binding_subsystem(session: &LiveSession) -> Option<usize> {
    let base = session.bases.game_assembly_base?;
    let singleton = session.registry.get_singleton("GameSubsystems").ok()?;
    let fields = fmlev_il2cpp::read_static_singleton(&session.driver, base, singleton).ok()?;
    let list = session.driver.read_ptr(fields).ok()?;
    let items = session.driver.read_ptr(list + 0x10).ok()?;
    let count = session.driver.read::<i32>(list + 0x18).ok()?;
    if !(12..=1024).contains(&count) { return None; }
    let subsystem = session.driver.read_ptr(items + 0x20 + 11 * 8).ok()?;
    (subsystem > 0x10000).then_some(subsystem)
}

fn unknown_clubfinances() -> ClubFinances {
    ClubFinances {
        club_id: 0,
        club_name: String::new(),
        balance: 0,
        transfer_budget: 0,
        wage_budget: 0,
        league_name: String::new(),
        reputation: 0,
        wage_used: 0,
        training_facilities: 0,
        youth_facilities: 0,
        youth_recruitment: 0,
        junior_coaching: 0,
        data_analysis_facilities: 0,
        corporate_facilities: 0,
        stadium_name: String::new(),
        stadium_capacity: 0,
        stadium_expansion_cap: 0,
        sugar_daddy_type: String::new(),
        board_patience: 0,
        fan_confidence: 0,
        board_confidence: 0,
        transfer_retention_pct: 0,
        max_wage_allowance: 0,
        affiliates: Vec::new(),
    }
}

fn unknown_nationdetails() -> NationDetails {
    NationDetails {
        id: 0,
        name: String::new(),
        three_letter_code: String::new(),
        continent: String::new(),
        reputation: 0,
        youth_rating: 0,
        economic_factor: 0,
        state_of_development: 0,
        fa_financial_power: 0,
        game_importance: 0,
        tax_rate: 0,
        corruption_index: 0,
        youth_facilities_avg: 0,
        youth_coaching_avg: 0,
        youth_recruitment_avg: 0,
    }
}

fn unknown_competitiondetails() -> CompetitionDetails {
    CompetitionDetails {
        id: 0,
        name: String::new(),
        short_name: String::new(),
        nation_name: String::new(),
        nation_id: 0,
        reputation: 0,
        max_substitutes: 0,
        max_bench_size: 0,
        substitution_windows: 0,
        var_active: false,
        max_squad_size: 0,
        max_foreign_players: 0,
        min_homegrown_club: 0,
        min_homegrown_nation: 0,
        winner_prize_money: 0,
        tv_rights_pool: 0,
        yellow_cards_for_ban: 0,
    }
}

fn build_template_live_player(id: i32, name: String, position: String) -> Player {
    let pos_str = position;
    let positions = PlayerPositions::unknown();

    Player {
        id,
        name,
        club_id: 0,
        club_name: String::new(),
        age: 0,
        nationality: String::new(),
        position: pos_str,
        positions,
        current_ability: 0,
        potential_ability: 0,
        condition: 0,
        sharpness: 0,
        morale: 0,
        morale_label: String::new(),
        is_injured: false,
        is_suspended: false,
        contract: PlayerContract {
            wage_per_week: 0,
            expiry_year: 0,
            release_clause: 0,
            squad_status: String::new(),
            happiness: String::new(),
        },
        attributes: PlayerAttributes::default(),
        user_modified: false,
    }
}

/// Merges non-zero attribute fields from `src` into `dst`.
fn merge_player_attributes(dst: &mut PlayerAttributes, src: &PlayerAttributes) {
    macro_rules! merge_field {
        ($($field:ident),+ $(,)?) => {
            $(
                if src.$field > 0 {
                    dst.$field = src.$field;
                }
            )+
        };
    }
    merge_field!(
        marking, long_throws, punching, dirtiness, controversy,
        corners, crossing, dribbling, finishing, first_touch, free_kicks,
        heading, long_shots, passing, penalty_taking, tackling, technique,
        aggression, anticipation, bravery, composure, concentration, decisions,
        determination, flair, leadership, off_the_ball, positioning, teamwork,
        vision, work_rate,
        acceleration, agility, balance, jumping_reach, natural_fitness, pace,
        stamina, strength,
        consistency, important_matches, injury_proneness, versatility,
        adaptability, ambition, loyalty, pressure, professionalism,
        sportsmanship, temperament,
        aerial_reach, command_of_area, communication, eccentricity, handling,
        kicking, one_on_ones, reflexes, rushing_out, throwing,
    );
}


/// Decoded representation of a managed TypedValue payload.
#[derive(Debug, Clone, PartialEq)]
pub enum DecodedTypedValue {
    Signed(i64),
    Unsigned(u64),
    Float(f64),
    Bool(bool),
    String(String),
    Object(usize),
    Reference { ptr: usize, ref_type: i32, m_index: i32 },
}

fn read_il2cpp_class_name(
    driver: &fmlev_mem::LinuxMemoryDriver,
    obj: usize,
) -> Option<(String, String)> {
    if obj <= 0x10000 || obj >= 0x7fff_ffff_ffff {
        return None;
    }
    let klass = driver.read_ptr(obj).ok()?;
    if klass <= 0x10000 || klass >= 0x7fff_ffff_ffff {
        return None;
    }
    let name_ptr = driver.read_ptr(klass + 0x10).ok()?;
    if name_ptr <= 0x10000 || name_ptr >= 0x7fff_ffff_ffff {
        return None;
    }
    let name = fmlev_il2cpp::types::read_cstring(driver, name_ptr, 128).ok()?;
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_graphic()) {
        return None;
    }
    let ns_ptr = driver.read_ptr(klass + 0x18).unwrap_or(0);
    let ns = if ns_ptr > 0x10000 && ns_ptr < 0x7fff_ffff_ffff {
        fmlev_il2cpp::types::read_cstring(driver, ns_ptr, 128).unwrap_or_default()
    } else {
        String::new()
    };
    Some((name, ns))
}

/// Decodes a live `SI.Core.TypedValue` object at `val_tv`:
/// - `SI.Core.NumericTypedValue`: inline `DynamicNumber` (`Union` 8 bytes at `+0x18`, `NumberKind` `i32` at `+0x20`: `0 = SignedWhole`, `1 = UnsignedWhole`, `2 = FloatingPoint`)
/// - `SI.Core.ReferenceTypedValue`: managed object pointer at `+0x18` (`System.String`, `FM.UI.*Reference`, `SI.Bindable.DynamicReference`, etc.)
/// - `SI.Core.TypedValue<T>`: inline primitive at `+0x18`
pub fn read_typed_value(
    driver: &fmlev_mem::LinuxMemoryDriver,
    val_tv: usize,
) -> Option<DecodedTypedValue> {
    if val_tv <= 0x10000 || val_tv >= 0x7fff_ffff_ffff {
        return None;
    }
    let mut raw = [0u8; 16];
    driver.read_bytes(val_tv + 0x18, &mut raw).ok()?;
    let raw_u64 = u64::from_ne_bytes(raw[0..8].try_into().ok()?);
    let kind_i32 = i32::from_ne_bytes(raw[8..12].try_into().ok()?);

    match read_il2cpp_class_name(driver, val_tv) {
        Some((cls_name, _)) if cls_name == "NumericTypedValue" => match kind_i32 {
            0 => Some(DecodedTypedValue::Signed(
                crate::live_write::read_numeric_binding_integer(driver, val_tv).unwrap_or(raw_u64 as i64)
            )),
            1 => Some(DecodedTypedValue::Unsigned(raw_u64)),
            2 => Some(DecodedTypedValue::Float(f64::from_bits(raw_u64))),
            _ => None,
        },
        Some((cls_name, _)) if cls_name == "ReferenceTypedValue" => {
            let ptr = raw_u64 as usize;
            if ptr <= 0x10000 || ptr >= 0x7fff_ffff_ffff {
                return None;
            }
            if let Some((t_name, t_ns)) = read_il2cpp_class_name(driver, ptr) {
                if t_name == "String" && t_ns == "System" {
                    let s = fmlev_il2cpp::Il2CppString::read(driver, ptr).ok()?;
                    return Some(DecodedTypedValue::String(s));
                }
                if t_name.ends_with("Reference") && t_ns != "SI.Bindable" {
                    let m_index = driver.read::<i32>(ptr + 0x20).unwrap_or(0);
                    let ref_type = driver.read::<i32>(ptr + 0x24).unwrap_or(0);
                    return Some(DecodedTypedValue::Reference {
                        ptr,
                        ref_type,
                        m_index,
                    });
                }
                if t_ns == "System" {
                    if let Ok(boxed_val) = driver.read::<i32>(ptr + 0x10) {
                        return Some(DecodedTypedValue::Signed(boxed_val as i64));
                    }
                }
            }
            Some(DecodedTypedValue::Object(ptr))
        }
        Some((cls_name, _)) if cls_name.starts_with("TypedValue") => {
            Some(DecodedTypedValue::Signed(raw_u64 as i64))
        }
        _ => match kind_i32 {
            0 => Some(DecodedTypedValue::Signed(raw_u64 as i64)),
            1 => Some(DecodedTypedValue::Unsigned(raw_u64)),
            2 => Some(DecodedTypedValue::Float(f64::from_bits(raw_u64))),
            _ => None,
        },
    }
}

fn extract_binding_number(driver: &fmlev_mem::LinuxMemoryDriver, val_tv: usize) -> Option<u32> {
    match read_typed_value(driver, val_tv)? {
        DecodedTypedValue::Signed(v) => {
            if v >= 0 && v <= u32::MAX as i64 {
                Some(v as u32)
            } else {
                None
            }
        }
        DecodedTypedValue::Unsigned(v) => {
            if v <= u32::MAX as u64 {
                Some(v as u32)
            } else {
                None
            }
        }
        DecodedTypedValue::Float(f) => {
            if f.is_finite() && f >= 0.0 && f <= u32::MAX as f64 {
                Some(f.round() as u32)
            } else {
                None
            }
        }
        DecodedTypedValue::Bool(b) => Some(u32::from(b)),
        DecodedTypedValue::String(s) => {
            let clean = s.trim();
            if let Ok(n) = clean.parse::<u32>() {
                return Some(n);
            }
            if let Some(first) = clean.split(|c: char| !c.is_numeric()).find(|p| !p.is_empty()) {
                if let Ok(n) = first.parse::<u32>() {
                    return Some(n);
                }
            }
            None
        }
        DecodedTypedValue::Reference { .. } | DecodedTypedValue::Object(_) => None,
    }
}

fn extract_binding_string(driver: &fmlev_mem::LinuxMemoryDriver, val_tv: usize) -> Option<String> {
    if let Some(DecodedTypedValue::String(raw_s)) = read_typed_value(driver, val_tv) {
        let (full, short) = decode_fm_link_name(&raw_s);
        let s = if !full.is_empty() { full } else { short };
        let trimmed = s.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    None
}

fn extract_binding_display_string(
    driver: &fmlev_mem::LinuxMemoryDriver,
    val_tv: usize,
) -> Option<String> {
    if let Some(DecodedTypedValue::String(raw_s)) = read_typed_value(driver, val_tv) {
        let (full, short) = decode_fm_link_name(&raw_s);
        let s = if !short.is_empty() { short } else { full };
        let trimmed = s.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    None
}

fn extract_binding_ref(
    driver: &fmlev_mem::LinuxMemoryDriver,
    val_tv: usize,
) -> Option<(i32, i32)> {
    match read_typed_value(driver, val_tv)? {
        DecodedTypedValue::Reference {
            ref_type, m_index, ..
        } if m_index > 0 => Some((ref_type, m_index)),
        DecodedTypedValue::Object(ptr) if ptr > 0x10000 => {
            let m_index = driver.read::<i32>(ptr + 0x20).unwrap_or(0);
            let ref_type = driver.read::<i32>(ptr + 0x24).unwrap_or(0);
            if m_index > 0 && ref_type > 0 {
                Some((ref_type, m_index))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn apply_attribute_by_name(attrs: &mut PlayerAttributes, name: &str, raw_val: u32) {
    if raw_val == 0 || raw_val > 20 {
        return;
    }
    let val = raw_val as u8;
    let norm = name
        .trim()
        .to_lowercase()
        .strip_prefix("attribute")
        .unwrap_or(&name.trim().to_lowercase())
        .replace([' ', '_', '-', '(', ')'], "");

    match norm.as_str() {
        // Technical & Set Pieces
        "marking" => attrs.marking = val,
        "longthrows" => attrs.long_throws = val,
        "punching" => attrs.punching = val,
        "dirtiness" => attrs.dirtiness = val,
        "controversy" => attrs.controversy = val,
        "corners" | "cornertaking" => attrs.corners = val,
        "crossing" => attrs.crossing = val,
        "dribbling" => attrs.dribbling = val,
        "finishing" => attrs.finishing = val,
        "firsttouch" => attrs.first_touch = val,
        "freekicks" | "freekicktaking" => attrs.free_kicks = val,
        "heading" => attrs.heading = val,
        "longshots" => attrs.long_shots = val,
        "passing" => attrs.passing = val,
        "penaltytaking" | "penalties" => attrs.penalty_taking = val,
        "tackling" => attrs.tackling = val,
        "technique" => attrs.technique = val,

        // Mental
        "aggression" => attrs.aggression = val,
        "anticipation" => attrs.anticipation = val,
        "bravery" => attrs.bravery = val,
        "composure" => attrs.composure = val,
        "concentration" => attrs.concentration = val,
        "decisions" => attrs.decisions = val,
        "determination" => attrs.determination = val,
        "flair" => attrs.flair = val,
        "leadership" => attrs.leadership = val,
        "offtheball" => attrs.off_the_ball = val,
        "positioning" => attrs.positioning = val,
        "teamwork" => attrs.teamwork = val,
        "vision" => attrs.vision = val,
        "workrate" => attrs.work_rate = val,

        // Physical
        "acceleration" => attrs.acceleration = val,
        "agility" => attrs.agility = val,
        "balance" => attrs.balance = val,
        "jumpingreach" | "jumping" => attrs.jumping_reach = val,
        "naturalfitness" => attrs.natural_fitness = val,
        "pace" => attrs.pace = val,
        "stamina" => attrs.stamina = val,
        "strength" => attrs.strength = val,

        // Goalkeeping
        "aerialreach" => attrs.aerial_reach = val,
        "commandofarea" => attrs.command_of_area = val,
        "communication" => attrs.communication = val,
        "eccentricity" => attrs.eccentricity = val,
        "handling" => attrs.handling = val,
        "kicking" => attrs.kicking = val,
        "oneonones" => attrs.one_on_ones = val,
        "reflexes" => attrs.reflexes = val,
        "rushingout" | "rushingouttendency" => attrs.rushing_out = val,
        "throwing" => attrs.throwing = val,

        // Hidden
        "consistency" => attrs.consistency = val,
        "importantmatches" => attrs.important_matches = val,
        "injuryproneness" => attrs.injury_proneness = val,
        "versatility" => attrs.versatility = val,
        "adaptability" => attrs.adaptability = val,
        "ambition" => attrs.ambition = val,
        "loyalty" => attrs.loyalty = val,
        "pressure" => attrs.pressure = val,
        "professionalism" => attrs.professionalism = val,
        "sportsmanship" => attrs.sportsmanship = val,
        "temperament" => attrs.temperament = val,
        _ => {}
    }
}

fn read_object_lookup_unid(
    driver: &fmlev_mem::LinuxMemoryDriver,
    val_tv: usize,
) -> Option<u32> {
    if val_tv <= 0x10000 || val_tv >= 0x7fff_ffff_ffff {
        return None;
    }
    let dyn_ref = driver.read_ptr(val_tv + 0x18).ok()?;
    if dyn_ref <= 0x10000 || dyn_ref >= 0x7fff_ffff_ffff {
        return None;
    }
    let entries = driver.read_ptr(dyn_ref + 0x18).ok()?;
    if entries <= 0x10000 || entries >= 0x7fff_ffff_ffff {
        return None;
    }
    let count = driver
        .read::<i32>(dyn_ref + 0x20)
        .unwrap_or(1)
        .clamp(1, 8) as usize;
    for k in 0..count {
        let entry_addr = entries + 0x20 + k * 24;
        let key = driver.read::<u32>(entry_addr + 0x08).unwrap_or(0);
        if key == 0x756e6964 {
            let num_tv = driver.read_ptr(entry_addr + 0x10).unwrap_or(0);
            if let Some(unid) = extract_binding_number(driver, num_tv) {
                if unid > 0 {
                    return Some(unid);
                }
            }
        }
    }
    None
}

fn fm_nation_name_from_unid(unid: u32) -> Option<&'static str> {
    match unid {
        113 => Some("Indonesia"),
        115 => Some("Iraq"),
        116 => Some("Japan"),
        757 => Some("Belgium"),
        761 => Some("Croatia"),
        765 => Some("England"),
        769 => Some("France"),
        771 => Some("Germany"),
        776 => Some("Italy"),
        784 => Some("Netherlands"),
        786 => Some("Norway"),
        788 => Some("Portugal"),
        796 => Some("Spain"),
        801 => Some("Wales"),
        1435 => Some("Australia"),
        1649 => Some("Argentina"),
        1651 => Some("Brazil"),
        _ => None,
    }
}

fn read_type_info_name(
    driver: &fmlev_mem::LinuxMemoryDriver,
    type_info_dyn_ref: usize,
) -> Option<String> {
    if type_info_dyn_ref <= 0x10000 || type_info_dyn_ref >= 0x7fff_ffff_ffff {
        return None;
    }
    let ti_entries = driver.read_ptr(type_info_dyn_ref + 0x18).ok()?;
    if ti_entries <= 0x10000 || ti_entries >= 0x7fff_ffff_ffff {
        return None;
    }
    let count = driver
        .read::<i32>(type_info_dyn_ref + 0x20)
        .unwrap_or(6)
        .clamp(1, 16) as usize;
    for k in 0..count {
        let entry_addr = ti_entries + 0x20 + k * 24;
        let key = driver.read::<u32>(entry_addr + 0x08).unwrap_or(0);
        if key == 0x6e616d65 {
            let name_tv = driver.read_ptr(entry_addr + 0x10).unwrap_or(0);
            if let Some(s) = extract_binding_string(driver, name_tv) {
                return Some(s);
            }
        }
    }
    None
}

fn read_attribute_values_number(
    driver: &fmlev_mem::LinuxMemoryDriver,
    values_dyn_ref: usize,
) -> Option<u32> {
    if values_dyn_ref <= 0x10000 || values_dyn_ref >= 0x7fff_ffff_ffff {
        return None;
    }
    let val_entries = driver.read_ptr(values_dyn_ref + 0x18).ok()?;
    if val_entries <= 0x10000 || val_entries >= 0x7fff_ffff_ffff {
        return None;
    }
    let count = driver
        .read::<i32>(values_dyn_ref + 0x20)
        .unwrap_or(4)
        .clamp(1, 16) as usize;
    for k in 0..count {
        let entry_addr = val_entries + 0x20 + k * 24;
        let key = driver.read::<u32>(entry_addr + 0x08).unwrap_or(0);
        if key == 0x7074766c || key == 0x44667a45 {
            let num_tv = driver.read_ptr(entry_addr + 0x10).unwrap_or(0);
            if let Some(n) = extract_binding_number(driver, num_tv) {
                if (1..=20).contains(&n) {
                    return Some(n);
                }
            }
        }
    }
    None
}

fn extract_open_profile_attribute_list(
    driver: &fmlev_mem::LinuxMemoryDriver,
    list_tv: usize,
    dyn_ref_props: &HashMap<(usize, String), usize>,
    attrs: &mut PlayerAttributes,
) {
    if list_tv <= 0x10000 || list_tv >= 0x7fff_ffff_ffff {
        return;
    }
    let list_ptr = driver.read_ptr(list_tv + 0x18).unwrap_or(0);
    if list_ptr <= 0x10000 || list_ptr >= 0x7fff_ffff_ffff {
        return;
    }
    let items_ptr = driver.read_ptr(list_ptr + 0x10).unwrap_or(0);
    let size = driver.read::<i32>(list_ptr + 0x18).unwrap_or(0);
    if items_ptr <= 0x10000 || size <= 0 {
        return;
    }

    let clamped = (size as usize).min(32);
    for i in 0..clamped {
        let elem_tv = driver.read_ptr(items_ptr + 0x20 + i * 8).unwrap_or(0);
        if elem_tv <= 0x10000 {
            continue;
        }
        let row_dyn_ref = driver.read_ptr(elem_tv + 0x18).unwrap_or(0);
        if row_dyn_ref <= 0x10000 {
            continue;
        }

        let mut attr_name: Option<String> = None;
        let mut attr_val: Option<u32> = None;

        // Path A: Direct row_dyn_ref Dictionary<uint, TypedValue> entries at row_dyn_ref + 0x18
        let row_entries = driver.read_ptr(row_dyn_ref + 0x18).unwrap_or(0);
        let row_count = driver
            .read::<i32>(row_dyn_ref + 0x20)
            .unwrap_or(2)
            .clamp(1, 8) as usize;
        if row_entries > 0x10000 && row_entries < 0x7fff_ffff_ffff {
            for r in 0..row_count {
                let child_tv = driver
                    .read_ptr(row_entries + 0x20 + r * 24 + 0x10)
                    .unwrap_or(0);
                let child_dyn_ref = if child_tv > 0x10000 {
                    driver.read_ptr(child_tv + 0x18).unwrap_or(0)
                } else {
                    0
                };
                if child_dyn_ref > 0x10000 {
                    if attr_name.is_none() {
                        attr_name = read_type_info_name(driver, child_dyn_ref);
                    }
                    if attr_val.is_none() {
                        attr_val = read_attribute_values_number(driver, child_dyn_ref);
                    }
                }
            }
        }

        // Path B: Fallback to m_ctx mappings for (row_dyn_ref, "AttributeTypeInfo") & (row_dyn_ref, "AttributeValues")
        if attr_name.is_none() {
            if let Some(&ti_tv) = dyn_ref_props.get(&(row_dyn_ref, "AttributeTypeInfo".to_string()))
            {
                let ti_dyn_ref = driver.read_ptr(ti_tv + 0x18).unwrap_or(0);
                attr_name = read_type_info_name(driver, ti_dyn_ref);
            }
        }
        if attr_val.is_none() {
            if let Some(&av_tv) = dyn_ref_props.get(&(row_dyn_ref, "AttributeValues".to_string())) {
                let av_dyn_ref = driver.read_ptr(av_tv + 0x18).unwrap_or(0);
                attr_val = read_attribute_values_number(driver, av_dyn_ref);
            }
        }

        if let (Some(name), Some(val)) = (attr_name, attr_val) {
            apply_attribute_by_name(attrs, &name, val);
        }
    }
}

fn extract_footedness(
    driver: &fmlev_mem::LinuxMemoryDriver,
    val_tv: usize,
    pos: &mut PlayerPositions,
) {
    if val_tv <= 0x10000 || val_tv >= 0x7fff_ffff_ffff {
        return;
    }
    let dyn_ref = driver.read_ptr(val_tv + 0x18).unwrap_or(0);
    if dyn_ref <= 0x10000 || dyn_ref >= 0x7fff_ffff_ffff {
        return;
    }
    let entries = driver.read_ptr(dyn_ref + 0x18).unwrap_or(0);
    let count = driver
        .read::<i32>(dyn_ref + 0x20)
        .unwrap_or(10)
        .clamp(1, 16) as usize;
    if entries <= 0x10000 || entries >= 0x7fff_ffff_ffff {
        return;
    }

    for k in 0..count {
        let ea = entries + 0x20 + k * 24;
        if driver.read::<i32>(ea).unwrap_or(-1) < 0 { continue; }
        let key = driver.read::<u32>(ea + 0x08).unwrap_or(0);
        let num_tv = driver.read_ptr(ea + 0x10).unwrap_or(0);
        if let Some(n) = extract_binding_number(driver, num_tv).filter(|n| (1..=20).contains(n)) {
            if key == 0x32342020 {
                // "24  " -> LeftFootedness
                pos.left_foot = n as u8;
            } else if key == 0x32332020 {
                // "23  " -> RightFootedness
                pos.right_foot = n as u8;
            }
        }
    }
}

fn extract_position_aptitude_list(
    driver: &fmlev_mem::LinuxMemoryDriver,
    val_tv: usize,
    pos: &mut PlayerPositions,
) {
    if val_tv <= 0x10000 || val_tv >= 0x7fff_ffff_ffff {
        return;
    }
    let list_ptr = driver.read_ptr(val_tv + 0x18).unwrap_or(0);
    if list_ptr <= 0x10000 || list_ptr >= 0x7fff_ffff_ffff {
        return;
    }
    let items = driver.read_ptr(list_ptr + 0x10).unwrap_or(0);
    let size = driver
        .read::<i32>(list_ptr + 0x18)
        .unwrap_or(0)
        .clamp(0, 32) as usize;
    if items <= 0x10000 || items >= 0x7fff_ffff_ffff {
        return;
    }

    for i in 0..size {
        let etv = driver.read_ptr(items + 0x20 + i * 8).unwrap_or(0);
        let row_dyn = if etv > 0x10000 {
            driver.read_ptr(etv + 0x18).unwrap_or(0)
        } else {
            0
        };
        if row_dyn <= 0x10000 {
            continue;
        }
        let re = driver.read_ptr(row_dyn + 0x18).unwrap_or(0);
        let rc = driver
            .read::<i32>(row_dyn + 0x20)
            .unwrap_or(3)
            .clamp(1, 8) as usize;
        if re <= 0x10000 {
            continue;
        }

        let mut mask = 0u64;
        let mut rating = 0u8;
        for k in 0..rc {
            let ea = re + 0x20 + k * 24;
            if driver.read::<i32>(ea).unwrap_or(-1) < 0 { continue; }
            let key = driver.read::<u32>(ea + 0x08).unwrap_or(0);
            let num_tv = driver.read_ptr(ea + 0x10).unwrap_or(0);
            if key == 0x506f7369 {
                // "Posi"
                if let Some(m) = extract_binding_number(driver, num_tv) {
                    mask = m as u64;
                }
            } else if key == 0x7074766c {
                // "ptvl"
                if let Some(r) = extract_binding_number(driver, num_tv).filter(|r| (1..=20).contains(r)) {
                    rating = r as u8;
                }
            }
        }

        if rating > 0 {
            match mask {
                1 => pos.gk = rating,
                4 => pos.dr = rating,
                8 => pos.dl = rating,
                16 => pos.dc = rating,
                32 => pos.wbr = rating,
                64 => pos.wbl = rating,
                128 => pos.dm = rating,
                256 => pos.mr = rating,
                512 => pos.ml = rating,
                1024 => pos.mc = rating,
                2048 => pos.amr = rating,
                4096 => pos.aml = rating,
                8192 => pos.amc = rating,
                16384 => pos.st = rating,
                _ => {}
            }
        }
    }
}

/// Extracts live bound players from FM26's `SI.Bindable.BindingSubsystem` in memory.
fn extract_live_bound_players(driver: &fmlev_mem::LinuxMemoryDriver, b_sub: usize) -> Vec<Player> {
    let m_data = driver.read_ptr(b_sub + 0x78).unwrap_or(0);
    let m_ctx = driver.read_ptr(b_sub + 0x88).unwrap_or(0);
    if m_data <= 0x10000 || m_ctx <= 0x10000 {
        return Vec::new();
    }

    let data_items = driver.read_ptr(m_data + 0x10).unwrap_or(0);
    let data_size = driver.read::<i32>(m_data + 0x18).unwrap_or(0);
    let entries = driver.read_ptr(m_ctx + 0x18).unwrap_or(0);
    let count = driver.read::<i32>(m_ctx + 0x20).unwrap_or(0);

    if data_items <= 0x10000 || entries <= 0x10000 || count <= 0 || data_size <= 0 {
        return Vec::new();
    }

    let max_entries = (count as usize).min(50_000);
    let mut raw_entries = vec![0u8; max_entries * 56];
    if driver.read_bytes(entries + 0x20, &mut raw_entries).is_err() {
        return Vec::new();
    }

    #[derive(Default)]
    struct PartialPerson {
        name: String,
        short_name: String,
        club_name: String,
        nationality: String,
        position: String,
        age: u8,
        condition: u16,
        sharpness: u16,
        morale: u8,
        current_ability: u8,
        potential_ability: u8,
        club_ref: Option<i32>,
        team_ref: Option<i32>,
        report_team_ref: Option<i32>,
        nation_ref: Option<i32>,
        attr_list_tvs: Vec<usize>,
        pos_list_tv: Option<usize>,
        foot_tv: Option<usize>,
        attributes: Option<PlayerAttributes>,
    }

    let mut map: HashMap<i32, PartialPerson> = HashMap::new();
    let mut ref_names: HashMap<(i32, i32), String> = HashMap::new();
    let mut team_to_club: HashMap<i32, i32> = HashMap::new();
    let mut club_to_team: HashMap<i32, i32> = HashMap::new();
    let mut dyn_ref_props: HashMap<(usize, String), usize> = HashMap::new();

    for i in 0..max_entries {
        let base_off = i * 56;
        if i32::from_ne_bytes(raw_entries[base_off..base_off + 4].try_into().unwrap()) < 0 { continue; }
        let src_tv = usize::from_ne_bytes(
            raw_entries[base_off + 8..base_off + 16]
                .try_into()
                .unwrap_or([0; 8]),
        );
        let prop_ptr = usize::from_ne_bytes(
            raw_entries[base_off + 16..base_off + 24]
                .try_into()
                .unwrap_or([0; 8]),
        );
        let data_key = (u64::from_ne_bytes(
            raw_entries[base_off + 40..base_off + 48]
                .try_into()
                .unwrap_or([0; 8]),
        ) & 0xFFFF_FFFF) as usize;

        if src_tv <= 0x10000 || prop_ptr <= 0x10000 || data_key >= (data_size as usize) {
            continue;
        }

        let ref_obj = driver.read_ptr(src_tv + 0x18).unwrap_or(0);
        if ref_obj <= 0x10000 {
            continue;
        }

        let m_idx = driver.read::<i32>(ref_obj + 0x20).unwrap_or(0);
        let t_typ = driver.read::<i32>(ref_obj + 0x24).unwrap_or(0);

        let p_str_ptr = driver.read_ptr(prop_ptr + 0x10).unwrap_or(0);
        let prop_name = fmlev_il2cpp::Il2CppString::read(driver, p_str_ptr)
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| fmlev_il2cpp::Il2CppString::read(driver, prop_ptr).ok())
            .unwrap_or_default();

        if prop_name.is_empty() {
            continue;
        }

        let d_obj = driver.read_ptr(data_items + 0x20 + data_key * 8).unwrap_or(0);
        let val_tv = if d_obj > 0x10000 {
            driver.read_ptr(d_obj + 0x30).unwrap_or(0)
        } else {
            0
        };
        if val_tv <= 0x10000 {
            continue;
        }

        // Record DynamicReference (t_typ == -1) AttributeTypeInfo / AttributeValues mappings
        if t_typ == -1 {
            if matches!(prop_name.as_str(), "AttributeTypeInfo" | "AttributeValues") {
                dyn_ref_props.insert((ref_obj, prop_name), val_tv);
            }
            continue;
        }

        // Collect reference names and Team -> Club chains for Club (3), Nation (10), Team (24), Comp (25)
        if matches!(t_typ, 3 | 10 | 24 | 25) && m_idx > 0 {
            match prop_name.as_str() {
                "Name" | "FullName" | "ClubName" | "TeamName" | "NationName" => {
                    if let Some(s) = extract_binding_display_string(driver, val_tv) {
                        ref_names.insert((t_typ, m_idx), s);
                    }
                }
                "ThreeLetterName" | "ShortName" => {
                    if let Some(s) = extract_binding_display_string(driver, val_tv) {
                        ref_names.entry((t_typ, m_idx)).or_insert(s);
                    }
                }
                "ObjectLookupData" if t_typ == 10 => {
                    if let Some(unid) = read_object_lookup_unid(driver, val_tv) {
                        if let Some(nation_name) = fm_nation_name_from_unid(unid) {
                            ref_names
                                .entry((10, m_idx))
                                .or_insert_with(|| nation_name.to_string());
                        }
                    }
                }
                "Club" | "ClubForObjectLookupData" if t_typ == 24 => {
                    if let Some((target_type, target_idx)) = extract_binding_ref(driver, val_tv) {
                        if target_type == 3 && target_idx > 0 {
                            team_to_club.insert(m_idx, target_idx);
                            club_to_team.entry(target_idx).or_insert(m_idx);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }

        if t_typ != 1 || m_idx <= 0 {
            continue;
        }

        let entry = map.entry(m_idx).or_default();
        match prop_name.as_str() {
            "Name" | "FullName" => {
                if let Some(s) = extract_binding_string(driver, val_tv) {
                    if entry.name.is_empty()
                        || entry.name.contains(". ")
                        || s.len() > entry.name.len()
                    {
                        entry.name = s;
                    }
                }
            }
            "InitialSurname" | "ShirtName" => {
                if let Some(s) = extract_binding_string(driver, val_tv) {
                    if entry.short_name.is_empty() {
                        entry.short_name = s.clone();
                    }
                    if entry.name.is_empty() {
                        entry.name = s;
                    }
                }
            }
            "Club" => {
                if let Some((target_type, target_idx)) = extract_binding_ref(driver, val_tv) {
                    if target_type == 3 {
                        entry.club_ref = Some(target_idx);
                    } else if target_type == 24 {
                        entry.team_ref = Some(target_idx);
                    }
                } else if let Some(s) = extract_binding_display_string(driver, val_tv) {
                    if entry.club_name.is_empty() {
                        entry.club_name = s;
                    }
                }
            }
            "Team" => {
                if let Some((target_type, target_idx)) = extract_binding_ref(driver, val_tv) {
                    if target_type == 24 {
                        entry.team_ref = Some(target_idx);
                    } else if target_type == 3 {
                        entry.club_ref = Some(target_idx);
                    }
                } else if let Some(s) = extract_binding_display_string(driver, val_tv) {
                    if entry.club_name.is_empty() {
                        entry.club_name = s;
                    }
                }
            }
            "ReportTeam" => {
                if let Some((target_type, target_idx)) = extract_binding_ref(driver, val_tv) {
                    if target_type == 24 {
                        entry.report_team_ref = Some(target_idx);
                    }
                }
            }
            "ClubName" | "TeamName" | "PlayerClub" | "PlayerClubName" | "ContractClub" => {
                if let Some(s) = extract_binding_display_string(driver, val_tv) {
                    if entry.club_name.is_empty() {
                        entry.club_name = s;
                    }
                }
            }
            "Nation" => {
                if let Some((target_type, target_idx)) = extract_binding_ref(driver, val_tv) {
                    if target_type == 10 {
                        entry.nation_ref = Some(target_idx);
                    }
                } else if let Some(s) = extract_binding_display_string(driver, val_tv) {
                    if entry.nationality.is_empty() {
                        entry.nationality = s;
                    }
                }
            }
            "Nationality" | "PlayerNationality" | "NationName" => {
                if let Some(s) = extract_binding_display_string(driver, val_tv) {
                    if entry.nationality.is_empty() {
                        entry.nationality = s;
                    }
                }
            }
            "PositionCombinedString" => {
                if let Some(s) = extract_binding_display_string(driver, val_tv) {
                    entry.position = s;
                }
            }
            "BestPositionShortString" | "Position" | "PositionString" => {
                if let Some(s) = extract_binding_display_string(driver, val_tv) {
                    if entry.position.is_empty() {
                        entry.position = s;
                    }
                }
            }
            "Age" | "PlayerAge" => {
                if let Some(n) = extract_binding_number(driver, val_tv) {
                    if (14..=65).contains(&n) {
                        entry.age = n as u8;
                    }
                }
            }
            "PlayerCurrentAbility" | "CurrentAbility" | "CA" => {
                if let Some(n) = extract_binding_number(driver, val_tv) {
                    if (1..=200).contains(&n) {
                        entry.current_ability = n as u8;
                    }
                }
            }
            "PlayerPotentialAbility" | "PotentialAbility" | "PA" => {
                if let Some(n) = extract_binding_number(driver, val_tv) {
                    if (1..=200).contains(&n) {
                        entry.potential_ability = n as u8;
                    }
                }
            }
            "Condition" => {
                if let Some(n) = extract_binding_number(driver, val_tv) {
                    if n > 0 && n <= 100 {
                        entry.condition = (n as u16) * 100;
                    } else if n > 100 && n <= 10000 {
                        entry.condition = n as u16;
                    }
                }
            }
            "Sharpness" => {
                if let Some(n) = extract_binding_number(driver, val_tv) {
                    if n > 0 && n <= 100 {
                        entry.sharpness = (n as u16) * 100;
                    } else if n > 100 && n <= 10000 {
                        entry.sharpness = n as u16;
                    }
                }
            }
            "Morale" | "HierarchyOverallHappinessLevel" => {
                if let Some(n) = extract_binding_number(driver, val_tv) {
                    if n > 0 && n <= 25 {
                        entry.morale = ((n as u8) * 5).min(100);
                    } else if n > 25 && n <= 100 {
                        entry.morale = n as u8;
                    }
                }
            }
            "AttributesMental"
            | "AttributesPhysical"
            | "AttributesSetPieces"
            | "AttributesTechnical"
            | "AttributesGoalkeeping" => {
                entry.attr_list_tvs.push(val_tv);
            }
            "PositionAptitudeList" => {
                entry.pos_list_tv = Some(val_tv);
            }
            "Footedness" => {
                entry.foot_tv = Some(val_tv);
            }
            other => {
                if let Some(n) = extract_binding_number(driver, val_tv) {
                    if (1..=20).contains(&n) {
                        let attrs = entry.attributes.get_or_insert_with(PlayerAttributes::default);
                        apply_attribute_by_name(attrs, other, n);
                    }
                }
            }
        }
    }

    let resolve_team_or_club_name = |team_idx: Option<i32>, club_idx: Option<i32>| -> Option<String> {
        if let Some(c_idx) = club_idx {
            if let Some(name) = ref_names.get(&(3, c_idx)) {
                return Some(name.clone());
            }
        }
        if let Some(t_idx) = team_idx {
            if let Some(name) = ref_names.get(&(24, t_idx)) {
                return Some(name.clone());
            }
            if let Some(&c_idx) = team_to_club.get(&t_idx) {
                if let Some(name) = ref_names.get(&(3, c_idx)) {
                    return Some(name.clone());
                }
            }
        }
        if let Some(c_idx) = club_idx {
            if let Some(&t_idx) = club_to_team.get(&c_idx) {
                if let Some(name) = ref_names.get(&(24, t_idx)) {
                    return Some(name.clone());
                }
            }
        }
        None
    };

    let mut out = Vec::new();
    for (id, mut p) in map {
        let display_name = if !p.name.is_empty() {
            p.name
        } else if !p.short_name.is_empty() {
            p.short_name
        } else {
            continue;
        };

        if p.nationality.is_empty() {
            if let Some(n_idx) = p.nation_ref {
                if let Some(n_name) = ref_names.get(&(10, n_idx)) {
                    p.nationality = n_name.clone();
                }
            }
        }

        if p.club_name.is_empty() {
            if let Some(c_name) = resolve_team_or_club_name(p.team_ref, p.club_ref)
                .or_else(|| resolve_team_or_club_name(p.report_team_ref, None))
            {
                p.club_name = c_name;
            }
        }

        if !p.attr_list_tvs.is_empty() {
            let attrs = p.attributes.get_or_insert_with(PlayerAttributes::default);
            for list_tv in &p.attr_list_tvs {
                extract_open_profile_attribute_list(driver, *list_tv, &dyn_ref_props, attrs);
            }
        }

        let mut live_pos: Option<PlayerPositions> = None;
        if let Some(pos_tv) = p.pos_list_tv {
            let mut pos = PlayerPositions::unknown();
            extract_position_aptitude_list(driver, pos_tv, &mut pos);
            live_pos = Some(pos);
        }
        if let Some(foot_tv) = p.foot_tv {
            let pos = live_pos.get_or_insert_with(PlayerPositions::unknown);
            extract_footedness(driver, foot_tv, pos);
        }

        let mut player = build_template_live_player(id, display_name, p.position.clone());
        player.club_id = p.club_ref
            .or_else(|| p.team_ref.and_then(|team| team_to_club.get(&team).copied()))
            .or_else(|| p.report_team_ref.and_then(|team| team_to_club.get(&team).copied()))
            .unwrap_or(0);
        if let Some(pos) = live_pos {
            player.positions = pos;
        }
        if !p.club_name.is_empty() {
            player.club_name = p.club_name;
        }
        if !p.nationality.is_empty() {
            player.nationality = p.nationality;
        }
        if p.age > 0 {
            player.age = p.age;
        }
        if p.current_ability > 0 {
            player.current_ability = p.current_ability;
        }
        if p.potential_ability > 0 {
            player.potential_ability = p.potential_ability;
        }
        if p.condition > 0 {
            player.condition = p.condition;
        }
        if p.sharpness > 0 {
            player.sharpness = p.sharpness;
        }
        if p.morale > 0 {
            player.morale = p.morale;
        }
        if let Some(attrs) = p.attributes {
            player.attributes = attrs;
        }
        out.push(player);
    }
    out.sort_by_key(|p| p.id);
    out
}

/// Decodes FM26's `\x01<Base64Metadata>\x02<ShortName>\x01` link strings into `(full_name, short_name)`.
pub fn decode_fm_link_name(raw: &str) -> (String, String) {
    if let Some(start) = raw.find('\x02') {
        let rest = &raw[start + 1..];
        let end = rest.find('\x01').unwrap_or(rest.len());
        let short_name = rest[..end].trim().to_string();

        let b64_part = raw[..start].trim_start_matches('\x01').trim();
        if let Some(decoded) = decode_base64_simple(b64_part) {
            let text = String::from_utf8_lossy(&decoded);
            if let Some(idx) = text.find("Click to view ") {
                let after = &text[idx + "Click to view ".len()..];
                let name_end = after
                    .find("'s profile")
                    .or_else(|| after.find("’s profile"))
                    .or_else(|| after.find("' profile"))
                    .or_else(|| after.find("’ profile"))
                    .or_else(|| after.find('\0'))
                    .unwrap_or(after.len());
                let full = after[..name_end].trim().to_string();
                if !full.is_empty() {
                    return (full, short_name);
                }
            }
        }
        (short_name.clone(), short_name)
    } else {
        let clean = raw.trim_matches(|c: char| c.is_control()).trim().to_string();
        (clean.clone(), clean)
    }
}

fn decode_base64_simple(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u8;
    for b in input.bytes() {
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => continue,
        };
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
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
        requested.contract.release_clause = 123;
        requested.name = "Another name".into();
        assert_eq!(unsupported_live_player_changes(&previous, &requested), vec!["contract.release_clause", "name"]);
    }

    #[test]
    fn unknown_edit_fields_and_identity_edits_are_rejected() {
        let mut manager = GameContextManager::new_offline();
        let previous = manager.get_context().player;
        for field in ["id", "user_modified", "attributes.unknown", "attributes.passing.extra"] {
            assert!(manager.update_player_fields(previous.clone(), Some(&[field.into()])).is_err());
        }
    }
    use super::*;

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
}
