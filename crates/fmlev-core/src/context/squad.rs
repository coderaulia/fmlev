//! Live squad-number edits (native contract + 0x5d; the game shows the change at the next day).
use crate::error::{CoreError, Result};
use crate::native_rtti::ModuleImage;
use crate::native_transfer::ACTUAL_PLAYER_OFFSET;
use super::GameContextManager;

impl GameContextManager {
    /// Sets a player's squad number (`None` clears it). A teammate wearing the number gets the
    /// player's old number; returns that teammate's index.
    pub fn update_squad_number(&mut self, id: i32, number: Option<u8>) -> Result<Option<i32>> {
        let record = self.verified_native_player(id)?;
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;
        record.write_squad_number(&session.driver, number)
    }
    /// Reads the selected native morale byte and optionally writes a change against a fresh baseline.
    pub fn player_morale(
        &mut self, id: i32, edit: Option<(u32, u8, u8)>,
    ) -> Result<(u32, u8)> {
        let driver = self.session.as_ref().filter(|s| s.bases.game_assembly_base.is_some())
            .map(|s| s.driver).ok_or_else(|| CoreError::InvalidValue("Attach to FM26 first".into()))?;
        let image = ModuleImage::discover(&driver)?.ok_or_else(|| CoreError::InvalidValue("game_plugin.dll is not mapped".into()))?;
        let record = self.verified_native_player(id)?;
        let actual = record.address().checked_sub(ACTUAL_PLAYER_OFFSET).ok_or_else(|| CoreError::InvalidValue("Invalid player record".into()))?;
        if !image.is_type(&driver, actual, ".?AVACTUAL_PLAYER@db@@") {
            return Err(CoreError::InvalidValue("Morale is unavailable for this native player type".into()));
        }
        let value = if let Some((uid, expected, value)) = edit {
            if uid != record.uid {
                return Err(CoreError::InvalidValue("Player identity changed. Read morale again".into()));
            }
            record.write_morale_from(&driver, value, Some(expected))?;
            for player in self.player_registry.get_mut(&id).into_iter()
                .chain((self.context.player.id == id).then_some(&mut self.context.player))
            {
                player.morale = value * 5;
                player.morale_label = format!("{value}/20");
            }
            value
        } else {
            record.read_morale(&driver)?
        };
        Ok((record.uid, value))
    }

}

impl GameContextManager {
    /// Maximises tactical familiarity for one player, or for his whole squad container.
    /// Returns (players processed, values changed).
    pub fn maximise_familiarity(&mut self, id: i32, squad: bool) -> Result<(usize, usize)> {
        let record = self.verified_native_player(id)?;
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;
        let players = if squad { record.teammates(&session.driver)? } else { vec![record] };
        let (mut changed, mut count) = (0, 0);
        for player in &players {
            changed += player.maximise_tactical_familiarity(&session.driver)?;
            count += 1;
        }
        Ok((count, changed))
    }
}

impl GameContextManager {
    /// Extends a player's contract by `years` (native expiry at contract + 0x48).
    /// Returns (old expiry year, new expiry year).
    pub fn extend_contract(&mut self, id: i32, years: i32) -> Result<(i32, i32)> {
        let record = self.verified_native_player(id)?;
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;
        record.extend_contract(&session.driver, years)
    }

    /// Releases a player as a free agent by detaching their contract and removing them from squad vectors.
    pub fn terminate_contract(&mut self, id: i32) -> Result<Vec<usize>> {
        let record = self.verified_native_player(id)?;
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;
        let res = record.terminate_contract(&session.driver)?;
        let _ = self.sync_live_context();
        Ok(res)
    }

    /// Terminates an active loan, returning the player to their parent club.
    pub fn terminate_loan(&mut self, id: i32) -> Result<(usize, usize)> {
        let record = self.verified_native_player(id)?;
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;
        let res = record.terminate_loan(&session.driver)?;
        let _ = self.sync_live_context();
        Ok(res)
    }

    /// Frees a slot in the destination squad by releasing `release_id` (a free agent afterwards) when it is full.
    /// Does nothing when a slot is already spare; refuses a player who is not in that squad.
    fn release_for_space(&mut self, team_addr: usize, release_id: Option<i32>) -> Result<()> {
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;
        let squad = crate::native_transfer::SquadVector::read(&session.driver, team_addr)?;
        if squad.spare_slots() > 0 {
            return Ok(());
        }
        let Some(release_id) = release_id else {
            return Err(CoreError::InvalidValue(
                "The destination squad list is full (the game allocates it at exactly its current size). Choose a player of that club to release as a free agent to make room".into(),
            ));
        };
        let victim = self.verified_native_player(release_id)?;
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;
        let actual = victim.address() - crate::native_transfer::ACTUAL_PLAYER_OFFSET;
        if !squad.contains(&session.driver, actual)? {
            return Err(CoreError::InvalidValue("The player to release is not in the destination first-team squad".into()));
        }
        victim.terminate_contract(&session.driver)?;
        let squad = crate::native_transfer::SquadVector::read(&session.driver, team_addr)?;
        if squad.spare_slots() == 0 {
            return Err(CoreError::InvalidValue("Releasing the player did not free a slot in the destination squad".into()));
        }
        Ok(())
    }

    /// Moves a contracted player to a target club's first team (or a specific team address).
    pub fn move_player(
        &mut self,
        id: i32,
        target_club_id: Option<i32>,
        target_team_address: Option<usize>,
        fee_gbp: Option<u32>,
        terms: Option<crate::native_transfer::NewContractTerms>,
        release_id: Option<i32>,
    ) -> Result<(usize, usize)> {
        let record = self.verified_native_player(id)?;
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;
        let team_addr = match (target_team_address, target_club_id) {
            (Some(addr), _) => addr,
            (None, Some(club_id)) => {
                let club = crate::native_club::NativeClubRecord::discover(&session.driver, club_id)?;
                club.first_team.ok_or_else(|| CoreError::InvalidValue("Target club has no valid first team".into()))?
            }
            (None, None) => return Err(CoreError::InvalidValue("Either target_club_id or target_team_address must be provided".into())),
        };
        self.release_for_space(team_addr, release_id)?;
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;
        let res = record.move_player(&session.driver, team_addr, fee_gbp, terms)?;
        let _ = self.sync_live_context();
        Ok(res)
    }

    /// Signs an unemployed free agent to a target club's first team (or specific team address).
    pub fn sign_free_agent(
        &mut self,
        id: i32,
        target_club_id: Option<i32>,
        target_team_address: Option<usize>,
        wage_gbp: Option<u32>,
        expiry_year: Option<u32>,
        release_id: Option<i32>,
    ) -> Result<usize> {
        let record = self.verified_native_player(id)?;
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;
        let team_addr = match (target_team_address, target_club_id) {
            (Some(addr), _) => addr,
            (None, Some(club_id)) => {
                let club = crate::native_club::NativeClubRecord::discover(&session.driver, club_id)?;
                club.first_team.ok_or_else(|| CoreError::InvalidValue("Target club has no valid first team".into()))?
            }
            (None, None) => return Err(CoreError::InvalidValue("Either target_club_id or target_team_address must be provided".into())),
        };
        self.release_for_space(team_addr, release_id)?;
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;
        let res = record.sign_free_agent(&session.driver, team_addr, wage_gbp, expiry_year)?;
        let _ = self.sync_live_context();
        Ok(res)
    }

    /// Completes an arranged future transfer immediately.
    pub fn complete_pending_transfer(&mut self, id: i32) -> Result<(usize, usize)> {
        let record = self.verified_native_player(id)?;
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;
        let res = record.complete_pending_transfer(&session.driver)?;
        let _ = self.sync_live_context();
        Ok(res)
    }

    /// Accelerates the scheduled join date of a pending future transfer.
    pub fn accelerate_pending_transfer(
        &mut self,
        id: i32,
        new_join_date: Option<u32>,
    ) -> Result<(u32, u32)> {
        let record = self.verified_native_player(id)?;
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;
        let res = record.accelerate_pending_transfer(&session.driver, new_join_date)?;
        let _ = self.sync_live_context();
        Ok(res)
    }
}
