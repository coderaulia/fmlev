//! Narrow native position/footedness layout, validated against live bindings before discovery.
//! No managed reference is ever accepted as a native record.
use crate::{
    error::{CoreError, Result},
    models::PlayerPositions,
};
use fmlev_mem::{parse_process_maps, LinuxMemoryDriver};

const POSITIONS: usize = 8;
const LEFT_FOOT: usize = 0x2f;
const RIGHT_FOOT: usize = 0x30;
const PERSON: usize = 0x140;
const INDEX: usize = PERSON + 8;
const UID: usize = PERSON + 12;

#[derive(Debug, Clone)]
pub struct NativePlayerRecord {
    address: usize,
    pid: i32,
    index: i32,
    uid: u32,
    player_vtable: usize,
    person_vtable: usize,
}

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidValue(message.to_string())
}

impl NativePlayerRecord {
    /// Find exactly one record whose identity, at least 14 of 15 aptitudes and both feet
    /// match independently decoded UI bindings. No PID-specific addresses.
    pub fn discover(
        driver: &LinuxMemoryDriver,
        index: i32,
        uid: u32,
        aptitudes: [u8; 15],
        left: u8,
        right: u8,
    ) -> Result<Self> {
        if index <= 0
            || uid == 0
            || !aptitudes.iter().all(|v| (1..=20).contains(v))
            || !(1..=20).contains(&left)
            || !(1..=20).contains(&right)
        {
            return Err(invalid("Complete live position and foot bindings are required to validate the native record"));
        }
        let identity: Vec<u8> = index
            .to_le_bytes()
            .into_iter()
            .chain(uid.to_le_bytes())
            .chain(uid.to_le_bytes())
            .collect();
        let pattern = identity
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ");
        let maps = parse_process_maps(driver.pid())?;
        let readonly = |p: usize| {
            maps.iter().any(|m| {
                m.is_readable()
                    && !m.is_writable()
                    && p >= m.start
                    && p.checked_add(8).is_some_and(|e| e <= m.end)
            })
        };
        let executable = |p: usize| {
            maps.iter()
                .any(|m| m.is_executable() && p >= m.start && p < m.end)
        };
        let mut records = Vec::new();
        for hit in driver.scan_aob(&pattern)? {
            let Some(address) = hit.checked_sub(INDEX) else {
                continue;
            };
            if address % 8 != 0
                || !maps.iter().any(|m| {
                    m.is_rw_private()
                        && address >= m.start
                        && address.checked_add(UID + 8).is_some_and(|e| e <= m.end)
                })
            {
                continue;
            }
            let Ok(player_vtable) = driver.read_ptr(address) else {
                continue;
            };
            let Ok(person_vtable) = driver.read_ptr(address + PERSON) else {
                continue;
            };
            if !readonly(player_vtable) || !readonly(person_vtable) {
                continue;
            }
            if !driver.read_ptr(player_vtable).is_ok_and(&executable)
                || !driver.read_ptr(person_vtable).is_ok_and(&executable)
            {
                continue;
            }
            let record = Self {
                address,
                pid: driver.pid(),
                index,
                uid,
                player_vtable,
                person_vtable,
            };
            if record.validate(driver).is_err() {
                continue;
            }
            let Ok(native_aptitudes) = driver.read::<[u8; 15]>(address + POSITIONS) else {
                continue;
            };
            // The old UI-only writer can leave a changed pointer in one cached row
            // (observed: Kurzawa WBL UI 20 / native 18). Identity plus 14 independent
            // aptitude matches and both scaled feet still uniquely validates layout.
            if !native_aptitudes.iter().all(|v| (1..=20).contains(v))
                || native_aptitudes
                    .iter()
                    .zip(aptitudes)
                    .filter(|(a, b)| **a == *b)
                    .count()
                    < 14
                || driver.read::<u8>(address + LEFT_FOOT).ok() != Some(left * 5)
                || driver.read::<u8>(address + RIGHT_FOOT).ok() != Some(right * 5)
            {
                continue;
            }
            records.push(record);
        }
        if records.len() != 1 {
            return Err(invalid(&format!(
                "Expected one identity and binding validated native player record, found {}",
                records.len()
            )));
        }
        Ok(records.remove(0))
    }

    fn validate(&self, driver: &LinuxMemoryDriver) -> Result<()> {
        if driver.pid() != self.pid {
            return Err(invalid(
                "Native player handle belongs to a different process",
            ));
        }
        if driver.read_ptr(self.address)? != self.player_vtable
            || driver.read_ptr(self.address + PERSON)? != self.person_vtable
            || driver.read::<i32>(self.address + INDEX)? != self.index
            || driver.read::<u32>(self.address + UID)? != self.uid
            || driver.read::<u32>(self.address + UID + 4)? != self.uid
        {
            return Err(invalid(
                "Native player identity changed; rediscover the player before editing",
            ));
        }
        Ok(())
    }

    pub fn read_positions(&self, driver: &LinuxMemoryDriver) -> Result<PlayerPositions> {
        self.validate(driver)?;
        let p = driver.read::<[u8; 15]>(self.address + POSITIONS)?;
        let left = driver.read::<u8>(self.address + LEFT_FOOT)?;
        let right = driver.read::<u8>(self.address + RIGHT_FOOT)?;
        if !p.iter().all(|v| (1..=20).contains(v))
            || left > 100
            || right > 100
            || left < 5
            || right < 5
            || left % 5 != 0
            || right % 5 != 0
        {
            return Err(invalid(
                "Native position or foot values no longer match the verified layout",
            ));
        }
        Ok(PlayerPositions {
            gk: p[0],
            dl: p[2],
            dc: p[3],
            dr: p[4],
            dm: p[5],
            ml: p[6],
            mc: p[7],
            mr: p[8],
            aml: p[9],
            amc: p[10],
            amr: p[11],
            st: p[12],
            wbl: p[13],
            wbr: p[14],
            left_foot: left / 5,
            right_foot: right / 5,
        })
    }

    fn rollback_positions(
        &self,
        driver: &LinuxMemoryDriver,
        old: &[u8; 15],
        left: u8,
        right: u8,
    ) -> Result<()> {
        // A reused record belongs to a different player; never roll old bytes into it.
        self.validate(driver)
            .map_err(|_| invalid("Rollback skipped: native player identity changed"))?;
        let positions_result = driver.write(self.address + POSITIONS, old);
        let left_result = driver.write(self.address + LEFT_FOOT, &left);
        let right_result = driver.write(self.address + RIGHT_FOOT, &right);
        if positions_result.is_err() || left_result.is_err() || right_result.is_err() {
            return Err(invalid(&format!("Rollback incomplete: positions={positions_result:?}, left={left_result:?}, right={right_result:?}")));
        }
        self.validate(driver)?;
        if driver.read::<[u8; 15]>(self.address + POSITIONS)? != *old
            || driver.read::<u8>(self.address + LEFT_FOOT)? != left
            || driver.read::<u8>(self.address + RIGHT_FOOT)? != right
        {
            return Err(invalid("Rollback readback mismatch"));
        }
        Ok(())
    }

    pub fn write_positions(
        &self,
        driver: &LinuxMemoryDriver,
        positions: &PlayerPositions,
    ) -> Result<()> {
        self.validate(driver)?;
        let old = driver.read::<[u8; 15]>(self.address + POSITIONS)?;
        let mut next = [
            positions.gk,
            old[1],
            positions.dl,
            positions.dc,
            positions.dr,
            positions.dm,
            positions.ml,
            positions.mc,
            positions.mr,
            positions.aml,
            positions.amc,
            positions.amr,
            positions.st,
            positions.wbl,
            positions.wbr,
        ];
        if !next.iter().all(|v| (1..=20).contains(v))
            || !(1..=20).contains(&positions.left_foot)
            || !(1..=20).contains(&positions.right_foot)
        {
            return Err(invalid("Positions and feet must all be between 1 and 20"));
        }
        // SW (mask 2) is not an editable model field and stays unchanged.
        next[1] = old[1];
        let old_left = driver.read::<u8>(self.address + LEFT_FOOT)?;
        let old_right = driver.read::<u8>(self.address + RIGHT_FOOT)?;
        let write_result = (|| -> Result<()> {
            driver.write(self.address + POSITIONS, &next)?;
            driver.write(self.address + LEFT_FOOT, &(positions.left_foot * 5))?;
            driver.write(self.address + RIGHT_FOOT, &(positions.right_foot * 5))?;
            if &self.read_positions(driver)? != positions {
                return Err(invalid(
                    "Native position/feet readback did not match the requested edit",
                ));
            }
            Ok(())
        })();
        if let Err(error) = write_result {
            let rollback = self.rollback_positions(driver, &old, old_left, old_right);
            return Err(invalid(&format!(
                "Native position edit failed: {error}; rollback: {rollback:?}"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Vec<u64>, NativePlayerRecord) {
        let mut data = vec![0u64; 64];
        let a = data.as_mut_ptr() as usize;
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let r = NativePlayerRecord {
            address: a,
            pid: driver.pid(),
            index: 65304,
            uid: 2000448177,
            player_vtable: 42,
            person_vtable: 43,
        };
        driver.write(a, &42usize).unwrap();
        driver.write(a + PERSON, &43usize).unwrap();
        driver.write(a + INDEX, &r.index).unwrap();
        driver.write(a + UID, &r.uid).unwrap();
        driver.write(a + UID + 4, &r.uid).unwrap();
        driver.write(a + POSITIONS, &[1u8; 15]).unwrap();
        driver.write(a + LEFT_FOOT, &40u8).unwrap();
        driver.write(a + RIGHT_FOOT, &100u8).unwrap();
        (data, r)
    }
    #[test]
    fn native_round_trip_preserves_identity_and_surrounding_bytes() {
        let (data, r) = fixture();
        let before = data.clone();
        let d = LinuxMemoryDriver::new(std::process::id() as i32);
        let mut p = r.read_positions(&d).unwrap();
        p.dl = 15;
        p.wbr = 14;
        p.left_foot = 15;
        r.write_positions(&d, &p).unwrap();
        assert_eq!(r.read_positions(&d).unwrap(), p);
        for i in 0..data.len() * 8 {
            if (POSITIONS..POSITIONS + 15).contains(&i) || i == LEFT_FOOT || i == RIGHT_FOOT {
                continue;
            }
            let expected = (before[i / 8] >> ((i % 8) * 8)) as u8;
            assert_eq!(d.read::<u8>(r.address + i).unwrap(), expected, "byte {i:x}");
        }
    }
    #[test]
    fn rollback_restores_all_fields_but_refuses_recycled_record() {
        let (_data, r) = fixture();
        let d = LinuxMemoryDriver::new(std::process::id() as i32);
        let old = d.read::<[u8; 15]>(r.address + POSITIONS).unwrap();
        d.write(r.address + POSITIONS, &[20u8; 15]).unwrap();
        d.write(r.address + LEFT_FOOT, &75u8).unwrap();
        d.write(r.address + RIGHT_FOOT, &75u8).unwrap();
        r.rollback_positions(&d, &old, 40, 100).unwrap();
        assert_eq!(d.read::<[u8; 15]>(r.address + POSITIONS).unwrap(), old);
        assert_eq!(d.read::<u8>(r.address + LEFT_FOOT).unwrap(), 40);
        assert_eq!(d.read::<u8>(r.address + RIGHT_FOOT).unwrap(), 100);
        d.write(r.address + INDEX, &99i32).unwrap();
        assert!(r.rollback_positions(&d, &[20u8; 15], 75, 75).is_err());
        assert_eq!(d.read::<[u8; 15]>(r.address + POSITIONS).unwrap(), old);
        assert_eq!(d.read::<u8>(r.address + LEFT_FOOT).unwrap(), 40);
    }
    #[test]
    fn rejects_both_stale_uids_and_vtables() {
        let (_data, r) = fixture();
        let d = LinuxMemoryDriver::new(std::process::id() as i32);
        for offset in [0, PERSON, UID, UID + 4] {
            let original = d.read::<u32>(r.address + offset).unwrap();
            d.write(r.address + offset, &0u32).unwrap();
            assert!(r.read_positions(&d).is_err(), "identity offset {offset:x}");
            assert!(r.rollback_positions(&d, &[20u8; 15], 100, 100).is_err());
            d.write(r.address + offset, &original).unwrap();
        }
    }
    #[test]
    fn rating_boundaries_and_fractional_feet() {
        let (_data, r) = fixture();
        let d = LinuxMemoryDriver::new(std::process::id() as i32);
        for value in [1, 20] {
            let mut p = r.read_positions(&d).unwrap();
            p.dl = value;
            p.left_foot = value;
            p.right_foot = value;
            r.write_positions(&d, &p).unwrap();
            assert_eq!(r.read_positions(&d).unwrap(), p);
        }
        let p = r.read_positions(&d).unwrap();
        for value in [0, 21] {
            let mut invalid = p.clone();
            invalid.left_foot = value;
            assert!(r.write_positions(&d, &invalid).is_err());
            invalid = p.clone();
            invalid.right_foot = value;
            assert!(r.write_positions(&d, &invalid).is_err());
            invalid = p.clone();
            invalid.dr = value;
            assert!(r.write_positions(&d, &invalid).is_err());
            assert_eq!(r.read_positions(&d).unwrap(), p);
        }
        d.write(r.address + LEFT_FOOT, &99u8).unwrap();
        assert!(r.read_positions(&d).is_err());
    }
    #[test]
    fn rejects_stale_identity_and_invalid_ratings_before_writing() {
        let (_data, r) = fixture();
        let d = LinuxMemoryDriver::new(std::process::id() as i32);
        let mut p = r.read_positions(&d).unwrap();
        p.dl = 21;
        assert!(r.write_positions(&d, &p).is_err());
        d.write(r.address + INDEX, &7i32).unwrap();
        assert!(r.read_positions(&d).is_err());
        assert!(r.write_positions(&d, &PlayerPositions::default()).is_err());
    }
}
