//! Live squad-number edits (native contract + 0x5d; the game shows the change at the next day).
use crate::error::{CoreError, Result};
use super::GameContextManager;

impl GameContextManager {
    /// Sets a player's squad number (`None` clears it). A teammate wearing the number gets the
    /// player's old number; returns that teammate's index.
    pub fn update_squad_number(&mut self, id: i32, number: Option<u8>) -> Result<Option<i32>> {
        let record = self.verified_native_player(id)?;
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("No live session".into()))?;
        record.write_squad_number(&session.driver, number)
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

    /// Moves a contracted player to a target club's first team (or a specific team address).
    pub fn move_player(
        &mut self,
        id: i32,
        target_club_id: Option<i32>,
        target_team_address: Option<usize>,
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
        let res = record.move_player(&session.driver, team_addr)?;
        let _ = self.sync_live_context();
        Ok(res)
    }
}
