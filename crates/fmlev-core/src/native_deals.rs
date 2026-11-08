//! Transfer deal objects (`FULL_TRANSFER_OFFER`, the game's Transfer Activity rows).
//!
//! Layout read live on PID 98701 (PE timestamp 0x6a229e8a, vtable RVA 0x470b128, ~2,060 objects in one pool at
//! stride 0xd0) and compared with the game's Recruitment > Transfer Activity screen for R. Madrid:
//! - `+0x08` u32 deal type (1 transfer, 2 loan; 3 pre-contract is a lead only).
//! - `+0x38` person: the `ACTUAL_PLAYER` person sub-object (+0x288; 1,658 deals), a staff sub-object (+0x100; 392
//!   deals) or a hybrid `ACTUAL_PLAYER_AND_NON_PLAYER` sub-object (+0x380; 12 deals). `+0x40`/`+0x48` buying team, `+0x50` selling team (may be null).
//! - `+0x58` i32 headline fee in raw GBP (Mbappe 92,818,912 = the screen's GBP 93M).
//! - `+0x6c..+0x7c` five packed date words (`year << 16 | flags | day of year in the low 9 bits`, 0x076c0001 = none);
//!   `+0x70` is the join date (matched on two deals).
//! - `+0x84` u8 and `+0x88` u32 status: only four combinations have screen labels (see [`status_label`]).
//! - `+0xa8` pointer to a heap block holding two vectors (begin, end, capacity): instalments (12-byte entries
//!   `{amount, months, interval flags}`) and performance clauses (8-byte entries `{amount, threshold}`). For Mbappe
//!   they read 29,533,388 over 24 months in steps of 12 (2 instalments) and 41,509,600 at 20 appearances, which with
//!   the 92.8M headline fee gives the screen's "GBP 93M (164M with clauses): 29.5M over 2 instalments, 41.5M after 20
//!   league appearances". Other clause kinds are not distinguished.
//!
//! Reads validate vtable, RTTI-typed pointers and sane fee/dates. Writes (experimental, unverified in game) change
//! only the fee (`+0x58`, 4 bytes) and the join date (`+0x70`, flag bits kept) of an existing deal through
//! [`apply_transaction`]; status, teams and clause lists are never written.

use std::collections::HashMap;

use fmlev_mem::{parse_process_maps, LinuxMemoryDriver, MemoryMapping};
use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::native_catalog::{is_complete_object_locator, native_string, pe_image_size, pooled_name, type_name};
use crate::native_player::player_uid_from_words;
use crate::native_transfer::{apply_transaction, NativeEdit};
use crate::search_index::{is_leap, GameDay};

const PE_TIMESTAMP: u32 = 0x6a22_9e8a;
const CHUNK: usize = 16 << 20;
const PLAYER_RECORD: usize = 0x148;
const PERSON_SUBOBJECT: usize = 0x288;
const MAX_CLAUSE_ENTRIES: usize = 16;
const DEAL_SIZE: usize = 0xd0;
const NONE_DATES: [u32; 3] = [0, 0xffff_ffff, 0x076c_0001];
const JOIN_DATE_OFFSET: usize = 0x70;
const FEE_OFFSET: usize = 0x58;
const DATE_OFFSETS: [usize; 5] = [0x6c, 0x70, 0x74, 0x78, 0x7c];

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::InvalidValue(message.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DealKind {
    Offer,
    Player,
    Hybrid,
    Staff,
    Team,
    NationalTeam,
    Club,
}

impl DealKind {
    fn from_type_name(name: &str) -> Option<Self> {
        Some(match name {
            ".?AVFULL_TRANSFER_OFFER@@" => Self::Offer,
            ".?AVACTUAL_PLAYER@db@@" => Self::Player,
            ".?AVACTUAL_PLAYER_AND_NON_PLAYER@db@@" => Self::Hybrid,
            ".?AVACTUAL_NON_PLAYER@db@@" => Self::Staff,
            ".?AVTEAM@db@@" => Self::Team,
            ".?AVNATIONAL_TEAM@db@@" => Self::NationalTeam,
            ".?AVCLUB@db@@" => Self::Club,
            _ => return None,
        })
    }
}

/// Vtable address -> `(type, sub-object offset)`, resolved by RTTI from the live image.
#[derive(Debug, Clone, Default)]
pub struct DealTypes {
    tables: HashMap<usize, (DealKind, u32)>,
}

impl DealTypes {
    pub fn from_tables(tables: HashMap<usize, (DealKind, u32)>) -> Self {
        Self { tables }
    }

    fn is(&self, driver: &LinuxMemoryDriver, address: usize, kind: DealKind, sub: u32) -> bool {
        driver.read_ptr(address).ok().and_then(|vt| self.tables.get(&vt)).is_some_and(|t| *t == (kind, sub))
    }

    fn kind_at(&self, driver: &LinuxMemoryDriver, address: usize) -> Option<(DealKind, u32)> {
        self.tables.get(&driver.read_ptr(address).ok()?).copied()
    }

    fn offer_vtable(&self) -> Option<usize> {
        self.tables.iter().find(|(_, t)| **t == (DealKind::Offer, 0)).map(|(vt, _)| *vt)
    }

    /// Resolves the vtables from game_plugin.dll RTTI; fails closed on an unknown build or ambiguous image.
    pub fn discover(driver: &LinuxMemoryDriver, maps: &[MemoryMapping]) -> Result<Self> {
        let mut images: Vec<(usize, usize)> = maps
            .iter()
            .filter(|m| m.pathname.ends_with("game_plugin.dll"))
            .filter_map(|m| pe_image_size(driver, m.start).map(|size| (m.start, size)))
            .collect();
        images.dedup();
        let (base, size) = match images.as_slice() {
            [image] => *image,
            [] => return Err(invalid("game_plugin.dll is not mapped; attach to FM26 with a save loaded first")),
            _ => return Err(invalid("Several game_plugin.dll images are mapped; deal discovery is ambiguous")),
        };
        let pe = driver.read::<u32>(base + 0x3c)? as usize;
        let stamp = driver.read::<u32>(base + pe + 8)?;
        if pe > 0x1000 || stamp != PE_TIMESTAMP {
            return Err(invalid(format!("Unknown game_plugin.dll build (PE timestamp {stamp:#x}); the deal layout is unverified for it")));
        }
        let in_image = |p: usize| p >= base && p - base < size;
        let mut names: HashMap<usize, Option<String>> = HashMap::new();
        let mut tables = HashMap::new();
        for map in maps.iter().filter(|m| m.start >= base && m.end <= base + size && m.is_readable() && !m.is_writable() && !m.is_executable()) {
            let mut start = map.start;
            while start < map.end {
                let len = (map.end - start).min(CHUNK);
                let mut buf = vec![0u8; len];
                if driver.read_bytes(start, &mut buf).is_ok() {
                    for i in (0..len.saturating_sub(15)).step_by(8) {
                        let locator = usize::from_le_bytes(buf[i..i + 8].try_into().unwrap());
                        if !in_image(locator) {
                            continue;
                        }
                        let Ok(col) = driver.read::<[u32; 6]>(locator) else { continue };
                        if !is_complete_object_locator(&col, locator, base) {
                            continue;
                        }
                        let descriptor = base + col[3] as usize;
                        let name = names.entry(descriptor).or_insert_with(|| type_name(driver, descriptor));
                        if let Some(kind) = name.as_deref().and_then(DealKind::from_type_name) {
                            tables.insert(start + i + 8, (kind, col[1]));
                        }
                    }
                }
                start += len;
            }
        }
        let types = Self { tables };
        if types.offer_vtable().is_none() {
            return Err(invalid("FULL_TRANSFER_OFFER RTTI not found in game_plugin.dll"));
        }
        Ok(types)
    }
}

// ---- dates -----------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DealDate {
    pub raw: u32,
    pub year: u16,
    /// 1-based day of year.
    pub day: u16,
    pub iso: String,
}

fn iso(year: u16, day: u16) -> String {
    let lengths = [31, if is_leap(year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut left = day;
    for (month, length) in lengths.iter().enumerate() {
        if left <= *length {
            return format!("{year:04}-{:02}-{left:02}", month + 1);
        }
        left -= length;
    }
    format!("{year:04}-12-31")
}

/// `Ok(None)` = the game's "no date" words, `Err(())` = not a plausible date.
fn decode_date(raw: u32) -> std::result::Result<Option<DealDate>, ()> {
    if NONE_DATES.contains(&raw) {
        return Ok(None);
    }
    let (year, day) = ((raw >> 16) as u16, (raw & 0x1ff) as u16);
    GameDay::new(year, day).map(|_| Some(DealDate { raw, year, day, iso: iso(year, day) })).ok_or(())
}

/// Replaces year and day, keeping the flag bits (9..=15) of `old`.
fn date_with(old: u32, year: u16, day: u16) -> u32 {
    (u32::from(year) << 16) | (old & 0xfe00) | u32::from(day)
}

// ---- deal model -------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DealTeam {
    pub address: usize,
    pub kind: &'static str,
    pub index: Option<i32>,
    pub club_id: Option<i32>,
    pub club_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DealPlayer {
    pub address: usize,
    pub kind: &'static str,
    pub id: Option<i32>,
    pub uid: Option<u32>,
    pub name: Option<String>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Instalment {
    pub amount_gbp: i32,
    pub months: u32,
    pub interval_raw: u32,
    /// Low byte of `interval_raw` (12 on the checked deal); heuristic.
    pub interval_months: u32,
    /// `months / interval_months` when it divides evenly (2 for the Mbappe deal on screen); heuristic.
    pub count: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PerformanceClause {
    pub amount_gbp: i32,
    /// Appearance count on the checked deal (Mbappe: 20 league appearances); other kinds are not distinguished.
    pub threshold: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DealClauses {
    pub present: bool,
    pub instalments: Vec<Instalment>,
    pub performance: Vec<PerformanceClause>,
    /// Headline fee plus every instalment and clause amount (screen: 92.8M + 29.5M + 41.5M = "164M").
    pub total_with_clauses_gbp: Option<i64>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OtherDate {
    pub offset: String,
    pub date: Option<DealDate>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NativeDeal {
    pub address: usize,
    pub deal_type: u32,
    pub deal_type_label: Option<&'static str>,
    pub player: DealPlayer,
    pub buying_team: DealTeam,
    pub selling_team: Option<DealTeam>,
    pub fee_gbp: i32,
    pub join_date: Option<DealDate>,
    pub other_dates: Vec<OtherDate>,
    pub status_raw: u32,
    pub byte_84: u8,
    pub status_label: Option<&'static str>,
    pub status_text: String,
    /// Raw `+0x64` / `+0x68` ints (-1 = none on most deals); meaning not verified.
    pub clause_a_raw: i32,
    pub clause_b_raw: i32,
    pub clauses: DealClauses,
}

/// Labels confirmed on the R. Madrid Transfer Activity screen; every other combination stays unlabelled.
pub fn status_label(status: u32, byte_84: u8) -> Option<&'static str> {
    match (status, byte_84) {
        (0x701, 0) => Some("Offer Made"),
        (0x700, 2) => Some("Work Permit"),
        (0x700, 3) => Some("Contract Rejected"),
        (0x100_0701, 2) => Some("Confirmation"),
        _ => None,
    }
}

fn deal_type_label(kind: u32) -> Option<&'static str> {
    match kind {
        1 => Some("transfer"),
        2 => Some("loan"),
        3 => Some("pre-contract (unverified)"),
        _ => None,
    }
}

fn read_team(driver: &LinuxMemoryDriver, types: &DealTypes, address: usize) -> Option<DealTeam> {
    let (kind, sub) = types.kind_at(driver, address)?;
    if sub != 0 {
        return None;
    }
    let (label, index) = match kind {
        DealKind::Team => {
            let ids = driver.read::<[u32; 3]>(address + 8).ok()?;
            if ids[0] > i32::MAX as u32 || ids[1] == 0 {
                return None;
            }
            ("team", Some(ids[0] as i32))
        }
        DealKind::NationalTeam => ("national_team", None),
        _ => return None,
    };
    let club = driver.read_ptr(address + 0x30).ok().filter(|c| types.is(driver, *c, DealKind::Club, 0));
    let club_id = club.and_then(|c| driver.read::<u32>(c + 8).ok()).filter(|i| *i <= i32::MAX as u32).map(|i| i as i32);
    let club_name = club.and_then(|c| native_string(driver, driver.read_ptr(c + 0xc0).ok()?));
    Some(DealTeam { address, kind: label, index, club_id, club_name })
}

fn read_player(driver: &LinuxMemoryDriver, types: &DealTypes, pointer: usize) -> Option<DealPlayer> {
    let (kind, sub) = types.kind_at(driver, pointer)?;
    match (kind, sub) {
        (DealKind::Player, 0) | (DealKind::Player, 0x288) => {
            let primary = if sub == 0 { pointer } else { pointer.checked_sub(PERSON_SUBOBJECT)? };
            // Both views must be typed: the primary object and the person sub-object it embeds.
            if !types.is(driver, primary, DealKind::Player, 0) || !types.is(driver, primary + PERSON_SUBOBJECT, DealKind::Player, 0x288) {
                return None;
            }
            let record = primary + PLAYER_RECORD;
            let ids = driver.read::<[u32; 3]>(record + 0x148).ok()?;
            let uid = player_uid_from_words(ids[1], ids[2]).filter(|_| ids[0] <= i32::MAX as u32)?;
            Some(DealPlayer { address: primary, kind: "player", id: Some(ids[0] as i32), uid: Some(uid), name: pooled_name(driver, record + 0x190), unavailable_reason: None })
        }
        // Staff (ACTUAL_NON_PLAYER sub-object at +0x100) and hybrid person/staff objects (sub-object at +0x380) are
        // named through the pooled name at +0x50 of the pointed sub-object (as for staff records); no player id.
        (DealKind::Staff, 0x100) | (DealKind::Hybrid, 0x380) => {
            let (label, reason) = if kind == DealKind::Staff {
                ("staff", "staff member's deal: not a player, so it has no player id")
            } else {
                ("player_and_staff", "hybrid player/staff object: its player id layout is not mapped")
            };
            Some(DealPlayer { address: pointer, kind: label, id: None, uid: None, name: pooled_name(driver, pointer + 0x50), unavailable_reason: Some(reason.into()) })
        }
        _ => None,
    }
}

fn read_vector(driver: &LinuxMemoryDriver, at: usize, entry: usize) -> std::result::Result<Vec<u8>, String> {
    let words = driver.read::<[usize; 3]>(at).map_err(|e| e.to_string())?;
    let [begin, end, cap] = words;
    if begin == 0 && end == 0 {
        return Ok(Vec::new());
    }
    if begin <= 0x10000 || end < begin || cap < end || (end - begin) % entry != 0 || (end - begin) / entry > MAX_CLAUSE_ENTRIES {
        return Err("a clause list failed its bounds check".into());
    }
    let mut bytes = vec![0u8; end - begin];
    if !bytes.is_empty() && driver.read_bytes(begin, &mut bytes).map_err(|e| e.to_string())? != bytes.len() {
        return Err("a clause list is unreadable".into());
    }
    Ok(bytes)
}

fn read_clauses(driver: &LinuxMemoryDriver, deal: usize, fee: i32) -> DealClauses {
    let block = match driver.read_ptr(deal + 0xa8) {
        Ok(0) => return DealClauses::default(),
        Ok(block) if block > 0x10000 => block,
        _ => return DealClauses { unavailable_reason: Some("clause block pointer is invalid".into()), ..Default::default() },
    };
    let read = || -> std::result::Result<DealClauses, String> {
        let first = read_vector(driver, block, 12)?;
        let second = read_vector(driver, block + 24, 8)?;
        let u = |chunk: &[u8], o: usize| u32::from_le_bytes(chunk[o..o + 4].try_into().unwrap());
        let amount = |chunk: &[u8]| i32::try_from(u(chunk, 0)).map_err(|_| "a clause amount is out of range".to_string());
        let mut out = DealClauses { present: true, ..Default::default() };
        for chunk in first.chunks_exact(12) {
            let interval_raw = u(chunk, 8);
            let (months, interval_months) = (u(chunk, 4), interval_raw & 0xff);
            let count = (interval_months > 0 && months % interval_months == 0).then(|| months / interval_months);
            out.instalments.push(Instalment { amount_gbp: amount(chunk)?, months, interval_raw, interval_months, count });
        }
        for chunk in second.chunks_exact(8) {
            out.performance.push(PerformanceClause { amount_gbp: amount(chunk)?, threshold: u(chunk, 4) });
        }
        let extra: i64 = out.instalments.iter().map(|i| i64::from(i.amount_gbp)).chain(out.performance.iter().map(|p| i64::from(p.amount_gbp))).sum();
        out.total_with_clauses_gbp = Some(i64::from(fee) + extra);
        Ok(out)
    };
    read().unwrap_or_else(|reason| DealClauses { unavailable_reason: Some(reason), ..Default::default() })
}

/// Reads and validates the deal at `address`. Errors give the reason a candidate was rejected.
pub fn read_deal(driver: &LinuxMemoryDriver, types: &DealTypes, address: usize) -> Result<NativeDeal> {
    if !types.is(driver, address, DealKind::Offer, 0) {
        return Err(invalid(format!("0x{address:x} is not a FULL_TRANSFER_OFFER")));
    }
    let mut raw = [0u8; DEAL_SIZE];
    if driver.read_bytes(address, &mut raw)? != DEAL_SIZE {
        return Err(invalid("deal object unreadable"));
    }
    let word = |o: usize| u32::from_le_bytes(raw[o..o + 4].try_into().unwrap());
    let ptr = |o: usize| usize::from_le_bytes(raw[o..o + 8].try_into().unwrap());
    let deal_type = word(8);
    if !(1..=3).contains(&deal_type) {
        return Err(invalid(format!("deal type {deal_type} is not 1..=3")));
    }
    let player = read_player(driver, types, ptr(0x38)).ok_or_else(|| invalid("player pointer is not an RTTI-typed player"))?;
    let buying_team = read_team(driver, types, ptr(0x40)).ok_or_else(|| invalid("buying team is not an RTTI-typed team"))?;
    if ptr(0x48) != 0 && read_team(driver, types, ptr(0x48)).is_none() {
        return Err(invalid("second buying-team pointer is not an RTTI-typed team"));
    }
    let selling_team = match ptr(0x50) {
        0 => None,
        p => Some(read_team(driver, types, p).ok_or_else(|| invalid("selling team is not an RTTI-typed team"))?),
    };
    let fee_gbp = word(FEE_OFFSET) as i32;
    if fee_gbp < 0 {
        return Err(invalid("negative headline fee"));
    }
    let mut dates = Vec::new();
    for offset in DATE_OFFSETS {
        dates.push((offset, decode_date(word(offset)).map_err(|_| invalid(format!("implausible date word {:#x} at +{offset:#x}", word(offset))))?));
    }
    let join_date = dates.iter().find(|(o, _)| *o == JOIN_DATE_OFFSET).and_then(|(_, d)| d.clone());
    let other_dates = dates.into_iter().filter(|(o, _)| *o != JOIN_DATE_OFFSET).map(|(o, date)| OtherDate { offset: format!("+{o:#x}"), date }).collect();
    let (status_raw, byte_84) = (word(0x88), raw[0x84]);
    let status_label = status_label(status_raw, byte_84);
    let status_text = status_label.map_or_else(|| format!("Status {status_raw:#x} / +0x84 {byte_84}"), str::to_owned);
    Ok(NativeDeal {
        address,
        deal_type,
        deal_type_label: deal_type_label(deal_type),
        player,
        buying_team,
        selling_team,
        fee_gbp,
        join_date,
        other_dates,
        status_raw,
        byte_84,
        status_label,
        status_text,
        clause_a_raw: word(0x64) as i32,
        clause_b_raw: word(0x68) as i32,
        clauses: read_clauses(driver, address, fee_gbp),
    })
}

// ---- scanning ---------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize)]
pub struct DealScan {
    pub deals: Vec<NativeDeal>,
    pub candidates: usize,
    pub rejected: usize,
    /// Why candidates were rejected (reason -> count), so a drop in coverage is visible.
    pub rejected_reasons: std::collections::BTreeMap<String, usize>,
}

/// One read-only pass over writable private memory for objects whose first qword is the deal vtable.
pub fn scan_deals(driver: &LinuxMemoryDriver, types: &DealTypes, maps: &[MemoryMapping]) -> DealScan {
    let Some(vtable) = types.offer_vtable() else { return DealScan::default() };
    let needle = vtable.to_le_bytes();
    let mut addresses = Vec::new();
    for map in maps.iter().filter(|m| m.is_rw_private()) {
        let mut start = map.start;
        while start < map.end {
            let len = (map.end - start).min(CHUNK);
            let mut buf = vec![0u8; len];
            if driver.read_bytes(start, &mut buf).is_ok() {
                for i in (0..len.saturating_sub(7)).step_by(8) {
                    if buf[i..i + 8] == needle {
                        addresses.push(start + i);
                    }
                }
            }
            start += len;
        }
    }
    let mut scan = DealScan { candidates: addresses.len(), ..Default::default() };
    for address in addresses {
        match read_deal(driver, types, address) {
            Ok(deal) => scan.deals.push(deal),
            Err(error) => {
                scan.rejected += 1;
                *scan.rejected_reasons.entry(error.to_string()).or_default() += 1;
            }
        }
    }
    scan
}

/// Resolves RTTI and scans the attached process. Blocking: call from a worker thread.
pub fn scan_process(driver: &LinuxMemoryDriver) -> Result<DealScan> {
    let maps = parse_process_maps(driver.pid())?;
    let types = DealTypes::discover(driver, &maps)?;
    Ok(scan_deals(driver, &types, &maps))
}

impl NativeDeal {
    pub fn involves_club(&self, club_id: i32) -> bool {
        self.buying_team.club_id == Some(club_id) || self.selling_team.as_ref().is_some_and(|t| t.club_id == Some(club_id))
    }
}

// ---- experimental scalar edits ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default)]
pub struct DealUpdate {
    pub fee_gbp: Option<i64>,
    /// `(year, day of year)`.
    pub join_date: Option<(u16, u16)>,
}

/// Edits the fee and/or join date of the deal at `address`. The deal is re-read and must still belong to
/// `player_id`; the join date must already be a real date (a "none" word is never overwritten).
pub fn update_deal_with(driver: &LinuxMemoryDriver, types: &DealTypes, address: usize, player_id: i32, update: DealUpdate) -> Result<NativeDeal> {
    if update.fee_gbp.is_none() && update.join_date.is_none() {
        return Err(invalid("Give a fee and/or a join date to change"));
    }
    let deal = read_deal(driver, types, address)?;
    if deal.player.id != Some(player_id) {
        return Err(invalid("This deal no longer belongs to that player; reload the deal list"));
    }
    let mut edits = Vec::new();
    if let Some(fee) = update.fee_gbp {
        let fee = i32::try_from(fee).ok().filter(|f| *f >= 0).ok_or_else(|| invalid("The fee must be a whole amount from 0 to 2,147,483,647"))?;
        edits.push(NativeEdit::new(address + FEE_OFFSET, deal.fee_gbp.to_le_bytes().to_vec(), fee.to_le_bytes().to_vec(), "deal: headline fee"));
    }
    if let Some((year, day)) = update.join_date {
        GameDay::new(year, day).ok_or_else(|| invalid("The join date is not a valid game date (year 1900..=2200, valid day of year)"))?;
        let old = deal.join_date.as_ref().ok_or_else(|| invalid("This deal has no join date set; a missing date is not overwritten"))?.raw;
        let next = date_with(old, year, day);
        edits.push(NativeEdit::new(address + JOIN_DATE_OFFSET, old.to_le_bytes().to_vec(), next.to_le_bytes().to_vec(), "deal: join date"));
    }
    apply_transaction(driver, &edits)?;
    read_deal(driver, types, address)
}

/// Same as [`update_deal_with`] after resolving RTTI for the attached process.
pub fn update_deal(driver: &LinuxMemoryDriver, address: usize, player_id: i32, update: DealUpdate) -> Result<NativeDeal> {
    let maps = parse_process_maps(driver.pid())?;
    let types = DealTypes::discover(driver, &maps)?;
    update_deal_with(driver, &types, address, player_id, update)
}

#[cfg(test)]
#[path = "native_deals_tests.rs"]
mod tests;
