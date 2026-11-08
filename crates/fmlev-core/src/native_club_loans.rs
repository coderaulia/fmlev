//! Club bank loans (debts). Read path plus EXPERIMENTAL scalar edits, unverified in game.
//!
//! Layout (found with the community tool FM26 Studio as a lead, then re-read live on PID 98701; see AGENTS.md):
//! - Club details subrecord at club +0x100 (the same one `native_club` validates through the founding year at +0xd0).
//! - Loan list vector at details +0x48 / +0x50 / +0x58 (begin, end, capacity) of pointers to 0x1c-byte records.
//! - Record: +0 original debt u32, +4 monthly repayment u32, +8 monthly interest u32, +0xc conditional repayment u32,
//!   +0x10 end day u16 (low 9 bits day of year, upper bits are flags), +0x12 end year u16, +0x14 start day u16, +0x16 start
//!   year u16, +0x18 loan type u8 (1..=16, meaning unmapped), +0x19 include-in-FFP u8, +0x1b interest-only u8.
//!
//! Rule 7: this module only ever writes scalar fields inside an existing record. It never changes the vector, so a loan can
//! never be removed; "clear" sets the amounts to 0.

use serde::Serialize;

use fmlev_mem::LinuxMemoryDriver;

use crate::error::{CoreError, Result};
use crate::native_club::NativeClubRecord;
use crate::native_transfer::{apply_transaction, NativeEdit};

const DETAILS_OFFSET: usize = 0x100;
const FOUNDING_YEAR_OFFSET: usize = 0xd0;
const VECTOR_OFFSET: usize = 0x48;
pub const RECORD_SIZE: usize = 0x1c;
pub const MAX_LOANS: usize = 64;
const DAY_MASK: u16 = 0x1ff;
const MAX_AMOUNT: u64 = i32::MAX as u64;

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidValue(message.to_owned())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClubLoan {
    pub index: usize,
    /// Record address in the game process (hex), so a caller can notice that the list changed.
    pub address: String,
    pub original_debt: u32,
    pub monthly_repayment: u32,
    pub monthly_interest: u32,
    pub conditional_repayment: u32,
    pub start_day: u16,
    pub start_year: u16,
    pub end_day: u16,
    pub end_year: u16,
    /// 1..=16. The meaning of each code is not mapped; do not guess labels.
    pub loan_type: u8,
    pub include_in_ffp: bool,
    pub interest_only: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoanEdit {
    pub original_debt: Option<u32>,
    pub monthly_repayment: Option<u32>,
    pub monthly_interest: Option<u32>,
    pub conditional_repayment: Option<u32>,
    pub start_day: Option<u16>,
    pub start_year: Option<u16>,
    pub end_day: Option<u16>,
    pub end_year: Option<u16>,
    pub loan_type: Option<u8>,
    pub include_in_ffp: Option<bool>,
    pub interest_only: Option<bool>,
    /// Sets original debt, repayments, interest and conditional repayment to 0 (the list itself is never shortened).
    pub clear: bool,
}

/// Address of the loan vector header (details + 0x48) after checking the details subrecord.
fn vector_header(driver: &LinuxMemoryDriver, club: usize) -> Result<usize> {
    let details = driver.read_ptr(club + DETAILS_OFFSET)?;
    if details <= 0x10000 {
        return Err(invalid("Club details subrecord not mapped; loans unavailable"));
    }
    // Founding year 0 = unknown (4,157 live clubs); anything else must be a real year.
    let year = driver.read::<u16>(details + FOUNDING_YEAR_OFFSET)?;
    if year != 0 && !(1800..=2200).contains(&year) {
        return Err(invalid("Club details subrecord failed its founding-year guard; loans unavailable"));
    }
    Ok(details + VECTOR_OFFSET)
}

fn decode(index: usize, address: usize, raw: &[u8; RECORD_SIZE]) -> Result<ClubLoan> {
    let u32_at = |at: usize| u32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
    let u16_at = |at: usize| u16::from_le_bytes(raw[at..at + 2].try_into().unwrap());
    let day = |at: usize| u16_at(at) & DAY_MASK;
    let loan = ClubLoan {
        index,
        address: format!("0x{address:x}"),
        original_debt: u32_at(0),
        monthly_repayment: u32_at(4),
        monthly_interest: u32_at(8),
        conditional_repayment: u32_at(0xc),
        end_day: day(0x10),
        end_year: u16_at(0x12),
        start_day: day(0x14),
        start_year: u16_at(0x16),
        loan_type: raw[0x18],
        include_in_ffp: raw[0x19] == 1,
        interest_only: raw[0x1b] == 1,
    };
    let amounts = [loan.original_debt, loan.monthly_repayment, loan.monthly_interest, loan.conditional_repayment];
    let ok = amounts.iter().all(|&v| u64::from(v) <= MAX_AMOUNT)
        && (1..=366).contains(&loan.start_day)
        && (1..=366).contains(&loan.end_day)
        && (1900..=2200).contains(&loan.start_year)
        && (1900..=2200).contains(&loan.end_year)
        && (1..=16).contains(&loan.loan_type)
        && raw[0x19] <= 1
        && raw[0x1b] <= 1;
    if ok {
        Ok(loan)
    } else {
        Err(invalid(&format!("Loan record {index} at 0x{address:x} failed range checks; refusing to read the list")))
    }
}

/// Reads the loan list of the club at `club`. An empty (or never allocated) list returns `[]`.
pub fn read_loans_at(driver: &LinuxMemoryDriver, club: usize) -> Result<Vec<ClubLoan>> {
    let header = vector_header(driver, club)?;
    let begin = driver.read_ptr(header)?;
    let end = driver.read_ptr(header + 8)?;
    let capacity = driver.read_ptr(header + 16)?;
    if begin == 0 && end == 0 {
        return Ok(Vec::new());
    }
    if begin <= 0x10000 || end < begin || capacity < end || (end - begin) % 8 != 0 {
        return Err(invalid("Loan list vector failed its shape checks"));
    }
    let count = (end - begin) / 8;
    if count > MAX_LOANS {
        return Err(invalid("Loan list is implausibly long; refusing to read it"));
    }
    let mut loans = Vec::with_capacity(count);
    for index in 0..count {
        let pointer = driver.read_ptr(begin + index * 8)?;
        if pointer <= 0x10000 {
            return Err(invalid(&format!("Loan {index} has a null record pointer")));
        }
        if loans.iter().any(|l: &ClubLoan| l.address == format!("0x{pointer:x}")) {
            return Err(invalid("Loan list holds the same record twice"));
        }
        let mut raw = [0u8; RECORD_SIZE];
        if driver.read_bytes(pointer, &mut raw)? != RECORD_SIZE {
            return Err(invalid("Short read of a loan record"));
        }
        loans.push(decode(index, pointer, &raw)?);
    }
    Ok(loans)
}

fn with_day(old_word: u16, day: u16) -> u16 {
    (old_word & !DAY_MASK) | day
}

/// Builds the edits for one loan record; pure so it can be unit-tested without a process.
pub fn plan_loan_edits(loan: &ClubLoan, raw: &[u8; RECORD_SIZE], edit: &LoanEdit) -> Result<Vec<NativeEdit>> {
    let base = usize::from_str_radix(loan.address.trim_start_matches("0x"), 16).map_err(|_| invalid("Bad loan address"))?;
    let mut edits = Vec::new();
    let mut put = |at: usize, len: usize, next: Vec<u8>, label: &'static str| {
        edits.push(NativeEdit::new(base + at, raw[at..at + len].to_vec(), next, label));
    };
    let amount = |name: &str, v: u32| -> Result<u32> {
        if u64::from(v) > MAX_AMOUNT {
            Err(invalid(&format!("{name} must be 0..=2,147,483,647")))
        } else {
            Ok(v)
        }
    };
    if edit.clear
        && (edit.original_debt.is_some() || edit.monthly_repayment.is_some() || edit.monthly_interest.is_some() || edit.conditional_repayment.is_some())
    {
        return Err(invalid("Use either clear or explicit amounts, not both"));
    }
    let zero = |v: Option<u32>| if edit.clear { Some(0) } else { v };
    for (at, value, label) in [
        (0usize, zero(edit.original_debt), "loan original debt"),
        (4, zero(edit.monthly_repayment), "loan monthly repayment"),
        (8, zero(edit.monthly_interest), "loan monthly interest"),
        (0xc, zero(edit.conditional_repayment), "loan conditional repayment"),
    ] {
        if let Some(v) = value {
            put(at, 4, amount(label, v)?.to_le_bytes().to_vec(), label);
        }
    }
    let day_edit = |name: &str, day: u16| -> Result<u16> {
        if (1..=366).contains(&day) {
            Ok(day)
        } else {
            Err(invalid(&format!("{name} must be a day of the year 1..=366")))
        }
    };
    let year_edit = |name: &str, year: u16| -> Result<u16> {
        if (1900..=2200).contains(&year) {
            Ok(year)
        } else {
            Err(invalid(&format!("{name} must be 1900..=2200")))
        }
    };
    let word = |at: usize| u16::from_le_bytes(raw[at..at + 2].try_into().unwrap());
    if let Some(d) = edit.end_day {
        put(0x10, 2, with_day(word(0x10), day_edit("end_day", d)?).to_le_bytes().to_vec(), "loan end day");
    }
    if let Some(y) = edit.end_year {
        put(0x12, 2, year_edit("end_year", y)?.to_le_bytes().to_vec(), "loan end year");
    }
    if let Some(d) = edit.start_day {
        put(0x14, 2, with_day(word(0x14), day_edit("start_day", d)?).to_le_bytes().to_vec(), "loan start day");
    }
    if let Some(y) = edit.start_year {
        put(0x16, 2, year_edit("start_year", y)?.to_le_bytes().to_vec(), "loan start year");
    }
    if let Some(t) = edit.loan_type {
        if !(1..=16).contains(&t) {
            return Err(invalid("loan_type must be 1..=16"));
        }
        put(0x18, 1, vec![t], "loan type");
    }
    if let Some(flag) = edit.include_in_ffp {
        put(0x19, 1, vec![u8::from(flag)], "loan include in FFP");
    }
    if let Some(flag) = edit.interest_only {
        put(0x1b, 1, vec![u8::from(flag)], "loan interest only");
    }
    let planned_end = (edit.end_year.unwrap_or(loan.end_year), edit.end_day.unwrap_or(loan.end_day));
    let planned_start = (edit.start_year.unwrap_or(loan.start_year), edit.start_day.unwrap_or(loan.start_day));
    if planned_end < planned_start {
        return Err(invalid("The loan would end before it starts"));
    }
    if edits.is_empty() {
        return Err(invalid("Nothing to change: give at least one field or clear"));
    }
    Ok(edits)
}

/// EXPERIMENTAL scalar edit of loan number `loan_index` (0-based). `expected_address`, when given, must match the
/// record currently at that index (the list is re-read first, so a changed list is refused instead of mis-edited).
pub fn write_loan_at(
    driver: &LinuxMemoryDriver,
    club: usize,
    loan_index: usize,
    expected_address: Option<&str>,
    edit: &LoanEdit,
) -> Result<ClubLoan> {
    let loans = read_loans_at(driver, club)?;
    let loan = loans.get(loan_index).ok_or_else(|| invalid(&format!("This club has {} loan(s); loan_index {loan_index} does not exist", loans.len())))?;
    if let Some(expected) = expected_address {
        if !expected.eq_ignore_ascii_case(&loan.address) {
            return Err(invalid("The loan list changed since it was read; reload the loans and retry"));
        }
    }
    let address = usize::from_str_radix(loan.address.trim_start_matches("0x"), 16).map_err(|_| invalid("Bad loan address"))?;
    let mut raw = [0u8; RECORD_SIZE];
    if driver.read_bytes(address, &mut raw)? != RECORD_SIZE {
        return Err(invalid("Short read of a loan record"));
    }
    let edits = plan_loan_edits(loan, &raw, edit)?;
    apply_transaction(driver, &edits)?;
    let after = read_loans_at(driver, club)?;
    after.get(loan_index).cloned().ok_or_else(|| invalid("Loan list changed after the write"))
}

impl NativeClubRecord {
    /// Bank loans of this club (read only; unverified against the game's Finances screen).
    pub fn read_loans(&self, driver: &LinuxMemoryDriver) -> Result<Vec<ClubLoan>> {
        self.validate(driver)?;
        read_loans_at(driver, self.club)
    }

    /// EXPERIMENTAL scalar loan edit; see [`write_loan_at`].
    pub fn write_loan(&self, driver: &LinuxMemoryDriver, loan_index: usize, expected_address: Option<&str>, edit: &LoanEdit) -> Result<ClubLoan> {
        self.validate(driver)?;
        write_loan_at(driver, self.club, loan_index, expected_address, edit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        _club: Vec<u8>,
        _details: Vec<u8>,
        _records: Vec<[u8; RECORD_SIZE]>,
        _vector: Vec<usize>,
        club: usize,
        details: usize,
        vector: usize,
    }

    fn record(debt: u32, repay: u32, end: (u16, u16), start: (u16, u16), loan_type: u8) -> [u8; RECORD_SIZE] {
        let mut r = [0u8; RECORD_SIZE];
        r[0..4].copy_from_slice(&debt.to_le_bytes());
        r[4..8].copy_from_slice(&repay.to_le_bytes());
        r[0x10..0x12].copy_from_slice(&end.0.to_le_bytes());
        r[0x12..0x14].copy_from_slice(&end.1.to_le_bytes());
        r[0x14..0x16].copy_from_slice(&start.0.to_le_bytes());
        r[0x16..0x18].copy_from_slice(&start.1.to_le_bytes());
        r[0x18] = loan_type;
        r[0x19] = 1;
        r
    }

    fn fake(records: Vec<[u8; RECORD_SIZE]>) -> Fake {
        let mut club = vec![0u8; 0x200];
        let mut details = vec![0u8; 0x200];
        let records = records;
        let vector: Vec<usize> = records.iter().map(|r| r.as_ptr() as usize).collect();
        let (begin, end) = if vector.is_empty() { (0, 0) } else { (vector.as_ptr() as usize, vector.as_ptr() as usize + vector.len() * 8) };
        details[0xd0..0xd2].copy_from_slice(&1902u16.to_le_bytes());
        details[0x48..0x50].copy_from_slice(&begin.to_le_bytes());
        details[0x50..0x58].copy_from_slice(&end.to_le_bytes());
        details[0x58..0x60].copy_from_slice(&end.to_le_bytes());
        let d = details.as_ptr() as usize;
        club[0x100..0x108].copy_from_slice(&d.to_le_bytes());
        Fake { club: club.as_ptr() as usize, details: d, vector: vector.as_ptr() as usize, _club: club, _details: details, _records: records, _vector: vector }
    }

    fn driver() -> LinuxMemoryDriver {
        LinuxMemoryDriver::new(std::process::id() as i32)
    }

    #[test]
    fn reads_loans_and_masks_the_day_flags() {
        // Day word 0x1AC1: flags 0x1a000, day 0xc1 = 193 (seen on live records).
        let f = fake(vec![record(454_572_768, 2_165_357, (181, 2051), (0x1ac1 | 0x0200, 2026), 1), record(10, 1, (365, 2030), (1, 2025), 16)]);
        let loans = read_loans_at(&driver(), f.club).unwrap();
        assert_eq!(loans.len(), 2);
        assert_eq!((loans[0].original_debt, loans[0].end_year, loans[0].start_day), (454_572_768, 2051, 193));
        assert!(loans[0].include_in_ffp && !loans[0].interest_only);
        assert_eq!(loans[1].loan_type, 16);
        let _ = (f.details, f.vector);
    }

    #[test]
    fn empty_and_broken_lists_fail_closed() {
        let f = fake(vec![]);
        assert!(read_loans_at(&driver(), f.club).unwrap().is_empty());
        // Out-of-range loan type.
        let bad = fake(vec![record(5, 1, (181, 2030), (1, 2025), 0)]);
        assert!(read_loans_at(&driver(), bad.club).unwrap_err().to_string().contains("range checks"));
        // Details guard: wrong founding year.
        let mut g = fake(vec![]);
        g._details[0xd0..0xd2].copy_from_slice(&1000u16.to_le_bytes());
        assert!(read_loans_at(&driver(), g.club).is_err());
        // Null details.
        let mut h = fake(vec![]);
        h._club[0x100..0x108].fill(0);
        assert!(read_loans_at(&driver(), h.club).is_err());
    }

    #[test]
    fn edits_are_scalar_keep_flag_bits_and_reject_bad_input() {
        let f = fake(vec![record(1000, 10, (181, 2030), (0x0200 | 100, 2025), 1)]);
        let d = driver();
        let before = read_loans_at(&d, f.club).unwrap();
        let edit = LoanEdit { monthly_repayment: Some(55), start_day: Some(120), end_year: Some(2040), include_in_ffp: Some(false), ..Default::default() };
        let after = write_loan_at(&d, f.club, 0, Some(&before[0].address), &edit).unwrap();
        assert_eq!((after.original_debt, after.monthly_repayment, after.start_day, after.end_year, after.include_in_ffp), (1000, 55, 120, 2040, false));
        assert_eq!(f._records[0][0x15] & 0x02, 0x02, "flag bits above the day stay as they were");
        // Clear sets amounts to 0 and nothing else; the list keeps its length.
        let cleared = write_loan_at(&d, f.club, 0, None, &LoanEdit { clear: true, ..Default::default() }).unwrap();
        assert_eq!((cleared.original_debt, cleared.monthly_repayment, cleared.end_year), (0, 0, 2040));
        assert_eq!(read_loans_at(&d, f.club).unwrap().len(), 1);
        // Rejections.
        for bad in [
            LoanEdit { end_year: Some(1800), ..Default::default() },
            LoanEdit { loan_type: Some(17), ..Default::default() },
            LoanEdit { start_day: Some(0), ..Default::default() },
            LoanEdit { original_debt: Some(u32::MAX), ..Default::default() },
            LoanEdit { clear: true, original_debt: Some(5), ..Default::default() },
            LoanEdit { end_year: Some(2020), ..Default::default() },
            LoanEdit::default(),
        ] {
            assert!(write_loan_at(&d, f.club, 0, None, &bad).is_err(), "{bad:?}");
        }
        assert!(write_loan_at(&d, f.club, 3, None, &edit).is_err(), "index out of range");
        assert!(write_loan_at(&d, f.club, 0, Some("0x1"), &edit).is_err(), "stale address");
    }
}
