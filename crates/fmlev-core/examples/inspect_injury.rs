//! Read-only: prints a player's native injury vector (player + 0x248) for a loaded save.
//! Usage: inspect_injury <m_index>
use fmlev_core::{
    live_write::read_person_native_fingerprint, native_player::NativePlayerRecord, LiveSession,
};
use fmlev_il2cpp::OffsetsRegistry;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let index: i32 = std::env::args().nth(1).ok_or("usage: inspect_injury <m_index>")?.parse()?;
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
    let a = rec.address();
    println!("record {a:#x}");
    for off in [0xc0usize, 0x100, 0x248, 0x250, 0x258] {
        println!("  +{off:#x}: {:#x}", d.read::<u64>(a + off)?);
    }
    let vec = d.read_ptr(a + 0x248)?;
    println!("injury vec ptr {vec:#x}");
    if vec > 0x10000 {
        for off in (0..0x40).step_by(8) {
            println!("  vec+{off:#x}: {:#x}", d.read::<u64>(vec + off)?);
        }
        let (start, end) = (d.read_ptr(vec + 0x18)?, d.read_ptr(vec + 0x20)?);
        println!("start {start:#x} end {end:#x} len {}", end.wrapping_sub(start));
        if end >= start && end - start <= 256 {
            for i in 0..(end - start) / 8 {
                let item = d.read_ptr(start + i * 8)?;
                println!("  item[{i}] {item:#x} +0x60={:#x}", d.read::<u64>(item + 0x60)?);
            }
        }
    }
    Ok(())
}
