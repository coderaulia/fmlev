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
