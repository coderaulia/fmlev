//! Read-only diagnostic: dump every entry of FM26's live `BindingSubsystem`
//! (mirrors the walk performed by `extract_live_bound_players` in
//! `fmlev-core/src/context.rs`) with raw TypedValue bytes and decoded values.
//!
//! Layout notes (output_dump/dump.cs):
//! - `Bindings.Data.m_value` @ 0x30 → TypedValue object
//! - `TypedValue<T>.m_value` @ 0x18
//! - `NumericTypedValue : TypedValue<DynamicNumber>`; `DynamicNumber` = { Union(i64/u64/f64) @0, NumberKind @8 }
//!   → object offsets 0x18 (union) and 0x20 (kind)
//! - `ReferenceTypedValue : TypedValue<object>` → 0x18 is an object pointer
//! - `enum DynamicNumber.NumberKind { SignedWhole = 0, UnsignedWhole = 1, FloatingPoint = 2, Invalid = 3 }`

use std::collections::{BTreeMap, HashMap};

use fmlev_il2cpp::{types::read_cstring, Il2CppString, OffsetsRegistry};
use fmlev_mem::{find_fm_process, find_module_bases, LinuxMemoryDriver};

/// Il2CppClass.parent offset (Unity 2021+/IL2CPP v29+ x64).
const KLASS_PARENT_OFFSET: usize = 0x58;
const ENTRY_SIZE: usize = 56;

pub fn number_kind_name(kind: i32) -> &'static str {
    match kind {
        0 => "SignedWhole",
        1 => "UnsignedWhole",
        2 => "FloatingPoint",
        3 => "Invalid",
        _ => "?",
    }
}

fn is_ptr(p: usize) -> bool {
    p > 0x10000 && p < 0x7fff_ffff_ffff
}

fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.len() < 200 && s.chars().all(|c| c.is_ascii_graphic())
}

/// Returns (class name, namespace) of the managed object at `obj`, if it looks valid.
fn class_of(driver: &LinuxMemoryDriver, obj: usize) -> Option<(String, String, usize)> {
    if !is_ptr(obj) {
        return None;
    }
    let klass = driver.read_ptr(obj).ok()?;
    class_name(driver, klass).map(|(n, ns)| (n, ns, klass))
}

fn class_name(driver: &LinuxMemoryDriver, klass: usize) -> Option<(String, String)> {
    if !is_ptr(klass) {
        return None;
    }
    let name_ptr = driver.read_ptr(klass + 0x10).ok()?;
    let ns_ptr = driver.read_ptr(klass + 0x18).ok()?;
    if !is_ptr(name_ptr) {
        return None;
    }
    let name = read_cstring(driver, name_ptr, 128).ok()?;
    if !is_ident(&name) {
        return None;
    }
    let ns = if is_ptr(ns_ptr) { read_cstring(driver, ns_ptr, 128).unwrap_or_default() } else { String::new() };
    Some((name, ns))
}

fn parent_name(driver: &LinuxMemoryDriver, klass: usize) -> String {
    driver
        .read_ptr(klass + KLASS_PARENT_OFFSET)
        .ok()
        .and_then(|p| class_name(driver, p))
        .map(|(n, _)| n)
        .unwrap_or_default()
}

fn read_string_obj(driver: &LinuxMemoryDriver, obj: usize) -> Option<String> {
    let (name, ns, _) = class_of(driver, obj)?;
    if name == "String" && ns == "System" {
        Il2CppString::read(driver, obj).ok()
    } else {
        None
    }
}

/// FM link strings look like `\u{1}<base64 payload>\u{2}Display Text\u{1}`; return the display text.
/// Accepts both raw control chars and their `{:?}`-escaped form.
pub fn decode_link_text(s: &str) -> String {
    let s = s.replace("\\u{1}", "\u{1}").replace("\\u{2}", "\u{2}");
    match s.rfind('\u{2}') {
        Some(p) => s[p + 1..].trim_end_matches('\u{1}').trim().to_string(),
        None => s.trim_matches('\u{1}').trim().to_string(),
    }
}

/// Read-only: print the managed class of `addr` and each qword of its first `size` bytes,
/// annotating pointers with their class name (and string contents), recursing `depth` levels.
pub fn inspect_object(addr: usize, size: usize, depth: usize) -> anyhow::Result<()> {
    let proc = find_fm_process().map_err(|e| anyhow::anyhow!("FM26 not running: {e}"))?;
    let driver = LinuxMemoryDriver::new(proc.pid);
    inspect_rec(&driver, addr, size, depth, 0);
    Ok(())
}

fn inspect_rec(driver: &LinuxMemoryDriver, addr: usize, size: usize, depth: usize, indent: usize) {
    let pad = "    ".repeat(indent);
    let cls = class_of(driver, addr)
        .map(|(n, ns, k)| format!("{ns}.{n} (klass 0x{k:x}, parent {})", parent_name(driver, k)))
        .unwrap_or_else(|| "<no class>".into());
    println!("{pad}@0x{addr:x}: {cls}");
    let mut buf = vec![0u8; size.clamp(8, 4096) & !7];
    if driver.read_bytes(addr, &mut buf).is_err() {
        println!("{pad}  <unreadable>");
        return;
    }
    for (i, q) in buf.chunks(8).enumerate() {
        let off = i * 8;
        if off < 0x10 {
            continue; // klass + monitor
        }
        let v = u64::from_ne_bytes(q.try_into().unwrap());
        let lo = v as u32 as i32;
        let hi = (v >> 32) as u32 as i32;
        let f = f64::from_bits(v);
        let mut note = String::new();
        let p = v as usize;
        if let Some(s) = read_string_obj(driver, p) {
            note = format!(" -> str {s:?}");
        } else if let Some((n, ns, _)) = class_of(driver, p) {
            note = format!(" -> {ns}.{n}");
        }
        println!("{pad}  +0x{off:03x}: {v:016x}  i32=({lo}, {hi}) f64={f:.4e}{note}");
        if !note.is_empty() && !note.contains("str ") && depth > 0 {
            inspect_rec(driver, p, size, depth - 1, indent + 1);
        }
    }
}

struct Row {
    entry_idx: usize,
    person: i32,
    prop: String,
    data_key: usize,
    tv_class: String,
    tv_parent: String,
    raw: [u8; 16],
    obj_desc: String,
}

pub struct ProbeArgs {
    pub person: Option<String>,
    pub prop: Option<String>,
    pub limit: usize,
    pub all_types: bool,
}

// Stage references use bounded InteropReference.Pair IDs, not the database
// index/type fields. Accept only an explicit competition reference and season.
fn stage_identity(pairs: &[u8]) -> Option<(i32, i32)> {
    if pairs.is_empty() || pairs.len() > 32 * 8 || pairs.len() % 8 != 0 { return None; }
    let mut competition = None;
    let mut year = None;
    for pair in pairs.chunks_exact(8) {
        let key = u32::from_ne_bytes(pair[..4].try_into().ok()?);
        let value = i32::from_ne_bytes(pair[4..].try_into().ok()?);
        if key == 0x64726566 {
            if competition.is_some() || (value as u32 >> 24) != 25 || value & 0x00ff_ffff == 0 { return None; }
            competition = Some(value & 0x00ff_ffff);
        } else if key == 0x79656172 {
            if year.is_some() || !(1900..=2200).contains(&value) { return None; }
            year = Some(value);
        }
    }
    Some((competition?, year?))
}

fn read_stage_identity(driver: &LinuxMemoryDriver, object: usize) -> Option<(i32, i32)> {
    let (class, namespace, _) = class_of(driver, object)?;
    if namespace != "FM.UI" || !matches!(class.as_str(), "CompetitionStageReference" | "CompetitionStageWithStatsReference") { return None; }
    let list = driver.read_ptr(object + 0x10).ok()?;
    if !is_ptr(list) { return None; }
    let items = driver.read_ptr(list + 0x10).ok()?;
    let count = driver.read::<i32>(list + 0x18).ok()?;
    if !is_ptr(items) || !(1..=32).contains(&count) { return None; }
    let capacity = driver.read::<usize>(items + 0x18).ok()?;
    if capacity < count as usize || capacity > 256 { return None; }
    let mut pairs = vec![0; count as usize * 8];
    driver.read_bytes(items + 0x20, &mut pairs).ok()?;
    stage_identity(&pairs)
}

pub fn run(args: ProbeArgs) -> anyhow::Result<()> {
    if args.person.as_deref() == Some("__deep__") {
        return run_deep_probe(args.prop.as_deref());
    }
    let proc = find_fm_process().map_err(|e| anyhow::anyhow!("FM26 not running: {e}"))?;
    let bases = find_module_bases(proc.pid)?;
    let base = bases.game_assembly_base.ok_or_else(|| anyhow::anyhow!("GameAssembly.dll not mapped"))?;
    let driver = LinuxMemoryDriver::new(proc.pid);

    let registry = OffsetsRegistry::load_from_file("offsets/v26.0.0.json")
        .or_else(|_| OffsetsRegistry::load_from_json(include_str!("../../../offsets/v26.0.0.json")))
        .map_err(|e| anyhow::anyhow!("offsets load failed: {e}"))?;

    let gs = registry.get_singleton("GameSubsystems").map_err(|e| anyhow::anyhow!("{e}"))?;
    let gs_sf = fmlev_il2cpp::read_static_singleton(&driver, base, gs)?;
    let sub_list = if is_ptr(gs_sf) { driver.read_ptr(gs_sf)? } else { 0 };
    anyhow::ensure!(is_ptr(sub_list), "GameSubsystems list null (gs_sf=0x{gs_sf:x})");
    let sub_items = driver.read_ptr(sub_list + 0x10)?;
    let sub_count = driver.read::<i32>(sub_list + 0x18)?;
    anyhow::ensure!(is_ptr(sub_items) && sub_count > 11, "subsystem list too small ({sub_count})");
    let b_sub = driver.read_ptr(sub_items + 0x20 + 11 * 8)?;
    let b_cls = class_of(&driver, b_sub).map(|c| c.0).unwrap_or_default();
    println!("PID {} GameAssembly 0x{base:x} | GameSubsystems[11] = 0x{b_sub:x} ({b_cls})", proc.pid);

    let m_data = driver.read_ptr(b_sub + 0x78)?;
    let m_ctx = driver.read_ptr(b_sub + 0x88)?;
    let data_items = driver.read_ptr(m_data + 0x10)?;
    let data_size = driver.read::<i32>(m_data + 0x18)?;
    let entries = driver.read_ptr(m_ctx + 0x18)?;
    let count = driver.read::<i32>(m_ctx + 0x20)?;
    println!("m_data=0x{m_data:x} size={data_size} | m_ctx=0x{m_ctx:x} entries=0x{entries:x} count={count}");
    anyhow::ensure!(is_ptr(data_items) && is_ptr(entries) && count > 0 && data_size > 0, "binding tables empty");

    let max_entries = (count as usize).min(200_000);
    let mut raw_entries = vec![0u8; max_entries * ENTRY_SIZE];
    driver.read_bytes(entries + 0x20, &mut raw_entries)?;

    let rd = |b: &[u8], o: usize| usize::from_ne_bytes(b[o..o + 8].try_into().unwrap());
    let mut rows: Vec<Row> = Vec::new();
    let mut names: HashMap<i32, String> = HashMap::new();
    let mut stage_groups: HashMap<usize, i32> = HashMap::new();
    let mut ref_types: BTreeMap<i32, usize> = BTreeMap::new();

    for i in 0..max_entries {
        let e = &raw_entries[i * ENTRY_SIZE..(i + 1) * ENTRY_SIZE];
        let src_tv = rd(e, 8);
        let prop_ptr = rd(e, 16);
        let data_key = rd(e, 40) & 0xFFFF_FFFF;
        if !is_ptr(src_tv) || !is_ptr(prop_ptr) || data_key >= data_size as usize {
            continue;
        }
        let ref_obj = driver.read_ptr(src_tv + 0x18).unwrap_or(0);
        if !is_ptr(ref_obj) {
            continue;
        }
        let m_idx = driver.read::<i32>(ref_obj + 0x20).unwrap_or(0);
        let t_typ = driver.read::<i32>(ref_obj + 0x24).unwrap_or(0);
        *ref_types.entry(t_typ).or_default() += 1;
        let stage = if args.all_types && m_idx <= 0 { read_stage_identity(&driver, ref_obj) } else { None };
        if stage.is_none() && ((!args.all_types && t_typ != 1) || m_idx <= 0) {
            continue;
        }
        let p_str_ptr = driver.read_ptr(prop_ptr + 0x10).unwrap_or(0);
        let prop = Il2CppString::read(&driver, p_str_ptr)
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| Il2CppString::read(&driver, prop_ptr).ok())
            .unwrap_or_default();
        if prop.is_empty() {
            continue;
        }
        let person = if let Some((competition, year)) = stage {
            let next_group = i32::MIN + stage_groups.len() as i32;
            let group = *stage_groups.entry(ref_obj).or_insert(next_group);
            names.entry(group).or_insert_with(|| format!("CompetitionStageReference source=0x{ref_obj:x} competition={competition} year={year}"));
            group
        } else if args.all_types { m_idx | (t_typ << 24) } else { m_idx };

        let d_obj = driver.read_ptr(data_items + 0x20 + data_key * 8).unwrap_or(0);
        let val_tv = if is_ptr(d_obj) { driver.read_ptr(d_obj + 0x30).unwrap_or(0) } else { 0 };
        let mut raw = [0u8; 16];
        let (mut tv_class, mut tv_parent) = (String::from("<null>"), String::new());
        let mut obj_desc = String::new();
        if is_ptr(val_tv) {
            let _ = driver.read_bytes(val_tv + 0x18, &mut raw);
            if let Some((n, ns, klass)) = class_of(&driver, val_tv) {
                tv_class = if ns.is_empty() { n } else { format!("{ns}.{n}") };
                tv_parent = parent_name(&driver, klass);
            } else {
                tv_class = "<bad klass>".into();
            }
            let p18 = usize::from_ne_bytes(raw[0..8].try_into().unwrap());
            if let Some(s) = read_string_obj(&driver, p18) {
                obj_desc = format!("str={s:?}");
            } else if let Some((n, ns, _)) = class_of(&driver, p18) {
                let r_idx = driver.read::<i32>(p18 + 0x20).unwrap_or(0);
                let r_typ = driver.read::<i32>(p18 + 0x24).unwrap_or(0);
                obj_desc = format!("obj={ns}.{n} [+0x20={r_idx} +0x24={r_typ}]");
                // Boxed primitives: value at +0x10
                if ns == "System" {
                    let v = driver.read::<i64>(p18 + 0x10).unwrap_or(0);
                    obj_desc.push_str(&format!(" boxed@0x10={v}"));
                }
            }
            if matches!(prop.as_str(), "Name" | "FullName" | "InitialSurname" | "ShirtName") {
                if let Some(s) = obj_desc.strip_prefix("str=") {
                    names.entry(person).or_insert_with(|| decode_link_text(s.trim_matches('"')));
                }
            }
        }
        rows.push(Row { entry_idx: i, person, prop, data_key, tv_class, tv_parent, raw, obj_desc });
    }

    println!("SourceRef type histogram (+0x24 → count): {ref_types:?}");
    println!("Collected {} rows for {} references\n", rows.len(), rows.iter().map(|r| r.person).collect::<std::collections::HashSet<_>>().len());

    let person_filter = args.person.as_ref().map(|s| s.to_lowercase());
    let prop_filter = args.prop.as_ref().map(|s| s.to_lowercase());
    let mut by_person: BTreeMap<i32, Vec<&Row>> = BTreeMap::new();
    for r in &rows {
        if let Some(pf) = &person_filter {
            let n = names.get(&r.person).map(|s| s.to_lowercase()).unwrap_or_default();
            if !n.contains(pf.as_str()) {
                continue;
            }
        }
        if let Some(pf) = &prop_filter {
            if !r.prop.to_lowercase().contains(pf.as_str()) {
                continue;
            }
        }
        by_person.entry(r.person).or_default().push(r);
    }

    let mut printed = 0usize;
    for (person, mut list) in by_person {
        if printed >= args.limit {
            println!("... (limit {} reached)", args.limit);
            break;
        }
        list.sort_by(|a, b| a.prop.cmp(&b.prop));
        println!("══ person m_index={person} name={:?} ({} props) ══", names.get(&person), list.len());
        for r in list {
            let i64v = i64::from_ne_bytes(r.raw[0..8].try_into().unwrap());
            let f64v = f64::from_ne_bytes(r.raw[0..8].try_into().unwrap());
            let kind = i32::from_ne_bytes(r.raw[8..12].try_into().unwrap());
            let hex: String = r.raw.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
            let decoded = if r.tv_class.ends_with("NumericTypedValue") {
                match kind {
                    0 => format!("= {i64v}"),
                    1 => format!("= {}", i64v as u64),
                    2 => format!("= {f64v}"),
                    _ => "= <invalid>".to_string(),
                }
            } else {
                String::new()
            };
            println!(
                "  #{:<6} key={:<6} {:<34} {:<28}({:<14}) [{hex}] i64={i64v} f64={f64v:.6e} kind@0x20={kind}({}) {decoded} {}",
                r.entry_idx,
                r.data_key,
                r.prop,
                r.tv_class,
                r.tv_parent,
                number_kind_name(kind),
                r.obj_desc
            );
        }
        printed += 1;
    }
    Ok(())
}

fn fourcc_str(k: u32) -> String {
    let b = k.to_le_bytes();
    let s: String = b.iter().map(|&c| if (32..=126).contains(&c) { c as char } else { '.' }).collect();
    let rev: String = b.iter().rev().map(|&c| if (32..=126).contains(&c) { c as char } else { '.' }).collect();
    format!("0x{k:08x} (le='{s}' be='{rev}')")
}

fn dump_tv_deep(
    driver: &LinuxMemoryDriver,
    tv: usize,
    dyn_ctx: &HashMap<usize, Vec<(String, usize, usize)>>,
    indent: usize,
    depth: usize,
) {
    let pad = "  ".repeat(indent);
    if !is_ptr(tv) {
        println!("{pad}<null tv 0x{tv:x}>");
        return;
    }
    let cls = class_of(driver, tv).map(|(n, ns, _)| format!("{ns}.{n}")).unwrap_or_else(|| "?".into());
    let val_u64 = driver.read::<u64>(tv + 0x18).unwrap_or(0);
    let kind = driver.read::<i32>(tv + 0x20).unwrap_or(0);
    if cls.ends_with("NumericTypedValue") {
        let desc = match kind {
            0 => format!("i64={}", val_u64 as i64),
            1 => format!("u64={val_u64}"),
            2 => format!("f64={}", f64::from_bits(val_u64)),
            _ => format!("raw=0x{val_u64:x} kind={kind}"),
        };
        println!("{pad}TV@0x{tv:x} [{cls}] {desc} (val_addr=0x{:x})", tv + 0x18);
    } else if cls.ends_with("ReferenceTypedValue") {
        let obj = val_u64 as usize;
        if let Some(s) = read_string_obj(driver, obj) {
            println!("{pad}TV@0x{tv:x} [{cls}] -> str {s:?} ( decoded={:?} )", decode_link_text(&s));
        } else if let Some((on, ons, _)) = class_of(driver, obj) {
            let r_idx = driver.read::<i32>(obj + 0x20).unwrap_or(0);
            let r_typ = driver.read::<i32>(obj + 0x24).unwrap_or(0);
            println!("{pad}TV@0x{tv:x} [{cls}] -> @0x{obj:x} {ons}.{on} (+0x20={r_idx}, +0x24={r_typ})");
            if depth > 0 {
                if on == "DynamicReference" || on == "DynamicReferenceWithNotify" || on == "Record" {
                    dump_dyn_ref(driver, obj, dyn_ctx, indent + 1, depth - 1);
                } else if on.starts_with("List`1") {
                    let items = driver.read_ptr(obj + 0x10).unwrap_or(0);
                    let sz = driver.read::<i32>(obj + 0x18).unwrap_or(0);
                    println!("{pad}  List size={sz} items@0x{items:x}");
                    for idx in 0..sz.clamp(0, 30) as usize {
                        let elem = driver.read_ptr(items + 0x20 + idx * 8).unwrap_or(0);
                        println!("{pad}  [{idx}]:");
                        dump_tv_deep(driver, elem, dyn_ctx, indent + 2, depth - 1);
                    }
                }
            }
        } else {
            println!("{pad}TV@0x{tv:x} [{cls}] -> ptr 0x{obj:x}");
        }
    } else {
        let b0 = (val_u64 & 0xff) as u8;
        println!("{pad}TV@0x{tv:x} [{cls}] raw_u64=0x{val_u64:x} (u8={b0}, i32={}) kind@0x20=0x{kind:x}", val_u64 as i32);
    }
}

fn dump_dyn_ref(
    driver: &LinuxMemoryDriver,
    dyn_ref: usize,
    dyn_ctx: &HashMap<usize, Vec<(String, usize, usize)>>,
    indent: usize,
    depth: usize,
) {
    let pad = "  ".repeat(indent);
    let entries = driver.read_ptr(dyn_ref + 0x18).unwrap_or(0);
    let count = driver.read::<i32>(dyn_ref + 0x20).unwrap_or(0);
    println!("{pad}DynamicReference@0x{dyn_ref:x} dict_count={count} entries@0x{entries:x}");
    if is_ptr(entries) && count > 0 && count <= 64 {
        for i in 0..count as usize {
            let ea = entries + 0x20 + i * 24;
            let hc = driver.read::<i32>(ea).unwrap_or(-1);
            let key = driver.read::<u32>(ea + 8).unwrap_or(0);
            let v_tv = driver.read_ptr(ea + 16).unwrap_or(0);
            println!("{pad}  dict[{i}] hc={hc} key={}:", fourcc_str(key));
            dump_tv_deep(driver, v_tv, dyn_ctx, indent + 2, depth);
        }
    }
    if let Some(ctx_list) = dyn_ctx.get(&dyn_ref) {
        println!("{pad}  m_ctx bindings on DynamicReference@0x{dyn_ref:x} ({} props):", ctx_list.len());
        for (prop, d_obj, v_tv) in ctx_list {
            let h_ptr = driver.read_ptr(*d_obj + 0x20).unwrap_or(0);
            let h_cls = class_of(driver, h_ptr).map(|(n, ns, _)| format!("{ns}.{n}")).unwrap_or_default();
            println!("{pad}    prop={prop:?} d_obj=0x{d_obj:x} (handler=0x{h_ptr:x} {h_cls}):");
            dump_tv_deep(driver, *v_tv, dyn_ctx, indent + 3, depth);
        }
    }
}

pub fn run_deep_probe(mode: Option<&str>) -> anyhow::Result<()> {
    let proc = find_fm_process().map_err(|e| anyhow::anyhow!("FM26 not running: {e}"))?;
    let bases = find_module_bases(proc.pid)?;
    let base = bases.game_assembly_base.ok_or_else(|| anyhow::anyhow!("GameAssembly.dll not mapped"))?;
    let driver = LinuxMemoryDriver::new(proc.pid);

    let registry = OffsetsRegistry::load_from_file("offsets/v26.0.0.json")
        .or_else(|_| OffsetsRegistry::load_from_json(include_str!("../../../offsets/v26.0.0.json")))
        .map_err(|e| anyhow::anyhow!("offsets load failed: {e}"))?;

    let gs = registry.get_singleton("GameSubsystems").map_err(|e| anyhow::anyhow!("{e}"))?;
    let gs_sf = fmlev_il2cpp::read_static_singleton(&driver, base, gs)?;
    let sub_list = driver.read_ptr(gs_sf)?;
    let sub_items = driver.read_ptr(sub_list + 0x10)?;
    let sub_count = driver.read::<i32>(sub_list + 0x18)?;
    println!("=== Subsystems in GameSubsystems (count={sub_count}) ===");
    for idx in 0..sub_count.clamp(0, 32) as usize {
        let s_ptr = driver.read_ptr(sub_items + 0x20 + idx * 8).unwrap_or(0);
        let s_cls = class_of(&driver, s_ptr).map(|(n, ns, _)| format!("{ns}.{n}")).unwrap_or_default();
        println!("  [{idx:2}] 0x{s_ptr:x} ({s_cls})");
    }

    let b_sub = driver.read_ptr(sub_items + 0x20 + 11 * 8)?;
    let m_data = driver.read_ptr(b_sub + 0x78)?;
    let m_ctx = driver.read_ptr(b_sub + 0x88)?;
    let data_items = driver.read_ptr(m_data + 0x10)?;
    let data_size = driver.read::<i32>(m_data + 0x18)?;
    let entries = driver.read_ptr(m_ctx + 0x18)?;
    let count = driver.read::<i32>(m_ctx + 0x20)?;

    // Also inspect m_handlers on BindingSubsystem! Where are IDataHandlers stored in Bindings?
    let max_entries = (count as usize).min(200_000);
    let mut raw_entries = vec![0u8; max_entries * ENTRY_SIZE];
    driver.read_bytes(entries + 0x20, &mut raw_entries)?;
    let rd = |b: &[u8], o: usize| usize::from_ne_bytes(b[o..o + 8].try_into().unwrap());

    let mut dyn_ctx: HashMap<usize, Vec<(String, usize, usize)>> = HashMap::new();
    let mut person_props: HashMap<i32, Vec<(String, usize, usize, usize)>> = HashMap::new();
    let mut handler_hist: HashMap<usize, (String, usize)> = HashMap::new();

    for i in 0..max_entries {
        let e = &raw_entries[i * ENTRY_SIZE..(i + 1) * ENTRY_SIZE];
        let src_tv = rd(e, 8);
        let prop_ptr = rd(e, 16);
        let data_key = rd(e, 40) & 0xFFFF_FFFF;
        if !is_ptr(src_tv) || !is_ptr(prop_ptr) || data_key >= data_size as usize {
            continue;
        }
        let ref_obj = driver.read_ptr(src_tv + 0x18).unwrap_or(0);
        if !is_ptr(ref_obj) {
            continue;
        }
        let p_str_ptr = driver.read_ptr(prop_ptr + 0x10).unwrap_or(0);
        let prop = Il2CppString::read(&driver, p_str_ptr)
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| Il2CppString::read(&driver, prop_ptr).ok())
            .unwrap_or_default();
        if prop.is_empty() {
            continue;
        }
        let d_obj = driver.read_ptr(data_items + 0x20 + data_key * 8).unwrap_or(0);
        let val_tv = if is_ptr(d_obj) { driver.read_ptr(d_obj + 0x30).unwrap_or(0) } else { 0 };
        let h_ptr = if is_ptr(d_obj) { driver.read_ptr(d_obj + 0x20).unwrap_or(0) } else { 0 };
        if is_ptr(h_ptr) {
            let e = handler_hist.entry(h_ptr).or_insert_with(|| {
                let c = class_of(&driver, h_ptr).map(|(n, ns, _)| format!("{ns}.{n}")).unwrap_or_default();
                (c, 0)
            });
            e.1 += 1;
        }

        let m_idx = driver.read::<i32>(ref_obj + 0x20).unwrap_or(0);
        let t_typ = driver.read::<i32>(ref_obj + 0x24).unwrap_or(0);
        if let Some((rn, _, _)) = class_of(&driver, ref_obj) {
            if rn == "DynamicReference" || rn == "DynamicReferenceWithNotify" || t_typ == -1 {
                dyn_ctx.entry(ref_obj).or_default().push((prop.clone(), d_obj, val_tv));
            }
        }
        if t_typ == 1 && (m_idx == 65304 || m_idx == 38141) {
            person_props.entry(m_idx).or_default().push((prop, d_obj, val_tv, ref_obj));
        }
    }

    println!("\n=== IDataHandler Histogram across m_ctx ===");
    for (h_ptr, (cls, cnt)) in &handler_hist {
        println!("  handler @ 0x{h_ptr:x} ({cls}) -> {cnt} bindings");
    }

    if mode != Some("scan_only") {
        for &pid_idx in &[65304i32, 38141i32] {
            println!("\n=======================================================");
            println!("=== DEEP BINDINGS FOR PERSON m_index={pid_idx} ===");
            println!("=======================================================");
            if let Some(plist) = person_props.get(&pid_idx) {
                for (prop, d_obj, val_tv, ref_obj) in plist {
                    let h_ptr = driver.read_ptr(*d_obj + 0x20).unwrap_or(0);
                    let h_cls = class_of(&driver, h_ptr).map(|(n, ns, _)| format!("{ns}.{n}")).unwrap_or_default();
                    let opener = driver.read::<u64>(*d_obj + 0x28).unwrap_or(0);
                    let d_key = driver.read::<u64>(*d_obj + 0x10).unwrap_or(0);
                    if matches!(
                        prop.as_str(),
                        "UniqueId"
                            | "Age"
                            | "Footedness"
                            | "PositionAptitudeList"
                            | "PositionalAbilitiesRoleTable"
                            | "AttributesTechnical"
                            | "AttributesMental"
                            | "AttributesPhysical"
                            | "AttributesSetPieces"
                            | "PlayerReportForContextTeam"
                            | "BestPositionMask"
                            | "CoachBestPosition"
                    ) {
                        println!(
                            "\n--- Property {prop:?} (PersonRef=0x{ref_obj:x}, d_obj=0x{d_obj:x}, d_key=0x{d_key:x}, opener=0x{opener:x}, handler=0x{h_ptr:x} [{h_cls}]) ---"
                        );
                        dump_tv_deep(&driver, *val_tv, &dyn_ctx, 1, 3);
                    }
                }
            }
        }
    }

    if mode == Some("scan") || mode == Some("scan_only") {
        scan_native_memory(proc.pid, &driver)?;
    }
    if mode == Some("struct") {
        analyze_native_player_structs(proc.pid, &driver)?;
    }

    Ok(())
}

fn analyze_native_player_structs(pid: i32, driver: &LinuxMemoryDriver) -> anyhow::Result<()> {
    let luk_player = 0x2c9798a00usize;
    let luk_person = 0x2c9798b40usize;
    let kur_player = 0x2d0a26a78usize;
    let kur_person = 0x2d0a26bb8usize;

    let mut luk_buf = vec![0u8; 0x3c8];
    let mut kur_buf = vec![0u8; 0x3c8];
    driver.read_bytes(luk_player, &mut luk_buf)?;
    driver.read_bytes(kur_player, &mut kur_buf)?;

    println!("=== Side-by-side Player/Person Struct Comparison (size 0x3c8 = 968 bytes) ===");
    println!("Lukmansyah: Player @ 0x{luk_player:x}, Person @ 0x{luk_person:x} (+0x140)");
    println!("Kurzawa:    Player @ 0x{kur_player:x}, Person @ 0x{kur_person:x} (+0x140)");

    println!("\n--- Byte-by-byte +0x00 .. +0x50 (Positions & Attributes) ---");
    for off in 0..0x50 {
        let lb = luk_buf[off];
        let kb = kur_buf[off];
        let l_div5 = (lb as f32 / 5.0).round() as u8;
        let k_div5 = (kb as f32 / 5.0).round() as u8;
        println!(
            "  +0x{off:02x}: Luk={lb:3} (0x{lb:02x}, /5={l_div5:2}) | Kur={kb:3} (0x{kb:02x}, /5={k_div5:2})"
        );
    }

    println!("\n--- 2-byte/4-byte/8-byte fields across +0x50 .. +0x3c8 ---");
    for off in (0x50..0x3c8).step_by(8) {
        let lq = u64::from_ne_bytes(luk_buf[off..off + 8].try_into().unwrap());
        let kq = u64::from_ne_bytes(kur_buf[off..off + 8].try_into().unwrap());
        if lq == 0 && kq == 0 {
            continue;
        }
        let l_hex: String = luk_buf[off..off + 8].iter().map(|b| format!("{b:02x} ")).collect();
        let k_hex: String = kur_buf[off..off + 8].iter().map(|b| format!("{b:02x} ")).collect();
        let person_rel = off as isize - 0x140;
        let l_u16s = format!(
            "u16=({},{},{},{})",
            u16::from_ne_bytes([luk_buf[off], luk_buf[off + 1]]),
            u16::from_ne_bytes([luk_buf[off + 2], luk_buf[off + 3]]),
            u16::from_ne_bytes([luk_buf[off + 4], luk_buf[off + 5]]),
            u16::from_ne_bytes([luk_buf[off + 6], luk_buf[off + 7]]),
        );
        let k_u16s = format!(
            "u16=({},{},{},{})",
            u16::from_ne_bytes([kur_buf[off], kur_buf[off + 1]]),
            u16::from_ne_bytes([kur_buf[off + 2], kur_buf[off + 3]]),
            u16::from_ne_bytes([kur_buf[off + 4], kur_buf[off + 5]]),
            u16::from_ne_bytes([kur_buf[off + 6], kur_buf[off + 7]]),
        );
        println!(
            "  Player+0x{off:03x} (Person{person_rel:+04x}): Luk=[{l_hex}] {l_u16s} | Kur=[{k_hex}] {k_u16s}"
        );
    }

    // Scan memory for pointers to luk_player, luk_person, kur_player, kur_person
    let maps = std::fs::read_to_string(format!("/proc/{pid}/maps"))?;
    let targets = [
        ("luk_player", luk_player as u64),
        ("luk_person", luk_person as u64),
        ("kur_player", kur_player as u64),
        ("kur_person", kur_person as u64),
    ];
    let mut ptr_hits: HashMap<&str, Vec<(usize, String)>> = HashMap::new();
    for line in maps.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 || !parts[1].starts_with('r') {
            continue;
        }
        let path = if parts.len() >= 6 { parts[5..].join(" ") } else { String::new() };
        let mut range_it = parts[0].split('-');
        let start = usize::from_str_radix(range_it.next().unwrap_or("0"), 16).unwrap_or(0);
        let end = usize::from_str_radix(range_it.next().unwrap_or("0"), 16).unwrap_or(0);
        if end <= start || end - start > 2 * 1024 * 1024 * 1024 {
            continue;
        }
        let chunk_sz = 16 * 1024 * 1024;
        let mut buf = Vec::new();
        let mut cur = start;
        while cur < end {
            let len = (end - cur).min(chunk_sz);
            buf.resize(len, 0);
            if driver.read_bytes(cur, &mut buf).is_ok() {
                for i in (0..len.saturating_sub(8)).step_by(8) {
                    let q = u64::from_ne_bytes(buf[i..i + 8].try_into().unwrap());
                    for &(name, t) in &targets {
                        if q == t {
                            ptr_hits.entry(name).or_default().push((cur + i, path.clone()));
                        }
                    }
                }
            }
            cur += len;
        }
    }

    println!("\n=== Pointer XREFs to Native Player/Person Structs ===");
    for &(name, t) in &targets {
        let list = ptr_hits.get(name).cloned().unwrap_or_default();
        println!("  {name} (0x{t:x}): {} hits", list.len());
        for (h, p) in &list {
            // Check if this hit is part of an m_index-indexed array!
            let m_idx = if name.starts_with("luk") { 65304usize } else { 38141usize };
            let other_idx = if name.starts_with("luk") { 38141usize } else { 65304usize };
            let base_cand = h.wrapping_sub(m_idx * 8);
            let other_ptr = driver.read_ptr(base_cand.wrapping_add(other_idx * 8)).unwrap_or(0);
            println!("    @ 0x{h:x} [{p}] (if table[m_idx*8], base=0x{base_cand:x}, table[{other_idx}]=0x{other_ptr:x})");
        }
    }

    Ok(())
}

fn scan_native_memory(pid: i32, driver: &LinuxMemoryDriver) -> anyhow::Result<()> {
    let maps = std::fs::read_to_string(format!("/proc/{pid}/maps"))?;
    let luk_uid = 2000448177u32.to_le_bytes();
    let kur_uid = 85010869u32.to_le_bytes();

    println!("\n=== Scanning /proc/{pid}/maps for UniqueIds 2000448177 (0x773c6ab1) & 85010869 (0x051129b5) ===");
    let mut luk_hits = Vec::new();
    let mut kur_hits = Vec::new();

    for line in maps.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let perms = parts[1];
        if !perms.starts_with("rw") {
            continue;
        }
        let path = if parts.len() >= 6 { parts[5..].join(" ") } else { String::new() };
        let mut range_it = parts[0].split('-');
        let start = usize::from_str_radix(range_it.next().unwrap_or("0"), 16).unwrap_or(0);
        let end = usize::from_str_radix(range_it.next().unwrap_or("0"), 16).unwrap_or(0);
        if end <= start || end - start > 2 * 1024 * 1024 * 1024 {
            continue;
        }

        let chunk_sz = 16 * 1024 * 1024;
        let mut buf = Vec::new();
        let mut cur = start;
        while cur < end {
            let len = (end - cur).min(chunk_sz);
            buf.resize(len, 0);
            if driver.read_bytes(cur, &mut buf).is_ok() {
                for i in (0..len.saturating_sub(4)).step_by(4) {
                    let b = &buf[i..i + 4];
                    if b == luk_uid {
                        luk_hits.push((cur + i, perms.to_string(), path.clone()));
                    } else if b == kur_uid {
                        kur_hits.push((cur + i, perms.to_string(), path.clone()));
                    }
                }
            }
            cur += len;
        }
    }

    println!("Found {} Lukmansyah UniqueId hits and {} Kurzawa UniqueId hits", luk_hits.len(), kur_hits.len());

    for (label, uid, m_idx, hits) in [
        ("Lukmansyah", 2000448177u32, 65304u32, &luk_hits),
        ("Kurzawa", 85010869u32, 38141u32, &kur_hits),
    ] {
        println!("\n==================== {label} (uid={uid}, m_idx={m_idx}) ====================");
        for (addr, perms, path) in hits {
            let tv_cand = addr.saturating_sub(0x18);
            let tv_cls = class_of(driver, tv_cand).map(|(n, ns, _)| format!("{ns}.{n}")).unwrap_or_default();
            println!("\nHit @ 0x{addr:x} [{perms} {path}] (addr-0x18 class={tv_cls:?})");
            if tv_cls.ends_with("NumericTypedValue") {
                continue;
            }
            // Dump -0x80 .. +0x280 around hit
            let dump_start = (addr & !7).saturating_sub(0x80);
            let mut dbuf = vec![0u8; 0x300];
            if driver.read_bytes(dump_start, &mut dbuf).is_ok() {
                for row in 0..(dbuf.len() / 16) {
                    let off = row * 16;
                    let a = dump_start + off;
                    let rel = a as isize - *addr as isize;
                    let q0 = u64::from_ne_bytes(dbuf[off..off + 8].try_into().unwrap());
                    let q1 = u64::from_ne_bytes(dbuf[off + 8..off + 16].try_into().unwrap());
                    let hex: String = dbuf[off..off + 16].iter().map(|b| format!("{b:02x} ")).collect();
                    let mut notes = String::new();
                    for (qi, q) in [(0, q0), (8, q1)] {
                        let lo = q as u32;
                        let hi = (q >> 32) as u32;
                        if lo == m_idx || hi == m_idx {
                            notes.push_str(&format!(" [MATCH m_idx={m_idx} @+{qi}]"));
                        }
                        if lo == uid || hi == uid {
                            notes.push_str(&format!(" [UID @+{qi}]"));
                        }
                        let p = q as usize;
                        if is_ptr(p) {
                            let mut pbuf = [0u8; 64];
                            if driver.read_bytes(p, &mut pbuf).is_ok() {
                                let pq0 = u64::from_ne_bytes(pbuf[0..8].try_into().unwrap());
                                // Check if utf-16 or ascii string
                                let ascii: String = pbuf.iter().take_while(|&&c| (32..=126).contains(&c)).map(|&c| c as char).collect();
                                let u16s: Vec<u16> = pbuf.chunks_exact(2).map(|c| u16::from_ne_bytes([c[0], c[1]])).take_while(|&c| (32..=126).contains(&c)).collect();
                                if ascii.len() >= 3 {
                                    notes.push_str(&format!(" ptr+{qi}->0x{p:x}(\"{ascii}\")"));
                                } else if u16s.len() >= 3 {
                                    let s16 = String::from_utf16_lossy(&u16s);
                                    notes.push_str(&format!(" ptr+{qi}->0x{p:x}(u16\"{s16}\")"));
                                } else {
                                    notes.push_str(&format!(" ptr+{qi}->0x{p:x}[q0=0x{pq0:x}]"));
                                }
                            }
                        }
                    }
                    println!("  0x{a:012x} ({rel:+05x}): {hex} | q0={q0:016x} q1={q1:016x}{notes}");
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_kind_mapping_matches_dump() {
        assert_eq!(number_kind_name(0), "SignedWhole");
        assert_eq!(number_kind_name(1), "UnsignedWhole");
        assert_eq!(number_kind_name(2), "FloatingPoint");
        assert_eq!(number_kind_name(3), "Invalid");
        assert_eq!(number_kind_name(9), "?");
    }

    #[test]
    fn link_text_decoding() {
        assert_eq!(decode_link_text("\u{1}QUJD\u{2}Igor Tolić\u{1}"), "Igor Tolić");
        assert_eq!(decode_link_text("\\u{1}QUJD\\u{2}IT\\u{1}"), "IT");
        assert_eq!(decode_link_text("Plain"), "Plain");
    }

    #[test]
    fn ident_check() {
        assert!(is_ident("NumericTypedValue"));
        assert!(!is_ident(""));
        assert!(!is_ident("bad name"));
    }
}

#[cfg(test)]
mod stage_probe_tests {
    use super::stage_identity;
    fn pairs(values: &[(u32, i32)]) -> Vec<u8> {
        values.iter().flat_map(|(key, value)| key.to_ne_bytes().into_iter().chain(value.to_ne_bytes())).collect()
    }
    #[test]
    fn stage_probe_validates_competition_and_season_without_database_offsets() {
        assert_eq!(stage_identity(&pairs(&[(0x64726566, (25 << 24) | 3295), (0x79656172, 2025), (0x73737374, -1)])), Some((3295, 2025)));
        assert_eq!(stage_identity(&pairs(&[(0x64726566, (24 << 24) | 3295), (0x79656172, 2025)])), None);
        assert_eq!(stage_identity(&pairs(&[(0x64726566, 25 << 24), (0x79656172, 2025)])), None);
        assert_eq!(stage_identity(&pairs(&[(0x64726566, (25 << 24) | 3295)])), None);
        assert_eq!(stage_identity(&pairs(&[(0x64726566, (25 << 24) | 3295), (0x79656172, 2025), (0x79656172, 2026)])), None);
        assert_eq!(stage_identity(&vec![0; 264]), None);
        assert_eq!(stage_identity(&[0; 7]), None);
    }
}
