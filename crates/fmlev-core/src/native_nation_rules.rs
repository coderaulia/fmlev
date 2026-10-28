//! Nationality rule candidates of the native nation record (plan C, READ-ONLY).
//!
//! Offsets come from the NATION serializer disassembly in game_plugin.dll (PE timestamp
//! 0x6a229e8a) and were compared with `fmscan entity ... nation --editor` / `tools/fmscan/nation_rules.py`:
//! - +0x1ed: years to gain nationality (tag `Nygn`, u8)
//! - +0x1eb: gain-nationality type code (tag `Ngct`, u8, only 1 and 2 seen; labels unknown)
//! - +0x1e9 bit 0x02: set = dual nationality NOT allowed (tag `Nndn`)
//!
//! Evidence ladder (AGENTS.md rule 3): nothing here is promoted to the UI until `CONFIDENCE`
//! says `Verified`. Raw candidates are always readable for discovery, but the payload built by
//! `NationalityRules::from_candidates` withholds every value that is not verified and says why.
//! There is no write path: writes stay unsupported until value and behaviour proof exist.

use fmlev_mem::{parse_process_maps, LinuxMemoryDriver};
use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::native_nation::NativeNationRecord;

const GAIN_YEARS_OFFSET: usize = 0x1ed;
const GAIN_TYPE_OFFSET: usize = 0x1eb;
const DUAL_FLAGS_OFFSET: usize = 0x1e9;
const NO_DUAL_BIT: u8 = 0x02;
const PE_TIMESTAMP: u32 = 0x6a22_9e8a;

/// How far a candidate field has climbed the evidence ladder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Candidate offset only.
    Unverified,
    /// Serializer trace plus a plausible live distribution; not compared with the Nation screen.
    Probable,
    /// Distribution, known nations and the in-game screen agree.
    Verified,
}

/// Current confidence per field. Flip an entry only after the evidence in AGENTS.md ("Nation rules").
pub const YEARS_CONFIDENCE: Confidence = Confidence::Probable;
pub const GAIN_TYPE_CONFIDENCE: Confidence = Confidence::Unverified;
pub const DUAL_CONFIDENCE: Confidence = Confidence::Unverified;

/// Raw bytes read from the nation record; unverified discovery data, never shown as a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NationalityCandidates {
    pub years: u8,
    pub gain_type_code: u8,
    pub flags: u8,
}

impl NationalityCandidates {
    pub fn dual_nationality_allowed(&self) -> bool {
        self.flags & NO_DUAL_BIT == 0
    }
}

/// One field of the read payload. `value` is only present when `status` is `verified`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuleField<T> {
    pub status: &'static str,
    pub confidence: Confidence,
    pub value: Option<T>,
    pub reason: Option<String>,
}

impl<T> RuleField<T> {
    fn gate(confidence: Confidence, candidate: Option<T>, why_missing: &str) -> Self {
        match (confidence, candidate) {
            (Confidence::Verified, Some(value)) => {
                Self { status: "verified", confidence, value: Some(value), reason: None }
            }
            (Confidence::Verified, None) => {
                Self { status: "unavailable", confidence, value: None, reason: Some(why_missing.to_owned()) }
            }
            (other, _) => Self {
                status: "unavailable",
                confidence: other,
                value: None,
                reason: Some(format!(
                    "candidate layout is {other:?}: not verified against the in-game Nation screen; value withheld"
                )),
            },
        }
    }
}

/// Read payload for `/api/nation/native`: verified fields only, the rest unavailable with a reason.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NationalityRules {
    pub years_to_gain_nationality: RuleField<u8>,
    pub gain_nationality_type_code: RuleField<u8>,
    pub dual_nationality_allowed: RuleField<bool>,
    pub write_support: &'static str,
}

impl NationalityRules {
    pub fn from_candidates(candidates: Option<NationalityCandidates>, why_missing: &str) -> Self {
        Self::with_confidence(candidates, why_missing, [YEARS_CONFIDENCE, GAIN_TYPE_CONFIDENCE, DUAL_CONFIDENCE])
    }

    fn with_confidence(candidates: Option<NationalityCandidates>, why: &str, c: [Confidence; 3]) -> Self {
        Self {
            years_to_gain_nationality: RuleField::gate(c[0], candidates.map(|v| v.years), why),
            // 1 and 2 are the only codes seen; anything else is not a known type.
            gain_nationality_type_code: RuleField::gate(
                c[1],
                candidates.map(|v| v.gain_type_code).filter(|code| (1..=2).contains(code)),
                why,
            ),
            dual_nationality_allowed: RuleField::gate(c[2], candidates.map(|v| v.dual_nationality_allowed()), why),
            write_support: "none: value and behaviour proof missing; edits return 501",
        }
    }
}

/// `game_plugin.dll` PE timestamp guard. Test processes have no module and skip it.
fn build_guard(driver: &LinuxMemoryDriver) -> Result<()> {
    let maps = parse_process_maps(driver.pid())?;
    let Some(base) = maps.iter().filter(|m| m.pathname.ends_with("game_plugin.dll")).map(|m| m.start).min() else {
        return Ok(());
    };
    let pe = driver.read::<u32>(base + 0x3c)? as usize;
    let stamp = driver.read::<u32>(base + pe + 8)?;
    if pe > 0x1000 || stamp != PE_TIMESTAMP {
        return Err(CoreError::InvalidValue(format!(
            "Unknown game_plugin.dll build (PE timestamp {stamp:#x}); nationality rules are unavailable"
        )));
    }
    Ok(())
}

impl NativeNationRecord {
    /// Reads the raw nationality-rule candidates after identity, ownership and build guards.
    pub fn read_nationality_candidates(&self, driver: &LinuxMemoryDriver) -> Result<NationalityCandidates> {
        self.validate(driver)?;
        build_guard(driver)?;
        Ok(NationalityCandidates {
            years: driver.read::<u8>(self.nation + GAIN_YEARS_OFFSET)?,
            gain_type_code: driver.read::<u8>(self.nation + GAIN_TYPE_OFFSET)?,
            flags: driver.read::<u8>(self.nation + DUAL_FLAGS_OFFSET)?,
        })
    }

    /// Verified-only rules payload; a failed read yields all-unavailable with the failure reason.
    pub fn read_nationality_rules(&self, driver: &LinuxMemoryDriver) -> NationalityRules {
        match self.read_nationality_candidates(driver) {
            Ok(candidates) => NationalityRules::from_candidates(Some(candidates), "native read unavailable"),
            Err(error) => NationalityRules::from_candidates(None, &format!("native read failed: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record_with(bytes: [u8; 3]) -> (Vec<u8>, NativeNationRecord, LinuxMemoryDriver) {
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let mut data = vec![0u8; 0x300];
        let nation = data.as_mut_ptr() as usize;
        driver.write(nation + 8, &170i32).unwrap();
        driver.write(nation + 0xc, &796i32).unwrap();
        driver.write(nation + 0x10, &796i32).unwrap();
        driver.write(nation + GAIN_YEARS_OFFSET, &bytes[0]).unwrap();
        driver.write(nation + GAIN_TYPE_OFFSET, &bytes[1]).unwrap();
        driver.write(nation + DUAL_FLAGS_OFFSET, &bytes[2]).unwrap();
        let record = NativeNationRecord {
            pid: driver.pid(),
            nation,
            men_container: None,
            women_container: None,
            men_team: None,
            women_team: None,
            nation_id: 170,
        };
        (data, record, driver)
    }

    #[test]
    fn candidates_are_read_with_polarity() {
        let (_keep, spain, driver) = record_with([10, 1, 0x00]);
        let c = spain.read_nationality_candidates(&driver).unwrap();
        assert_eq!((c.years, c.gain_type_code), (10, 1));
        assert!(c.dual_nationality_allowed());
        let (_keep, japan, driver) = record_with([5, 2, 0x02]);
        assert!(!japan.read_nationality_candidates(&driver).unwrap().dual_nationality_allowed());
        // Other bits of the flag byte do not change dual nationality.
        let (_keep, other, driver) = record_with([5, 2, 0xfd]);
        assert!(other.read_nationality_candidates(&driver).unwrap().dual_nationality_allowed());
    }

    #[test]
    fn unverified_candidates_are_withheld_from_the_payload() {
        let (_keep, spain, driver) = record_with([10, 1, 0x00]);
        let rules = spain.read_nationality_rules(&driver);
        assert_eq!(rules.years_to_gain_nationality.status, "unavailable");
        assert_eq!(rules.years_to_gain_nationality.confidence, Confidence::Probable);
        assert_eq!(rules.years_to_gain_nationality.value, None);
        assert!(rules.years_to_gain_nationality.reason.as_deref().unwrap().contains("not verified"));
        assert_eq!(rules.gain_nationality_type_code.value, None);
        assert_eq!(rules.dual_nationality_allowed.value, None);
        let json = serde_json::to_value(&rules).unwrap();
        assert!(json["years_to_gain_nationality"]["value"].is_null());
        assert!(json["write_support"].as_str().unwrap().starts_with("none"));
    }

    #[test]
    fn only_verified_fields_expose_values() {
        let candidates = NationalityCandidates { years: 10, gain_type_code: 2, flags: 0x02 };
        let rules = NationalityRules::with_confidence(
            Some(candidates),
            "x",
            [Confidence::Verified, Confidence::Unverified, Confidence::Verified],
        );
        assert_eq!(rules.years_to_gain_nationality.status, "verified");
        assert_eq!(rules.years_to_gain_nationality.value, Some(10));
        assert_eq!(rules.dual_nationality_allowed.value, Some(false));
        assert_eq!(rules.gain_nationality_type_code.status, "unavailable");
        assert_eq!(rules.gain_nationality_type_code.value, None);
    }

    #[test]
    fn verified_field_with_unknown_type_code_stays_unavailable() {
        let candidates = NationalityCandidates { years: 5, gain_type_code: 7, flags: 0 };
        let rules = NationalityRules::with_confidence(Some(candidates), "bad", [Confidence::Verified; 3]);
        assert_eq!(rules.gain_nationality_type_code.status, "unavailable");
        assert_eq!(rules.gain_nationality_type_code.reason.as_deref(), Some("bad"));
    }

    #[test]
    fn failed_read_reports_reason_not_defaults() {
        let (_keep, mut record, driver) = record_with([10, 1, 0]);
        record.nation_id = 999; // identity mismatch
        let rules = record.read_nationality_rules(&driver);
        assert_eq!(rules.years_to_gain_nationality.value, None);
        assert_eq!(rules.dual_nationality_allowed.value, None);
    }
}
