//! Read-only: prints the board objectives and club status of clubs by id.
//! Usage: cargo run --release -p fmlev-core --example board_read -- <club_id>...
use fmlev_core::native_board::{read_board, read_club_status};
use fmlev_core::native_club::NativeClubRecord;
use fmlev_mem::{find_fm_process, LinuxMemoryDriver};

fn main() -> fmlev_core::Result<()> {
    let pid = find_fm_process().map_err(|e| fmlev_core::CoreError::InvalidValue(e.to_string()))?.pid;
    let driver = LinuxMemoryDriver::new(pid);
    for id in std::env::args().skip(1).filter_map(|v| v.parse::<i32>().ok()) {
        let record = NativeClubRecord::discover(&driver, id)?;
        println!("club {id} {:?}: status {:?}", record.read_name(&driver), read_club_status(&driver, &record));
        match read_board(&driver, &record) {
            Ok(report) => {
                println!("  chain {:?}", report.chain);
                for o in &report.objectives {
                    println!(
                        "  #{} {} type={} {:?} target={:?}/{:?} value={:?} imp={:?} status={} start={:?} deadline={:?} {:?}",
                        o.index,
                        o.class,
                        o.type_code,
                        o.type_label,
                        o.target_id,
                        o.target_kind_code,
                        o.value,
                        o.importance,
                        o.status_code,
                        o.start.as_ref().map(|d| &d.iso),
                        o.deadline.as_ref().map(|d| &d.iso),
                        o.notes
                    );
                }
            }
            Err(error) => println!("  board: {error}"),
        }
    }
    Ok(())
}
