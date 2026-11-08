//! Club sponsor deals. Read path plus EXPERIMENTAL scalar value edits, unverified in game.
//!
//! Layout (lead from the community tool FM26 Studio, re-read live on PID 98701 for 4,235 clubs; see AGENTS.md):
//! - Pointer at club +0x130 to a list object whose first three qwords are the vector (begin, end, capacity) of pointers
//!   to 0x18-byte sponsor records. A null pointer means the club has no list (20,143 of 24,378 clubs).
//! - Record: +0 start day u16 (low 9 bits day of year), +2 start year u16, +4 end day u16, +6 end year u16,
//!   +8 total value u32 (raw GBP over the whole deal), +0xc annual value u32 (present for about 14% of deals, 0 otherwise;
//!   total / annual is a whole number of years in every record that has it), +0x14 type code u16 (low byte = type,
//!   bit 0x100 = unmapped flag). Bytes +0x10..+0x13 and +0x16..+0x17 are unmapped and ignored. No sponsor name is stored here.
//!
//! Rule 7: the list is never resized (one community routine shrinks it; that is forbidden here). Only the two u32 values
//! of an existing record can be edited.

use serde::Serialize;

use fmlev_mem::LinuxMemoryDriver;

use crate::error::{CoreError, Result};
use crate::native_club::NativeClubRecord;
use crate::native_transfer::{apply_transaction, NativeEdit};

const SPONSORS_OFFSET: usize = 0x130;
pub const RECORD_SIZE: usize = 0x18;
pub const MAX_SPONSORS: usize = 64;
const DAY_MASK: u16 = 0x1ff;
const MAX_VALUE: u64 = i32::MAX as u64;

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidValue(message.to_owned())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClubSponsor {
    pub index: usize,
    pub address: String,
    pub start_day: u16,
    pub start_year: u16,
    pub end_day: u16,
    pub end_year: u16,
    pub total_value: u32,
    /// None when the record stores no annual value (the game then derives it; not mapped).
    pub annual_value: Option<u32>,
    pub type_code: u16,
    pub type_base: u8,
    pub type_flag_0x100: bool,
    /// Community label, plausibility only (Barcelona 10-year Nike-like deal = 22, Real Madrid 65M/yr kit sponsor = 1).
    pub type_label_candidate: Option<&'static str>,
    pub type_label_status: &'static str,
    /// Heuristic note: some small-club records end before they start; they are reported as stored.
    pub dates_inverted: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClubSponsors {
    /// False when the club has no sponsor list object at all.
    pub list_present: bool,
    pub sponsors: Vec<ClubSponsor>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SponsorEdit {
    pub annual_value: Option<u32>,
    pub total_value: Option<u32>,
}

fn label_for(base: u8) -> Option<&'static str> {
    Some(match base {
        1 => "Kit sponsor",
        2 => "Government / council grant",
        3 => "Stadium sponsor",
        4 => "General sponsor",
        5 => "Individual TV deal",
        6 => "Other income",
        7 => "Club membership",
        10 => "Parachute payment",
        16 => "Continental competition",
        17 => "Equity injection",
        18 => "Total commercial income",
        19 => "Shirt sleeve",
        22 => "Kit manufacturer",
        23 => "Stadium",
        _ => return None,
    })
}

fn decode(index: usize, address: usize, raw: &[u8; RECORD_SIZE]) -> Result<ClubSponsor> {
    let u32_at = |at: usize| u32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
    let u16_at = |at: usize| u16::from_le_bytes(raw[at..at + 2].try_into().unwrap());
    let (start_day, start_year, end_day, end_year) = (u16_at(0) & DAY_MASK, u16_at(2), u16_at(4) & DAY_MASK, u16_at(6));
    let (total, annual, code) = (u32_at(8), u32_at(0xc), u16_at(0x14));
    let base = (code & 0xff) as u8;
    let years_ok = [start_year, end_year].iter().all(|y| (1900..=2200).contains(y));
    if u64::from(total) > MAX_VALUE || u64::from(annual) > MAX_VALUE || !years_ok || start_day > 366 || end_day > 366 || !(1..=23).contains(&base) {
        return Err(invalid(&format!("Sponsor record {index} at 0x{address:x} failed range checks; refusing to read the list")));
    }
    Ok(ClubSponsor {
        index,
        address: format!("0x{address:x}"),
        start_day,
        start_year,
        end_day,
        end_year,
        total_value: total,
        annual_value: (annual != 0).then_some(annual),
        type_code: code,
        type_base: base,
        type_flag_0x100: code & 0x100 != 0,
        type_label_candidate: label_for(base),
        type_label_status: "unverified",
        dates_inverted: (end_year, end_day) < (start_year, start_day),
    })
}

pub fn read_sponsors_at(driver: &LinuxMemoryDriver, club: usize) -> Result<ClubSponsors> {
    let list = driver.read_ptr(club + SPONSORS_OFFSET)?;
    if list == 0 {
        return Ok(ClubSponsors { list_present: false, sponsors: Vec::new() });
    }
    if list <= 0x10000 {
        return Err(invalid("Sponsor list pointer is not valid"));
    }
    let (begin, end, capacity) = (driver.read_ptr(list)?, driver.read_ptr(list + 8)?, driver.read_ptr(list + 16)?);
    if begin == 0 && end == 0 {
        return Ok(ClubSponsors { list_present: true, sponsors: Vec::new() });
    }
    if begin <= 0x10000 || end < begin || capacity < end || (end - begin) % 8 != 0 {
        return Err(invalid("Sponsor list vector failed its shape checks"));
    }
    let count = (end - begin) / 8;
    if count > MAX_SPONSORS {
        return Err(invalid("Sponsor list is implausibly long; refusing to read it"));
    }
    let mut sponsors = Vec::with_capacity(count);
    for index in 0..count {
        let pointer = driver.read_ptr(begin + index * 8)?;
        if pointer <= 0x10000 {
            return Err(invalid(&format!("Sponsor {index} has a null record pointer")));
        }
        let mut raw = [0u8; RECORD_SIZE];
        if driver.read_bytes(pointer, &mut raw)? != RECORD_SIZE {
            return Err(invalid("Short read of a sponsor record"));
        }
        sponsors.push(decode(index, pointer, &raw)?);
    }
    let mut addresses: Vec<&str> = sponsors.iter().map(|s| s.address.as_str()).collect();
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.len() != sponsors.len() {
        return Err(invalid("Sponsor list holds the same record twice"));
    }
    Ok(ClubSponsors { list_present: true, sponsors })
}

/// Pure planner. When only `annual_value` is given, the total is rescaled by the deal's existing whole number of years
/// (total / annual, tolerance of one unit per year); if that ratio is not whole the caller must give `total_value` too.
pub fn plan_sponsor_edits(sponsor: &ClubSponsor, edit: &SponsorEdit) -> Result<Vec<NativeEdit>> {
    let base = usize::from_str_radix(sponsor.address.trim_start_matches("0x"), 16).map_err(|_| invalid("Bad sponsor address"))?;
    let check = |name: &str, v: u32| -> Result<u32> {
        if u64::from(v) > MAX_VALUE {
            Err(invalid(&format!("{name} must be 0..=2,147,483,647")))
        } else {
            Ok(v)
        }
    };
    let mut new_annual = None;
    let mut new_total = None;
    if let Some(annual) = edit.annual_value {
        let annual = check("annual_value", annual)?;
        let old_annual = sponsor
            .annual_value
            .ok_or_else(|| invalid("This deal stores no annual value, so it cannot be edited; set total_value instead"))?;
        new_annual = Some(annual);
        if edit.total_value.is_none() {
            let ratio = (f64::from(sponsor.total_value) / f64::from(old_annual)).round() as u64;
            let expected = ratio * u64::from(old_annual);
            if ratio == 0 || expected.abs_diff(u64::from(sponsor.total_value)) > ratio {
                return Err(invalid("Total is not a whole number of years of the annual value; give total_value as well"));
            }
            let rescaled = ratio * u64::from(annual);
            if rescaled > MAX_VALUE {
                return Err(invalid("The rescaled total would exceed 2,147,483,647"));
            }
            new_total = Some(rescaled as u32);
        }
    }
    if let Some(total) = edit.total_value {
        new_total = Some(check("total_value", total)?);
    }
    let mut edits = Vec::new();
    if let Some(total) = new_total {
        edits.push(NativeEdit::new(base + 8, sponsor.total_value.to_le_bytes().to_vec(), total.to_le_bytes().to_vec(), "sponsor total value"));
    }
    if let Some(annual) = new_annual {
        edits.push(NativeEdit::new(base + 0xc, sponsor.annual_value.unwrap_or(0).to_le_bytes().to_vec(), annual.to_le_bytes().to_vec(), "sponsor annual value"));
    }
    if edits.is_empty() {
        return Err(invalid("Nothing to change: give annual_value and/or total_value"));
    }
    Ok(edits)
}

/// EXPERIMENTAL scalar value edit of sponsor number `sponsor_index` (0-based); `expected_address` guards against a changed list.
pub fn write_sponsor_at(
    driver: &LinuxMemoryDriver,
    club: usize,
    sponsor_index: usize,
    expected_address: Option<&str>,
    edit: &SponsorEdit,
) -> Result<ClubSponsor> {
    let list = read_sponsors_at(driver, club)?;
    let sponsor = list
        .sponsors
        .get(sponsor_index)
        .ok_or_else(|| invalid(&format!("This club has {} sponsor deal(s); sponsor_index {sponsor_index} does not exist", list.sponsors.len())))?;
    if let Some(expected) = expected_address {
        if !expected.eq_ignore_ascii_case(&sponsor.address) {
            return Err(invalid("The sponsor list changed since it was read; reload the sponsors and retry"));
        }
    }
    let edits = plan_sponsor_edits(sponsor, edit)?;
    apply_transaction(driver, &edits)?;
    let after = read_sponsors_at(driver, club)?;
    after.sponsors.get(sponsor_index).cloned().ok_or_else(|| invalid("Sponsor list changed after the write"))
}

impl NativeClubRecord {
    /// Sponsor deals of this club (read only; unverified against the game's Finances screen).
    pub fn read_sponsors(&self, driver: &LinuxMemoryDriver) -> Result<ClubSponsors> {
        self.validate(driver)?;
        read_sponsors_at(driver, self.club)
    }

    /// EXPERIMENTAL scalar sponsor value edit; see [`write_sponsor_at`].
    pub fn write_sponsor(&self, driver: &LinuxMemoryDriver, sponsor_index: usize, expected_address: Option<&str>, edit: &SponsorEdit) -> Result<ClubSponsor> {
        self.validate(driver)?;
        write_sponsor_at(driver, self.club, sponsor_index, expected_address, edit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(total: u32, annual: u32, years: (u16, u16), code: u16) -> [u8; RECORD_SIZE] {
        let mut r = [0u8; RECORD_SIZE];
        r[0..2].copy_from_slice(&182u16.to_le_bytes());
        r[2..4].copy_from_slice(&years.0.to_le_bytes());
        r[4..6].copy_from_slice(&181u16.to_le_bytes());
        r[6..8].copy_from_slice(&years.1.to_le_bytes());
        r[8..12].copy_from_slice(&total.to_le_bytes());
        r[12..16].copy_from_slice(&annual.to_le_bytes());
        r[0x14..0x16].copy_from_slice(&code.to_le_bytes());
        r
    }

    struct Fake {
        _club: Vec<u8>,
        records: Vec<[u8; RECORD_SIZE]>,
        _vector: Vec<usize>,
        _list: Vec<usize>,
        club: usize,
    }

    fn fake(records: Vec<[u8; RECORD_SIZE]>, with_list: bool) -> Fake {
        let mut club = vec![0u8; 0x200];
        let vector: Vec<usize> = records.iter().map(|r| r.as_ptr() as usize).collect();
        let (b, e) = (vector.as_ptr() as usize, vector.as_ptr() as usize + vector.len() * 8);
        let list = vec![b, e, e, 0, 0];
        if with_list {
            club[SPONSORS_OFFSET..SPONSORS_OFFSET + 8].copy_from_slice(&(list.as_ptr() as usize).to_le_bytes());
        }
        Fake { club: club.as_ptr() as usize, _club: club, records, _vector: vector, _list: list }
    }

    fn driver() -> LinuxMemoryDriver {
        LinuxMemoryDriver::new(std::process::id() as i32)
    }

    #[test]
    fn reads_values_types_and_missing_list() {
        let f = fake(vec![record(259_262_032, 64_815_508, (2022, 2026), 257), record(587_660, 0, (2008, 2024), 1)], true);
        let read = read_sponsors_at(&driver(), f.club).unwrap();
        assert!(read.list_present);
        let s = &read.sponsors;
        assert_eq!((s[0].annual_value, s[0].type_base, s[0].type_flag_0x100, s[0].type_label_candidate), (Some(64_815_508), 1, true, Some("Kit sponsor")));
        assert_eq!((s[1].annual_value, s[1].type_flag_0x100, s[1].start_day), (None, false, 182));
        assert_eq!(s[0].type_label_status, "unverified");
        let none = fake(vec![], false);
        let read = read_sponsors_at(&driver(), none.club).unwrap();
        assert!(!read.list_present && read.sponsors.is_empty());
    }

    #[test]
    fn corrupt_records_fail_closed() {
        let f = fake(vec![record(5, 0, (1500, 2026), 4)], true);
        assert!(read_sponsors_at(&driver(), f.club).is_err());
        let g = fake(vec![record(5, 0, (2020, 2026), 99)], true);
        assert!(read_sponsors_at(&driver(), g.club).is_err());
        let h = fake(vec![record(u32::MAX, 0, (2020, 2026), 4)], true);
        assert!(read_sponsors_at(&driver(), h.club).is_err());
    }

    #[test]
    fn annual_edit_rescales_the_total_by_whole_years_and_nothing_else_changes() {
        let f = fake(vec![record(855_564_736, 85_556_473, (2018, 2028), 278), record(1000, 0, (2024, 2026), 4), record(100, 30, (2024, 2027), 4)], true);
        let d = driver();
        let after = write_sponsor_at(&d, f.club, 0, None, &SponsorEdit { annual_value: Some(90_000_000), total_value: None }).unwrap();
        assert_eq!((after.annual_value, after.total_value), (Some(90_000_000), 900_000_000));
        assert_eq!(read_sponsors_at(&d, f.club).unwrap().sponsors.len(), 3, "list length is untouched");
        assert_eq!(f.records[0][0x14], 22);
        // No stored annual value: only total_value can be edited.
        assert!(write_sponsor_at(&d, f.club, 1, None, &SponsorEdit { annual_value: Some(5), total_value: None }).is_err());
        let t = write_sponsor_at(&d, f.club, 1, None, &SponsorEdit { annual_value: None, total_value: Some(2000) }).unwrap();
        assert_eq!((t.total_value, t.annual_value), (2000, None));
        // 100 / 30 is not whole years: refuse unless total is given as well.
        assert!(write_sponsor_at(&d, f.club, 2, None, &SponsorEdit { annual_value: Some(40), total_value: None }).is_err());
        let both = write_sponsor_at(&d, f.club, 2, None, &SponsorEdit { annual_value: Some(40), total_value: Some(120) }).unwrap();
        assert_eq!((both.annual_value, both.total_value), (Some(40), 120));
        // Rejections.
        assert!(write_sponsor_at(&d, f.club, 0, None, &SponsorEdit::default()).is_err());
        assert!(write_sponsor_at(&d, f.club, 0, None, &SponsorEdit { annual_value: None, total_value: Some(u32::MAX) }).is_err());
        assert!(write_sponsor_at(&d, f.club, 9, None, &SponsorEdit { annual_value: None, total_value: Some(1) }).is_err());
        assert!(write_sponsor_at(&d, f.club, 0, Some("0x1"), &SponsorEdit { annual_value: None, total_value: Some(1) }).is_err());
        assert!(write_sponsor_at(&d, f.club, 0, None, &SponsorEdit { annual_value: Some(i32::MAX as u32), total_value: None }).is_err(), "rescaled total overflow");
    }
}
