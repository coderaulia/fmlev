//! Read-only competition overview from the active FM26 binding table.
//! Rule data is deliberately absent: the visible stage bindings do not expose it.

use std::collections::BTreeMap;

use fmlev_il2cpp::{types::read_cstring, Il2CppString};
use fmlev_mem::LinuxMemoryDriver;
use serde::Serialize;

use crate::error::{CoreError, Result};

const ENTRY_SIZE: usize = 56;
const MAX_ENTRIES: usize = 200_000;
const MAX_DATA: usize = 200_000;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct LiveCompetitionSnapshot {
    pub id: i32,
    pub name: String,
    pub unique_id: Option<i64>,
    pub nation_id: Option<i32>,
    pub continent_id: Option<i32>,
    pub holder_team_id: Option<i32>,
    pub number_of_teams: Option<u16>,
    pub season_name: Option<String>,
    pub asia_reputation_position: Option<u16>,
    pub reputation_description: Option<String>,
}

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidValue(message.to_owned())
}

fn valid_ptr(p: usize) -> bool {
    p > 0x10000 && p < 0x7fff_ffff_ffff
}

fn class(driver: &LinuxMemoryDriver, object: usize) -> Option<(String, String)> {
    if !valid_ptr(object) {
        return None;
    }
    let klass = driver.read_ptr(object).ok()?;
    if !valid_ptr(klass) {
        return None;
    }
    let name = read_cstring(driver, driver.read_ptr(klass + 0x10).ok()?, 128).ok()?;
    let namespace = read_cstring(driver, driver.read_ptr(klass + 0x18).ok()?, 128).ok()?;
    Some((name, namespace))
}

fn reference_id(
    driver: &LinuxMemoryDriver,
    object: usize,
    name: &str,
    type_id: i32,
) -> Option<i32> {
    let (actual, namespace) = class(driver, object)?;
    if actual != name || namespace != "FM.UI" {
        return None;
    }
    let id = driver.read::<i32>(object + 0x20).ok()?;
    let kind = driver.read::<i32>(object + 0x24).ok()?;
    (id > 0 && kind == type_id).then_some(id)
}

fn string_value(driver: &LinuxMemoryDriver, tv: usize) -> Option<String> {
    if class(driver, tv)? != ("ReferenceTypedValue".into(), "SI.Core".into()) {
        return None;
    }
    let obj = driver.read_ptr(tv + 0x18).ok()?;
    if class(driver, obj)? != ("String".into(), "System".into()) {
        return None;
    }
    let s = Il2CppString::read(driver, obj).ok()?;
    let visible = s
        .rsplit('\u{2}')
        .next()
        .unwrap_or(&s)
        .trim_matches('\u{1}')
        .trim();
    (!visible.is_empty() && visible.len() <= 256).then(|| visible.to_owned())
}

fn numeric_value(driver: &LinuxMemoryDriver, tv: usize) -> Option<i64> {
    if class(driver, tv)? != ("NumericTypedValue".into(), "SI.Core".into()) {
        return None;
    }
    let kind = driver.read::<i32>(tv + 0x20).ok()?;
    let value = driver.read::<i64>(tv + 0x18).ok()?;
    match kind {
        0 => Some(value),
        1 if value >= 0 => Some(value),
        _ => None,
    }
}

fn ref_value(driver: &LinuxMemoryDriver, tv: usize, name: &str, type_id: i32) -> Option<i32> {
    if class(driver, tv)? != ("ReferenceTypedValue".into(), "SI.Core".into()) {
        return None;
    }
    reference_id(driver, driver.read_ptr(tv + 0x18).ok()?, name, type_id)
}

fn reputation_description(driver: &LinuxMemoryDriver, tv: usize) -> Option<String> {
    if class(driver, tv)? != ("ReferenceTypedValue".into(), "SI.Core".into()) {
        return None;
    }
    let object = driver.read_ptr(tv + 0x18).ok()?;
    if class(driver, object)? != ("DynamicReference".into(), "SI.Bindable".into()) {
        return None;
    }
    let entries = driver.read_ptr(object + 0x18).ok()?;
    if !valid_ptr(entries) {
        return None;
    }
    let count = driver.read::<usize>(entries + 0x18).ok()?;
    if !(1..=32).contains(&count) {
        return None;
    }
    let mut description = None;
    for i in 0..count {
        let entry = entries + 0x20 + i * 24;
        // DynamicReference string entry; key is the game fourcc "Strn".
        if driver.read::<u64>(entry + 8).ok()? != 0x5374_726e {
            continue;
        }
        let value = driver.read_ptr(entry + 16).ok()?;
        let text = string_value(driver, value)?;
        if description.as_ref().is_some_and(|old| old != &text) {
            return None;
        }
        description = Some(text);
    }
    description
}

fn set_once<T: Eq + Clone>(slot: &mut Option<T>, value: T) -> Result<()> {
    if slot.as_ref().is_some_and(|old| old != &value) {
        return Err(invalid("Conflicting live competition bindings"));
    }
    *slot = Some(value);
    Ok(())
}

#[derive(Default)]
struct Candidate {
    name: Option<String>,
    unique_id: Option<i64>,
    nation_id: Option<i32>,
    continent_id: Option<i32>,
    holder_team_id: Option<i32>,
    number_of_teams: Option<u16>,
    season_name: Option<String>,
    reputation_position: Option<u16>,
    reputation_description: Option<String>,
}

impl Candidate {
    fn finish(self, id: i32) -> Option<LiveCompetitionSnapshot> {
        Some(LiveCompetitionSnapshot {
            id,
            name: self.name?,
            unique_id: self.unique_id,
            nation_id: self.nation_id,
            continent_id: self.continent_id,
            holder_team_id: self.holder_team_id,
            number_of_teams: self.number_of_teams,
            season_name: self.season_name,
            asia_reputation_position: self.reputation_position,
            reputation_description: self.reputation_description,
        })
    }
}

/// Read only the active table's verified CompReference (type 25) overview fields.
/// Missing properties remain `None`; this function does not infer rule values.
fn try_read_live_competitions(
    driver: &LinuxMemoryDriver,
    binding_subsystem: usize,
) -> Result<Vec<LiveCompetitionSnapshot>> {
    let m_data = driver.read_ptr(binding_subsystem + 0x78)?;
    let m_ctx = driver.read_ptr(binding_subsystem + 0x88)?;
    if !valid_ptr(m_data) || !valid_ptr(m_ctx) {
        return Err(invalid("Binding table unavailable"));
    }
    let data_items = driver.read_ptr(m_data + 0x10)?;
    let data_size = driver.read::<i32>(m_data + 0x18)?;
    let entries = driver.read_ptr(m_ctx + 0x18)?;
    let count = driver.read::<i32>(m_ctx + 0x20)?;
    if !valid_ptr(data_items)
        || !valid_ptr(entries)
        || !(0..=MAX_DATA as i32).contains(&data_size)
        || !(0..=MAX_ENTRIES as i32).contains(&count)
    {
        return Err(invalid("Binding table exceeds safe bounds"));
    }
    let mut raw = vec![0u8; count as usize * ENTRY_SIZE];
    driver.read_bytes(entries + 0x20, &mut raw)?;
    let mut found: BTreeMap<i32, Candidate> = BTreeMap::new();
    for entry in raw.chunks_exact(ENTRY_SIZE) {
        let ptr = |at: usize| usize::from_ne_bytes(entry[at..at + 8].try_into().unwrap());
        let src = ptr(8);
        let prop = ptr(16);
        let key = ptr(40) & 0xffff_ffff;
        if !valid_ptr(src) || !valid_ptr(prop) || key >= data_size as usize {
            continue;
        }
        let ref_obj = driver.read_ptr(src + 0x18).unwrap_or(0);
        let Some(id) = reference_id(driver, ref_obj, "CompReference", 25) else {
            continue;
        };
        let prop_str = Il2CppString::read(driver, prop)
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                let p = driver.read_ptr(prop + 0x10).ok()?;
                Il2CppString::read(driver, p).ok().filter(|s| !s.is_empty())
            });
        let Some(prop_name) = prop_str else {
            continue;
        };
        let d_obj = driver.read_ptr(data_items + 0x20 + key * 8).unwrap_or(0);
        if !valid_ptr(d_obj) {
            continue;
        }
        let tv = driver.read_ptr(d_obj + 0x30).unwrap_or(0);
        if !valid_ptr(tv) {
            continue;
        }
        let item = found.entry(id).or_default();
        match prop_name.as_str() {
            "Name" => {
                if let Some(v) = string_value(driver, tv) {
                    set_once(&mut item.name, v)?;
                }
            }
            "UniqueId" => {
                if let Some(v) = numeric_value(driver, tv).filter(|v| *v > 0) {
                    set_once(&mut item.unique_id, v)?;
                }
            }
            "Nation" => {
                if let Some(v) = ref_value(driver, tv, "NationReference", 10) {
                    set_once(&mut item.nation_id, v)?;
                }
            }
            "Continent" => {
                if let Some(v) = ref_value(driver, tv, "ContinentReference", 5) {
                    set_once(&mut item.continent_id, v)?;
                }
            }
            "Holder" => {
                if let Some(v) = ref_value(driver, tv, "TeamReference", 24) {
                    set_once(&mut item.holder_team_id, v)?;
                }
            }
            "NumberOfTeams" => {
                if let Some(v) = numeric_value(driver, tv)
                    .and_then(|v| u16::try_from(v).ok())
                    .filter(|v| (2..=256).contains(v))
                {
                    set_once(&mut item.number_of_teams, v)?;
                }
            }
            "SeasonName" => {
                if let Some(v) = string_value(driver, tv) {
                    set_once(&mut item.season_name, v)?;
                }
            }
            "CurrentReputationPosition" => {
                if let Some(v) = numeric_value(driver, tv)
                    .and_then(|v| u16::try_from(v).ok())
                    .filter(|v| *v > 0)
                {
                    set_once(&mut item.reputation_position, v)?;
                }
            }
            "ReputationScope" => {
                if let Some(v) = reputation_description(driver, tv) {
                    set_once(&mut item.reputation_description, v)?;
                }
            }
            _ => {}
        }
    }
    Ok(found
        .into_iter()
        .filter_map(|(id, c)| c.finish(id))
        .collect())
}

pub fn read_live_competitions(
    driver: &LinuxMemoryDriver,
    binding_subsystem: usize,
) -> Vec<LiveCompetitionSnapshot> {
    try_read_live_competitions(driver, binding_subsystem).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflicting_same_entity_values_are_rejected() {
        let mut value = Some(18);
        assert!(set_once(&mut value, 18).is_ok());
        assert!(set_once(&mut value, 20).is_err());
    }

    #[test]
    fn missing_name_does_not_create_an_entity() {
        let candidate = Candidate {
            number_of_teams: Some(18),
            ..Default::default()
        };
        assert!(candidate.finish(3295).is_none());
    }
}
