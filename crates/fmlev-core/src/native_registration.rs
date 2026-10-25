//! "Register anytime". Two parts, both verified in game:
//! 1. Manager flag: bit 0x100 of the dword at X + 0x404 on the human manager object, where
//!    X = first element of the vector at [game_plugin + 0x4e360f8]->+0x18. Enough for
//!    continental competitions (Champions League).
//! 2. Domestic leagues also read a per-stage rule entry: competition record (+0xb0) -> ACTUAL_COMP,
//!    [ACT+0x208] stage, [stage+0x1e0] vector object, begin/end at +0x60/+0x68, current stage index
//!    byte at ACT+0x31. Entry + 0x9a = 1 means "registration currently allowed"; entry + 0xcd is the
//!    competition type. The Registration screen shows "Registration Open" and Confirm stays enabled.
//!
//! Rule entries are per competition and rebuilt on a new season or stage change; re-apply then.

use fmlev_mem::{parse_process_maps, LinuxMemoryDriver};

use crate::error::{CoreError, Result};

pub const REGISTRATION_BIT: u32 = 0x100;
pub const SQUAD_NUMBER_BIT: u32 = 0x200;
pub const MANAGER_FLAGS_BITS: u32 = REGISTRATION_BIT | SQUAD_NUMBER_BIT;
const HUMAN_FLAGS: usize = 0x404;
const HUMAN_VTABLE_RVA: usize = 0x44a_5238;
const HUMAN_LIST_RVA: usize = 0x4e3_60f8;
const GATE_BYTE: usize = 0x9a;
const TYPE_BYTE: usize = 0xcd;

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidValue(message.to_owned())
}

fn module_base(driver: &LinuxMemoryDriver) -> Result<usize> {
    parse_process_maps(driver.pid())?
        .iter()
        .filter(|m| m.pathname.ends_with("game_plugin.dll"))
        .map(|m| m.start)
        .min()
        .ok_or_else(|| invalid("game_plugin.dll is not mapped; attach to FM26 first"))
}

/// The human manager object, validated by its HUMAN_NON_PLAYER vftable at +0x450.
fn human_object(driver: &LinuxMemoryDriver, base: usize) -> Result<usize> {
    let list = driver.read_ptr(base + HUMAN_LIST_RVA)?;
    let begin = driver.read_ptr(list + 0x18)?;
    let human = driver.read_ptr(begin)?;
    if list <= 0x10000 || begin <= 0x10000 || human <= 0x10000 || driver.read_ptr(human + 0x450)? != base + HUMAN_VTABLE_RVA {
        return Err(invalid("Human manager object failed validation"));
    }
    Ok(human)
}

/// Competition record by index and unique id (vtable, i32 index at +8, uid twice).
fn competition_record(driver: &LinuxMemoryDriver, index: i32, uid: i32) -> Result<usize> {
    let maps = parse_process_maps(driver.pid())?;
    let readonly = |p: usize| maps.iter().any(|m| m.is_readable() && !m.is_writable() && p >= m.start && p < m.end);
    let mut found = Vec::new();
    for map in maps.iter().filter(|m| m.is_rw_private()) {
        let mut start = map.start;
        while start < map.end {
            let len = (map.end - start).min(16 << 20);
            let mut buf = vec![0u8; len];
            if driver.read_bytes(start, &mut buf).is_ok() {
                for i in (8..len.saturating_sub(12)).step_by(8) {
                    let word = |at: usize| i32::from_le_bytes(buf[at..at + 4].try_into().unwrap());
                    if word(i) == index && word(i + 4) == uid && word(i + 8) == uid
                        && driver.read_ptr(start + i - 8).is_ok_and(&readonly)
                    {
                        found.push(start + i - 8);
                    }
                }
            }
            start += len;
        }
    }
    match found.as_slice() {
        [record] => Ok(*record),
        other => Err(invalid(&format!("Expected one competition record for {index}/{uid}, found {}", other.len()))),
    }
}

/// Address of the current stage's rule entry gate byte for a competition.
fn gate_byte_address(driver: &LinuxMemoryDriver, comp: usize) -> Result<(usize, u8)> {
    let act = driver.read_ptr(comp + 0xb0)?;
    let stage = driver.read_ptr(act + 0x208)?;
    let vec_obj = driver.read_ptr(stage + 0x1e0)?;
    let (begin, end) = (driver.read_ptr(vec_obj + 0x60)?, driver.read_ptr(vec_obj + 0x68)?);
    let stage_index = driver.read::<u8>(act + 0x31)?;
    if act <= 0x10000 || stage <= 0x10000 || vec_obj <= 0x10000 || begin == 0 || begin >= end
        || (end - begin) % 8 != 0 || stage_index == 0xff || usize::from(stage_index) >= (end - begin) / 8
    {
        return Err(invalid("Competition stage rules failed validation"));
    }
    let entry = driver.read_ptr(begin + usize::from(stage_index) * 8)?;
    if entry <= 0x10000 {
        return Err(invalid("Competition stage rule entry is missing"));
    }
    let kind = driver.read::<u8>(entry + TYPE_BYTE)?;
    Ok((entry + GATE_BYTE, kind))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegistrationState {
    pub manager_flag: bool,
    pub squad_number_flag: bool,
    pub competition_gate: bool,
    /// Gate byte before the first change, so disabling restores it exactly.
    pub original_gate: u8,
}

/// Enables or disables registration-anytime and squad-number-anytime for the managed human
/// and one domestic competition.
/// `original_gate` must be the value returned when it was first enabled (None on first call).
pub fn set_registration_anytime(
    driver: &LinuxMemoryDriver,
    comp_index: i32,
    comp_uid: i32,
    enabled: bool,
    original_gate: Option<u8>,
) -> Result<RegistrationState> {
    let base = module_base(driver)?;
    let human = human_object(driver, base)?;
    let (gate, _kind) = gate_byte_address(driver, competition_record(driver, comp_index, comp_uid)?)?;
    let current_gate = driver.read::<u8>(gate)?;
    if !enabled && original_gate.is_none() {
        return Err(invalid("No saved original rule value for this competition; nothing to restore"));
    }
    let original = original_gate.unwrap_or(current_gate);
    let flags = driver.read::<u32>(human + HUMAN_FLAGS)?;
    let (new_flags, new_gate) = if enabled {
        (flags | MANAGER_FLAGS_BITS, 1u8)
    } else {
        (flags & !MANAGER_FLAGS_BITS, original)
    };
    driver.write(human + HUMAN_FLAGS, &new_flags)?;
    if let Err(error) = driver.write(gate, &new_gate) {
        let _ = driver.write(human + HUMAN_FLAGS, &flags);
        let _ = driver.write(gate, &current_gate);
        return Err(error.into());
    }
    let verified = driver.read::<u32>(human + HUMAN_FLAGS)? == new_flags && driver.read::<u8>(gate)? == new_gate;
    if !verified {
        let _ = driver.write(human + HUMAN_FLAGS, &flags);
        let _ = driver.write(gate, &current_gate);
        return Err(invalid("Registration write readback mismatch; rolled back"));
    }
    Ok(RegistrationState {
        manager_flag: new_flags & REGISTRATION_BIT != 0,
        squad_number_flag: new_flags & SQUAD_NUMBER_BIT != 0,
        competition_gate: new_gate != 0,
        original_gate: original,
    })
}

/// Toggles the squad-number-anytime manager flag (bit 0x200 at manager + 0x404)
/// with transactional readback verification and rollback on failure.
pub fn set_squad_number_anytime(
    driver: &LinuxMemoryDriver,
    enabled: bool,
) -> Result<bool> {
    let base = module_base(driver)?;
    let human = human_object(driver, base)?;
    let flags = driver.read::<u32>(human + HUMAN_FLAGS)?;
    let new_flags = if enabled {
        flags | SQUAD_NUMBER_BIT
    } else {
        flags & !SQUAD_NUMBER_BIT
    };
    if new_flags == flags {
        return Ok(new_flags & SQUAD_NUMBER_BIT != 0);
    }
    driver.write(human + HUMAN_FLAGS, &new_flags)?;
    if driver.read::<u32>(human + HUMAN_FLAGS)? != new_flags {
        let _ = driver.write(human + HUMAN_FLAGS, &flags);
        return Err(invalid("Squad number flag readback mismatch; rolled back"));
    }
    Ok(new_flags & SQUAD_NUMBER_BIT != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_bits_and_state_flags() {
        assert_eq!(REGISTRATION_BIT, 0x100);
        assert_eq!(SQUAD_NUMBER_BIT, 0x200);
        assert_eq!(MANAGER_FLAGS_BITS, 0x300);

        let state = RegistrationState {
            manager_flag: true,
            squad_number_flag: true,
            competition_gate: true,
            original_gate: 0,
        };
        assert!(state.manager_flag);
        assert!(state.squad_number_flag);
        assert!(state.competition_gate);
        assert_eq!(state.original_gate, 0);
    }
}
