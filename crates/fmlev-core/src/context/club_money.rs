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

    /// Reads the verified native values (money, facilities, reputation, stadium) of a loaded club and
    /// makes them the registry copy, so the UI loads real values instead of binding caches.
    pub fn read_club_native(&mut self, club_id: i32) -> Result<ClubFinances> {
        let session = self.session.as_ref().ok_or_else(|| CoreError::InvalidValue("Attach to FM26 first".into()))?;
        let mut club = self.club_registry.get(&club_id).cloned()
            .or_else(|| (self.context.club.club_id == club_id).then(|| self.context.club.clone()))
            .unwrap_or_else(|| {
                let mut c = super::inspector::unknown_clubfinances();
                c.club_id = club_id;
                c
            });
        let driver = &session.driver;
        let record = crate::native_club::NativeClubRecord::discover(driver, club_id)?;
        if club.club_name.is_empty() || club.club_name == "Unknown (live)" || club.club_name == "Indonesia" || club.club_name.starts_with("Club #") {
            if let Some(name) = record.read_name(driver) {
                club.club_name = name;
            }
        }
        club.balance = i64::from(record.read_balance(driver)?);
        club.transfer_budget = i64::from(record.read_transfer_budget(driver)?);
        let (training, youth, coaching, recruitment, corp, analysis) = record.read_facilities(driver)?;
        for (field, value) in [
            (&mut club.training_facilities, training),
            (&mut club.youth_facilities, youth),
            (&mut club.junior_coaching, coaching),
            (&mut club.youth_recruitment, recruitment),
            (&mut club.corporate_facilities, corp),
            (&mut club.data_analysis_facilities, analysis),
        ] {
            if value > 0 { *field = value; }
        }
        if let Ok((rep, _)) = record.read_reputations(driver) {
            if rep > 0 { club.reputation = rep; }
        }
        if let Ok((cap, exp)) = record.read_stadium_capacities(driver) {
            if cap > 0 { club.stadium_capacity = cap; }
            if exp > 0 { club.stadium_expansion_cap = exp; }
        }
        if let Ok(sd) = record.read_sugar_daddy(driver) {
            club.sugar_daddy_type = match sd {
                1 => "Foreground".to_string(),
                2 => "Background".to_string(),
                3 => "Underwriter".to_string(),
                _ => "None".to_string(),
            };
        }
        self.club_registry.insert(club_id, club.clone());
        self.native_synced_clubs.insert(club_id);
        if self.context.club.club_id == club_id {
            self.context.club = club.clone();
        }
        Ok(club)
    }
}
