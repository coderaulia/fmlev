//! Read-only: prints the native stadium name and capacities of clubs by id.
//! Usage: cargo run --release -p fmlev-core --example stadium_read -- <club_id>...
use fmlev_core::native_club::NativeClubRecord;
use fmlev_mem::{find_fm_process, LinuxMemoryDriver};

fn main() -> fmlev_core::Result<()> {
    let pid = find_fm_process().map_err(|e| fmlev_core::CoreError::InvalidValue(e.to_string()))?.pid;
    let driver = LinuxMemoryDriver::new(pid);
    for id in std::env::args().skip(1).filter_map(|v| v.parse::<i32>().ok()) {
        let record = NativeClubRecord::discover(&driver, id)?;
        println!(
            "club {id} {:?}: stadium={:?} capacities={:?}",
            record.read_name(&driver),
            record.read_stadium_name(&driver),
            record.read_stadium_capacities(&driver).ok()
        );
    }
    Ok(())
}
