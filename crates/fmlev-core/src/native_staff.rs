//! Native staff (non-player) records, read-only.
//!
//! Layout traced on game_plugin.dll PE timestamp 0x6a229e8a with `tools/fmscan` and matched against
//! seven on-screen staff profiles (UI-binding values, zero mismatches):
//! - Record `R` holds `.?AVACTUAL_NON_PLAYER@db@@` sub-object vtables at `R+0x100` and `R+0x128`.
//! - Identity `(index, uid, uid)` at `R+0x108`; pooled first/last name pointers at `R+0x150`/`R+0x158`.
//! - 22 attributes: most are native 1..=100 bytes shown as `max(1, round(native / 5))`; Working With
//!   Youngsters (+0x1c), Authority (+0x14) and Adaptability (+0x170) are stored 1..=20.
//!
//! Information candidates (`STAFF_INFORMATION`) and the Contract tab (`read_contract`) are read-only.
//! Verified against on-screen profiles (Engonga, Pintus, Ortega; see AGENTS.md "Staff contract/info"):
//! year of birth (`R+0x18a`) and the contract link (`R+0x1a8` -> `FULL_CONTRACT`, person back-pointer
//! `+0x8 == R+0x100`, weekly wage u32 `+0x20`, expiry `(year << 16) | day-of-year` at `+0x48`).
//! Reputation slots and CA/PA stay `unavailable` until their offsets pass the evidence ladder.
//!
//! Writes (`write_attributes`) are transactional (pre-check, write, readback, rollback) but are
//! EXPERIMENTAL: they have not passed the in-game evidence ladder (screen check after a day tick and
//! save/reload on a throwaway save). Native fields are written as `value * 5`, which displays as `value`.

use fmlev_mem::{parse_process_maps, LinuxMemoryDriver};
use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::native_transfer::{apply_transaction, NativeEdit};

pub const STAFF_VTABLE_RVA: usize = 0x456_5018;
pub const STAFF_SECOND_VTABLE_RVA: usize = 0x456_4fc8;
pub const SUBOBJECT_OFFSET: usize = 0x100;
pub const SECOND_SUBOBJECT_OFFSET: usize = 0x128;
const IDENTITY_OFFSET: usize = 0x108;
const FIRST_NAME_OFFSET: usize = 0x150;
const LAST_NAME_OFFSET: usize = 0x158;
const CHUNK: usize = 16 << 20;
pub const MAX_SEARCH_RESULTS: usize = 50;

pub struct StaffAttributeDef {
    pub group: &'static str,
    pub field: &'static str,
    pub offset: usize,
    /// true: native 1..=100 shown as max(1, round(native / 5)); false: stored 1..=20.
    pub native: bool,
}

const fn def(group: &'static str, field: &'static str, offset: usize, native: bool) -> StaffAttributeDef {
    StaffAttributeDef { group, field, offset, native }
}

pub const STAFF_ATTRIBUTES: [StaffAttributeDef; 22] = [
    def("coaching", "attacking", 0x32, true),
    def("coaching", "defending", 0x33, true),
    def("coaching", "fitness", 0x34, true),
    def("coaching", "goalkeeping", 0x2b, true),
    def("coaching", "possession", 0x35, true),
    def("coaching", "set_pieces", 0x43, true),
    def("coaching", "tactical", 0x37, true),
    def("coaching", "technical", 0x36, true),
    def("coaching", "working_with_youngsters", 0x1c, false),
    def("mental", "adaptability", 0x170, false),
    def("mental", "authority", 0x14, false),
    def("mental", "determination", 0x1d, true),
    def("mental", "motivating", 0x2f, true),
    def("mental", "people_management", 0x2e, true),
    def("knowledge", "judging_player_ability", 0x2c, true),
    def("knowledge", "judging_player_potential", 0x2d, true),
    def("knowledge", "judging_staff_ability", 0x42, true),
    def("knowledge", "negotiating", 0x41, true),
    def("knowledge", "tactical_knowledge", 0x31, true),
    def("scouting", "analyzing_data", 0x3c, true),
    def("medical", "physiotherapy", 0x30, true),
    def("medical", "sports_science", 0x3f, true),
];

/// Screen value of a native 1..=100 staff attribute.
pub fn display_from_native(native: u8) -> u8 {
    ((u16::from(native) + 2) / 5).max(1) as u8
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaffAttribute {
    pub group: &'static str,
    pub field: &'static str,
    /// 1..=20 as shown in FM26; `None` when the byte is unreadable or out of range.
    pub value: Option<u8>,
    pub native: Option<u8>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaffAttributeChange {
    pub field: &'static str,
    pub old_value: u8,
    pub new_value: u8,
    pub old_byte: u8,
    pub new_byte: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeStaffRecord {
    #[serde(skip)]
    pub pid: i32,
    #[serde(serialize_with = "hex")]
    pub record: usize,
    pub index: i32,
    pub uid: i32,
    pub name: String,
}

fn hex<S: serde::Serializer>(value: &usize, s: S) -> std::result::Result<S::Ok, S::Error> {
    s.serialize_str(&format!("{value:#x}"))
}

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidValue(message.to_owned())
}

fn native_string(driver: &LinuxMemoryDriver, ptr: usize) -> Option<String> {
    if ptr <= 0x10000 {
        return None;
    }
    let len = driver.read::<u32>(ptr).ok()? as usize;
    if len == 0 || len > 256 {
        return None;
    }
    let mut buf = vec![0u8; len];
    if driver.read_bytes(ptr + 4, &mut buf).ok()? != len {
        return None;
    }
    String::from_utf8(buf).ok().filter(|s| !s.chars().any(char::is_control))
}

/// First + last name from the pooled name references; `None` if a present reference is broken.
fn read_staff_name(driver: &LinuxMemoryDriver, record: usize) -> Option<String> {
    let mut parts = Vec::new();
    for offset in [FIRST_NAME_OFFSET, LAST_NAME_OFFSET] {
        let reference = driver.read_ptr(record + offset).ok()?;
        if reference == 0 {
            continue;
        }
        parts.push(native_string(driver, driver.read_ptr(reference).ok()?)?);
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// Expected vtables: the game_plugin.dll ones, or `None` in a test process without the module.
fn expected_vtables(driver: &LinuxMemoryDriver) -> Result<Option<(usize, usize)>> {
    let maps = parse_process_maps(driver.pid())?;
    Ok(maps
        .iter()
        .filter(|m| m.pathname.ends_with("game_plugin.dll"))
        .map(|m| m.start)
        .min()
        .map(|base| (base + STAFF_VTABLE_RVA, base + STAFF_SECOND_VTABLE_RVA)))
}

impl NativeStaffRecord {
    /// Validates a candidate sub-object address (`R + 0x100`) and returns the record.
    fn at_subobject(driver: &LinuxMemoryDriver, sub: usize, vtables: (usize, usize)) -> Option<Self> {
        let record = sub.checked_sub(SUBOBJECT_OFFSET)?;
        if driver.read_ptr(sub).ok()? != vtables.0
            || driver.read_ptr(record + SECOND_SUBOBJECT_OFFSET).ok()? != vtables.1
        {
            return None;
        }
        let index = driver.read::<i32>(record + IDENTITY_OFFSET).ok()?;
        let uid = driver.read::<i32>(record + IDENTITY_OFFSET + 4).ok()?;
        if index < 0 || uid == 0 || driver.read::<i32>(record + IDENTITY_OFFSET + 8).ok()? != uid {
            return None;
        }
        let name = read_staff_name(driver, record)?;
        Some(Self { pid: driver.pid(), record, index, uid, name })
    }

    /// Scans writable private memory for staff sub-objects and keeps those `keep` accepts.
    fn scan(driver: &LinuxMemoryDriver, vtables: (usize, usize), mut keep: impl FnMut(&Self) -> bool) -> Result<Vec<Self>> {
        let maps = parse_process_maps(driver.pid())?;
        let needle = (vtables.0 as u64).to_le_bytes();
        let mut found = Vec::new();
        for map in maps.iter().filter(|m| m.is_rw_private()) {
            let mut start = map.start;
            while start < map.end {
                let len = (map.end - start).min(CHUNK);
                let mut buf = vec![0u8; len];
                if driver.read_bytes(start, &mut buf).is_ok() {
                    for i in (0..len.saturating_sub(8)).step_by(8) {
                        if buf[i..i + 8] != needle {
                            continue;
                        }
                        if let Some(staff) = Self::at_subobject(driver, start + i, vtables) {
                            if keep(&staff) {
                                found.push(staff);
                            }
                        }
                    }
                }
                start += len;
            }
        }
        Ok(found)
    }

    fn live_vtables(driver: &LinuxMemoryDriver) -> Result<(usize, usize)> {
        expected_vtables(driver)?.ok_or_else(|| invalid("game_plugin.dll is not mapped; attach to FM26 first"))
    }

    /// Finds exactly one staff record by index (cached; the cached record is re-validated).
    pub fn discover_by_index(driver: &LinuxMemoryDriver, index: i32) -> Result<Self> {
        crate::locator_cache::cached("staff", driver.pid(), index, |r: &Self| r.validate(driver).is_ok(), || Self::scan_by_index(driver, index))
    }

    /// Finds exactly one staff record by index; duplicates are refused.
    fn scan_by_index(driver: &LinuxMemoryDriver, index: i32) -> Result<Self> {
        if index < 0 {
            return Err(invalid("A staff id (index) is required"));
        }
        let vtables = Self::live_vtables(driver)?;
        Self::pick_one(Self::scan(driver, vtables, |s| s.index == index)?, index)
    }

    fn pick_one(mut found: Vec<Self>, index: i32) -> Result<Self> {
        match found.len() {
            1 => Ok(found.remove(0)),
            n => Err(invalid(&format!("Expected one native staff record for id {index}, found {n}"))),
        }
    }

    /// Case-insensitive name search, at most `max` results (capped at `MAX_SEARCH_RESULTS`).
    /// Ids seen at more than one address are dropped rather than guessed.
    pub fn search(driver: &LinuxMemoryDriver, query: &str, max: usize) -> Result<Vec<Self>> {
        let needle = Self::search_query(query)?;
        let vtables = Self::live_vtables(driver)?;
        Ok(Self::rank(Self::scan(driver, vtables, |s| s.name.to_lowercase().contains(&needle))?, max))
    }

    fn search_query(query: &str) -> Result<String> {
        let query = query.trim().to_lowercase();
        if query.chars().count() < 2 {
            return Err(invalid("Type at least 2 characters to search staff"));
        }
        Ok(query)
    }

    fn rank(mut found: Vec<Self>, max: usize) -> Vec<Self> {
        let mut counts = std::collections::HashMap::new();
        for s in &found {
            *counts.entry(s.index).or_insert(0usize) += 1;
        }
        found.retain(|s| counts[&s.index] == 1);
        found.sort_by(|a, b| a.name.cmp(&b.name).then(a.index.cmp(&b.index)));
        found.truncate(max.clamp(1, MAX_SEARCH_RESULTS));
        found
    }

    /// Re-checks identity before a read (the record may have moved after a reload).
    pub fn validate(&self, driver: &LinuxMemoryDriver) -> Result<()> {
        let ok = driver.pid() == self.pid
            && driver.read::<i32>(self.record + IDENTITY_OFFSET)? == self.index
            && driver.read::<i32>(self.record + IDENTITY_OFFSET + 4)? == self.uid
            && driver.read::<i32>(self.record + IDENTITY_OFFSET + 8)? == self.uid;
        if ok { Ok(()) } else { Err(invalid("Staff record identity changed; search again")) }
    }

    pub fn read_attributes(&self, driver: &LinuxMemoryDriver) -> Result<Vec<StaffAttribute>> {
        self.validate(driver)?;
        Ok(STAFF_ATTRIBUTES
            .iter()
            .map(|d| {
                let (value, native, reason) = match driver.read::<u8>(self.record + d.offset) {
                    Err(_) => (None, None, Some("unreadable".to_string())),
                    Ok(v) if d.native && (1..=100).contains(&v) => (Some(display_from_native(v)), Some(v), None),
                    Ok(v) if !d.native && (1..=20).contains(&v) => (Some(v), None, None),
                    Ok(v) => (None, None, Some(format!("out of range {v}"))),
                };
                StaffAttribute { group: d.group, field: d.field, value, native, reason }
            })
            .collect())
    }
}

impl NativeStaffRecord {
    /// Sets attributes to display values 1..=20 in one transaction. Attributes already showing the
    /// requested value are left untouched (their exact native byte is kept).
    pub fn write_attributes(&self, driver: &LinuxMemoryDriver, changes: &[(String, u8)]) -> Result<Vec<StaffAttributeChange>> {
        self.validate(driver)?;
        if changes.is_empty() {
            return Err(invalid("No staff attribute changes given"));
        }
        let mut edits = Vec::new();
        let mut report = Vec::new();
        for (i, (field, value)) in changes.iter().enumerate() {
            if changes[..i].iter().any(|(f, _)| f == field) {
                return Err(invalid(&format!("Staff attribute {field} given twice")));
            }
            let def = STAFF_ATTRIBUTES
                .iter()
                .find(|d| d.field == field)
                .ok_or_else(|| invalid(&format!("Unknown staff attribute {field}")))?;
            if !(1..=20).contains(value) {
                return Err(invalid(&format!("{field} must be 1..=20 (got {value})")));
            }
            let address = self.record + def.offset;
            let old_byte = driver.read::<u8>(address)?;
            let valid = if def.native { (1..=100).contains(&old_byte) } else { (1..=20).contains(&old_byte) };
            if !valid {
                return Err(invalid(&format!("{field} currently reads out of range ({old_byte}); not writing")));
            }
            let old_value = if def.native { display_from_native(old_byte) } else { old_byte };
            if old_value == *value {
                continue;
            }
            let new_byte = if def.native { value * 5 } else { *value };
            edits.push(NativeEdit::new(address, vec![old_byte], vec![new_byte], def.field));
            report.push(StaffAttributeChange { field: def.field, old_value, new_value: *value, old_byte, new_byte });
        }
        if !edits.is_empty() {
            apply_transaction(driver, &edits)?;
        }
        Ok(report)
    }
}

/// Why Information-tab candidates and the Contract tab stay unavailable (kept in one place for routes/UI/tests).
pub const UNVERIFIED_INFO_REASON: &str =
    "unverified: offset is a candidate from the task brief; no live range survey or on-screen profile comparison yet";
pub const CONTRACT_UNMAPPED_REASON: &str = "unverified: no staff contract field is mapped for this value (club job, role and national team job have no screen-checked offset)";
pub const NO_CONTRACT_REASON: &str = "no contract object is linked to this staff member (the game shows N/A)";

const CONTRACT_LINK_OFFSET: usize = 0x1a8;
const CONTRACT_PERSON_OFFSET: usize = 0x8;
const CONTRACT_WAGE_OFFSET: usize = 0x20;
const CONTRACT_EXPIRY_OFFSET: usize = 0x48;
/// `FULL_CONTRACT` vtable RVA (same constant the player transfer code resolves).
const CONTRACT_VTABLE_RVA: usize = crate::native_transfer::FULL_CONTRACT_VTABLE_RVA;
const PE_TIMESTAMP: u32 = 0x6a22_9e8a;
/// Sanity bound for a weekly wage in raw GBP; larger values are reported unavailable, not shown.
const MAX_WEEKLY_WAGE: u32 = 10_000_000;

/// A staff Information field read from the record. `verified` is flipped to true only after the evidence
/// ladder (live range survey plus match against the open staff profile for several people).
pub struct StaffInfoDef {
    pub field: &'static str,
    pub label: &'static str,
    pub offset: usize,
    pub lo: u16,
    pub hi: u16,
    pub verified: bool,
}

const fn info(field: &'static str, label: &'static str, offset: usize, lo: u16, hi: u16) -> StaffInfoDef {
    StaffInfoDef { field, label, offset, lo, hi, verified: false }
}

const fn verified_info(field: &'static str, label: &'static str, offset: usize, lo: u16, hi: u16) -> StaffInfoDef {
    StaffInfoDef { field, label, offset, lo, hi, verified: true }
}

/// u16 candidates. Reputation meaning is unknown, so the labels stay neutral.
pub const STAFF_INFORMATION: [StaffInfoDef; 6] = [
    info("reputation_0xd4", "Reputation (slot 0xd4, meaning unknown)", 0xd4, 0, 10_000),
    info("reputation_0xd6", "Reputation (slot 0xd6, meaning unknown)", 0xd6, 0, 10_000),
    info("reputation_0xd8", "Reputation (slot 0xd8, meaning unknown)", 0xd8, 0, 10_000),
    info("current_ability", "Current ability", 0xda, 0, 200),
    info("potential_ability", "Potential ability", 0xdc, 0, 200),
    verified_info("birth_year", "Year of birth", 0x18a, 1900, 2100),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaffInfoField {
    pub field: &'static str,
    pub label: &'static str,
    /// `verified` fields carry a value; everything else is `unavailable` with a reason (never a default).
    pub status: &'static str,
    pub value: Option<u32>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaffContractInfo {
    pub status: &'static str,
    pub reason: String,
    pub fields: Vec<StaffInfoField>,
}

impl NativeStaffRecord {
    fn read_info_with(&self, driver: &LinuxMemoryDriver, defs: &[StaffInfoDef]) -> Result<Vec<StaffInfoField>> {
        self.validate(driver)?;
        Ok(defs
            .iter()
            .map(|d| {
                let unavailable = |reason: String| StaffInfoField { field: d.field, label: d.label, status: "unavailable", value: None, reason: Some(reason) };
                if !d.verified {
                    return unavailable(UNVERIFIED_INFO_REASON.to_string());
                }
                match driver.read::<u16>(self.record + d.offset) {
                    Err(_) => unavailable("unreadable".to_string()),
                    Ok(v) if (d.lo..=d.hi).contains(&v) => StaffInfoField { field: d.field, label: d.label, status: "verified", value: Some(u32::from(v)), reason: None },
                    Ok(v) => unavailable(format!("out of range {v}")),
                }
            })
            .collect())
    }

    /// Information-tab fields beyond the 22 attributes. Only fields flagged `verified` return values.
    pub fn read_information(&self, driver: &LinuxMemoryDriver) -> Result<Vec<StaffInfoField>> {
        self.read_info_with(driver, &STAFF_INFORMATION)
    }

    /// Contract-tab read. The contract link, wage and expiry are screen-verified; job, role and national
    /// team job are not mapped and stay unavailable. A null link means "no contract" (the game shows N/A).
    pub fn read_contract(&self, driver: &LinuxMemoryDriver) -> Result<StaffContractInfo> {
        self.validate(driver)?;
        let unavailable = |field: &'static str, label: &'static str, reason: &str| StaffInfoField {
            field,
            label,
            status: "unavailable",
            value: None,
            reason: Some(reason.to_string()),
        };
        let unmapped = |fields: &mut Vec<StaffInfoField>| {
            for (field, label) in [("club_job", "Club job"), ("role", "Role"), ("national_team_job", "National team job")] {
                fields.push(unavailable(field, label, CONTRACT_UNMAPPED_REASON));
            }
        };
        let mut fields = Vec::new();
        let (status, reason) = match self.read_contract_values(driver) {
            Ok(None) => {
                fields.push(unavailable("weekly_wage", "Wage", NO_CONTRACT_REASON));
                fields.push(unavailable("contract_expiry", "Expiry", NO_CONTRACT_REASON));
                ("no_contract", NO_CONTRACT_REASON.to_string())
            }
            Ok(Some((wage, year, day))) => {
                let ok = |field: &'static str, label: &'static str, value: u32| StaffInfoField {
                    field,
                    label,
                    status: "verified",
                    value: Some(value),
                    reason: None,
                };
                fields.push(ok("weekly_wage", "Wage (raw GBP per week)", wage));
                fields.push(ok("contract_expiry", "Expiry year", u32::from(year)));
                fields.push(ok("contract_expiry_day", "Expiry day of year", u32::from(day)));
                ("ok", "wage and expiry verified against on-screen profiles".to_string())
            }
            Err(why) => {
                fields.push(unavailable("weekly_wage", "Wage", &why));
                fields.push(unavailable("contract_expiry", "Expiry", &why));
                ("unavailable", why)
            }
        };
        unmapped(&mut fields);
        Ok(StaffContractInfo { status, reason, fields })
    }

    /// `Ok(None)` = no contract linked, `Ok(Some((weekly wage, expiry year, day of year)))`, `Err(reason)` =
    /// the link failed a guard (reported unavailable, never guessed).
    fn read_contract_values(&self, driver: &LinuxMemoryDriver) -> std::result::Result<Option<(u32, u16, u16)>, String> {
        let base = parse_process_maps(driver.pid())
            .ok()
            .and_then(|maps| maps.iter().filter(|m| m.pathname.ends_with("game_plugin.dll")).map(|m| m.start).min());
        if let Some(base) = base {
            let pe = driver.read::<u32>(base + 0x3c).map_err(|e| e.to_string())? as usize;
            let stamp = driver.read::<u32>(base + pe + 8).map_err(|e| e.to_string())?;
            if pe > 0x1000 || stamp != PE_TIMESTAMP {
                return Err(format!("unknown game_plugin.dll build (PE timestamp {stamp:#x}); contract layout is unverified for it"));
            }
        }
        let link = driver.read_ptr(self.record + CONTRACT_LINK_OFFSET).map_err(|e| format!("contract link unreadable: {e}"))?;
        if link == 0 {
            return Ok(None);
        }
        if let Some(base) = base {
            let vtable = driver.read_ptr(link).map_err(|e| format!("contract object unreadable: {e}"))?;
            if vtable != base + CONTRACT_VTABLE_RVA {
                return Err("linked object is not a FULL_CONTRACT (vtable mismatch)".to_string());
            }
        }
        let person = driver.read_ptr(link + CONTRACT_PERSON_OFFSET).map_err(|e| format!("contract owner unreadable: {e}"))?;
        if person != self.record + SUBOBJECT_OFFSET {
            return Err("contract owner back-pointer does not match this staff member".to_string());
        }
        let wage = driver.read::<u32>(link + CONTRACT_WAGE_OFFSET).map_err(|e| e.to_string())?;
        let expiry = driver.read::<u32>(link + CONTRACT_EXPIRY_OFFSET).map_err(|e| e.to_string())?;
        let (year, day) = ((expiry >> 16) as u16, (expiry & 0x1ff) as u16);
        if wage > MAX_WEEKLY_WAGE || !(1900..=2100).contains(&year) || !(1..=366).contains(&day) {
            return Err(format!("contract values out of range (wage {wage}, expiry year {year}, day {day})"));
        }
        Ok(Some((wage, year, day)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_birth_year_is_verified_and_no_link_means_no_contract() {
        let mut f = fake(97, 1197, "Info", "Test");
        put(&mut f.record, 0xda, 120u16);
        put(&mut f.record, 0x18a, 1971u16);
        let staff = read(&f).unwrap();
        let d = driver();
        let fields = staff.read_information(&d).unwrap();
        assert_eq!(fields.len(), 6);
        for x in &fields {
            if x.field == "birth_year" {
                assert_eq!((x.status, x.value), ("verified", Some(1971)));
            } else {
                assert!(x.status == "unavailable" && x.value.is_none() && x.reason.as_deref() == Some(UNVERIFIED_INFO_REASON), "{x:?}");
            }
        }
        let contract = staff.read_contract(&d).unwrap();
        assert_eq!(contract.status, "no_contract");
        assert!(contract.fields.iter().all(|x| x.value.is_none() && x.status == "unavailable"));
        put(&mut f.record, IDENTITY_OFFSET + 4, 1i32);
        assert!(staff.read_information(&d).is_err());
        assert!(staff.read_contract(&d).is_err());
    }

    fn contract_bytes(owner: usize, wage: u32, expiry: u32) -> Vec<u8> {
        let mut c = vec![0u8; 0xc8];
        put(&mut c, CONTRACT_PERSON_OFFSET, owner);
        put(&mut c, CONTRACT_WAGE_OFFSET, wage);
        put(&mut c, CONTRACT_EXPIRY_OFFSET, expiry);
        c
    }

    fn contract_status(owner: Option<usize>, wage: u32, expiry: u32) -> StaffContractInfo {
        let mut f = fake(99, 1199, "Con", "Tract");
        let staff = read(&f).unwrap();
        let contract = contract_bytes(owner.unwrap_or(f.sub()), wage, expiry);
        put(&mut f.record, CONTRACT_LINK_OFFSET, contract.as_ptr() as usize);
        // `staff.record` points at `f.record`, so the link written above is visible to the driver.
        let info = staff.read_contract(&driver()).unwrap();
        drop(contract);
        info
    }

    #[test]
    fn contract_wage_and_expiry_are_read_after_the_owner_check() {
        // Ortega (Real Madrid physio): £345K p/a = 6641 per week, expiry 30 Jun 2027 = year 2027, day 181.
        let info = contract_status(None, 6641, (2027 << 16) | 181);
        assert_eq!(info.status, "ok", "{info:?}");
        let value = |name: &str| info.fields.iter().find(|x| x.field == name).and_then(|x| x.value);
        assert_eq!(value("weekly_wage"), Some(6641));
        assert_eq!(value("contract_expiry"), Some(2027));
        assert_eq!(value("contract_expiry_day"), Some(181));
        let job = info.fields.iter().find(|x| x.field == "club_job").unwrap();
        assert_eq!((job.status, job.value), ("unavailable", None), "job/role stay unmapped");
    }

    #[test]
    fn contract_with_foreign_owner_or_impossible_values_is_unavailable() {
        let foreign = contract_status(Some(0x1234_5678), 6641, (2027 << 16) | 181);
        assert_eq!(foreign.status, "unavailable");
        assert!(foreign.reason.contains("back-pointer"), "{}", foreign.reason);
        let wage = contract_status(None, MAX_WEEKLY_WAGE + 1, (2027 << 16) | 181);
        assert_eq!(wage.status, "unavailable");
        let year = contract_status(None, 100, (1500 << 16) | 181);
        assert_eq!(year.status, "unavailable");
        let day = contract_status(None, 100, (2027 << 16) | 0);
        assert_eq!(day.status, "unavailable");
        assert!(foreign.fields.iter().all(|x| x.value.is_none()));
    }

    #[test]
    fn verified_information_fields_are_range_checked() {
        let mut f = fake(98, 1198, "Info", "Verified");
        put(&mut f.record, 0xda, 120u16);
        put(&mut f.record, 0xdc, 500u16);
        let staff = read(&f).unwrap();
        let defs = [
            StaffInfoDef { verified: true, ..info("current_ability", "CA", 0xda, 0, 200) },
            StaffInfoDef { verified: true, ..info("potential_ability", "PA", 0xdc, 0, 200) },
            info("birth_year", "Birth", 0x18a, 1900, 2100),
        ];
        let fields = staff.read_info_with(&driver(), &defs).unwrap();
        assert_eq!((fields[0].status, fields[0].value), ("verified", Some(120)));
        assert_eq!((fields[1].status, fields[1].value), ("unavailable", None));
        assert_eq!(fields[1].reason.as_deref(), Some("out of range 500"));
        assert_eq!(fields[2].status, "unavailable");
    }

    #[test]
    fn information_table_is_unique_and_only_birth_year_is_verified() {
        for (i, a) in STAFF_INFORMATION.iter().enumerate() {
            assert_eq!(a.verified, a.field == "birth_year", "{} verification must match the recorded live evidence", a.field);
            assert!(STAFF_INFORMATION[i + 1..].iter().all(|b| b.field != a.field && b.offset != a.offset));
        }
    }

    // Read-only statics stand in for the DLL vtables in a test process.
    static VT1: u64 = 0x1111;
    static VT2: u64 = 0x2222;

    fn vtables() -> (usize, usize) {
        (std::ptr::addr_of!(VT1) as usize, std::ptr::addr_of!(VT2) as usize)
    }

    struct Fake {
        record: Vec<u8>,
        _strings: Vec<Box<[u8]>>,
        _refs: Box<[usize; 2]>,
    }

    impl Fake {
        fn sub(&self) -> usize {
            self.record.as_ptr() as usize + SUBOBJECT_OFFSET
        }
    }

    fn put<T: Copy>(buf: &mut [u8], at: usize, v: T) {
        let bytes = unsafe { std::slice::from_raw_parts(&v as *const T as *const u8, std::mem::size_of::<T>()) };
        buf[at..at + bytes.len()].copy_from_slice(bytes);
    }

    fn fake(index: i32, uid: i32, first: &str, last: &str) -> Fake {
        let text = |s: &str| {
            let mut b = (s.len() as u32).to_le_bytes().to_vec();
            b.extend_from_slice(s.as_bytes());
            b.into_boxed_slice()
        };
        let strings = vec![text(first), text(last)];
        let refs = Box::new([strings[0].as_ptr() as usize, strings[1].as_ptr() as usize]);
        let mut record = vec![0u8; 0x240];
        put(&mut record, SUBOBJECT_OFFSET, vtables().0);
        put(&mut record, SECOND_SUBOBJECT_OFFSET, vtables().1);
        put(&mut record, IDENTITY_OFFSET, index);
        put(&mut record, IDENTITY_OFFSET + 4, uid);
        put(&mut record, IDENTITY_OFFSET + 8, uid);
        put(&mut record, FIRST_NAME_OFFSET, &refs[0] as *const usize as usize);
        put(&mut record, LAST_NAME_OFFSET, &refs[1] as *const usize as usize);
        for d in &STAFF_ATTRIBUTES {
            record[d.offset] = if d.native { 50 } else { 10 };
        }
        Fake { record, _strings: strings, _refs: refs }
    }

    fn driver() -> LinuxMemoryDriver {
        LinuxMemoryDriver::new(std::process::id() as i32)
    }

    fn read(f: &Fake) -> Option<NativeStaffRecord> {
        NativeStaffRecord::at_subobject(&driver(), f.sub(), vtables())
    }

    #[test]
    fn display_rounds_to_nearest_with_minimum_one() {
        assert_eq!(display_from_native(1), 1);
        assert_eq!(display_from_native(16), 3);
        assert_eq!(display_from_native(52), 10);
        assert_eq!(display_from_native(89), 18);
        assert_eq!(display_from_native(100), 20);
    }

    #[test]
    fn attribute_table_has_unique_offsets() {
        for (i, a) in STAFF_ATTRIBUTES.iter().enumerate() {
            assert!(STAFF_ATTRIBUTES[i + 1..].iter().all(|b| b.offset != a.offset && b.field != a.field), "{}", a.field);
        }
    }

    #[test]
    fn reads_identity_name_and_attributes() {
        let mut f = fake(79, 1120, "Pep", "Guardiola");
        f.record[0x32] = 89;
        f.record[0x41] = 1;
        f.record[0x14] = 15;
        let staff = read(&f).unwrap();
        assert_eq!((staff.name.as_str(), staff.index, staff.uid), ("Pep Guardiola", 79, 1120));
        assert_eq!(staff.record, f.record.as_ptr() as usize);
        let attrs = staff.read_attributes(&driver()).unwrap();
        let get = |field: &str| attrs.iter().find(|a| a.field == field).unwrap().clone();
        assert_eq!((get("attacking").value, get("attacking").native), (Some(18), Some(89)));
        assert_eq!(get("negotiating").value, Some(1));
        assert_eq!((get("authority").value, get("authority").native), (Some(15), None));
        assert_eq!(attrs.len(), 22);
    }

    #[test]
    fn out_of_range_bytes_are_reported_not_invented() {
        let mut f = fake(81, 1130, "Range", "Check");
        f.record[0x32] = 0;
        f.record[0x14] = 25;
        let attrs = read(&f).unwrap().read_attributes(&driver()).unwrap();
        for field in ["attacking", "authority"] {
            let a = attrs.iter().find(|a| a.field == field).unwrap();
            assert_eq!(a.value, None, "{field}");
            assert!(a.reason.as_deref().unwrap().starts_with("out of range"));
        }
    }

    #[test]
    fn records_without_owner_vtable_matching_uid_or_name_are_rejected() {
        let mut no_owner = fake(82, 1140, "No", "Owner");
        put(&mut no_owner.record, SECOND_SUBOBJECT_OFFSET, 0usize);
        let mut bad_uid = fake(83, 1150, "Bad", "Uid");
        put(&mut bad_uid.record, IDENTITY_OFFSET + 8, 1i32);
        let mut no_name = fake(85, 1170, "No", "Name");
        put(&mut no_name.record, FIRST_NAME_OFFSET, 0usize);
        put(&mut no_name.record, LAST_NAME_OFFSET, 0usize);
        assert!(read(&no_owner).is_none());
        assert!(read(&bad_uid).is_none());
        assert!(read(&no_name).is_none());
    }

    #[test]
    fn identity_change_blocks_attribute_reads() {
        let mut f = fake(84, 1160, "Moving", "Record");
        let staff = read(&f).unwrap();
        put(&mut f.record, IDENTITY_OFFSET + 4, 9999i32);
        assert!(staff.read_attributes(&driver()).is_err());
    }

    #[test]
    fn duplicates_are_refused_and_search_ranks_unique_ids() {
        let a = read(&fake(90, 1, "Ann", "Coach")).unwrap();
        let b = read(&fake(91, 2, "Bob", "Coach")).unwrap();
        let copy = NativeStaffRecord { record: a.record + 0x1000, ..a.clone() };
        assert!(NativeStaffRecord::pick_one(vec![a.clone(), copy.clone()], 90).is_err());
        assert!(NativeStaffRecord::pick_one(vec![], 90).is_err());
        assert_eq!(NativeStaffRecord::pick_one(vec![b.clone()], 91).unwrap().index, 91);
        let ranked = NativeStaffRecord::rank(vec![b.clone(), a.clone(), copy], 10);
        assert_eq!(ranked.iter().map(|s| s.index).collect::<Vec<_>>(), vec![91], "ambiguous id 90 dropped");
        assert!(NativeStaffRecord::search_query("g").is_err());
        assert_eq!(NativeStaffRecord::search_query("  GuArd ").unwrap(), "guard");
    }

    #[test]
    fn writes_display_values_and_skips_unchanged() {
        let mut f = fake(95, 1195, "Write", "Test");
        f.record[0x37] = 28; // tactical shows 6
        f.record[0x14] = 9; // authority raw
        f.record[0x32] = 89; // attacking shows 18
        let staff = read(&f).unwrap();
        let d = driver();
        let report = staff
            .write_attributes(&d, &[("tactical".into(), 10), ("authority".into(), 12), ("attacking".into(), 18)])
            .unwrap();
        assert_eq!(report.len(), 2, "attacking already shows 18");
        assert_eq!((report[0].old_value, report[0].new_value, report[0].new_byte), (6, 10, 50));
        assert_eq!(f.record[0x37], 50);
        assert_eq!(f.record[0x14], 12);
        assert_eq!(f.record[0x32], 89, "unchanged attribute keeps its exact native byte");
        let attrs = staff.read_attributes(&d).unwrap();
        assert_eq!(attrs.iter().find(|a| a.field == "tactical").unwrap().value, Some(10));
        assert_eq!(staff.write_attributes(&d, &[("tactical".into(), 10)]).unwrap(), vec![]);
    }

    #[test]
    fn invalid_writes_change_nothing() {
        let mut f = fake(96, 1196, "Bad", "Write");
        f.record[0x2b] = 0; // goalkeeping out of range
        let before = f.record.clone();
        let staff = read(&f).unwrap();
        let d = driver();
        for changes in [
            vec![("tactical".to_string(), 0u8)],
            vec![("tactical".to_string(), 21)],
            vec![("ambition".to_string(), 10)],
            vec![("tactical".to_string(), 10), ("tactical".to_string(), 11)],
            vec![("tactical".to_string(), 12), ("goalkeeping".to_string(), 5)],
            vec![],
        ] {
            assert!(staff.write_attributes(&d, &changes).is_err(), "{changes:?}");
            assert_eq!(f.record, before, "{changes:?} must not write anything");
        }
        put(&mut f.record, IDENTITY_OFFSET + 4, 1i32);
        assert!(staff.write_attributes(&d, &[("tactical".into(), 12)]).is_err());
        assert_eq!(f.record[0x37], 50);
    }
}
