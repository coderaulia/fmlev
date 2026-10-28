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
use serde::Serialize;

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

/// Read-only snapshot of the registration gates the game checks for one competition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct RegistrationGates {
    pub manager_registration_bit: bool,
    pub manager_squad_number_bit: bool,
    pub stage_gate_open: bool,
    pub competition_type: u8,
    pub max_squad_size: u8,
    /// 255 = no home-grown requirement.
    pub min_homegrown: u8,
    /// 255 = standard (no cap).
    pub max_squad_number: u8,
}

/// Reads the registration gates without changing anything.
pub fn read_registration_gates(driver: &LinuxMemoryDriver, comp_index: i32, comp_uid: i32) -> Result<RegistrationGates> {
    let base = module_base(driver)?;
    let human = human_object(driver, base)?;
    let flags = driver.read::<u32>(human + HUMAN_FLAGS)?;
    let (gate, kind) = gate_byte_address(driver, competition_record(driver, comp_index, comp_uid)?)?;
    let entry = gate - GATE_BYTE;
    Ok(RegistrationGates {
        manager_registration_bit: flags & REGISTRATION_BIT != 0,
        manager_squad_number_bit: flags & SQUAD_NUMBER_BIT != 0,
        stage_gate_open: driver.read::<u8>(gate)? != 0,
        competition_type: kind,
        max_squad_size: driver.read::<u8>(entry + 0x98)?,
        min_homegrown: driver.read::<u8>(entry + 0x99)?,
        max_squad_number: driver.read::<u8>(entry + 0x9b)?,
    })
}

/// What the game had before the first enable, so disabling restores it exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegistrationOriginal {
    /// The competition's stage gate byte.
    pub gate: u8,
    /// The manager's `REGISTRATION_BIT | SQUAD_NUMBER_BIT` bits (the game or the user may already have set them).
    pub manager_bits: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegistrationState {
    pub manager_flag: bool,
    pub squad_number_flag: bool,
    pub competition_gate: bool,
    /// Values before the first change, kept by the caller until the competition is restored.
    pub original: RegistrationOriginal,
}

/// Manager flags after an enable or disable. The two bits are shared by every competition, so disabling
/// one competition leaves them alone while another is still enabled, and otherwise restores the bits the
/// manager had before the first enable (not blindly cleared).
fn next_manager_flags(flags: u32, enabled: bool, original: RegistrationOriginal, others_enabled: bool) -> u32 {
    if enabled {
        flags | MANAGER_FLAGS_BITS
    } else if others_enabled {
        flags
    } else {
        (flags & !MANAGER_FLAGS_BITS) | (original.manager_bits & MANAGER_FLAGS_BITS)
    }
}

/// Enables or disables registration-anytime and squad-number-anytime for the managed human
/// and one domestic competition.
/// `original` must be the value returned when it was first enabled (None on first call).
/// `others_enabled` is true while another competition is still enabled (the manager bits are shared).
pub fn set_registration_anytime(
    driver: &LinuxMemoryDriver,
    comp_index: i32,
    comp_uid: i32,
    enabled: bool,
    original: Option<RegistrationOriginal>,
    others_enabled: bool,
) -> Result<RegistrationState> {
    let base = module_base(driver)?;
    let human = human_object(driver, base)?;
    let (gate, _kind) = gate_byte_address(driver, competition_record(driver, comp_index, comp_uid)?)?;
    let current_gate = driver.read::<u8>(gate)?;
    let flags = driver.read::<u32>(human + HUMAN_FLAGS)?;
    if !enabled && original.is_none() {
        return Err(invalid("No saved original rule value for this competition; nothing to restore"));
    }
    let original = original.unwrap_or(RegistrationOriginal { gate: current_gate, manager_bits: flags & MANAGER_FLAGS_BITS });
    let new_flags = next_manager_flags(flags, enabled, original, others_enabled);
    let new_gate = if enabled { 1u8 } else { original.gate };
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
        original,
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
    }

    fn original(manager_bits: u32) -> RegistrationOriginal {
        RegistrationOriginal { gate: 0, manager_bits }
    }

    #[test]
    fn enabling_sets_both_manager_bits_and_keeps_the_rest() {
        assert_eq!(next_manager_flags(0x0000_0401, true, original(0), false), 0x0000_0701);
    }

    #[test]
    fn disabling_restores_the_original_manager_bits_not_a_blind_clear() {
        // The manager already had the registration bit before the first enable: it must still be set after.
        assert_eq!(next_manager_flags(0x0000_0701, false, original(REGISTRATION_BIT), false), 0x0000_0501);
        // Nothing was set before: both bits are cleared and unrelated bits stay.
        assert_eq!(next_manager_flags(0x8000_0701, false, original(0), false), 0x8000_0401);
        // Garbage outside the two bits in the saved value never leaks back in.
        assert_eq!(next_manager_flags(0x0000_0701, false, original(0xffff_ffff), false), 0x0000_0701);
    }

    #[test]
    fn disabling_one_competition_keeps_the_shared_bits_while_another_is_enabled() {
        assert_eq!(next_manager_flags(0x0000_0701, false, original(0), true), 0x0000_0701);
    }
}
