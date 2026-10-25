//! Competition prize-money table and native competition record.
//!
//! Prize-money: FM26 keeps the league prize list as a plain `i32` array of one
//! raw-GBP amount per finishing position. Editing it changes what the
//! competition Rules page shows (verified in game by reopening Competition > Rules).
//!
//! Native competition record:
//! - vtable, i32 index at +8, uid twice at +0xC/+0x10.
//! - +0x188: native competition reputation (u16, 1..=200 native scale).

use fmlev_mem::{parse_process_maps, LinuxMemoryDriver};

use crate::error::{CoreError, Result};
use crate::models::CompetitionDetails;

const CHUNK: usize = 16 << 20;
const MAX_PRIZE: i32 = 2_000_000_000;
const REPUTATION_OFFSET: usize = 0x188;

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidValue(message.to_owned())
}

/// True when `words` begins with a plausible prize table of `teams` entries: non-increasing
/// positive amounts starting at `first`, ending at `last`, (the word after it is not constrained: it is zero for some leagues and unrelated data for others).
fn is_prize_table(words: &[i32], teams: usize, first: i32, last: i32, tolerance: i32) -> bool {
    words.len() > teams
        && (words[0] - first).abs() <= tolerance
        && (words[teams - 1] - last).abs() <= tolerance
        && words[..teams].iter().all(|v| *v > 0)
        && words[..teams].windows(2).all(|pair| pair[0] >= pair[1])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrizeTable {
    pid: i32,
    address: usize,
    teams: usize,
}

impl PrizeTable {
    /// Finds the single table matching the first and last prize shown on the Rules page (within
    /// `tolerance` raw GBP, for rounded displays such as £53.1M).
    /// Zero or several matches are rejected; no address is compiled in.
    pub fn discover(driver: &LinuxMemoryDriver, teams: usize, first: i32, last: i32, tolerance: i32) -> Result<Self> {
        if !(2..=64).contains(&teams) || first <= 0 || last <= 0 || last > first || !(0..=1_000_000).contains(&tolerance) {
            return Err(invalid("Provide the team count and the first and last prize shown on the Rules page"));
        }
        let mut found = Vec::new();
        for map in parse_process_maps(driver.pid())?.iter().filter(|m| m.is_rw_private()) {
            let mut start = map.start;
            while start < map.end {
                let len = (map.end - start).min(CHUNK);
                let mut buf = vec![0u8; len];
                if driver.read_bytes(start, &mut buf).is_ok() {
                    let words: Vec<i32> = buf
                        .chunks_exact(4)
                        .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
                        .collect();
                    for i in 0..words.len().saturating_sub(teams) {
                        if (words[i] - first).abs() <= tolerance && is_prize_table(&words[i..], teams, first, last, tolerance) {
                            found.push(start + i * 4);
                        }
                    }
                }
                start += len;
            }
        }
        match found.as_slice() {
            [address] => Ok(Self { pid: driver.pid(), address: *address, teams }),
            other => Err(invalid(&format!(
                "Expected one prize table matching the values on the Rules page, found {}; open Competition > Rules and retry",
                other.len()
            ))),
        }
    }

    fn read_words(&self, driver: &LinuxMemoryDriver) -> Result<Vec<i32>> {
        if driver.pid() != self.pid {
            return Err(invalid("Prize table belongs to a different process"));
        }
        let mut buf = vec![0u8; (self.teams + 1) * 4];
        driver.read_bytes(self.address, &mut buf)?;
        Ok(buf.chunks_exact(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect())
    }

    pub fn read(&self, driver: &LinuxMemoryDriver) -> Result<Vec<i32>> {
        let words = self.read_words(driver)?;
        if words[..self.teams].iter().any(|v| *v <= 0) {
            return Err(invalid("Prize table changed; reopen Competition > Rules and retry"));
        }
        Ok(words[..self.teams].to_vec())
    }

    /// Writes all positions, verifies readback, and restores the old amounts on failure.
    pub fn write(&self, driver: &LinuxMemoryDriver, prizes: &[i32]) -> Result<()> {
        if prizes.len() != self.teams {
            return Err(invalid(&format!("Expected {} prize amounts", self.teams)));
        }
        if prizes.iter().any(|v| !(0..=MAX_PRIZE).contains(v)) {
            return Err(invalid("Prize amounts must be between 0 and 2,000,000,000"));
        }
        let old = self.read(driver)?;
        let old_tail = self.read_words(driver)?[self.teams];
        if old == prizes {
            return Ok(());
        }
        let put = |values: &[i32]| -> Result<()> {
            for (i, v) in values.iter().enumerate() {
                driver.write(self.address + i * 4, v)?;
            }
            Ok(())
        };
        put(prizes)?;
        let after = self.read_words(driver);
        if !after.as_ref().is_ok_and(|w| w[..self.teams] == *prizes && w[self.teams] == old_tail) {
            let rollback = put(&old);
            return Err(invalid(&format!("Prize readback failed; rollback: {rollback:?}")));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeCompetitionRecord {
    pub pid: i32,
    pub comp: usize,
    pub comp_id: i32,
}

impl NativeCompetitionRecord {
    /// Finds the native competition record for `comp_id`.
    pub fn discover(driver: &LinuxMemoryDriver, comp_id: i32) -> Result<Self> {
        if comp_id <= 0 {
            return Err(invalid("A live competition id is required"));
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
                        if word(i) != comp_id || word(i + 4) != word(i + 8) || word(i + 4) == 0 {
                            continue;
                        }
                        let comp = start + i - 8;
                        let Ok(vtable) = driver.read_ptr(comp) else { continue };
                        if readonly(vtable) {
                            found.push(Self { pid: driver.pid(), comp, comp_id });
                        }
                    }
                }
                start += len;
            }
        }
        match found.as_slice() {
            [record] => Ok(*record),
            other => Err(invalid(&format!("Expected one native competition record for competition {comp_id}, found {}", other.len()))),
        }
    }

    pub fn validate(&self, driver: &LinuxMemoryDriver) -> Result<()> {
        if driver.pid() != self.pid
            || driver.read::<i32>(self.comp + 8)? != self.comp_id
            || driver.read::<i32>(self.comp + 0xc)? != driver.read::<i32>(self.comp + 0x10)?
        {
            return Err(invalid("Native competition record identity mismatch"));
        }
        Ok(())
    }

    pub fn read_reputation(&self, driver: &LinuxMemoryDriver) -> Result<u16> {
        self.validate(driver)?;
        Ok(driver.read::<u16>(self.comp + REPUTATION_OFFSET)?)
    }

    pub fn write_reputation(&self, driver: &LinuxMemoryDriver, value: u16) -> Result<()> {
        if !(1..=200).contains(&value) {
            return Err(invalid("Competition reputation must be between 1 and 200 (native scale)"));
        }
        self.validate(driver)?;
        let old = self.read_reputation(driver)?;
        if old == value {
            return Ok(());
        }
        driver.write(self.comp + REPUTATION_OFFSET, &value)?;
        if self.read_reputation(driver).ok() != Some(value) {
            let _ = driver.write(self.comp + REPUTATION_OFFSET, &old);
            return Err(invalid("Competition reputation readback mismatch; rolled back"));
        }
        Ok(())
    }

    pub fn write_competition_edits(&self, driver: &LinuxMemoryDriver, previous: &CompetitionDetails, requested: &CompetitionDetails) -> Result<()> {
        self.validate(driver)?;
        if previous.reputation != requested.reputation && requested.reputation > 0 {
            let native_val = if requested.reputation > 200 {
                // Map from 1..10000 UI fixture scale to 1..200 native scale
                let scaled = ((f64::from(requested.reputation) / 10000.0) * 200.0).round() as u16;
                scaled.clamp(1, 200)
            } else {
                requested.reputation
            };
            self.write_reputation(driver, native_val)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_shape_requires_descending_positive_amounts() {
        let table = [165, 160, 155, 0, 9];
        assert!(is_prize_table(&table, 3, 165, 155, 0));
        assert!(is_prize_table(&table, 3, 166, 154, 1));
        assert!(!is_prize_table(&[165, 170, 155, 0], 3, 165, 155, 0));
        assert!(is_prize_table(&[165, 160, 155, 7], 3, 165, 155, 0));
        assert!(!is_prize_table(&[165, 160, 150, 0], 3, 165, 155, 0));
        assert!(!is_prize_table(&[165, 0, 155, 0], 3, 165, 155, 0));
    }

    #[test]
    fn competition_native_reputation_read_write() {
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let mut comp_data = vec![0u8; 0x300];
        let comp = comp_data.as_mut_ptr() as usize;
        let comp_id = 51i32;
        let uid = 67i32;

        driver.write(comp + 8, &comp_id).unwrap();
        driver.write(comp + 0xc, &uid).unwrap();
        driver.write(comp + 0x10, &uid).unwrap();
        driver.write(comp + REPUTATION_OFFSET, &178u16).unwrap();

        let record = NativeCompetitionRecord {
            pid: driver.pid(),
            comp,
            comp_id,
        };

        assert_eq!(record.read_reputation(&driver).unwrap(), 178);

        record.write_reputation(&driver, 195).unwrap();
        assert_eq!(record.read_reputation(&driver).unwrap(), 195);

        assert!(record.write_reputation(&driver, 250).is_err());
    }
}
