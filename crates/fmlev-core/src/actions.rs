use fmlev_mem::LinuxMemoryDriver;
use crate::error::Result;
use tracing::info;

pub struct SquadActions;

impl SquadActions {
    /// Instant Squad Heal: Restores condition to 10,000 (100%), sharpness to 10,000, clears fatigue and injuries.
    pub fn heal_player(driver: &LinuxMemoryDriver, player_ptr: usize, condition_offset: usize) -> Result<()> {
        let max_condition: u16 = 10000;
        let max_sharpness: u16 = 10000;

        // Write condition
        driver.write(player_ptr + condition_offset, &max_condition)?;
        // Write sharpness (usually contiguous or offset + 2)
        driver.write(player_ptr + condition_offset + 2, &max_sharpness)?;

        info!("Healed player at 0x{:x}", player_ptr);
        Ok(())
    }

    /// Instant Squad Morale Boost: Restores morale to maximum.
    pub fn boost_morale(driver: &LinuxMemoryDriver, player_ptr: usize, morale_offset: usize) -> Result<()> {
        let max_morale: u8 = 100;
        driver.write(player_ptr + morale_offset, &max_morale)?;
        info!("Boosted morale for player at 0x{:x}", player_ptr);
        Ok(())
    }

    /// Adjust Club Financial Budgets
    #[allow(clippy::too_many_arguments)]
    pub fn update_finances(
        driver: &LinuxMemoryDriver,
        club_ptr: usize,
        balance_offset: usize,
        transfer_offset: usize,
        wage_offset: usize,
        balance: i64,
        transfer_budget: i64,
        wage_budget: i32,
    ) -> Result<()> {
        driver.write(club_ptr + balance_offset, &balance)?;
        driver.write(club_ptr + transfer_offset, &transfer_budget)?;
        driver.write(club_ptr + wage_offset, &wage_budget)?;
        info!("Updated finances for club at 0x{:x}", club_ptr);
        Ok(())
    }
}
