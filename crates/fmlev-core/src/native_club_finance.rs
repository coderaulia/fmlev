//! Club transfer embargo and financial statements, read from the native finance object.
//!
//! Both live in the finance object at club +0x150 (the object `native_club` already validates:
//! back-pointer to the club at +8). Leads came from the community tool FM26 Studio; every offset
//! was re-read on a live save (PID 98701, 24,378 clubs) before it was used here.
//!
//! Embargo (finance object):
//! - `+0x2c` / `+0x30` / `+0x34`: start, end and appeal dates as u32 `(year << 16) | (flags << 9) | day`.
//!   `0x076c0001` (1900, day 1) means "no date". Live: 24,265 clubs have no flags and no dates.
//! - `+0x38`: age restriction (u8; live values 0, 18, 20).
//! - `+0x3b`: flag byte. Live: 0x04 on 101 clubs (end date set, no start date), 0x10 on 5, 0x02 on 2.
//!   Bit names come from FM26 Studio and are **unverified**: no on-screen comparison was possible.
//!
//! Financial statements (finance object), four periods each with an expenditure block and an income
//! block of i32 raw GBP. Period stride is 280 bytes: expenditure at `base`, income at `base + 156`:
//! this month `640 / 796`, last month `920 / 1076`, this season `1200 / 1356`, last season
//! `1480 / 1636`. FM26 Studio reads the this-season income at `+1200` (the expenditure block), which is
//! wrong: the income base is `+1356`. Evidence: 296 simulated clubs have positive wages at `+1216`
//! and every named field is >= 0 there; this-season values are >= last-month values in all but a
//! handful of cases (cumulative); R. Madrid this-season player wages (117.4M) match about six
//! months of the last-month wages (18.9M). The ~24,000 other clubs hold negative or non-simulated
//! data in the same area, so a period is only reported when it passes a plausibility check.
//! Field labels are **unverified** against the Finances screen (it was not open).

use serde::{Deserialize, Serialize};

use fmlev_mem::LinuxMemoryDriver;

use crate::error::{CoreError, Result};
use crate::native_club::NativeClubRecord;
use crate::native_transfer::{apply_transaction, NativeEdit};
use crate::search_index::is_leap;

const EMBARGO_START: usize = 0x2c;
const EMBARGO_END: usize = 0x30;
const EMBARGO_APPEAL: usize = 0x34;
const EMBARGO_AGE: usize = 0x38;
const EMBARGO_FLAGS: usize = 0x3b;
/// "No date" in the game's packed format (year 1900, day 1).
pub const EMPTY_DATE: u32 = 0x076c_0001;
const MAX_AGE_RESTRICTION: u8 = 40;
/// Bit 0x40 has never been seen set and is not named anywhere; writes refuse it.
const WRITABLE_FLAG_MASK: u8 = 0xbf;

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::InvalidValue(message.into())
}

/// Flag bits with the names FM26 Studio gives them. UNVERIFIED in this build.
const FLAG_NAMES: [(u8, &str); 7] = [
    (0x01, "undergoing_takeover"),
    (0x02, "bankrupt"),
    (0x04, "global_transfer_ban"),
    (0x08, "ffp_sanction"),
    (0x10, "unnamed_0x10"),
    (0x20, "attached_player_ban"),
    (0x80, "transfers_disabled"),
];

/// A packed embargo date as stored (u32 `(year << 16) | (flags << 9) | day`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct EmbargoDate {
    pub raw: u32,
    /// True for the game's "no date" value (or 0).
    pub empty: bool,
    pub year: Option<u16>,
    pub day: Option<u16>,
}

impl EmbargoDate {
    fn decode(raw: u32, label: &str) -> Result<Self> {
        if raw == EMPTY_DATE || raw == 0 {
            return Ok(Self { raw, empty: true, year: None, day: None });
        }
        let year = (raw >> 16) as u16;
        let day = (raw & 0x1ff) as u16;
        let max = if is_leap(year) { 366 } else { 365 };
        if !(1900..=2200).contains(&year) || !(1..=max).contains(&day) {
            return Err(invalid(format!("Embargo {label} date word 0x{raw:08x} is not a valid game date")));
        }
        Ok(Self { raw, empty: false, year: Some(year), day: Some(day) })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClubEmbargo {
    pub club_id: i32,
    pub start: EmbargoDate,
    pub end: EmbargoDate,
    pub appeal: EmbargoDate,
    pub age_restriction: u8,
    pub flags: u8,
    /// Names of the set bits (Studio names, unverified).
    pub flag_names: Vec<&'static str>,
    /// True when any date or flag is set, i.e. the record is not the "no embargo" pattern.
    pub any_set: bool,
    pub verification: &'static str,
}

const EMBARGO_NOTE: &str = "Unverified in game: dates and flags read from native memory and checked against 24,378 live clubs (most have none); bit names come from a community tool and no screen comparison was possible.";

fn embargo_edit_address(record: &NativeClubRecord, offset: usize) -> usize {
    record.finance + offset
}

/// Reads the embargo block after identity validation. Out-of-range values fail closed.
pub fn read_embargo(driver: &LinuxMemoryDriver, record: &NativeClubRecord) -> Result<ClubEmbargo> {
    record.validate(driver)?;
    let date = |offset: usize, label: &str| -> Result<EmbargoDate> {
        EmbargoDate::decode(driver.read::<u32>(record.finance + offset)?, label)
    };
    let start = date(EMBARGO_START, "start")?;
    let end = date(EMBARGO_END, "end")?;
    let appeal = date(EMBARGO_APPEAL, "appeal")?;
    let age_restriction = driver.read::<u8>(record.finance + EMBARGO_AGE)?;
    if age_restriction > MAX_AGE_RESTRICTION {
        return Err(invalid(format!("Embargo age restriction {age_restriction} is outside 0..={MAX_AGE_RESTRICTION}")));
    }
    let flags = driver.read::<u8>(record.finance + EMBARGO_FLAGS)?;
    let flag_names = FLAG_NAMES.iter().filter(|(bit, _)| flags & bit != 0).map(|(_, name)| *name).collect();
    let any_set = !(start.empty && end.empty && appeal.empty && age_restriction == 0 && flags == 0);
    Ok(ClubEmbargo { club_id: record.club_id, start, end, appeal, age_restriction, flags, flag_names, any_set, verification: EMBARGO_NOTE })
}

/// A requested date change: set a date, or clear it back to "no date".
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum DateEdit {
    Set(SetDate),
    Clear(ClearDate),
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SetDate {
    pub year: u16,
    pub day: u16,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClearDate {
    pub clear: bool,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EmbargoEdit {
    pub start: Option<DateEdit>,
    pub end: Option<DateEdit>,
    pub appeal: Option<DateEdit>,
    pub age_restriction: Option<u8>,
    pub flags: Option<u8>,
}

fn packed_for_edit(edit: DateEdit, current: u32, label: &str) -> Result<u32> {
    match edit {
        DateEdit::Clear(ClearDate { clear: true }) => Ok(EMPTY_DATE),
        DateEdit::Clear(_) => Err(invalid(format!("Embargo {label}: use {{\"clear\": true}} or {{\"year\", \"day\"}}"))),
        DateEdit::Set(SetDate { year, day }) => {
            let max = if is_leap(year) { 366 } else { 365 };
            if !(1900..=2200).contains(&year) || !(1..=max).contains(&day) {
                return Err(invalid(format!("Embargo {label}: {year} day {day} is not a valid date (day 1..={max}, year 1900..=2200)")));
            }
            // Keep the date's own flag bits when it already holds a date; a cleared slot starts with none.
            let keep = if current == EMPTY_DATE || current == 0 { 0 } else { current & 0xfe00 };
            Ok((u32::from(year) << 16) | keep | u32::from(day))
        }
    }
}

/// EXPERIMENTAL scalar edit of the embargo block: one transaction (pre-read, write, readback, rollback).
/// Only the dates, the age restriction and the flag byte are touched; no pointer or vector changes.
/// Returns the embargo as read back. Not proven in game.
pub fn write_embargo(driver: &LinuxMemoryDriver, record: &NativeClubRecord, edit: &EmbargoEdit) -> Result<ClubEmbargo> {
    let before = read_embargo(driver, record)?;
    if edit == &EmbargoEdit::default() {
        return Err(invalid("No embargo field was requested"));
    }
    let mut edits: Vec<NativeEdit> = Vec::new();
    for (request, offset, label, current) in [
        (edit.start, EMBARGO_START, "start", before.start.raw),
        (edit.end, EMBARGO_END, "end", before.end.raw),
        (edit.appeal, EMBARGO_APPEAL, "appeal", before.appeal.raw),
    ] {
        if let Some(request) = request {
            let next = packed_for_edit(request, current, label)?;
            edits.push(NativeEdit::new(
                embargo_edit_address(record, offset),
                current.to_le_bytes().to_vec(),
                next.to_le_bytes().to_vec(),
                "embargo date",
            ));
        }
    }
    if let Some(age) = edit.age_restriction {
        if age > MAX_AGE_RESTRICTION {
            return Err(invalid(format!("Embargo age restriction must be 0..={MAX_AGE_RESTRICTION}")));
        }
        edits.push(NativeEdit::new(embargo_edit_address(record, EMBARGO_AGE), vec![before.age_restriction], vec![age], "embargo age restriction"));
    }
    if let Some(flags) = edit.flags {
        if flags & !WRITABLE_FLAG_MASK != 0 {
            return Err(invalid("Embargo flag bit 0x40 is not understood and is never written"));
        }
        edits.push(NativeEdit::new(embargo_edit_address(record, EMBARGO_FLAGS), vec![before.flags], vec![flags], "embargo flags"));
    }
    record.validate(driver)?;
    apply_transaction(driver, &edits)?;
    read_embargo(driver, record)
}

// ---------------------------------------------------------------------------------------------
// Financial statements
// ---------------------------------------------------------------------------------------------

/// `(key, label, byte offset in the block)`. Labels from FM26 Studio, UNVERIFIED against the screen.
const EXPENDITURE_FIELDS: [(&str, &str, usize); 25] = [
    ("other", "Other", 0),
    ("loan_repayments", "Loan repayments", 4),
    ("league_fines", "League fines", 8),
    ("ground_maintenance", "Ground maintenance", 12),
    ("player_wages", "Player wages", 16),
    ("staff_wages", "Staff wages", 20),
    ("dividends", "Dividends", 24),
    ("bonuses", "Bonuses", 28),
    ("loyalty_bonuses", "Loyalty bonuses", 32),
    ("transfer_expenditure", "Transfer expenditure", 36),
    ("tax_income", "Income tax", 44),
    ("tax_employer", "Employer tax", 48),
    ("tax_corporation", "Corporation tax", 52),
    ("tax_vat", "VAT", 56),
    ("tax_national_insurance", "National insurance", 60),
    ("tax_pension", "Pension", 64),
    ("tax_transfer", "Transfer tax", 112),
    ("match_day_expenses", "Match day expenses", 116),
    ("director_emoluments", "Director emoluments", 120),
    ("non_football_costs", "Non-football costs", 124),
    ("agent_fees", "Agent fees", 128),
    ("youth_setup", "Youth setup", 132),
    ("scouting_costs", "Scouting costs", 136),
    ("travel_costs", "Travel costs", 140),
    ("insurance", "Insurance", 148),
];
/// Studio also reads `loan_fees` at +152.
const EXPENDITURE_EXTRA: [(&str, &str, usize); 1] = [("loan_fees", "Loan fees", 152)];
const EXPENDITURE_BYTES: usize = 156;

const INCOME_FIELDS: [(&str, &str, usize); 16] = [
    ("gate_receipts", "Gate receipts", 0),
    ("other", "Other", 4),
    ("season_tickets", "Season tickets", 8),
    ("interest", "Interest", 12),
    ("investments", "Investments", 16),
    ("merchandising", "Merchandising", 20),
    ("players_sold", "Players sold", 24),
    ("sponsorship", "Sponsorship", 28),
    ("match_day_income", "Match day income", 32),
    ("fund_raising", "Fund raising", 36),
    ("grants", "Grants", 44),
    ("prize_money", "Prize money", 52),
    ("tv_revenue", "TV revenue", 56),
    ("corporate_facilities", "Corporate facilities", 60),
    ("solidarity_payments", "Solidarity payments", 64),
    ("loan_income", "Loan income", 68),
];
/// Only words 0..=18 of the income block hold amounts; what follows are pointer-like values.
const INCOME_BYTES: usize = 76;

/// (key, expenditure base, income base) offsets inside the finance object.
const PERIODS: [(&str, &str, usize, usize); 4] = [
    ("this_month", "This month", 640, 796),
    ("last_month", "Last month", 920, 1076),
    ("this_season", "This season", 1200, 1356),
    ("last_season", "Last season", 1480, 1636),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatementField {
    pub key: &'static str,
    pub label: &'static str,
    /// Byte offset inside the finance object.
    pub offset: usize,
    /// Raw GBP.
    pub value: i32,
}

/// A nonzero word inside a block that no label is assigned to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnmappedWord {
    pub offset: usize,
    pub value: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatementPeriod {
    pub key: &'static str,
    pub label: &'static str,
    /// False when the block fails the plausibility check; then no amounts are returned.
    pub available: bool,
    pub reason: Option<String>,
    /// True when every word of the block is zero (for example last season on a first-season save).
    pub all_zero: bool,
    pub income: Vec<StatementField>,
    pub expenditure: Vec<StatementField>,
    pub unmapped_income: Vec<UnmappedWord>,
    pub unmapped_expenditure: Vec<UnmappedWord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClubStatements {
    pub club_id: i32,
    pub periods: Vec<StatementPeriod>,
    pub verification: &'static str,
}

const STATEMENT_NOTE: &str = "Unverified labels: offsets and cumulative behaviour were checked on a live save, but the Finances screen was not compared. Amounts are raw GBP for the current display period; statements are history the game recomputes, so they are read only.";

fn words(bytes: &[u8]) -> Vec<i32> {
    bytes.chunks_exact(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect()
}

fn collect(
    block: &[i32],
    base: usize,
    named: &[(&'static str, &'static str, usize)],
    extra: &[(&'static str, &'static str, usize)],
) -> (Vec<StatementField>, Vec<UnmappedWord>) {
    let mut fields = Vec::new();
    for (key, label, offset) in named.iter().chain(extra.iter()) {
        fields.push(StatementField { key, label, offset: base + offset, value: block[offset / 4] });
    }
    let unmapped = block
        .iter()
        .enumerate()
        .filter(|(index, value)| **value != 0 && !named.iter().chain(extra.iter()).any(|(_, _, offset)| offset / 4 == *index))
        .map(|(index, value)| UnmappedWord { offset: base + index * 4, value: *value })
        .collect();
    (fields, unmapped)
}

/// Reads the four statement periods after identity validation. A period whose named amounts are not
/// all non-negative is reported unavailable with a reason (not simulated / not initialised), never
/// zero-filled.
pub fn read_statements(driver: &LinuxMemoryDriver, record: &NativeClubRecord) -> Result<ClubStatements> {
    record.validate(driver)?;
    let mut periods = Vec::new();
    for (key, label, exp_base, inc_base) in PERIODS {
        let mut exp_bytes = vec![0u8; EXPENDITURE_BYTES];
        let mut inc_bytes = vec![0u8; INCOME_BYTES];
        if driver.read_bytes(record.finance + exp_base, &mut exp_bytes)? != exp_bytes.len()
            || driver.read_bytes(record.finance + inc_base, &mut inc_bytes)? != inc_bytes.len()
        {
            return Err(invalid("Short read of the club finance statements"));
        }
        let (exp, inc) = (words(&exp_bytes), words(&inc_bytes));
        let all_zero = exp.iter().chain(inc.iter()).all(|word| *word == 0);
        let (expenditure, unmapped_expenditure) = collect(&exp, exp_base, &EXPENDITURE_FIELDS, &EXPENDITURE_EXTRA);
        let (income, unmapped_income) = collect(&inc, inc_base, &INCOME_FIELDS, &[]);
        let negative = expenditure.iter().chain(income.iter()).find(|field| field.value < 0);
        if let Some(bad) = negative {
            periods.push(StatementPeriod {
                key,
                label,
                available: false,
                reason: Some(format!(
                    "Field {} holds {}: this club's statement block is not live data (the club is probably not simulated in this save)",
                    bad.key, bad.value
                )),
                all_zero: false,
                income: Vec::new(),
                expenditure: Vec::new(),
                unmapped_income: Vec::new(),
                unmapped_expenditure: Vec::new(),
            });
            continue;
        }
        periods.push(StatementPeriod { key, label, available: true, reason: None, all_zero, income, expenditure, unmapped_income, unmapped_expenditure });
    }
    Ok(ClubStatements { club_id: record.club_id, periods, verification: STATEMENT_NOTE })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        _club: Vec<u8>,
        _finance: Vec<u8>,
        record: NativeClubRecord,
        driver: LinuxMemoryDriver,
    }

    fn fixture() -> Fixture {
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let mut club = vec![0u8; 0x200];
        let mut finance = vec![0u8; 0x1000];
        let (club_ptr, finance_ptr) = (club.as_mut_ptr() as usize, finance.as_mut_ptr() as usize);
        driver.write(club_ptr + 8, &1089i32).unwrap();
        driver.write(club_ptr + 0xc, &1736i32).unwrap();
        driver.write(club_ptr + 0x10, &1736i32).unwrap();
        driver.write(club_ptr + 0x150, &finance_ptr).unwrap();
        driver.write(finance_ptr + 8, &club_ptr).unwrap();
        for offset in [EMBARGO_START, EMBARGO_END, EMBARGO_APPEAL] {
            driver.write(finance_ptr + offset, &EMPTY_DATE).unwrap();
        }
        let record = NativeClubRecord { pid: driver.pid(), club: club_ptr, details: None, finance: finance_ptr, first_team: None, stadium: None, club_id: 1089 };
        Fixture { _club: club, _finance: finance, record, driver }
    }

    #[test]
    fn no_embargo_pattern_reads_as_empty() {
        let f = fixture();
        let embargo = read_embargo(&f.driver, &f.record).unwrap();
        assert!(!embargo.any_set);
        assert!(embargo.start.empty && embargo.end.empty && embargo.appeal.empty);
        assert!(embargo.flag_names.is_empty());
    }

    #[test]
    fn embargo_reads_dates_age_and_flag_names() {
        let f = fixture();
        f.driver.write(f.record.finance + EMBARGO_END, &0x07eb_0097u32).unwrap();
        f.driver.write(f.record.finance + EMBARGO_AGE, &20u8).unwrap();
        f.driver.write(f.record.finance + EMBARGO_FLAGS, &0x04u8).unwrap();
        let embargo = read_embargo(&f.driver, &f.record).unwrap();
        assert!(embargo.any_set);
        assert_eq!((embargo.end.year, embargo.end.day), (Some(2027), Some(151)));
        assert_eq!(embargo.age_restriction, 20);
        assert_eq!(embargo.flag_names, vec!["global_transfer_ban"]);
    }

    #[test]
    fn embargo_read_fails_closed_on_garbage() {
        let f = fixture();
        f.driver.write(f.record.finance + EMBARGO_START, &0x1234_5678u32).unwrap();
        assert!(read_embargo(&f.driver, &f.record).is_err());
        f.driver.write(f.record.finance + EMBARGO_START, &EMPTY_DATE).unwrap();
        f.driver.write(f.record.finance + EMBARGO_AGE, &200u8).unwrap();
        assert!(read_embargo(&f.driver, &f.record).is_err());
    }

    #[test]
    fn embargo_write_sets_and_lifts_with_readback_and_validates() {
        let f = fixture();
        f.driver.write(f.record.finance + EMBARGO_END, &0x07eb_0097u32).unwrap();
        f.driver.write(f.record.finance + EMBARGO_FLAGS, &0x04u8).unwrap();
        // Lift: clear the end date and the flag.
        let lift = EmbargoEdit { end: Some(DateEdit::Clear(ClearDate { clear: true })), flags: Some(0), ..Default::default() };
        let after = write_embargo(&f.driver, &f.record, &lift).unwrap();
        assert!(!after.any_set);
        // Impose a new one.
        let set = EmbargoEdit { end: Some(DateEdit::Set(SetDate { year: 2028, day: 100 })), age_restriction: Some(18), flags: Some(0x04), ..Default::default() };
        let after = write_embargo(&f.driver, &f.record, &set).unwrap();
        assert_eq!((after.end.year, after.end.day, after.age_restriction, after.flags), (Some(2028), Some(100), 18, 4));
        // Validation.
        for bad in [
            EmbargoEdit::default(),
            EmbargoEdit { flags: Some(0x40), ..Default::default() },
            EmbargoEdit { age_restriction: Some(99), ..Default::default() },
            EmbargoEdit { start: Some(DateEdit::Set(SetDate { year: 2027, day: 366 })), ..Default::default() },
            EmbargoEdit { start: Some(DateEdit::Set(SetDate { year: 1800, day: 1 })), ..Default::default() },
            EmbargoEdit { start: Some(DateEdit::Clear(ClearDate { clear: false })), ..Default::default() },
        ] {
            assert!(write_embargo(&f.driver, &f.record, &bad).is_err(), "{bad:?}");
        }
        assert_eq!(read_embargo(&f.driver, &f.record).unwrap(), after);
    }

    #[test]
    fn embargo_write_refuses_a_foreign_finance_object() {
        let f = fixture();
        f.driver.write(f.record.finance + 8, &0x1234usize).unwrap();
        let edit = EmbargoEdit { age_restriction: Some(18), ..Default::default() };
        assert!(write_embargo(&f.driver, &f.record, &edit).is_err());
    }

    #[test]
    fn statements_read_named_fields_and_report_unmapped_words() {
        let f = fixture();
        let fin = f.record.finance;
        f.driver.write(fin + 640 + 16, &4_762_656i32).unwrap(); // this month player wages
        f.driver.write(fin + 796 + 28, &1_311_907i32).unwrap(); // this month sponsorship
        f.driver.write(fin + 1200 + 100, &9i32).unwrap(); // unmapped expenditure word
        f.driver.write(fin + 1356 + 28, &201_798_048i32).unwrap(); // this season sponsorship (income base 1356)
        let statements = read_statements(&f.driver, &f.record).unwrap();
        assert_eq!(statements.periods.len(), 4);
        let month = &statements.periods[0];
        assert!(month.available && !month.all_zero);
        assert_eq!(month.expenditure.iter().find(|x| x.key == "player_wages").unwrap().value, 4_762_656);
        assert_eq!(month.income.iter().find(|x| x.key == "sponsorship").unwrap().value, 1_311_907);
        let season = &statements.periods[2];
        assert_eq!(season.income.iter().find(|x| x.key == "sponsorship").unwrap().value, 201_798_048);
        assert_eq!(season.unmapped_expenditure, vec![UnmappedWord { offset: 1300, value: 9 }]);
        assert!(statements.periods[3].all_zero);
    }

    #[test]
    fn statements_with_negative_amounts_are_unavailable_not_zeroed() {
        let f = fixture();
        f.driver.write(f.record.finance + 1200 + 16, &-5i32).unwrap();
        let statements = read_statements(&f.driver, &f.record).unwrap();
        let season = &statements.periods[2];
        assert!(!season.available);
        assert!(season.reason.as_deref().unwrap().contains("player_wages"));
        assert!(season.income.is_empty() && season.expenditure.is_empty());
        assert!(statements.periods[0].available);
    }
}
