//! Native nation record, containers, youth ratings and reputations.
//!
//! Layout traced in game_plugin.dll:
//! - Nation record: vtable, i32 index at +8, uid twice at +0xC/+0x10.
//!   - +0x108: Men's national team container pointer
//!   - +0x110: Women's national team container pointer
//!   - +0x1ec: state of development code (u8, 0..=255)
//!   - +0x1f0: economic factor (u8, 1..=20)
//! - National team container:
//!   - +0x170: back-pointer to nation
//!   - +0x18: first team vector ([begin, end, cap]). First pointer is team.
//!   - +0x864: youth rating (u16, 1..=200)
//! - National team:
//!   - +0x30: back-pointer to container
//!   - +0xa8: senior team reputation (u16, 1..=10000)

use fmlev_mem::{parse_process_maps, LinuxMemoryDriver};

use crate::error::{CoreError, Result};
use crate::models::NationDetails;

const CHUNK: usize = 16 << 20;
const MEN_CONTAINER_OFFSET: usize = 0x108;
const WOMEN_CONTAINER_OFFSET: usize = 0x110;
const STATE_OF_DEV_OFFSET: usize = 0x1ec;
const ECONOMIC_FACTOR_OFFSET: usize = 0x1f0;
const POPULATION_OFFSET: usize = 0x1e4;
const CONTAINER_NATION_BACKPTR: usize = 0x170;
const CONTAINER_VECTOR_OFFSET: usize = 0x18;
const YOUTH_RATING_OFFSET: usize = 0x864;
const TEAM_BACKPTR_OFFSET: usize = 0x30;
const REPUTATION_OFFSET: usize = 0xa8;

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidValue(message.to_owned())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FieldPatch {
    address: usize,
    old_bytes: Vec<u8>,
    new_bytes: Vec<u8>,
    label: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeNationRecord {
    pub pid: i32,
    pub nation: usize,
    pub men_container: Option<usize>,
    pub women_container: Option<usize>,
    pub men_team: Option<usize>,
    pub women_team: Option<usize>,
    pub nation_id: i32,
}

impl NativeNationRecord {
    /// Discovers the native nation record for `nation_id`.
    pub fn discover(driver: &LinuxMemoryDriver, nation_id: i32) -> Result<Self> {
        if nation_id <= 0 {
            return Err(invalid("A live nation id is required"));
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
                        if word(i) != nation_id || word(i + 4) != word(i + 8) || word(i + 4) == 0 {
                            continue;
                        }
                        let nation = start + i - 8;
                        let Ok(vtable) = driver.read_ptr(nation) else { continue };
                        if !readonly(vtable) {
                            continue;
                        }

                        let resolve_container = |offset: usize| -> Option<(usize, Option<usize>)> {
                            let container = driver.read_ptr(nation + offset).ok()?;
                            if container <= 0x10000 || driver.read_ptr(container + CONTAINER_NATION_BACKPTR).ok() != Some(nation) {
                                return None;
                            }
                            let team = (|| -> Option<usize> {
                                let begin = driver.read_ptr(container + CONTAINER_VECTOR_OFFSET).ok()?;
                                let end = driver.read_ptr(container + CONTAINER_VECTOR_OFFSET + 8).ok()?;
                                if begin == 0 || end <= begin || (end - begin) < 8 {
                                    return None;
                                }
                                let t = driver.read_ptr(begin).ok()?;
                                if t > 0x10000 && driver.read_ptr(t + TEAM_BACKPTR_OFFSET).ok() == Some(container) {
                                    Some(t)
                                } else {
                                    None
                                }
                            })();
                            Some((container, team))
                        };

                        let men = resolve_container(MEN_CONTAINER_OFFSET);
                        let women = resolve_container(WOMEN_CONTAINER_OFFSET);

                        let candidate = Self {
                            pid: driver.pid(),
                            nation,
                            men_container: men.map(|m| m.0),
                            women_container: women.map(|w| w.0),
                            men_team: men.and_then(|m| m.1),
                            women_team: women.and_then(|w| w.1),
                            nation_id,
                        };
                        if candidate.validate(driver).is_ok() {
                            found.push(candidate);
                        }
                    }
                }
                start += len;
            }
        }
        match found.as_slice() {
            [record] => Ok(*record),
            other => Err(invalid(&format!("Expected one native nation record for nation {nation_id}, found {}", other.len()))),
        }
    }

    pub fn validate(&self, driver: &LinuxMemoryDriver) -> Result<()> {
        if driver.pid() != self.pid
            || driver.read::<i32>(self.nation + 8)? != self.nation_id
            || driver.read::<i32>(self.nation + 0xc)? != driver.read::<i32>(self.nation + 0x10)?
        {
            return Err(invalid("Native nation record identity mismatch"));
        }
        if let Some(men) = self.men_container {
            if driver.read_ptr(self.nation + MEN_CONTAINER_OFFSET)? != men
                || driver.read_ptr(men + CONTAINER_NATION_BACKPTR)? != self.nation
            {
                return Err(invalid("Men's national team container back-pointer mismatch"));
            }
        }
        if let Some(women) = self.women_container {
            if driver.read_ptr(self.nation + WOMEN_CONTAINER_OFFSET)? != women
                || driver.read_ptr(women + CONTAINER_NATION_BACKPTR)? != self.nation
            {
                return Err(invalid("Women's national team container back-pointer mismatch"));
            }
        }
        Ok(())
    }

    // --- Reads ---

    pub fn read_men_youth_rating(&self, driver: &LinuxMemoryDriver) -> Result<u16> {
        self.validate(driver)?;
        let container = self.men_container.ok_or_else(|| invalid("Men's container not mapped"))?;
        Ok(driver.read::<u16>(container + YOUTH_RATING_OFFSET)?)
    }

    pub fn read_women_youth_rating(&self, driver: &LinuxMemoryDriver) -> Result<u16> {
        self.validate(driver)?;
        let container = self.women_container.ok_or_else(|| invalid("Women's container not mapped"))?;
        Ok(driver.read::<u16>(container + YOUTH_RATING_OFFSET)?)
    }

    pub fn read_men_reputation(&self, driver: &LinuxMemoryDriver) -> Result<u16> {
        self.validate(driver)?;
        let team = self.men_team.ok_or_else(|| invalid("Men's national team not mapped"))?;
        Ok(driver.read::<u16>(team + REPUTATION_OFFSET)?)
    }

    pub fn read_women_reputation(&self, driver: &LinuxMemoryDriver) -> Result<u16> {
        self.validate(driver)?;
        let team = self.women_team.ok_or_else(|| invalid("Women's national team not mapped"))?;
        Ok(driver.read::<u16>(team + REPUTATION_OFFSET)?)
    }

    pub fn read_economic_factor(&self, driver: &LinuxMemoryDriver) -> Result<u8> {
        self.validate(driver)?;
        Ok(driver.read::<u8>(self.nation + ECONOMIC_FACTOR_OFFSET)?)
    }

    pub fn read_state_of_development(&self, driver: &LinuxMemoryDriver) -> Result<u8> {
        self.validate(driver)?;
        Ok(driver.read::<u8>(self.nation + STATE_OF_DEV_OFFSET)?)
    }

    pub fn read_population(&self, driver: &LinuxMemoryDriver) -> Result<u32> {
        self.validate(driver)?;
        Ok(driver.read::<u32>(self.nation + POPULATION_OFFSET)?)
    }

    // --- Writes ---

    fn apply_patches(&self, driver: &LinuxMemoryDriver, patches: &[FieldPatch]) -> Result<()> {
        let mut applied: Vec<&FieldPatch> = Vec::new();
        for patch in patches {
            if patch.old_bytes == patch.new_bytes {
                continue;
            }
            if let Err(err) = driver.write_bytes(patch.address, &patch.new_bytes) {
                self.rollback_patches(driver, &applied);
                return Err(invalid(&format!("Failed writing {}: {err}", patch.label)));
            }
            let mut readback = vec![0u8; patch.new_bytes.len()];
            if driver.read_bytes(patch.address, &mut readback).is_err() || readback != patch.new_bytes {
                self.rollback_patches(driver, &applied);
                let _ = driver.write_bytes(patch.address, &patch.old_bytes);
                return Err(invalid(&format!("Readback mismatch for {}", patch.label)));
            }
            applied.push(patch);
        }
        Ok(())
    }

    fn rollback_patches(&self, driver: &LinuxMemoryDriver, applied: &[&FieldPatch]) {
        for patch in applied.iter().rev() {
            let _ = driver.write_bytes(patch.address, &patch.old_bytes);
        }
    }

    /// Transactionally updates changed nation details with readback and rollback.
    pub fn write_nation_edits(&self, driver: &LinuxMemoryDriver, previous: &NationDetails, requested: &NationDetails) -> Result<()> {
        self.validate(driver)?;
        let mut patches = Vec::new();

        // Youth rating (maps to Men's container youth rating)
        if previous.youth_rating != requested.youth_rating && requested.youth_rating > 0 {
            if !(1..=200).contains(&requested.youth_rating) {
                return Err(invalid("Youth rating must be between 1 and 200"));
            }
            let container = self.men_container.ok_or_else(|| invalid("Men's national container not mapped"))?;
            let old = driver.read::<u16>(container + YOUTH_RATING_OFFSET)?;
            let new_val = u16::from(requested.youth_rating);
            patches.push(FieldPatch {
                address: container + YOUTH_RATING_OFFSET,
                old_bytes: old.to_le_bytes().to_vec(),
                new_bytes: new_val.to_le_bytes().to_vec(),
                label: "Men's youth rating",
            });
        }

        // Reputation (maps to Men's national team reputation)
        if previous.reputation != requested.reputation && requested.reputation > 0 {
            if !(1..=10000).contains(&requested.reputation) {
                return Err(invalid("Nation reputation must be between 1 and 10000"));
            }
            let team = self.men_team.ok_or_else(|| invalid("Men's national team not mapped"))?;
            let old = driver.read::<u16>(team + REPUTATION_OFFSET)?;
            patches.push(FieldPatch {
                address: team + REPUTATION_OFFSET,
                old_bytes: old.to_le_bytes().to_vec(),
                new_bytes: requested.reputation.to_le_bytes().to_vec(),
                label: "Men's reputation",
            });
        }

        // Economic factor
        if previous.economic_factor != requested.economic_factor && requested.economic_factor > 0 {
            if !(1..=20).contains(&requested.economic_factor) {
                return Err(invalid("Economic factor must be between 1 and 20"));
            }
            let old = driver.read::<u8>(self.nation + ECONOMIC_FACTOR_OFFSET)?;
            patches.push(FieldPatch {
                address: self.nation + ECONOMIC_FACTOR_OFFSET,
                old_bytes: vec![old],
                new_bytes: vec![requested.economic_factor],
                label: "Economic factor",
            });
        }

        // State of development
        if previous.state_of_development != requested.state_of_development {
            let old = driver.read::<u8>(self.nation + STATE_OF_DEV_OFFSET)?;
            patches.push(FieldPatch {
                address: self.nation + STATE_OF_DEV_OFFSET,
                old_bytes: vec![old],
                new_bytes: vec![requested.state_of_development],
                label: "State of development",
            });
        }

        // Population
        if previous.population != requested.population && requested.population > 0 {
            if !(1..=2_000_000_000).contains(&requested.population) {
                return Err(invalid("Nation population must be between 1 and 2,000,000,000"));
            }
            let old = driver.read::<u32>(self.nation + POPULATION_OFFSET)?;
            patches.push(FieldPatch {
                address: self.nation + POPULATION_OFFSET,
                old_bytes: old.to_le_bytes().to_vec(),
                new_bytes: requested.population.to_le_bytes().to_vec(),
                label: "Population",
            });
        }

        self.apply_patches(driver, &patches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockNationFixture {
        _nation_data: Vec<u8>,
        _men_container_data: Vec<u8>,
        _women_container_data: Vec<u8>,
        _men_team_data: Vec<u8>,
        _women_team_data: Vec<u8>,
        _men_vec_data: Vec<usize>,
        _women_vec_data: Vec<usize>,
        record: NativeNationRecord,
        driver: LinuxMemoryDriver,
    }

    fn mock_nation() -> MockNationFixture {
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let mut nation_data = vec![0u8; 0x300];
        let mut men_container_data = vec![0u8; 0x1000];
        let mut women_container_data = vec![0u8; 0x1000];
        let mut men_team_data = vec![0u8; 0x200];
        let mut women_team_data = vec![0u8; 0x200];
        let mut men_vec_data = vec![0usize; 3];
        let mut women_vec_data = vec![0usize; 3];

        let nation = nation_data.as_mut_ptr() as usize;
        let men_container = men_container_data.as_mut_ptr() as usize;
        let women_container = women_container_data.as_mut_ptr() as usize;
        let men_team = men_team_data.as_mut_ptr() as usize;
        let women_team = women_team_data.as_mut_ptr() as usize;
        let men_vec = men_vec_data.as_mut_ptr() as usize;
        let women_vec = women_vec_data.as_mut_ptr() as usize;

        let nation_id = 170i32;
        let uid = 796i32;

        // Nation header
        driver.write(nation + 8, &nation_id).unwrap();
        driver.write(nation + 0xc, &uid).unwrap();
        driver.write(nation + 0x10, &uid).unwrap();
        driver.write(nation + MEN_CONTAINER_OFFSET, &men_container).unwrap();
        driver.write(nation + WOMEN_CONTAINER_OFFSET, &women_container).unwrap();
        driver.write(nation + ECONOMIC_FACTOR_OFFSET, &17u8).unwrap();
        driver.write(nation + STATE_OF_DEV_OFFSET, &1u8).unwrap();
        driver.write(nation + POPULATION_OFFSET, &49_570_725u32).unwrap();

        // Men's container
        driver.write(men_container + CONTAINER_NATION_BACKPTR, &nation).unwrap();
        driver.write(men_container + YOUTH_RATING_OFFSET, &145u16).unwrap();
        men_vec_data[0] = men_team;
        men_vec_data[1] = men_team + 8;
        men_vec_data[2] = men_team + 8;
        driver.write(men_container + CONTAINER_VECTOR_OFFSET, &men_vec).unwrap();
        driver.write(men_container + CONTAINER_VECTOR_OFFSET + 8, &(men_vec + 8)).unwrap();

        // Women's container
        driver.write(women_container + CONTAINER_NATION_BACKPTR, &nation).unwrap();
        driver.write(women_container + YOUTH_RATING_OFFSET, &160u16).unwrap();
        women_vec_data[0] = women_team;
        women_vec_data[1] = women_team + 8;
        women_vec_data[2] = women_team + 8;
        driver.write(women_container + CONTAINER_VECTOR_OFFSET, &women_vec).unwrap();
        driver.write(women_container + CONTAINER_VECTOR_OFFSET + 8, &(women_vec + 8)).unwrap();

        // Teams
        driver.write(men_team + TEAM_BACKPTR_OFFSET, &men_container).unwrap();
        driver.write(men_team + REPUTATION_OFFSET, &9011u16).unwrap();
        driver.write(women_team + TEAM_BACKPTR_OFFSET, &women_container).unwrap();
        driver.write(women_team + REPUTATION_OFFSET, &9300u16).unwrap();

        let record = NativeNationRecord {
            pid: driver.pid(),
            nation,
            men_container: Some(men_container),
            women_container: Some(women_container),
            men_team: Some(men_team),
            women_team: Some(women_team),
            nation_id,
        };

        MockNationFixture {
            _nation_data: nation_data,
            _men_container_data: men_container_data,
            _women_container_data: women_container_data,
            _men_team_data: men_team_data,
            _women_team_data: women_team_data,
            _men_vec_data: men_vec_data,
            _women_vec_data: women_vec_data,
            record,
            driver,
        }
    }

    #[test]
    fn read_nation_native_scalars() {
        let f = mock_nation();
        assert_eq!(f.record.read_men_youth_rating(&f.driver).unwrap(), 145);
        assert_eq!(f.record.read_women_youth_rating(&f.driver).unwrap(), 160);
        assert_eq!(f.record.read_men_reputation(&f.driver).unwrap(), 9011);
        assert_eq!(f.record.read_women_reputation(&f.driver).unwrap(), 9300);
        assert_eq!(f.record.read_economic_factor(&f.driver).unwrap(), 17);
        assert_eq!(f.record.read_state_of_development(&f.driver).unwrap(), 1);
        assert_eq!(f.record.read_population(&f.driver).unwrap(), 49_570_725);
    }

    #[test]
    fn write_nation_edits_round_trip() {
        let f = mock_nation();
        let mut prev = NationDetails::default();
        prev.id = f.record.nation_id;
        prev.youth_rating = 145;
        prev.reputation = 9011;
        prev.economic_factor = 17;
        prev.state_of_development = 1;
        prev.population = 49_570_725;

        let mut req = prev.clone();
        req.youth_rating = 195;
        req.reputation = 9500;
        req.economic_factor = 20;
        req.state_of_development = 2;
        req.population = 50_000_000;

        f.record.write_nation_edits(&f.driver, &prev, &req).unwrap();

        assert_eq!(f.record.read_men_youth_rating(&f.driver).unwrap(), 195);
        assert_eq!(f.record.read_men_reputation(&f.driver).unwrap(), 9500);
        assert_eq!(f.record.read_economic_factor(&f.driver).unwrap(), 20);
        assert_eq!(f.record.read_state_of_development(&f.driver).unwrap(), 2);
        assert_eq!(f.record.read_population(&f.driver).unwrap(), 50_000_000);
    }

    #[test]
    fn rejects_out_of_range_youth_rating_and_factor() {
        let f = mock_nation();
        let prev = NationDetails::default();
        let mut req = prev.clone();
        req.economic_factor = 25;
        assert!(f.record.write_nation_edits(&f.driver, &prev, &req).is_err());
        let mut req_pop = prev.clone();
        req_pop.population = 2_000_000_001;
        assert!(f.record.write_nation_edits(&f.driver, &prev, &req_pop).is_err());
    }
}
