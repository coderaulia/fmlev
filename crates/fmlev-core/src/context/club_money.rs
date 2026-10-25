//! Live club money edits (native balance and transfer budget).
use crate::error::{CoreError, Result};
use crate::models::ClubFinances;
use super::GameContextManager;

impl GameContextManager {
    /// Changes only the balance and/or transfer budget of a live club (native values).
    pub fn update_club_money(&mut self, club_id: i32, balance: Option<i64>, transfer_budget: Option<i64>) -> Result<ClubFinances> {
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("Attach to FM26 first".into()))?;
        let mut club = self.club_registry.get(&club_id).cloned()
            .or_else(|| (self.context.club.club_id == club_id).then(|| self.context.club.clone()))
            .ok_or_else(|| CoreError::InvalidValue(format!("Club {club_id} is not loaded; open one of its pages in FM26")))?;
        // Start from native values so a stale registry copy cannot write back old money.
        let record = crate::native_club::NativeClubFinance::discover(&session.driver, club_id)?;
        club.balance = i64::from(record.read_balance(&session.driver)?);
        club.transfer_budget = i64::from(record.read_transfer_budget(&session.driver)?);
        self.club_registry.insert(club_id, club.clone());
        if let Some(value) = balance { club.balance = value; }
        if let Some(value) = transfer_budget { club.transfer_budget = value; }
        self.update_club(club.clone())?;
        Ok(club)
    }
}
