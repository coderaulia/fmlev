//! Read-only: lists FULL_TRANSFER_OFFER deals of the running game.
//! Usage: cargo run --release -p fmlev-core --example deals_read -- [club_id] [player_name_part]
use fmlev_core::native_deals::scan_process;
use fmlev_mem::{find_fm_process, LinuxMemoryDriver};

fn main() -> fmlev_core::Result<()> {
    let pid = find_fm_process().map_err(|e| fmlev_core::CoreError::InvalidValue(e.to_string()))?.pid;
    let driver = LinuxMemoryDriver::new(pid);
    let started = std::time::Instant::now();
    let scan = scan_process(&driver)?;
    println!("pid {pid}: {} candidates, {} deals, {} rejected, {:?}", scan.candidates, scan.deals.len(), scan.rejected, started.elapsed());
    println!("rejected: {:?}", scan.rejected_reasons);
    let mut args = std::env::args().skip(1);
    let club = args.next().and_then(|v| v.parse::<i32>().ok());
    let name = args.next().map(|v| v.to_lowercase());
    for deal in scan.deals.iter().filter(|d| club.is_none_or(|c| d.involves_club(c))).filter(|d| name.as_ref().is_none_or(|n| d.player.name.as_deref().is_some_and(|p| p.to_lowercase().contains(n)))) {
        println!("{}", serde_json::to_string(deal).unwrap());
    }
    Ok(())
}
