//! Read-only: bank loans and sponsor deals of clubs.
//! Usage: cargo run --release -p fmlev-core --example club_debts_read -- <club_id>...
//!        cargo run --release -p fmlev-core --example club_debts_read -- --sweep
//! `--sweep` walks the game's global club vector (game_plugin.dll RVA 0x4e49830, specific to PE timestamp 0x6a229e8a)
//! and reports how many clubs read cleanly; it is a survey tool, not product code.
use fmlev_core::native_club::NativeClubRecord;
use fmlev_core::native_club_loans::read_loans_at;
use fmlev_core::native_club_sponsors::read_sponsors_at;
use fmlev_mem::{find_fm_process, parse_process_maps, LinuxMemoryDriver};

fn main() -> fmlev_core::Result<()> {
    let pid = find_fm_process().map_err(|e| fmlev_core::CoreError::InvalidValue(e.to_string()))?.pid;
    let driver = LinuxMemoryDriver::new(pid);
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--sweep") {
        let base = parse_process_maps(pid)?
            .iter()
            .filter(|m| m.pathname.ends_with("game_plugin.dll"))
            .map(|m| m.start)
            .min()
            .ok_or_else(|| fmlev_core::CoreError::InvalidValue("game_plugin.dll not mapped".into()))?;
        let (begin, end) = (driver.read_ptr(base + 0x4e49830)?, driver.read_ptr(base + 0x4e49838)?);
        let (mut clubs, mut with_loans, mut loan_total, mut loan_errors) = (0usize, 0usize, 0usize, 0usize);
        let (mut with_list, mut sponsor_total, mut sponsor_errors, mut inverted) = (0usize, 0usize, 0usize, 0usize);
        for i in 0..(end - begin) / 8 {
            let club = driver.read_ptr(begin + i * 8)?;
            clubs += 1;
            match read_loans_at(&driver, club) {
                Ok(l) if !l.is_empty() => {
                    with_loans += 1;
                    loan_total += l.len();
                }
                Ok(_) => {}
                Err(e) => {
                    loan_errors += 1;
                    if loan_errors <= 3 {
                        println!("loan error club 0x{club:x}: {e}");
                    }
                }
            }
            match read_sponsors_at(&driver, club) {
                Ok(s) if s.list_present => {
                    with_list += 1;
                    sponsor_total += s.sponsors.len();
                    inverted += s.sponsors.iter().filter(|x| x.dates_inverted).count();
                }
                Ok(_) => {}
                Err(e) => {
                    sponsor_errors += 1;
                    if sponsor_errors <= 3 {
                        println!("sponsor error club 0x{club:x}: {e}");
                    }
                }
            }
        }
        println!("clubs {clubs}: with loans {with_loans} ({loan_total} loans, {loan_errors} errors); with sponsor list {with_list} ({sponsor_total} deals, {inverted} end-before-start, {sponsor_errors} errors)");
        return Ok(());
    }
    for id in args.iter().filter_map(|v| v.parse::<i32>().ok()) {
        let record = NativeClubRecord::discover(&driver, id)?;
        println!("club {id} {:?}", record.read_name(&driver));
        println!("{}", serde_json::to_string_pretty(&record.read_loans(&driver)?).unwrap());
        println!("{}", serde_json::to_string_pretty(&record.read_sponsors(&driver)?).unwrap());
    }
    Ok(())
}
