//! Read-only: scans FM26 memory for 4-byte unique ID hits and prints context around each.
//! Usage: probe_native_record <unique_id> [index]
use fmlev_core::LiveSession;
use fmlev_il2cpp::OffsetsRegistry;
use fmlev_mem::parse_process_maps;

fn main() -> fmlev_core::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let uid: u32 = args.get(1).expect("unique_id").parse().expect("number");
    let index: Option<i32> = args.get(2).map(|v| v.parse().expect("number"));
    let registry = OffsetsRegistry::load_from_json(include_str!("../../../offsets/v26.0.0.json"))?;
    let session = LiveSession::attach_auto(registry)?;
    let d = &session.driver;
    let maps = parse_process_maps(d.pid())?;
    let readonly = |p: usize| maps.iter().any(|m| m.is_readable() && !m.is_writable() && p >= m.start && p.checked_add(8).is_some_and(|e| e <= m.end));
    let hits = d.scan_value::<u32>(uid)?;
    println!("uid {uid}: {} raw hits", hits.len());
    let mut shown = 0;
    for hit in hits {
        if hit % 4 != 0 || !maps.iter().any(|m| m.is_rw_private() && hit >= m.start && hit + 4 <= m.end) {
            continue;
        }
        let near_index = index.map(|i| {
            (1..=4usize).any(|k| d.read::<i32>(hit - 4 * k).ok() == Some(i) || d.read::<i32>(hit + 4 * k).ok() == Some(i))
        });
        if index.is_some() && near_index != Some(true) {
            continue;
        }
        // Look backwards for a vtable-like pointer (read-only target) in 8-aligned slots.
        let start = hit.saturating_sub(0x100) & !7;
        let vt = (0..0x20).map(|k| start + k * 8).find(|a| d.read_ptr(*a).is_ok_and(&readonly));
        println!("--- hit {hit:#x} first_vtable_slot={:?}", vt.map(|a| format!("{a:#x} (hit-{:#x})", hit - a)));
        let from = hit.saturating_sub(0x40) & !7;
        let mut buf = vec![0u8; 0x100];
        if d.read_bytes(from, &mut buf).is_err() { continue; }
        for (row, chunk) in buf.chunks(16).enumerate() {
            let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
            println!("{:#x} {:+}: {}", from + row * 16, (from + row * 16).wrapping_sub(hit) as isize, hex.join(" "));
        }
        shown += 1;
        if shown >= 6 { break; }
    }
    Ok(())
}
