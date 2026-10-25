use std::collections::HashMap;

use crate::models::{
    ClubFinances, CompetitionDetails, NationDetails, Player,
    PlayerAttributes, PlayerContract, PlayerPositions,
};
use crate::session::LiveSession;

/// Apply the user's changed fields to fresh authoritative values, preserving untouched native ratings.
#[cfg(test)]
pub(crate) fn apply_changed_positions(previous: &PlayerPositions, requested: &PlayerPositions, native: &mut PlayerPositions) {
    macro_rules! patch {
        ($($field:ident),+ $(,)?) => { $(if requested.$field != previous.$field { native.$field = requested.$field; })+ };
    }
    patch!(gk, dl, dc, dr, wbl, wbr, dm, ml, mc, mr, aml, amc, amr, st, left_foot, right_foot);
}

pub(crate) fn unsupported_live_player_changes(previous: &Player, requested: &Player) -> Vec<String> {
    let mut comparable = requested.clone();
    comparable.positions = previous.positions.clone();
    if requested.positions != previous.positions { comparable.position = previous.position.clone(); }
    comparable.user_modified = previous.user_modified;
    comparable.current_ability = previous.current_ability;
    comparable.potential_ability = previous.potential_ability;
    comparable.contract.wage_per_week = previous.contract.wage_per_week;
    comparable.contract.expiry_year = previous.contract.expiry_year;
    for field in crate::native_player::native_attributes() {
        (field.set)(&mut comparable.attributes, (field.get)(&previous.attributes));
    }
    fn differences(path: &str, old: &serde_json::Value, new: &serde_json::Value, output: &mut Vec<String>) {
        if old == new { return; }
        if let (Some(old), Some(new)) = (old.as_object(), new.as_object()) {
            for (key, value) in new {
                let path = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
                differences(&path, &old[key], value, output);
            }
        } else { output.push(path.to_string()); }
    }
    let mut fields = Vec::new();
    differences("", &serde_json::to_value(previous).unwrap(), &serde_json::to_value(comparable).unwrap(), &mut fields);
    fields
}

#[cfg(test)]
pub(crate) fn is_positions_only_edit(previous: &Player, requested: &Player) -> bool {
    let mut comparable = requested.clone();
    comparable.positions = previous.positions.clone();
    comparable.position = previous.position.clone();
    comparable.user_modified = previous.user_modified;
    comparable == *previous
}

pub(crate) fn resolve_binding_subsystem(session: &LiveSession) -> Option<usize> {
    let base = session.bases.game_assembly_base?;
    let singleton = session.registry.get_singleton("GameSubsystems").ok()?;
    let fields = fmlev_il2cpp::read_static_singleton(&session.driver, base, singleton).ok()?;
    let list = session.driver.read_ptr(fields).ok()?;
    let items = session.driver.read_ptr(list + 0x10).ok()?;
    let count = session.driver.read::<i32>(list + 0x18).ok()?;
    if !(12..=1024).contains(&count) { return None; }
    let subsystem = session.driver.read_ptr(items + 0x20 + 11 * 8).ok()?;
    (subsystem > 0x10000).then_some(subsystem)
}

pub(crate) fn unknown_clubfinances() -> ClubFinances {
    ClubFinances {
        club_id: 0,
        club_name: String::new(),
        balance: 0,
        transfer_budget: 0,
        wage_budget: 0,
        league_name: String::new(),
        reputation: 0,
        wage_used: 0,
        training_facilities: 0,
        youth_facilities: 0,
        youth_recruitment: 0,
        junior_coaching: 0,
        data_analysis_facilities: 0,
        corporate_facilities: 0,
        stadium_name: String::new(),
        stadium_capacity: 0,
        stadium_expansion_cap: 0,
        sugar_daddy_type: String::new(),
        board_patience: 0,
        fan_confidence: 0,
        board_confidence: 0,
        transfer_retention_pct: 0,
        max_wage_allowance: 0,
        affiliates: Vec::new(),
    }
}

pub(crate) fn unknown_nationdetails() -> NationDetails {
    NationDetails {
        id: 0,
        name: String::new(),
        three_letter_code: String::new(),
        continent: String::new(),
        reputation: 0,
        youth_rating: 0,
        economic_factor: 0,
        state_of_development: 0,
        fa_financial_power: 0,
        game_importance: 0,
        tax_rate: 0,
        corruption_index: 0,
        youth_facilities_avg: 0,
        youth_coaching_avg: 0,
        youth_recruitment_avg: 0,
    }
}

pub(crate) fn unknown_competitiondetails() -> CompetitionDetails {
    CompetitionDetails {
        id: 0,
        name: String::new(),
        short_name: String::new(),
        nation_name: String::new(),
        nation_id: 0,
        reputation: 0,
        max_substitutes: 0,
        max_bench_size: 0,
        substitution_windows: 0,
        var_active: false,
        max_squad_size: 0,
        max_foreign_players: 0,
        min_homegrown_club: 0,
        min_homegrown_nation: 0,
        winner_prize_money: 0,
        tv_rights_pool: 0,
        yellow_cards_for_ban: 0,
    }
}

pub(crate) fn build_template_live_player(id: i32, name: String, position: String) -> Player {
    let pos_str = position;
    let positions = PlayerPositions::unknown();

    Player {
        id,
        name,
        club_id: 0,
        club_name: String::new(),
        age: 0,
        nationality: String::new(),
        position: pos_str,
        positions,
        current_ability: 0,
        potential_ability: 0,
        condition: 0,
        sharpness: 0,
        morale: 0,
        morale_label: String::new(),
        is_injured: false,
        is_suspended: false,
        contract: PlayerContract {
            wage_per_week: 0,
            expiry_year: 0,
            release_clause: 0,
            squad_status: String::new(),
            happiness: String::new(),
        },
        attributes: PlayerAttributes::default(),
        user_modified: false,
    }
}

/// Merges non-zero attribute fields from `src` into `dst`.
pub(crate) fn merge_player_attributes(dst: &mut PlayerAttributes, src: &PlayerAttributes) {
    macro_rules! merge_field {
        ($($field:ident),+ $(,)?) => {
            $(
                if src.$field > 0 {
                    dst.$field = src.$field;
                }
            )+
        };
    }
    merge_field!(
        marking, long_throws, punching, dirtiness, controversy,
        corners, crossing, dribbling, finishing, first_touch, free_kicks,
        heading, long_shots, passing, penalty_taking, tackling, technique,
        aggression, anticipation, bravery, composure, concentration, decisions,
        determination, flair, leadership, off_the_ball, positioning, teamwork,
        vision, work_rate,
        acceleration, agility, balance, jumping_reach, natural_fitness, pace,
        stamina, strength,
        consistency, important_matches, injury_proneness, versatility,
        adaptability, ambition, loyalty, pressure, professionalism,
        sportsmanship, temperament,
        aerial_reach, command_of_area, communication, eccentricity, handling,
        kicking, one_on_ones, reflexes, rushing_out, throwing,
    );
}


/// Decoded representation of a managed TypedValue payload.
#[derive(Debug, Clone, PartialEq)]
pub enum DecodedTypedValue {
    Signed(i64),
    Unsigned(u64),
    Float(f64),
    Bool(bool),
    String(String),
    Object(usize),
    Reference { ptr: usize, ref_type: i32, m_index: i32 },
}

pub(crate) fn read_il2cpp_class_name(
    driver: &fmlev_mem::LinuxMemoryDriver,
    obj: usize,
) -> Option<(String, String)> {
    if obj <= 0x10000 || obj >= 0x7fff_ffff_ffff {
        return None;
    }
    let klass = driver.read_ptr(obj).ok()?;
    if klass <= 0x10000 || klass >= 0x7fff_ffff_ffff {
        return None;
    }
    let name_ptr = driver.read_ptr(klass + 0x10).ok()?;
    if name_ptr <= 0x10000 || name_ptr >= 0x7fff_ffff_ffff {
        return None;
    }
    let name = fmlev_il2cpp::types::read_cstring(driver, name_ptr, 128).ok()?;
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_graphic()) {
        return None;
    }
    let ns_ptr = driver.read_ptr(klass + 0x18).unwrap_or(0);
    let ns = if ns_ptr > 0x10000 && ns_ptr < 0x7fff_ffff_ffff {
        fmlev_il2cpp::types::read_cstring(driver, ns_ptr, 128).unwrap_or_default()
    } else {
        String::new()
    };
    Some((name, ns))
}

/// Decodes a live `SI.Core.TypedValue` object at `val_tv`:
/// - `SI.Core.NumericTypedValue`: inline `DynamicNumber` (`Union` 8 bytes at `+0x18`, `NumberKind` `i32` at `+0x20`: `0 = SignedWhole`, `1 = UnsignedWhole`, `2 = FloatingPoint`)
/// - `SI.Core.ReferenceTypedValue`: managed object pointer at `+0x18` (`System.String`, `FM.UI.*Reference`, `SI.Bindable.DynamicReference`, etc.)
/// - `SI.Core.TypedValue<T>`: inline primitive at `+0x18`
pub fn read_typed_value(
    driver: &fmlev_mem::LinuxMemoryDriver,
    val_tv: usize,
) -> Option<DecodedTypedValue> {
    if val_tv <= 0x10000 || val_tv >= 0x7fff_ffff_ffff {
        return None;
    }
    let mut raw = [0u8; 16];
    driver.read_bytes(val_tv + 0x18, &mut raw).ok()?;
    let raw_u64 = u64::from_ne_bytes(raw[0..8].try_into().ok()?);
    let kind_i32 = i32::from_ne_bytes(raw[8..12].try_into().ok()?);

    match read_il2cpp_class_name(driver, val_tv) {
        Some((cls_name, _)) if cls_name == "NumericTypedValue" => match kind_i32 {
            0 => Some(DecodedTypedValue::Signed(
                crate::live_write::read_numeric_binding_integer(driver, val_tv).unwrap_or(raw_u64 as i64)
            )),
            1 => Some(DecodedTypedValue::Unsigned(raw_u64)),
            2 => Some(DecodedTypedValue::Float(f64::from_bits(raw_u64))),
            _ => None,
        },
        Some((cls_name, _)) if cls_name == "ReferenceTypedValue" => {
            let ptr = raw_u64 as usize;
            if ptr <= 0x10000 || ptr >= 0x7fff_ffff_ffff {
                return None;
            }
            if let Some((t_name, t_ns)) = read_il2cpp_class_name(driver, ptr) {
                if t_name == "String" && t_ns == "System" {
                    let s = fmlev_il2cpp::Il2CppString::read(driver, ptr).ok()?;
                    return Some(DecodedTypedValue::String(s));
                }
                if t_name.ends_with("Reference") && t_ns != "SI.Bindable" {
                    let m_index = driver.read::<i32>(ptr + 0x20).unwrap_or(0);
                    let ref_type = driver.read::<i32>(ptr + 0x24).unwrap_or(0);
                    return Some(DecodedTypedValue::Reference {
                        ptr,
                        ref_type,
                        m_index,
                    });
                }
                if t_ns == "System" {
                    if let Ok(boxed_val) = driver.read::<i32>(ptr + 0x10) {
                        return Some(DecodedTypedValue::Signed(boxed_val as i64));
                    }
                }
            }
            Some(DecodedTypedValue::Object(ptr))
        }
        Some((cls_name, _)) if cls_name.starts_with("TypedValue") => {
            Some(DecodedTypedValue::Signed(raw_u64 as i64))
        }
        _ => match kind_i32 {
            0 => Some(DecodedTypedValue::Signed(raw_u64 as i64)),
            1 => Some(DecodedTypedValue::Unsigned(raw_u64)),
            2 => Some(DecodedTypedValue::Float(f64::from_bits(raw_u64))),
            _ => None,
        },
    }
}

pub(crate) fn extract_binding_number(driver: &fmlev_mem::LinuxMemoryDriver, val_tv: usize) -> Option<u32> {
    match read_typed_value(driver, val_tv)? {
        DecodedTypedValue::Signed(v) => {
            if v >= 0 && v <= u32::MAX as i64 {
                Some(v as u32)
            } else {
                None
            }
        }
        DecodedTypedValue::Unsigned(v) => {
            if v <= u32::MAX as u64 {
                Some(v as u32)
            } else {
                None
            }
        }
        DecodedTypedValue::Float(f) => {
            if f.is_finite() && f >= 0.0 && f <= u32::MAX as f64 {
                Some(f.round() as u32)
            } else {
                None
            }
        }
        DecodedTypedValue::Bool(b) => Some(u32::from(b)),
        DecodedTypedValue::String(s) => {
            let clean = s.trim();
            if let Ok(n) = clean.parse::<u32>() {
                return Some(n);
            }
            if let Some(first) = clean.split(|c: char| !c.is_numeric()).find(|p| !p.is_empty()) {
                if let Ok(n) = first.parse::<u32>() {
                    return Some(n);
                }
            }
            None
        }
        DecodedTypedValue::Reference { .. } | DecodedTypedValue::Object(_) => None,
    }
}

pub(crate) fn extract_binding_string(driver: &fmlev_mem::LinuxMemoryDriver, val_tv: usize) -> Option<String> {
    if let Some(DecodedTypedValue::String(raw_s)) = read_typed_value(driver, val_tv) {
        let (full, short) = decode_fm_link_name(&raw_s);
        let s = if !full.is_empty() { full } else { short };
        let trimmed = s.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    None
}

pub(crate) fn extract_binding_display_string(
    driver: &fmlev_mem::LinuxMemoryDriver,
    val_tv: usize,
) -> Option<String> {
    if let Some(DecodedTypedValue::String(raw_s)) = read_typed_value(driver, val_tv) {
        let (full, short) = decode_fm_link_name(&raw_s);
        let s = if !short.is_empty() { short } else { full };
        let trimmed = s.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    None
}

pub(crate) fn extract_binding_ref(
    driver: &fmlev_mem::LinuxMemoryDriver,
    val_tv: usize,
) -> Option<(i32, i32)> {
    match read_typed_value(driver, val_tv)? {
        DecodedTypedValue::Reference {
            ref_type, m_index, ..
        } if m_index > 0 => Some((ref_type, m_index)),
        DecodedTypedValue::Object(ptr) if ptr > 0x10000 => {
            let m_index = driver.read::<i32>(ptr + 0x20).unwrap_or(0);
            let ref_type = driver.read::<i32>(ptr + 0x24).unwrap_or(0);
            if m_index > 0 && ref_type > 0 {
                Some((ref_type, m_index))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) fn apply_attribute_by_name(attrs: &mut PlayerAttributes, name: &str, raw_val: u32) {
    if raw_val == 0 || raw_val > 20 {
        return;
    }
    let val = raw_val as u8;
    let norm = name
        .trim()
        .to_lowercase()
        .strip_prefix("attribute")
        .unwrap_or(&name.trim().to_lowercase())
        .replace([' ', '_', '-', '(', ')'], "");

    match norm.as_str() {
        // Technical & Set Pieces
        "marking" => attrs.marking = val,
        "longthrows" => attrs.long_throws = val,
        "punching" => attrs.punching = val,
        "dirtiness" => attrs.dirtiness = val,
        "controversy" => attrs.controversy = val,
        "corners" | "cornertaking" => attrs.corners = val,
        "crossing" => attrs.crossing = val,
        "dribbling" => attrs.dribbling = val,
        "finishing" => attrs.finishing = val,
        "firsttouch" => attrs.first_touch = val,
        "freekicks" | "freekicktaking" => attrs.free_kicks = val,
        "heading" => attrs.heading = val,
        "longshots" => attrs.long_shots = val,
        "passing" => attrs.passing = val,
        "penaltytaking" | "penalties" => attrs.penalty_taking = val,
        "tackling" => attrs.tackling = val,
        "technique" => attrs.technique = val,

        // Mental
        "aggression" => attrs.aggression = val,
        "anticipation" => attrs.anticipation = val,
        "bravery" => attrs.bravery = val,
        "composure" => attrs.composure = val,
        "concentration" => attrs.concentration = val,
        "decisions" => attrs.decisions = val,
        "determination" => attrs.determination = val,
        "flair" => attrs.flair = val,
        "leadership" => attrs.leadership = val,
        "offtheball" => attrs.off_the_ball = val,
        "positioning" => attrs.positioning = val,
        "teamwork" => attrs.teamwork = val,
        "vision" => attrs.vision = val,
        "workrate" => attrs.work_rate = val,

        // Physical
        "acceleration" => attrs.acceleration = val,
        "agility" => attrs.agility = val,
        "balance" => attrs.balance = val,
        "jumpingreach" | "jumping" => attrs.jumping_reach = val,
        "naturalfitness" => attrs.natural_fitness = val,
        "pace" => attrs.pace = val,
        "stamina" => attrs.stamina = val,
        "strength" => attrs.strength = val,

        // Goalkeeping
        "aerialreach" => attrs.aerial_reach = val,
        "commandofarea" => attrs.command_of_area = val,
        "communication" => attrs.communication = val,
        "eccentricity" => attrs.eccentricity = val,
        "handling" => attrs.handling = val,
        "kicking" => attrs.kicking = val,
        "oneonones" => attrs.one_on_ones = val,
        "reflexes" => attrs.reflexes = val,
        "rushingout" | "rushingouttendency" => attrs.rushing_out = val,
        "throwing" => attrs.throwing = val,

        // Hidden
        "consistency" => attrs.consistency = val,
        "importantmatches" => attrs.important_matches = val,
        "injuryproneness" => attrs.injury_proneness = val,
        "versatility" => attrs.versatility = val,
        "adaptability" => attrs.adaptability = val,
        "ambition" => attrs.ambition = val,
        "loyalty" => attrs.loyalty = val,
        "pressure" => attrs.pressure = val,
        "professionalism" => attrs.professionalism = val,
        "sportsmanship" => attrs.sportsmanship = val,
        "temperament" => attrs.temperament = val,
        _ => {}
    }
}

pub(crate) fn read_object_lookup_unid(
    driver: &fmlev_mem::LinuxMemoryDriver,
    val_tv: usize,
) -> Option<u32> {
    if val_tv <= 0x10000 || val_tv >= 0x7fff_ffff_ffff {
        return None;
    }
    let dyn_ref = driver.read_ptr(val_tv + 0x18).ok()?;
    if dyn_ref <= 0x10000 || dyn_ref >= 0x7fff_ffff_ffff {
        return None;
    }
    let entries = driver.read_ptr(dyn_ref + 0x18).ok()?;
    if entries <= 0x10000 || entries >= 0x7fff_ffff_ffff {
        return None;
    }
    let count = driver
        .read::<i32>(dyn_ref + 0x20)
        .unwrap_or(1)
        .clamp(1, 8) as usize;
    for k in 0..count {
        let entry_addr = entries + 0x20 + k * 24;
        let key = driver.read::<u32>(entry_addr + 0x08).unwrap_or(0);
        if key == 0x756e6964 {
            let num_tv = driver.read_ptr(entry_addr + 0x10).unwrap_or(0);
            if let Some(unid) = extract_binding_number(driver, num_tv) {
                if unid > 0 {
                    return Some(unid);
                }
            }
        }
    }
    None
}

pub(crate) fn fm_nation_name_from_unid(unid: u32) -> Option<&'static str> {
    match unid {
        113 => Some("Indonesia"),
        115 => Some("Iraq"),
        116 => Some("Japan"),
        757 => Some("Belgium"),
        761 => Some("Croatia"),
        765 => Some("England"),
        769 => Some("France"),
        771 => Some("Germany"),
        776 => Some("Italy"),
        784 => Some("Netherlands"),
        786 => Some("Norway"),
        788 => Some("Portugal"),
        796 => Some("Spain"),
        801 => Some("Wales"),
        1435 => Some("Australia"),
        1649 => Some("Argentina"),
        1651 => Some("Brazil"),
        _ => None,
    }
}

pub(crate) fn read_type_info_name(
    driver: &fmlev_mem::LinuxMemoryDriver,
    type_info_dyn_ref: usize,
) -> Option<String> {
    if type_info_dyn_ref <= 0x10000 || type_info_dyn_ref >= 0x7fff_ffff_ffff {
        return None;
    }
    let ti_entries = driver.read_ptr(type_info_dyn_ref + 0x18).ok()?;
    if ti_entries <= 0x10000 || ti_entries >= 0x7fff_ffff_ffff {
        return None;
    }
    let count = driver
        .read::<i32>(type_info_dyn_ref + 0x20)
        .unwrap_or(6)
        .clamp(1, 16) as usize;
    for k in 0..count {
        let entry_addr = ti_entries + 0x20 + k * 24;
        let key = driver.read::<u32>(entry_addr + 0x08).unwrap_or(0);
        if key == 0x6e616d65 {
            let name_tv = driver.read_ptr(entry_addr + 0x10).unwrap_or(0);
            if let Some(s) = extract_binding_string(driver, name_tv) {
                return Some(s);
            }
        }
    }
    None
}

pub(crate) fn read_attribute_values_number(
    driver: &fmlev_mem::LinuxMemoryDriver,
    values_dyn_ref: usize,
) -> Option<u32> {
    if values_dyn_ref <= 0x10000 || values_dyn_ref >= 0x7fff_ffff_ffff {
        return None;
    }
    let val_entries = driver.read_ptr(values_dyn_ref + 0x18).ok()?;
    if val_entries <= 0x10000 || val_entries >= 0x7fff_ffff_ffff {
        return None;
    }
    let count = driver
        .read::<i32>(values_dyn_ref + 0x20)
        .unwrap_or(4)
        .clamp(1, 16) as usize;
    for k in 0..count {
        let entry_addr = val_entries + 0x20 + k * 24;
        let key = driver.read::<u32>(entry_addr + 0x08).unwrap_or(0);
        if key == 0x7074766c || key == 0x44667a45 {
            let num_tv = driver.read_ptr(entry_addr + 0x10).unwrap_or(0);
            if let Some(n) = extract_binding_number(driver, num_tv) {
                if (1..=20).contains(&n) {
                    return Some(n);
                }
            }
        }
    }
    None
}

pub(crate) fn extract_open_profile_attribute_list(
    driver: &fmlev_mem::LinuxMemoryDriver,
    list_tv: usize,
    dyn_ref_props: &HashMap<(usize, String), usize>,
    attrs: &mut PlayerAttributes,
) {
    if list_tv <= 0x10000 || list_tv >= 0x7fff_ffff_ffff {
        return;
    }
    let list_ptr = driver.read_ptr(list_tv + 0x18).unwrap_or(0);
    if list_ptr <= 0x10000 || list_ptr >= 0x7fff_ffff_ffff {
        return;
    }
    let items_ptr = driver.read_ptr(list_ptr + 0x10).unwrap_or(0);
    let size = driver.read::<i32>(list_ptr + 0x18).unwrap_or(0);
    if items_ptr <= 0x10000 || size <= 0 {
        return;
    }

    let clamped = (size as usize).min(32);
    for i in 0..clamped {
        let elem_tv = driver.read_ptr(items_ptr + 0x20 + i * 8).unwrap_or(0);
        if elem_tv <= 0x10000 {
            continue;
        }
        let row_dyn_ref = driver.read_ptr(elem_tv + 0x18).unwrap_or(0);
        if row_dyn_ref <= 0x10000 {
            continue;
        }

        let mut attr_name: Option<String> = None;
        let mut attr_val: Option<u32> = None;

        // Path A: Direct row_dyn_ref Dictionary<uint, TypedValue> entries at row_dyn_ref + 0x18
        let row_entries = driver.read_ptr(row_dyn_ref + 0x18).unwrap_or(0);
        let row_count = driver
            .read::<i32>(row_dyn_ref + 0x20)
            .unwrap_or(2)
            .clamp(1, 8) as usize;
        if row_entries > 0x10000 && row_entries < 0x7fff_ffff_ffff {
            for r in 0..row_count {
                let child_tv = driver
                    .read_ptr(row_entries + 0x20 + r * 24 + 0x10)
                    .unwrap_or(0);
                let child_dyn_ref = if child_tv > 0x10000 {
                    driver.read_ptr(child_tv + 0x18).unwrap_or(0)
                } else {
                    0
                };
                if child_dyn_ref > 0x10000 {
                    if attr_name.is_none() {
                        attr_name = read_type_info_name(driver, child_dyn_ref);
                    }
                    if attr_val.is_none() {
                        attr_val = read_attribute_values_number(driver, child_dyn_ref);
                    }
                }
            }
        }

        // Path B: Fallback to m_ctx mappings for (row_dyn_ref, "AttributeTypeInfo") & (row_dyn_ref, "AttributeValues")
        if attr_name.is_none() {
            if let Some(&ti_tv) = dyn_ref_props.get(&(row_dyn_ref, "AttributeTypeInfo".to_string()))
            {
                let ti_dyn_ref = driver.read_ptr(ti_tv + 0x18).unwrap_or(0);
                attr_name = read_type_info_name(driver, ti_dyn_ref);
            }
        }
        if attr_val.is_none() {
            if let Some(&av_tv) = dyn_ref_props.get(&(row_dyn_ref, "AttributeValues".to_string())) {
                let av_dyn_ref = driver.read_ptr(av_tv + 0x18).unwrap_or(0);
                attr_val = read_attribute_values_number(driver, av_dyn_ref);
            }
        }

        if let (Some(name), Some(val)) = (attr_name, attr_val) {
            apply_attribute_by_name(attrs, &name, val);
        }
    }
}

pub(crate) fn extract_footedness(
    driver: &fmlev_mem::LinuxMemoryDriver,
    val_tv: usize,
    pos: &mut PlayerPositions,
) {
    if val_tv <= 0x10000 || val_tv >= 0x7fff_ffff_ffff {
        return;
    }
    let dyn_ref = driver.read_ptr(val_tv + 0x18).unwrap_or(0);
    if dyn_ref <= 0x10000 || dyn_ref >= 0x7fff_ffff_ffff {
        return;
    }
    let entries = driver.read_ptr(dyn_ref + 0x18).unwrap_or(0);
    let count = driver
        .read::<i32>(dyn_ref + 0x20)
        .unwrap_or(10)
        .clamp(1, 16) as usize;
    if entries <= 0x10000 || entries >= 0x7fff_ffff_ffff {
        return;
    }

    for k in 0..count {
        let ea = entries + 0x20 + k * 24;
        if driver.read::<i32>(ea).unwrap_or(-1) < 0 { continue; }
        let key = driver.read::<u32>(ea + 0x08).unwrap_or(0);
        let num_tv = driver.read_ptr(ea + 0x10).unwrap_or(0);
        if let Some(n) = extract_binding_number(driver, num_tv).filter(|n| (1..=20).contains(n)) {
            if key == 0x32342020 {
                // "24  " -> LeftFootedness
                pos.left_foot = n as u8;
            } else if key == 0x32332020 {
                // "23  " -> RightFootedness
                pos.right_foot = n as u8;
            }
        }
    }
}

pub(crate) fn extract_position_aptitude_list(
    driver: &fmlev_mem::LinuxMemoryDriver,
    val_tv: usize,
    pos: &mut PlayerPositions,
) {
    if val_tv <= 0x10000 || val_tv >= 0x7fff_ffff_ffff {
        return;
    }
    let list_ptr = driver.read_ptr(val_tv + 0x18).unwrap_or(0);
    if list_ptr <= 0x10000 || list_ptr >= 0x7fff_ffff_ffff {
        return;
    }
    let items = driver.read_ptr(list_ptr + 0x10).unwrap_or(0);
    let size = driver
        .read::<i32>(list_ptr + 0x18)
        .unwrap_or(0)
        .clamp(0, 32) as usize;
    if items <= 0x10000 || items >= 0x7fff_ffff_ffff {
        return;
    }

    for i in 0..size {
        let etv = driver.read_ptr(items + 0x20 + i * 8).unwrap_or(0);
        let row_dyn = if etv > 0x10000 {
            driver.read_ptr(etv + 0x18).unwrap_or(0)
        } else {
            0
        };
        if row_dyn <= 0x10000 {
            continue;
        }
        let re = driver.read_ptr(row_dyn + 0x18).unwrap_or(0);
        let rc = driver
            .read::<i32>(row_dyn + 0x20)
            .unwrap_or(3)
            .clamp(1, 8) as usize;
        if re <= 0x10000 {
            continue;
        }

        let mut mask = 0u64;
        let mut rating = 0u8;
        for k in 0..rc {
            let ea = re + 0x20 + k * 24;
            if driver.read::<i32>(ea).unwrap_or(-1) < 0 { continue; }
            let key = driver.read::<u32>(ea + 0x08).unwrap_or(0);
            let num_tv = driver.read_ptr(ea + 0x10).unwrap_or(0);
            if key == 0x506f7369 {
                // "Posi"
                if let Some(m) = extract_binding_number(driver, num_tv) {
                    mask = m as u64;
                }
            } else if key == 0x7074766c {
                // "ptvl"
                if let Some(r) = extract_binding_number(driver, num_tv).filter(|r| (1..=20).contains(r)) {
                    rating = r as u8;
                }
            }
        }

        if rating > 0 {
            match mask {
                1 => pos.gk = rating,
                4 => pos.dr = rating,
                8 => pos.dl = rating,
                16 => pos.dc = rating,
                32 => pos.wbr = rating,
                64 => pos.wbl = rating,
                128 => pos.dm = rating,
                256 => pos.mr = rating,
                512 => pos.ml = rating,
                1024 => pos.mc = rating,
                2048 => pos.amr = rating,
                4096 => pos.aml = rating,
                8192 => pos.amc = rating,
                16384 => pos.st = rating,
                _ => {}
            }
        }
    }
}

// A TeamName can describe a reserve/youth squad sharing its parent Club ID.
// Prefer the actual ClubReference name so it cannot contaminate the club registry.
pub(crate) fn resolve_bound_club_name(
    club_id: i32,
    fallback: String,
    ref_names: &HashMap<(i32, i32), String>,
) -> String {
    ref_names.get(&(3, club_id))
        .filter(|name| !name.trim().is_empty())
        .cloned()
        .unwrap_or(fallback)
}

/// Extracts live bound players from FM26's `SI.Bindable.BindingSubsystem` in memory.
pub(crate) fn extract_live_bound_players(
    driver: &fmlev_mem::LinuxMemoryDriver,
    b_sub: usize,
) -> (Vec<Player>, HashMap<(i32, i32), String>) {
    let m_data = driver.read_ptr(b_sub + 0x78).unwrap_or(0);
    let m_ctx = driver.read_ptr(b_sub + 0x88).unwrap_or(0);
    if m_data <= 0x10000 || m_ctx <= 0x10000 {
        return (Vec::new(), HashMap::new());
    }

    let data_items = driver.read_ptr(m_data + 0x10).unwrap_or(0);
    let data_size = driver.read::<i32>(m_data + 0x18).unwrap_or(0);
    let entries = driver.read_ptr(m_ctx + 0x18).unwrap_or(0);
    let count = driver.read::<i32>(m_ctx + 0x20).unwrap_or(0);

    if data_items <= 0x10000 || entries <= 0x10000 || count <= 0 || data_size <= 0 {
        return (Vec::new(), HashMap::new());
    }

    let max_entries = (count as usize).min(50_000);
    let mut raw_entries = vec![0u8; max_entries * 56];
    if driver.read_bytes(entries + 0x20, &mut raw_entries).is_err() {
        return (Vec::new(), HashMap::new());
    }

    #[derive(Default)]
    struct PartialPerson {
        name: String,
        short_name: String,
        club_name: String,
        nationality: String,
        position: String,
        age: u8,
        condition: u16,
        sharpness: u16,
        morale: u8,
        current_ability: u8,
        potential_ability: u8,
        club_ref: Option<i32>,
        team_ref: Option<i32>,
        report_team_ref: Option<i32>,
        nation_ref: Option<i32>,
        attr_list_tvs: Vec<usize>,
        pos_list_tv: Option<usize>,
        foot_tv: Option<usize>,
        attributes: Option<PlayerAttributes>,
    }

    let mut map: HashMap<i32, PartialPerson> = HashMap::new();
    let mut ref_names: HashMap<(i32, i32), String> = HashMap::new();
    let mut team_to_club: HashMap<i32, i32> = HashMap::new();
    let mut club_to_team: HashMap<i32, i32> = HashMap::new();
    let mut dyn_ref_props: HashMap<(usize, String), usize> = HashMap::new();

    for i in 0..max_entries {
        let base_off = i * 56;
        if i32::from_ne_bytes(raw_entries[base_off..base_off + 4].try_into().unwrap()) < 0 { continue; }
        let src_tv = usize::from_ne_bytes(
            raw_entries[base_off + 8..base_off + 16]
                .try_into()
                .unwrap_or([0; 8]),
        );
        let prop_ptr = usize::from_ne_bytes(
            raw_entries[base_off + 16..base_off + 24]
                .try_into()
                .unwrap_or([0; 8]),
        );
        let data_key = (u64::from_ne_bytes(
            raw_entries[base_off + 40..base_off + 48]
                .try_into()
                .unwrap_or([0; 8]),
        ) & 0xFFFF_FFFF) as usize;

        if src_tv <= 0x10000 || prop_ptr <= 0x10000 || data_key >= (data_size as usize) {
            continue;
        }

        let ref_obj = driver.read_ptr(src_tv + 0x18).unwrap_or(0);
        if ref_obj <= 0x10000 {
            continue;
        }

        let m_idx = driver.read::<i32>(ref_obj + 0x20).unwrap_or(0);
        let t_typ = driver.read::<i32>(ref_obj + 0x24).unwrap_or(0);

        let p_str_ptr = driver.read_ptr(prop_ptr + 0x10).unwrap_or(0);
        let prop_name = fmlev_il2cpp::Il2CppString::read(driver, p_str_ptr)
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| fmlev_il2cpp::Il2CppString::read(driver, prop_ptr).ok())
            .unwrap_or_default();

        if prop_name.is_empty() {
            continue;
        }

        let d_obj = driver.read_ptr(data_items + 0x20 + data_key * 8).unwrap_or(0);
        let val_tv = if d_obj > 0x10000 {
            driver.read_ptr(d_obj + 0x30).unwrap_or(0)
        } else {
            0
        };
        if val_tv <= 0x10000 {
            continue;
        }

        // Record DynamicReference (t_typ == -1) AttributeTypeInfo / AttributeValues mappings
        if t_typ == -1 {
            if matches!(prop_name.as_str(), "AttributeTypeInfo" | "AttributeValues") {
                dyn_ref_props.insert((ref_obj, prop_name), val_tv);
            }
            continue;
        }

        // Collect reference names and Team -> Club chains for Club (3), Nation (10), Team (24), Comp (25)
        if matches!(t_typ, 3 | 10 | 24 | 25) && m_idx > 0 {
            match prop_name.as_str() {
                "Name" | "FullName" | "ClubName" | "TeamName" | "NationName" => {
                    if let Some(s) = extract_binding_display_string(driver, val_tv) {
                        ref_names.insert((t_typ, m_idx), s);
                    }
                }
                "ThreeLetterName" | "ShortName" => {
                    if let Some(s) = extract_binding_display_string(driver, val_tv) {
                        ref_names.entry((t_typ, m_idx)).or_insert(s);
                    }
                }
                "ObjectLookupData" if t_typ == 10 => {
                    if let Some(unid) = read_object_lookup_unid(driver, val_tv) {
                        if let Some(nation_name) = fm_nation_name_from_unid(unid) {
                            ref_names
                                .entry((10, m_idx))
                                .or_insert_with(|| nation_name.to_string());
                        }
                    }
                }
                "Club" | "ClubForObjectLookupData" if t_typ == 24 => {
                    if let Some((target_type, target_idx)) = extract_binding_ref(driver, val_tv) {
                        if target_type == 3 && target_idx > 0 {
                            team_to_club.insert(m_idx, target_idx);
                            club_to_team.entry(target_idx).or_insert(m_idx);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }

        if t_typ != 1 || m_idx <= 0 {
            continue;
        }

        let entry = map.entry(m_idx).or_default();
        match prop_name.as_str() {
            "Name" | "FullName" => {
                if let Some(s) = extract_binding_string(driver, val_tv) {
                    if entry.name.is_empty()
                        || entry.name.contains(". ")
                        || s.len() > entry.name.len()
                    {
                        entry.name = s;
                    }
                }
            }
            "InitialSurname" | "ShirtName" => {
                if let Some(s) = extract_binding_string(driver, val_tv) {
                    if entry.short_name.is_empty() {
                        entry.short_name = s.clone();
                    }
                    if entry.name.is_empty() {
                        entry.name = s;
                    }
                }
            }
            "Club" => {
                if let Some((target_type, target_idx)) = extract_binding_ref(driver, val_tv) {
                    if target_type == 3 {
                        entry.club_ref = Some(target_idx);
                    } else if target_type == 24 {
                        entry.team_ref = Some(target_idx);
                    }
                } else if let Some(s) = extract_binding_display_string(driver, val_tv) {
                    if entry.club_name.is_empty() {
                        entry.club_name = s;
                    }
                }
            }
            "Team" => {
                if let Some((target_type, target_idx)) = extract_binding_ref(driver, val_tv) {
                    if target_type == 24 {
                        entry.team_ref = Some(target_idx);
                    } else if target_type == 3 {
                        entry.club_ref = Some(target_idx);
                    }
                } else if let Some(s) = extract_binding_display_string(driver, val_tv) {
                    if entry.club_name.is_empty() {
                        entry.club_name = s;
                    }
                }
            }
            "ReportTeam" => {
                if let Some((target_type, target_idx)) = extract_binding_ref(driver, val_tv) {
                    if target_type == 24 {
                        entry.report_team_ref = Some(target_idx);
                    }
                }
            }
            "ClubName" | "TeamName" | "PlayerClub" | "PlayerClubName" | "ContractClub" => {
                if let Some(s) = extract_binding_display_string(driver, val_tv) {
                    if entry.club_name.is_empty() {
                        entry.club_name = s;
                    }
                }
            }
            "Nation" => {
                if let Some((target_type, target_idx)) = extract_binding_ref(driver, val_tv) {
                    if target_type == 10 {
                        entry.nation_ref = Some(target_idx);
                    }
                } else if let Some(s) = extract_binding_display_string(driver, val_tv) {
                    if entry.nationality.is_empty() {
                        entry.nationality = s;
                    }
                }
            }
            "Nationality" | "PlayerNationality" | "NationName" => {
                if let Some(s) = extract_binding_display_string(driver, val_tv) {
                    if entry.nationality.is_empty() {
                        entry.nationality = s;
                    }
                }
            }
            "PositionCombinedString" => {
                if let Some(s) = extract_binding_display_string(driver, val_tv) {
                    entry.position = s;
                }
            }
            "BestPositionShortString" | "Position" | "PositionString" => {
                if let Some(s) = extract_binding_display_string(driver, val_tv) {
                    if entry.position.is_empty() {
                        entry.position = s;
                    }
                }
            }
            "Age" | "PlayerAge" => {
                if let Some(n) = extract_binding_number(driver, val_tv) {
                    if (14..=65).contains(&n) {
                        entry.age = n as u8;
                    }
                }
            }
            "PlayerCurrentAbility" | "CurrentAbility" | "CA" => {
                if let Some(n) = extract_binding_number(driver, val_tv) {
                    if (1..=200).contains(&n) {
                        entry.current_ability = n as u8;
                    }
                }
            }
            "PlayerPotentialAbility" | "PotentialAbility" | "PA" => {
                if let Some(n) = extract_binding_number(driver, val_tv) {
                    if (1..=200).contains(&n) {
                        entry.potential_ability = n as u8;
                    }
                }
            }
            "Condition" => {
                if let Some(n) = extract_binding_number(driver, val_tv) {
                    if n > 0 && n <= 100 {
                        entry.condition = (n as u16) * 100;
                    } else if n > 100 && n <= 10000 {
                        entry.condition = n as u16;
                    }
                }
            }
            "Sharpness" => {
                if let Some(n) = extract_binding_number(driver, val_tv) {
                    if n > 0 && n <= 100 {
                        entry.sharpness = (n as u16) * 100;
                    } else if n > 100 && n <= 10000 {
                        entry.sharpness = n as u16;
                    }
                }
            }
            "Morale" | "HierarchyOverallHappinessLevel" => {
                if let Some(n) = extract_binding_number(driver, val_tv) {
                    if n > 0 && n <= 25 {
                        entry.morale = ((n as u8) * 5).min(100);
                    } else if n > 25 && n <= 100 {
                        entry.morale = n as u8;
                    }
                }
            }
            "AttributesMental"
            | "AttributesPhysical"
            | "AttributesSetPieces"
            | "AttributesTechnical"
            | "AttributesGoalkeeping" => {
                entry.attr_list_tvs.push(val_tv);
            }
            "PositionAptitudeList" => {
                entry.pos_list_tv = Some(val_tv);
            }
            "Footedness" => {
                entry.foot_tv = Some(val_tv);
            }
            other => {
                if let Some(n) = extract_binding_number(driver, val_tv) {
                    if (1..=20).contains(&n) {
                        let attrs = entry.attributes.get_or_insert_with(PlayerAttributes::default);
                        apply_attribute_by_name(attrs, other, n);
                    }
                }
            }
        }
    }

    let resolve_team_or_club_name = |team_idx: Option<i32>, club_idx: Option<i32>| -> Option<String> {
        if let Some(c_idx) = club_idx {
            if let Some(name) = ref_names.get(&(3, c_idx)) {
                return Some(name.clone());
            }
        }
        if let Some(t_idx) = team_idx {
            if let Some(name) = ref_names.get(&(24, t_idx)) {
                return Some(name.clone());
            }
            if let Some(&c_idx) = team_to_club.get(&t_idx) {
                if let Some(name) = ref_names.get(&(3, c_idx)) {
                    return Some(name.clone());
                }
            }
        }
        if let Some(c_idx) = club_idx {
            if let Some(&t_idx) = club_to_team.get(&c_idx) {
                if let Some(name) = ref_names.get(&(24, t_idx)) {
                    return Some(name.clone());
                }
            }
        }
        None
    };

    let mut out = Vec::new();
    for (id, mut p) in map {
        let display_name = if !p.name.is_empty() {
            p.name
        } else if !p.short_name.is_empty() {
            p.short_name
        } else {
            continue;
        };

        if p.nationality.is_empty() {
            if let Some(n_idx) = p.nation_ref {
                if let Some(n_name) = ref_names.get(&(10, n_idx)) {
                    p.nationality = n_name.clone();
                }
            }
        }

        if p.club_name.is_empty() {
            if let Some(c_name) = resolve_team_or_club_name(p.team_ref, p.club_ref)
                .or_else(|| resolve_team_or_club_name(p.report_team_ref, None))
            {
                p.club_name = c_name;
            }
        }

        if !p.attr_list_tvs.is_empty() {
            let attrs = p.attributes.get_or_insert_with(PlayerAttributes::default);
            for list_tv in &p.attr_list_tvs {
                extract_open_profile_attribute_list(driver, *list_tv, &dyn_ref_props, attrs);
            }
        }

        let mut live_pos: Option<PlayerPositions> = None;
        if let Some(pos_tv) = p.pos_list_tv {
            let mut pos = PlayerPositions::unknown();
            extract_position_aptitude_list(driver, pos_tv, &mut pos);
            live_pos = Some(pos);
        }
        if let Some(foot_tv) = p.foot_tv {
            let pos = live_pos.get_or_insert_with(PlayerPositions::unknown);
            extract_footedness(driver, foot_tv, pos);
        }

        let mut player = build_template_live_player(id, display_name, p.position.clone());
        player.club_id = p.club_ref
            .or_else(|| p.team_ref.and_then(|team| team_to_club.get(&team).copied()))
            .or_else(|| p.report_team_ref.and_then(|team| team_to_club.get(&team).copied()))
            .unwrap_or(0);
        if let Some(pos) = live_pos {
            player.positions = pos;
        }
        player.club_name = resolve_bound_club_name(player.club_id, p.club_name, &ref_names);
        if !p.nationality.is_empty() {
            player.nationality = p.nationality;
        }
        if p.age > 0 {
            player.age = p.age;
        }
        if p.current_ability > 0 {
            player.current_ability = p.current_ability;
        }
        if p.potential_ability > 0 {
            player.potential_ability = p.potential_ability;
        }
        if p.condition > 0 {
            player.condition = p.condition;
        }
        if p.sharpness > 0 {
            player.sharpness = p.sharpness;
        }
        if p.morale > 0 {
            player.morale = p.morale;
        }
        if let Some(attrs) = p.attributes {
            player.attributes = attrs;
        }
        out.push(player);
    }
    out.sort_by_key(|p| p.id);
    (out, ref_names)
}

/// Decodes FM26's `\x01<Base64Metadata>\x02<ShortName>\x01` link strings into `(full_name, short_name)`.
pub fn decode_fm_link_name(raw: &str) -> (String, String) {
    if let Some(start) = raw.find('\x02') {
        let rest = &raw[start + 1..];
        let end = rest.find('\x01').unwrap_or(rest.len());
        let short_name = rest[..end].trim().to_string();

        let b64_part = raw[..start].trim_start_matches('\x01').trim();
        if let Some(decoded) = decode_base64_simple(b64_part) {
            let text = String::from_utf8_lossy(&decoded);
            if let Some(idx) = text.find("Click to view ") {
                let after = &text[idx + "Click to view ".len()..];
                let name_end = after
                    .find("'s profile")
                    .or_else(|| after.find("’s profile"))
                    .or_else(|| after.find("' profile"))
                    .or_else(|| after.find("’ profile"))
                    .or_else(|| after.find('\0'))
                    .unwrap_or(after.len());
                let full = after[..name_end].trim().to_string();
                if !full.is_empty() {
                    return (full, short_name);
                }
            }
        }
        (short_name.clone(), short_name)
    } else {
        let clean = raw.trim_matches(|c: char| c.is_control()).trim().to_string();
        (clean.clone(), clean)
    }
}

pub(crate) fn decode_base64_simple(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u8;
    for b in input.bytes() {
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => continue,
        };
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

/// Club fields with a live write path. Everything else is a UI-only or fixture value.
const LIVE_CLUB_WRITABLE: [&str; 14] = [
    "balance",
    "transfer_budget",
    "training_facilities",
    "youth_facilities",
    "junior_coaching",
    "youth_recruitment",
    "corporate_facilities",
    "data_analysis_facilities",
    "sugar_daddy_type",
    "reputation",
    "stadium_capacity",
    "stadium_expansion_cap",
    "club_id",
    "club_name",
];

pub(crate) fn unsupported_live_club_changes(
    previous: &crate::models::ClubFinances,
    requested: &crate::models::ClubFinances,
) -> Vec<String> {
    let old = serde_json::to_value(previous).unwrap();
    let new = serde_json::to_value(requested).unwrap();
    let (Some(old), Some(new)) = (old.as_object(), new.as_object()) else { return Vec::new(); };
    new.iter()
        .filter(|(key, value)| old.get(*key) != Some(*value) && !LIVE_CLUB_WRITABLE.contains(&key.as_str()))
        .map(|(key, _)| key.clone())
        .collect()
}
