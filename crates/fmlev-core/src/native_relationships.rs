//! Player relationship records and the "trained at club / nation" (homegrown) flags.
//!
//! Layout (read live on PID 98701, Phil Foden 10173 and Raul Asencio 33054; heuristic, see AGENTS.md):
//! the person sub-object `S = record + 0x140` (`ACTUAL_PLAYER + 0x288`) holds at `S + 0x78` a pointer to a
//! block whose first three qwords are `{begin, end, cap}` of a vector of 16-byte records
//! `{u64 target, i16 value, u8 category, u8 sub_context, u8 type, u8 b13, u8 b14, u8 b15}`.
//! - Homegrown at club: category 1, sub_context 72, type 1, target = CLUB object (b13/b14/b15 = ff/00/ff).
//! - Homegrown at nation: category 8, sub_context 70, type 1, target = NATION object (b13/b14/b15 = 00/00/ff).
//! Other categories (people, teams, competitions) exist and are never modified.
//!
//! WRITE RISK (user decision 2026-11-08, overrides AGENTS.md Rule 7 for this feature only, throwaway save):
//! the vector lives in the game's own allocator. When it is full, `ensure_homegrown` allocates a NEW array
//! in the game process (`fmlev_mem::remote_mmap`), copies the old records plus the new ones and swaps
//! `begin/end/cap`. The game may later `free`/`realloc` that foreign array (it was not obtained from its
//! allocator), which can crash or corrupt the heap. The old array is leaked on purpose (the game may still
//! reference it); the new mapping is never freed. Existing records are never modified.

use fmlev_mem::LinuxMemoryDriver;

use crate::error::{CoreError, Result};
use crate::native_catalog::{native_string, CatalogKind, RttiCatalog};
use crate::native_transfer::{apply_transaction, NativeEdit};

pub const PERSON_FROM_RECORD: usize = 0x140;
pub const PLAYER_PRIMARY_FROM_RECORD: usize = 0x148;
pub const RELATIONS_POINTER_OFFSET: usize = 0x78;
pub const RECORD_SIZE: usize = 16;
pub const MAX_RECORDS: usize = 4096;
const NATIONALITY_OFFSET: usize = 0x1a8;
const CONTRACT_OFFSET: usize = 0x1e8;
const PERSON_SUBOBJECT: u32 = 0x288;
const MIN_POINTER: usize = 0x10000;
const USER_MAX: usize = 0x7fff_ffff_ffff;

pub const CLUB_CATEGORY: u8 = 1;
pub const CLUB_SUB_CONTEXT: u8 = 72;
pub const NATION_CATEGORY: u8 = 8;
pub const NATION_SUB_CONTEXT: u8 = 70;
pub const HOMEGROWN_TYPE: u8 = 1;

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::InvalidValue(message.into())
}

/// Allocates writable memory inside the game process. A trait so the full-vector path is testable
/// without ptrace.
pub trait RemoteAllocator {
    fn allocate(&self, len: usize) -> Result<usize>;
}

/// Allocation through ptrace syscall injection (`fmlev_mem::remote_mmap`).
pub struct PtraceAllocator {
    pub pid: i32,
}

impl RemoteAllocator for PtraceAllocator {
    fn allocate(&self, len: usize) -> Result<usize> {
        fmlev_mem::remote_mmap(self.pid, len).map_err(|error| invalid(format!("Could not allocate memory in the game process: {error}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct RelationRecord {
    pub address: usize,
    pub target: usize,
    pub value: i16,
    pub category: u8,
    pub sub_context: u8,
    pub kind: u8,
    pub tail: [u8; 3],
}

impl RelationRecord {
    pub fn parse(address: usize, raw: &[u8; RECORD_SIZE]) -> Self {
        Self {
            address,
            target: usize::from_le_bytes(raw[0..8].try_into().unwrap()),
            value: i16::from_le_bytes([raw[8], raw[9]]),
            category: raw[10],
            sub_context: raw[11],
            kind: raw[12],
            tail: [raw[13], raw[14], raw[15]],
        }
    }

    pub fn to_bytes(&self) -> [u8; RECORD_SIZE] {
        let mut raw = [0u8; RECORD_SIZE];
        raw[0..8].copy_from_slice(&(self.target as u64).to_le_bytes());
        raw[8..10].copy_from_slice(&self.value.to_le_bytes());
        raw[10] = self.category;
        raw[11] = self.sub_context;
        raw[12] = self.kind;
        raw[13..16].copy_from_slice(&self.tail);
        raw
    }

    /// The record exactly as live data holds it (value 0, type 1).
    pub fn homegrown(kind: HomegrownKind, target: usize) -> Self {
        let (category, sub_context, tail) = match kind {
            HomegrownKind::Club => (CLUB_CATEGORY, CLUB_SUB_CONTEXT, [0xff, 0x00, 0xff]),
            HomegrownKind::Nation => (NATION_CATEGORY, NATION_SUB_CONTEXT, [0x00, 0x00, 0xff]),
        };
        Self { address: 0, target, value: 0, category, sub_context, kind: HOMEGROWN_TYPE, tail }
    }

    pub fn is_homegrown(&self, kind: HomegrownKind) -> bool {
        let (category, sub_context) = kind.category();
        self.category == category && self.sub_context == sub_context && self.kind == HOMEGROWN_TYPE
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HomegrownKind {
    Club,
    Nation,
}

impl HomegrownKind {
    fn category(self) -> (u8, u8) {
        match self {
            Self::Club => (CLUB_CATEGORY, CLUB_SUB_CONTEXT),
            Self::Nation => (NATION_CATEGORY, NATION_SUB_CONTEXT),
        }
    }

    fn catalog_kind(self) -> CatalogKind {
        match self {
            Self::Club => CatalogKind::Club,
            Self::Nation => CatalogKind::Nation,
        }
    }

    /// Length-prefixed UTF-8 name pointer offset (CLUB + 0xc0, NATION + 0x18; same as the search catalog).
    fn name_offset(self) -> usize {
        match self {
            Self::Club => 0xc0,
            Self::Nation => 0x18,
        }
    }
}

/// The relationship vector of one player, with the block that owns `{begin, end, cap}`.
#[derive(Debug, Clone)]
pub struct RelationList {
    pub person: usize,
    pub block: usize,
    pub begin: usize,
    pub end: usize,
    pub cap: usize,
    pub records: Vec<RelationRecord>,
}

impl RelationList {
    pub fn spare_slots(&self) -> usize {
        (self.cap - self.end) / RECORD_SIZE
    }

    pub fn has_target(&self, kind: HomegrownKind, target: usize) -> bool {
        self.records.iter().any(|r| r.is_homegrown(kind) && r.target == target)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TargetRef {
    pub address: usize,
    pub id: Option<i32>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HomegrownSummary {
    pub clubs: Vec<TargetRef>,
    pub nations: Vec<TargetRef>,
    /// Homegrown-category records whose target is not a CLUB / NATION by RTTI (reported, never guessed).
    pub unresolved_targets: usize,
    pub relation_count: usize,
    pub spare_slots: usize,
}

fn valid_pointer(value: usize) -> bool {
    value > MIN_POINTER && value <= USER_MAX && value % 8 == 0
}

/// Validates the player object chain and returns the person sub-object `S`.
/// `record` is the attribute view (`ACTUAL_PLAYER + 0x148`), as in `NativePlayerRecord`.
pub fn validate_person(driver: &LinuxMemoryDriver, catalog: &RttiCatalog, record: usize) -> Result<usize> {
    let primary = record
        .checked_sub(PLAYER_PRIMARY_FROM_RECORD)
        .ok_or_else(|| invalid("Invalid player record address"))?;
    let person = record + PERSON_FROM_RECORD;
    if !catalog.is(driver, person, CatalogKind::Player, PERSON_SUBOBJECT) {
        return Err(invalid("Player person sub-object is not an ACTUAL_PLAYER (RTTI mismatch)"));
    }
    match catalog.at(driver, primary) {
        Some(native) if native.kind == CatalogKind::Player => {}
        _ => return Err(invalid("Player primary object is not an ACTUAL_PLAYER (RTTI mismatch)")),
    }
    Ok(person)
}

/// Reads and validates the relationship vector (begin <= end <= cap, 16-byte multiples, <= 4096 records).
pub fn read_relations(driver: &LinuxMemoryDriver, catalog: &RttiCatalog, record: usize) -> Result<RelationList> {
    let person = validate_person(driver, catalog, record)?;
    let block = driver.read_ptr(person + RELATIONS_POINTER_OFFSET)?;
    if !valid_pointer(block) {
        return Err(invalid("Player has no relationship block"));
    }
    let [begin, end, cap] = driver.read::<[usize; 3]>(block)?;
    if !valid_pointer(begin) || end < begin || cap < end {
        return Err(invalid("Relationship vector pointers are not ordered (begin <= end <= cap)"));
    }
    if (end - begin) % RECORD_SIZE != 0 || (cap - begin) % RECORD_SIZE != 0 || (cap - begin) / RECORD_SIZE > MAX_RECORDS {
        return Err(invalid("Relationship vector size is not a sane multiple of 16 bytes"));
    }
    let count = (end - begin) / RECORD_SIZE;
    let mut raw = vec![0u8; count * RECORD_SIZE];
    if count > 0 && driver.read_bytes(begin, &mut raw)? != raw.len() {
        return Err(invalid("Short read of the relationship vector"));
    }
    let records = raw
        .chunks_exact(RECORD_SIZE)
        .enumerate()
        .map(|(i, chunk)| RelationRecord::parse(begin + i * RECORD_SIZE, chunk.try_into().unwrap()))
        .collect();
    Ok(RelationList { person, block, begin, end, cap, records })
}

/// True when `target` carries the RTTI vtable of the requested kind (CLUB or NATION, primary vtable).
fn target_is(driver: &LinuxMemoryDriver, catalog: &RttiCatalog, kind: HomegrownKind, target: usize) -> bool {
    valid_pointer(target) && catalog.is(driver, target, kind.catalog_kind(), 0)
}

pub fn describe_target(driver: &LinuxMemoryDriver, kind: HomegrownKind, target: usize) -> TargetRef {
    TargetRef {
        address: target,
        id: driver.read::<i32>(target + 8).ok(),
        name: driver.read_ptr(target + kind.name_offset()).ok().and_then(|p| native_string(driver, p)),
    }
}

pub fn homegrown_summary(driver: &LinuxMemoryDriver, catalog: &RttiCatalog, record: usize) -> Result<HomegrownSummary> {
    let list = read_relations(driver, catalog, record)?;
    let mut summary = HomegrownSummary { clubs: vec![], nations: vec![], unresolved_targets: 0, relation_count: list.records.len(), spare_slots: list.spare_slots() };
    for relation in &list.records {
        for kind in [HomegrownKind::Club, HomegrownKind::Nation] {
            if !relation.is_homegrown(kind) {
                continue;
            }
            if target_is(driver, catalog, kind, relation.target) {
                let target = describe_target(driver, kind, relation.target);
                match kind {
                    HomegrownKind::Club => summary.clubs.push(target),
                    HomegrownKind::Nation => summary.nations.push(target),
                }
            } else {
                summary.unresolved_targets += 1;
            }
        }
    }
    Ok(summary)
}

/// The player's current club object: parent contract (`record + 0x1e8`, owner back-pointer checked) ->
/// team (`+0x10`) -> club (`team + 0x30`), RTTI-validated at every step. `None` when any link is absent.
pub fn current_club(driver: &LinuxMemoryDriver, catalog: &RttiCatalog, record: usize) -> Option<usize> {
    let contract = driver.read_ptr(record + CONTRACT_OFFSET).ok().filter(|&p| valid_pointer(p))?;
    if !catalog.is(driver, contract, CatalogKind::FullContract, 0) || driver.read_ptr(contract + 8).ok()? != record + PERSON_FROM_RECORD {
        return None;
    }
    let team = driver.read_ptr(contract + 0x10).ok().filter(|&p| valid_pointer(p))?;
    if !catalog.is(driver, team, CatalogKind::Team, 0) {
        return None;
    }
    let club = driver.read_ptr(team + 0x30).ok()?;
    target_is(driver, catalog, HomegrownKind::Club, club).then_some(club)
}

/// Training nation for a resolved parent club. Ignore absent or incorrectly typed links.
pub fn club_nation(driver: &LinuxMemoryDriver, catalog: &RttiCatalog, club: usize) -> Option<usize> {
    if !target_is(driver, catalog, HomegrownKind::Club, club) {
        return None;
    }
    let nation = driver.read_ptr(club + crate::native_catalog::CLUB_NATION_LINK).ok()?;
    target_is(driver, catalog, HomegrownKind::Nation, nation).then_some(nation)
}

/// First nationality NATION object (`record + 0x1a8`), RTTI-validated.
pub fn first_nationality(driver: &LinuxMemoryDriver, catalog: &RttiCatalog, record: usize) -> Option<usize> {
    let nation = driver.read_ptr(record + NATIONALITY_OFFSET).ok()?;
    target_is(driver, catalog, HomegrownKind::Nation, nation).then_some(nation)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    NotRequested,
    AlreadyPresent,
    Appended,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EnsureReport {
    pub club: Outcome,
    pub nation: Outcome,
    /// `Some((new_begin, new_cap_records))` when a new array was allocated and swapped in.
    pub new_array: Option<(usize, usize)>,
}

/// Adds the requested homegrown records unless an equal one (same category/sub-context/type and target)
/// already exists. Targets must be CLUB / NATION objects by RTTI. One transaction with readback and
/// rollback; see the module comment for the risk of the new-array path.
pub fn ensure_homegrown(
    driver: &LinuxMemoryDriver,
    catalog: &RttiCatalog,
    record: usize,
    club: Option<usize>,
    nation: Option<usize>,
    allocator: &dyn RemoteAllocator,
) -> Result<EnsureReport> {
    if club.is_none() && nation.is_none() {
        return Err(invalid("Give a club, a nation or both"));
    }
    for (kind, target) in [(HomegrownKind::Club, club), (HomegrownKind::Nation, nation)] {
        if let Some(target) = target {
            if !target_is(driver, catalog, kind, target) {
                return Err(invalid(format!("Target 0x{target:x} is not a {} object (RTTI mismatch)", if kind == HomegrownKind::Club { "CLUB" } else { "NATION" })));
            }
        }
    }
    let list = read_relations(driver, catalog, record)?;
    let mut outcome = EnsureReport { club: Outcome::NotRequested, nation: Outcome::NotRequested, new_array: None };
    let mut additions: Vec<RelationRecord> = Vec::new();
    for (kind, target) in [(HomegrownKind::Club, club), (HomegrownKind::Nation, nation)] {
        let Some(target) = target else { continue };
        let slot = if kind == HomegrownKind::Club { &mut outcome.club } else { &mut outcome.nation };
        if list.has_target(kind, target) {
            *slot = Outcome::AlreadyPresent;
        } else {
            *slot = Outcome::Appended;
            additions.push(RelationRecord::homegrown(kind, target));
        }
    }
    if additions.is_empty() {
        return Ok(outcome);
    }
    let added_bytes: Vec<u8> = additions.iter().flat_map(|r| r.to_bytes()).collect();
    let used = list.end - list.begin;
    if list.end + added_bytes.len() <= list.cap {
        // In-capacity append: data first, then `end`; both rolled back together on any mismatch.
        let mut old_tail = vec![0u8; added_bytes.len()];
        driver.read_bytes(list.end, &mut old_tail)?;
        let edits = [
            NativeEdit::new(list.end, old_tail, added_bytes, "relationship records"),
            NativeEdit::new(list.block + 8, (list.end as u64).to_le_bytes().to_vec(), ((list.end + additions.len() * RECORD_SIZE) as u64).to_le_bytes().to_vec(), "relationship end"),
        ];
        apply_transaction(driver, &edits)?;
        return Ok(outcome);
    }
    // Full: build a new array (old records + new ones + headroom), then swap begin/end/cap in one transaction.
    let count = used / RECORD_SIZE + additions.len();
    let new_cap_records = (count + 8).max(16);
    let bytes = new_cap_records * RECORD_SIZE;
    let new_begin = allocator.allocate(bytes)?;
    if !valid_pointer(new_begin) {
        return Err(invalid(format!("Allocator returned an unusable address 0x{new_begin:x}")));
    }
    let mut payload = vec![0u8; used];
    if used > 0 && driver.read_bytes(list.begin, &mut payload)? != used {
        return Err(invalid("Short read while copying the relationship vector"));
    }
    payload.extend_from_slice(&added_bytes);
    if driver.write_bytes(new_begin, &payload)? != payload.len() {
        return Err(invalid("Short write into the new relationship array"));
    }
    let mut check = vec![0u8; payload.len()];
    if driver.read_bytes(new_begin, &mut check)? != check.len() || check != payload {
        return Err(invalid("Readback mismatch in the new relationship array; the vector was not changed"));
    }
    let triple = |begin: usize, end: usize, cap: usize| -> Vec<u8> { [begin, end, cap].iter().flat_map(|v| (*v as u64).to_le_bytes()).collect() };
    let edits = [NativeEdit::new(
        list.block,
        triple(list.begin, list.end, list.cap),
        triple(new_begin, new_begin + count * RECORD_SIZE, new_begin + new_cap_records * RECORD_SIZE),
        "relationship begin/end/cap",
    )];
    apply_transaction(driver, &edits)?;
    outcome.new_array = Some((new_begin, new_cap_records));
    Ok(outcome)
}

#[cfg(test)]
#[path = "native_relationships_tests.rs"]
mod tests;
