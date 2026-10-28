//! Screen-independent enumeration of the loaded FM26 database by MSVC RTTI (plan G2).
//!
//! Port of `tools/fmscan` `NativeCatalog` + `read_entity`: vtables are resolved by RTTI type name inside
//! game_plugin.dll (no RVA is compiled in), then one pass over writable private memory collects every
//! object whose first qword is a known vtable. Each candidate must pass the same identity checks as
//! fmscan (index, duplicate uid, names) before it is indexed. Read-only: nothing here writes memory.
//!
//! Layouts (relative to the object carrying the vtable):
//! - Club: ids at +8, name string at +0xc0, nation pointer at +0xd8.
//! - Nation: ids at +8, name string at +0x18.
//! - Competition: ids at +8, name string at +0x40, nation pointer at +0x60.
//! - Team: ids at +8 (uid at +0xc and +0x10 may differ), club pointer at +0x30.
//! - Player (`ACTUAL_PLAYER` primary object `P`): record `R = P + 0x148`, ids at `R + 0x148`, person
//!   sub-object (`ACTUAL_PLAYER`, offset 0x288) at `R + 0x140`, positions `R + 8..R + 0x17`, feet at
//!   `R + 0x2f/0x30`, pooled names at `R + 0x190/0x198`, parent contract at `R + 0x1e8`
//!   (`FULL_CONTRACT`, person back-pointer at +8, team at +0x10).
//! - Staff (`ACTUAL_NON_PLAYER` sub-object `S` at record + 0x100): second sub-object at `S + 0x28`,
//!   ids at `S + 8`, pooled names at `S + 0x50/0x58`.

use std::collections::HashMap;

use fmlev_mem::{parse_process_maps, LinuxMemoryDriver, MemoryMapping};

use crate::error::{CoreError, Result};
use crate::search_index::{drop_duplicate_ids, EntityKind, IndexEntry};

const CHUNK: usize = 16 << 20;
const MAX_STRING: usize = 512;
const PLAYER_RECORD: usize = 0x148;
const PLAYER_PERSON_SUBOBJECT: u32 = 0x288;
const STAFF_SUBOBJECT: u32 = 0x100;
const STAFF_SECOND_SUBOBJECT: u32 = 0x128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CatalogKind {
    Player,
    Staff,
    Club,
    Team,
    Nation,
    Competition,
    FullContract,
}

impl CatalogKind {
    pub fn from_type_name(name: &str) -> Option<Self> {
        Some(match name {
            ".?AVACTUAL_PLAYER@db@@" => Self::Player,
            ".?AVACTUAL_NON_PLAYER@db@@" => Self::Staff,
            ".?AVCLUB@db@@" => Self::Club,
            ".?AVTEAM@db@@" => Self::Team,
            ".?AVNATION@db@@" => Self::Nation,
            ".?AVCOMP@db@@" => Self::Competition,
            ".?AVFULL_CONTRACT@db@@" => Self::FullContract,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeType {
    pub kind: CatalogKind,
    /// MSVC complete-object-locator offset: 0 for the primary vtable, else the sub-object offset.
    pub subobject_offset: u32,
}

/// Vtable address -> database type, resolved by RTTI from the live image.
#[derive(Debug, Clone, Default)]
pub struct RttiCatalog {
    pub tables: HashMap<usize, NativeType>,
}

/// x64 MSVC complete object locator: signature 1, and its self RVA must equal its own image offset.
pub fn is_complete_object_locator(col: &[u32; 6], locator: usize, base: usize) -> bool {
    col[0] == 1 && locator >= base && col[5] as usize == locator - base
}

/// PE32+ image size from the headers at `base`, or `None` when they are not a valid image.
fn pe_image_size(driver: &LinuxMemoryDriver, base: usize) -> Option<usize> {
    if driver.read::<u16>(base).ok()? != 0x5a4d {
        return None;
    }
    let pe = driver.read::<u32>(base + 0x3c).ok()? as usize;
    if pe > CHUNK || driver.read::<u32>(base + pe).ok()? != 0x4550 || driver.read::<u16>(base + pe + 24).ok()? != 0x20b {
        return None;
    }
    let size = driver.read::<u32>(base + pe + 80).ok()? as usize;
    (size >= 24).then_some(size)
}

fn type_name(driver: &LinuxMemoryDriver, descriptor: usize) -> Option<String> {
    let mut raw = [0u8; 128];
    driver.read_bytes(descriptor + 16, &mut raw).ok()?;
    let end = raw.iter().position(|&b| b == 0)?;
    let name = std::str::from_utf8(&raw[..end]).ok()?;
    (name.starts_with(".?A") && name.bytes().all(|b| (32..=126).contains(&b))).then(|| name.to_owned())
}

impl RttiCatalog {
    pub fn from_tables(tables: HashMap<usize, NativeType>) -> Self {
        Self { tables }
    }

    /// Resolves the supported database vtables from game_plugin.dll RTTI.
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
            _ => return Err(invalid("Several game_plugin.dll images are mapped; native discovery is ambiguous")),
        };
        let in_image = |p: usize| p >= base && p - base < size;
        let mut names: HashMap<usize, Option<String>> = HashMap::new();
        let mut tables = HashMap::new();
        // Proton maps PE sections anonymously, so every read-only, non-executable mapping in the image counts.
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
                        if let Some(kind) = name.as_deref().and_then(CatalogKind::from_type_name) {
                            tables.insert(start + i + 8, NativeType { kind, subobject_offset: col[1] });
                        }
                    }
                }
                start += len;
            }
        }
        if tables.is_empty() {
            return Err(invalid("No supported FM26 database RTTI types found in game_plugin.dll"));
        }
        Ok(Self { tables })
    }

    fn at(&self, driver: &LinuxMemoryDriver, address: usize) -> Option<NativeType> {
        self.tables.get(&driver.read_ptr(address).ok()?).copied()
    }

    fn is(&self, driver: &LinuxMemoryDriver, address: usize, kind: CatalogKind, subobject_offset: u32) -> bool {
        self.at(driver, address) == Some(NativeType { kind, subobject_offset })
    }
}

/// Result of one enumeration pass.
#[derive(Debug, Clone, Default)]
pub struct Enumeration {
    pub entries: Vec<IndexEntry>,
    pub duplicates_dropped: usize,
    pub candidates: usize,
}

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidValue(message.to_owned())
}

fn native_string(driver: &LinuxMemoryDriver, ptr: usize) -> Option<String> {
    if ptr <= 0x10000 {
        return None;
    }
    let len = driver.read::<u32>(ptr).ok()? as usize;
    if len == 0 || len > MAX_STRING {
        return None;
    }
    let mut buf = vec![0u8; len];
    if driver.read_bytes(ptr + 4, &mut buf).ok()? != len {
        return None;
    }
    String::from_utf8(buf).ok().filter(|s| !s.chars().any(char::is_control))
}

/// First + last name from pooled references. `None` when a present reference is broken or both are empty.
fn pooled_name(driver: &LinuxMemoryDriver, first_ref: usize) -> Option<String> {
    let mut parts = Vec::new();
    for reference in [first_ref, first_ref + 8] {
        let pointer = driver.read_ptr(reference).ok()?;
        if pointer == 0 {
            continue;
        }
        parts.push(native_string(driver, driver.read_ptr(pointer).ok()?)?);
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// `(index, uid)` when `[index, uid, uid2]` at `address` passes the identity rule.
fn identity(driver: &LinuxMemoryDriver, address: usize, uid_twice: bool) -> Option<(i32, u32)> {
    let ids = driver.read::<[u32; 3]>(address).ok()?;
    let ok = ids[0] <= i32::MAX as u32 && ids[1] != 0 && if uid_twice { ids[1] == ids[2] } else { ids[2] != 0 };
    ok.then_some((ids[0] as i32, ids[1]))
}

struct Regions(Vec<MemoryMapping>);

impl Regions {
    fn new(maps: &[MemoryMapping]) -> Self {
        let mut maps = maps.to_vec();
        maps.sort_by_key(|m| m.start);
        Self(maps)
    }

    fn of(&self, address: usize) -> Option<&MemoryMapping> {
        let i = self.0.partition_point(|m| m.start <= address);
        self.0[..i].last().filter(|m| address < m.end)
    }
}

struct Pass<'a> {
    driver: &'a LinuxMemoryDriver,
    catalog: &'a RttiCatalog,
    regions: Regions,
}

impl Pass<'_> {
    fn named(&self, address: usize, name_offset: usize) -> Option<(i32, u32, String)> {
        let (id, uid) = identity(self.driver, address + 8, true)?;
        let name = native_string(self.driver, self.driver.read_ptr(address + name_offset).ok()?)?;
        Some((id, uid, name))
    }

    /// Player primary object `P`; returns `(record, id, uid, name)`.
    fn player(&self, address: usize) -> Option<(usize, i32, u32, String)> {
        let driver = self.driver;
        let record = address + PLAYER_RECORD;
        let (id, uid) = identity(driver, record + 0x148, true)?;
        if !self.catalog.is(driver, record + 0x140, CatalogKind::Player, PLAYER_PERSON_SUBOBJECT) {
            return None;
        }
        // The attribute view's vtable sits in read-only memory and its first slot points at code.
        let view_vtable = driver.read_ptr(record).ok()?;
        let function = driver.read_ptr(view_vtable).ok()?;
        if self.regions.of(view_vtable).is_none_or(|m| m.is_writable()) || self.regions.of(function).is_none_or(|m| !m.is_executable()) {
            return None;
        }
        let positions = driver.read::<[u8; 15]>(record + 8).ok()?;
        let feet = driver.read::<[u8; 2]>(record + 0x2f).ok()?;
        if !positions.iter().all(|p| (1..=20).contains(p)) || !feet.iter().all(|f| (1..=100).contains(f) && f % 5 == 0) {
            return None;
        }
        Some((record, id, uid, pooled_name(driver, record + 0x190)?))
    }

    fn staff(&self, address: usize) -> Option<(i32, u32, String)> {
        let second = address + (STAFF_SECOND_SUBOBJECT - STAFF_SUBOBJECT) as usize;
        if !self.catalog.is(self.driver, second, CatalogKind::Staff, STAFF_SECOND_SUBOBJECT) {
            return None;
        }
        let (id, uid) = identity(self.driver, address + 8, true)?;
        Some((id, uid, pooled_name(self.driver, address + 0x50)?))
    }

    /// Club name of a staff member's contract (`R + 0x1a8`, owner back-pointer `R + 0x100`), only when
    /// every link validates. Verified on live profiles: wage, expiry and club agree with the game screens.
    fn staff_club(&self, record: usize, teams: &HashMap<usize, ()>, clubs: &HashMap<usize, String>) -> Option<String> {
        let driver = self.driver;
        let contract = driver.read_ptr(record + 0x1a8).ok()?;
        if contract == 0
            || !self.catalog.is(driver, contract, CatalogKind::FullContract, 0)
            || driver.read_ptr(contract + 8).ok()? != record + STAFF_SUBOBJECT as usize
        {
            return None;
        }
        let team = driver.read_ptr(contract + 0x10).ok()?;
        teams.get(&team)?;
        clubs.get(&driver.read_ptr(team + 0x30).ok()?).cloned()
    }

    /// Club name of a player's parent contract, only when every link validates.
    fn player_club(&self, record: usize, teams: &HashMap<usize, ()>, clubs: &HashMap<usize, String>) -> Option<String> {
        let driver = self.driver;
        let contract = driver.read_ptr(record + 0x1e8).ok()?;
        if contract == 0
            || !self.catalog.is(driver, contract, CatalogKind::FullContract, 0)
            || driver.read_ptr(contract + 8).ok()? != record + 0x140
        {
            return None;
        }
        let team = driver.read_ptr(contract + 0x10).ok()?;
        teams.get(&team)?;
        clubs.get(&driver.read_ptr(team + 0x30).ok()?).cloned()
    }
}

/// One read-only pass over writable private memory collecting every supported database record.
pub fn enumerate(driver: &LinuxMemoryDriver, catalog: &RttiCatalog, maps: &[MemoryMapping]) -> Enumeration {
    let (Some(&low), Some(&high)) = (catalog.tables.keys().min(), catalog.tables.keys().max()) else {
        return Enumeration::default();
    };
    let mut candidates: Vec<(usize, NativeType)> = Vec::new();
    for map in maps.iter().filter(|m| m.is_rw_private()) {
        let mut start = map.start;
        while start < map.end {
            let len = (map.end - start).min(CHUNK);
            let mut buf = vec![0u8; len];
            if driver.read_bytes(start, &mut buf).is_ok() {
                for i in (0..len.saturating_sub(7)).step_by(8) {
                    let word = usize::from_le_bytes(buf[i..i + 8].try_into().unwrap());
                    if word < low || word > high {
                        continue;
                    }
                    if let Some(t) = catalog.tables.get(&word) {
                        candidates.push((start + i, *t));
                    }
                }
            }
            start += len;
        }
    }

    let pass = Pass { driver, catalog, regions: Regions::new(maps) };
    let mut nations: HashMap<usize, String> = HashMap::new();
    let mut clubs: HashMap<usize, String> = HashMap::new();
    let mut teams: HashMap<usize, ()> = HashMap::new();
    let mut entries = Vec::new();
    let mut players = Vec::new();
    let mut staff_found = Vec::new();
    let mut linked = Vec::new();
    for &(address, t) in &candidates {
        match (t.kind, t.subobject_offset) {
            (CatalogKind::Nation, 0) => {
                if let Some((id, uid, name)) = pass.named(address, 0x18) {
                    nations.insert(address, name.clone());
                    entries.push(IndexEntry::new(EntityKind::Nation, id, uid, name, None, address));
                }
            }
            (CatalogKind::Club, 0) => {
                if let Some((id, uid, name)) = pass.named(address, 0xc0) {
                    clubs.insert(address, name.clone());
                    linked.push((EntityKind::Club, id, uid, name, address, 0xd8));
                }
            }
            (CatalogKind::Competition, 0) => {
                if let Some((id, uid, name)) = pass.named(address, 0x40) {
                    linked.push((EntityKind::Competition, id, uid, name, address, 0x60));
                }
            }
            (CatalogKind::Team, 0) => {
                if identity(driver, address + 8, false).is_some() {
                    teams.insert(address, ());
                }
            }
            (CatalogKind::Player, 0) => {
                if let Some(player) = pass.player(address) {
                    players.push(player);
                }
            }
            (CatalogKind::Staff, STAFF_SUBOBJECT) => {
                if let Some((id, uid, name)) = pass.staff(address) {
                    staff_found.push((address - STAFF_SUBOBJECT as usize, id, uid, name));
                }
            }
            _ => {}
        }
    }
    // Links resolve only to records validated in this pass.
    for (kind, id, uid, name, address, nation_offset) in linked {
        let nation = driver.read_ptr(address + nation_offset).ok().and_then(|p| nations.get(&p).cloned());
        entries.push(IndexEntry::new(kind, id, uid, name, nation, address));
    }
    for (record, id, uid, name) in players {
        let club = pass.player_club(record, &teams, &clubs);
        entries.push(IndexEntry::new(EntityKind::Player, id, uid, name, club, record));
    }
    for (record, id, uid, name) in staff_found {
        let club = pass.staff_club(record, &teams, &clubs);
        entries.push(IndexEntry::new(EntityKind::Staff, id, uid, name, club, record));
    }
    let (entries, duplicates_dropped) = drop_duplicate_ids(entries);
    Enumeration { entries, duplicates_dropped, candidates: candidates.len() }
}

/// True while the record at `entry.address` still carries the identity the index stored. A reload
/// frees or reuses the old objects, so a changed or unreadable identity marks the index stale.
pub fn entry_still_valid(driver: &LinuxMemoryDriver, entry: &IndexEntry) -> bool {
    let ids_at = match entry.kind {
        EntityKind::Player => entry.address + 0x148, // address is the record R = P + 0x148
        EntityKind::Staff => entry.address + STAFF_SUBOBJECT as usize + 8,
        _ => entry.address + 8,
    };
    driver.read::<[u32; 3]>(ids_at).is_ok_and(|ids| ids[0] as i32 == entry.id && ids[1] == entry.uid && ids[2] == entry.uid)
}

/// Resolves the catalog and enumerates the attached process. Blocking: call from a worker thread.
pub fn enumerate_process(driver: &LinuxMemoryDriver) -> Result<Enumeration> {
    let maps = parse_process_maps(driver.pid())?;
    let catalog = RttiCatalog::discover(driver, &maps)?;
    Ok(enumerate(driver, &catalog, &maps))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy() {}
    // Lives in read-only (RELRO) memory and points at code, like a real attribute-view vtable.
    static VIEW_VTABLE: [fn(); 1] = [dummy];

    const PLAYER_VT: usize = 0x5a5a_0000_1000;
    const PERSON_VT: usize = 0x5a5a_0000_1008;
    const STAFF_VT: usize = 0x5a5a_0000_1010;
    const STAFF_SECOND_VT: usize = 0x5a5a_0000_1018;
    const CLUB_VT: usize = 0x5a5a_0000_1020;
    const TEAM_VT: usize = 0x5a5a_0000_1028;
    const NATION_VT: usize = 0x5a5a_0000_1030;
    const COMP_VT: usize = 0x5a5a_0000_1038;
    const CONTRACT_VT: usize = 0x5a5a_0000_1040;

    fn catalog() -> RttiCatalog {
        let t = |kind, subobject_offset| NativeType { kind, subobject_offset };
        RttiCatalog::from_tables(HashMap::from([
            (PLAYER_VT, t(CatalogKind::Player, 0)),
            (PERSON_VT, t(CatalogKind::Player, PLAYER_PERSON_SUBOBJECT)),
            (STAFF_VT, t(CatalogKind::Staff, STAFF_SUBOBJECT)),
            (STAFF_SECOND_VT, t(CatalogKind::Staff, STAFF_SECOND_SUBOBJECT)),
            (CLUB_VT, t(CatalogKind::Club, 0)),
            (TEAM_VT, t(CatalogKind::Team, 0)),
            (NATION_VT, t(CatalogKind::Nation, 0)),
            (COMP_VT, t(CatalogKind::Competition, 0)),
            (CONTRACT_VT, t(CatalogKind::FullContract, 0)),
        ]))
    }

    /// Fake heap objects inside one 8-aligned buffer of this process.
    struct Arena(Vec<u64>);

    impl Arena {
        fn new() -> Self {
            Self(vec![0; 0x20000 / 8])
        }
        fn at(&self, offset: usize) -> usize {
            self.0.as_ptr() as usize + offset
        }
        fn bytes(&mut self) -> &mut [u8] {
            let len = self.0.len() * 8;
            unsafe { std::slice::from_raw_parts_mut(self.0.as_mut_ptr().cast::<u8>(), len) }
        }
        fn put(&mut self, offset: usize, value: &[u8]) {
            self.bytes()[offset..offset + value.len()].copy_from_slice(value);
        }
        fn ptr(&mut self, offset: usize, value: usize) {
            self.put(offset, &value.to_le_bytes());
        }
        fn ids(&mut self, offset: usize, index: u32, uid: u32) {
            for (k, v) in [index, uid, uid].iter().enumerate() {
                self.put(offset + 4 * k, &v.to_le_bytes());
            }
        }
        fn string(&mut self, offset: usize, text: &str) -> usize {
            self.put(offset, &(text.len() as u32).to_le_bytes());
            self.put(offset + 4, text.as_bytes());
            self.at(offset)
        }
        /// Pooled name: reference slot -> pointer slot -> string.
        fn pooled(&mut self, slot: usize, holder: usize, string_at: usize, text: &str) {
            let string = self.string(string_at, text);
            self.ptr(holder, string);
            let holder = self.at(holder);
            self.ptr(slot, holder);
        }
    }

    fn player(arena: &mut Arena, p: usize, index: u32, uid: u32, names: [usize; 3]) {
        let r = p + PLAYER_RECORD;
        arena.ptr(p, PLAYER_VT);
        arena.ptr(r, VIEW_VTABLE.as_ptr() as usize);
        arena.put(r + 8, &[10u8; 15]);
        arena.put(r + 0x2f, &[100, 50]);
        arena.ptr(r + 0x140, PERSON_VT);
        arena.ids(r + 0x148, index, uid);
        arena.pooled(r + 0x190, names[0], names[1], "Kylian");
        arena.pooled(r + 0x198, names[0] + 8, names[2], "Mbappé");
    }

    #[test]
    fn locator_rule_matches_fmscan() {
        assert!(is_complete_object_locator(&[1, 0, 0, 0x10, 0x20, 0x500], 0x1000_0500, 0x1000_0000));
        assert!(!is_complete_object_locator(&[0, 0, 0, 0x10, 0x20, 0x500], 0x1000_0500, 0x1000_0000));
        assert!(!is_complete_object_locator(&[1, 0, 0, 0x10, 0x20, 0x400], 0x1000_0500, 0x1000_0000));
        assert!(!is_complete_object_locator(&[1, 0, 0, 0x10, 0x20, 0x500], 0x0fff_0000, 0x1000_0000));
        assert_eq!(CatalogKind::from_type_name(".?AVCOMP@db@@"), Some(CatalogKind::Competition));
        assert_eq!(CatalogKind::from_type_name(".?AVPERSON@db@@"), None);
    }

    #[test]
    fn enumerates_synthetic_records_with_validated_links() {
        let mut a = Arena::new();
        // Nation 0x100, club 0x400, team 0x800, competition 0xa00, contract 0xc00.
        a.ptr(0x100, NATION_VT);
        a.ids(0x108, 7, 7007);
        let s = a.string(0x200, "Côte d'Ivoire");
        a.ptr(0x118, s);
        a.ptr(0x400, CLUB_VT);
        a.ids(0x408, 70, 7070);
        let s = a.string(0x300, "ASEC Mimosas");
        a.ptr(0x4c0, s);
        let nation = a.at(0x100);
        a.ptr(0x4d8, nation);
        a.ptr(0x800, TEAM_VT);
        a.put(0x808, &[1u8, 0, 0, 0, 9, 0, 0, 0, 3, 0, 0, 0]);
        let club = a.at(0x400);
        a.ptr(0x830, club);
        a.ptr(0xa00, COMP_VT);
        a.ids(0xa08, 700, 7700);
        let s = a.string(0xb00, "Ligue 1 Côte d'Ivoire");
        a.ptr(0xa40, s);
        a.ptr(0xa60, nation);
        // Player at 0x1000 with a parent contract to the team.
        player(&mut a, 0x1000, 77, 9077, [0x2000, 0x2100, 0x2200]);
        let r = a.at(0x1000 + PLAYER_RECORD);
        a.ptr(0xc00, CONTRACT_VT);
        a.ptr(0xc08, r + 0x140);
        let team = a.at(0x800);
        a.ptr(0xc10, team);
        let contract = a.at(0xc00);
        a.ptr(0x1000 + PLAYER_RECORD + 0x1e8, contract);
        // Staff sub-object at 0x3100 (record 0x3000).
        a.ptr(0x3100, STAFF_VT);
        a.ptr(0x3128, STAFF_SECOND_VT);
        a.ids(0x3108, 799, 107740);
        a.pooled(0x3150, 0x3400, 0x3500, "Igor");
        a.pooled(0x3158, 0x3408, 0x3600, "Tolić");
        // Staff contract at 0x3700: owner is the staff sub-object, team is the one the player uses.
        a.ptr(0x3700, CONTRACT_VT);
        a.ptr(0x3708, a.at(0x3100));
        a.ptr(0x3710, team);
        a.ptr(0x31a8, a.at(0x3700));
        // Rejected: a club whose uid copies disagree, and a player with an out-of-range position.
        a.ptr(0x4000, CLUB_VT);
        a.put(0x4008, &[5u8, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0]);
        player(&mut a, 0x5000, 78, 9078, [0x6000, 0x6100, 0x6200]);
        a.put(0x5000 + PLAYER_RECORD + 8, &[0u8]);

        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let maps = parse_process_maps(driver.pid()).unwrap();
        let found = enumerate(&driver, &catalog(), &maps);
        let mine: Vec<&IndexEntry> = found
            .entries
            .iter()
            .filter(|e| e.address >= a.at(0) && e.address < a.at(0x20000))
            .collect();
        let find = |kind| mine.iter().find(|e| e.kind == kind).copied();

        let player = find(EntityKind::Player).unwrap();
        assert_eq!((player.id, player.uid, player.name.as_str()), (77, 9077, "Kylian Mbappé"));
        assert_eq!(player.detail.as_deref(), Some("ASEC Mimosas"));
        let club = find(EntityKind::Club).unwrap();
        assert_eq!((club.id, club.detail.as_deref()), (70, Some("Côte d'Ivoire")));
        let comp = find(EntityKind::Competition).unwrap();
        assert_eq!((comp.id, comp.name.as_str(), comp.detail.as_deref()), (700, "Ligue 1 Côte d'Ivoire", Some("Côte d'Ivoire")));
        assert_eq!(find(EntityKind::Nation).unwrap().id, 7);
        let staff = find(EntityKind::Staff).unwrap();
        assert_eq!((staff.id, staff.uid, staff.name.as_str(), staff.address), (799, 107740, "Igor Tolić", a.at(0x3000)));
        assert_eq!(staff.detail.as_deref(), Some("ASEC Mimosas"), "staff club comes from the validated contract chain");
        assert_eq!(mine.len(), 5, "rejected candidates must not be indexed: {mine:?}");
    }

    #[test]
    fn duplicate_player_ids_are_dropped() {
        let mut a = Arena::new();
        player(&mut a, 0x1000, 4242, 94242, [0x2000, 0x2100, 0x2200]);
        player(&mut a, 0x8000, 4242, 94242, [0x9000, 0x9100, 0x9200]);
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let maps = parse_process_maps(driver.pid()).unwrap();
        let found = enumerate(&driver, &catalog(), &maps);
        assert!(!found.entries.iter().any(|e| e.kind == EntityKind::Player && e.id == 4242));
        assert!(found.duplicates_dropped >= 2);
    }

    #[test]
    fn stale_entries_are_detected_after_the_records_change() {
        let mut a = Arena::new();
        player(&mut a, 0x1000, 31, 9031, [0x2000, 0x2100, 0x2200]);
        a.ptr(0x3100, STAFF_VT);
        a.ptr(0x3128, STAFF_SECOND_VT);
        a.ids(0x3108, 5, 505);
        a.pooled(0x3150, 0x3400, 0x3500, "Igor");
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let p = IndexEntry::new(EntityKind::Player, 31, 9031, "x".into(), None, a.at(0x1000 + PLAYER_RECORD));
        let s = IndexEntry::new(EntityKind::Staff, 5, 505, "x".into(), None, a.at(0x3000));
        assert!(entry_still_valid(&driver, &p) && entry_still_valid(&driver, &s));
        a.ids(0x1000 + PLAYER_RECORD + 0x148, 32, 9032);
        a.ids(0x3108, 6, 606);
        assert!(!entry_still_valid(&driver, &p) && !entry_still_valid(&driver, &s));
        let gone = IndexEntry::new(EntityKind::Club, 1, 1, "x".into(), None, 0x10);
        assert!(!entry_still_valid(&driver, &gone));
    }

    #[test]
    fn discover_refuses_a_process_without_game_plugin() {
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let maps = parse_process_maps(driver.pid()).unwrap();
        let error = RttiCatalog::discover(&driver, &maps).unwrap_err().to_string();
        assert!(error.contains("game_plugin.dll is not mapped"), "{error}");
    }
}
