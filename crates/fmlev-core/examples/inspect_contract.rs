//! Read-only: dumps a player's native contract record (+0x00..+0x100) and the objects it points to.
//! Usage: inspect_contract <m_index>   (run on own player, other-club player, loan-in, loan-out; then diff)
use fmlev_core::{
    live_write::read_person_native_fingerprint, native_player::NativePlayerRecord, LiveSession,
};
use fmlev_il2cpp::OffsetsRegistry;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let index: i32 = std::env::args().nth(1).ok_or("usage: inspect_contract <m_index>")?.parse()?;
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
    let (address, bytes) = rec.contract_dump(d, 0x100)?;
    println!("player index {index} record {:#x} contract {address:#x}", rec.address());
    for (i, row) in bytes.chunks(16).enumerate() {
        let words: Vec<String> = row.chunks(4).map(|w| format!("{:08x}", u32::from_le_bytes(w.try_into().unwrap()))).collect();
        println!("  +{:#04x}: {}", i * 16, words.join(" "));
    }
    // Follow every 8-aligned pointer-looking value one level: print the pointee's first 0x30 bytes.
    for off in (0..0x100).step_by(8) {
        let v = u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap()) as usize;
        if v > 0x10000 && v < 0x7fff_ffff_ffff {
            let mut head = [0u8; 0x30];
            if d.read_bytes(v, &mut head).is_ok() {
                let words: Vec<String> = head.chunks(8).map(|w| format!("{:016x}", u64::from_le_bytes(w.try_into().unwrap()))).collect();
                println!("  ptr +{off:#04x} -> {v:#x}: {}", words.join(" "));
            }
        }
    }
    // Other contract-like objects reachable from the player record (same vtable as the main contract).
    let vtable = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    let mut record = vec![0u8; 0x400];
    d.read_bytes(rec.address(), &mut record)?;
    for off in (0..0x400).step_by(8) {
        let v = u64::from_le_bytes(record[off..off + 8].try_into().unwrap()) as usize;
        if v > 0x10000 && v < 0x7fff_ffff_ffff && d.read::<u64>(v).ok() == Some(vtable) {
            println!("  record+{off:#x} -> contract-like object {v:#x}{}", if v == address { " (main)" } else { "" });
        }
    }
    // Team-like objects (same vtable as the main contract's team) referenced from the player record.
    let team = d.read_ptr(address + 0x10)?;
    let team_vtable = d.read::<u64>(team)?;
    for off in (0..0x400).step_by(8) {
        let v = u64::from_le_bytes(record[off..off + 8].try_into().unwrap()) as usize;
        if v > 0x10000 && v < 0x7fff_ffff_ffff && d.read::<u64>(v).ok() == Some(team_vtable) {
            println!("  record+{off:#x} -> team-like {v:#x} index {:#x}", d.read::<i32>(v + 8).unwrap_or(-1));
        }
    }
    Ok(())
}
