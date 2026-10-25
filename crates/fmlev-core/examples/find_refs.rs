//! Read-only: lists every 8-aligned location in rw-private memory holding a given pointer value.
//! Usage: find_refs <hex_address> [near_hex]   (with near: only hits with that value within +-0x80 bytes)
use fmlev_core::LiveSession;
use fmlev_il2cpp::OffsetsRegistry;
use fmlev_mem::parse_process_maps;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target = usize::from_str_radix(std::env::args().nth(1).ok_or("usage: find_refs <hex>")?.trim_start_matches("0x"), 16)?;
    let near = std::env::args().nth(2).map(|n| usize::from_str_radix(n.trim_start_matches("0x"), 16)).transpose()?;
    let registry = OffsetsRegistry::load_from_json(include_str!("../../../offsets/v26.0.0.json"))?;
    let session = LiveSession::attach_auto(registry)?;
    let d = &session.driver;
    for map in parse_process_maps(d.pid())?.iter().filter(|m| m.is_rw_private()) {
        let mut start = map.start;
        while start < map.end {
            let len = (map.end - start).min(16 << 20);
            let mut buf = vec![0u8; len];
            if d.read_bytes(start, &mut buf).is_ok() {
                for i in (0..len.saturating_sub(8)).step_by(8) {
                    if usize::from_le_bytes(buf[i..i + 8].try_into().unwrap()) == target {
                        let at = start + i;
                        if let Some(n) = near {
                            let lo = i.saturating_sub(0x80);
                            let hi = (i + 0x80).min(len.saturating_sub(8));
                            if !(lo..=hi).step_by(8).any(|j| j != i && usize::from_le_bytes(buf[j..j + 8].try_into().unwrap()) & 0xffff_ffff_ffff == n) { continue; }
                        }
                        let mut head = [0u8; 0x20];
                        let _ = d.read_bytes(at.saturating_sub(0x10), &mut head);
                        let words: Vec<String> = head.chunks(8).map(|w| format!("{:016x}", u64::from_le_bytes(w.try_into().unwrap()))).collect();
                        println!("{at:#x}: {}", words.join(" "));
                    }
                }
            }
            start += len;
        }
    }
    Ok(())
}
