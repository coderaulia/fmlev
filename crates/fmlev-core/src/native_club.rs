//! Native club money. The native club record (vtable, i32 index at + 8, uid twice at + 0xC/+ 0x10)
//! points at + 0x150 to a finance object that points back to the record at + 8. In it:
//! - + 0x14: live balance (i32 raw GBP). Verified in game: shows on Club > Finances after reopening
//!   and survives day ticks. Month-end rollover persistence is unconfirmed.
//! - + 0x7cc: available transfer budget (i32 raw GBP), followed by the allocated budget at + 0x7d0.
//!   Verified in game for Persib and Real Madrid (shows after reopening Club > Finances).
//!
//! Other copies of these values (UI bindings, a heap `[allocated, available, .., balance]` block and
//! a summary table) are caches the game rewrites; never write those.

use fmlev_mem::{parse_process_maps, LinuxMemoryDriver};

use crate::error::{CoreError, Result};

const CHUNK: usize = 16 << 20;
const FINANCE: usize = 0x150;
const BALANCE: usize = 0x14;
const TRANSFER: usize = 0x7cc;

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidValue(message.to_owned())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeClubFinance {
    pid: i32,
    club: usize,
    finance: usize,
    club_id: i32,
}

impl NativeClubFinance {
    /// Finds the native club record for `club_id` whose finance object points back to it.
    /// Zero or several matches are rejected; no address is compiled in.
    pub fn discover(driver: &LinuxMemoryDriver, club_id: i32) -> Result<Self> {
        if club_id <= 0 {
            return Err(invalid("A live club id is required"));
        }
        let maps = parse_process_maps(driver.pid())?;
        let readonly = |p: usize| maps.iter().any(|m| m.is_readable() && !m.is_writable() && p >= m.start && p < m.end);
        let mut found = Vec::new();
        for map in maps.iter().filter(|m| m.is_rw_private()) {
            let mut start = map.start;
            while start < map.end {
                let len = (map.end - start).min(CHUNK);
                let mut buf = vec![0u8; len];
                if driver.read_bytes(start, &mut buf).is_ok() {
                    for i in (8..len.saturating_sub(12)).step_by(8) {
                        let word = |at: usize| i32::from_le_bytes(buf[at..at + 4].try_into().unwrap());
                        if word(i) != club_id || word(i + 4) != word(i + 8) || word(i + 4) == 0 {
                            continue;
                        }
                        let club = start + i - 8;
                        let Ok(finance) = driver.read_ptr(club + FINANCE) else { continue };
                        let candidate = Self { pid: driver.pid(), club, finance, club_id };
                        if driver.read_ptr(club).is_ok_and(&readonly) && candidate.validate(driver).is_ok() {
                            found.push(candidate);
                        }
                    }
                }
                start += len;
            }
        }
        match found.as_slice() {
            [record] => Ok(*record),
            other => Err(invalid(&format!("Expected one native club record for club {club_id}, found {}", other.len()))),
        }
    }

    fn validate(&self, driver: &LinuxMemoryDriver) -> Result<()> {
        if driver.pid() != self.pid
            || driver.read::<i32>(self.club + 8)? != self.club_id
            || driver.read::<i32>(self.club + 0xc)? != driver.read::<i32>(self.club + 0x10)?
            || driver.read_ptr(self.club + FINANCE)? != self.finance
            || driver.read_ptr(self.finance + 8)? != self.club
        {
            return Err(invalid("Native club finance object changed; reopen Club > Finances and retry"));
        }
        Ok(())
    }

    fn read_field(&self, driver: &LinuxMemoryDriver, offset: usize) -> Result<i32> {
        self.validate(driver)?;
        Ok(driver.read::<i32>(self.finance + offset)?)
    }

    /// Writes one i32 field, verifies readback and restores the old value on failure.
    fn write_field(&self, driver: &LinuxMemoryDriver, offset: usize, value: i64, label: &str) -> Result<()> {
        let value = i32::try_from(value).map_err(|_| invalid(&format!("{label} is outside the 32-bit native range")))?;
        let old = self.read_field(driver, offset)?;
        if old == value {
            return Ok(());
        }
        driver.write(self.finance + offset, &value)?;
        if self.read_field(driver, offset).ok() != Some(value) {
            let rollback = driver.write(self.finance + offset, &old);
            return Err(invalid(&format!("{label} readback failed; rollback: {rollback:?}")));
        }
        Ok(())
    }

    pub fn read_balance(&self, driver: &LinuxMemoryDriver) -> Result<i32> {
        self.read_field(driver, BALANCE)
    }

    pub fn read_transfer_budget(&self, driver: &LinuxMemoryDriver) -> Result<i32> {
        self.read_field(driver, TRANSFER)
    }

    pub fn write_balance(&self, driver: &LinuxMemoryDriver, value: i64) -> Result<()> {
        self.write_field(driver, BALANCE, value, "Balance")
    }

    pub fn write_transfer_budget(&self, driver: &LinuxMemoryDriver, value: i64) -> Result<()> {
        if value < 0 {
            return Err(invalid("Transfer budget must not be negative"));
        }
        self.write_field(driver, TRANSFER, value, "Transfer budget")
    }
}
