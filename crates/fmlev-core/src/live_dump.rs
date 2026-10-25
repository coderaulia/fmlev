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

fn describe_dynamic(driver: &LinuxMemoryDriver, object: usize) -> String {
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
        parts.push(format!("{}={}", fourcc(key), describe_value(driver, tv)));
    }
    format!("DynamicReference[{}]", parts.join(", "))
}

fn describe_value(driver: &LinuxMemoryDriver, tv: usize) -> String {
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
            Some((name, _)) if name == "DynamicReference" => describe_dynamic(driver, ptr),
            Some((name, ns)) if name.starts_with("List`1") => {
                format!("object {ns}.{name} {}", describe_list(driver, ptr))
            }
            Some((name, ns)) => format!("object {ns}.{name}"),
            None => "object(unknown)".into(),
        },
        None => "undecodable".into(),
    }
}

/// Summarises a managed `List<T>`: element count and the class of each element (first 24).
fn describe_list(driver: &LinuxMemoryDriver, list: usize) -> String {
    let items = driver.read_ptr(list + 0x10).unwrap_or(0);
    let size = driver.read::<i32>(list + 0x18).unwrap_or(-1);
    if !valid_ptr(items) || !(0..=4096).contains(&size) {
        return "(unreadable)".into();
    }
    let classes: Vec<String> = (0..size.min(24) as usize)
        .map(|i| {
            let element = driver.read_ptr(items + 0x20 + i * 8).unwrap_or(0);
            class_name(driver, element)
                .map(|(n, ns)| format!("{ns}.{n}@{element:#x}"))
                .unwrap_or_else(|| "?".into())
        })
        .collect();
    format!("size={size} [{}]", classes.join(", "))
}

/// Enumerates bound properties whose source is a database record reference.
pub fn dump_bound_properties(
    driver: &LinuxMemoryDriver,
    binding_subsystem: usize,
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
            value: describe_value(driver, tv),
        });
    }
    out.sort_by(|a, b| {
        (a.reference_type, a.reference_id, &a.property)
            .cmp(&(b.reference_type, b.reference_id, &b.property))
    });
    out
}
