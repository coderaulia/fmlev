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

const COMP_VTABLE_RVA: usize = 0x44b_fe68;
const CHUNK: usize = 16 << 20;
const MAX_PRIZE: i32 = 2_000_000_000;
const REPUTATION_OFFSET: usize = 0x188;

const STAGE_SQUAD_CAP_PRIMARY: usize = 0x98;
const STAGE_HOMEGROWN_OFFSET: usize = 0x99;
const STAGE_REGISTRATION_GATE_OFFSET: usize = 0x9a;
const STAGE_SQUAD_NUM_CAP_OFFSET: usize = 0x9b;
const STAGE_SQUAD_CAP_SECONDARY: usize = 0xd6;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct NativeStageSquadRules {
    pub max_squad_size: u8,
    pub min_homegrown: u8,
    pub registration_gate: u8,
    pub max_squad_number: u8,
    pub rule_entry_address: usize,
}

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
        // `.?AVCOMP@db@@` vtable (game_plugin.dll RVA, traced on PE timestamp 0x6a229e8a). Other record
        // types share the index/uid pattern, so the vtable pins the type. Test processes have no module.
        let comp_vtable = maps
            .iter()
            .filter(|m| m.pathname.ends_with("game_plugin.dll"))
            .map(|m| m.start)
            .min()
            .map(|base| base + COMP_VTABLE_RVA);
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
                        // Other record types share the index/uid pattern. A real competition record has name
                        // pointers (+0x40/+0x48/+0x50), the ACTUAL_COMP pointer (+0xb0) and a 1..=200 reputation.
                        let structured = [0x40usize, 0x48, 0x50, 0xb0]
                            .iter()
                            .all(|o| driver.read_ptr(comp + o).is_ok_and(|p| p > 0x10000))
                            && driver.read::<u16>(comp + REPUTATION_OFFSET).is_ok_and(|r| (1..=200).contains(&r));
                        if readonly(vtable) && structured && comp_vtable.is_none_or(|expected| vtable == expected) {
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

    /// Locates the active competition stage rule entry in the simulation engine.
    pub fn stage_rule_entry(&self, driver: &LinuxMemoryDriver) -> Result<usize> {
        self.validate(driver)?;
        let act = driver.read_ptr(self.comp + 0xb0)?;
        if act <= 0x10000 {
            return Err(invalid("Competition ACTUAL_COMP is missing or null"));
        }
        let stage = driver.read_ptr(act + 0x208)?;
        let vec_obj = driver.read_ptr(stage + 0x1e0)?;
        let (begin, end) = (driver.read_ptr(vec_obj + 0x60)?, driver.read_ptr(vec_obj + 0x68)?);
        let stage_index = driver.read::<u8>(act + 0x31)?;
        if stage <= 0x10000 || vec_obj <= 0x10000 || begin == 0 || begin >= end
            || (end - begin) % 8 != 0 || stage_index == 0xff || usize::from(stage_index) >= (end - begin) / 8
        {
            return Err(invalid("Competition stage rules failed validation"));
        }
        let entry = driver.read_ptr(begin + usize::from(stage_index) * 8)?;
        if entry <= 0x10000 {
            return Err(invalid("Competition stage rule entry is missing"));
        }
        Ok(entry)
    }

    /// Reads live squad registration and number rules from the active simulation stage rule entry.
    pub fn read_stage_squad_rules(&self, driver: &LinuxMemoryDriver) -> Result<NativeStageSquadRules> {
        let entry = self.stage_rule_entry(driver)?;
        let max_squad_size = driver.read::<u8>(entry + STAGE_SQUAD_CAP_PRIMARY)?;
        let min_homegrown = driver.read::<u8>(entry + STAGE_HOMEGROWN_OFFSET)?;
        let registration_gate = driver.read::<u8>(entry + STAGE_REGISTRATION_GATE_OFFSET)?;
        let max_squad_number = driver.read::<u8>(entry + STAGE_SQUAD_NUM_CAP_OFFSET)?;
        Ok(NativeStageSquadRules {
            max_squad_size,
            min_homegrown,
            registration_gate,
            max_squad_number,
            rule_entry_address: entry,
        })
    }

    /// Atomically updates the stage registration squad size limit (both primary +0x98 and secondary +0xd6)
    /// with verification and automatic rollback on mismatch.
    pub fn write_stage_squad_cap(&self, driver: &LinuxMemoryDriver, new_cap: u8) -> Result<()> {
        if !(1..=99).contains(&new_cap) {
            return Err(invalid("Squad size limit must be between 1 and 99"));
        }
        let entry = self.stage_rule_entry(driver)?;
        let old_primary = driver.read::<u8>(entry + STAGE_SQUAD_CAP_PRIMARY)?;
        let old_secondary = driver.read::<u8>(entry + STAGE_SQUAD_CAP_SECONDARY)?;
        if old_primary == new_cap && old_secondary == new_cap {
            return Ok(());
        }
        driver.write(entry + STAGE_SQUAD_CAP_PRIMARY, &new_cap)?;
        if let Err(e) = driver.write(entry + STAGE_SQUAD_CAP_SECONDARY, &new_cap) {
            let _ = driver.write(entry + STAGE_SQUAD_CAP_PRIMARY, &old_primary);
            return Err(e.into());
        }
        let verified = driver.read::<u8>(entry + STAGE_SQUAD_CAP_PRIMARY)? == new_cap
            && driver.read::<u8>(entry + STAGE_SQUAD_CAP_SECONDARY)? == new_cap;
        if !verified {
            let _ = driver.write(entry + STAGE_SQUAD_CAP_PRIMARY, &old_primary);
            let _ = driver.write(entry + STAGE_SQUAD_CAP_SECONDARY, &old_secondary);
            return Err(invalid("Stage squad cap readback mismatch; rolled back"));
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

    #[test]
    fn competition_stage_squad_rules_read_write() {
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let mut comp_data = vec![0u8; 0x300];
        let mut act_data = vec![0u8; 0x300];
        let mut stage_data = vec![0u8; 0x300];
        let mut vec_obj_data = vec![0u8; 0x100];
        let mut entry_data = vec![0u8; 0x100];

        let comp = comp_data.as_mut_ptr() as usize;
        let act = act_data.as_mut_ptr() as usize;
        let stage = stage_data.as_mut_ptr() as usize;
        let vec_obj = vec_obj_data.as_mut_ptr() as usize;
        let entry = entry_data.as_mut_ptr() as usize;

        let comp_id = 51i32;
        let uid = 67i32;

        driver.write(comp + 8, &comp_id).unwrap();
        driver.write(comp + 0xc, &uid).unwrap();
        driver.write(comp + 0x10, &uid).unwrap();
        driver.write(comp + 0xb0, &act).unwrap();

        driver.write(act + 0x31, &0u8).unwrap();
        driver.write(act + 0x208, &stage).unwrap();

        driver.write(stage + 0x1e0, &vec_obj).unwrap();

        let mut ptr_vec = vec![entry];
        let begin = ptr_vec.as_mut_ptr() as usize;
        let end = begin + 8;
        driver.write(vec_obj + 0x60, &begin).unwrap();
        driver.write(vec_obj + 0x68, &end).unwrap();

        driver.write(entry + STAGE_SQUAD_CAP_PRIMARY, &25u8).unwrap();
        driver.write(entry + STAGE_HOMEGROWN_OFFSET, &8u8).unwrap();
        driver.write(entry + STAGE_REGISTRATION_GATE_OFFSET, &0u8).unwrap();
        driver.write(entry + STAGE_SQUAD_NUM_CAP_OFFSET, &255u8).unwrap();
        driver.write(entry + STAGE_SQUAD_CAP_SECONDARY, &25u8).unwrap();

        let record = NativeCompetitionRecord {
            pid: driver.pid(),
            comp,
            comp_id,
        };

        let rules = record.read_stage_squad_rules(&driver).unwrap();
        assert_eq!(rules.max_squad_size, 25);
        assert_eq!(rules.min_homegrown, 8);
        assert_eq!(rules.registration_gate, 0);
        assert_eq!(rules.max_squad_number, 255);
        assert_eq!(rules.rule_entry_address, entry);

        record.write_stage_squad_cap(&driver, 30).unwrap();
        let updated = record.read_stage_squad_rules(&driver).unwrap();
        assert_eq!(updated.max_squad_size, 30);
        assert_eq!(driver.read::<u8>(entry + STAGE_SQUAD_CAP_SECONDARY).unwrap(), 30);

        // Clamping / bounds check:
        assert!(record.write_stage_squad_cap(&driver, 0).is_err());
        assert!(record.write_stage_squad_cap(&driver, 100).is_err());
    }
}
