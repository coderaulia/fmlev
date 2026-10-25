//! Competition prize-money table. FM26 keeps the league prize list as a plain `i32` array of one
//! raw-GBP amount per finishing position, followed by a zero word. Editing it changes what the
//! competition Rules page shows (verified in game by reopening Competition > Rules).
//!
//! CAVEAT: the array lies among UI model entries. It is not proven that season-end payouts read
//! the same table, so treat edits as display-verified only until a season rolls over.

use fmlev_mem::{parse_process_maps, LinuxMemoryDriver};

use crate::error::{CoreError, Result};

const CHUNK: usize = 16 << 20;
const MAX_PRIZE: i32 = 2_000_000_000;

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidValue(message.to_owned())
}

/// True when `words` begins with a plausible prize table of `teams` entries: non-increasing
/// positive amounts starting at `first`, ending at `last`, followed by a zero terminator.
fn is_prize_table(words: &[i32], teams: usize, first: i32, last: i32) -> bool {
    words.len() > teams
        && words[0] == first
        && words[teams - 1] == last
        && words[teams] == 0
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
    /// Finds the single table matching the first and last prize shown on the Rules page.
    /// Zero or several matches are rejected; no address is compiled in.
    pub fn discover(driver: &LinuxMemoryDriver, teams: usize, first: i32, last: i32) -> Result<Self> {
        if !(2..=64).contains(&teams) || first <= 0 || last <= 0 || last > first {
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
                        if words[i] == first && is_prize_table(&words[i..], teams, first, last) {
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
        if words[self.teams] != 0 || words[..self.teams].iter().any(|v| *v <= 0) {
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
        if !after.as_ref().is_ok_and(|w| w[..self.teams] == *prizes && w[self.teams] == 0) {
            let rollback = put(&old);
            return Err(invalid(&format!("Prize readback failed; rollback: {rollback:?}")));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_shape_requires_descending_amounts_and_terminator() {
        let table = [165, 160, 155, 0, 9];
        assert!(is_prize_table(&table, 3, 165, 155));
        assert!(!is_prize_table(&[165, 170, 155, 0], 3, 165, 155));
        assert!(!is_prize_table(&[165, 160, 155, 7], 3, 165, 155));
        assert!(!is_prize_table(&[165, 160, 150, 0], 3, 165, 155));
        assert!(!is_prize_table(&[165, 0, 155, 0], 3, 165, 155));
    }
}
