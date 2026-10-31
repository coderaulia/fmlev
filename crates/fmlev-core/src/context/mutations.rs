use std::collections::HashMap;
use tracing::info;

use crate::error::{CoreError, Result};
use crate::models::{
    ClubAffiliate, ClubFinances, CompetitionDetails, NationDetails, Player,
    PlayerAttributes, PlayerPositions,
};
use super::inspector::{
    build_template_live_player, extract_live_bound_players, merge_player_attributes,
    resolve_binding_subsystem, unknown_clubfinances, unknown_competitiondetails,
    unknown_nationdetails, unsupported_live_club_changes, unsupported_live_player_changes,
};
use super::GameContextManager;

/// How long polling waits before repeating a failed native discovery of the followed id.
const NATIVE_DISCOVERY_RETRY: std::time::Duration = std::time::Duration::from_secs(60);

impl GameContextManager {
    /// The configured entity offsets describe test records, not FM26's native records.
    /// Managed PersonReference/ClubReference objects must never be treated as those records.
    pub(crate) fn ensure_record_writes_supported(&self) -> Result<()> {
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
                // Read from the native contract on load; present only when that read succeeded for this player.
                let contract = &self.context.player.contract;
                if contract.squad_status_code.is_some() { fields.push("contract.squad_status".to_string()); }
                if contract.release_clause_present == Some(true) { fields.push("contract.release_clause".to_string()); }
            }
        }
        Some(fields)
    }

    pub fn discover_loaded_players(&mut self) -> Result<usize> {
        let (seed, driver) = self.player_discovery_seed()?;
        let found = Self::collect_loaded_players(&seed, &driver)?;
        Ok(self.apply_loaded_players(found))
    }

    /// Phase 1 of loaded-player discovery (needs `&mut self`, quick): the calibrated seed record and a
    /// copy of the driver, so the scan can run without holding the server state lock.
    pub fn player_discovery_seed(&mut self) -> Result<(crate::native_player::NativePlayerRecord, fmlev_mem::LinuxMemoryDriver)> {
        let seed = self.verified_native_player(self.context.player.id)?;
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live game session".into()))?;
        Ok((seed, session.driver))
    }

    /// Phase 2 (blocking full scan, no `self`): every loaded peer of `seed` with its name, positions,
    /// attributes and CA/PA read into a template player.
    pub fn collect_loaded_players(
        seed: &crate::native_player::NativePlayerRecord,
        driver: &fmlev_mem::LinuxMemoryDriver,
    ) -> Result<Vec<(crate::native_player::NativePlayerRecord, Player)>> {
        let mut found = Vec::new();
        for record in seed.discover_loaded(driver)? {
            let id = record.index();
            let mut player = build_template_live_player(id, record.name(driver)?, String::new());
            player.positions = record.read_positions(driver)?;
            player.position = player.positions.to_position_string();
            record.refresh_attributes(driver, &mut player.attributes)?;
            record.refresh_abilities(driver, &mut player)?;
            found.push((record, player));
        }
        Ok(found)
    }

    /// Phase 3 (needs `&mut self`, quick): merge the scan into the registries. Existing players keep
    /// their other fields; native fields are refreshed. Returns the number of native records.
    pub fn apply_loaded_players(&mut self, found: Vec<(crate::native_player::NativePlayerRecord, Player)>) -> usize {
        for (record, scanned) in found {
            let id = record.index();
            let player = self.player_registry.entry(id).or_insert_with(|| scanned.clone());
            player.positions = scanned.positions;
            player.position = scanned.position;
            player.attributes = scanned.attributes;
            player.current_ability = scanned.current_ability;
            player.potential_ability = scanned.potential_ability;
            self.native_player_records.insert(id, record);
            self.failed_native_fingerprints.remove(&id);
        }
        self.native_player_records.len()
    }

    /// Adds a minimal registry entry for a nation found by the search index, so it can be selected.
    /// Live sessions only; an existing entry is kept.
    pub fn ensure_live_nation(&mut self, id: i32, name: &str) {
        if self.is_live_session() {
            self.nation_registry.entry(id).or_insert_with(|| {
                let mut nation = super::inspector::unknown_nationdetails();
                nation.id = id;
                nation.name = name.to_owned();
                nation
            });
        }
    }

    /// Competition counterpart of `ensure_live_nation`.
    pub fn ensure_live_competition(&mut self, id: i32, name: &str, nation_name: Option<&str>) {
        if self.is_live_session() {
            self.competition_registry.entry(id).or_insert_with(|| {
                let mut comp = super::inspector::unknown_competitiondetails();
                comp.id = id;
                comp.name = name.to_owned();
                comp.nation_name = nation_name.unwrap_or_default().to_owned();
                comp
            });
        }
    }

    /// True when attached to a real game process (not the in-process test session).
    pub fn is_live_session(&self) -> bool {
        self.session.as_ref().is_some_and(|session| session.driver.pid() != std::process::id() as i32)
    }

    /// Populate global search once a real player's attributes have calibrated
    /// the native layout. Manual discovery remains available for retry.
    pub fn maybe_auto_discover_players(&mut self) -> Option<Result<usize>> {
        if self.auto_discovery_attempted || self.native_player_records.len() > 100 {
            return None;
        }
        let id = self.context.player.id;
        if !self.native_player_records.get(&id).is_some_and(|record| record.attributes_verified()) {
            return None;
        }
        self.auto_discovery_attempted = true;
        Some(self.discover_loaded_players())
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

    /// Native discovery for selection and polling. A failed attempt is a full process scan (seconds,
    /// under the state lock), so it is not repeated for the same id within `NATIVE_DISCOVERY_RETRY`.
    pub(crate) fn discover_native_player_throttled(&mut self, id: i32) {
        let now = std::time::Instant::now();
        if self.native_discovery_retry_at.get(&id).is_some_and(|at| now < *at) {
            return;
        }
        if self.verified_native_player(id).is_ok() {
            self.native_discovery_retry_at.remove(&id);
        } else {
            self.native_discovery_retry_at.insert(id, now + NATIVE_DISCOVERY_RETRY);
        }
    }

    pub(crate) fn verified_native_player(&mut self, id: i32) -> Result<crate::native_player::NativePlayerRecord> {
        if let Some(mut record) = self.native_player_records.get(&id).cloned() {
            if !record.attributes_verified() {
                if let Some(session) = self.session.as_ref() {
                    if let Some(player) = self.player_registry.get(&id) {
                        let _ = record.verify_attributes(&session.driver, &player.attributes);
                    } else {
                        let _ = record.verify_native_attributes(&session.driver);
                    }
                    self.native_player_records.insert(id, record.clone());
                }
            }
            return Ok(record);
        }
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;

        let mut record_opt = None;
        if let Some(subsystem) = resolve_binding_subsystem(session) {
            if let Ok(fingerprint) = crate::live_write::read_person_native_fingerprint(&session.driver, subsystem, id) {
                if self.failed_native_fingerprints.get(&id) != Some(&fingerprint) {
                    match crate::native_player::NativePlayerRecord::discover(
                        &session.driver, id, fingerprint.unique_id, fingerprint.aptitudes,
                        fingerprint.left_foot, fingerprint.right_foot,
                    ) {
                        Ok(record) => {
                            self.failed_native_fingerprints.remove(&id);
                            record_opt = Some(record);
                        }
                        Err(_) => {
                            self.failed_native_fingerprints.insert(id, fingerprint);
                        }
                    }
                }
            }
        }

        let mut record = match record_opt {
            Some(r) => r,
            None => crate::native_player::NativePlayerRecord::discover_by_index(&session.driver, id)?,
        };

        if let Some(player) = self.player_registry.get(&id) {
            let _ = record.verify_attributes(&session.driver, &player.attributes);
        } else {
            let _ = record.verify_native_attributes(&session.driver);
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
        if self.session.as_ref().is_some_and(|session|
            session.driver.pid() != std::process::id() as i32 || session.bases.game_assembly_base.is_some())
        {
            return self.update_live_club(club);
        }
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

    fn update_live_club(&mut self, club: ClubFinances) -> Result<()> {
        let club_id = club.club_id;
        if let Some(previous) = self.club_registry.get(&club_id) {
            let unsupported = unsupported_live_club_changes(previous, &club);
            if !unsupported.is_empty() {
                return Err(CoreError::InvalidValue(format!(
                    "No verified live write path for: {}. No fields were written. Balance, transfer budget and facility ratings can be saved.", unsupported.join(", ")
                )));
            }
        }
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;
        let b_sub = resolve_binding_subsystem(session).unwrap_or(0);
        crate::live_club::write_live_club(
            &session.driver, b_sub, club_id, &club, self.club_registry.get(&club_id),
        )?;
        self.user_modified_clubs.insert(club_id);
        self.context.club = club.clone();
        self.club_registry.insert(club_id, club);
        // Store what the game now holds, not the submitted form, so the next save diffs against truth.
        let _ = self.read_club_native(club_id);
        Ok(())
    }

    pub fn update_nation(&mut self, nation: NationDetails) -> Result<()> {
        if self.session.as_ref().is_some_and(|session|
            session.driver.pid() != std::process::id() as i32 || session.bases.game_assembly_base.is_some())
        {
            return self.update_live_nation(nation);
        }
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

    fn update_live_nation(&mut self, nation: NationDetails) -> Result<()> {
        self.update_live_nation_checked(nation)
    }

    pub fn update_competition(&mut self, comp: CompetitionDetails) -> Result<()> {
        if self.session.as_ref().is_some_and(|session|
            session.driver.pid() != std::process::id() as i32 || session.bases.game_assembly_base.is_some())
        {
            return self.update_live_competition(comp);
        }
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

    fn update_live_competition(&mut self, comp: CompetitionDetails) -> Result<()> {
        self.update_live_competition_checked(comp)
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
        if self.session.as_ref().is_some_and(|session|
            session.driver.pid() != std::process::id() as i32 || session.bases.game_assembly_base.is_some())
            && matches!(normalized.as_str(), "heal_player" | "heal")
        {
            let record = self.verified_native_player(self.context.player.id)?;
            let session = self.session.as_ref().unwrap();
            record.heal(&session.driver)?;
            self.context.player.condition = 10000;
            self.context.player.sharpness = 10000;
            self.context.player.is_injured = false;
            self.player_registry
                .insert(self.context.player.id, self.context.player.clone());
            info!("Applied live quick action: heal_player (Player ID: {})", self.context.player.id);
            return Ok(());
        }
        if self.session.as_ref().is_some_and(|session|
            session.driver.pid() != std::process::id() as i32 || session.bases.game_assembly_base.is_some())
            && matches!(normalized.as_str(), "remove_promises" | "clear_promises")
        {
            return Err(CoreError::InvalidValue(
                "Clearing promises has no verified native field yet. Use Heal for condition and sharpness.".to_string(),
            ));
        }
        if self.session.as_ref().is_some_and(|session|
            session.driver.pid() != std::process::id() as i32 || session.bases.game_assembly_base.is_some())
            && matches!(normalized.as_str(), "heal_squad")
        {
            let target_club_id = self.context.club.club_id;
            let active_id = self.context.player.id;
            let active_rec = self.verified_native_player(active_id).ok();
            let session = self.session.as_ref().unwrap();
            for (&pid, record) in &self.native_player_records {
                if let Some(player) = self.player_registry.get(&pid) {
                    if target_club_id == 0 || player.club_id == target_club_id {
                        let _ = record.heal(&session.driver);
                    }
                }
            }
            if let Some(rec) = active_rec {
                let _ = rec.heal(&session.driver);
            }
            self.context.player.condition = 10000;
            self.context.player.sharpness = 10000;
            self.context.player.is_injured = false;
            for player in self.player_registry.values_mut() {
                if target_club_id == 0 || player.club_id == target_club_id {
                    player.condition = 10000;
                    player.sharpness = 10000;
                    player.is_injured = false;
                }
            }
            info!("Applied live quick action: heal_squad (Club ID: {})", target_club_id);
            return Ok(());
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
                | "remove_promises" | "clear_promises"
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
        let is_record_action = is_club_action
            || matches!(
                normalized.as_str(),
                "golden_generation" | "nation_golden_generation" | "relax_rules" | "competition_relax_rules"
            );
        if is_record_action && self.ensure_record_writes_supported().is_err() {
            // Live session: presets would write UI-binding caches. Only the native facility maximiser is verified.
            if matches!(normalized.as_str(), "max_facilities" | "facilities" | "club_max_facilities") {
                let club_id = self.context.club.club_id;
                let mut club = self.read_club_native(club_id)?;
                let data_analysis = club.data_analysis_facilities; // unverified natively: keep as is
                club.max_all_facilities();
                club.data_analysis_facilities = data_analysis;
                return self.update_club(club);
            }
            return Err(CoreError::InvalidValue(
                "This preset has no verified native write path on a live session and was not applied. Use the individual native editors.".to_string(),
            ));
        }
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
            "remove_promises" | "clear_promises" => {
                self.context.player.contract.happiness = "Delighted".to_string();
                self.player_registry
                    .insert(self.context.player.id, self.context.player.clone());
                info!("Applied quick action: remove_promises (Player ID: {})", self.context.player.id);
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
        let mut live_bound_ref_names: HashMap<(i32, i32), String> = HashMap::new();
        let mut live_clubs = Vec::new();
        let mut live_nations = Vec::new();
        let mut live_competitions = Vec::new();
        let mut panel_history_person_id: Option<i32> = None;
        let mut panel_history_club_id: Option<i32> = None;
        let mut panel_history_nation_id: Option<i32> = None;
        let mut panel_history_competition_id: Option<i32> = None;

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
                                            } else if lower.contains("nation") {
                                                detected_screen = Some("nation".to_string());
                                            } else if lower.contains("competition") || lower.contains("comp") {
                                                detected_screen = Some("competition".to_string());
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
                                                                    } else if t_typ == 10 && m_idx > 0 {
                                                                        panel_history_nation_id = Some(m_idx);
                                                                    } else if t_typ == 25 && m_idx > 0 {
                                                                        panel_history_competition_id = Some(m_idx);
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
                            let (players, names) = extract_live_bound_players(&session.driver, b_sub);
                            live_discovered_players = players;
                            live_bound_ref_names = names;
                            live_clubs = crate::live_club::read_live_clubs(&session.driver, b_sub);
                            match detected_screen.as_deref() {
                                Some("nation") => live_nations = crate::live_nation::read_live_nations(&session.driver, b_sub),
                                Some("competition") => live_competitions = crate::live_competition::read_live_competitions(&session.driver, b_sub),
                                _ => {}
                            }
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
        for ((kind, id), name) in live_bound_ref_names {
            if name.trim().is_empty() { continue; }
            match kind {
                3 => {
                    let club = self.club_registry.entry(id).or_insert_with(unknown_clubfinances);
                    club.club_id = id;
                    if club.club_name.is_empty() { club.club_name = name; }
                }
                10 => {
                    let nation = self.nation_registry.entry(id).or_insert_with(unknown_nationdetails);
                    nation.id = id;
                    if nation.name.is_empty() { nation.name = name; }
                }
                25 => {
                    let comp = self.competition_registry.entry(id).or_insert_with(unknown_competitiondetails);
                    comp.id = id;
                    if comp.name.is_empty() { comp.name = name; }
                }
                _ => {}
            }
        }
        let sole_live_club = (live_clubs.len() == 1).then(|| live_clubs[0].id);
        let sole_live_nation = (live_nations.len() == 1).then(|| live_nations[0].id);
        let sole_live_competition = (live_competitions.len() == 1).then(|| live_competitions[0].id);
        for snapshot in live_clubs {
            let club = self.club_registry.entry(snapshot.id).or_insert_with(unknown_clubfinances);
            club.club_id = snapshot.id;
            club.club_name = snapshot.name.clone();
            if !self.native_synced_clubs.contains(&snapshot.id) {
                if let Some(bal) = snapshot.balance_game_units { club.balance = bal; }
                if let Some(tb) = snapshot.transfer_budget_game_units { club.transfer_budget = tb; }
            }
            if !self.native_synced_clubs.contains(&snapshot.id) {
                if let Some(tr) = snapshot.training_rating { club.training_facilities = tr; }
                if let Some(yr) = snapshot.youth_rating { club.youth_facilities = yr; }
                if let Some(cr) = snapshot.corporate_rating { club.corporate_facilities = cr; }
            }
            if let Some(stadium) = &snapshot.stadium_name { club.stadium_name = stadium.clone(); }
            if self.context.club.club_id == snapshot.id {
                self.context.club = club.clone();
            }
            self.live_clubs.insert(snapshot.id, snapshot);
        }
        for snapshot in live_nations {
            let nation = self.nation_registry.entry(snapshot.id).or_insert_with(unknown_nationdetails);
            nation.id = snapshot.id;
            nation.name = snapshot.name.clone();
            if let Some(continent) = &snapshot.continent { nation.continent = continent.clone(); }
            if self.context.nation.id == snapshot.id {
                self.context.nation = nation.clone();
            }
            self.live_nations.insert(snapshot.id, snapshot);
        }
        for snapshot in live_competitions {
            let competition = self.competition_registry.entry(snapshot.id).or_insert_with(unknown_competitiondetails);
            competition.id = snapshot.id;
            competition.name = snapshot.name.clone();
            if let Some(nation_id) = snapshot.nation_id { competition.nation_id = nation_id; }
            if self.context.competition.id == snapshot.id {
                self.context.competition = competition.clone();
            }
            self.live_competitions.insert(snapshot.id, snapshot);
        }
        let mut first_live_id: Option<i32> = None;
        let mut active_profile_live_id: Option<i32> = None;
        for lp in live_discovered_players {
            if lp.club_id > 0 && !lp.club_name.is_empty() {
                let club = self.club_registry.entry(lp.club_id).or_insert_with(unknown_clubfinances);
                club.club_id = lp.club_id;
                if let Some(snapshot) = self.live_clubs.get(&lp.club_id) {
                    club.club_name = snapshot.name.clone();
                } else if club.club_name.is_empty() {
                    club.club_name = lp.club_name.clone();
                }
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
            if let Some(screen) = detected_screen.as_deref() {
                self.context.active_screen = screen.to_string();
                match screen {
                    "club" => {
                        if let Some(id) = sole_live_club.or(panel_history_club_id) {
                            if self.club_registry.contains_key(&id) && self.context.club.club_id != id { let _ = self.select_club(id); }
                        }
                    }
                    "nation" => {
                        if let Some(id) = sole_live_nation.or(panel_history_nation_id) {
                            if self.nation_registry.contains_key(&id) && self.context.nation.id != id { let _ = self.select_nation(id); }
                        }
                    }
                    "competition" => {
                        if let Some(id) = sole_live_competition.or(panel_history_competition_id) {
                            if self.competition_registry.contains_key(&id) && self.context.competition.id != id { let _ = self.select_competition(id); }
                        }
                    }
                    _ => {}
                }
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
            self.discover_native_player_throttled(self.context.player.id);
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
}
