//! Native layout diagnostic. Read-only unless --round-trip is explicitly supplied.
use fmlev_core::{
    live_write::read_person_native_fingerprint, native_player::NativePlayerRecord, LiveSession,
};
use fmlev_il2cpp::OffsetsRegistry;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let index: i32 = std::env::args()
        .nth(1)
        .ok_or("usage: verify_native_positions <m_index>")?
        .parse()?;
    let registry = OffsetsRegistry::load_from_json(include_str!("../../../offsets/v26.0.0.json"))?;
    let session = LiveSession::attach_auto(registry)?;
    let base = session
        .bases
        .game_assembly_base
        .ok_or("GameAssembly unavailable")?;
    let gs = session.registry.get_singleton("GameSubsystems")?;
    let sf = fmlev_il2cpp::read_static_singleton(&session.driver, base, gs)?;
    let list = session.driver.read_ptr(sf)?;
    let items = session.driver.read_ptr(list + 0x10)?;
    if session.driver.read::<i32>(list + 0x18)? <= 11 {
        return Err("binding subsystem unavailable".into());
    }
    let subsystem = session.driver.read_ptr(items + 0x20 + 11 * 8)?;
    let fingerprint = read_person_native_fingerprint(&session.driver, subsystem, index)?;
    println!(
        "PID {} index {} bindings {:?}",
        session.driver.pid(),
        index,
        fingerprint
    );
    let record = NativePlayerRecord::discover(
        &session.driver,
        index,
        fingerprint.unique_id,
        fingerprint.aptitudes,
        fingerprint.left_foot,
        fingerprint.right_foot,
    )?;
    println!("Verified record: {:?}", record);
    let original = record.read_positions(&session.driver)?;
    println!("Authoritative positions: {:?}", original);
    if std::env::args().nth(2).as_deref() == Some("--context-round-trip") {
        let mut manager = fmlev_core::GameContextManager::new();
        manager.sync_live_context()?;
        manager.select_player(index)?;
        let mut edited = manager.get_context().player;
        edited.positions = original.clone();
        edited.positions.left_foot = if original.left_foot == 20 {
            19
        } else {
            original.left_foot + 1
        };
        edited.positions.dr = if original.dr == 20 {
            19
        } else {
            original.dr + 1
        };
        let expected = edited.positions.clone();
        let result = (|| -> fmlev_core::Result<_> {
            manager.update_player(edited)?;
            manager.sync_live_context()?;
            Ok(manager.get_context().player.positions)
        })();
        // Restore through the native handle even if the context save failed.
        record.write_positions(&session.driver, &original)?;
        if result? != expected || record.read_positions(&session.driver)? != original {
            return Err("context round-trip mismatch".into());
        }
        println!("PASS context save/native readback/poll/restore; all original positions and feet restored");
    }
    if std::env::args().nth(2).as_deref() == Some("--round-trip") {
        let mut edited = original.clone();
        edited.left_foot = if original.left_foot == 20 {
            19
        } else {
            original.left_foot + 1
        };
        let result = record
            .write_positions(&session.driver, &edited)
            .and_then(|_| record.read_positions(&session.driver));
        // Always restore before reporting the test result, including a failed edit.
        record.write_positions(&session.driver, &original)?;
        let restored = record.read_positions(&session.driver)?;
        if result? != edited || restored != original {
            return Err("round-trip mismatch".into());
        }
        println!("PASS native edit/readback/restore; all original positions and feet restored");
    }
    Ok(())
}
