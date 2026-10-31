//! Narrow native position/footedness layout, validated against live bindings before discovery.
//! No managed reference is ever accepted as a native record.
use crate::{
    error::{CoreError, Result},
    models::{Player, PlayerAttributes, PlayerContract, PlayerPositions},
    preferred_moves::{PreferredMoves, PPM_BITSET_OFFSET},
};
use fmlev_mem::{parse_process_maps, LinuxMemoryDriver};

const POSITIONS: usize = 8;
const LEFT_FOOT: usize = 0x2f;
const RIGHT_FOOT: usize = 0x30;
const PERSON: usize = 0x140;
/// Pointer at record + 0x1f0 to a loan node; non-null while the player is on loan (the screens then show the loan contract).
const LOAN_NODE_OFFSET: usize = 0x1f0;
const INDEX: usize = PERSON + 8;
const UID: usize = PERSON + 12;
const SQUAD_NUMBER: usize = 0x5d;
const NO_SQUAD_NUMBER: u8 = 255;
const CONTRACT_TEAM: usize = 0x10;
const FAMILIARITY: usize = 0x48; // pointer to the familiarity object lies at record - 0x48
const FAMILIARITY_MAX: u32 = 10_000;
const CHUNK: usize = 16 << 20;

pub struct NativeAttribute {
    pub name: &'static str,
    offset: usize,
    scale: u8,
    pub get: fn(&PlayerAttributes) -> u8,
    pub set: fn(&mut PlayerAttributes, u8),
}

macro_rules! attributes {
    ($name:ident; $($field:ident: $offset:literal => $scale:literal),+ $(,)?) => {
        pub const $name: &[NativeAttribute] = &[$(NativeAttribute {
            name: stringify!($field), offset: $offset, scale: $scale,
            get: |a| a.$field, set: |a, value| a.$field = value,
        }),+];
    };
}

// Native ratings use a 0..100 scale with nearest-integer 1..20 presentation.
// This block was correlated against independent bindings for Dion and Zulkifli.
// Every record must also pass the complete observed-attribute check below.
attributes! { NATIVE_OUTFIELD_ATTRIBUTES;
    crossing: 0x17 => 5, dribbling: 0x18 => 5, finishing: 0x19 => 5, heading: 0x1a => 5,
    long_shots: 0x1b => 5, off_the_ball: 0x1d => 5, passing: 0x1e => 5,
    penalty_taking: 0x1f => 5, tackling: 0x20 => 5, vision: 0x21 => 5,
    anticipation: 0x28 => 5, decisions: 0x29 => 5, positioning: 0x2b => 5,
    first_touch: 0x2d => 5, technique: 0x2e => 5, flair: 0x31 => 5, corners: 0x32 => 5,
    teamwork: 0x33 => 5, work_rate: 0x34 => 5, acceleration: 0x39 => 5,
    free_kicks: 0x3a => 5, strength: 0x3b => 5, stamina: 0x3c => 5, pace: 0x3d => 5,
    jumping_reach: 0x3e => 5, leadership: 0x3f => 5, balance: 0x41 => 5, bravery: 0x42 => 5,
    aggression: 0x44 => 5, agility: 0x45 => 5, natural_fitness: 0x49 => 5,
    determination: 0x4a => 5, composure: 0x4b => 5, concentration: 0x4c => 5,
}

attributes! { NATIVE_GOALKEEPER_ATTRIBUTES;
    handling: 0x22 => 5, aerial_reach: 0x23 => 5, command_of_area: 0x24 => 5,
    communication: 0x25 => 5, kicking: 0x26 => 5, throwing: 0x27 => 5,
    one_on_ones: 0x2a => 5, reflexes: 0x2c => 5, eccentricity: 0x36 => 5,
    rushing_out: 0x37 => 5, passing: 0x1e => 5, first_touch: 0x2d => 5,
    anticipation: 0x28 => 5, decisions: 0x29 => 5, positioning: 0x2b => 5,
    flair: 0x31 => 5, teamwork: 0x33 => 5, work_rate: 0x34 => 5,
    acceleration: 0x39 => 5, strength: 0x3b => 5, stamina: 0x3c => 5, pace: 0x3d => 5,
    jumping_reach: 0x3e => 5, leadership: 0x3f => 5, balance: 0x41 => 5, bravery: 0x42 => 5,
    aggression: 0x44 => 5, agility: 0x45 => 5, natural_fitness: 0x49 => 5,
    determination: 0x4a => 5, composure: 0x4b => 5, concentration: 0x4c => 5,
    off_the_ball: 0x1d => 5, vision: 0x21 => 5,
}

// Relative offsets corroborated by FMSuperScout's native field map. Our validated
// player subobject is 0x148 bytes after its plao base. No third-party code copied.
attributes! { NATIVE_EXTRA_ATTRIBUTES;
    marking: 0x1c => 5, handling: 0x22 => 5, aerial_reach: 0x23 => 5,
    command_of_area: 0x24 => 5, communication: 0x25 => 5, kicking: 0x26 => 5,
    throwing: 0x27 => 5, one_on_ones: 0x2a => 5, reflexes: 0x2c => 5,
    long_throws: 0x35 => 5, eccentricity: 0x36 => 5, rushing_out: 0x37 => 5,
    punching: 0x38 => 5, dirtiness: 0x40 => 5, consistency: 0x43 => 5,
    important_matches: 0x46 => 5, injury_proneness: 0x47 => 5, versatility: 0x48 => 5,
    adaptability: 0x1b0 => 1, ambition: 0x1b1 => 1, loyalty: 0x1b2 => 1,
    pressure: 0x1b3 => 1, professionalism: 0x1b4 => 1, sportsmanship: 0x1b5 => 1,
    temperament: 0x1b6 => 1, controversy: 0x1b7 => 1,
}

pub fn native_attributes() -> impl Iterator<Item = &'static NativeAttribute> {
    NATIVE_OUTFIELD_ATTRIBUTES.iter().chain(NATIVE_EXTRA_ATTRIBUTES.iter())
}

impl NativeAttribute {
    /// Decode an already read native record for read-only diagnostics. Does not
    /// calibrate or authorize a NativePlayerRecord or any live write.
    pub fn decode_snapshot(&self, bytes: &[u8]) -> Result<u8> {
        self.decode(*bytes.get(self.offset).ok_or_else(|| invalid("Short native attribute snapshot"))?)
    }

    fn decode(&self, raw: u8) -> Result<u8> {
        if self.scale == 5 { displayed_rating(raw) }
        else if raw <= 20 { Ok(raw) }
        else { Err(invalid(&format!("Native {} is outside the verified personality scale", self.name))) }
    }
}

fn displayed_rating(raw: u8) -> Result<u8> {
    if raw > 100 {
        return Err(invalid("Native attribute exceeded the verified 0..100 scale"));
    }
    Ok(((raw as u16 + 2) / 5).max(1) as u8)
}

struct NativeEdit { address: usize, old: Vec<u8>, next: Vec<u8> }

impl NativeEdit {
    fn capture(driver: &LinuxMemoryDriver, address: usize, next: Vec<u8>) -> Result<Self> {
        let mut old = vec![0; next.len()];
        if driver.read_bytes(address, &mut old)? != old.len() { return Err(invalid("Short native edit snapshot")); }
        Ok(Self { address, old, next })
    }
}

#[derive(Clone, Copy)]
struct NativeContract { address: usize, vtable: usize }

fn leap(year: u32) -> bool { year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) }

fn expiry_with_year(raw: u32, year: i32) -> Result<u32> {
    let old_year = raw >> 16;
    let mut day = raw & 0x1ff;
    if !(2000..=2200).contains(&old_year) || !(2000..=2200).contains(&year)
        || day == 0 || day > if leap(old_year) { 366 } else { 365 }
    { return Err(invalid("Contract expiry is not a valid FM calendar date")); }
    let mut month = 0;
    let mut days = [31, if leap(old_year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    while day > days[month] { day -= days[month]; month += 1; }
    days[1] = if leap(year as u32) { 29 } else { 28 };
    day = day.min(days[month]);
    let new_day = days[..month].iter().sum::<u32>() + day;
    Ok(((year as u32) << 16) | (raw & 0xfe00) | new_day)
}

#[derive(Debug, Clone)]
pub struct NativePlayerRecord {
    pub(crate) address: usize,
    pub(crate) pid: i32,
    pub(crate) index: i32,
    pub(crate) uid: u32,
    pub(crate) player_vtable: usize,
    pub(crate) person_vtable: usize,
    pub(crate) attributes_verified: bool,
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
                attributes_verified: false,
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

    /// Discover a native player record directly from memory by their database index (id), cached; the
    /// cached record is re-validated (identity and both vtables) before every reuse.
    pub fn discover_by_index(driver: &LinuxMemoryDriver, index: i32) -> Result<Self> {
        crate::locator_cache::cached("player", driver.pid(), index, |r: &Self| r.validate(driver).is_ok(), || Self::scan_by_index(driver, index))
    }

    /// Discover a native player record directly from memory by their database index (id)
    /// without requiring an open screen or UI bindings. Validates identity, duplicate UID,
    /// readonly vtables, 15 positions, and scaled feet.
    fn scan_by_index(driver: &LinuxMemoryDriver, index: i32) -> Result<Self> {
        if index <= 0 {
            return Err(invalid("A live player index is required"));
        }
        let maps = parse_process_maps(driver.pid())?;
        let readonly = |p: usize| maps.iter().any(|m| m.is_readable() && !m.is_writable() && p >= m.start && p < m.end);
        let executable = |p: usize| maps.iter().any(|m| m.is_executable() && p >= m.start && p < m.end);
        let mut found = Vec::new();
        for map in maps.iter().filter(|m| m.is_rw_private()) {
            let mut start = map.start;
            while start < map.end {
                let len = (map.end - start).min(CHUNK);
                let mut buf = vec![0u8; len];
                if driver.read_bytes(start, &mut buf).is_ok() {
                    for i in (INDEX..len.saturating_sub(12)).step_by(8) {
                        let word = |at: usize| i32::from_le_bytes(buf[at..at + 4].try_into().unwrap());
                        let uword = |at: usize| u32::from_le_bytes(buf[at..at + 4].try_into().unwrap());
                        if word(i) != index {
                            continue;
                        }
                        let uid = uword(i + 4);
                        let uid2 = uword(i + 8);
                        if uid == 0 || uid != uid2 {
                            continue;
                        }
                        let address = start + i - INDEX;
                        if address % 8 != 0 {
                            continue;
                        }
                        let Ok(player_vtable) = driver.read_ptr(address) else { continue };
                        let Ok(person_vtable) = driver.read_ptr(address + PERSON) else { continue };
                        if !readonly(player_vtable) || !readonly(person_vtable) {
                            continue;
                        }
                        if !driver.read_ptr(player_vtable).is_ok_and(&executable)
                            || !driver.read_ptr(person_vtable).is_ok_and(&executable)
                        {
                            continue;
                        }
                        let Ok(native_aptitudes) = driver.read::<[u8; 15]>(address + POSITIONS) else { continue };
                        if !native_aptitudes.iter().all(|v| (1..=20).contains(v)) {
                            continue;
                        }
                        let Ok(left) = driver.read::<u8>(address + LEFT_FOOT) else { continue };
                        let Ok(right) = driver.read::<u8>(address + RIGHT_FOOT) else { continue };
                        if !(5..=100).contains(&left) || left % 5 != 0 || !(5..=100).contains(&right) || right % 5 != 0 {
                            continue;
                        }
                        let mut record = Self {
                            address,
                            pid: driver.pid(),
                            index,
                            uid,
                            player_vtable,
                            person_vtable,
                            attributes_verified: false,
                        };
                        if record.validate(driver).is_ok() {
                            let _ = record.verify_native_attributes(driver);
                            found.push(record);
                        }
                    }
                }
                start += len;
            }
        }
        match found.as_slice() {
            [record] => Ok(record.clone()),
            [] => Err(invalid(&format!("No native player record found for player {index}"))),
            other => Err(invalid(&format!("Expected one native player record for player {index}, found {}", other.len()))),
        }
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

    /// Squad number: u8 at contract + 0x5d (255 = none). Verified against the squad screen for
    /// 11 players and by an in-game write; the screen refreshes at the next day tick.
    pub fn squad_number(&self, driver: &LinuxMemoryDriver) -> Result<Option<u8>> {
        let contract = self.contract(driver)?;
        let value = driver.read::<u8>(contract.address + SQUAD_NUMBER)?;
        Ok((value != NO_SQUAD_NUMBER).then_some(value))
    }

    /// Finds the teammate (same contract team object) currently wearing `number`.
    fn squad_number_holder(&self, driver: &LinuxMemoryDriver, team: usize, number: u8) -> Result<Option<(Self, NativeContract)>> {
        let mut found = Vec::new();
        for map in parse_process_maps(driver.pid())?.iter().filter(|m| m.is_rw_private()) {
            let mut start = map.start;
            while start < map.end {
                let len = (map.end - start).min(16 << 20);
                let mut buf = vec![0u8; len];
                if driver.read_bytes(start, &mut buf).is_ok() {
                    for (i, chunk) in buf.chunks_exact(8).enumerate() {
                        if usize::from_le_bytes(chunk.try_into().unwrap()) != self.player_vtable { continue; }
                        let address = start + i * 8;
                        if address == self.address { continue; }
                        let Ok(contract) = driver.read_ptr(address + 0x1e8) else { continue };
                        if contract <= 0x10000
                            || driver.read_ptr(contract + 8).ok() != Some(address + PERSON)
                            || driver.read_ptr(contract + CONTRACT_TEAM).ok() != Some(team)
                            || driver.read::<u8>(contract + SQUAD_NUMBER).ok() != Some(number)
                        { continue; }
                        found.push(address);
                    }
                }
                start += len;
            }
        }
        match found.as_slice() {
            [] => Ok(None),
            [address] => {
                let holder = Self {
                    address: *address,
                    pid: self.pid,
                    index: driver.read::<i32>(address + INDEX)?,
                    uid: driver.read::<u32>(address + UID)?,
                    player_vtable: self.player_vtable,
                    person_vtable: driver.read_ptr(address + PERSON)?,
                    attributes_verified: false,
                };
                let contract = holder.contract(driver)?;
                Ok(Some((holder, contract)))
            }
            many => Err(invalid(&format!("{} teammates already wear number {number}; not written", many.len()))),
        }
    }

    /// Sets the squad number (`None` clears it). A teammate already wearing the number receives
    /// this player's old number in the same transaction. Returns the swapped teammate's index.
    pub fn write_squad_number(&self, driver: &LinuxMemoryDriver, number: Option<u8>) -> Result<Option<i32>> {
        if number.is_some_and(|n| !(1..=99).contains(&n)) {
            return Err(invalid("Squad number must be between 1 and 99"));
        }
        let contract = self.contract(driver)?;
        let team = driver.read_ptr(contract.address + CONTRACT_TEAM)?;
        if team <= 0x10000 { return Err(invalid("Player's contract has no team")); }
        let old = driver.read::<u8>(contract.address + SQUAD_NUMBER)?;
        let next = number.unwrap_or(NO_SQUAD_NUMBER);
        if old == next { return Ok(None); }
        let holder = match number {
            Some(n) => self.squad_number_holder(driver, team, n)?,
            None => None,
        };
        let mut edits = vec![(self, contract, old, next)];
        if let Some((record, holder_contract)) = &holder {
            edits.push((record, *holder_contract, next, old));
        }
        let mut written = 0;
        let result = (|| -> Result<()> {
            for (record, c, expected, value) in &edits {
                record.validate_contract(driver, *c)?;
                if driver.read::<u8>(c.address + SQUAD_NUMBER)? != *expected {
                    return Err(invalid("Squad number changed during Save; refresh and retry"));
                }
                driver.write(c.address + SQUAD_NUMBER, value)?;
                written += 1;
            }
            for (_, c, _, value) in &edits {
                if driver.read::<u8>(c.address + SQUAD_NUMBER)? != *value {
                    return Err(invalid("Squad number readback mismatch"));
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            let mut failures = Vec::new();
            for (_, c, expected, _) in edits[..written].iter().rev() {
                if driver.write(c.address + SQUAD_NUMBER, expected).is_err()
                    || driver.read::<u8>(c.address + SQUAD_NUMBER).ok() != Some(*expected)
                { failures.push(c.address); }
            }
            return Err(invalid(&format!("Squad number Save failed: {error}; rollback failures: {failures:x?}")));
        }
        Ok(holder.map(|(record, _)| record.index))
    }

    /// Tactical familiarity lives in a lazily allocated object at record - 0x48 holding 8 vectors:
    /// seven of u32 entries whose low 14 bits are the value (0..=10000), and one of 16-byte
    /// per-formation entries with a u16 value at +0xe. The game's own "maximise familiarity"
    /// action sets every value to 10000; this does the same. Verified in game (all bars Fluid).
    fn familiarity_edits(&self, driver: &LinuxMemoryDriver) -> Result<Vec<NativeEdit>> {
        self.validate(driver)?;
        let object = driver.read_ptr(self.address - FAMILIARITY)?;
        if object == 0 { return Ok(Vec::new()); } // never allocated: the game already treats it as default
        let maps = parse_process_maps(driver.pid())?;
        let writable = |p: usize, len: usize| maps.iter().any(|m| m.is_rw_private() && p >= m.start && p.checked_add(len).is_some_and(|e| e <= m.end));
        if object <= 0x10000 || !writable(object, 0x100) { return Err(invalid("Familiarity object failed memory validation")); }
        let mut edits = Vec::new();
        for k in 0..8usize {
            let (begin, end, cap) = (driver.read_ptr(object + k * 0x20)?, driver.read_ptr(object + k * 0x20 + 8)?, driver.read_ptr(object + k * 0x20 + 0x10)?);
            if begin == 0 && end == 0 { continue; }
            let size = end.wrapping_sub(begin);
            let stride = if k < 7 { 4 } else { 16 };
            if begin > end || end > cap || size > 0x4000 || size % stride != 0 || !writable(begin, size.max(1)) {
                return Err(invalid("Familiarity vector failed validation"));
            }
            for entry in (begin..end).step_by(stride) {
                if k < 7 {
                    let value = driver.read::<u32>(entry)?;
                    let next = (value & 0xffff_c000) | FAMILIARITY_MAX;
                    if next != value { edits.push(NativeEdit::capture(driver, entry, next.to_le_bytes().to_vec())?); }
                } else {
                    let value = driver.read::<u16>(entry + 0xe)?;
                    if u32::from(value) != FAMILIARITY_MAX { edits.push(NativeEdit::capture(driver, entry + 0xe, (FAMILIARITY_MAX as u16).to_le_bytes().to_vec())?); }
                }
            }
        }
        Ok(edits)
    }

    /// Sets every tactical familiarity value of this player to the maximum. Returns changed values.
    pub fn maximise_tactical_familiarity(&self, driver: &LinuxMemoryDriver) -> Result<usize> {
        let edits = self.familiarity_edits(driver)?;
        if !edits.is_empty() { self.apply_edits(driver, &edits, None)?; }
        Ok(edits.len())
    }

    /// Players sharing this player's contract team (the squad container), including this one.
    pub fn teammates(&self, driver: &LinuxMemoryDriver) -> Result<Vec<Self>> {
        let contract = self.contract(driver)?;
        let team = driver.read_ptr(contract.address + CONTRACT_TEAM)?;
        if team <= 0x10000 { return Err(invalid("Player's contract has no team")); }
        let mut found = Vec::new();
        for map in parse_process_maps(driver.pid())?.iter().filter(|m| m.is_rw_private()) {
            let mut start = map.start;
            while start < map.end {
                let len = (map.end - start).min(16 << 20);
                let mut buf = vec![0u8; len];
                if driver.read_bytes(start, &mut buf).is_ok() {
                    for (i, chunk) in buf.chunks_exact(8).enumerate() {
                        if usize::from_le_bytes(chunk.try_into().unwrap()) != self.player_vtable { continue; }
                        let address = start + i * 8;
                        let Ok(c) = driver.read_ptr(address + 0x1e8) else { continue };
                        if c <= 0x10000
                            || driver.read_ptr(c + 8).ok() != Some(address + PERSON)
                            || driver.read_ptr(c + CONTRACT_TEAM).ok() != Some(team)
                        { continue; }
                        let Ok(person_vtable) = driver.read_ptr(address + PERSON) else { continue };
                        let (Ok(index), Ok(uid)) = (driver.read::<i32>(address + INDEX), driver.read::<u32>(address + UID)) else { continue };
                        found.push(Self { address, pid: self.pid, index, uid, player_vtable: self.player_vtable, person_vtable, attributes_verified: false });
                    }
                }
                start += len;
            }
        }
        Ok(found)
    }

    pub fn attributes_verified(&self) -> bool { self.attributes_verified }

    pub fn index(&self) -> i32 { self.index }
    pub fn address(&self) -> usize { self.address }

    /// Native name records are pooled references to length-prefixed UTF-8 strings.
    /// These are read-only: editing pooled strings would rename other people too.
    pub fn name(&self, driver: &LinuxMemoryDriver) -> Result<String> {
        self.validate(driver)?;
        let mut names = Vec::new();
        for offset in [0x190, 0x198] {
            let reference = driver.read_ptr(self.address + offset)?;
            if reference == 0 { continue; }
            let string = driver.read_ptr(reference)?;
            if string == 0 { continue; }
            let length = driver.read::<u32>(string)? as usize;
            if length == 0 { continue; }
            if length > 256 { return Err(invalid("Native player name length is invalid")); }
            let mut bytes = vec![0u8; length];
            if driver.read_bytes(string + 4, &mut bytes)? != length { return Err(invalid("Short native name read")); }
            let name = String::from_utf8(bytes).map_err(|_| invalid("Native player name is not UTF-8"))?;
            if name.chars().any(char::is_control) { return Err(invalid("Native player name contains control characters")); }
            names.push(name);
        }
        if names.is_empty() { return Err(invalid("Native player has no readable name")); }
        Ok(names.join(" "))
    }

    /// Bootstrap the loaded database from a record independently validated against
    /// game bindings. Both native C++ vtables must match that record's type; each
    /// candidate still validates its own index, duplicate UID, positions and ratings.
    pub fn discover_loaded(&self, driver: &LinuxMemoryDriver) -> Result<Vec<Self>> {
        self.validate(driver)?;
        if !self.attributes_verified { return Err(invalid("Open one player's Attributes screen first to calibrate the native player layout")); }
        let pattern = self.player_vtable.to_le_bytes().iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ");
        let mut found = std::collections::HashMap::<i32, Option<Self>>::new();
        for address in driver.scan_aob(&pattern)? {
            let Ok(candidate) = self.peer_at(driver, address) else { continue; };
            found.entry(candidate.index).and_modify(|record| *record = None).or_insert(Some(candidate));
        }
        self.validate(driver)?;
        Ok(found.into_values().flatten().collect())
    }

    fn peer_at(&self, driver: &LinuxMemoryDriver, address: usize) -> Result<Self> {
        self.validate(driver)?;
        if !self.attributes_verified || address % 8 != 0 { return Err(invalid("Uncalibrated native player type or unaligned address")); }
        let index = driver.read::<i32>(address + INDEX)?;
        let uid = driver.read::<u32>(address + UID)?;
        if index <= 0 || uid == 0 { return Err(invalid("Invalid native player identity")); }
        let candidate = Self { address, index, uid, ..self.clone() };
        candidate.validate(driver)?;
        candidate.read_positions(driver)?;
        candidate.refresh_attributes(driver, &mut PlayerAttributes::default())?;
        candidate.name(driver)?;
        Ok(candidate)
    }

    /// Independently match visible attributes before accepting this layout,
    /// or verify native layout validity directly when observed bindings are empty/masked.
    pub fn verify_attributes(&mut self, driver: &LinuxMemoryDriver, observed: &PlayerAttributes) -> Result<()> {
        self.validate(driver)?;
        let has_observed = observed.reflexes > 0 || observed.passing > 0 || observed.tackling > 0;
        if !has_observed {
            return self.verify_native_attributes(driver);
        }
        let fields = if observed.reflexes > 0 && observed.handling > 0 {
            NATIVE_GOALKEEPER_ATTRIBUTES
        } else {
            NATIVE_OUTFIELD_ATTRIBUTES
        };
        for field in fields {
            let expected = (field.get)(observed);
            if !(1..=20).contains(&expected)
                || displayed_rating(driver.read::<u8>(self.address + field.offset)?)? != expected
            {
                return self.verify_native_attributes(driver);
            }
        }
        self.attributes_verified = true;
        Ok(())
    }

    /// Verifies the native attribute layout directly by ensuring all 60 native attributes
    /// decode within valid ranges (1..=20) and CA/PA are within 0..=200.
    pub fn verify_native_attributes(&mut self, driver: &LinuxMemoryDriver) -> Result<()> {
        self.validate(driver)?;
        for field in native_attributes() {
            let byte = driver.read::<u8>(self.address + field.offset)?;
            let rating = field.decode(byte)?;
            if !(1..=20).contains(&rating) {
                return Err(invalid(&format!("Native attribute {} out of range: {}", field.name, rating)));
            }
        }
        let ca = driver.read::<u16>(self.address + 0x11c)?;
        let pa = driver.read::<u16>(self.address + 0x11e)?;
        if ca > 200 || pa > 200 {
            return Err(invalid("Native CA or PA out of range"));
        }
        self.attributes_verified = true;
        Ok(())
    }

    pub fn refresh_attributes(&self, driver: &LinuxMemoryDriver, attributes: &mut PlayerAttributes) -> Result<()> {
        self.validate(driver)?;
        if !self.attributes_verified { return Err(invalid("Native attributes have not been verified for this player")); }
        // Read all before publishing, so an invalid byte never produces a partial update.
        let values: Vec<u8> = native_attributes().map(|field|
            field.decode(driver.read::<u8>(self.address + field.offset)?)).collect::<Result<_>>()?;
        for (field, value) in native_attributes().zip(values) { (field.set)(attributes, value); }
        Ok(())
    }

    fn contract(&self, driver: &LinuxMemoryDriver) -> Result<NativeContract> {
        self.validate(driver)?;
        let address = driver.read_ptr(self.address + 0x1e8)?;
        if address <= 0x10000 { return Err(invalid("Player has no native full contract")); }
        let vtable = driver.read_ptr(address)?;
        let maps = parse_process_maps(driver.pid())?;
        if !maps.iter().any(|m| m.is_rw_private() && address >= m.start && address.checked_add(0x60).is_some_and(|end| end <= m.end))
            || !maps.iter().any(|m| m.is_readable() && !m.is_writable() && vtable >= m.start && vtable.checked_add(8).is_some_and(|end| end <= m.end))
            || !maps.iter().any(|m| m.is_executable() && driver.read_ptr(vtable).is_ok_and(|p| p >= m.start && p < m.end))
        { return Err(invalid("Native contract memory/type validation failed")); }
        let contract = NativeContract { address, vtable };
        self.validate_contract(driver, contract)?;
        let expiry = driver.read::<u32>(address + 0x48)?;
        expiry_with_year(expiry, (expiry >> 16) as i32)?;
        Ok(contract)
    }

    fn validate_contract(&self, driver: &LinuxMemoryDriver, contract: NativeContract) -> Result<()> {
        self.validate(driver)?;
        if driver.read_ptr(self.address + 0x1e8)? != contract.address
            || driver.read_ptr(contract.address)? != contract.vtable
            || driver.read_ptr(contract.address + 8)? != self.address + PERSON
        { return Err(invalid("Native contract ownership changed")); }
        Ok(())
    }

    pub fn refresh_abilities(&self, driver: &LinuxMemoryDriver, player: &mut Player) -> Result<()> {
        self.validate(driver)?;
        if !self.attributes_verified { return Err(invalid("Native attributes have not been verified for this player")); }
        let ca = driver.read::<u16>(self.address + 0x11c)?;
        let pa = driver.read::<u16>(self.address + 0x11e)?;
        player.current_ability = if (1..=200).contains(&ca) { ca as u8 } else { 0 };
        player.potential_ability = if (1..=200).contains(&pa) { pa as u8 } else { 0 };
        Ok(())
    }

    /// Read-only preferred-moves bitset (record + 0x200). Only bits with screen evidence are
    /// named. Never written: the bit-to-move table is incomplete and unverified for writes.
    pub fn read_preferred_moves(&self, driver: &LinuxMemoryDriver) -> Result<PreferredMoves> {
        self.validate(driver)?;
        let raw = driver.read::<u64>(self.address + PPM_BITSET_OFFSET)?;
        Ok(PreferredMoves::decode(raw))
    }

    pub fn refresh_player_fields(&self, driver: &LinuxMemoryDriver, player: &mut Player) -> Result<()> {
        self.refresh_attributes(driver, &mut player.attributes)?;
        self.refresh_abilities(driver, player)?;
        player.preferred_moves = self.read_preferred_moves(driver).ok();
        if let Ok(contract) = self.contract(driver) {
            let wage = driver.read::<u32>(contract.address + 0x20)?;
            player.contract.wage_per_week = i32::try_from(wage).unwrap_or(0);
            player.contract.expiry_year = (driver.read::<u32>(contract.address + 0x48)? >> 16) as i32;
            self.refresh_contract_terms(driver, contract.address, &mut player.contract);
            let team = driver.read_ptr(contract.address + CONTRACT_TEAM).unwrap_or(0);
            if team > 0x10000 {
                let club = driver.read_ptr(team + 0x30).unwrap_or(0);
                if club > 0x10000 {
                    let club_id = driver.read::<i32>(club + 8).unwrap_or(0);
                    if club_id > 0 {
                        player.club_id = club_id;
                        if player.club_name.is_empty()
                            || player.club_name == "Unknown (live)"
                            || player.club_name == "Indonesia"
                            || player.club_name.starts_with("Club #")
                        {
                            if let Ok(c) = crate::native_club::NativeClubRecord::discover(driver, club_id) {
                                if let Some(name) = c.read_name(driver) {
                                    player.club_name = name;
                                }
                            }
                        }
                    }
                }
            }
        } else {
            player.contract.wage_per_week = 0;
            player.contract.expiry_year = 0;
            player.contract.squad_status = String::new();
            player.contract.squad_status_code = None;
            player.contract.release_clause = 0;
            player.contract.release_clause_present = None;
            player.contract.happiness = String::new();
            player.contract.terms_note = "no contract".to_string();
        }
        Ok(())
    }

    /// True while the player has a loan agreement: the loan node at `+0x1f0` points to a node whose first
    /// pointer is an agreement owned by this player (`+0x8` is the person sub-object). A non-null node with an
    /// empty first pointer (for example Bruno Iglesias, a reserve-team player) is not a loan. Unreadable state
    /// counts as on loan, so nothing is shown or edited on a guess.
    fn is_on_loan(&self, driver: &LinuxMemoryDriver) -> bool {
        let Ok(node) = driver.read_ptr(self.address + LOAN_NODE_OFFSET) else { return true };
        if node <= 0x10000 {
            return false;
        }
        let Ok(agreement) = driver.read_ptr(node) else { return true };
        if agreement <= 0x10000 {
            return false;
        }
        driver.read_ptr(agreement + 8).map(|owner| owner == self.address + PERSON).unwrap_or(true)
    }

    /// Fills squad status and the release clause from the native contract; anything not read stays empty/None
    /// (never the template defaults). Happiness is derived by the game from several factors and is never read.
    fn refresh_contract_terms(&self, driver: &LinuxMemoryDriver, contract: usize, out: &mut PlayerContract) {
        use crate::native_contract_terms::{read_terms, squad_status_label, ReleaseClause};
        out.happiness = String::new();
        out.squad_status = String::new();
        out.squad_status_code = None;
        out.release_clause = 0;
        out.release_clause_present = None;
        out.terms_note = String::new();
        // A loaned player's screens show the loan contract; the parent contract's terms would be misleading.
        if self.is_on_loan(driver) {
            out.terms_note = "on loan: the game shows the loan contract, not this one".to_string();
            return;
        }
        let Ok(terms) = read_terms(driver, contract) else {
            out.terms_note = "contract terms unreadable".to_string();
            return;
        };
        out.squad_status_code = Some(terms.squad_status_code);
        out.squad_status = squad_status_label(terms.squad_status_code).map(str::to_string)
            .unwrap_or_else(|| format!("Code {}", terms.squad_status_code));
        match terms.release_clause {
            ReleaseClause::Present { value, .. } => {
                out.release_clause = i64::from(value);
                out.release_clause_present = Some(true);
            }
            ReleaseClause::Absent => out.release_clause_present = Some(false),
            ReleaseClause::Unavailable(reason) => out.terms_note = format!("release clause {reason}"),
        }
    }

    /// Read-only diagnostic: the raw contract record bytes (`len` <= 0x200) and its address.
    pub fn contract_dump(&self, driver: &LinuxMemoryDriver, len: usize) -> Result<(usize, Vec<u8>)> {
        let contract = self.contract(driver)?;
        let mut bytes = vec![0u8; len.min(0x200)];
        driver.read_bytes(contract.address, &mut bytes)?;
        Ok((contract.address, bytes))
    }

    /// Moves the contract expiry `years` calendar years later (u32 at contract + 0x48, same date,
    /// leap-day clamped). Readback-verified with rollback. Returns (old year, new year).
    /// The contract found at +0x1e8 is the parent contract: a loaned-out player's loan is not
    /// stored here, so this extends the parent deal.
    pub fn extend_contract(&self, driver: &LinuxMemoryDriver, years: i32) -> Result<(i32, i32)> {
        if !(1..=10).contains(&years) {
            return Err(invalid("Extension must be between 1 and 10 years"));
        }
        let contract = self.contract(driver)?;
        let old = driver.read::<u32>(contract.address + 0x48)?;
        let old_year = (old >> 16) as i32;
        let next = expiry_with_year(old, old_year + years)?;
        let edit = NativeEdit::capture(driver, contract.address + 0x48, next.to_le_bytes().to_vec())?;
        self.apply_edits(driver, &[edit], Some(contract))?;
        Ok((old_year, (next >> 16) as i32))
    }

    pub fn has_contract(&self, driver: &LinuxMemoryDriver) -> bool { self.contract(driver).is_ok() }

    /// Apply changed positions/attributes in one checked transaction. Keep exact old
    /// bytes for rollback, including fractional (e.g. 53/100) native attributes.
    pub fn write_player_edits(&self, driver: &LinuxMemoryDriver, previous: &Player, requested: &Player) -> Result<()> {
        self.validate(driver)?;
        let mut edits = Vec::new();
        for field in native_attributes() {
            if (field.get)(&previous.attributes) != (field.get)(&requested.attributes) {
                if !self.attributes_verified { return Err(invalid("Open this player's Attributes screen in FM26 to verify the native attribute layout")); }
                let rating = (field.get)(&requested.attributes);
                if !(1..=20).contains(&rating) { return Err(invalid(&format!("{} must be between 1 and 20", field.name))); }
                let old = driver.read::<u8>(self.address + field.offset)?;
                field.decode(old)?;
                edits.push((field.offset, old, rating * field.scale));
            }
        }
        macro_rules! position {
            ($field:ident, $offset:expr, $scale:expr) => {
                if previous.positions.$field != requested.positions.$field {
                    let value = requested.positions.$field;
                    if !(1..=20).contains(&value) { return Err(invalid(concat!(stringify!($field), " must be between 1 and 20"))); }
                    let old = driver.read::<u8>(self.address + $offset)?;
                    edits.push(($offset, old, value * $scale));
                }
            };
        }
        // SW is absent from the model and never touched.
        position!(gk, 8, 1); position!(dl, 10, 1); position!(dc, 11, 1);
        position!(dr, 12, 1); position!(dm, 13, 1); position!(ml, 14, 1);
        position!(mc, 15, 1); position!(mr, 16, 1); position!(aml, 17, 1);
        position!(amc, 18, 1); position!(amr, 19, 1); position!(st, 20, 1);
        position!(wbl, 21, 1); position!(wbr, 22, 1);
        position!(left_foot, LEFT_FOOT, 5); position!(right_foot, RIGHT_FOOT, 5);
        let mut patches: Vec<NativeEdit> = edits.into_iter().map(|(offset, old, next)| NativeEdit {
            address: self.address + offset, old: vec![old], next: vec![next],
        }).collect();
        for (before, next, offset, name) in [
            (previous.current_ability, requested.current_ability, 0x11c, "CA"),
            (previous.potential_ability, requested.potential_ability, 0x11e, "PA"),
        ] {
            if before != next {
                if !self.attributes_verified || !(1..=200).contains(&next)
                    || !(1..=200).contains(&driver.read::<u16>(self.address + offset)?)
                { return Err(invalid(&format!("{name} requires a verified native player and values between 1 and 200"))); }
                patches.push(NativeEdit::capture(driver, self.address + offset, (next as u16).to_le_bytes().to_vec())?);
            }
        }
        let terms_changed = previous.contract.squad_status != requested.contract.squad_status
            || previous.contract.release_clause != requested.contract.release_clause;
        let contract = if previous.contract.wage_per_week != requested.contract.wage_per_week
            || previous.contract.expiry_year != requested.contract.expiry_year
            || terms_changed
        {
            if !self.attributes_verified { return Err(invalid("Native player layout has not been calibrated")); }
            let contract = self.contract(driver)?;
            if previous.contract.wage_per_week != requested.contract.wage_per_week {
                let wage = u32::try_from(requested.contract.wage_per_week).map_err(|_| invalid("Weekly wage cannot be negative"))?;
                patches.push(NativeEdit::capture(driver, contract.address + 0x20, wage.to_le_bytes().to_vec())?);
            }
            if previous.contract.expiry_year != requested.contract.expiry_year {
                let old = driver.read::<u32>(contract.address + 0x48)?;
                let next = expiry_with_year(old, requested.contract.expiry_year)?;
                patches.push(NativeEdit::capture(driver, contract.address + 0x48, next.to_le_bytes().to_vec())?);
            }
            if terms_changed && self.is_on_loan(driver) {
                return Err(invalid("This player is on loan (or the loan state is unreadable); the game shows the loan contract, so the parent contract's terms are not edited"));
            }
            if previous.contract.squad_status != requested.contract.squad_status {
                let code = crate::native_contract_terms::squad_status_code(&requested.contract.squad_status)
                    .ok_or_else(|| invalid("Squad status must be Star Player, Important Player, Regular Starter, Squad Player or Fringe Player"))?;
                for (address, bytes) in crate::native_contract_terms::squad_status_edits(driver, contract.address, code)? {
                    patches.push(NativeEdit::capture(driver, address, bytes)?);
                }
            }
            if previous.contract.release_clause != requested.contract.release_clause {
                let (address, bytes) = crate::native_contract_terms::release_clause_edit(driver, contract.address, requested.contract.release_clause)?;
                patches.push(NativeEdit::capture(driver, address, bytes)?);
            }
            Some(contract)
        } else { None };
        self.apply_edits(driver, &patches, contract)
    }

    /// Restore condition and sharpness to 100% without corrupting external simulation objects.
    pub fn heal(&self, driver: &LinuxMemoryDriver) -> Result<()> {
        self.validate(driver)?;
        let mut patches = Vec::new();

        let c0 = driver.read::<i32>(self.address + 0xc0)?;
        if c0 < 10000 {
            patches.push(NativeEdit::capture(driver, self.address + 0xc0, 10000i32.to_le_bytes().to_vec())?);
        }
        let f0 = driver.read::<i32>(self.address + 0x100)?;
        if f0 < 10000 {
            patches.push(NativeEdit::capture(driver, self.address + 0x100, 10000i32.to_le_bytes().to_vec())?);
        }

        if !patches.is_empty() {
            self.apply_edits(driver, &patches, None)?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn apply_byte_edits(&self, driver: &LinuxMemoryDriver, edits: &[(usize, u8, u8)]) -> Result<()> {
        let edits: Vec<_> = edits.iter().map(|&(offset, old, next)| NativeEdit {
            address: self.address + offset, old: vec![old], next: vec![next],
        }).collect();
        self.apply_edits(driver, &edits, None)
    }

    fn apply_edits(&self, driver: &LinuxMemoryDriver, edits: &[NativeEdit], contract: Option<NativeContract>) -> Result<()> {
        self.validate(driver)?;
        let mut attempted = 0;
        let result = (|| -> Result<()> {
            for edit in edits {
                self.validate(driver)?;
                if let Some(contract) = contract { self.validate_contract(driver, contract)?; }
                let mut current = vec![0; edit.old.len()];
                if driver.read_bytes(edit.address, &mut current)? != current.len() || current != edit.old { return Err(invalid("Player data changed during Save; refresh and retry")); }
                attempted += 1;
                if driver.write_bytes(edit.address, &edit.next)? != edit.next.len() { return Err(invalid("Short native player write")); }
            }
            self.validate(driver)?;
            if let Some(contract) = contract { self.validate_contract(driver, contract)?; }
            for edit in edits {
                let mut current = vec![0; edit.next.len()];
                if driver.read_bytes(edit.address, &mut current)? != current.len() || current != edit.next { return Err(invalid("Native player Save readback mismatch")); }
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.validate(driver).map_err(|_| invalid("Save failed and player identity changed; rollback skipped"))?;
            if let Some(contract) = contract { self.validate_contract(driver, contract).map_err(|_| invalid("Save failed and contract ownership changed; rollback skipped"))?; }
            let rollback = (|| -> Result<()> {
                let mut failures = Vec::new();
                for edit in edits[..attempted].iter().rev() {
                    if !driver.write_bytes(edit.address, &edit.old).is_ok_and(|n| n == edit.old.len()) { failures.push(edit.address); }
                }
                for edit in &edits[..attempted] {
                    let mut current = vec![0; edit.old.len()];
                    if !driver.read_bytes(edit.address, &mut current).is_ok_and(|n| n == current.len()) || current != edit.old { failures.push(edit.address); }
                }
                if !failures.is_empty() { return Err(invalid(&format!("Player Save rollback failed at {failures:x?}"))); }
                Ok(())
            })();
            return Err(invalid(&format!("Player Save failed: {error}; rollback: {rollback:?}")));
        }
        Ok(())
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

    /// Terminates the player's contract, detaching all team/contract/loan links and
    /// removing the player from team squad vectors.
    pub fn terminate_contract(&self, driver: &LinuxMemoryDriver) -> Result<Vec<usize>> {
        self.validate(driver)?;
        crate::native_transfer::terminate_contract(driver, self)
    }

    /// Terminates an active loan, returning the player to their parent team.
    pub fn terminate_loan(&self, driver: &LinuxMemoryDriver) -> Result<(usize, usize)> {
        self.validate(driver)?;
        crate::native_transfer::terminate_loan(driver, self)
    }

    /// Moves a contracted player to a destination team squad vector.
    pub fn move_player(&self, driver: &LinuxMemoryDriver, target_team: usize, fee_gbp: Option<u32>) -> Result<(usize, usize)> {
        self.validate(driver)?;
        crate::native_transfer::move_player(driver, self, target_team, fee_gbp)
    }

    /// Signs an unemployed free agent to a target team.
    pub fn sign_free_agent(
        &self,
        driver: &LinuxMemoryDriver,
        target_team: usize,
        wage_gbp: Option<u32>,
        expiry_year: Option<u32>,
    ) -> Result<usize> {
        self.validate(driver)?;
        crate::native_transfer::sign_free_agent(driver, self, target_team, wage_gbp, expiry_year)
    }

    /// Completes an arranged future transfer immediately.
    pub fn complete_pending_transfer(&self, driver: &LinuxMemoryDriver) -> Result<(usize, usize)> {
        self.validate(driver)?;
        crate::native_transfer::complete_pending_transfer(driver, self)
    }

    /// Accelerates the scheduled join date of a pending future transfer.
    pub fn accelerate_pending_transfer(
        &self,
        driver: &LinuxMemoryDriver,
        new_join_date: Option<u32>,
    ) -> Result<(u32, u32)> {
        self.validate(driver)?;
        crate::native_transfer::accelerate_pending_transfer(driver, self, new_join_date)
    }
}

#[cfg(test)]
#[path = "native_player_tests.rs"]
mod tests;
