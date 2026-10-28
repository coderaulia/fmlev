//! Read-only: search staff by name or read one staff record's 22 attributes.
//! Usage: cargo run -p fmlev-core --example staff_data -- search <text>
//!        cargo run -p fmlev-core --example staff_data -- read <id> <uid>
//!        cargo run -p fmlev-core --example staff_data -- info <id> <uid>   (Information + Contract tabs)
use fmlev_core::native_staff::NativeStaffRecord;
use fmlev_mem::{find_fm_process, LinuxMemoryDriver};

fn main() -> fmlev_core::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let pid = find_fm_process().map_err(|e| fmlev_core::CoreError::InvalidValue(e.to_string()))?.pid;
    let driver = LinuxMemoryDriver::new(pid);
    match args.first().map(String::as_str) {
        Some("search") => {
            for s in NativeStaffRecord::search(&driver, args.get(1).map(String::as_str).unwrap_or(""), 50)? {
                println!("{} {} {} {:#x}", s.index, s.uid, s.name, s.record);
            }
        }
        Some("read") => {
            let parse = |i: usize| args.get(i).and_then(|v| v.parse().ok()).unwrap_or(-1);
            let staff = NativeStaffRecord::discover_by_index(&driver, parse(1))?;
            if staff.uid != parse(2) {
                return Err(fmlev_core::CoreError::InvalidValue(format!("uid is {}", staff.uid)));
            }
            println!("{} {} {}", staff.index, staff.uid, staff.name);
            for a in staff.read_attributes(&driver)? {
                println!("{}.{} = {:?} native={:?} {}", a.group, a.field, a.value, a.native, a.reason.unwrap_or_default());
            }
        }
        Some("info") => {
            let parse = |i: usize| args.get(i).and_then(|v| v.parse().ok()).unwrap_or(-1);
            let staff = NativeStaffRecord::discover_by_index(&driver, parse(1))?;
            if staff.uid != parse(2) {
                return Err(fmlev_core::CoreError::InvalidValue(format!("uid is {}", staff.uid)));
            }
            println!("{} {} {}", staff.index, staff.uid, staff.name);
            for f in staff.read_information(&driver)? {
                println!("info.{} = {:?} [{}] {}", f.field, f.value, f.status, f.reason.unwrap_or_default());
            }
            let contract = staff.read_contract(&driver)?;
            println!("contract status={} {}", contract.status, contract.reason);
            for f in contract.fields {
                println!("contract.{} = {:?} [{}] {}", f.field, f.value, f.status, f.reason.unwrap_or_default());
            }
        }
        _ => eprintln!("usage: staff_data search <text> | read <id> <uid> | info <id> <uid>"),
    }
    Ok(())
}
