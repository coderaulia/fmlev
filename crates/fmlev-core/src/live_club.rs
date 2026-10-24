//! Bounded, read-only snapshot of club data currently exposed by FM26 bindings.
//! Values are attached to a verified club reference. Numeric money remains in
//! game units until the game's display-currency conversion is established.

use std::collections::HashMap;

use fmlev_il2cpp::{Il2CppString, types::read_cstring};
use fmlev_mem::LinuxMemoryDriver;
use serde::Serialize;

use crate::context::{DecodedTypedValue, read_typed_value};

#[derive(Debug, Clone, Default, Serialize)]
pub struct LiveClubSnapshot {
    pub id: i32,
    pub name: String,
    pub main_team_id: Option<i32>,
    pub stadium_id: Option<i32>,
    pub stadium_name: Option<String>,
    pub stadium_capacity_text: Option<String>,
    pub stadium_year_built: Option<i64>,
    pub stadium_owner: Option<String>,
    pub training_description: Option<String>,
    pub youth_description: Option<String>,
    pub corporate_description: Option<String>,
    pub balance_game_units: Option<i64>,
    pub transfer_budget_game_units: Option<i64>,
    pub remaining_wage_budget_text: Option<String>,
    pub current_wage_spending_text: Option<String>,
}

#[derive(Default)]
struct Partial {
    name: Option<String>,
    team: Option<i32>,
    parent_club: Option<i32>,
    stadium: Option<i32>,
    capacity: Option<String>,
    year: Option<i64>,
    owner: Option<String>,
    training: Option<String>,
    youth: Option<String>,
    corporate: Option<String>,
    balance: Option<i64>,
    transfer: Option<i64>,
    wage_remaining: Option<String>,
    wage_spending: Option<String>,
}

fn ptr(p: usize) -> bool { p > 0x10000 && p < 0x7fff_ffff_ffff }

fn object_class(driver: &LinuxMemoryDriver, object: usize) -> Option<(String, String)> {
    if !ptr(object) { return None; }
    let klass = driver.read_ptr(object).ok()?;
    if !ptr(klass) { return None; }
    let name = read_cstring(driver, driver.read_ptr(klass + 0x10).ok()?, 128).ok()?;
    let ns = read_cstring(driver, driver.read_ptr(klass + 0x18).ok()?, 128).ok()?;
    Some((name, ns))
}

fn text_value(driver: &LinuxMemoryDriver, tv: usize) -> Option<String> {
    let DecodedTypedValue::String(value) = read_typed_value(driver, tv)? else { return None; };
    let text = value.rsplit('\u{2}').next()?.trim_end_matches('\u{1}').trim();
    if text.is_empty() || text.len() > 512 { None } else { Some(text.to_owned()) }
}

fn number_value(driver: &LinuxMemoryDriver, tv: usize) -> Option<i64> {
    match read_typed_value(driver, tv)? {
        DecodedTypedValue::Signed(v) => Some(v),
        DecodedTypedValue::Unsigned(v) => i64::try_from(v).ok(),
        _ => None,
    }
}

fn year_value(driver: &LinuxMemoryDriver, tv: usize) -> Option<i64> {
    number_value(driver, tv).or_else(|| text_value(driver, tv)?.parse().ok()).filter(|year| (1800..=2200).contains(year))
}

fn reference_value(driver: &LinuxMemoryDriver, tv: usize, kind: i32) -> Option<i32> {
    let DecodedTypedValue::Reference { ref_type, m_index, .. } = read_typed_value(driver, tv)? else { return None; };
    (ref_type == kind && m_index > 0).then_some(m_index)
}

fn object_value(driver: &LinuxMemoryDriver, tv: usize) -> Option<usize> {
    let DecodedTypedValue::Object(p) = read_typed_value(driver, tv)? else { return None; };
    ptr(p).then_some(p)
}

/// Decode only the exact `desc` string of an owned DynamicReference. Other
/// dictionary keys are deliberately ignored because their scale is unknown.
fn dynamic_description(driver: &LinuxMemoryDriver, object: usize) -> Option<String> {
    if object_class(driver, object)? != ("DynamicReference".into(), "SI.Bindable".into()) { return None; }
    let entries = driver.read_ptr(object + 0x18).ok()?;
    if !ptr(entries) { return None; }
    let count = driver.read::<usize>(entries + 0x18).ok()?;
    if count == 0 || count > 64 { return None; }
    for i in 0..count {
        let entry = entries + 0x20 + i * 24;
        let key = driver.read::<u32>(entry + 8).ok()?;
        if key != 0x6465_7363 { continue; }
        let tv = driver.read_ptr(entry + 16).ok()?;
        if ptr(tv) { return text_value(driver, tv); }
    }
    None
}

fn extract_rows(driver: &LinuxMemoryDriver, subsystem: usize) -> Vec<(i32, i32, String, usize)> {
    if !ptr(subsystem) { return Vec::new(); }
    let Ok(data) = driver.read_ptr(subsystem + 0x78) else { return Vec::new(); };
    let Ok(ctx) = driver.read_ptr(subsystem + 0x88) else { return Vec::new(); };
    if !ptr(data) || !ptr(ctx) { return Vec::new(); }
    let Ok(items) = driver.read_ptr(data + 0x10) else { return Vec::new(); };
    let Ok(size) = driver.read::<i32>(data + 0x18) else { return Vec::new(); };
    let Ok(entries) = driver.read_ptr(ctx + 0x18) else { return Vec::new(); };
    let Ok(count) = driver.read::<i32>(ctx + 0x20) else { return Vec::new(); };
    if !ptr(items) || !ptr(entries) || !(1..=200_000).contains(&count) || !(1..=200_000).contains(&size) { return Vec::new(); }
    let mut raw = vec![0; count as usize * 56];
    if driver.read_bytes(entries + 0x20, &mut raw).is_err() { return Vec::new(); }
    let mut rows = Vec::new();
    let mut classes: HashMap<usize, Option<(String, String)>> = HashMap::new();
    let mut properties: HashMap<usize, Option<String>> = HashMap::new();
    for e in raw.chunks_exact(56) {
        if i32::from_ne_bytes(e[0..4].try_into().unwrap()) < 0 { continue; }
        let source = usize::from_ne_bytes(e[8..16].try_into().unwrap());
        let property = usize::from_ne_bytes(e[16..24].try_into().unwrap());
        let key = u64::from_ne_bytes(e[40..48].try_into().unwrap()) as u32 as usize;
        if !ptr(source) || !ptr(property) || key >= size as usize { continue; }
        let Ok(object) = driver.read_ptr(source + 0x18) else { continue; };
        let Ok(klass) = driver.read_ptr(object) else { continue; };
        let Some((class, ns)) = classes.entry(klass).or_insert_with(|| object_class(driver, object)).clone() else { continue; };
        if ns != "FM.UI" || !matches!(class.as_str(), "ClubReference" | "TeamReference" | "StadiumReference") { continue; }
        let Ok(index) = driver.read::<i32>(object + 0x20) else { continue; };
        let Ok(kind) = driver.read::<i32>(object + 0x24) else { continue; };
        if index <= 0 || !matches!((class.as_str(),kind), ("ClubReference",3) | ("TeamReference",24) | ("StadiumReference",21)) { continue; }
        let Some(name) = properties.entry(property).or_insert_with(|| {
            let indirect = driver.read_ptr(property + 0x10).ok()
                .and_then(|p| Il2CppString::read(driver, p).ok());
            indirect.or_else(|| Il2CppString::read(driver, property).ok())
                .filter(|s| !s.is_empty() && s.len() < 128)
        }).clone() else { continue; };
        let Ok(binding) = driver.read_ptr(items + 0x20 + key * 8) else { continue; };
        let Ok(tv) = driver.read_ptr(binding + 0x30) else { continue; };
        if ptr(tv) { rows.push((kind, index, name, tv)); }
    }
    rows
}

/// Returns only clubs for which an authoritative `ClubReference.Name` exists.
/// Each call resolves current bindings again; addresses are never cached.
pub fn read_live_clubs(driver: &LinuxMemoryDriver, binding_subsystem: usize) -> Vec<LiveClubSnapshot> {
    let rows = extract_rows(driver, binding_subsystem);
    let mut clubs: HashMap<i32, Partial> = HashMap::new();
    let mut teams: HashMap<i32, Partial> = HashMap::new();
    let mut stadiums: HashMap<i32, Partial> = HashMap::new();
    for (kind, id, prop, tv) in rows {
        let part = match kind { 3 => clubs.entry(id).or_default(), 24 => teams.entry(id).or_default(), 21 => stadiums.entry(id).or_default(), _ => continue };
        match (kind, prop.as_str()) {
            (_, "Name") => part.name = text_value(driver, tv),
            (3, "MainTeam") => part.team = reference_value(driver, tv, 24),
            (24, "TeamContainer" | "Club") => part.parent_club = reference_value(driver, tv, 3),
            (24, "Stadium") => part.stadium = reference_value(driver, tv, 21),
            (21, "CapacityWithText" | "Capacity") => part.capacity = text_value(driver, tv),
            (21, "YearBuilt") => part.year = year_value(driver, tv),
            (21, "Owner" | "OwnedBy" | "OwnedByString") => part.owner = text_value(driver, tv),
            (3, "TrainingFacilityRating") => part.training = object_value(driver, tv).and_then(|p| dynamic_description(driver, p)),
            (3, "YouthFacilityRating") => part.youth = object_value(driver, tv).and_then(|p| dynamic_description(driver, p)),
            (3, "CorporateFacilityRating") => part.corporate = object_value(driver, tv).and_then(|p| dynamic_description(driver, p)),
            (3, "OverallBalance") => part.balance = number_value(driver, tv),
            (3, "TransferBudget") => part.transfer = number_value(driver, tv),
            (3, "RemainingWageBudgetWithText") => part.wage_remaining = text_value(driver, tv),
            (3, "CurrentWageSpendingWithText") => part.wage_spending = text_value(driver, tv),
            _ => {}
        }
    }
    let mut output = Vec::new();
    for (id, club) in clubs {
        let Some(name) = club.name.filter(|s| !s.is_empty()) else { continue; };
        let team = club.team.and_then(|team_id| teams.get(&team_id).filter(|team| team.parent_club == Some(id)).map(|team| (team_id,team)));
        let stadium = team.as_ref().and_then(|(_,t)| t.stadium).and_then(|id| stadiums.get(&id).map(|s| (id,s)));
        output.push(LiveClubSnapshot {
            id, name, main_team_id: team.map(|(id,_)| id),
            stadium_id: stadium.map(|(id,_)| id),
            stadium_name: stadium.and_then(|(_,s)| s.name.clone()),
            stadium_capacity_text: stadium.and_then(|(_,s)| s.capacity.clone()),
            stadium_year_built: stadium.and_then(|(_,s)| s.year),
            stadium_owner: stadium.and_then(|(_,s)| s.owner.clone()),
            training_description: club.training, youth_description: club.youth,
            corporate_description: club.corporate,
            balance_game_units: club.balance, transfer_budget_game_units: club.transfer,
            remaining_wage_budget_text: club.wage_remaining,
            current_wage_spending_text: club.wage_spending,
        });
    }
    output.sort_by_key(|club| club.id);
    output
}
