//! Native awards (`.?AVAWARD@db@@`, vtable resolved by RTTI). Read path plus experimental writes.
//!
//! Live evidence (one build, PE timestamp 0x6a229e8a, 3,884 objects with contiguous indexes 0..=3883):
//! - +8 i32 index, +0xc and +0x10 the unique id (twice), +0x30 / +0x38 pointers to length-prefixed
//!   UTF-8 strings (name, short name; many awards point both at the same buffer).
//! - The community lead (FM26 Studio) puts reputation at +0x64, type at +0x69 and recipient type at
//!   +0x70. On this build those bytes do NOT look like that: +0x64 reads only 0 or 1, +0x69 is 0 for
//!   all but seven awards, and +0x70 is the low byte of a pointer. They are therefore not exposed.
//! - Bytes that look like reputation-style values: +0x91 and +0x92 (u8, 0..=200, for example
//!   200/200 for the World Golden Ball, 170/170 for the World Player of the Year, 0 for many
//!   league awards). Their meaning is NOT proven and they are labelled candidates everywhere.
//!   +0x81 is a small award-kind code (37, 3, 4, 255 ...), shown read-only.
//!
//! Writes are experimental, scalar only, through `apply_transaction`: the two candidate bytes, and
//! names rewritten in place within their existing length (the length prefix shrinks, the terminator
//! moves, nothing is allocated and no pointer changes). A buffer shared by name and short name
//! changes both.

use fmlev_mem::{parse_process_maps, LinuxMemoryDriver};
use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::native_competition_extra::Field;
use crate::native_rtti::{read_len_string, ModuleImage};
use crate::native_transfer::{apply_transaction, NativeEdit};

const AWARD_TYPE: &str = ".?AVAWARD@db@@";
const INDEX: usize = 8;
const UID: usize = 0xc;
const UID_SECOND: usize = 0x10;
const NAME: usize = 0x30;
const SHORT_NAME: usize = 0x38;
const KIND_CODE: usize = 0x81;
const CANDIDATE_A: usize = 0x91;
const CANDIDATE_B: usize = 0x92;
const MAX_NAME: usize = 250;
const MAX_AWARDS: i32 = 100_000;
const CHUNK: usize = 16 << 20;
pub const MAX_CANDIDATE: u8 = 200;

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidValue(message.to_owned())
}

/// `(address, index)` of every validated award object found in the process.
pub type AwardList = Vec<(usize, i32)>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AwardSummary {
    pub index: i32,
    pub uid: i32,
    pub name: Option<String>,
    pub short_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AwardInfo {
    pub index: i32,
    pub uid: i32,
    pub address: usize,
    pub name: Field<String>,
    pub short_name: Field<String>,
    /// True when name and short name point at the same string buffer (writing one changes both).
    pub names_share_buffer: bool,
    /// Longest text a name write may use (the current byte length).
    pub name_capacity: Option<usize>,
    pub short_name_capacity: Option<usize>,
    pub kind_code: Field<u8>,
    pub reputation_candidate_a: Field<u8>,
    pub reputation_candidate_b: Field<u8>,
    pub status: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AwardEdits {
    pub reputation_candidate_a: Option<u8>,
    pub reputation_candidate_b: Option<u8>,
    pub name: Option<String>,
    pub short_name: Option<String>,
}

impl AwardEdits {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Object-level identity check: index, uid twice, and readable name strings. `vtable` pins the type.
fn identity(driver: &LinuxMemoryDriver, address: usize, vtable: Option<usize>) -> Option<(i32, i32)> {
    if address <= 0x10000 || address % 8 != 0 {
        return None;
    }
    if let Some(vtable) = vtable {
        if driver.read_ptr(address).ok()? != vtable {
            return None;
        }
    }
    let index = driver.read::<i32>(address + INDEX).ok()?;
    let (uid, again) = (driver.read::<i32>(address + UID).ok()?, driver.read::<i32>(address + UID_SECOND).ok()?);
    if !(0..MAX_AWARDS).contains(&index) || uid == 0 || uid != again {
        return None;
    }
    let name = driver.read_ptr(address + NAME).ok()?;
    read_len_string(driver, name, MAX_NAME)?;
    Some((index, uid))
}

fn scan_awards(driver: &LinuxMemoryDriver, vtable: usize) -> Result<AwardList> {
    let maps = parse_process_maps(driver.pid())?;
    let mut found: AwardList = Vec::new();
    let pattern = vtable.to_le_bytes();
    for map in maps.iter().filter(|m| m.is_rw_private()) {
        let mut start = map.start;
        while start < map.end {
            let len = (map.end - start).min(CHUNK);
            let mut buf = vec![0u8; len];
            if driver.read_bytes(start, &mut buf).is_ok() {
                for i in (0..len.saturating_sub(7)).step_by(8) {
                    if buf[i..i + 8] == pattern {
                        if let Some((index, _)) = identity(driver, start + i, Some(vtable)) {
                            found.push((start + i, index));
                        }
                    }
                }
            }
            start += len;
        }
    }
    // An index found twice is ambiguous: drop it instead of picking one.
    found.sort_by_key(|(_, index)| *index);
    let mut unique: AwardList = Vec::with_capacity(found.len());
    let mut i = 0;
    while i < found.len() {
        let mut j = i;
        while j < found.len() && found[j].1 == found[i].1 {
            j += 1;
        }
        if j - i == 1 {
            unique.push(found[i]);
        }
        i = j;
    }
    if unique.is_empty() {
        return Err(invalid("No award objects found (load a save first)"));
    }
    Ok(unique)
}

fn award_vtable(driver: &LinuxMemoryDriver, image: &ModuleImage) -> Result<usize> {
    crate::locator_cache::cached(
        "award_vtable",
        driver.pid(),
        0,
        |vtable: &usize| image.type_of_vtable(driver, *vtable).as_deref() == Some(AWARD_TYPE),
        || image.find_vtable(driver, AWARD_TYPE),
    )
}

/// All awards, cached per process; a cached list is reused while a sample of its entries still validates.
pub fn award_list(driver: &LinuxMemoryDriver) -> Result<AwardList> {
    let image = ModuleImage::discover(driver)?.ok_or_else(|| invalid("game_plugin.dll is not mapped; attach to FM26 with a save loaded first"))?;
    let vtable = award_vtable(driver, &image)?;
    crate::locator_cache::cached(
        "award_list",
        driver.pid(),
        0,
        |list: &AwardList| {
            let n = list.len();
            [0, n / 3, 2 * n / 3, n.saturating_sub(1)]
                .iter()
                .all(|&i| list.get(i).is_some_and(|&(address, index)| identity(driver, address, Some(vtable)).is_some_and(|(found, _)| found == index)))
        },
        || scan_awards(driver, vtable),
    )
}

/// Name/short-name search (case-insensitive substring) or exact index/uid, paged. Returns `(total, page)`.
pub fn search_awards(driver: &LinuxMemoryDriver, query: &str, offset: usize, limit: usize) -> Result<(usize, Vec<AwardSummary>)> {
    let list = award_list(driver)?;
    let needle = query.trim().to_lowercase();
    let number = needle.parse::<i64>().ok();
    let mut matches = Vec::new();
    for &(address, index) in &list {
        let Some((_, uid)) = identity(driver, address, None) else { continue };
        let name = driver.read_ptr(address + NAME).ok().and_then(|p| read_len_string(driver, p, MAX_NAME));
        let short_name = driver.read_ptr(address + SHORT_NAME).ok().and_then(|p| read_len_string(driver, p, MAX_NAME));
        let hit = needle.is_empty()
            || number.is_some_and(|n| n == i64::from(index) || n == i64::from(uid))
            || name.as_deref().is_some_and(|n| n.to_lowercase().contains(&needle))
            || short_name.as_deref().is_some_and(|n| n.to_lowercase().contains(&needle));
        if hit {
            matches.push(AwardSummary { index, uid, name, short_name });
        }
    }
    let total = matches.len();
    Ok((total, matches.into_iter().skip(offset).take(limit).collect()))
}

/// Address of the award `index`/`uid` after the identity check.
pub fn locate_award(driver: &LinuxMemoryDriver, index: i32, uid: i32) -> Result<usize> {
    let list = award_list(driver)?;
    let &(address, _) = list.iter().find(|(_, i)| *i == index).ok_or_else(|| invalid(&format!("Award {index} was not found")))?;
    match identity(driver, address, None) {
        Some((found, found_uid)) if found == index && found_uid == uid => Ok(address),
        Some((_, found_uid)) => Err(invalid(&format!("Award {index} has uid {found_uid}, not {uid}"))),
        None => Err(invalid("Award object failed identity validation")),
    }
}

fn string_field(driver: &LinuxMemoryDriver, address: usize, offset: usize) -> (Field<String>, Option<usize>, usize) {
    let pointer = driver.read_ptr(address + offset).unwrap_or(0);
    match read_len_string(driver, pointer, MAX_NAME) {
        Some(text) => {
            let capacity = text.len();
            (Field { value: Some(text), reason: None }, Some(capacity), pointer)
        }
        None => (Field { value: None, reason: Some("string pointer unreadable".to_owned()) }, None, pointer),
    }
}

fn byte_field(driver: &LinuxMemoryDriver, address: usize, offset: usize, ok: impl Fn(u8) -> bool) -> Field<u8> {
    match driver.read::<u8>(address + offset) {
        Ok(v) if ok(v) => Field { value: Some(v), reason: None },
        Ok(v) => Field { value: None, reason: Some(format!("value {v} is outside the range seen on the live survey")) },
        Err(e) => Field { value: None, reason: Some(format!("unreadable: {e}")) },
    }
}

/// Reads one award object that already passed `locate_award`.
pub fn read_award(driver: &LinuxMemoryDriver, address: usize) -> Result<AwardInfo> {
    let (index, uid) = identity(driver, address, None).ok_or_else(|| invalid("Award object failed identity validation"))?;
    let (name, name_capacity, name_ptr) = string_field(driver, address, NAME);
    let (short_name, short_capacity, short_ptr) = string_field(driver, address, SHORT_NAME);
    Ok(AwardInfo {
        index,
        uid,
        address,
        name,
        short_name,
        names_share_buffer: name_ptr == short_ptr && name_ptr != 0,
        name_capacity,
        short_name_capacity: short_capacity,
        kind_code: byte_field(driver, address, KIND_CODE, |_| true),
        reputation_candidate_a: byte_field(driver, address, CANDIDATE_A, |v| v <= MAX_CANDIDATE),
        reputation_candidate_b: byte_field(driver, address, CANDIDATE_B, |v| v <= MAX_CANDIDATE),
        status: "read_live_on_one_build_candidates_unverified_in_game",
    })
}

/// Edits that rewrite a length-prefixed string in place (`new` no longer than the current text).
fn string_edits(driver: &LinuxMemoryDriver, address: usize, offset: usize, new: &str, label: &'static str) -> Result<Vec<NativeEdit>> {
    if new.is_empty() || new.chars().any(char::is_control) {
        return Err(invalid("Names must be non-empty text without control characters"));
    }
    let pointer = driver.read_ptr(address + offset)?;
    let current = read_len_string(driver, pointer, MAX_NAME).ok_or_else(|| invalid("The current name is unreadable; refusing to rewrite it"))?;
    if new.len() > current.len() {
        return Err(invalid(&format!(
            "The new text is {} bytes but the existing buffer holds {}; names can only be rewritten within their current length",
            new.len(),
            current.len()
        )));
    }
    // Rewrite length prefix, text and terminator; the old tail stays behind the new terminator.
    let span = 4 + current.len() + 1;
    let mut old = vec![0u8; span];
    driver.read_bytes(pointer, &mut old)?;
    let mut next = old.clone();
    next[..4].copy_from_slice(&(new.len() as u32).to_le_bytes());
    next[4..4 + new.len()].copy_from_slice(new.as_bytes());
    next[4 + new.len()] = 0;
    Ok(vec![NativeEdit::new(pointer, old, next, label)])
}

/// Experimental scalar and in-place name writes in one transaction. Returns the changed labels.
pub fn write_award(driver: &LinuxMemoryDriver, address: usize, edits: &AwardEdits) -> Result<Vec<&'static str>> {
    if edits.is_empty() {
        return Err(invalid("No award fields to change"));
    }
    ModuleImage::discover(driver)?;
    identity(driver, address, None).ok_or_else(|| invalid("Award object failed identity validation"))?;
    let mut native = Vec::new();
    for (offset, value, label) in [
        (CANDIDATE_A, edits.reputation_candidate_a, "award reputation candidate A"),
        (CANDIDATE_B, edits.reputation_candidate_b, "award reputation candidate B"),
    ] {
        if let Some(value) = value {
            if value > MAX_CANDIDATE {
                return Err(invalid("Award reputation candidates must be between 0 and 200"));
            }
            native.push(NativeEdit::new(address + offset, vec![driver.read::<u8>(address + offset)?], vec![value], label));
        }
    }
    let shared = driver.read_ptr(address + NAME)? == driver.read_ptr(address + SHORT_NAME)?;
    if shared {
        if let (Some(a), Some(b)) = (&edits.name, &edits.short_name) {
            if a != b {
                return Err(invalid("Name and short name share one buffer; give the same text for both or change only one"));
            }
        }
    }
    if let Some(name) = &edits.name {
        native.extend(string_edits(driver, address, NAME, name, "award name")?);
    }
    if let Some(short) = &edits.short_name {
        if !(shared && edits.name.as_deref() == Some(short.as_str())) {
            native.extend(string_edits(driver, address, SHORT_NAME, short, "award short name")?);
        }
    }
    let changed = native.iter().filter(|e| e.old != e.next).map(|e| e.label).collect();
    apply_transaction(driver, &native)?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn driver() -> LinuxMemoryDriver {
        LinuxMemoryDriver::new(std::process::id() as i32)
    }

    fn buffer(text: &str) -> Vec<u8> {
        let mut b = vec![0u8; 64];
        b[..4].copy_from_slice(&(text.len() as u32).to_le_bytes());
        b[4..4 + text.len()].copy_from_slice(text.as_bytes());
        b[4 + text.len() + 1] = b'X'; // the pool continues behind the terminator
        b
    }

    struct Fake {
        object: Vec<u64>,
        name: Vec<u8>,
        short: Vec<u8>,
    }

    fn fake(name: &str, short: Option<&str>) -> Fake {
        let mut object = vec![0u64; 0x20];
        let mut name = buffer(name);
        let mut short = short.map(buffer);
        let base = object.as_mut_ptr() as usize;
        let d = driver();
        d.write(base + INDEX, &42i32).unwrap();
        d.write(base + UID, &102_450i32).unwrap();
        d.write(base + UID_SECOND, &102_450i32).unwrap();
        d.write(base + NAME, &(name.as_mut_ptr() as usize)).unwrap();
        let short_ptr = short.as_mut().map_or(name.as_mut_ptr() as usize, |s| s.as_mut_ptr() as usize);
        d.write(base + SHORT_NAME, &short_ptr).unwrap();
        d.write(base + KIND_CODE, &37u8).unwrap();
        d.write(base + CANDIDATE_A, &200u8).unwrap();
        d.write(base + CANDIDATE_B, &170u8).unwrap();
        Fake { object, name, short: short.unwrap_or_default() }
    }

    fn address(f: &mut Fake) -> usize {
        f.object.as_mut_ptr() as usize
    }

    #[test]
    fn award_read_reports_names_candidates_and_shared_buffers() {
        let mut f = fake("World Golden Ball", None);
        let a = address(&mut f);
        let info = read_award(&driver(), a).unwrap();
        assert_eq!((info.index, info.uid), (42, 102_450));
        assert_eq!(info.name.value.as_deref(), Some("World Golden Ball"));
        assert!(info.names_share_buffer);
        assert_eq!(info.name_capacity, Some(17));
        assert_eq!(info.reputation_candidate_a.value, Some(200));
        assert_eq!(info.kind_code.value, Some(37));
        driver().write(a + CANDIDATE_B, &201u8).unwrap();
        let odd = read_award(&driver(), a).unwrap();
        assert_eq!(odd.reputation_candidate_b.value, None);
        assert!(odd.reputation_candidate_b.reason.is_some());
        // A broken identity is rejected, not reported.
        driver().write(a + UID_SECOND, &7i32).unwrap();
        assert!(read_award(&driver(), a).is_err());
        drop(f.short);
        drop(f.name);
    }

    #[test]
    fn scalar_and_in_place_name_writes_are_transactional() {
        let mut f = fake("Player of the Year", Some("POTY"));
        let a = address(&mut f);
        let d = driver();
        let edits = AwardEdits { reputation_candidate_a: Some(150), short_name: Some("PY".into()), name: Some("Player of Year".into()), ..Default::default() };
        let changed = write_award(&d, a, &edits).unwrap();
        assert_eq!(changed, vec!["award reputation candidate A", "award name", "award short name"]);
        let info = read_award(&d, a).unwrap();
        assert_eq!(info.name.value.as_deref(), Some("Player of Year"));
        assert_eq!(info.short_name.value.as_deref(), Some("PY"));
        assert_eq!(info.reputation_candidate_a.value, Some(150));
        // The byte behind the old terminator (the next pooled string) is untouched.
        assert_eq!(f.name[4 + 18 + 1], b'X');
        // Too long, empty, control characters and out-of-range values are refused without any write.
        for bad in [
            AwardEdits { name: Some("A much longer award name than before".into()), ..Default::default() },
            AwardEdits { name: Some(String::new()), ..Default::default() },
            AwardEdits { short_name: Some("a\nb".into()), ..Default::default() },
            AwardEdits { reputation_candidate_b: Some(201), ..Default::default() },
            AwardEdits::default(),
        ] {
            assert!(write_award(&d, a, &bad).is_err());
        }
        assert_eq!(read_award(&d, a).unwrap().name.value.as_deref(), Some("Player of Year"));
    }

    #[test]
    fn shared_name_buffers_refuse_conflicting_texts() {
        let mut f = fake("Golden Ball", None);
        let a = address(&mut f);
        let d = driver();
        let conflict = AwardEdits { name: Some("Gold Ball".into()), short_name: Some("Golden".into()), ..Default::default() };
        assert!(write_award(&d, a, &conflict).unwrap_err().to_string().contains("share one buffer"));
        let same = AwardEdits { name: Some("Gold Ball".into()), short_name: Some("Gold Ball".into()), ..Default::default() };
        assert_eq!(write_award(&d, a, &same).unwrap(), vec!["award name"]);
        let info = read_award(&d, a).unwrap();
        assert_eq!(info.short_name.value.as_deref(), Some("Gold Ball"));
    }
}
