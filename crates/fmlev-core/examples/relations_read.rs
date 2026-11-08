//! Read-only: list a player's relationship records and homegrown summary.
//! cargo run --release -p fmlev-core --example relations_read -- PID PLAYER_INDEX
use fmlev_core::native_catalog::RttiCatalog;
use fmlev_core::native_player::NativePlayerRecord;
use fmlev_core::native_relationships as rel;
use fmlev_mem::{parse_process_maps, LinuxMemoryDriver};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (pid, index): (i32, i32) = (args[1].parse().expect("PID"), args[2].parse().expect("INDEX"));
    let driver = LinuxMemoryDriver::new(pid);
    let started = std::time::Instant::now();
    let catalog = RttiCatalog::discover(&driver, &parse_process_maps(pid).unwrap()).expect("rtti");
    println!("rtti catalog: {} vtables in {:?}", catalog.tables.len(), started.elapsed());
    let record = NativePlayerRecord::discover_by_index(&driver, index).expect("player");
    let address = record.address();
    println!("record 0x{address:x}");
    match rel::read_relations(&driver, &catalog, address) {
        Ok(list) => {
            println!("block 0x{:x} begin 0x{:x} end 0x{:x} cap 0x{:x} count {} spare {}", list.block, list.begin, list.end, list.cap, list.records.len(), list.spare_slots());
            for r in &list.records {
                println!("  cat {} sub {} type {} value {} tail {:02x?} target 0x{:x}", r.category, r.sub_context, r.kind, r.value, r.tail, r.target);
            }
        }
        Err(error) => println!("relations: {error}"),
    }
    println!("{:#?}", rel::homegrown_summary(&driver, &catalog, address));
    let club = rel::current_club(&driver, &catalog, address);
    let nation = rel::first_nationality(&driver, &catalog, address);
    println!("current club {club:x?} nation {nation:x?}");
}
