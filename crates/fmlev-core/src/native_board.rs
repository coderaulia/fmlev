//! Club board objectives ("club vision") and the club status byte (read verified in shape, writes EXPERIMENTAL).
//!
//! Layout found live (PID 98701, R. Madrid club 1089 and a survey of all 24,378 clubs; see AGENTS.md):
//!
//! ```text
//! club + 0x18            vector begin -> first TEAM (.?AVTEAM@db@@, team + 0x30 == club)
//! team + 0xa0            link object (no RTTI)
//! link + 0xc8            manager details: .?AVCLUB_HUMAN_MANAGER_DETAILS@@ (user club)
//!                        or .?AVCLUB_MANAGER_DETAILS@@ (AI clubs with a vision); details + 0x40 == team
//! details + 0x78         vector object { begin, end, cap } of pointers to objective objects
//! objective              .?AVCLUB_VISION_OBJECTIVE@db@@ or .?AVCLUB_COMPETITION_VISION_OBJECTIVE@db@@
//!   +0x08 i16 target id (-1 none)   +0x0b u8 target kind (0xff none; 3 club, 9 nation, 0x19 competition: heuristic)
//!   +0x20 u32 start date            +0x24 u32 deadline (0x076c0001 = none)
//!   +0x28 i32 value (-1 none)       +0x2c u8 type code      +0x2d u8 importance 1..10
//!   +0x38 u8 status code (1 or 2, meaning NOT mapped)
//! club + 0x16a           u8 club status: 1 professional, 2 semi-professional, 3 amateur (inferred)
//! ```
//!
//! Type labels come from a community catalog and agree with the live target kinds (type 184 targets
//! club 1074, type 60 targets nation 170), but they were not compared with the game's own board
//! screen: they are labelled `catalog_unverified`. Nothing here changes a vector, pointer or object
//! count; writes are single scalar fields through `apply_transaction` (readback, rollback).

use fmlev_mem::{parse_process_maps, LinuxMemoryDriver};
use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::native_club::NativeClubRecord;
use crate::native_transfer::{apply_transaction, NativeEdit};

const CLUB_VECTOR: usize = 0x18;
const TEAM_CLUB: usize = 0x30;
const TEAM_LINK: usize = 0xa0;
const LINK_DETAILS: usize = 0xc8;
const DETAILS_TEAM: usize = 0x40;
const DETAILS_OBJECTIVES: usize = 0x78;
const MAX_OBJECTIVES: usize = 64;
const OBJECT_SIZE: usize = 0x50;

const O_TARGET_ID: usize = 0x08;
const O_TARGET_KIND: usize = 0x0b;
const O_START: usize = 0x20;
const O_DEADLINE: usize = 0x24;
const O_VALUE: usize = 0x28;
const O_TYPE: usize = 0x2c;
const O_IMPORTANCE: usize = 0x2d;
const O_STATUS: usize = 0x38;
const CLUB_STATUS_OFFSET: usize = 0x16a;

/// Date word with year 1900, day 1: the game's "no date".
const EMPTY_DATE: u32 = 0x076c_0001;

const TEAM_TYPE: &str = ".?AVTEAM@db@@";
const HUMAN_DETAILS_TYPE: &str = ".?AVCLUB_HUMAN_MANAGER_DETAILS@@";
const AI_DETAILS_TYPE: &str = ".?AVCLUB_MANAGER_DETAILS@@";
const VISION_TYPE: &str = ".?AVCLUB_VISION_OBJECTIVE@db@@";
const COMP_VISION_TYPE: &str = ".?AVCLUB_COMPETITION_VISION_OBJECTIVE@db@@";

const UNVERIFIED_LABEL: &str = "catalog_unverified";

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidValue(message.to_owned())
}

/// Type name of the object at `address` (`.?AV...` without the locator suffix), by RTTI.
pub type RttiResolver<'a> = &'a dyn Fn(usize) -> Option<String>;

/// RTTI resolver for the live game: object vtable -> complete object locator -> type descriptor in game_plugin.dll.
pub fn live_rtti(driver: &LinuxMemoryDriver) -> Result<impl Fn(usize) -> Option<String> + '_> {
    let maps = parse_process_maps(driver.pid())?;
    let base = maps
        .iter()
        .filter(|m| m.pathname.ends_with("game_plugin.dll"))
        .map(|m| m.start)
        .min()
        .ok_or_else(|| invalid("game_plugin.dll is not mapped; attach to FM26 with a save loaded first"))?;
    Ok(move |object: usize| {
        let vtable = driver.read_ptr(object).ok()?;
        if vtable <= base {
            return None;
        }
        let locator = driver.read_ptr(vtable - 8).ok()?;
        let col = driver.read::<[u32; 6]>(locator).ok()?;
        if !crate::native_catalog::is_complete_object_locator(&col, locator, base) || col[1] != 0 {
            return None;
        }
        let mut raw = [0u8; 128];
        driver.read_bytes(base + col[3] as usize + 16, &mut raw).ok()?;
        let end = raw.iter().position(|&b| b == 0)?;
        let name = std::str::from_utf8(&raw[..end]).ok()?;
        (name.starts_with(".?A") && name.bytes().all(|b| (32..=126).contains(&b))).then(|| name.to_owned())
    })
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct GameDate {
    pub year: u16,
    /// 1-based day of the year (low 9 bits of the date word).
    pub day: u16,
    pub iso: String,
    pub raw: String,
}

fn is_leap(year: u16) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_year(year: u16) -> u16 {
    if is_leap(year) { 366 } else { 365 }
}

fn iso_date(year: u16, day: u16) -> String {
    let months = [31u16, if is_leap(year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let (mut month, mut left) = (1u16, day.max(1));
    for length in months {
        if left <= length {
            break;
        }
        left -= length;
        month += 1;
    }
    format!("{year:04}-{month:02}-{left:02}")
}

/// Decodes a date word `(year << 16) | flags | day`; the empty marker and implausible years give `None`.
pub fn decode_date(word: u32) -> Option<GameDate> {
    let (year, day) = ((word >> 16) as u16, (word & 0x1ff) as u16);
    if word == EMPTY_DATE || !(1990..=2300).contains(&year) || day == 0 || day > days_in_year(year) {
        return None;
    }
    Some(GameDate { year, day, iso: iso_date(year, day), raw: format!("0x{word:08x}") })
}

/// Same word with a new year and day; the flag bits (0xfe00) keep their current value.
pub fn with_date(old: u32, year: u16, day: u16) -> Result<u32> {
    if decode_date(old).is_none() {
        return Err(invalid("The objective has no deadline to change (its date is empty or unreadable)"));
    }
    if !(2000..=2200).contains(&year) || day == 0 || day > days_in_year(year) {
        return Err(invalid("Deadline year must be 2000..=2200 and the day a valid day of that year"));
    }
    Ok((u32::from(year) << 16) | (old & 0xfe00) | u32::from(day))
}

/// Objective type label from the community catalog (NOT compared with the game's board screen).
pub fn type_label(code: u8) -> Option<&'static str> {
    Some(match code {
        1 => "Win competition",
        2 => "Qualify for competition",
        3 => "Gain promotion",
        4 => "Become recognised as best of the rest",
        5 => "Title challenge",
        6 => "Reach playoffs",
        7 => "Record a top-half finish",
        8 => "Establish in league",
        9 => "Reach final of cup",
        10 => "Reach later stages of cup",
        11 => "Reach knockout stages of cup",
        12 => "Reach proper stages of cup",
        13 => "Win silverware",
        14 => "Challenge for silverware",
        15 => "Remain in the competition",
        16 => "Gain automatic promotion",
        27 => "Challenge for promotion to the competition",
        28 => "Attempt to avoid finishing bottom",
        29 => "Attempt to avoid relegation",
        30 => "Reach competition",
        31 | 33 => "Be competitive in the competition",
        32 => "Finish in mid-table",
        34 => "Qualify from the competition",
        35 => "Challenge for promotion from the competition",
        36 => "Avoid relegation",
        37 => "Avoid finishing bottom",
        38 => "Finish in the competition's top places",
        40 => "Become self-sustainable",
        41 => "Repair the club's financial damage",
        42 => "Increase commercial revenue",
        43 | 45 => "Sign young players to develop for profit",
        44 => "Work within wage budget",
        46 => "Spend the original transfer budget",
        47 => "Reduce the wage budget",
        60 => "Become the most reputable team in the nation",
        61 => "Become the most reputable team in the continent",
        62 => "Become the most reputable team in the world",
        63 => "Grow the club's global reputation",
        64 => "Grow the club's reputation",
        80 => "Break even in the transfer market",
        81 => "Maximum one-year contracts for players over age X",
        82 => "Maximum two-year contracts for players over age X",
        83 => "Maximum three-year contracts for players over age X",
        84 => "Minimum X-year contracts for first-team players",
        85 => "Sign players to sell for profit",
        86 => "Cannot sign non-Basque players",
        94 => "Recoup funds through player sales",
        95 => "Negative transfer budget business model",
        96 => "Maintain FFP compliance",
        99 => "Match all transfer objectives",
        100 => "Sign players of a nationality",
        101 => "Sign players based in the nation",
        102 => "Sign players from the lower levels of the domestic game",
        103 => "Sign high reputation players",
        104 => "Sign players under age X for the first team",
        105 => "Sign players under age X for the future",
        106 => "Sign players from domestic rivals",
        107 => "Do not sign players over age X",
        120 => "Develop players using the club's youth system",
        121 => "Develop the best youth system in the country",
        122 => "Develop the division's best youth system",
        123 => "Develop the best youth system in the world",
        124 => "Develop players for the main club",
        125 => "Give playing time to first-team players",
        150 => "Play attacking football",
        151 => "Play defensively solid football",
        152 => "Play possession football",
        153 => "Play direct football",
        154 => "Make the most of set pieces",
        155 => "Play entertaining football",
        156 => "Play counter-attacking football",
        157 => "Play high-tempo pressing football",
        170 => "Build a new stadium",
        171 => "Expand stadium",
        172 => "Buy current stadium",
        180 => "Get the better of our rivals",
        181 => "Get the better of our rivals if we play them",
        182 => "Be competitive against a club",
        183 => "Be competitive against a club if we play them",
        184 => "Finish above a club in the league",
        185 => "Forge a higher reputation than a club",
        _ => return None,
    })
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct BoardObjective {
    pub index: usize,
    pub address: String,
    pub class: &'static str,
    pub type_code: u8,
    pub type_label: Option<&'static str>,
    pub label_status: &'static str,
    pub target_id: Option<i16>,
    pub target_kind_code: Option<u8>,
    pub value: Option<i32>,
    pub importance: Option<u8>,
    pub status_code: u8,
    pub status_note: &'static str,
    pub start: Option<GameDate>,
    pub deadline: Option<GameDate>,
    /// Why a field is missing or was not trusted (never a substituted default).
    pub notes: Vec<String>,
}

/// Decodes the fixed part of an objective from `raw` (the first 0x50 bytes).
pub fn decode_objective(index: usize, address: usize, class: &'static str, raw: &[u8]) -> Result<BoardObjective> {
    if raw.len() < OBJECT_SIZE {
        return Err(invalid("Objective object is shorter than expected"));
    }
    let u32_at = |at: usize| u32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
    let mut notes = Vec::new();
    let target = i16::from_le_bytes([raw[O_TARGET_ID], raw[O_TARGET_ID + 1]]);
    let kind = raw[O_TARGET_KIND];
    let value = u32_at(O_VALUE) as i32;
    let importance = raw[O_IMPORTANCE];
    let importance = if (1..=10).contains(&importance) {
        Some(importance)
    } else {
        notes.push(format!("importance byte {importance} is outside 1..=10"));
        None
    };
    let date = |at: usize, name: &str, notes: &mut Vec<String>| {
        let word = u32_at(at);
        let parsed = decode_date(word);
        if parsed.is_none() && word != EMPTY_DATE {
            notes.push(format!("{name} word 0x{word:08x} is not a plausible date"));
        }
        parsed
    };
    let start = date(O_START, "start", &mut notes);
    let deadline = date(O_DEADLINE, "deadline", &mut notes);
    let type_code = raw[O_TYPE];
    Ok(BoardObjective {
        index,
        address: format!("0x{address:x}"),
        class,
        type_code,
        type_label: type_label(type_code),
        label_status: UNVERIFIED_LABEL,
        target_id: (target != -1).then_some(target),
        target_kind_code: (kind != 0xff).then_some(kind),
        value: (value != -1).then_some(value),
        importance,
        status_code: raw[O_STATUS],
        status_note: "unmapped code (1 and 2 seen); meaning unknown",
        start,
        deadline,
        notes,
    })
}

/// Located objective container of one club.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub struct BoardChain {
    pub team: usize,
    pub details: usize,
    pub vector_object: usize,
    pub begin: usize,
    pub end: usize,
    pub capacity: usize,
    pub human_manager_details: bool,
}

impl BoardChain {
    pub fn count(&self) -> usize {
        (self.end - self.begin) / 8
    }
}

/// Walks `club -> team -> link -> manager details -> objective vector`, checking RTTI and back-pointers on every hop.
pub fn locate_chain(driver: &LinuxMemoryDriver, club: usize, rtti: RttiResolver) -> Result<BoardChain> {
    let step = |what: &str, value: std::result::Result<usize, fmlev_mem::MemoryError>| -> Result<usize> {
        let p = value.map_err(|_| invalid(&format!("Board chain unreadable at {what}")))?;
        if p <= 0x10000 || p % 8 != 0 {
            return Err(invalid(&format!("Board chain: {what} is not a valid pointer")));
        }
        Ok(p)
    };
    let begin = step("club team vector", driver.read_ptr(club + CLUB_VECTOR))?;
    let end = driver.read_ptr(club + CLUB_VECTOR + 8)?;
    if end <= begin {
        return Err(invalid("Board chain: the club has no team"));
    }
    let team = step("first team", driver.read_ptr(begin))?;
    if rtti(team).as_deref() != Some(TEAM_TYPE) || driver.read_ptr(team + TEAM_CLUB).ok() != Some(club) {
        return Err(invalid("Board chain: first team is not a TEAM that points back at the club"));
    }
    let link = step("team link", driver.read_ptr(team + TEAM_LINK))?;
    let details = step("manager details", driver.read_ptr(link + LINK_DETAILS))?;
    let human = match rtti(details).as_deref() {
        Some(HUMAN_DETAILS_TYPE) => true,
        Some(AI_DETAILS_TYPE) => false,
        other => {
            return Err(invalid(&format!(
                "This club has no board objective data of the club-level kind (manager details are {})",
                other.unwrap_or("not a database object")
            )))
        }
    };
    if driver.read_ptr(details + DETAILS_TEAM).ok() != Some(team) {
        return Err(invalid("Board chain: manager details do not point back at the team"));
    }
    let vector_object = step("objective vector", driver.read_ptr(details + DETAILS_OBJECTIVES))?;
    let (vbegin, vend, vcap) = (
        driver.read_ptr(vector_object)?,
        driver.read_ptr(vector_object + 8)?,
        driver.read_ptr(vector_object + 16)?,
    );
    if vbegin <= 0x10000 || vbegin % 8 != 0 || vend < vbegin || vcap < vend || (vend - vbegin) % 8 != 0 {
        return Err(invalid("Board chain: objective vector bounds are not plausible"));
    }
    if (vend - vbegin) / 8 > MAX_OBJECTIVES {
        return Err(invalid("Board chain: objective vector is implausibly long"));
    }
    Ok(BoardChain { team, details, vector_object, begin: vbegin, end: vend, capacity: vcap, human_manager_details: human })
}

fn objective_class(rtti: RttiResolver, address: usize) -> Option<&'static str> {
    match rtti(address).as_deref() {
        Some(VISION_TYPE) => Some("club_vision"),
        Some(COMP_VISION_TYPE) => Some("competition_vision"),
        _ => None,
    }
}

fn read_objective_at(driver: &LinuxMemoryDriver, rtti: RttiResolver, index: usize, address: usize) -> Result<BoardObjective> {
    if address <= 0x10000 || address % 8 != 0 {
        return Err(invalid(&format!("Objective {index} pointer is not valid")));
    }
    let class = objective_class(rtti, address)
        .ok_or_else(|| invalid(&format!("Objective {index} is not a club vision objective object")))?;
    let mut raw = vec![0u8; OBJECT_SIZE];
    if driver.read_bytes(address, &mut raw)? != OBJECT_SIZE {
        return Err(invalid(&format!("Objective {index} is not fully readable")));
    }
    decode_objective(index, address, class, &raw)
}

#[derive(Clone, Debug, Serialize)]
pub struct BoardReport {
    pub club_id: i32,
    pub chain: BoardChain,
    pub objectives: Vec<BoardObjective>,
    pub evidence: &'static str,
}

pub fn read_board(driver: &LinuxMemoryDriver, club: &NativeClubRecord) -> Result<BoardReport> {
    club.validate(driver)?;
    let rtti = live_rtti(driver)?;
    read_board_with(driver, club.club, club.club_id, &rtti)
}

pub fn read_board_with(driver: &LinuxMemoryDriver, club: usize, club_id: i32, rtti: RttiResolver) -> Result<BoardReport> {
    let chain = locate_chain(driver, club, rtti)?;
    let objectives = (0..chain.count())
        .map(|i| {
            let address = driver.read_ptr(chain.begin + i * 8)?;
            read_objective_at(driver, rtti, i, address)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(BoardReport {
        club_id,
        chain,
        objectives,
        evidence: "container shape, RTTI and fields read on a live save; type labels from a community catalog and the status code are not compared with the game's board screen",
    })
}

/// Requested scalar changes for one objective. At least one field must be set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ObjectiveEdit {
    pub importance: Option<u8>,
    pub value: Option<i32>,
    /// `(year, day of year)`.
    pub deadline: Option<(u16, u16)>,
    pub status: Option<u8>,
}

/// Builds the single-field edits for the objective at `address` from its current bytes. Nothing is written.
pub fn plan_objective_edits(driver: &LinuxMemoryDriver, address: usize, edit: &ObjectiveEdit) -> Result<Vec<NativeEdit>> {
    if edit == &ObjectiveEdit::default() {
        return Err(invalid("No objective field to change"));
    }
    let mut raw = vec![0u8; OBJECT_SIZE];
    if driver.read_bytes(address, &mut raw)? != OBJECT_SIZE {
        return Err(invalid("Objective is not fully readable"));
    }
    let mut edits = Vec::new();
    let mut push = |at: usize, old: &[u8], next: Vec<u8>, label: &'static str| {
        edits.push(NativeEdit::new(address + at, old.to_vec(), next, label));
    };
    if let Some(importance) = edit.importance {
        if !(1..=10).contains(&importance) {
            return Err(invalid("Importance must be 1..=10"));
        }
        if !(1..=10).contains(&raw[O_IMPORTANCE]) {
            return Err(invalid("The current importance is not readable, refusing to change it"));
        }
        push(O_IMPORTANCE, &raw[O_IMPORTANCE..=O_IMPORTANCE], vec![importance], "objective importance");
    }
    if let Some(value) = edit.value {
        if value < 0 {
            return Err(invalid("Target value must be 0 or more"));
        }
        let current = i32::from_le_bytes(raw[O_VALUE..O_VALUE + 4].try_into().unwrap());
        if current == -1 {
            return Err(invalid("This objective type has no target value (-1), refusing to add one"));
        }
        push(O_VALUE, &raw[O_VALUE..O_VALUE + 4], value.to_le_bytes().to_vec(), "objective value");
    }
    if let Some((year, day)) = edit.deadline {
        let old = u32::from_le_bytes(raw[O_DEADLINE..O_DEADLINE + 4].try_into().unwrap());
        let next = with_date(old, year, day)?;
        let start = u32::from_le_bytes(raw[O_START..O_START + 4].try_into().unwrap());
        if let Some(start) = decode_date(start) {
            if (year, day) < (start.year, start.day) {
                return Err(invalid("The deadline cannot be before the objective's start date"));
            }
        }
        push(O_DEADLINE, &raw[O_DEADLINE..O_DEADLINE + 4], next.to_le_bytes().to_vec(), "objective deadline");
    }
    if let Some(status) = edit.status {
        if !matches!(status, 1 | 2) || !matches!(raw[O_STATUS], 1 | 2) {
            return Err(invalid("Status code must be 1 or 2 and the current code must be 1 or 2 (the only codes seen; meaning unmapped)"));
        }
        push(O_STATUS, &raw[O_STATUS..=O_STATUS], vec![status], "objective status");
    }
    Ok(edits)
}

/// Experimental scalar write: `objective_index` must carry the `expected_type` code, and the object is
/// re-validated by RTTI inside the live chain before and after the transaction.
pub fn write_objective(
    driver: &LinuxMemoryDriver,
    club: &NativeClubRecord,
    objective_index: usize,
    expected_type: u8,
    edit: &ObjectiveEdit,
) -> Result<BoardObjective> {
    club.validate(driver)?;
    let rtti = live_rtti(driver)?;
    write_objective_with(driver, club.club, objective_index, expected_type, edit, &rtti)
}

pub fn write_objective_with(
    driver: &LinuxMemoryDriver,
    club: usize,
    objective_index: usize,
    expected_type: u8,
    edit: &ObjectiveEdit,
    rtti: RttiResolver,
) -> Result<BoardObjective> {
    let chain = locate_chain(driver, club, rtti)?;
    if objective_index >= chain.count() {
        return Err(invalid("Objective index is out of range; reload the board"));
    }
    let address = driver.read_ptr(chain.begin + objective_index * 8)?;
    let current = read_objective_at(driver, rtti, objective_index, address)?;
    if current.type_code != expected_type {
        return Err(invalid("The objective at that position has a different type now; reload the board"));
    }
    let edits = plan_objective_edits(driver, address, edit)?;
    apply_transaction(driver, &edits)?;
    read_objective_at(driver, rtti, objective_index, address)
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ClubStatusReport {
    pub club_id: i32,
    pub code: Option<u8>,
    pub label: Option<&'static str>,
    pub label_status: &'static str,
    pub reason: Option<String>,
}

pub fn status_label(code: u8) -> Option<&'static str> {
    match code {
        1 => Some("Professional"),
        2 => Some("Semi-professional"),
        3 => Some("Amateur"),
        _ => None,
    }
}

/// Status byte of the club record; codes outside 1..=3 (special clubs such as All-Stars or Bosman pools) are
/// reported as unavailable with a reason, never mapped.
pub fn read_club_status(driver: &LinuxMemoryDriver, club: &NativeClubRecord) -> Result<ClubStatusReport> {
    club.validate(driver)?;
    let code = driver.read::<u8>(club.club + CLUB_STATUS_OFFSET)?;
    let label = status_label(code);
    Ok(ClubStatusReport {
        club_id: club.club_id,
        code: label.map(|_| code),
        label,
        label_status: "inferred_from_reputation_distribution",
        reason: label.is_none().then(|| format!("status byte {code} is not one of 1..=3 (special club type); not editable")),
    })
}

/// Experimental: sets the status byte to 1..=3 when it currently holds 1..=3 (single byte, readback, rollback).
pub fn write_club_status(driver: &LinuxMemoryDriver, club: &NativeClubRecord, value: u8) -> Result<ClubStatusReport> {
    club.validate(driver)?;
    write_status_at(driver, club.club + CLUB_STATUS_OFFSET, value)?;
    read_club_status(driver, club)
}

fn write_status_at(driver: &LinuxMemoryDriver, address: usize, value: u8) -> Result<()> {
    if status_label(value).is_none() {
        return Err(invalid("Club status must be 1 (professional), 2 (semi-professional) or 3 (amateur)"));
    }
    let old = driver.read::<u8>(address)?;
    if status_label(old).is_none() {
        return Err(invalid("The current club status byte is not 1..=3; refusing to change this club"));
    }
    apply_transaction(driver, &[NativeEdit::new(address, vec![old], vec![value], "club status")])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn put<T: Copy>(buf: &mut [u8], at: usize, v: T) {
        let bytes = unsafe { std::slice::from_raw_parts(&v as *const T as *const u8, std::mem::size_of::<T>()) };
        buf[at..at + bytes.len()].copy_from_slice(bytes);
    }

    fn driver() -> LinuxMemoryDriver {
        LinuxMemoryDriver::new(std::process::id() as i32)
    }

    fn objective_bytes(type_code: u8, value: i32, importance: u8, deadline: u32) -> Vec<u8> {
        let mut o = vec![0u8; 0x60];
        put(&mut o, O_TARGET_ID, -1i16);
        o[O_TARGET_KIND] = 0xff;
        put(&mut o, O_START, 0x07e9_1ac3u32);
        put(&mut o, O_DEADLINE, deadline);
        put(&mut o, O_VALUE, value);
        o[O_TYPE] = type_code;
        o[O_IMPORTANCE] = importance;
        o[O_STATUS] = 1;
        o
    }

    struct World {
        club: Vec<u8>,
        team: Vec<u8>,
        link: Vec<u8>,
        details: Vec<u8>,
        vobj: Vec<u8>,
        ptrs: Vec<usize>,
        objs: Vec<Vec<u8>>,
        names: HashMap<usize, String>,
    }

    fn world(objs: Vec<Vec<u8>>, details_type: &str) -> Box<World> {
        let mut w = Box::new(World {
            club: vec![0u8; 0x200],
            team: vec![0u8; 0x100],
            link: vec![0u8; 0x100],
            details: vec![0u8; 0x100],
            vobj: vec![0u8; 0x20],
            ptrs: vec![],
            objs,
            names: HashMap::new(),
        });
        let teams = Box::new([w.team.as_ptr() as usize]);
        let teams = Box::leak(teams);
        put(&mut w.club, CLUB_VECTOR, teams.as_ptr() as usize);
        put(&mut w.club, CLUB_VECTOR + 8, teams.as_ptr() as usize + 8);
        put(&mut w.team, TEAM_CLUB, w.club.as_ptr() as usize);
        put(&mut w.team, TEAM_LINK, w.link.as_ptr() as usize);
        put(&mut w.link, LINK_DETAILS, w.details.as_ptr() as usize);
        put(&mut w.details, DETAILS_TEAM, w.team.as_ptr() as usize);
        put(&mut w.details, DETAILS_OBJECTIVES, w.vobj.as_ptr() as usize);
        w.ptrs = w.objs.iter().map(|o| o.as_ptr() as usize).collect();
        let (b, e) = (w.ptrs.as_ptr() as usize, w.ptrs.as_ptr() as usize + w.ptrs.len() * 8);
        put(&mut w.vobj, 0, b);
        put(&mut w.vobj, 8, e);
        put(&mut w.vobj, 16, e);
        w.names.insert(w.team.as_ptr() as usize, TEAM_TYPE.into());
        w.names.insert(w.details.as_ptr() as usize, details_type.into());
        for o in &w.objs {
            w.names.insert(o.as_ptr() as usize, VISION_TYPE.into());
        }
        w
    }

    #[test]
    fn dates_decode_and_deadline_keeps_flag_bits() {
        let d = decode_date(0x07ea_0076).unwrap();
        assert_eq!((d.year, d.day, d.iso.as_str()), (2026, 118, "2026-04-28"));
        assert!(decode_date(EMPTY_DATE).is_none());
        assert_eq!(decode_date(0x07e9_7319).unwrap().day, 281);
        assert_eq!(with_date(0x07e9_7319, 2027, 100).unwrap(), (2027 << 16) | 0x7200 | 100);
        assert!(with_date(EMPTY_DATE, 2027, 1).is_err());
        assert!(with_date(0x07ea_0076, 2026, 366).is_err());
        assert!(with_date(0x07ea_0076, 1999, 1).is_err());
    }

    #[test]
    fn decode_marks_missing_values_and_unverified_labels() {
        let o = objective_bytes(150, -1, 4, EMPTY_DATE);
        let d = decode_objective(3, 0x1000, "club_vision", &o).unwrap();
        assert_eq!((d.value, d.importance, d.deadline.is_none()), (None, Some(4), true));
        assert_eq!((d.type_label, d.label_status), (Some("Play attacking football"), "catalog_unverified"));
        let bad = objective_bytes(222, 5, 0, 0x1234_5678);
        let d = decode_objective(0, 0x1000, "club_vision", &bad).unwrap();
        assert_eq!((d.type_label, d.importance), (None, None));
        assert_eq!(d.notes.len(), 2, "{:?}", d.notes);
    }

    #[test]
    fn reads_the_chain_and_refuses_wrong_kinds_and_broken_back_pointers() {
        let w = world(vec![objective_bytes(44, -1, 10, EMPTY_DATE), objective_bytes(7, 11, 5, 0x07ea_0076)], HUMAN_DETAILS_TYPE);
        let names = w.names.clone();
        let rtti = |a: usize| names.get(&a).cloned();
        let report = read_board_with(&driver(), w.club.as_ptr() as usize, 7, &rtti).unwrap();
        assert!(report.chain.human_manager_details);
        assert_eq!(report.objectives.len(), 2);
        assert_eq!(report.objectives[1].value, Some(11));
        assert_eq!(report.objectives[1].deadline.as_ref().unwrap().iso, "2026-04-28");
        // Wrong details type (plain MANAGER_DETAILS, e.g. Getafe) fails closed.
        let mut other = names.clone();
        other.insert(w.details.as_ptr() as usize, ".?AVMANAGER_DETAILS@@".into());
        assert!(read_board_with(&driver(), w.club.as_ptr() as usize, 7, &|a| other.get(&a).cloned()).is_err());
        // An object that is not an objective class is refused.
        let mut bad = names.clone();
        bad.insert(w.objs[0].as_ptr() as usize, ".?AVSUPPORT_STAFF@db@@".into());
        assert!(read_board_with(&driver(), w.club.as_ptr() as usize, 7, &|a| bad.get(&a).cloned()).is_err());
        // AI club details type is accepted and marked.
        let ai = world(vec![objective_bytes(44, -1, 10, EMPTY_DATE)], AI_DETAILS_TYPE);
        let n = ai.names.clone();
        assert!(!read_board_with(&driver(), ai.club.as_ptr() as usize, 7, &|a| n.get(&a).cloned()).unwrap().chain.human_manager_details);
    }

    #[test]
    fn broken_team_back_pointer_is_refused() {
        let mut w = world(vec![objective_bytes(44, -1, 10, EMPTY_DATE)], HUMAN_DETAILS_TYPE);
        put(&mut w.details, DETAILS_TEAM, 0usize);
        let names = w.names.clone();
        assert!(read_board_with(&driver(), w.club.as_ptr() as usize, 7, &|a| names.get(&a).cloned()).is_err());
    }

    #[test]
    fn writes_change_only_the_requested_scalar_and_refuse_unsafe_requests() {
        let w = world(vec![objective_bytes(7, 11, 5, 0x07ea_0076), objective_bytes(150, -1, 4, EMPTY_DATE)], HUMAN_DETAILS_TYPE);
        let names = w.names.clone();
        let rtti = |a: usize| names.get(&a).cloned();
        let club = w.club.as_ptr() as usize;
        let before = w.objs[0].clone();
        let edit = ObjectiveEdit { importance: Some(9), value: Some(12), deadline: Some((2027, 100)), status: Some(2) };
        let after = write_objective_with(&driver(), club, 0, 7, &edit, &rtti).unwrap();
        assert_eq!((after.importance, after.value, after.status_code), (Some(9), Some(12), 2));
        assert_eq!(after.deadline.unwrap().iso, "2027-04-10");
        let changed: Vec<usize> = (0..0x60).filter(|&i| w.objs[0][i] != before[i]).collect();
        assert!(changed.iter().all(|i| [O_IMPORTANCE, O_VALUE, O_DEADLINE, O_DEADLINE + 1, O_DEADLINE + 2, O_DEADLINE + 3, O_STATUS].contains(i)), "{changed:?}");
        // wrong expected type, bad index, no fields, value on a -1 objective, deadline on an empty date, bad status
        assert!(write_objective_with(&driver(), club, 0, 8, &edit, &rtti).is_err());
        assert!(write_objective_with(&driver(), club, 5, 7, &edit, &rtti).is_err());
        assert!(write_objective_with(&driver(), club, 0, 7, &ObjectiveEdit::default(), &rtti).is_err());
        assert!(write_objective_with(&driver(), club, 1, 150, &ObjectiveEdit { value: Some(3), ..Default::default() }, &rtti).is_err());
        assert!(write_objective_with(&driver(), club, 1, 150, &ObjectiveEdit { deadline: Some((2027, 1)), ..Default::default() }, &rtti).is_err());
        assert!(write_objective_with(&driver(), club, 0, 7, &ObjectiveEdit { status: Some(7), ..Default::default() }, &rtti).is_err());
        assert!(write_objective_with(&driver(), club, 0, 7, &ObjectiveEdit { importance: Some(11), ..Default::default() }, &rtti).is_err());
        assert!(write_objective_with(&driver(), club, 0, 7, &ObjectiveEdit { deadline: Some((2025, 1)), ..Default::default() }, &rtti).is_err(), "before start");
    }

    #[test]
    fn club_status_codes_map_only_one_to_three() {
        assert_eq!(status_label(1), Some("Professional"));
        assert_eq!(status_label(3), Some("Amateur"));
        assert_eq!(status_label(24), None);
        assert_eq!(status_label(0), None);
        let mut cell = vec![2u8, 0, 0, 0];
        let at = cell.as_mut_ptr() as usize;
        write_status_at(&driver(), at, 3).unwrap();
        assert_eq!(cell[0], 3);
        assert!(write_status_at(&driver(), at, 4).is_err());
        cell[0] = 24;
        assert!(write_status_at(&driver(), at, 1).is_err(), "special clubs are not editable");
        assert_eq!(cell[0], 24);
    }
}
