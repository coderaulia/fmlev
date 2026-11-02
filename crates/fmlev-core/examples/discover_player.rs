//! Read-only: discover a player record by database index and report the result or the reason.
//! cargo run --release -p fmlev-core --example discover_player -- PID INDEX
use fmlev_core::native_player::NativePlayerRecord;
use fmlev_mem::LinuxMemoryDriver;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (pid, index): (i32, i32) = (args[1].parse().expect("PID"), args[2].parse().expect("INDEX"));
    let driver = LinuxMemoryDriver::new(pid);
    match NativePlayerRecord::discover_by_index(&driver, index) {
        Ok(mut record) => {
            println!("verify_native_attributes: {:?}", record.verify_native_attributes(&driver));
            println!("found at 0x{:x}, attributes_verified={}, name={:?}", record.address(), record.attributes_verified(), record.name(&driver));
        }
        Err(error) => println!("not found: {error}"),
    }
}
