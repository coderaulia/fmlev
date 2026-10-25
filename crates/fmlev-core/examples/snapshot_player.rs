//! Read-only: writes a before/after snapshot of one player for transfer diffing.
//! Usage: snapshot_player <m_index> <out_file>   (player profile must be open)
//! Lines: REC <hex>, CON <addr> <hex>, TEAM <addr> <hex>, REF <addr> (every 8-aligned rw-private
//! slot holding the player's person pointer). Diff two files with the scratchpad `snapdiff.py`.
use std::fmt::Write as _;

use fmlev_core::{live_write::read_person_native_fingerprint, native_player::NativePlayerRecord, LiveSession};
use fmlev_il2cpp::OffsetsRegistry;
use fmlev_mem::parse_process_maps;

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, x| { let _ = write!(s, "{x:02x}"); s })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let index: i32 = args.next().ok_or("usage: snapshot_player <m_index> <out>")?.parse()?;
    let out = args.next().ok_or("usage: snapshot_player <m_index> <out>")?;
    let registry = OffsetsRegistry::load_from_json(include_str!("../../../offsets/v26.0.0.json"))?;
    let session = LiveSession::attach_auto(registry)?;
    let d = &session.driver;
    let base = session.bases.game_assembly_base.ok_or("GameAssembly unavailable")?;
    let gs = session.registry.get_singleton("GameSubsystems")?;
    let sf = fmlev_il2cpp::read_static_singleton(d, base, gs)?;
    let list = d.read_ptr(sf)?;
    let items = d.read_ptr(list + 0x10)?;
    let subsystem = d.read_ptr(items + 0x20 + 11 * 8)?;
    let f = read_person_native_fingerprint(d, subsystem, index)?;
    let rec = NativePlayerRecord::discover(d, index, f.unique_id, f.aptitudes, f.left_foot, f.right_foot)?;
    let mut text = String::new();
    let mut record = vec![0u8; 0x400];
    d.read_bytes(rec.address(), &mut record)?;
    writeln!(text, "REC {}", hex(&record))?;
    let contract = u64::from_le_bytes(record[0x1e8..0x1f0].try_into()?) as usize;
    if contract > 0x10000 {
        let mut c = vec![0u8; 0x100];
        if d.read_bytes(contract, &mut c).is_ok() {
            writeln!(text, "CON {contract:#x} {}", hex(&c))?;
            let team = u64::from_le_bytes(c[0x10..0x18].try_into()?) as usize;
            let mut t = vec![0u8; 0x200];
            if team > 0x10000 && d.read_bytes(team, &mut t).is_ok() {
                writeln!(text, "TEAM {team:#x} {}", hex(&t))?;
            }
        }
    }
    let person = rec.address() + 0x140;
    for map in parse_process_maps(d.pid())?.iter().filter(|m| m.is_rw_private()) {
        let mut start = map.start;
        while start < map.end {
            let len = (map.end - start).min(16 << 20);
            let mut buf = vec![0u8; len];
            if d.read_bytes(start, &mut buf).is_ok() {
                for i in (0..len.saturating_sub(8)).step_by(8) {
                    if usize::from_le_bytes(buf[i..i + 8].try_into()?) == person {
                        writeln!(text, "REF {:#x}", start + i)?;
                    }
                }
            }
            start += len;
        }
    }
    std::fs::write(&out, text)?;
    println!("record {:#x} contract {contract:#x} -> {out}", rec.address());
    Ok(())
}
