//! Read-only: prints every bound record-reference property currently visible in FM26.
//! Usage: cargo run -p fmlev-core --example dump_live_bindings [> out.txt]
use fmlev_core::{live_dump::dump_bound_properties, LiveSession};
use fmlev_il2cpp::OffsetsRegistry;

fn main() -> fmlev_core::Result<()> {
    let registry = OffsetsRegistry::load_from_json(include_str!("../../../offsets/v26.0.0.json"))?;
    let session = LiveSession::attach_auto(registry)?;
    let base = session.bases.game_assembly_base.expect("GameAssembly base not found");
    let singleton = session.registry.get_singleton("GameSubsystems")?;
    let gs = fmlev_il2cpp::read_static_singleton(&session.driver, base, singleton)?;
    let list = session.driver.read_ptr(gs)?;
    let items = session.driver.read_ptr(list + 0x10)?;
    let binding = session.driver.read_ptr(items + 0x20 + 11 * 8)?;
    for p in dump_bound_properties(&session.driver, binding) {
        println!(
            "{}({}) id={} | {} | {} | {}",
            p.reference_class, p.reference_type, p.reference_id, p.property, p.value_class, p.value
        );
    }
    Ok(())
}
