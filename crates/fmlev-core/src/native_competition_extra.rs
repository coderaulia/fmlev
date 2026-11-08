//! Competition extras beyond reputation: COMP detail fields, the native league prize table and the
//! league table. Everything here is live-read evidence on one build (PE timestamp 0x6a229e8a); writes
//! are experimental scalar writes through `apply_transaction` and unverified in game.
//!
//! COMP record (`NativeCompetitionRecord::comp`):
//! - +0x18a original reputation (u16, same 1..=200 scale as +0x188), +0x19c trophy type (u8),
//!   +0x19e lowest category (u8), pitch sizes u16 at +0x15c min length, +0x15e min width,
//!   +0x160 max length, +0x162 max width. Pitch values read 900..1200 (lengths) and 550..900
//!   (widths), for example 1050 x 680 for the Premier League, so they look like tenths of a metre
//!   (heuristic unit).
//!
//! Stage chain (live, 171 competitions with an active season): COMP+0xb0 -> ACTUAL_COMP
//! (`sicomps`) whose vector at +0x1c0/+0x1c8/+0x1d0 holds stage pointers (`LEAGUE_STAGE`,
//! `GROUP_STAGE`, `CUP_STAGE`, ...). A `LEAGUE_STAGE` has the standings row vector at +0xc0/+0xc8 and
//! a `LEAGUE_SETTINGS` pointer at +0xf0 whose +0x1c8 points at the i32 prize array (one raw-GBP amount
//! per position). Competitions without an active league stage (1,056 of 1,227) have none of this.

use fmlev_mem::LinuxMemoryDriver;
use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::native_competition::NativeCompetitionRecord;
use crate::native_rtti::{read_len_string, ModuleImage};
use crate::native_transfer::{apply_transaction, NativeEdit};

const ORIGINAL_REPUTATION: usize = 0x18a;
const TROPHY_TYPE: usize = 0x19c;
const LOWEST_CATEGORY: usize = 0x19e;
const PITCH_MIN_LENGTH: usize = 0x15c;
const PITCH_MIN_WIDTH: usize = 0x15e;
const PITCH_MAX_LENGTH: usize = 0x160;
const PITCH_MAX_WIDTH: usize = 0x162;

/// Values seen across the live survey (1,227 competitions); anything else is refused on write.
pub const TROPHY_TYPES_SEEN: [u8; 5] = [0, 1, 2, 4, 6];
pub const LOWEST_CATEGORIES_SEEN: [u8; 4] = [0, 1, 2, 6];
const PITCH_LENGTH_RANGE: std::ops::RangeInclusive<u16> = 600..=1400;
const PITCH_WIDTH_RANGE: std::ops::RangeInclusive<u16> = 400..=1000;

const ACTUAL_COMP_TYPE: &str = ".?AVACTUAL_COMP@sicomps@@";
const LEAGUE_STAGE_TYPE: &str = ".?AVLEAGUE_STAGE@sicomps@@";
const LEAGUE_SETTINGS_TYPE: &str = ".?AVLEAGUE_SETTINGS@sicomps@@";
const TEAM_TYPE: &str = ".?AVTEAM@db@@";
const CLUB_TYPE: &str = ".?AVCLUB@db@@";

const ACT_STAGES: usize = 0x1c0;
const STAGE_ROWS: usize = 0xc0;
const STAGE_SETTINGS: usize = 0xf0;
const SETTINGS_PRIZES: usize = 0x1c8;
const MAX_STAGES: usize = 32;
const MAX_TEAMS: usize = 64;
pub const MAX_PRIZE: i32 = 2_000_000_000;

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidValue(message.to_owned())
}

/// One native value, or null with the reason it is unavailable (never a default).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Field<T> {
    pub value: Option<T>,
    pub reason: Option<String>,
}

impl<T> Field<T> {
    fn ok(value: T) -> Self {
        Self { value: Some(value), reason: None }
    }
    fn missing(reason: impl Into<String>) -> Self {
        Self { value: None, reason: Some(reason.into()) }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CompetitionExtras {
    pub comp_index: i32,
    pub comp_uid: i32,
    pub original_reputation: Field<u16>,
    pub trophy_type: Field<u8>,
    pub lowest_category: Field<u8>,
    pub pitch_min_length: Field<u16>,
    pub pitch_min_width: Field<u16>,
    pub pitch_max_length: Field<u16>,
    pub pitch_max_width: Field<u16>,
    pub pitch_unit: &'static str,
    pub status: &'static str,
}

fn plausible<T: PartialOrd + Copy + std::fmt::Display>(value: T, ok: impl Fn(T) -> bool, what: &str) -> Field<T> {
    if ok(value) {
        Field::ok(value)
    } else {
        Field::missing(format!("{what} {value} is outside the range seen on the live survey"))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExtraEdits {
    pub original_reputation: Option<u16>,
    pub trophy_type: Option<u8>,
    pub lowest_category: Option<u8>,
    pub pitch_min_length: Option<u16>,
    pub pitch_min_width: Option<u16>,
    pub pitch_max_length: Option<u16>,
    pub pitch_max_width: Option<u16>,
}

impl ExtraEdits {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Locates the COMP record for `index` and checks the unique id (identity first).
pub fn locate_competition(driver: &LinuxMemoryDriver, comp_index: i32, comp_uid: i32) -> Result<NativeCompetitionRecord> {
    let record = NativeCompetitionRecord::discover(driver, comp_index)?;
    record.validate(driver)?;
    let uid = driver.read::<i32>(record.comp + 0xc)?;
    if uid != comp_uid {
        return Err(invalid(&format!("Competition {comp_index} has uid {uid}, not {comp_uid}")));
    }
    Ok(record)
}

pub fn read_extras(driver: &LinuxMemoryDriver, record: &NativeCompetitionRecord) -> Result<CompetitionExtras> {
    record.validate(driver)?;
    ModuleImage::discover(driver)?; // PE timestamp guard
    let c = record.comp;
    let field_u16 = |offset: usize, ok: &dyn Fn(u16) -> bool, what: &str| match driver.read::<u16>(c + offset) {
        Ok(v) => plausible(v, ok, what),
        Err(e) => Field::missing(format!("unreadable: {e}")),
    };
    let field_u8 = |offset: usize, ok: &dyn Fn(u8) -> bool, what: &str| match driver.read::<u8>(c + offset) {
        Ok(v) => plausible(v, ok, what),
        Err(e) => Field::missing(format!("unreadable: {e}")),
    };
    Ok(CompetitionExtras {
        comp_index: record.comp_id,
        comp_uid: driver.read::<i32>(c + 0xc)?,
        original_reputation: field_u16(ORIGINAL_REPUTATION, &|v| (1..=200).contains(&v), "original reputation"),
        trophy_type: field_u8(TROPHY_TYPE, &|v| TROPHY_TYPES_SEEN.contains(&v), "trophy type"),
        lowest_category: field_u8(LOWEST_CATEGORY, &|v| LOWEST_CATEGORIES_SEEN.contains(&v), "lowest category"),
        pitch_min_length: field_u16(PITCH_MIN_LENGTH, &|v| PITCH_LENGTH_RANGE.contains(&v), "pitch length"),
        pitch_min_width: field_u16(PITCH_MIN_WIDTH, &|v| PITCH_WIDTH_RANGE.contains(&v), "pitch width"),
        pitch_max_length: field_u16(PITCH_MAX_LENGTH, &|v| PITCH_LENGTH_RANGE.contains(&v), "pitch length"),
        pitch_max_width: field_u16(PITCH_MAX_WIDTH, &|v| PITCH_WIDTH_RANGE.contains(&v), "pitch width"),
        pitch_unit: "tenths of a metre (heuristic: 1050 x 680 for the Premier League)",
        status: "read_live_on_one_build_unverified_in_game",
    })
}

/// Experimental scalar writes in one transaction (pre-read, write, readback, rollback). Returns the
/// labels of the fields that changed; values already equal are skipped.
pub fn write_extras(driver: &LinuxMemoryDriver, record: &NativeCompetitionRecord, edits: &ExtraEdits) -> Result<Vec<&'static str>> {
    if edits.is_empty() {
        return Err(invalid("No competition fields to change"));
    }
    record.validate(driver)?;
    ModuleImage::discover(driver)?;
    let c = record.comp;
    if let Some(v) = edits.original_reputation {
        if !(1..=200).contains(&v) {
            return Err(invalid("Original reputation must be between 1 and 200 (native scale)"));
        }
    }
    if edits.trophy_type.is_some_and(|v| !TROPHY_TYPES_SEEN.contains(&v)) {
        return Err(invalid(&format!("Trophy type must be one of {TROPHY_TYPES_SEEN:?} (the values seen in the game)")));
    }
    if edits.lowest_category.is_some_and(|v| !LOWEST_CATEGORIES_SEEN.contains(&v)) {
        return Err(invalid(&format!("Lowest category must be one of {LOWEST_CATEGORIES_SEEN:?} (the values seen in the game)")));
    }
    for (v, range, what) in [
        (edits.pitch_min_length, &PITCH_LENGTH_RANGE, "Minimum pitch length"),
        (edits.pitch_max_length, &PITCH_LENGTH_RANGE, "Maximum pitch length"),
        (edits.pitch_min_width, &PITCH_WIDTH_RANGE, "Minimum pitch width"),
        (edits.pitch_max_width, &PITCH_WIDTH_RANGE, "Maximum pitch width"),
    ] {
        if v.is_some_and(|v| !range.contains(&v)) {
            return Err(invalid(&format!("{what} must be between {} and {} (tenths of a metre)", range.start(), range.end())));
        }
    }
    let current = |offset: usize| -> Result<u16> { Ok(driver.read::<u16>(c + offset)?) };
    let min_l = edits.pitch_min_length.map_or_else(|| current(PITCH_MIN_LENGTH), Ok)?;
    let max_l = edits.pitch_max_length.map_or_else(|| current(PITCH_MAX_LENGTH), Ok)?;
    let min_w = edits.pitch_min_width.map_or_else(|| current(PITCH_MIN_WIDTH), Ok)?;
    let max_w = edits.pitch_max_width.map_or_else(|| current(PITCH_MAX_WIDTH), Ok)?;
    if min_l > max_l || min_w > max_w {
        return Err(invalid("The minimum pitch size cannot exceed the maximum"));
    }
    let mut native = Vec::new();
    let mut push16 = |offset: usize, value: Option<u16>, label: &'static str| -> Result<()> {
        if let Some(value) = value {
            native.push(NativeEdit::new(c + offset, driver.read::<u16>(c + offset)?.to_le_bytes().to_vec(), value.to_le_bytes().to_vec(), label));
        }
        Ok(())
    };
    push16(ORIGINAL_REPUTATION, edits.original_reputation, "original reputation")?;
    push16(PITCH_MIN_LENGTH, edits.pitch_min_length, "minimum pitch length")?;
    push16(PITCH_MIN_WIDTH, edits.pitch_min_width, "minimum pitch width")?;
    push16(PITCH_MAX_LENGTH, edits.pitch_max_length, "maximum pitch length")?;
    push16(PITCH_MAX_WIDTH, edits.pitch_max_width, "maximum pitch width")?;
    for (offset, value, label) in [(TROPHY_TYPE, edits.trophy_type, "trophy type"), (LOWEST_CATEGORY, edits.lowest_category, "lowest category")] {
        if let Some(value) = value {
            native.push(NativeEdit::new(c + offset, vec![driver.read::<u8>(c + offset)?], vec![value], label));
        }
    }
    let changed: Vec<&'static str> = native.iter().filter(|e| e.old != e.next).map(|e| e.label).collect();
    apply_transaction(driver, &native)?;
    Ok(changed)
}

// ---------------------------------------------------------------------------------------------
// League stages: native prize table and league table
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LeagueStage {
    /// Index in the ACTUAL_COMP stage vector.
    index: usize,
    address: usize,
    rows_begin: usize,
    teams: usize,
}

fn type_is(driver: &LinuxMemoryDriver, image: Option<&ModuleImage>, object: usize, name: &str) -> bool {
    image.is_none_or(|image| image.is_type(driver, object, name))
}

fn good_ptr(p: usize) -> bool {
    p > 0x10000 && p % 8 == 0
}

fn league_stages(driver: &LinuxMemoryDriver, image: Option<&ModuleImage>, record: &NativeCompetitionRecord) -> Result<Vec<LeagueStage>> {
    record.validate(driver)?;
    let act = driver.read_ptr(record.comp + 0xb0)?;
    if !good_ptr(act) || !type_is(driver, image, act, ACTUAL_COMP_TYPE) {
        return Err(invalid("Competition has no ACTUAL_COMP object (RTTI check failed)"));
    }
    let (begin, end, cap) = (driver.read_ptr(act + ACT_STAGES)?, driver.read_ptr(act + ACT_STAGES + 8)?, driver.read_ptr(act + ACT_STAGES + 16)?);
    if begin == 0 && end == 0 {
        return Err(invalid("This competition has no active stages (not part of the current season)"));
    }
    if !good_ptr(begin) || end < begin || cap < end || (end - begin) % 8 != 0 || (end - begin) / 8 > MAX_STAGES {
        return Err(invalid("Competition stage vector failed validation"));
    }
    let mut stages = Vec::new();
    for index in 0..(end - begin) / 8 {
        let address = driver.read_ptr(begin + index * 8)?;
        if !good_ptr(address) || !type_is(driver, image, address, LEAGUE_STAGE_TYPE) {
            continue;
        }
        let (rb, re, rc) = (driver.read_ptr(address + STAGE_ROWS)?, driver.read_ptr(address + STAGE_ROWS + 8)?, driver.read_ptr(address + STAGE_ROWS + 16)?);
        if !good_ptr(rb) || re < rb || rc < re || (re - rb) % 8 != 0 {
            continue;
        }
        let teams = (re - rb) / 8;
        if (2..=MAX_TEAMS).contains(&teams) {
            stages.push(LeagueStage { index, address, rows_begin: rb, teams });
        }
    }
    if stages.is_empty() {
        return Err(invalid("This competition has no league stage (cup or group competition, or no active season)"));
    }
    Ok(stages)
}

fn pick_stage(stages: &[LeagueStage], wanted: Option<usize>) -> Result<LeagueStage> {
    match (stages, wanted) {
        (stages, Some(index)) => stages.iter().find(|s| s.index == index).copied().ok_or_else(|| invalid(&format!("Stage {index} is not a league stage of this competition"))),
        ([only], None) => Ok(*only),
        (_, None) => Err(invalid("Several league stages exist; pass stage_index")),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NativePrizeTable {
    pub stage_index: usize,
    pub address: usize,
    pub positions: usize,
    pub amounts: Vec<i32>,
    pub monotonic: bool,
    pub status: &'static str,
}

fn read_prize_table(driver: &LinuxMemoryDriver, image: Option<&ModuleImage>, stage: LeagueStage) -> Result<NativePrizeTable> {
    let settings = driver.read_ptr(stage.address + STAGE_SETTINGS)?;
    if !good_ptr(settings) || !type_is(driver, image, settings, LEAGUE_SETTINGS_TYPE) {
        return Err(invalid("League settings object is missing (RTTI check failed)"));
    }
    let address = driver.read_ptr(settings + SETTINGS_PRIZES)?;
    if address <= 0x10000 || address % 4 != 0 {
        return Err(invalid("This league has no native prize table (pointer is null)"));
    }
    let mut raw = vec![0u8; stage.teams * 4];
    if driver.read_bytes(address, &mut raw)? != raw.len() {
        return Err(invalid("Prize table is unreadable"));
    }
    let amounts: Vec<i32> = raw.chunks_exact(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect();
    if amounts.iter().any(|v| !(0..=MAX_PRIZE).contains(v)) {
        return Err(invalid("Prize table values failed the range check; refusing to report them"));
    }
    Ok(NativePrizeTable {
        stage_index: stage.index,
        address,
        positions: stage.teams,
        monotonic: amounts.windows(2).all(|p| p[0] >= p[1]),
        amounts,
        status: "read_live_matches_screen_search_table_unverified_payout",
    })
}

/// Screen-independent prize tables for every league stage of the competition (no value search).
pub fn read_native_prizes(driver: &LinuxMemoryDriver, record: &NativeCompetitionRecord) -> Result<Vec<NativePrizeTable>> {
    let image = ModuleImage::discover(driver)?;
    let stages = league_stages(driver, image.as_ref(), record)?;
    let mut tables = Vec::new();
    let mut last_error = None;
    for stage in stages {
        match read_prize_table(driver, image.as_ref(), stage) {
            Ok(table) => tables.push(table),
            Err(error) => last_error = Some(error),
        }
    }
    match (tables.is_empty(), last_error) {
        (true, Some(error)) => Err(error),
        _ => Ok(tables),
    }
}

/// Experimental: writes all positions of one table in one transaction (the same i32 array the
/// screen-search route edits). Counts must match exactly.
pub fn write_native_prizes(driver: &LinuxMemoryDriver, record: &NativeCompetitionRecord, stage_index: Option<usize>, prizes: &[i32]) -> Result<NativePrizeTable> {
    let image = ModuleImage::discover(driver)?;
    let stage = pick_stage(&league_stages(driver, image.as_ref(), record)?, stage_index)?;
    let table = read_prize_table(driver, image.as_ref(), stage)?;
    if prizes.len() != table.positions {
        return Err(invalid(&format!("Expected {} prize amounts", table.positions)));
    }
    if prizes.iter().any(|v| !(0..=MAX_PRIZE).contains(v)) {
        return Err(invalid("Prize amounts must be between 0 and 2,000,000,000"));
    }
    let edits: Vec<NativeEdit> = table
        .amounts
        .iter()
        .zip(prizes)
        .enumerate()
        .map(|(i, (old, next))| NativeEdit::new(table.address + i * 4, old.to_le_bytes().to_vec(), next.to_le_bytes().to_vec(), "league prize amount"))
        .collect();
    apply_transaction(driver, &edits)?;
    read_prize_table(driver, image.as_ref(), stage)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StandingRow {
    pub position: usize,
    pub row_address: usize,
    pub played: u8,
    pub won: u8,
    pub drawn: u8,
    pub lost: u8,
    pub goals_for: u16,
    pub goals_against: u16,
    pub points: u16,
    /// `played == won + drawn + lost` and `points == 3 * won + drawn`; false after a points deduction.
    pub consistent: bool,
    pub club_index: Option<i32>,
    pub club_uid: Option<i32>,
    pub club_name: Option<String>,
    pub team_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LeagueTable {
    pub stage_index: usize,
    pub rows: Vec<StandingRow>,
    pub layout: &'static str,
}

fn read_row(driver: &LinuxMemoryDriver, image: Option<&ModuleImage>, position: usize, row: usize) -> Result<StandingRow> {
    let (goals_for, goals_against, points) = (driver.read::<u16>(row + 8)?, driver.read::<u16>(row + 0xa)?, driver.read::<u16>(row + 0xc)?);
    let (played, won, drawn, lost) = (driver.read::<u8>(row + 0xe)?, driver.read::<u8>(row + 0x10)?, driver.read::<u8>(row + 0x11)?, driver.read::<u8>(row + 0x12)?);
    let mut out = StandingRow {
        position,
        row_address: row,
        played,
        won,
        drawn,
        lost,
        goals_for,
        goals_against,
        points,
        consistent: u16::from(played) == u16::from(won) + u16::from(drawn) + u16::from(lost) && points == 3 * u16::from(won) + u16::from(drawn),
        club_index: None,
        club_uid: None,
        club_name: None,
        team_reason: None,
    };
    let team = driver.read_ptr(row + 0x88)?;
    let club = if good_ptr(team) && type_is(driver, image, team, TEAM_TYPE) { driver.read_ptr(team + 0x30).unwrap_or(0) } else { 0 };
    if good_ptr(club) && type_is(driver, image, club, CLUB_TYPE) {
        out.club_index = driver.read::<i32>(club + 8).ok();
        out.club_uid = driver.read::<i32>(club + 0xc).ok();
        out.club_name = read_len_string(driver, driver.read_ptr(club + 0xc0).unwrap_or(0), 256);
        if out.club_name.is_none() {
            out.team_reason = Some("club name unreadable".to_owned());
        }
    } else {
        out.team_reason = Some("team or club pointer failed the RTTI check".to_owned());
    }
    Ok(out)
}

/// Read-only league tables, one per league stage, in the game's own row order (the standings order).
pub fn read_league_tables(driver: &LinuxMemoryDriver, record: &NativeCompetitionRecord) -> Result<Vec<LeagueTable>> {
    let image = ModuleImage::discover(driver)?;
    let mut tables = Vec::new();
    for stage in league_stages(driver, image.as_ref(), record)? {
        let mut rows = Vec::new();
        for i in 0..stage.teams {
            let row = driver.read_ptr(stage.rows_begin + i * 8)?;
            if !good_ptr(row) {
                return Err(invalid("A standings row pointer failed validation"));
            }
            rows.push(read_row(driver, image.as_ref(), i + 1, row)?);
        }
        tables.push(LeagueTable { stage_index: stage.index, rows, layout: "heuristic row layout: gf +8, ga +0xa, pts +0xc, played +0xe, won +0x10, drawn +0x11, lost +0x12, team +0x88" });
    }
    Ok(tables)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_rtti::fake::FakeImage;

    struct World {
        _keep: Vec<Vec<u64>>,
        record: NativeCompetitionRecord,
        image: FakeImage,
        prizes: usize,
        comp: usize,
    }

    fn driver() -> LinuxMemoryDriver {
        LinuxMemoryDriver::new(std::process::id() as i32)
    }

    fn words(n: usize) -> Vec<u64> {
        vec![0u64; n]
    }

    fn addr(v: &mut Vec<u64>) -> usize {
        v.as_mut_ptr() as usize
    }

    fn put<T: Copy>(address: usize, value: T) {
        driver().write(address, &value).unwrap();
    }

    fn name_buffer(text: &str) -> Vec<u64> {
        let mut buf = words(8);
        let bytes = unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, 64) };
        bytes[..4].copy_from_slice(&(text.len() as u32).to_le_bytes());
        bytes[4..4 + text.len()].copy_from_slice(text.as_bytes());
        buf
    }

    /// A COMP with a league stage of three teams and a prize array.
    fn world() -> World {
        let mut image = FakeImage::new();
        let act_vt = image.add_type(ACTUAL_COMP_TYPE);
        let stage_vt = image.add_type(LEAGUE_STAGE_TYPE);
        let settings_vt = image.add_type(LEAGUE_SETTINGS_TYPE);
        let team_vt = image.add_type(TEAM_TYPE);
        let club_vt = image.add_type(CLUB_TYPE);
        let mut keep = Vec::new();
        let mut comp = words(0x40);
        let mut act = words(0x40);
        let mut stage = words(0x40);
        let mut settings = words(0x40);
        let mut prize = vec![50_000_000u64 | (30_000_000u64 << 32), 10_000_000u64 | (0u64 << 32)];
        let mut rows = Vec::new();
        let mut row_ptrs = words(3);
        let names = ["Alpha", "Beta", "Gamma"];
        for (i, name) in names.iter().enumerate() {
            let mut club = words(0x40);
            let mut team = words(0x10);
            let mut row = words(0x20);
            let mut name_buf = name_buffer(name);
            put(addr(&mut club), club_vt as u64);
            put(addr(&mut club) + 8, 100 + i as i32);
            put(addr(&mut club) + 0xc, 1000 + i as i32);
            put(addr(&mut club) + 0xc0, addr(&mut name_buf) as u64);
            put(addr(&mut team), team_vt as u64);
            put(addr(&mut team) + 0x30, addr(&mut club) as u64);
            put(addr(&mut row) + 8, 5u16);
            put(addr(&mut row) + 0xa, 2u16);
            let (w, d, l) = (2u8 - i as u8, 1u8, i as u8);
            put(addr(&mut row) + 0xc, 3 * u16::from(w) + u16::from(d));
            put(addr(&mut row) + 0xe, w + d + l);
            put(addr(&mut row) + 0x10, w);
            put(addr(&mut row) + 0x11, d);
            put(addr(&mut row) + 0x12, l);
            put(addr(&mut row) + 0x88, addr(&mut team) as u64);
            put(addr(&mut row_ptrs) + i * 8, addr(&mut row) as u64);
            rows.extend([club, team, row, name_buf]);
        }
        put(addr(&mut comp) + 8, 7i32);
        put(addr(&mut comp) + 0xc, 11i32);
        put(addr(&mut comp) + 0x10, 11i32);
        put(addr(&mut comp) + 0xb0, addr(&mut act) as u64);
        put(addr(&mut comp) + 0x188, 150u16);
        put(addr(&mut comp) + ORIGINAL_REPUTATION, 150u16);
        put(addr(&mut comp) + TROPHY_TYPE, 6u8);
        put(addr(&mut comp) + LOWEST_CATEGORY, 6u8);
        put(addr(&mut comp) + PITCH_MIN_LENGTH, 1000u16);
        put(addr(&mut comp) + PITCH_MIN_WIDTH, 640u16);
        put(addr(&mut comp) + PITCH_MAX_LENGTH, 1050u16);
        put(addr(&mut comp) + PITCH_MAX_WIDTH, 680u16);
        put(addr(&mut act), act_vt as u64);
        let mut stage_vec = vec![addr(&mut stage) as u64];
        let sv = addr(&mut stage_vec);
        put(addr(&mut act) + ACT_STAGES, sv as u64);
        put(addr(&mut act) + ACT_STAGES + 8, (sv + 8) as u64);
        put(addr(&mut act) + ACT_STAGES + 16, (sv + 8) as u64);
        put(addr(&mut stage), stage_vt as u64);
        let rp = addr(&mut row_ptrs);
        put(addr(&mut stage) + STAGE_ROWS, rp as u64);
        put(addr(&mut stage) + STAGE_ROWS + 8, (rp + 24) as u64);
        put(addr(&mut stage) + STAGE_ROWS + 16, (rp + 24) as u64);
        put(addr(&mut stage) + STAGE_SETTINGS, addr(&mut settings) as u64);
        put(addr(&mut settings), settings_vt as u64);
        let prizes = addr(&mut prize);
        put(addr(&mut settings) + SETTINGS_PRIZES, prizes as u64);
        let comp_address = addr(&mut comp);
        keep.extend([comp, act, stage, settings, prize, row_ptrs, stage_vec]);
        keep.extend(rows);
        let record = NativeCompetitionRecord { pid: driver().pid(), comp: comp_address, comp_id: 7 };
        World { _keep: keep, record, image, prizes, comp: comp_address }
    }

    #[test]
    fn extras_are_read_with_plausibility_and_written_transactionally() {
        let w = world();
        let d = driver();
        let extras = read_extras(&d, &w.record).unwrap();
        assert_eq!(extras.original_reputation.value, Some(150));
        assert_eq!(extras.trophy_type.value, Some(6));
        assert_eq!(extras.pitch_max_length.value, Some(1050));
        put(w.comp + TROPHY_TYPE, 9u8);
        put(w.comp + PITCH_MIN_WIDTH, 5u16);
        let odd = read_extras(&d, &w.record).unwrap();
        assert_eq!(odd.trophy_type.value, None);
        assert!(odd.trophy_type.reason.as_deref().unwrap().contains("outside"));
        assert_eq!(odd.pitch_min_width.value, None);
        put(w.comp + TROPHY_TYPE, 6u8);
        put(w.comp + PITCH_MIN_WIDTH, 640u16);

        let edits = ExtraEdits { trophy_type: Some(2), pitch_max_length: Some(1100), original_reputation: Some(150), ..Default::default() };
        let changed = write_extras(&d, &w.record, &edits).unwrap();
        assert_eq!(changed, vec!["maximum pitch length", "trophy type"]);
        assert_eq!(d.read::<u8>(w.comp + TROPHY_TYPE).unwrap(), 2);
        assert_eq!(d.read::<u16>(w.comp + PITCH_MAX_LENGTH).unwrap(), 1100);
        // Refusals leave memory untouched.
        for bad in [
            ExtraEdits { trophy_type: Some(3), ..Default::default() },
            ExtraEdits { lowest_category: Some(9), ..Default::default() },
            ExtraEdits { original_reputation: Some(0), ..Default::default() },
            ExtraEdits { pitch_min_length: Some(1200), ..Default::default() },
            ExtraEdits { pitch_max_width: Some(300), ..Default::default() },
            ExtraEdits::default(),
        ] {
            assert!(write_extras(&d, &w.record, &bad).is_err());
        }
        assert_eq!(d.read::<u16>(w.comp + PITCH_MIN_LENGTH).unwrap(), 1000);
    }

    #[test]
    fn prize_table_reads_and_writes_through_the_stage_chain() {
        let w = world();
        let d = driver();
        let stages = league_stages(&d, Some(&w.image.image()), &w.record).unwrap();
        assert_eq!(stages.len(), 1);
        let table = read_prize_table(&d, Some(&w.image.image()), stages[0]).unwrap();
        assert_eq!(table.amounts, vec![50_000_000, 30_000_000, 10_000_000]);
        assert_eq!(table.address, w.prizes);
        assert!(table.monotonic);
        // The array tail word (beyond the positions) is never touched.
        put(w.prizes + 12, 1_169_454_544i32);
        let stage = pick_stage(&stages, None).unwrap();
        let edits: Vec<NativeEdit> = [(0usize, 60_000_000i32), (2, 9_000_000)]
            .iter()
            .map(|(i, v)| NativeEdit::new(w.prizes + i * 4, table.amounts[*i].to_le_bytes().to_vec(), v.to_le_bytes().to_vec(), "x"))
            .collect();
        apply_transaction(&d, &edits).unwrap();
        let after = read_prize_table(&d, Some(&w.image.image()), stage).unwrap();
        assert_eq!(after.amounts, vec![60_000_000, 30_000_000, 9_000_000]);
        assert_eq!(d.read::<i32>(w.prizes + 12).unwrap(), 1_169_454_544);
        // Negative or absurd values are refused instead of reported.
        put(w.prizes, -5i32);
        assert!(read_prize_table(&d, Some(&w.image.image()), stage).is_err());
    }

    #[test]
    fn stages_need_the_right_rtti_types_and_pointer_shapes() {
        let w = world();
        let d = driver();
        // A different image does not know these vtables: the RTTI pin rejects everything.
        let other = FakeImage::new();
        assert!(league_stages(&d, Some(&other.image()), &w.record).is_err());
        // A competition without stages reports it plainly.
        let act = d.read_ptr(w.comp + 0xb0).unwrap();
        let saved = d.read_ptr(act + ACT_STAGES).unwrap();
        put(act + ACT_STAGES, 0u64);
        put(act + ACT_STAGES + 8, 0u64);
        let error = league_stages(&d, Some(&w.image.image()), &w.record).unwrap_err().to_string();
        assert!(error.contains("no active stages"), "{error}");
        put(act + ACT_STAGES, saved as u64);
    }

    #[test]
    fn league_table_rows_resolve_clubs_and_flag_inconsistent_points() {
        let w = world();
        let d = driver();
        let stage = league_stages(&d, Some(&w.image.image()), &w.record).unwrap()[0];
        let first = read_row(&d, Some(&w.image.image()), 1, d.read_ptr(stage.rows_begin).unwrap()).unwrap();
        assert_eq!(first.club_name.as_deref(), Some("Alpha"));
        assert_eq!((first.club_index, first.club_uid), (Some(100), Some(1000)));
        assert_eq!((first.played, first.won, first.drawn, first.lost, first.points), (3, 2, 1, 0, 7));
        assert!(first.consistent);
        let row = d.read_ptr(stage.rows_begin).unwrap();
        put(row + 0xc, 4u16);
        assert!(!read_row(&d, Some(&w.image.image()), 1, row).unwrap().consistent);
        put(row + 0x88, 0u64);
        let broken = read_row(&d, Some(&w.image.image()), 1, row).unwrap();
        assert_eq!(broken.club_name, None);
        assert!(broken.team_reason.as_deref().unwrap().contains("RTTI"));
    }

    #[test]
    fn several_league_stages_need_an_explicit_index() {
        let a = LeagueStage { index: 0, address: 1, rows_begin: 1, teams: 4 };
        let b = LeagueStage { index: 2, address: 2, rows_begin: 2, teams: 4 };
        assert!(pick_stage(&[a, b], None).is_err());
        assert_eq!(pick_stage(&[a, b], Some(2)).unwrap(), b);
        assert!(pick_stage(&[a, b], Some(1)).is_err());
        assert_eq!(pick_stage(&[a], None).unwrap(), a);
    }
}
