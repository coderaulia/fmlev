//! Read-only: lists TEAM records (by vtable RVA) whose squad vector has spare capacity.
//! Usage: list_spare_teams [min_players]
use fmlev_core::LiveSession;
use fmlev_il2cpp::OffsetsRegistry;
use fmlev_mem::parse_process_maps;

const TEAM_VTABLE_RVA: usize = 0x44c_13a8;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let min: usize = std::env::args().nth(1).map(|v| v.parse()).transpose()?.unwrap_or(15);
    let registry = OffsetsRegistry::load_from_json(include_str!("../../../offsets/v26.0.0.json"))?;
    let session = LiveSession::attach_auto(registry)?;
    let d = &session.driver;
    let maps = parse_process_maps(d.pid())?;
    let base = maps.iter().filter(|m| m.pathname.ends_with("game_plugin.dll")).map(|m| m.start).min().ok_or("no game_plugin")?;
    let vt = base + TEAM_VTABLE_RVA;
    for map in maps.iter().filter(|m| m.is_rw_private()) {
        let mut start = map.start;
        while start < map.end {
            let len = (map.end - start).min(16 << 20);
            let mut buf = vec![0u8; len];
            if d.read_bytes(start, &mut buf).is_ok() {
                for i in (0..len.saturating_sub(0x50)).step_by(8) {
                    if usize::from_le_bytes(buf[i..i + 8].try_into().unwrap()) != vt { continue; }
                    let rec = start + i;
                    let (Ok(b), Ok(e), Ok(c)) = (d.read_ptr(rec + 0x38), d.read_ptr(rec + 0x40), d.read_ptr(rec + 0x48)) else { continue };
                    if b == 0 || e < b || c < e || (e - b) % 8 != 0 || (c - b) > 4096 * 8 { continue; }
                    let (n, spare) = ((e - b) / 8, (c - e) / 8);
                    let idx = d.read::<i32>(rec + 8).unwrap_or(0);
                    let uid = d.read::<i32>(rec + 0xc).unwrap_or(0);
                    if spare > 0 && n >= min { println!("{rec:#x} index={idx} uid={uid} players={n} spare={spare}"); }
                }
            }
            start += len;
        }
    }
    Ok(())
}
