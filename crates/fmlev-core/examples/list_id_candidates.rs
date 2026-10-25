//! Read-only: lists records whose i32 at +8 is `id` and whose +0xc/+0x10 words are equal and non-zero,
//! with the vtable and structural fields, to tell record types apart. Usage: list_id_candidates <id>
use fmlev_core::LiveSession;
use fmlev_il2cpp::OffsetsRegistry;
use fmlev_mem::parse_process_maps;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let id: i32 = std::env::args().nth(1).ok_or("usage: list_id_candidates <id>")?.parse()?;
    let registry = OffsetsRegistry::load_from_json(include_str!("../../../offsets/v26.0.0.json"))?;
    let session = LiveSession::attach_auto(registry)?;
    let d = &session.driver;
    let maps = parse_process_maps(d.pid())?;
    let readonly = |p: usize| maps.iter().any(|m| m.is_readable() && !m.is_writable() && p >= m.start && p < m.end);
    for map in maps.iter().filter(|m| m.is_rw_private()) {
        let mut start = map.start;
        while start < map.end {
            let len = (map.end - start).min(16 << 20);
            let mut buf = vec![0u8; len];
            if d.read_bytes(start, &mut buf).is_ok() {
                for i in (8..len.saturating_sub(12)).step_by(8) {
                    let w = |at: usize| i32::from_le_bytes(buf[at..at + 4].try_into().unwrap());
                    if w(i) != id || w(i + 4) != w(i + 8) || w(i + 4) == 0 { continue; }
                    let rec = start + i - 8;
                    let Ok(vt) = d.read_ptr(rec) else { continue };
                    if !readonly(vt) { continue; }
                    let p = |o: usize| d.read_ptr(rec + o).unwrap_or(0);
                    println!(
                        "{rec:#x} vt={vt:#x} uid={} +0x40={:#x} +0x48={:#x} +0x50={:#x} +0xb0={:#x} rep188={:?}",
                        w(i + 4), p(0x40), p(0x48), p(0x50), p(0xb0), d.read::<u16>(rec + 0x188).ok()
                    );
                }
            }
            start += len;
        }
    }
    Ok(())
}
