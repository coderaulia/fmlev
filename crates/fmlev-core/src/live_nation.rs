//! Read-only nation overview values from currently bound FM26 UI references.
//! These are UI observations, not a verified native nation write layout.

use std::collections::{HashMap, HashSet};

use fmlev_il2cpp::{types::read_cstring, Il2CppString};
use fmlev_mem::LinuxMemoryDriver;
use serde::Serialize;

use crate::context::{decode_fm_link_name, read_typed_value, DecodedTypedValue};

type Key = (i32, i32);
type Fields = HashMap<String, DecodedTypedValue>;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LiveNationalTeamOverview {
    pub container_id: i32,
    pub federation_founded: Option<u16>,
    pub world_affiliation_year: Option<u16>,
    pub continental_affiliation_year: Option<u16>,
    pub fifa_rank: Option<u16>,
    pub fifa_points: Option<f64>,
    pub national_stadium: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LiveNationSnapshot {
    pub id: i32,
    pub unique_id: Option<u32>,
    pub name: String,
    pub continent: Option<String>,
    pub population: Option<u64>,
    pub spoken_languages: Option<String>,
    pub men: Option<LiveNationalTeamOverview>,
    pub women: Option<LiveNationalTeamOverview>,
}

fn valid_ptr(ptr: usize) -> bool {
    ptr > 0x10000 && ptr < 0x7fff_ffff_ffff
}

fn class_name(driver: &LinuxMemoryDriver, obj: usize) -> Option<(String, String)> {
    if !valid_ptr(obj) {
        return None;
    }
    let klass = driver.read_ptr(obj).ok()?;
    if !valid_ptr(klass) {
        return None;
    }
    let name = driver.read_ptr(klass + 0x10).ok()?;
    let namespace = driver.read_ptr(klass + 0x18).ok()?;
    if !valid_ptr(name) || !valid_ptr(namespace) {
        return None;
    }
    Some((
        read_cstring(driver, name, 96).ok()?,
        read_cstring(driver, namespace, 96).ok()?,
    ))
}

fn expected_reference_class(ref_type: i32) -> Option<&'static str> {
    match ref_type {
        5 => Some("ContinentReference"),
        9 => Some("NationalTeamContainerReference"),
        10 => Some("NationReference"),
        21 => Some("StadiumReference"),
        _ => None,
    }
}

fn display_string(value: &DecodedTypedValue) -> Option<String> {
    let DecodedTypedValue::String(raw) = value else {
        return None;
    };
    let (full, short) = decode_fm_link_name(raw);
    let displayed = if !short.is_empty() { short } else { full };
    let displayed = displayed.trim();
    if displayed.is_empty() || displayed.len() > 256 {
        None
    } else {
        Some(displayed.to_owned())
    }
}

fn unsigned(value: &DecodedTypedValue) -> Option<u64> {
    match value {
        DecodedTypedValue::Unsigned(v) => Some(*v),
        DecodedTypedValue::Signed(v) => u64::try_from(*v).ok(),
        _ => None,
    }
}

fn year(fields: &Fields, property: &str) -> Option<u16> {
    let year = u16::try_from(unsigned(fields.get(property)?)?).ok()?;
    (1800..=2200).contains(&year).then_some(year)
}

fn rank(fields: &Fields, property: &str) -> Option<u16> {
    let rank = u16::try_from(unsigned(fields.get(property)?)?).ok()?;
    (1..=9999).contains(&rank).then_some(rank)
}

fn reference(fields: &Fields, property: &str, expected_type: i32) -> Option<Key> {
    match fields.get(property)? {
        DecodedTypedValue::Reference {
            ref_type, m_index, ..
        } if *ref_type == expected_type && *m_index > 0 => Some((*ref_type, *m_index)),
        _ => None,
    }
}

fn team(
    index: i32,
    fields: &HashMap<Key, Fields>,
    stadium_names: &HashMap<Key, String>,
) -> Option<LiveNationalTeamOverview> {
    let own = fields.get(&(9, index))?;
    let fifa_points = match own.get("FifaPoints") {
        Some(DecodedTypedValue::Float(points))
            if points.is_finite() && (0.0..=10000.0).contains(points) =>
        {
            Some(*points)
        }
        _ => None,
    };
    let national_stadium =
        reference(own, "NationalStadium", 21).and_then(|key| stadium_names.get(&key).cloned());
    Some(LiveNationalTeamOverview {
        container_id: index,
        federation_founded: year(own, "AssociationFormedYear"),
        world_affiliation_year: year(own, "FifaAffiliationYear"),
        continental_affiliation_year: year(own, "FederationAffiliationYear"),
        fifa_rank: rank(own, "FifaRanking"),
        fifa_points,
        national_stadium,
    })
}

fn assemble(fields: &HashMap<Key, Fields>) -> Vec<LiveNationSnapshot> {
    let names: HashMap<Key, String> = fields
        .iter()
        .filter_map(|(key, props)| {
            props
                .get("Name")
                .and_then(display_string)
                .map(|name| (*key, name))
        })
        .collect();
    let mut nations = Vec::new();
    for (&(kind, id), own) in fields {
        if kind != 10 || id <= 0 {
            continue;
        }
        let Some(name) = names.get(&(kind, id)).cloned() else {
            continue;
        };
        let unique_id = own
            .get("UniqueId")
            .and_then(unsigned)
            .and_then(|v| u32::try_from(v).ok());
        let continent = reference(own, "Continent", 5).and_then(|key| names.get(&key).cloned());
        let population = own
            .get("Population")
            .and_then(unsigned)
            .filter(|v| *v <= 10_000_000_000);
        let spoken_languages = own.get("SpokenLanguages").and_then(display_string);
        let men =
            reference(own, "MainMenTeamContainer", 9).and_then(|(_, id)| team(id, fields, &names));
        let women = reference(own, "MainWomanTeamContainer", 9)
            .and_then(|(_, id)| team(id, fields, &names));
        nations.push(LiveNationSnapshot {
            id,
            unique_id,
            name,
            continent,
            population,
            spoken_languages,
            men,
            women,
        });
    }
    nations.sort_by_key(|nation| nation.id);
    nations
}

/// Returns only values currently present in validated nation-owned bindings.
/// No field here implies an editable native nation record.
pub fn read_live_nations(
    driver: &LinuxMemoryDriver,
    binding_subsystem: usize,
) -> Vec<LiveNationSnapshot> {
    if !valid_ptr(binding_subsystem) {
        return Vec::new();
    }
    let m_data = driver.read_ptr(binding_subsystem + 0x78).unwrap_or(0);
    let m_ctx = driver.read_ptr(binding_subsystem + 0x88).unwrap_or(0);
    if !valid_ptr(m_data) || !valid_ptr(m_ctx) {
        return Vec::new();
    }
    let data_items = driver.read_ptr(m_data + 0x10).unwrap_or(0);
    let data_size = driver.read::<i32>(m_data + 0x18).unwrap_or(0);
    let entries = driver.read_ptr(m_ctx + 0x18).unwrap_or(0);
    let count = driver.read::<i32>(m_ctx + 0x20).unwrap_or(0);
    if !valid_ptr(data_items)
        || !valid_ptr(entries)
        || !(1..=50_000).contains(&count)
        || !(1..=200_000).contains(&data_size)
    {
        return Vec::new();
    }
    let mut raw = vec![0u8; count as usize * 56];
    if driver.read_bytes(entries + 0x20, &mut raw).is_err() {
        return Vec::new();
    }
    let mut fields: HashMap<Key, Fields> = HashMap::new();
    let mut conflicted: HashSet<(Key, String)> = HashSet::new();
    for entry in raw.chunks_exact(56) {
        let ptr = |offset| usize::from_ne_bytes(entry[offset..offset + 8].try_into().unwrap());
        let source_tv = ptr(8);
        let property_ptr = ptr(16);
        let data_key = ptr(40) & 0xffff_ffff;
        if !valid_ptr(source_tv) || !valid_ptr(property_ptr) || data_key >= data_size as usize {
            continue;
        }
        let source = driver.read_ptr(source_tv + 0x18).unwrap_or(0);
        if !valid_ptr(source) {
            continue;
        }
        let id = driver.read::<i32>(source + 0x20).unwrap_or(0);
        let kind = driver.read::<i32>(source + 0x24).unwrap_or(0);
        let Some(expected_class) = expected_reference_class(kind) else {
            continue;
        };
        if id <= 0
            || class_name(driver, source)
                .as_ref()
                .is_none_or(|(name, ns)| name != expected_class || ns != "FM.UI")
        {
            continue;
        }
        let property_string = driver
            .read_ptr(property_ptr + 0x10)
            .ok()
            .and_then(|ptr| Il2CppString::read(driver, ptr).ok())
            .or_else(|| Il2CppString::read(driver, property_ptr).ok());
        let Some(property) = property_string.filter(|s| !s.is_empty() && s.len() < 128) else {
            continue;
        };
        let data = driver
            .read_ptr(data_items + 0x20 + data_key * 8)
            .unwrap_or(0);
        let value_tv = if valid_ptr(data) {
            driver.read_ptr(data + 0x30).unwrap_or(0)
        } else {
            0
        };
        let Some(value) = read_typed_value(driver, value_tv) else {
            continue;
        };
        let key = (kind, id);
        let conflict_key = (key, property.clone());
        if conflicted.contains(&conflict_key) {
            continue;
        }
        let group = fields.entry(key).or_default();
        if let Some(previous) = group.get(&property) {
            if previous != &value {
                group.remove(&property);
                conflicted.insert(conflict_key);
            }
        } else {
            group.insert(property, value);
        }
    }
    assemble(&fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Manual, read-only integration check when FM26 is running. It is skipped
    // in ordinary test runs because it depends on the user's current screen.
    #[test]
    #[ignore]
    fn live_nation_binding_probe() {
        use fmlev_il2cpp::OffsetsRegistry;
        use fmlev_mem::{find_fm_process, find_module_bases};

        let process = find_fm_process().expect("FM26 must be running");
        let bases = find_module_bases(process.pid).unwrap();
        let base = bases.game_assembly_base.unwrap();
        let driver = LinuxMemoryDriver::new(process.pid);
        let registry = OffsetsRegistry::load_from_json(include_str!("../../../offsets/v26.0.0.json")).unwrap();
        let singleton = registry.get_singleton("GameSubsystems").unwrap();
        let gs = fmlev_il2cpp::read_static_singleton(&driver, base, singleton).unwrap();
        let list = driver.read_ptr(gs).unwrap();
        let items = driver.read_ptr(list + 0x10).unwrap();
        let binding_subsystem = driver.read_ptr(items + 0x20 + 11 * 8).unwrap();
        let nations = read_live_nations(&driver, binding_subsystem);
        println!("live nations: {nations:#?}");
    }

    fn reference(typ: i32, id: i32) -> DecodedTypedValue {
        DecodedTypedValue::Reference {
            ptr: 0x100000,
            ref_type: typ,
            m_index: id,
        }
    }

    #[test]
    fn indonesia_overview_requires_owned_references() {
        let mut fields = HashMap::new();
        fields.insert(
            (10, 58),
            HashMap::from([
                ("Name".into(), DecodedTypedValue::String("Indonesia".into())),
                ("Population".into(), DecodedTypedValue::Signed(283_487_931)),
                (
                    "SpokenLanguages".into(),
                    DecodedTypedValue::String("Bahasa Indonesia".into()),
                ),
                ("Continent".into(), reference(5, 1)),
                ("MainMenTeamContainer".into(), reference(9, 58)),
                ("MainWomanTeamContainer".into(), reference(9, 260)),
            ]),
        );
        fields.insert(
            (5, 1),
            HashMap::from([("Name".into(), DecodedTypedValue::String("Asia".into()))]),
        );
        fields.insert(
            (21, 1102),
            HashMap::from([(
                "Name".into(),
                DecodedTypedValue::String("Gelora Bung Karno".into()),
            )]),
        );
        fields.insert(
            (9, 58),
            HashMap::from([
                ("AssociationFormedYear".into(), DecodedTypedValue::Signed(1930)),
                ("FifaAffiliationYear".into(), DecodedTypedValue::Signed(1952)),
                ("FederationAffiliationYear".into(), DecodedTypedValue::Signed(1954)),
                ("FifaRanking".into(), DecodedTypedValue::Signed(122)),
                ("FifaPoints".into(), DecodedTypedValue::Float(1144.88)),
                ("NationalStadium".into(), reference(21, 1102)),
            ]),
        );
        fields.insert(
            (9, 260),
            HashMap::from([("FifaRanking".into(), DecodedTypedValue::Signed(95))]),
        );
        let nation = assemble(&fields).pop().unwrap();
        assert_eq!(nation.name, "Indonesia");
        assert_eq!(nation.continent.as_deref(), Some("Asia"));
        assert_eq!(nation.population, Some(283_487_931));
        assert_eq!(nation.men.as_ref().unwrap().fifa_rank, Some(122));
        assert_eq!(nation.men.as_ref().unwrap().federation_founded, Some(1930));
        assert_eq!(nation.men.as_ref().unwrap().world_affiliation_year, Some(1952));
        assert_eq!(nation.men.as_ref().unwrap().continental_affiliation_year, Some(1954));
        assert_eq!(
            nation.men.as_ref().unwrap().national_stadium.as_deref(),
            Some("Gelora Bung Karno")
        );
        assert_eq!(nation.women.as_ref().unwrap().fifa_rank, Some(95));
        fields
            .get_mut(&(10, 58))
            .unwrap()
            .insert("MainMenTeamContainer".into(), reference(9, 260));
        assert_eq!(
            assemble(&fields)[0].men.as_ref().unwrap().fifa_rank,
            Some(95)
        );
    }
}
