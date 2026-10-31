//! Native club record and finances.
//!
//! Layout traced in game_plugin.dll:
//! - Club record: vtable, i32 index at +8, uid twice at +0xC/+0x10.
//! - First team: vector at +0x18 ([begin, end, cap]). First pointer is team.
//!   Team +0x30 points back to club. Main-team reputation is u16 at team +0xa8 (1..=10000).
//!   Stadium is pointer at team +0x78. Stadium capacity at +0x7c (u32), expansion at +0x8c (u32).
//! - Details subrecord at club +0x100:
//!   - +0xd0: founding year (u16, 1800..=2200)
//!   - +0x118: training facilities (u8, 1..=20)
//!   - +0x123: youth facilities (u8, 1..=20)
//!   - +0x124: junior coaching (u8, 1..=20)
//!   - +0x125: youth recruitment (u8, 1..=20)
//! - Finance object at club +0x150:
//!   - +0x8: back-pointer to club
//!   - +0x14: balance (i32 raw GBP)
//!   - +0x7cc: available transfer budget (i32 raw GBP)
//!   - +0x7d0: allocated transfer budget (i32 raw GBP)
//!   - +0x7d8: transfer retention percentage (u32, 0..=100)
//!   - +0x810: wage budget (i32 raw GBP)
//!   - +0x8b5: corporate facilities (u8, 1..=20)
//! - World reputation: u16 at club +0x166 (1..=10000)

use fmlev_mem::{parse_process_maps, LinuxMemoryDriver};

use crate::error::{CoreError, Result};
use crate::models::ClubFinances;

const CHUNK: usize = 16 << 20;
const DETAILS_OFFSET: usize = 0x100;
const FINANCE_OFFSET: usize = 0x150;
const FIRST_TEAM_VECTOR_OFFSET: usize = 0x18;
const BALANCE_OFFSET: usize = 0x14;
const TRANSFER_OFFSET: usize = 0x7cc;
const RETENTION_OFFSET: usize = 0x7d8;
const WAGE_BUDGET_OFFSET: usize = 0x810;
const CORP_FACILITIES_OFFSET: usize = 0x8b5;
const TRAINING_FACILITIES_OFFSET: usize = 0x118;
const YOUTH_FACILITIES_OFFSET: usize = 0x123;
const JUNIOR_COACHING_OFFSET: usize = 0x124;
const YOUTH_RECRUITMENT_OFFSET: usize = 0x125;
const DATA_ANALYSIS_FACILITIES_OFFSET: usize = 0x11e;
const SUGAR_DADDY_OFFSET: usize = 0x3c;
const WORLD_REPUTATION_OFFSET: usize = 0x166;
const TEAM_REPUTATION_OFFSET: usize = 0xa8;
#[allow(dead_code)]
const TEAM_BACKPTR_OFFSET: usize = 0x30;
const STADIUM_PTR_OFFSET: usize = 0x78;
const STADIUM_NAME_OFFSET: usize = 0x40;
const STADIUM_CAP_OFFSET: usize = 0x7c;
const STADIUM_EXP_OFFSET: usize = 0x8c;

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
pub struct NativeClubRecord {
    pub pid: i32,
    pub club: usize,
    pub details: Option<usize>,
    pub finance: usize,
    pub first_team: Option<usize>,
    pub stadium: Option<usize>,
    pub club_id: i32,
}

pub type NativeClubFinance = NativeClubRecord;

/// `(address, old bytes, new bytes)` that add `delta` raw GBP to the native balance of the club at `club`
/// (`finance + 0x14`, i32). The finance object must point back at the club; overflow is refused, not wrapped.
pub fn balance_edit(driver: &LinuxMemoryDriver, club: usize, delta: i64) -> Result<(usize, Vec<u8>, Vec<u8>)> {
    if club <= 0x10000 {
        return Err(invalid("Club pointer is not valid"));
    }
    let finance = driver.read_ptr(club + FINANCE_OFFSET)?;
    if finance <= 0x10000 || driver.read_ptr(finance + 8)? != club {
        return Err(invalid("Club finance object does not point back at the club"));
    }
    let old = driver.read::<i32>(finance + BALANCE_OFFSET)?;
    let next = i32::try_from(i64::from(old) + delta).map_err(|_| invalid("The club balance would leave the 32-bit range"))?;
    Ok((finance + BALANCE_OFFSET, old.to_le_bytes().to_vec(), next.to_le_bytes().to_vec()))
}

/// u32 length (1..=256) followed by UTF-8 text without control characters, at `pointer`.
fn read_length_prefixed(driver: &LinuxMemoryDriver, pointer: usize) -> Option<String> {
    if pointer <= 0x10000 {
        return None;
    }
    let len = driver.read::<u32>(pointer).ok()? as usize;
    if len == 0 || len > 256 {
        return None;
    }
    let mut buf = vec![0u8; len];
    if driver.read_bytes(pointer + 4, &mut buf).ok()? != len {
        return None;
    }
    String::from_utf8(buf).ok().filter(|text| !text.chars().any(char::is_control))
}

impl NativeClubRecord {
    /// Finds the native club record for `club_id` (cached; the cached record is re-validated).
    pub fn discover(driver: &LinuxMemoryDriver, club_id: i32) -> Result<Self> {
        crate::locator_cache::cached("club", driver.pid(), club_id, |r: &Self| r.validate(driver).is_ok(), || Self::scan(driver, club_id))
    }

    /// Finds the native club record for `club_id` whose finance object points back to it.
    /// Zero or several matches are rejected; no address is compiled in.
    fn scan(driver: &LinuxMemoryDriver, club_id: i32) -> Result<Self> {
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
                        let Ok(finance) = driver.read_ptr(club + FINANCE_OFFSET) else { continue };

                        let details = driver.read_ptr(club + DETAILS_OFFSET).ok().filter(|&p| {
                            p > 0x10000 && driver.read::<u16>(p + 0xd0).map_or(false, |y| (1800..=2200).contains(&y))
                        });

                        let first_team = (|| -> Option<usize> {
                            let begin = driver.read_ptr(club + FIRST_TEAM_VECTOR_OFFSET).ok()?;
                            let end = driver.read_ptr(club + FIRST_TEAM_VECTOR_OFFSET + 8).ok()?;
                            if begin == 0 || end <= begin || (end - begin) < 8 {
                                return None;
                            }
                            let team = driver.read_ptr(begin).ok()?;
                            if team > 0x10000 && driver.read_ptr(team + 0x30).ok() == Some(club) {
                                Some(team)
                            } else {
                                None
                            }
                        })();

                        let stadium = first_team.and_then(|team| {
                            let s = driver.read_ptr(team + STADIUM_PTR_OFFSET).ok()?;
                            if s > 0x10000 { Some(s) } else { None }
                        });

                        let candidate = Self {
                            pid: driver.pid(),
                            club,
                            details,
                            finance,
                            first_team,
                            stadium,
                            club_id,
                        };
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

    pub fn validate(&self, driver: &LinuxMemoryDriver) -> Result<()> {
        if driver.pid() != self.pid
            || driver.read::<i32>(self.club + 8)? != self.club_id
            || driver.read::<i32>(self.club + 0xc)? != driver.read::<i32>(self.club + 0x10)?
            || driver.read_ptr(self.club + FINANCE_OFFSET)? != self.finance
            || driver.read_ptr(self.finance + 8)? != self.club
        {
            return Err(invalid("Native club finance object changed; reopen Club > Finances and retry"));
        }
        if let Some(details) = self.details {
            if driver.read_ptr(self.club + DETAILS_OFFSET)? != details {
                return Err(invalid("Native club details object changed"));
            }
        }
        Ok(())
    }

    // --- Reads ---

    pub fn read_balance(&self, driver: &LinuxMemoryDriver) -> Result<i32> {
        self.validate(driver)?;
        Ok(driver.read::<i32>(self.finance + BALANCE_OFFSET)?)
    }

    pub fn read_transfer_budget(&self, driver: &LinuxMemoryDriver) -> Result<i32> {
        self.validate(driver)?;
        Ok(driver.read::<i32>(self.finance + TRANSFER_OFFSET)?)
    }

    pub fn read_transfer_retention(&self, driver: &LinuxMemoryDriver) -> Result<u32> {
        self.validate(driver)?;
        Ok(driver.read::<u32>(self.finance + RETENTION_OFFSET)?)
    }

    pub fn read_wage_budget(&self, driver: &LinuxMemoryDriver) -> Result<i32> {
        self.validate(driver)?;
        Ok(driver.read::<i32>(self.finance + WAGE_BUDGET_OFFSET)?)
    }

    pub fn read_corporate_facilities(&self, driver: &LinuxMemoryDriver) -> Result<u8> {
        self.validate(driver)?;
        Ok(driver.read::<u8>(self.finance + CORP_FACILITIES_OFFSET)?)
    }

    pub fn read_data_analysis_facilities(&self, driver: &LinuxMemoryDriver) -> Result<u8> {
        self.validate(driver)?;
        let details = self.details.ok_or_else(|| invalid("Club details subrecord not mapped"))?;
        Ok(driver.read::<u8>(details + DATA_ANALYSIS_FACILITIES_OFFSET)?)
    }

    pub fn read_sugar_daddy(&self, driver: &LinuxMemoryDriver) -> Result<u8> {
        self.validate(driver)?;
        Ok(driver.read::<u8>(self.finance + SUGAR_DADDY_OFFSET)?)
    }

    pub fn read_facilities(&self, driver: &LinuxMemoryDriver) -> Result<(u8, u8, u8, u8, u8, u8)> {
        self.validate(driver)?;
        let details = self.details.ok_or_else(|| invalid("Club details subrecord not mapped"))?;
        let training = driver.read::<u8>(details + TRAINING_FACILITIES_OFFSET)?;
        let youth = driver.read::<u8>(details + YOUTH_FACILITIES_OFFSET)?;
        let coaching = driver.read::<u8>(details + JUNIOR_COACHING_OFFSET)?;
        let recruitment = driver.read::<u8>(details + YOUTH_RECRUITMENT_OFFSET)?;
        let corp = driver.read::<u8>(self.finance + CORP_FACILITIES_OFFSET)?;
        let analysis = driver.read::<u8>(details + DATA_ANALYSIS_FACILITIES_OFFSET).unwrap_or(0);
        Ok((training, youth, coaching, recruitment, corp, analysis))
    }

    pub fn read_reputations(&self, driver: &LinuxMemoryDriver) -> Result<(u16, u16)> {
        self.validate(driver)?;
        let world = driver.read::<u16>(self.club + WORLD_REPUTATION_OFFSET)?;
        let team_rep = if let Some(team) = self.first_team {
            driver.read::<u16>(team + TEAM_REPUTATION_OFFSET).unwrap_or(0)
        } else {
            0
        };
        Ok((team_rep, world))
    }

    pub fn read_stadium_capacities(&self, driver: &LinuxMemoryDriver) -> Result<(u32, u32)> {
        self.validate(driver)?;
        let stadium = self.stadium.ok_or_else(|| invalid("Stadium record not mapped"))?;
        let cap = driver.read::<u32>(stadium + STADIUM_CAP_OFFSET)?;
        let exp = driver.read::<u32>(stadium + STADIUM_EXP_OFFSET)?;
        Ok((cap, exp))
    }

    /// Reads length-prefixed UTF-8 club name from `club + 0xc0`.
    pub fn read_name(&self, driver: &LinuxMemoryDriver) -> Option<String> {
        let str_ptr = driver.read_ptr(self.club + 0xc0).ok()?;
        if str_ptr <= 0x10000 {
            return None;
        }
        let len = driver.read::<u32>(str_ptr).ok()? as usize;
        if len == 0 || len > 256 {
            return None;
        }
        let mut buf = vec![0u8; len];
        if driver.read_bytes(str_ptr + 4, &mut buf).ok()? != len {
            return None;
        }
        String::from_utf8(buf).ok()
    }

    /// First-team squad occupancy `(players, capacity, spare slots)` from the team's squad vector, read-only.
    /// A transfer into this club's first team needs at least one spare slot.
    pub fn first_team_squad_space(&self, driver: &LinuxMemoryDriver) -> Result<(usize, usize, usize)> {
        let team = self.first_team.ok_or_else(|| invalid("Club has no valid first team"))?;
        let squad = crate::native_transfer::SquadVector::read(driver, team)?;
        Ok((squad.count(), squad.capacity_count(), squad.spare_slots()))
    }

    /// Stadium name of the main team's stadium (`stadium + 0x40`): a length-prefixed string, or two pooled
    /// name references (as for players), matching `fmscan entity ... club --editor` (Camp Nou, Santiago
    /// Bernabeu and Coliseum on a live save). `None` when the stadium is not mapped or the text does not
    /// validate; no name is ever invented.
    pub fn read_stadium_name(&self, driver: &LinuxMemoryDriver) -> Option<String> {
        let stadium = self.stadium?;
        let field = driver.read_ptr(stadium + STADIUM_NAME_OFFSET).ok()?;
        if field <= 0x10000 {
            return None;
        }
        if let Some(name) = read_length_prefixed(driver, field) {
            return Some(name);
        }
        let mut parts = Vec::new();
        for reference in [field, field + 8] {
            let pointer = driver.read_ptr(reference).ok()?;
            if pointer == 0 {
                continue;
            }
            parts.push(read_length_prefixed(driver, driver.read_ptr(pointer).ok()?)?);
        }
        (!parts.is_empty()).then(|| parts.join(" "))
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

    pub fn write_balance(&self, driver: &LinuxMemoryDriver, value: i64) -> Result<()> {
        let value = i32::try_from(value).map_err(|_| invalid("Balance is outside the 32-bit native range"))?;
        self.validate(driver)?;
        let old = self.read_balance(driver)?;
        let patch = FieldPatch {
            address: self.finance + BALANCE_OFFSET,
            old_bytes: old.to_le_bytes().to_vec(),
            new_bytes: value.to_le_bytes().to_vec(),
            label: "Balance",
        };
        self.apply_patches(driver, &[patch])
    }

    pub fn write_transfer_budget(&self, driver: &LinuxMemoryDriver, value: i64) -> Result<()> {
        if value < 0 {
            return Err(invalid("Transfer budget must not be negative"));
        }
        let value = i32::try_from(value).map_err(|_| invalid("Transfer budget is outside the 32-bit native range"))?;
        self.validate(driver)?;
        let old = self.read_transfer_budget(driver)?;
        let patch = FieldPatch {
            address: self.finance + TRANSFER_OFFSET,
            old_bytes: old.to_le_bytes().to_vec(),
            new_bytes: value.to_le_bytes().to_vec(),
            label: "Transfer budget",
        };
        self.apply_patches(driver, &[patch])
    }

    /// Transactionally updates changed club finances, facility ratings, reputations, and stadium capacities.
    pub fn write_club_edits(&self, driver: &LinuxMemoryDriver, previous: &ClubFinances, requested: &ClubFinances) -> Result<()> {
        self.validate(driver)?;
        let mut patches = Vec::new();

        // Balance
        if previous.balance != requested.balance {
            let value = i32::try_from(requested.balance).map_err(|_| invalid("Balance out of 32-bit range"))?;
            let old = driver.read::<i32>(self.finance + BALANCE_OFFSET)?;
            patches.push(FieldPatch {
                address: self.finance + BALANCE_OFFSET,
                old_bytes: old.to_le_bytes().to_vec(),
                new_bytes: value.to_le_bytes().to_vec(),
                label: "Balance",
            });
        }

        // Transfer budget
        if previous.transfer_budget != requested.transfer_budget {
            if requested.transfer_budget < 0 {
                return Err(invalid("Transfer budget must not be negative"));
            }
            let value = i32::try_from(requested.transfer_budget).map_err(|_| invalid("Transfer budget out of 32-bit range"))?;
            let old = driver.read::<i32>(self.finance + TRANSFER_OFFSET)?;
            patches.push(FieldPatch {
                address: self.finance + TRANSFER_OFFSET,
                old_bytes: old.to_le_bytes().to_vec(),
                new_bytes: value.to_le_bytes().to_vec(),
                label: "Transfer budget",
            });
        }

        // Facilities in details
        let facility_fields = [
            (previous.training_facilities, requested.training_facilities, TRAINING_FACILITIES_OFFSET, "Training facilities"),
            (previous.youth_facilities, requested.youth_facilities, YOUTH_FACILITIES_OFFSET, "Youth facilities"),
            (previous.junior_coaching, requested.junior_coaching, JUNIOR_COACHING_OFFSET, "Junior coaching"),
            (previous.youth_recruitment, requested.youth_recruitment, YOUTH_RECRUITMENT_OFFSET, "Youth recruitment"),
            (previous.data_analysis_facilities, requested.data_analysis_facilities, DATA_ANALYSIS_FACILITIES_OFFSET, "Data analysis facilities"),
        ];

        let has_details_change = facility_fields.iter().any(|(prev, req, _, _)| *prev != *req && *req > 0);
        if has_details_change {
            let details = self.details.ok_or_else(|| invalid("Club details subrecord not mapped; cannot update facilities"))?;
            for (prev, req, offset, label) in facility_fields {
                if prev != req && req > 0 {
                    if !(1..=20).contains(&req) {
                        return Err(invalid(&format!("{label} must be between 1 and 20")));
                    }
                    let old = driver.read::<u8>(details + offset)?;
                    patches.push(FieldPatch {
                        address: details + offset,
                        old_bytes: vec![old],
                        new_bytes: vec![req],
                        label,
                    });
                }
            }
        }

        // Corporate facilities in finance
        if previous.corporate_facilities != requested.corporate_facilities && requested.corporate_facilities > 0 {
            if !(1..=20).contains(&requested.corporate_facilities) {
                return Err(invalid("Corporate facilities must be between 1 and 20"));
            }
            let old = driver.read::<u8>(self.finance + CORP_FACILITIES_OFFSET)?;
            patches.push(FieldPatch {
                address: self.finance + CORP_FACILITIES_OFFSET,
                old_bytes: vec![old],
                new_bytes: vec![requested.corporate_facilities],
                label: "Corporate facilities",
            });
        }

        // Sugar daddy in finance
        if previous.sugar_daddy_type != requested.sugar_daddy_type && !requested.sugar_daddy_type.is_empty() {
            let sd_val: u8 = match requested.sugar_daddy_type.as_str() {
                "Foreground" | "1" => 1,
                "Background" | "FrontEnd" | "2" => 2,
                "Underwriter" | "3" => 3,
                _ => 0,
            };
            let old = driver.read::<u8>(self.finance + SUGAR_DADDY_OFFSET)?;
            patches.push(FieldPatch {
                address: self.finance + SUGAR_DADDY_OFFSET,
                old_bytes: vec![old],
                new_bytes: vec![sd_val],
                label: "Sugar daddy",
            });
        }

        // Reputations
        if previous.reputation != requested.reputation && requested.reputation > 0 {
            let team = self.first_team.ok_or_else(|| invalid("First team not mapped; cannot update team reputation"))?;
            if !(1..=10000).contains(&requested.reputation) {
                return Err(invalid("Reputation must be between 1 and 10000"));
            }
            let old = driver.read::<u16>(team + TEAM_REPUTATION_OFFSET)?;
            patches.push(FieldPatch {
                address: team + TEAM_REPUTATION_OFFSET,
                old_bytes: old.to_le_bytes().to_vec(),
                new_bytes: requested.reputation.to_le_bytes().to_vec(),
                label: "Reputation",
            });
        }

        // Stadium capacities
        if (previous.stadium_capacity != requested.stadium_capacity || previous.stadium_expansion_cap != requested.stadium_expansion_cap)
            && (requested.stadium_capacity > 0 || requested.stadium_expansion_cap > 0)
        {
            let stadium = self.stadium.ok_or_else(|| invalid("Stadium not mapped; cannot update stadium capacities"))?;
            if previous.stadium_capacity != requested.stadium_capacity && requested.stadium_capacity > 0 {
                let old = driver.read::<u32>(stadium + STADIUM_CAP_OFFSET)?;
                patches.push(FieldPatch {
                    address: stadium + STADIUM_CAP_OFFSET,
                    old_bytes: old.to_le_bytes().to_vec(),
                    new_bytes: requested.stadium_capacity.to_le_bytes().to_vec(),
                    label: "Stadium capacity",
                });
            }
            if previous.stadium_expansion_cap != requested.stadium_expansion_cap && requested.stadium_expansion_cap > 0 {
                let old = driver.read::<u32>(stadium + STADIUM_EXP_OFFSET)?;
                patches.push(FieldPatch {
                    address: stadium + STADIUM_EXP_OFFSET,
                    old_bytes: old.to_le_bytes().to_vec(),
                    new_bytes: requested.stadium_expansion_cap.to_le_bytes().to_vec(),
                    label: "Stadium expansion capacity",
                });
            }
        }

        self.apply_patches(driver, &patches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockClubFixture {
        _club_data: Vec<u8>,
        _details_data: Vec<u8>,
        _finance_data: Vec<u8>,
        _team_data: Vec<u8>,
        _stadium_data: Vec<u8>,
        _vector_data: Vec<usize>,
        record: NativeClubRecord,
        driver: LinuxMemoryDriver,
    }

    fn mock_club() -> MockClubFixture {
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let mut club_data = vec![0u8; 0x200];
        let mut details_data = vec![0u8; 0x200];
        let mut finance_data = vec![0u8; 0x1000];
        let mut team_data = vec![0u8; 0x200];
        let mut stadium_data = vec![0u8; 0x200];
        let mut vector_data = vec![0usize; 3];

        let club = club_data.as_mut_ptr() as usize;
        let details = details_data.as_mut_ptr() as usize;
        let finance = finance_data.as_mut_ptr() as usize;
        let team = team_data.as_mut_ptr() as usize;
        let stadium = stadium_data.as_mut_ptr() as usize;
        let vector = vector_data.as_mut_ptr() as usize;

        let club_id = 1074i32;
        let uid = 1708i32;

        // Club header
        driver.write(club + 8, &club_id).unwrap();
        driver.write(club + 0xc, &uid).unwrap();
        driver.write(club + 0x10, &uid).unwrap();
        driver.write(club + DETAILS_OFFSET, &details).unwrap();
        driver.write(club + FINANCE_OFFSET, &finance).unwrap();
        driver.write(club + WORLD_REPUTATION_OFFSET, &7500u16).unwrap();

        // Vector pointing to team
        vector_data[0] = team;
        vector_data[1] = team + 8;
        vector_data[2] = team + 8;
        driver.write(club + FIRST_TEAM_VECTOR_OFFSET, &vector).unwrap();
        driver.write(club + FIRST_TEAM_VECTOR_OFFSET + 8, &(vector + 8)).unwrap();

        // Details header
        driver.write(details + 0xd0, &1899u16).unwrap(); // founding year
        driver.write(details + TRAINING_FACILITIES_OFFSET, &18u8).unwrap();
        driver.write(details + YOUTH_FACILITIES_OFFSET, &19u8).unwrap();
        driver.write(details + JUNIOR_COACHING_OFFSET, &17u8).unwrap();
        driver.write(details + YOUTH_RECRUITMENT_OFFSET, &16u8).unwrap();
        driver.write(details + DATA_ANALYSIS_FACILITIES_OFFSET, &14u8).unwrap();

        // Finance header
        driver.write(finance + 8, &club).unwrap();
        driver.write(finance + BALANCE_OFFSET, &50_000_000i32).unwrap();
        driver.write(finance + TRANSFER_OFFSET, &30_000_000i32).unwrap();
        driver.write(finance + RETENTION_OFFSET, &100u32).unwrap();
        driver.write(finance + CORP_FACILITIES_OFFSET, &15u8).unwrap();
        driver.write(finance + SUGAR_DADDY_OFFSET, &1u8).unwrap();

        // Team header
        driver.write(team + TEAM_BACKPTR_OFFSET, &club).unwrap();
        driver.write(team + TEAM_REPUTATION_OFFSET, &8800u16).unwrap();
        driver.write(team + STADIUM_PTR_OFFSET, &stadium).unwrap();

        // Stadium header
        driver.write(stadium + STADIUM_CAP_OFFSET, &65_000u32).unwrap();
        driver.write(stadium + STADIUM_EXP_OFFSET, &100_000u32).unwrap();

        let record = NativeClubRecord {
            pid: driver.pid(),
            club,
            details: Some(details),
            finance,
            first_team: Some(team),
            stadium: Some(stadium),
            club_id,
        };

        MockClubFixture {
            _club_data: club_data,
            _details_data: details_data,
            _finance_data: finance_data,
            _team_data: team_data,
            _stadium_data: stadium_data,
            _vector_data: vector_data,
            record,
            driver,
        }
    }

    #[test]
    fn read_club_native_scalars() {
        let f = mock_club();
        assert_eq!(f.record.read_balance(&f.driver).unwrap(), 50_000_000);
        assert_eq!(f.record.read_transfer_budget(&f.driver).unwrap(), 30_000_000);
        assert_eq!(f.record.read_facilities(&f.driver).unwrap(), (18, 19, 17, 16, 15, 14));
        assert_eq!(f.record.read_data_analysis_facilities(&f.driver).unwrap(), 14);
        assert_eq!(f.record.read_sugar_daddy(&f.driver).unwrap(), 1);
        assert_eq!(f.record.read_reputations(&f.driver).unwrap(), (8800, 7500));
        assert_eq!(f.record.read_stadium_capacities(&f.driver).unwrap(), (65_000, 100_000));
    }

    #[test]
    fn write_club_edits_round_trip_and_validation() {
        let f = mock_club();
        let mut prev = ClubFinances::default();
        prev.club_id = f.record.club_id;
        prev.balance = 50_000_000;
        prev.transfer_budget = 30_000_000;
        prev.training_facilities = 18;
        prev.youth_facilities = 19;
        prev.junior_coaching = 17;
        prev.youth_recruitment = 16;
        prev.corporate_facilities = 15;
        prev.data_analysis_facilities = 14;
        prev.sugar_daddy_type = "Foreground".to_string();
        prev.reputation = 8800;
        prev.stadium_capacity = 65_000;
        prev.stadium_expansion_cap = 100_000;

        let mut req = prev.clone();
        req.balance = 75_000_000;
        req.transfer_budget = 45_000_000;
        req.training_facilities = 20;
        req.youth_facilities = 20;
        req.junior_coaching = 20;
        req.youth_recruitment = 20;
        req.corporate_facilities = 20;
        req.data_analysis_facilities = 20;
        req.sugar_daddy_type = "Underwriter".to_string();
        req.reputation = 9500;
        req.stadium_capacity = 80_000;
        req.stadium_expansion_cap = 110_000;

        f.record.write_club_edits(&f.driver, &prev, &req).unwrap();

        assert_eq!(f.record.read_balance(&f.driver).unwrap(), 75_000_000);
        assert_eq!(f.record.read_transfer_budget(&f.driver).unwrap(), 45_000_000);
        assert_eq!(f.record.read_facilities(&f.driver).unwrap(), (20, 20, 20, 20, 20, 20));
        assert_eq!(f.record.read_data_analysis_facilities(&f.driver).unwrap(), 20);
        assert_eq!(f.record.read_sugar_daddy(&f.driver).unwrap(), 3);
        assert_eq!(f.record.read_reputations(&f.driver).unwrap(), (9500, 7500));
        assert_eq!(f.record.read_stadium_capacities(&f.driver).unwrap(), (80_000, 110_000));
    }

    #[test]
    fn rejects_out_of_range_facility_and_negative_budget() {
        let f = mock_club();
        let prev = ClubFinances::default();
        let mut req = prev.clone();
        req.training_facilities = 25;
        assert!(f.record.write_club_edits(&f.driver, &prev, &req).is_err());

        let mut req_budget = prev.clone();
        req_budget.transfer_budget = -100;
        assert!(f.record.write_club_edits(&f.driver, &prev, &req_budget).is_err());
    }

    /// A length-prefixed string buffer: u32 length, then the bytes.
    fn text(value: &str) -> Vec<u8> {
        let mut bytes = (value.len() as u32).to_le_bytes().to_vec();
        bytes.extend_from_slice(value.as_bytes());
        bytes
    }

    #[test]
    fn stadium_name_reads_a_direct_length_prefixed_string() {
        let fixture = mock_club();
        let name = text("Camp Nou");
        let stadium = fixture.record.stadium.unwrap();
        fixture.driver.write(stadium + STADIUM_NAME_OFFSET, &(name.as_ptr() as usize)).unwrap();
        assert_eq!(fixture.record.read_stadium_name(&fixture.driver).as_deref(), Some("Camp Nou"));
    }

    #[test]
    fn stadium_name_reads_pooled_parts_and_joins_them() {
        let fixture = mock_club();
        let (first, second) = (text("Estadio"), text("Coliseum"));
        // The field points at two references; each reference points at a pointer to the string.
        let first_pointer = first.as_ptr() as usize;
        let second_pointer = second.as_ptr() as usize;
        let slots = [first_pointer, second_pointer];
        let refs = [slots.as_ptr() as usize, unsafe { slots.as_ptr().add(1) } as usize];
        let stadium = fixture.record.stadium.unwrap();
        fixture.driver.write(stadium + STADIUM_NAME_OFFSET, &(refs.as_ptr() as usize)).unwrap();
        assert_eq!(fixture.record.read_stadium_name(&fixture.driver).as_deref(), Some("Estadio Coliseum"));
    }

    #[test]
    fn stadium_name_is_none_for_bad_or_missing_data_never_a_default() {
        let mut fixture = mock_club();
        let stadium = fixture.record.stadium.unwrap();
        // No name pointer written: nothing to read.
        assert_eq!(fixture.record.read_stadium_name(&fixture.driver), None);
        // Control characters are not a name.
        let dirty = text("Bad\u{1}Name");
        fixture.driver.write(stadium + STADIUM_NAME_OFFSET, &(dirty.as_ptr() as usize)).unwrap();
        assert_eq!(fixture.record.read_stadium_name(&fixture.driver), None);
        // An empty (zero-length) string is not a name either.
        let empty = text("");
        fixture.driver.write(stadium + STADIUM_NAME_OFFSET, &(empty.as_ptr() as usize)).unwrap();
        assert_eq!(fixture.record.read_stadium_name(&fixture.driver), None);
        // A club whose stadium record is not mapped has no stadium name.
        fixture.record.stadium = None;
        assert_eq!(fixture.record.read_stadium_name(&fixture.driver), None);
    }

    #[test]
    fn balance_edit_adds_a_delta_and_refuses_overflow_or_a_foreign_finance_object() {
        let f = mock_club();
        let (address, old, next) = balance_edit(&f.driver, f.record.club, -5_000_000).unwrap();
        assert_eq!(address, f.record.finance + BALANCE_OFFSET);
        assert_eq!(i32::from_le_bytes(old.try_into().unwrap()), 50_000_000);
        assert_eq!(i32::from_le_bytes(next.try_into().unwrap()), 45_000_000);
        assert!(balance_edit(&f.driver, f.record.club, i64::from(i32::MAX)).is_err(), "would overflow upwards");
        assert!(balance_edit(&f.driver, f.record.club, -i64::from(i32::MAX) * 2).is_err(), "would overflow downwards");
        // A finance object that does not point back at the club is refused.
        f.driver.write(f.record.finance + 8, &0usize).unwrap();
        assert!(balance_edit(&f.driver, f.record.club, 1).is_err());
        assert!(balance_edit(&f.driver, 0, 1).is_err());
    }
}
