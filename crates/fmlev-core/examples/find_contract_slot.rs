//! Read-only: shows where `find_zero_contract_slot` would put a new contract in the running game.
//! Usage: cargo run --release -p fmlev-core --example find_contract_slot
use fmlev_core::native_transfer::{find_zero_contract_slot, CONTRACT_SIZE};
use fmlev_mem::{find_fm_process, LinuxMemoryDriver};

fn main() -> fmlev_core::Result<()> {
    let pid = find_fm_process().map_err(|e| fmlev_core::CoreError::InvalidValue(e.to_string()))?.pid;
    let driver = LinuxMemoryDriver::new(pid);
    let started = std::time::Instant::now();
    let slot = find_zero_contract_slot(&driver, 0)?;
    let mut block = vec![0u8; CONTRACT_SIZE];
    driver.read_bytes(slot, &mut block)?;
    println!("slot={slot:#x} all_zero={} took_ms={}", block.iter().all(|&b| b == 0), started.elapsed().as_millis());
    Ok(())
}
