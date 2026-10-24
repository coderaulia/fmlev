//! Live memory write path for FM26 IL2CPP / BindingSubsystem.
//!
//! # Architecture & Authoritative Storage
//! In FM26, Football Manager's simulation engine and database tables are **not** managed
//! IL2CPP objects. They reside entirely within Sports Interactive's native C++ simulation
//! engine (`fm.exe`).
//!
//! The Unity / IL2CPP layer (`GameAssembly.dll`) functions as the UI and presentation frontend.
//! Entities are referenced across the interop boundary using lightweight handles:
//! - `FM.UI.PersonReference : DatabaseRecordReference` (`m_index` @ 0x20, `Type` @ 0x24)
//! - `FM.UI.ClubReference : DatabaseRecordReference`
//! - `FM.UI.DatabaseTableType` (`Award = 0`, `Person = 1`, `City = 2`, `Club = 3`, `Nation = 10`, etc.)
//!
//! The UI displays values supplied by the native engine via `GameInteropSubsystem`
//! (`MarshallingWrappers.FM_GamePlugin_IGamePlugin_NativeWrapper` -> `IGamePlugin*`).
//! Deserialized binding values are cached in UI objects inside `SI.Bindable.BindingSubsystem`:
//! - `Bindings.Data.m_value` @ 0x30 -> `TypedValue*`
//! - `TypedValue<T>.m_value` @ 0x18
//! - `NumericTypedValue` is `TypedValue<DynamicNumber>` with `DynamicNumber`:
//!   - `m_value` union (`SignedWhole` i64 / `UnsignedWhole` u64 / `FloatingPoint` f64) at offset `+0x18`
//!   - `NumberKind` enum (0 = SignedWhole, 1 = UnsignedWhole, 2 = FloatingPoint) at offset `+0x20`
//!
//! ## Open Player Attributes Binding (Section 5 Discovery)
//! For an open player profile (`PersonReference` `m_index`), player attributes are bound
//! under 4 `List<TypedValue>` properties in `BindingSubsystem`:
//! - `AttributesMental`
//! - `AttributesPhysical`
//! - `AttributesSetPieces`
//! - `AttributesTechnical`
//!
//! Each item in those lists is a `ReferenceTypedValue` pointing to a row `SI.Bindable.DynamicReference`
//! (`Record : Dictionary<uint, TypedValue>`). Each row contains:
//! 1. `AttributeTypeInfo` (entry with key `0x6e616d65` / `"name"` containing the attribute name string)
//! 2. `AttributeValues` (entries `[0]` and `[1]` containing `NumericTypedValue` slots with the `1..20` integer value)
//!
//! Writing to these `BindingSlot`s updates the live UI display and is verified by read-back.
//! However, authoritative simulation attributes reside in `fm.exe`'s native database table,
//! and require reverse-engineering the native engine table layout to achieve persistent
//! background engine modifications.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::{CoreError, Result};
use fmlev_mem::LinuxMemoryDriver;

/// NumberKind enum values from FM26 `SI.Core.DynamicNumber.NumberKind`.
pub const NUMBER_KIND_SIGNED_WHOLE: i32 = 0;
pub const NUMBER_KIND_UNSIGNED_WHOLE: i32 = 1;
pub const NUMBER_KIND_FLOATING_POINT: i32 = 2;
pub const NUMBER_KIND_INVALID: i32 = 3;

/// Distinguishes the high-level kind of managed slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotKind {
    Numeric,
    NumericTypedValue,
    String,
    Boolean,
    Object,
    Other,
}

impl SlotKind {
    /// Returns true if this slot represents a numeric value that can be written with `write_numeric_slot`.
    pub fn is_numeric(&self) -> bool {
        matches!(self, SlotKind::Numeric | SlotKind::NumericTypedValue)
    }
}

/// Identifies a specific UI-bound value slot inside FM26's `BindingSubsystem`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingSlot {
    /// Remote address of the `TypedValue` object in target process memory.
    pub typed_value_addr: usize,
    /// High-level class kind of the slot.
    pub class_kind: SlotKind,
    /// `DynamicNumber.NumberKind` (0 = SignedWhole, 1 = UnsignedWhole, 2 = FloatingPoint).
    pub number_kind: i32,
    /// Remote address(es) that hold the `TypedValue*` pointer (e.g. entry + 0x10 in DynamicReference).
    #[serde(default)]
    pub ptr_holder_addrs: Vec<usize>,
    /// Associated `Bindings.Data` (`d_obj`) address(es) whose `m_value` and `m_version` are updated.
    #[serde(default)]
    pub data_obj_addrs: Vec<usize>,
}

impl BindingSlot {
    /// Creates a new `BindingSlot`.
    pub fn new(typed_value_addr: usize, class_kind: SlotKind, number_kind: i32) -> Self {
        Self {
            typed_value_addr,
            class_kind,
            number_kind,
            ptr_holder_addrs: Vec::new(),
            data_obj_addrs: Vec::new(),
        }
    }

    /// Returns true if this slot is a numeric slot.
    pub fn is_numeric(&self) -> bool {
        self.class_kind.is_numeric()
    }

    /// Basic pointer sanity check for 64-bit user-space address.
    pub fn is_valid(&self) -> bool {
        self.typed_value_addr > 0x10000 && self.typed_value_addr < 0x7fff_ffff_ffff
    }
}

/// Represents an attribute that may have both a primary and secondary NumericTypedValue slot
/// (such as entry [0] and entry [1] in FM26's AttributeValues DynamicReference).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeBinding {
    pub name: String,
    pub primary_slot: BindingSlot,
    pub secondary_slot: Option<BindingSlot>,
}

/// Encodes an integer `value` into the 8-byte union format expected by `DynamicNumber.Union`
/// at `TypedValue + 0x18`, according to its `NumberKind`.
pub fn encode_numeric_slot_value(number_kind: i32, value: i64) -> Result<[u8; 8]> {
    match number_kind {
        NUMBER_KIND_SIGNED_WHOLE => Ok(value.to_ne_bytes()),
        NUMBER_KIND_UNSIGNED_WHOLE => u64::try_from(value)
            .map(u64::to_ne_bytes)
            .map_err(|_| CoreError::InvalidValue("Negative value cannot be stored as an unsigned number".into())),
        NUMBER_KIND_FLOATING_POINT => {
            let f = value as f64;
            Ok(f.to_ne_bytes())
        }
        _ => Err(CoreError::InvalidValue(format!(
            "Unsupported or invalid NumberKind {number_kind} for numeric slot (expected 0=SignedWhole, 1=UnsignedWhole, 2=FloatingPoint)"
        ))),
    }
}

/// Decodes the 8-byte union representation at `TypedValue + 0x18` back to an integer `i64`
/// according to its `NumberKind`.
pub fn decode_numeric_slot_value(number_kind: i32, bytes: [u8; 8]) -> Result<i64> {
    match number_kind {
        NUMBER_KIND_SIGNED_WHOLE => Ok(i64::from_ne_bytes(bytes)),
        NUMBER_KIND_UNSIGNED_WHOLE => i64::try_from(u64::from_ne_bytes(bytes)).map_err(|_| {
            CoreError::InvalidValue("Unsigned number exceeds supported integer range".into())
        }),
        NUMBER_KIND_FLOATING_POINT => {
            let f = f64::from_ne_bytes(bytes);
            if !f.is_finite() || f < i64::MIN as f64 || f >= -(i64::MIN as f64) {
                return Err(CoreError::InvalidValue(
                    "Floating point number is not a finite supported integer".into(),
                ));
            }
            Ok(f.round() as i64)
        }
        _ => Err(CoreError::InvalidValue(format!(
            "Unsupported or invalid NumberKind {number_kind} for numeric slot"
        ))),
    }
}

/// Resolve the common integer pool from the slot's class ancestry, without assuming
/// a process-specific address. `TypedValue.s_commonInts` is a static field at +0x40.
fn common_ints_for_slot(driver: &LinuxMemoryDriver, slot: &BindingSlot) -> Option<usize> {
    let mut klass = driver.read_ptr(slot.typed_value_addr).ok()?;
    for _ in 0..8 {
        if klass <= 0x10000 {
            return None;
        }
        let class = fmlev_il2cpp::Il2CppClass::read(driver, klass).ok()?;
        if class.name == "TypedValue" && class.namespaze == "SI.Core" {
            let array = class.read_static_field_ptr(driver, 0x40).ok()?;
            return (driver.read::<u64>(array + 0x18).ok()? == 102).then_some(array);
        }
        klass = driver.read_ptr(klass + 0x58).ok()?;
    }
    None
}

/// Replaces binding references with a validated immutable common integer object.
/// Never mutates the numeric object itself: FM26 shares these objects across players.
/// This changes only UI caches, not authoritative simulation data.
pub fn write_numeric_slot(
    driver: &LinuxMemoryDriver,
    slot: &BindingSlot,
    value: i64,
) -> Result<()> {
    if !slot.is_valid() {
        return Err(CoreError::InvalidValue(format!(
            "Invalid typed_value_addr: 0x{:x}",
            slot.typed_value_addr
        )));
    }
    if !slot.is_numeric() {
        return Err(CoreError::InvalidValue(format!(
            "Cannot write numeric value to non-numeric slot kind: {:?}",
            slot.class_kind
        )));
    }
    encode_numeric_slot_value(slot.number_kind, value)?;
    if slot.ptr_holder_addrs.is_empty() && slot.data_obj_addrs.is_empty() {
        return Err(CoreError::InvalidValue("Cannot safely edit a potentially shared numeric object without its binding pointer holder".into()));
    }
    if !(-1..=100).contains(&value) {
        return Err(CoreError::InvalidValue(
            "UI binding replacement requires an available common integer (-1..=100)".into(),
        ));
    }
    let array = common_ints_for_slot(driver, slot).ok_or_else(|| {
        CoreError::InvalidValue(
            "Cannot resolve TypedValue.s_commonInts; refusing to mutate shared numeric data".into(),
        )
    })?;
    let replacement = driver.read_ptr(array + 0x20 + (value + 1) as usize * 8)?;
    let replacement_slot = checked_numeric_slot(driver, replacement)
        .ok_or_else(|| CoreError::InvalidValue("Common integer entry is not numeric".into()))?;
    let mut bytes = [0; 8];
    driver.read_bytes(replacement + 0x18, &mut bytes)?;
    if decode_numeric_slot_value(replacement_slot.number_kind, bytes)? != value {
        return Err(CoreError::InvalidValue(
            "Common integer pool value does not match requested value".into(),
        ));
    }
    let mut holders = slot.ptr_holder_addrs.clone();
    holders.extend(slot.data_obj_addrs.iter().map(|addr| addr + 0x30));
    holders.sort_unstable();
    holders.dedup();
    // Preflight every reference before the first mutation.
    for &holder in &holders {
        if driver.read_ptr(holder)? != slot.typed_value_addr {
            return Err(CoreError::InvalidValue(
                "Binding changed while editing; refresh and retry".into(),
            ));
        }
    }
    for holder in holders {
        driver.write(holder, &replacement)?;
        if driver.read_ptr(holder)? != replacement {
            return Err(CoreError::InvalidValue(
                "Binding pointer read-back verification failed".into(),
            ));
        }
    }
    for &data in &slot.data_obj_addrs {
        let version = driver.read::<u8>(data + 0x40)?;
        driver.write(data + 0x40, &version.wrapping_add(1))?;
    }
    Ok(())
}

fn checked_numeric_slot(driver: &LinuxMemoryDriver, tv: usize) -> Option<BindingSlot> {
    if tv <= 0x10000 {
        return None;
    }
    let klass = driver.read_ptr(tv).ok()?;
    let class = fmlev_il2cpp::Il2CppClass::read(driver, klass).ok()?;
    if class.name != "NumericTypedValue" || class.namespaze != "SI.Core" {
        return None;
    }
    let kind = driver.read::<i32>(tv + 0x20).ok()?;
    if !(0..=2).contains(&kind) {
        return None;
    }
    Some(BindingSlot::new(tv, SlotKind::NumericTypedValue, kind))
}

/// Normalizes attribute names to simplify matching across UI binding conventions.
/// E.g. "AttributeFinishing", "Finishing", and "finishing" all map to "finishing".
fn normalize_prop_name(name: &str) -> String {
    let lower = name.to_lowercase();
    let stripped = lower.strip_prefix("attribute").unwrap_or(&lower);
    stripped.replace('_', "").trim().to_string()
}

/// Extracts player attributes from FM26's `AttributesMental`, `AttributesPhysical`,
/// `AttributesSetPieces`, and `AttributesTechnical` binding lists on `PersonReference`.
pub fn extract_list_attribute_bindings(
    driver: &LinuxMemoryDriver,
    list_tv: usize,
) -> Result<Vec<AttributeBinding>> {
    extract_list_attribute_bindings_with_context(driver, list_tv, &HashMap::new())
}

fn extract_list_attribute_bindings_with_context(
    driver: &LinuxMemoryDriver,
    list_tv: usize,
    context: &HashMap<(usize, String), usize>,
) -> Result<Vec<AttributeBinding>> {
    if list_tv <= 0x10000 {
        return Ok(Vec::new());
    }

    let list_ptr = driver.read_ptr(list_tv + 0x18)?;
    if list_ptr <= 0x10000 {
        return Ok(Vec::new());
    }

    let items_ptr = driver.read_ptr(list_ptr + 0x10)?;
    let size = driver.read::<i32>(list_ptr + 0x18)?;
    if items_ptr <= 0x10000 || size <= 0 {
        return Ok(Vec::new());
    }

    let clamped_size = (size as usize).min(32);
    let mut results = Vec::new();

    for j in 0..clamped_size {
        let elem_tv = driver.read_ptr(items_ptr + 0x20 + j * 8)?;
        if elem_tv <= 0x10000 {
            continue;
        }

        let row_dyn_ref = driver.read_ptr(elem_tv + 0x18)?;
        if row_dyn_ref <= 0x10000 {
            continue;
        }

        let row_entries = driver.read_ptr(row_dyn_ref + 0x18).unwrap_or(0);
        let row_count = if row_entries > 0x10000 {
            driver
                .read::<i32>(row_dyn_ref + 0x20)
                .unwrap_or(0)
                .clamp(0, 8) as usize
        } else {
            0
        };

        let mut attr_name = String::new();
        let mut primary_slot: Option<BindingSlot> = None;
        let mut secondary_slot: Option<BindingSlot> = None;

        let mut children: Vec<usize> = (0..row_count)
            .map(|r| {
                driver
                    .read_ptr(row_entries + 0x20 + r * 24 + 0x10)
                    .unwrap_or(0)
            })
            .collect();
        for property in ["AttributeTypeInfo", "AttributeValues"] {
            if let Some(&tv) = context.get(&(row_dyn_ref, property.into())) {
                children.push(tv);
            }
        }
        for child_tv in children {
            if child_tv <= 0x10000 {
                continue;
            }
            let child_dyn = driver.read_ptr(child_tv + 0x18).unwrap_or(0);
            if child_dyn <= 0x10000 {
                continue;
            }
            let c_entries = driver.read_ptr(child_dyn + 0x18).unwrap_or(0);
            let c_count = driver
                .read::<i32>(child_dyn + 0x20)
                .unwrap_or(4)
                .clamp(1, 16) as usize;
            if c_entries <= 0x10000 {
                continue;
            }

            for k in 0..c_count {
                let entry_addr = c_entries + 0x20 + k * 24;
                let key = driver.read::<u32>(entry_addr + 0x08).unwrap_or(0);
                if key == 0x6e616d65 {
                    // "name" (AttributeTypeInfo)
                    let name_tv = driver.read_ptr(entry_addr + 0x10).unwrap_or(0);
                    if name_tv > 0x10000 {
                        let str_ptr = driver.read_ptr(name_tv + 0x18).unwrap_or(0);
                        if str_ptr > 0x10000 {
                            if let Ok(s) = fmlev_il2cpp::Il2CppString::read(driver, str_ptr) {
                                attr_name = s.trim().to_string();
                            }
                        }
                    }
                } else if key == 0x7074766c {
                    // "ptvl" (PropertyValue primary slot)
                    let num_tv = driver.read_ptr(entry_addr + 0x10).unwrap_or(0);
                    if num_tv > 0x10000 {
                        let Some(mut slot) = checked_numeric_slot(driver, num_tv) else {
                            continue;
                        };
                        slot.ptr_holder_addrs.push(entry_addr + 0x10);
                        primary_slot = Some(slot);
                    }
                } else if key == 0x44667a45 {
                    // "DfzE" (AttributeMinValue secondary slot)
                    let num_tv = driver.read_ptr(entry_addr + 0x10).unwrap_or(0);
                    if num_tv > 0x10000 {
                        let Some(mut slot) = checked_numeric_slot(driver, num_tv) else {
                            continue;
                        };
                        slot.ptr_holder_addrs.push(entry_addr + 0x10);
                        secondary_slot = Some(slot);
                    }
                }
            }
        }

        if !attr_name.is_empty() {
            if let Some(prim) = primary_slot {
                results.push(AttributeBinding {
                    name: attr_name,
                    primary_slot: prim,
                    secondary_slot,
                });
            }
        }
    }

    Ok(results)
}

fn dynamic_reference_context(
    driver: &LinuxMemoryDriver,
    b_sub: usize,
) -> Result<HashMap<(usize, String), usize>> {
    let data = driver.read_ptr(b_sub + 0x78)?;
    let ctx = driver.read_ptr(b_sub + 0x88)?;
    let items = driver.read_ptr(data + 0x10)?;
    let size = driver.read::<i32>(data + 0x18)?.max(0) as usize;
    let entries = driver.read_ptr(ctx + 0x18)?;
    let count = driver.read::<i32>(ctx + 0x20)?.clamp(0, 50_000) as usize;
    let mut result = HashMap::new();
    for i in 0..count {
        let entry = entries + 0x20 + i * 56;
        let source = driver.read_ptr(entry + 8).unwrap_or(0);
        let reference = driver.read_ptr(source + 0x18).unwrap_or(0);
        let property = driver.read_ptr(entry + 16).unwrap_or(0);
        let string = driver.read_ptr(property + 0x10).unwrap_or(0);
        let name = fmlev_il2cpp::Il2CppString::read(driver, string)
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| fmlev_il2cpp::Il2CppString::read(driver, property).ok())
            .unwrap_or_default();
        if !matches!(name.as_str(), "AttributeTypeInfo" | "AttributeValues") {
            continue;
        }
        let key = driver.read::<u32>(entry + 40).unwrap_or(u32::MAX) as usize;
        if key >= size {
            continue;
        }
        let data_obj = driver.read_ptr(items + 0x20 + key * 8).unwrap_or(0);
        let value = driver.read_ptr(data_obj + 0x30).unwrap_or(0);
        if reference > 0x10000 && value > 0x10000 {
            result.insert((reference, name), value);
        }
    }
    Ok(result)
}

/// Scans FM26's live `BindingSubsystem` (`b_sub`) for all binding slots associated with
/// the specified person index (`m_index`), returning a map of property name -> `BindingSlot`.
///
/// Supports both:
/// 1. Row `DynamicReference` attribute lists (`AttributesMental`, `AttributesPhysical`,
///    `AttributesSetPieces`, `AttributesTechnical`) where individual attributes like
///    `"Vision"`, `"Passing"`, `"Finishing"` are bound.
/// 2. Direct flat property bindings (e.g. `"Age"`, `"Condition"`, `"UniqueId"`, `"CurrentAbility"`).
pub fn find_person_binding_slots(
    driver: &LinuxMemoryDriver,
    b_sub: usize,
    target_person_index: i32,
) -> Result<HashMap<String, BindingSlot>> {
    if b_sub <= 0x10000 {
        return Err(CoreError::InvalidValue(format!(
            "Invalid BindingSubsystem base 0x{b_sub:x}"
        )));
    }

    let m_data = driver.read_ptr(b_sub + 0x78)?;
    let m_ctx = driver.read_ptr(b_sub + 0x88)?;
    if m_data <= 0x10000 || m_ctx <= 0x10000 {
        return Ok(HashMap::new());
    }

    let data_items = driver.read_ptr(m_data + 0x10)?;
    let data_size = driver.read::<i32>(m_data + 0x18)?;
    let entries = driver.read_ptr(m_ctx + 0x18)?;
    let count = driver.read::<i32>(m_ctx + 0x20)?;

    if data_items <= 0x10000 || entries <= 0x10000 || count <= 0 || data_size <= 0 {
        return Ok(HashMap::new());
    }

    let max_entries = (count as usize).min(50_000);
    let mut raw_entries = vec![0u8; max_entries * 56];
    driver.read_bytes(entries + 0x20, &mut raw_entries)?;
    let dynamic_context = dynamic_reference_context(driver, b_sub)?;

    let mut slots = HashMap::new();

    for i in 0..max_entries {
        let base_off = i * 56;
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
        // DatabaseTableType::Person == 1
        if t_typ != 1 || m_idx != target_person_index {
            continue;
        }

        let p_str_ptr = driver.read_ptr(prop_ptr + 0x10).unwrap_or(0);
        let prop_name = fmlev_il2cpp::Il2CppString::read(driver, p_str_ptr)
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| fmlev_il2cpp::Il2CppString::read(driver, prop_ptr).ok())
            .unwrap_or_default();

        if prop_name.is_empty() {
            continue;
        }

        let d_obj = driver
            .read_ptr(data_items + 0x20 + data_key * 8)
            .unwrap_or(0);
        let val_tv = if d_obj > 0x10000 {
            driver.read_ptr(d_obj + 0x30).unwrap_or(0)
        } else {
            0
        };
        if val_tv <= 0x10000 {
            continue;
        }

        // Check if this property is one of FM26's attribute lists:
        if matches!(
            prop_name.as_str(),
            "AttributesMental"
                | "AttributesPhysical"
                | "AttributesSetPieces"
                | "AttributesTechnical"
        ) {
            if let Ok(attr_bindings) =
                extract_list_attribute_bindings_with_context(driver, val_tv, &dynamic_context)
            {
                for binding in attr_bindings {
                    slots.insert(binding.name, binding.primary_slot);
                }
            }
            continue;
        }

        if let Some(mut slot) = checked_numeric_slot(driver, val_tv) {
            slot.ptr_holder_addrs.push(d_obj + 0x30);
            slot.data_obj_addrs.push(d_obj);
            slots.insert(prop_name, slot);
        }
    }

    Ok(slots)
}

/// Scans FM26's live `BindingSubsystem` for all comprehensive `AttributeBinding`s (including secondary slots).
pub fn find_person_attribute_bindings(
    driver: &LinuxMemoryDriver,
    b_sub: usize,
    target_person_index: i32,
) -> Result<Vec<AttributeBinding>> {
    if b_sub <= 0x10000 {
        return Err(CoreError::InvalidValue(format!(
            "Invalid BindingSubsystem base 0x{b_sub:x}"
        )));
    }

    let m_data = driver.read_ptr(b_sub + 0x78)?;
    let m_ctx = driver.read_ptr(b_sub + 0x88)?;
    if m_data <= 0x10000 || m_ctx <= 0x10000 {
        return Ok(Vec::new());
    }

    let data_items = driver.read_ptr(m_data + 0x10)?;
    let data_size = driver.read::<i32>(m_data + 0x18)?;
    let entries = driver.read_ptr(m_ctx + 0x18)?;
    let count = driver.read::<i32>(m_ctx + 0x20)?;

    if data_items <= 0x10000 || entries <= 0x10000 || count <= 0 || data_size <= 0 {
        return Ok(Vec::new());
    }

    let max_entries = (count as usize).min(50_000);
    let mut raw_entries = vec![0u8; max_entries * 56];
    driver.read_bytes(entries + 0x20, &mut raw_entries)?;
    let dynamic_context = dynamic_reference_context(driver, b_sub)?;

    let mut results = Vec::new();

    for i in 0..max_entries {
        let base_off = i * 56;
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
        if t_typ != 1 || m_idx != target_person_index {
            continue;
        }

        let p_str_ptr = driver.read_ptr(prop_ptr + 0x10).unwrap_or(0);
        let prop_name = fmlev_il2cpp::Il2CppString::read(driver, p_str_ptr)
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| fmlev_il2cpp::Il2CppString::read(driver, prop_ptr).ok())
            .unwrap_or_default();

        if !matches!(
            prop_name.as_str(),
            "AttributesMental"
                | "AttributesPhysical"
                | "AttributesSetPieces"
                | "AttributesTechnical"
        ) {
            continue;
        }

        let d_obj = driver
            .read_ptr(data_items + 0x20 + data_key * 8)
            .unwrap_or(0);
        let val_tv = if d_obj > 0x10000 {
            driver.read_ptr(d_obj + 0x30).unwrap_or(0)
        } else {
            0
        };
        if val_tv <= 0x10000 {
            continue;
        }

        if let Ok(mut bindings) =
            extract_list_attribute_bindings_with_context(driver, val_tv, &dynamic_context)
        {
            results.append(&mut bindings);
        }
    }

    Ok(results)
}

/// Finds a specific attribute slot for a person in `BindingSubsystem`.
pub fn find_person_attribute_slot(
    driver: &LinuxMemoryDriver,
    b_sub: usize,
    person_index: i32,
    attr: &str,
) -> Result<BindingSlot> {
    let slots = find_person_binding_slots(driver, b_sub, person_index)?;
    let target_norm = normalize_prop_name(attr);

    for (name, slot) in slots {
        if normalize_prop_name(&name) == target_norm {
            return Ok(slot);
        }
    }

    Err(CoreError::InvalidValue(format!(
        "Binding slot for attribute '{attr}' not found for person index {person_index}"
    )))
}

/// Writes a numeric attribute to a player's UI-side binding slot in `BindingSubsystem`,
/// if found, and verifies the write by read-back.
/// If secondary slots (entry [1] in AttributeValues) are present, writes to both.
pub fn write_person_binding_slot(
    driver: &LinuxMemoryDriver,
    b_sub: usize,
    person_index: i32,
    attr: &str,
    value: i64,
) -> Result<()> {
    let target_norm = normalize_prop_name(attr);

    // 1. Try finding in comprehensive list bindings first (has secondary slot sync)
    if let Ok(bindings) = find_person_attribute_bindings(driver, b_sub, person_index) {
        for binding in bindings {
            if normalize_prop_name(&binding.name) == target_norm {
                write_numeric_slot(driver, &binding.primary_slot, value)?;
                if let Some(sec) = &binding.secondary_slot {
                    write_numeric_slot(driver, sec, value)?;
                }
                return Ok(());
            }
        }
    }

    // 2. Fall back to direct/flat slot
    let slot = find_person_attribute_slot(driver, b_sub, person_index, attr)?;
    write_numeric_slot(driver, &slot, value)
}

/// Ordered native aptitude bytes, including the obsolete sweeper slot at mask 2.
pub const NATIVE_POSITION_MASKS: [u64; 15] = [
    1, 2, 8, 16, 4, 128, 512, 1024, 256, 4096, 8192, 2048, 16384, 64, 32,
];

/// Complete, observed UI binding data used to validate an independently found native record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativePlayerFingerprint {
    pub unique_id: u32,
    pub aptitudes: [u8; 15],
    pub left_foot: u8,
    pub right_foot: u8,
}

/// Reads numeric bindings, recovering shared integer identity even if an older writer
/// has accidentally changed a common integer object's payload. No game memory is changed.
pub fn read_numeric_binding_integer(driver: &LinuxMemoryDriver, tv: usize) -> Result<i64> {
    let slot = checked_numeric_slot(driver, tv)
        .ok_or_else(|| CoreError::InvalidValue("Expected numeric player binding".into()))?;
    if let Some(array) = common_ints_for_slot(driver, &slot) {
        let mut pointers = [0u8; 102 * 8];
        driver.read_bytes(array + 0x20, &mut pointers)?;
        for (index, bytes) in pointers.chunks_exact(8).enumerate() {
            if usize::from_ne_bytes(bytes.try_into().unwrap()) == tv {
                return Ok(index as i64 - 1);
            }
        }
    }
    let mut bytes = [0; 8];
    driver.read_bytes(tv + 0x18, &mut bytes)?;
    decode_numeric_slot_value(slot.number_kind, bytes)
}

fn required_rating(driver: &LinuxMemoryDriver, tv: usize) -> Result<u8> {
    let value = read_numeric_binding_integer(driver, tv)?;
    if !(1..=20).contains(&value) {
        return Err(CoreError::InvalidValue(
            "Incomplete or out-of-range player rating binding".into(),
        ));
    }
    Ok(value as u8)
}

/// Reads all fifteen aptitude rows, both foot ratings and the unique database ID.
/// Missing bindings fail rather than supplying inferred/default ratings.
pub fn read_person_native_fingerprint(
    driver: &LinuxMemoryDriver,
    b_sub: usize,
    target_person_index: i32,
) -> Result<NativePlayerFingerprint> {
    if b_sub <= 0x10000 {
        return Err(CoreError::InvalidValue("Invalid BindingSubsystem".into()));
    }
    let data = driver.read_ptr(b_sub + 0x78)?;
    let ctx = driver.read_ptr(b_sub + 0x88)?;
    let items = driver.read_ptr(data + 0x10)?;
    let size = driver.read::<i32>(data + 0x18)?.max(0) as usize;
    let entries = driver.read_ptr(ctx + 0x18)?;
    let count = driver.read::<i32>(ctx + 0x20)?.clamp(0, 50_000) as usize;
    let mut properties = HashMap::new();
    for i in 0..count {
        let entry = entries + 0x20 + i * 56;
        let source = driver.read_ptr(entry + 8).unwrap_or(0);
        let reference = driver.read_ptr(source + 0x18).unwrap_or(0);
        if driver.read::<i32>(reference + 0x20).unwrap_or(-1) != target_person_index
            || driver.read::<i32>(reference + 0x24).unwrap_or(-1) != 1
        {
            continue;
        }
        let property = driver.read_ptr(entry + 16).unwrap_or(0);
        let string = driver.read_ptr(property + 0x10).unwrap_or(0);
        let name = fmlev_il2cpp::Il2CppString::read(driver, string)
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| fmlev_il2cpp::Il2CppString::read(driver, property).ok())
            .unwrap_or_default();
        if !matches!(
            name.as_str(),
            "UniqueId" | "Footedness" | "PositionAptitudeList"
        ) {
            continue;
        }
        let key = driver.read::<u32>(entry + 40).unwrap_or(u32::MAX) as usize;
        if key >= size {
            continue;
        }
        let data_obj = driver.read_ptr(items + 0x20 + key * 8).unwrap_or(0);
        let value = driver.read_ptr(data_obj + 0x30).unwrap_or(0);
        if value > 0x10000 {
            properties.insert(name, value);
        }
    }
    let required = |name: &str| {
        properties.get(name).copied().ok_or_else(|| {
            CoreError::InvalidValue(format!(
                "Open the player's profile: required {name} binding unavailable"
            ))
        })
    };
    let unique = read_numeric_binding_integer(driver, required("UniqueId")?)?;
    let unique_id = u32::try_from(unique)
        .ok()
        .filter(|&id| id > 0)
        .ok_or_else(|| CoreError::InvalidValue("Invalid unique player ID binding".into()))?;
    let foot = driver.read_ptr(required("Footedness")? + 0x18)?;
    let foot_entries = driver.read_ptr(foot + 0x18)?;
    let foot_count = driver.read::<i32>(foot + 0x20)?.clamp(0, 16) as usize;
    let mut left = None;
    let mut right = None;
    for i in 0..foot_count {
        let entry = foot_entries + 0x20 + i * 24;
        let key = driver.read::<u32>(entry + 8)?;
        if key == 0x32342020 {
            left = Some(required_rating(driver, driver.read_ptr(entry + 16)?)?);
        }
        if key == 0x32332020 {
            right = Some(required_rating(driver, driver.read_ptr(entry + 16)?)?);
        }
    }
    let list = driver.read_ptr(required("PositionAptitudeList")? + 0x18)?;
    let list_items = driver.read_ptr(list + 0x10)?;
    let list_size = driver.read::<i32>(list + 0x18)?.clamp(0, 32) as usize;
    let mut ratings = [None; 15];
    for i in 0..list_size {
        let row = driver.read_ptr(driver.read_ptr(list_items + 0x20 + i * 8)? + 0x18)?;
        let row_entries = driver.read_ptr(row + 0x18)?;
        let row_count = driver.read::<i32>(row + 0x20)?.clamp(0, 8) as usize;
        let mut mask = None;
        let mut rating = None;
        for j in 0..row_count {
            let entry = row_entries + 0x20 + j * 24;
            let key = driver.read::<u32>(entry + 8)?;
            if key == 0x506f7369 {
                mask = Some(
                    read_numeric_binding_integer(driver, driver.read_ptr(entry + 16)?)? as u64,
                );
            }
            if key == 0x7074766c {
                rating = Some(required_rating(driver, driver.read_ptr(entry + 16)?)?);
            }
        }
        if let (Some(mask), Some(value)) = (mask, rating) {
            if let Some(index) = NATIVE_POSITION_MASKS.iter().position(|&m| m == mask) {
                if ratings[index].is_some_and(|previous| previous != value) {
                    return Err(CoreError::InvalidValue(
                        "Conflicting position aptitude bindings".into(),
                    ));
                }
                ratings[index] = Some(value);
            }
        }
    }
    let mut aptitudes = [0; 15];
    for (index, rating) in ratings.into_iter().enumerate() {
        aptitudes[index] = rating.ok_or_else(|| {
            CoreError::InvalidValue(format!(
                "Missing position aptitude mask {}",
                NATIVE_POSITION_MASKS[index]
            ))
        })?;
    }
    Ok(NativePlayerFingerprint {
        unique_id,
        aptitudes,
        left_foot: left
            .ok_or_else(|| CoreError::InvalidValue("Missing left foot binding".into()))?,
        right_foot: right
            .ok_or_else(|| CoreError::InvalidValue("Missing right foot binding".into()))?,
    })
}

/// Refuses to present UI-cache changes as authoritative position/footedness edits.
pub fn write_person_positions_and_feet(
    _driver: &LinuxMemoryDriver,
    _b_sub: usize,
    target_person_index: i32,
    _positions: &crate::models::PlayerPositions,
) -> Result<()> {
    Err(CoreError::InvalidValue(format!(
        "Authoritative positions and footedness writing is unavailable for person index {target_person_index}: Footedness and PositionAptitudeList are UI caches; the native engine record layout has not been verified"
    )))
}

/// Authoritative person attribute write stub.
///
/// # Architectural Reality in FM26
/// In Football Manager 2026, person attributes do **not** live in managed IL2CPP memory.
/// The Unity/IL2CPP layer (`GameAssembly.dll`) contains only UI presentation code and
/// binding caches (`SI.Bindable.BindingSubsystem`).
///
/// The authoritative game data lives entirely in Sports Interactive's native C++ simulation
/// engine (`fm.exe`). Managed entity handles are thin `PersonReference : DatabaseRecordReference`
/// structs holding only `m_index` @ 0x20 and `Type` (`DatabaseTableType::Person = 1`) @ 0x24.
///
/// Writing via [`write_numeric_slot`] or [`write_person_binding_slot`] updates the UI cache
/// entry (`NumericTypedValue` @ +0x18), changing the visible value on screen. However,
/// because authoritative data is stored in native C++ memory, UI updates will eventually be
/// overwritten when the game engine pushes fresh responses across the `GameInteropSubsystem` channel.
///
/// Direct, authoritative editing of simulation attributes requires reverse-engineering the native
/// `fm.exe` database table layout for table type `Person (1)`.
pub fn write_person_attribute(
    _driver: &LinuxMemoryDriver,
    person_index: i32,
    attr: &str,
    _value: u8,
) -> Result<()> {
    Err(CoreError::InvalidValue(format!(
        "Authoritative person attribute writing is unavailable for attribute '{attr}': player attributes in FM26 \
         reside in the native C++ engine ('fm.exe'), not in managed IL2CPP memory. \
         Managed memory holds only lightweight PersonReference handles (m_index={person_index}) \
         and UI cache slots in BindingSubsystem. To update the live UI display, locate the \
         attribute's BindingSlot and call `write_numeric_slot` (or `write_person_binding_slot`). \
         To modify simulation attributes authoritatively, the native engine's Person database \
         table (DatabaseTableType 1) in fm.exe must be reverse-engineered."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Arena(Vec<Box<[u8]>>);
    impl Arena {
        fn alloc(&mut self, size: usize) -> usize {
            let mut bytes = vec![0u8; size].into_boxed_slice();
            let address = bytes.as_mut_ptr() as usize;
            self.0.push(bytes);
            address
        }
        fn put<T: Copy>(&self, address: usize, value: T) {
            // All addresses in these fixtures refer to live, owned arena allocations.
            unsafe {
                std::ptr::write_unaligned(address as *mut T, value);
            }
        }
        fn cstring(&mut self, text: &str) -> usize {
            let address = self.alloc(256);
            for (i, byte) in text.bytes().enumerate() {
                self.put(address + i, byte);
            }
            address
        }
        fn string(&mut self, text: &str) -> usize {
            let address = self.alloc(0x14 + text.len() * 2);
            self.put(address + 0x10, text.len() as i32);
            for (i, unit) in text.encode_utf16().enumerate() {
                self.put(address + 0x14 + i * 2, unit);
            }
            address
        }
        fn class(&mut self, name: &str) -> usize {
            let address = self.alloc(0xc0);
            let name = self.cstring(name);
            let namespace = self.cstring("SI.Core");
            self.put(address + 0x10, name);
            self.put(address + 0x18, namespace);
            address
        }
        fn tv(&mut self, value: usize) -> usize {
            let address = self.alloc(0x28);
            self.put(address + 0x18, value);
            address
        }
        fn dictionary(&mut self, key: u32, value: usize) -> usize {
            let dictionary = self.alloc(0x28);
            let entries = self.alloc(0x38);
            self.put(dictionary + 0x18, entries);
            self.put(dictionary + 0x20, 1i32);
            self.put(entries + 0x28, key);
            self.put(entries + 0x30, value);
            dictionary
        }
    }

    #[test]
    fn replaces_singleton_references_without_mutating_shared_values() {
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let mut arena = Arena::default();
        let class = arena.class("NumericTypedValue");
        let base = arena.class("TypedValue");
        let statics = arena.alloc(0x48);
        let pool = arena.alloc(0x20 + 102 * 8);
        arena.put(class + 0x58, base);
        arena.put(base + 0xb8, statics);
        arena.put(statics + 0x40, pool);
        arena.put(pool + 0x18, 102u64);
        let original = arena.tv(15);
        let replacement = arena.tv(19);
        arena.put(original, class);
        arena.put(replacement, class);
        arena.put(pool + 0x20 + 20 * 8, replacement);
        let holder = arena.alloc(8);
        let data = arena.alloc(0x48);
        arena.put(holder, original);
        arena.put(data + 0x30, original);
        arena.put(data + 0x40, 255u8);
        let mut slot = BindingSlot::new(original, SlotKind::Numeric, 0);
        slot.ptr_holder_addrs.push(holder);
        slot.data_obj_addrs.push(data);
        write_numeric_slot(&driver, &slot, 19).unwrap();
        assert_eq!(driver.read_ptr(holder).unwrap(), replacement);
        assert_eq!(driver.read_ptr(data + 0x30).unwrap(), replacement);
        assert_eq!(driver.read::<i64>(original + 0x18).unwrap(), 15);
        arena.put(pool + 0x20 + 16 * 8, original);
        arena.put(original + 0x18, 11i64);
        assert_eq!(read_numeric_binding_integer(&driver, original).unwrap(), 15);
        assert_eq!(
            driver.read::<i64>(original + 0x18).unwrap(),
            11,
            "read-only recovery must not mutate game memory"
        );
        assert_eq!(driver.read::<u8>(data + 0x40).unwrap(), 0);
        assert!(
            write_numeric_slot(&driver, &slot, 19).is_err(),
            "stale references must be rejected"
        );
    }

    #[test]
    fn attribute_list_resolves_context_when_row_dictionary_is_empty() {
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let mut arena = Arena::default();
        let numeric_class = arena.class("NumericTypedValue");
        let number = arena.tv(18);
        arena.put(number, numeric_class);
        let name = arena.string("Crossing");
        let name_tv = arena.tv(name);
        let info = arena.dictionary(0x6e616d65, name_tv);
        let values = arena.dictionary(0x7074766c, number);
        let info_tv = arena.tv(info);
        let values_tv = arena.tv(values);
        let row = arena.alloc(0x28); // no direct children: only m_ctx has them
        let row_tv = arena.tv(row);
        let items = arena.alloc(0x28);
        arena.put(items + 0x20, row_tv);
        let list = arena.alloc(0x20);
        arena.put(list + 0x10, items);
        arena.put(list + 0x18, 1i32);
        let list_tv = arena.tv(list);
        let context = HashMap::from([
            ((row, "AttributeTypeInfo".into()), info_tv),
            ((row, "AttributeValues".into()), values_tv),
        ]);
        assert!(extract_list_attribute_bindings(&driver, list_tv)
            .unwrap()
            .is_empty());
        let bindings =
            extract_list_attribute_bindings_with_context(&driver, list_tv, &context).unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].name, "Crossing");
        assert_eq!(bindings[0].primary_slot.typed_value_addr, number);
        assert!(!bindings[0].primary_slot.ptr_holder_addrs.is_empty());
    }

    #[test]
    fn positions_never_report_success_without_authoritative_storage() {
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        assert!(write_person_positions_and_feet(&driver, 0, 65304, &Default::default()).is_err());
    }

    #[test]
    fn test_encode_decode_signed_whole() {
        let val: i64 = -42;
        let encoded = encode_numeric_slot_value(NUMBER_KIND_SIGNED_WHOLE, val).unwrap();
        assert_eq!(encoded, (-42i64).to_ne_bytes());
        let decoded = decode_numeric_slot_value(NUMBER_KIND_SIGNED_WHOLE, encoded).unwrap();
        assert_eq!(decoded, -42);

        let val_pos: i64 = 18;
        let encoded_pos = encode_numeric_slot_value(NUMBER_KIND_SIGNED_WHOLE, val_pos).unwrap();
        assert_eq!(encoded_pos, 18i64.to_ne_bytes());
        let decoded_pos = decode_numeric_slot_value(NUMBER_KIND_SIGNED_WHOLE, encoded_pos).unwrap();
        assert_eq!(decoded_pos, 18);
    }

    #[test]
    fn test_encode_decode_unsigned_whole() {
        let val: i64 = 999;
        let encoded = encode_numeric_slot_value(NUMBER_KIND_UNSIGNED_WHOLE, val).unwrap();
        assert_eq!(encoded, 999u64.to_ne_bytes());
        let decoded = decode_numeric_slot_value(NUMBER_KIND_UNSIGNED_WHOLE, encoded).unwrap();
        assert_eq!(decoded, 999);
    }

    #[test]
    fn test_encode_decode_floating_point() {
        let val: i64 = 17;
        let encoded = encode_numeric_slot_value(NUMBER_KIND_FLOATING_POINT, val).unwrap();
        let expected_f64 = 17.0_f64;
        assert_eq!(encoded, expected_f64.to_ne_bytes());

        let decoded = decode_numeric_slot_value(NUMBER_KIND_FLOATING_POINT, encoded).unwrap();
        assert_eq!(decoded, 17);
    }

    #[test]
    fn native_fingerprint_requires_complete_observed_bindings() {
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let mut arena = Arena::default();
        let numeric_class = arena.class("NumericTypedValue");
        fn number(arena: &mut Arena, class: usize, n: usize) -> usize {
            let tv = arena.tv(n);
            arena.put(tv, class);
            tv
        }
        fn record(arena: &mut Arena, pairs: &[(u32, usize)]) -> usize {
            let obj = arena.alloc(0x28);
            let entries = arena.alloc(0x20 + pairs.len() * 24);
            arena.put(obj + 0x18, entries);
            arena.put(obj + 0x20, pairs.len() as i32);
            for (index, &(key, value)) in pairs.iter().enumerate() {
                arena.put(entries + 0x20 + index * 24 + 8, key);
                arena.put(entries + 0x20 + index * 24 + 16, value);
            }
            arena.tv(obj)
        }
        let unique = number(&mut arena, numeric_class, 2000448177);
        let left = number(&mut arena, numeric_class, 8);
        let right = number(&mut arena, numeric_class, 20);
        let foot = record(&mut arena, &[(0x32342020, left), (0x32332020, right)]);
        let list_items = arena.alloc(0x20 + 15 * 8);
        for (index, &mask) in NATIVE_POSITION_MASKS.iter().enumerate() {
            let mask_tv = number(&mut arena, numeric_class, mask as usize);
            let rating = number(&mut arena, numeric_class, index + 1);
            let row = record(&mut arena, &[(0x506f7369, mask_tv), (0x7074766c, rating)]);
            arena.put(list_items + 0x20 + index * 8, row);
        }
        let list = arena.alloc(0x20);
        arena.put(list + 0x10, list_items);
        arena.put(list + 0x18, 15i32);
        let list_tv = arena.tv(list);
        let reference = arena.alloc(0x28);
        arena.put(reference + 0x20, 65304i32);
        arena.put(reference + 0x24, 1i32);
        let source = arena.tv(reference);
        let data = arena.alloc(0x20);
        let data_items = arena.alloc(0x20 + 3 * 8);
        arena.put(data + 0x10, data_items);
        arena.put(data + 0x18, 3i32);
        let context = arena.alloc(0x28);
        let context_entries = arena.alloc(0x20 + 3 * 56);
        arena.put(context + 0x18, context_entries);
        arena.put(context + 0x20, 3i32);
        for (index, (name, value)) in [
            ("UniqueId", unique),
            ("Footedness", foot),
            ("PositionAptitudeList", list_tv),
        ]
        .into_iter()
        .enumerate()
        {
            let property = arena.string(name);
            let data_object = arena.alloc(0x38);
            arena.put(data_object + 0x30, value);
            arena.put(data_items + 0x20 + index * 8, data_object);
            let entry = context_entries + 0x20 + index * 56;
            arena.put(entry + 8, source);
            arena.put(entry + 16, property);
            arena.put(entry + 40, index as u32);
        }
        let subsystem = arena.alloc(0x90);
        arena.put(subsystem + 0x78, data);
        arena.put(subsystem + 0x88, context);
        let fingerprint = read_person_native_fingerprint(&driver, subsystem, 65304).unwrap();
        assert_eq!(fingerprint.unique_id, 2000448177);
        assert_eq!(
            fingerprint.aptitudes,
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
        );
        assert_eq!((fingerprint.left_foot, fingerprint.right_foot), (8, 20));
        arena.put(list + 0x18, 14i32);
        assert!(read_person_native_fingerprint(&driver, subsystem, 65304)
            .unwrap_err()
            .to_string()
            .contains("Missing position aptitude"));
        assert!(read_person_native_fingerprint(&driver, subsystem, 38141).is_err());
    }

    #[test]
    fn numeric_codec_rejects_lossy_or_invalid_values() {
        assert!(encode_numeric_slot_value(NUMBER_KIND_UNSIGNED_WHOLE, -1).is_err());
        assert!(
            decode_numeric_slot_value(NUMBER_KIND_UNSIGNED_WHOLE, u64::MAX.to_ne_bytes()).is_err()
        );
        assert!(
            decode_numeric_slot_value(NUMBER_KIND_FLOATING_POINT, f64::NAN.to_ne_bytes()).is_err()
        );
        assert!(
            decode_numeric_slot_value(NUMBER_KIND_FLOATING_POINT, f64::INFINITY.to_ne_bytes())
                .is_err()
        );
    }

    #[test]
    fn test_encode_invalid_kind() {
        assert!(encode_numeric_slot_value(NUMBER_KIND_INVALID, 10).is_err());
        assert!(encode_numeric_slot_value(99, 10).is_err());
        assert!(decode_numeric_slot_value(99, [0u8; 8]).is_err());
    }

    #[test]
    fn test_slot_kind_predicates() {
        assert!(SlotKind::Numeric.is_numeric());
        assert!(SlotKind::NumericTypedValue.is_numeric());
        assert!(!SlotKind::String.is_numeric());
        assert!(!SlotKind::Boolean.is_numeric());
        assert!(!SlotKind::Object.is_numeric());
        assert!(!SlotKind::Other.is_numeric());
    }

    #[test]
    fn test_binding_slot_construction() {
        let slot = BindingSlot::new(0x1234_5678, SlotKind::Numeric, NUMBER_KIND_SIGNED_WHOLE);
        assert_eq!(slot.typed_value_addr, 0x1234_5678);
        assert_eq!(slot.class_kind, SlotKind::Numeric);
        assert_eq!(slot.number_kind, NUMBER_KIND_SIGNED_WHOLE);
        assert!(slot.is_numeric());
        assert!(slot.is_valid());

        let invalid_slot = BindingSlot::new(0x50, SlotKind::Numeric, NUMBER_KIND_SIGNED_WHOLE);
        assert!(!invalid_slot.is_valid());
    }

    #[test]
    fn test_write_numeric_slot_refuses_to_mutate_shared_object() {
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let mut object = [0u8; 40];
        object[0x18..0x20].copy_from_slice(&15i64.to_ne_bytes());
        let slot = BindingSlot::new(object.as_mut_ptr() as usize, SlotKind::Numeric, 0);
        assert!(write_numeric_slot(&driver, &slot, 19).is_err());
        assert_eq!(
            i64::from_ne_bytes(object[0x18..0x20].try_into().unwrap()),
            15
        );
    }

    #[test]
    fn test_write_numeric_slot_rejects_non_numeric() {
        let my_pid = std::process::id() as i32;
        let driver = LinuxMemoryDriver::new(my_pid);
        let slot = BindingSlot::new(0x20000, SlotKind::String, NUMBER_KIND_SIGNED_WHOLE);
        let res = write_numeric_slot(&driver, &slot, 10);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("non-numeric"));
    }

    #[test]
    fn test_write_numeric_slot_rejects_null_addr() {
        let my_pid = std::process::id() as i32;
        let driver = LinuxMemoryDriver::new(my_pid);
        let slot = BindingSlot::new(0x0, SlotKind::Numeric, NUMBER_KIND_SIGNED_WHOLE);
        let res = write_numeric_slot(&driver, &slot, 10);
        assert!(res.is_err());
    }

    #[test]
    fn test_write_person_attribute_stub_returns_informative_error() {
        let my_pid = std::process::id() as i32;
        let driver = LinuxMemoryDriver::new(my_pid);
        let err = write_person_attribute(&driver, 12345, "Finishing", 18).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("native C++ engine"));
        assert!(msg.contains("fm.exe"));
        assert!(msg.contains("12345"));
        assert!(msg.contains("Finishing"));
    }

    #[test]
    fn test_normalize_prop_name() {
        assert_eq!(normalize_prop_name("AttributeFinishing"), "finishing");
        assert_eq!(normalize_prop_name("Finishing"), "finishing");
        assert_eq!(normalize_prop_name("attribute_crossing"), "crossing");
        assert_eq!(normalize_prop_name("CA"), "ca");
    }

    #[test]
    fn test_attribute_binding_struct() {
        let primary = BindingSlot::new(0x20000, SlotKind::Numeric, NUMBER_KIND_SIGNED_WHOLE);
        let sec = BindingSlot::new(0x30000, SlotKind::Numeric, NUMBER_KIND_SIGNED_WHOLE);
        let binding = AttributeBinding {
            name: "Vision".into(),
            primary_slot: primary.clone(),
            secondary_slot: Some(sec.clone()),
        };
        assert_eq!(binding.name, "Vision");
        assert_eq!(binding.primary_slot, primary);
        assert_eq!(binding.secondary_slot, Some(sec));
    }
}
