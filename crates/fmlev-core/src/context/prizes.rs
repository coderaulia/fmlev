//! Live competition prize-money access (display-verified; payout source unconfirmed).
use crate::error::{CoreError, Result};
use crate::native_competition::PrizeTable;
use fmlev_mem::LinuxMemoryDriver;
use super::GameContextManager;

/// Reads (and optionally writes) the prize table with only a driver, so a caller can run the memory scan
/// without holding the global state lock.
pub fn competition_prizes_with(
    driver: &LinuxMemoryDriver,
    teams: usize,
    first: i32,
    last: i32,
    tolerance: i32,
    new_prizes: Option<Vec<i32>>,
) -> Result<Vec<i32>> {
    let table = PrizeTable::discover(driver, teams, first, last, tolerance)?;
    if let Some(prizes) = new_prizes {
        table.write(driver, &prizes)?;
    }
    table.read(driver)
}

impl GameContextManager {
    /// Reads the prize table identified by the first/last amounts on the Rules page and, when
    /// `new_prizes` is given, writes it. Returns the amounts now in memory.
    pub fn competition_prizes(
        &mut self,
        teams: usize,
        first: i32,
        last: i32,
        tolerance: i32,
        new_prizes: Option<Vec<i32>>,
    ) -> Result<Vec<i32>> {
        let session = self.session.as_ref().filter(|s| s.bases.game_assembly_base.is_some())
            .ok_or_else(|| CoreError::InvalidValue("Attach to FM26 first".into()))?;
        competition_prizes_with(&session.driver, teams, first, last, tolerance, new_prizes)
    }
}
