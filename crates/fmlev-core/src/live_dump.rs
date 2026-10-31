//! Read-only diagnostic: lists every property currently bound for record references
//! (club, team, stadium, nation, national team, continent, competition) in the FM26
//! BindingSubsystem. Nothing here writes to the game.

use fmlev_il2cpp::{types::read_cstring, Il2CppString};
use fmlev_mem::LinuxMemoryDriver;
use serde::Serialize;

use crate::context::inspector::{read_typed_value, DecodedTypedValue};

const ENTRY_SIZE: usize = 56;
const MAX_ENTRIES: usize = 200_000;

#[derive(Clone, Debug, Serialize)]
pub struct BoundProperty {
    pub reference_class: String,
    pub reference_type: i32,
    pub reference_id: i32,
    pub property: String,
    pub value_class: String,
    pub value: String,
}

fn valid_ptr(p: usize) -> bool {
    p > 0x10000 && p < 0x7fff_ffff_ffff
}

fn class_name(driver: &LinuxMemoryDriver, object: usize) -> Option<(String, String)> {
    if !valid_ptr(object) {
        return None;
    }
    let klass = driver.read_ptr(object).ok()?;
    if !valid_ptr(klass) {
        return None;
    }
    let name = read_cstring(driver, driver.read_ptr(klass + 0x10).ok()?, 128).ok()?;
    let ns = read_cstring(driver, driver.read_ptr(klass + 0x18).ok()?, 128).ok()?;
    Some((name, ns))
}

fn fourcc(key: u64) -> String {
    let bytes = (key as u32).to_be_bytes();
    if bytes.iter().all(|b| (0x20..0x7f).contains(b)) {
        format!("'{}'", String::from_utf8_lossy(&bytes))
    } else {
        format!("{key:#x}")
    }
}

fn describe_dynamic(driver: &LinuxMemoryDriver, object: usize, depth: u8) -> String {
    let Ok(entries) = driver.read_ptr(object + 0x18) else {
        return "DynamicReference(unreadable)".into();
    };
    let count = driver.read::<usize>(entries + 0x18).unwrap_or(0);
    if !valid_ptr(entries) || count > 64 {
        return "DynamicReference(out of bounds)".into();
    }
    let mut parts = Vec::new();
    for i in 0..count {
        let entry = entries + 0x20 + i * 24;
        let key = driver.read::<u64>(entry + 8).unwrap_or(0);
        let tv = driver.read_ptr(entry + 16).unwrap_or(0);
        parts.push(format!("{}={}", fourcc(key), describe_value(driver, tv, depth)));
    }
    format!("DynamicReference[{}]", parts.join(", "))
}

fn describe_value(driver: &LinuxMemoryDriver, tv: usize, depth: u8) -> String {
    if depth > 3 {
        return "(nested too deep)".into();
    }
    match read_typed_value(driver, tv) {
        Some(DecodedTypedValue::Signed(v)) => format!("i64 {v}"),
        Some(DecodedTypedValue::Unsigned(v)) => format!("u64 {v}"),
        Some(DecodedTypedValue::Float(v)) => format!("f64 {v}"),
        Some(DecodedTypedValue::Bool(v)) => format!("bool {v}"),
        Some(DecodedTypedValue::String(s)) => format!("str {s:?}"),
        Some(DecodedTypedValue::Reference { ref_type, m_index, .. }) => {
            format!("ref type={ref_type} id={m_index}")
        }
        Some(DecodedTypedValue::Object(ptr)) => match class_name(driver, ptr) {
            Some((name, _)) if name == "DynamicReference" => describe_dynamic(driver, ptr, depth + 1),
            Some((name, ns)) if name.starts_with("List`1") => {
                format!("object {ns}.{name} {}", describe_list(driver, ptr, depth + 1))
            }
            Some((name, ns)) => format!("object {ns}.{name}"),
            None => "object(unknown)".into(),
        },
        None => "undecodable".into(),
    }
}

/// Summarises a managed `List<T>`: element count and each element (first 24); typed values are decoded.
fn describe_list(driver: &LinuxMemoryDriver, list: usize, depth: u8) -> String {
    let items = driver.read_ptr(list + 0x10).unwrap_or(0);
    let size = driver.read::<i32>(list + 0x18).unwrap_or(-1);
    if !valid_ptr(items) || !(0..=4096).contains(&size) {
        return "(unreadable)".into();
    }
    let classes: Vec<String> = (0..size.min(24) as usize)
        .map(|i| {
            let element = driver.read_ptr(items + 0x20 + i * 8).unwrap_or(0);
            match class_name(driver, element) {
                Some((n, _)) if n.ends_with("TypedValue") || n.starts_with("TypedValue") => describe_value(driver, element, depth),
                Some((n, ns)) => format!("{ns}.{n}@{element:#x}"),
                None => "?".into(),
            }
        })
        .collect();
    format!("size={size} [{}]", classes.join(", "))
}

/// Enumerates bound properties whose source is a database record reference.
pub fn dump_bound_properties(
    driver: &LinuxMemoryDriver,
    binding_subsystem: usize,
) -> Vec<BoundProperty> {
    dump_bound_properties_where(driver, binding_subsystem, &|_, _| true)
}

/// Like `dump_bound_properties`, but `keep(reference_class, property)` is checked before a value is decoded, so
/// asking for one property does not pay for decoding every bound value.
pub fn dump_bound_properties_where(
    driver: &LinuxMemoryDriver,
    binding_subsystem: usize,
    keep: &dyn Fn(&str, &str) -> bool,
) -> Vec<BoundProperty> {
    let mut out = Vec::new();
    let (Ok(m_data), Ok(m_ctx)) = (
        driver.read_ptr(binding_subsystem + 0x78),
        driver.read_ptr(binding_subsystem + 0x88),
    ) else {
        return out;
    };
    if !valid_ptr(m_data) || !valid_ptr(m_ctx) {
        return out;
    }
    let (Ok(data_items), Ok(data_size), Ok(entries), Ok(count)) = (
        driver.read_ptr(m_data + 0x10),
        driver.read::<i32>(m_data + 0x18),
        driver.read_ptr(m_ctx + 0x18),
        driver.read::<i32>(m_ctx + 0x20),
    ) else {
        return out;
    };
    if !valid_ptr(data_items)
        || !valid_ptr(entries)
        || !(0..=MAX_ENTRIES as i32).contains(&data_size)
        || !(0..=MAX_ENTRIES as i32).contains(&count)
    {
        return out;
    }
    let mut raw = vec![0u8; count as usize * ENTRY_SIZE];
    if driver.read_bytes(entries + 0x20, &mut raw).is_err() {
        return out;
    }
    for entry in raw.chunks_exact(ENTRY_SIZE) {
        let ptr = |at: usize| usize::from_ne_bytes(entry[at..at + 8].try_into().unwrap());
        let (src, prop) = (ptr(8), ptr(16));
        let key = ptr(40) & 0xffff_ffff;
        if !valid_ptr(src) || !valid_ptr(prop) || key >= data_size as usize {
            continue;
        }
        let ref_obj = driver.read_ptr(src + 0x18).unwrap_or(0);
        let Some((ref_class, ns)) = class_name(driver, ref_obj) else {
            continue;
        };
        if ns != "FM.UI" || !ref_class.ends_with("Reference") {
            continue;
        }
        let reference_id = driver.read::<i32>(ref_obj + 0x20).unwrap_or(0);
        let reference_type = driver.read::<i32>(ref_obj + 0x24).unwrap_or(0);
        if reference_id <= 0 {
            continue;
        }
        let property = Il2CppString::read(driver, prop)
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                let p = driver.read_ptr(prop + 0x10).ok()?;
                Il2CppString::read(driver, p).ok().filter(|s| !s.is_empty())
            });
        let Some(property) = property else { continue };
        if !keep(&ref_class, &property) {
            continue;
        }
        let d_obj = driver.read_ptr(data_items + 0x20 + key * 8).unwrap_or(0);
        let tv = driver.read_ptr(d_obj + 0x30).unwrap_or(0);
        let value_class = class_name(driver, tv)
            .map(|(n, _)| n)
            .unwrap_or_else(|| "?".into());
        out.push(BoundProperty {
            reference_class: ref_class,
            reference_type,
            reference_id,
            property,
            value_class,
            value: describe_value(driver, tv, 0),
        });
    }
    out.sort_by(|a, b| {
        (a.reference_type, a.reference_id, &a.property)
            .cmp(&(b.reference_type, b.reference_id, &b.property))
    });
    out
}

/// The game's current date as the raw packed value of `GameReference.CurrentGameDate` (changes on every day
/// tick). `None` when the bindings are unavailable. Used to notice that days have passed, not to interpret the date.
pub fn read_game_date(session: &crate::session::LiveSession) -> Option<i64> {
    let binding = crate::context::inspector::resolve_binding_subsystem(session)?;
    dump_bound_properties_where(&session.driver, binding, &|class, property| class == "GameReference" && property == "CurrentGameDate")
        .into_iter()
        .find_map(|p| p.value.strip_prefix("i64 ").and_then(|v| v.trim().parse::<i64>().ok()))
}
