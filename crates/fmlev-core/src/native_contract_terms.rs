//! Contract terms beyond wage and expiry: squad status (agreed playing time) and the minimum-fee release clause.
//!
//! Layout of a `FULL_CONTRACT` (200 bytes), traced on live R. Madrid squads and checked against the game's squad,
//! contract and playing-time screens (PID 4496, PE timestamp 0x6a229e8a):
//! - Squad status: u8 at `+0x54` (agreed playing time). `+0x56` normally holds the same code. Codes seen on screens:
//!   1 Star Player (Mbappe, Courtois), 2 Important Player (Bellingham, Vinicius, Bernat), 3 Regular Starter
//!   (Camavinga, Valverde, Rudiger ...), 4 Squad Player (Mastantuono, Asencio, Carreras ...), 7 Fringe Player
//!   (Bruno Iglesias). Other codes (9, 10, 20, 22 ...) exist but their labels were not confirmed on screen.
//! - Clauses: a pointer vector at `+0x68` (begin, end, capacity) of 8-byte entries `{u32 value, u16 aux, u16 type}`.
//!   The entry with type 0 is the minimum-fee release clause (Bruno 43,000,420 = GBP 43M, Mbappe 864,206,784 =
//!   GBP 864M); other entries are bonuses (types 32/33/34/38) and the promotion raise (type 4).
//!
//! Editing only changes the value of an existing release clause entry or the two status bytes. Adding or
//! removing a clause would resize a vector the game iterates, so that is never done here. Happiness is not
//! stored in a single field (it is derived from several factors) and is deliberately not handled.

use fmlev_mem::LinuxMemoryDriver;

use crate::error::{CoreError, Result};

pub const SQUAD_STATUS_OFFSET: usize = 0x54;
pub const SQUAD_STATUS_EXPECTED_OFFSET: usize = 0x56;
pub const CLAUSE_VECTOR_OFFSET: usize = 0x68;
const CLAUSE_ENTRY_SIZE: usize = 8;
const MAX_CLAUSE_BYTES: usize = 0x400;
const RELEASE_CLAUSE_TYPE: u16 = 0;

/// Squad status codes whose labels were confirmed on the game's screens.
pub const SQUAD_STATUSES: [(u8, &str); 5] =
    [(1, "Star Player"), (2, "Important Player"), (3, "Regular Starter"), (4, "Squad Player"), (7, "Fringe Player")];

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidValue(message.to_owned())
}

pub fn squad_status_label(code: u8) -> Option<&'static str> {
    SQUAD_STATUSES.iter().find(|(c, _)| *c == code).map(|(_, label)| *label)
}

pub fn squad_status_code(label: &str) -> Option<u8> {
    SQUAD_STATUSES.iter().find(|(_, l)| *l == label).map(|(code, _)| *code)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseClause {
    /// No type-0 entry (the contract has no release clause, or no clause list).
    Absent,
    /// The entry's address and its value.
    Present { entry: usize, value: u32 },
    /// The list failed a guard or several release entries exist; the reason is reported, nothing is guessed.
    Unavailable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractTerms {
    pub squad_status_code: u8,
    pub expected_status_code: u8,
    pub release_clause: ReleaseClause,
}

fn read_release_clause(driver: &LinuxMemoryDriver, contract: usize) -> ReleaseClause {
    let begin = driver.read_ptr(contract + CLAUSE_VECTOR_OFFSET);
    let end = driver.read_ptr(contract + CLAUSE_VECTOR_OFFSET + 8);
    let capacity = driver.read_ptr(contract + CLAUSE_VECTOR_OFFSET + 16);
    let (Ok(begin), Ok(end), Ok(capacity)) = (begin, end, capacity) else {
        return ReleaseClause::Unavailable("clause list unreadable".into());
    };
    if begin == 0 && end == 0 {
        return ReleaseClause::Absent;
    }
    if begin <= 0x10000 || end < begin || capacity < end || (end - begin) % CLAUSE_ENTRY_SIZE != 0 || end - begin > MAX_CLAUSE_BYTES {
        return ReleaseClause::Unavailable("clause list failed its bounds check".into());
    }
    let mut bytes = vec![0u8; end - begin];
    if !bytes.is_empty() && driver.read_bytes(begin, &mut bytes).ok() != Some(bytes.len()) {
        return ReleaseClause::Unavailable("clause list unreadable".into());
    }
    let mut found = Vec::new();
    for (index, entry) in bytes.chunks_exact(CLAUSE_ENTRY_SIZE).enumerate() {
        let kind = u16::from_le_bytes([entry[6], entry[7]]);
        if kind == RELEASE_CLAUSE_TYPE {
            found.push((begin + index * CLAUSE_ENTRY_SIZE, u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]])));
        }
    }
    match found.as_slice() {
        [] => ReleaseClause::Absent,
        [(entry, value)] => ReleaseClause::Present { entry: *entry, value: *value },
        _ => ReleaseClause::Unavailable("several release-clause entries; refusing to guess".into()),
    }
}

/// Reads squad status and the release clause of the contract at `contract` (already ownership-checked by the caller).
pub fn read_terms(driver: &LinuxMemoryDriver, contract: usize) -> Result<ContractTerms> {
    Ok(ContractTerms {
        squad_status_code: driver.read::<u8>(contract + SQUAD_STATUS_OFFSET)?,
        expected_status_code: driver.read::<u8>(contract + SQUAD_STATUS_EXPECTED_OFFSET)?,
        release_clause: read_release_clause(driver, contract),
    })
}

/// `(address, new bytes)` edits that set the squad status. `+0x56` follows only when it already matched `+0x54`,
/// so a contract with differing bytes keeps its second byte.
pub fn squad_status_edits(driver: &LinuxMemoryDriver, contract: usize, code: u8) -> Result<Vec<(usize, Vec<u8>)>> {
    if squad_status_label(code).is_none() {
        return Err(invalid("Squad status must be Star Player, Important Player, Regular Starter, Squad Player or Fringe Player"));
    }
    let current = driver.read::<u8>(contract + SQUAD_STATUS_OFFSET)?;
    let expected = driver.read::<u8>(contract + SQUAD_STATUS_EXPECTED_OFFSET)?;
    let mut edits = vec![(contract + SQUAD_STATUS_OFFSET, vec![code])];
    if expected == current {
        edits.push((contract + SQUAD_STATUS_EXPECTED_OFFSET, vec![code]));
    }
    Ok(edits)
}

/// `(address, new bytes)` for the value of the existing release clause entry. A contract without a release clause
/// is refused: adding an entry would resize a list the game iterates.
pub fn release_clause_edit(driver: &LinuxMemoryDriver, contract: usize, value: i64) -> Result<(usize, Vec<u8>)> {
    let value = u32::try_from(value)
        .ok()
        .filter(|v| (1..=i32::MAX as u32).contains(v))
        .ok_or_else(|| invalid("Release clause must be a whole amount from 1 to 2,147,483,647"))?;
    match read_release_clause(driver, contract) {
        ReleaseClause::Present { entry, .. } => Ok((entry, value.to_le_bytes().to_vec())),
        ReleaseClause::Absent => Err(invalid("This contract has no release clause; adding or removing one would resize a list the game iterates, so only an existing clause can be edited")),
        ReleaseClause::Unavailable(reason) => Err(invalid(&format!("Release clause unavailable: {reason}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn driver() -> LinuxMemoryDriver {
        LinuxMemoryDriver::new(std::process::id() as i32)
    }

    fn put<T: Copy>(buf: &mut [u8], at: usize, v: T) {
        let bytes = unsafe { std::slice::from_raw_parts(&v as *const T as *const u8, std::mem::size_of::<T>()) };
        buf[at..at + bytes.len()].copy_from_slice(bytes);
    }

    fn entry(value: u32, aux: u16, kind: u16) -> Vec<u8> {
        let mut e = value.to_le_bytes().to_vec();
        e.extend_from_slice(&aux.to_le_bytes());
        e.extend_from_slice(&kind.to_le_bytes());
        e
    }

    /// A fake contract with a clause list holding `entries`; the list outlives the returned contract buffer.
    fn contract(status: u8, expected: u8, entries: &[Vec<u8>]) -> (Vec<u8>, Vec<u8>) {
        let list: Vec<u8> = entries.concat();
        let mut c = vec![0u8; 0xc8];
        c[SQUAD_STATUS_OFFSET] = status;
        c[SQUAD_STATUS_EXPECTED_OFFSET] = expected;
        if !list.is_empty() {
            let begin = list.as_ptr() as usize;
            put(&mut c, CLAUSE_VECTOR_OFFSET, begin);
            put(&mut c, CLAUSE_VECTOR_OFFSET + 8, begin + list.len());
            put(&mut c, CLAUSE_VECTOR_OFFSET + 16, begin + list.len());
        }
        (c, list)
    }

    #[test]
    fn labels_cover_only_screen_confirmed_codes() {
        assert_eq!(squad_status_label(1), Some("Star Player"));
        assert_eq!(squad_status_label(7), Some("Fringe Player"));
        assert_eq!(squad_status_label(10), None);
        assert_eq!(squad_status_code("Regular Starter"), Some(3));
        assert_eq!(squad_status_code("Hot Prospect"), None);
    }

    #[test]
    fn reads_status_and_the_type_zero_clause_among_bonuses() {
        // Bruno Iglesias: appearance fee, assist bonus, unused sub fee, promotion raise, release clause 43,000,420.
        let (c, _list) = contract(7, 7, &[entry(1612, 0xffff, 0x20), entry(806, 0xffff, 0x27), entry(1075, 0xffff, 0x26), entry(0xffff_ffff, 25, 4), entry(43_000_420, 0xffff, 0)]);
        let terms = read_terms(&driver(), c.as_ptr() as usize).unwrap();
        assert_eq!((terms.squad_status_code, terms.expected_status_code), (7, 7));
        match terms.release_clause {
            ReleaseClause::Present { value, .. } => assert_eq!(value, 43_000_420),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn no_list_or_no_type_zero_entry_means_no_release_clause() {
        let (none, _a) = contract(3, 3, &[]);
        assert_eq!(read_terms(&driver(), none.as_ptr() as usize).unwrap().release_clause, ReleaseClause::Absent);
        let (bonus_only, _b) = contract(3, 3, &[entry(100, 0xffff, 0x20)]);
        assert_eq!(read_terms(&driver(), bonus_only.as_ptr() as usize).unwrap().release_clause, ReleaseClause::Absent);
    }

    #[test]
    fn broken_or_ambiguous_lists_are_unavailable_never_guessed() {
        let (two, _l) = contract(3, 3, &[entry(1, 0xffff, 0), entry(2, 0xffff, 0)]);
        assert!(matches!(read_terms(&driver(), two.as_ptr() as usize).unwrap().release_clause, ReleaseClause::Unavailable(_)));
        let mut bad = vec![0u8; 0xc8];
        put(&mut bad, CLAUSE_VECTOR_OFFSET, 0x2_0000usize);
        put(&mut bad, CLAUSE_VECTOR_OFFSET + 8, 0x1_0000usize); // end before begin
        assert!(matches!(read_terms(&driver(), bad.as_ptr() as usize).unwrap().release_clause, ReleaseClause::Unavailable(_)));
    }

    #[test]
    fn release_clause_edit_targets_only_the_existing_entry_and_validates_the_amount() {
        let (c, list) = contract(1, 1, &[entry(5, 0xffff, 0x20), entry(864_206_784, 0xffff, 0)]);
        let d = driver();
        let (address, bytes) = release_clause_edit(&d, c.as_ptr() as usize, 1_000_000).unwrap();
        assert_eq!(address, list.as_ptr() as usize + 8, "the second entry, not the first");
        assert_eq!(bytes, 1_000_000u32.to_le_bytes().to_vec());
        for bad in [0i64, -5, i64::from(i32::MAX) + 1] {
            assert!(release_clause_edit(&d, c.as_ptr() as usize, bad).is_err(), "{bad}");
        }
        let (none, _l) = contract(1, 1, &[entry(5, 0xffff, 0x20)]);
        let error = release_clause_edit(&d, none.as_ptr() as usize, 10).unwrap_err().to_string();
        assert!(error.contains("no release clause"), "{error}");
    }

    #[test]
    fn squad_status_edit_moves_the_second_byte_only_when_it_matched() {
        let d = driver();
        let (same, _a) = contract(4, 4, &[]);
        let edits = squad_status_edits(&d, same.as_ptr() as usize, 2).unwrap();
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0], (same.as_ptr() as usize + SQUAD_STATUS_OFFSET, vec![2]));
        assert_eq!(edits[1], (same.as_ptr() as usize + SQUAD_STATUS_EXPECTED_OFFSET, vec![2]));
        let (differs, _b) = contract(2, 1, &[]);
        assert_eq!(squad_status_edits(&d, differs.as_ptr() as usize, 3).unwrap().len(), 1, "Bernat-style contract keeps +0x56");
        assert!(squad_status_edits(&d, same.as_ptr() as usize, 20).is_err(), "unlabelled codes are not written");
    }
}
