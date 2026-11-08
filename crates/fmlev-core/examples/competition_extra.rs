//! Read-only: competition extras, native prize table, league table and awards.
//! Usage:
//!   competition_extra comp <index> <uid>     extras, native prizes and league tables
//!   competition_extra award <query>          award search (first 10)
//!   competition_extra award-info <index> <uid>
use fmlev_core::native_award;
use fmlev_core::native_competition_extra as extra;
use fmlev_mem::{find_fm_process, LinuxMemoryDriver};

fn main() -> fmlev_core::Result<()> {
    let pid = find_fm_process().map_err(|e| fmlev_core::CoreError::InvalidValue(e.to_string()))?.pid;
    let driver = LinuxMemoryDriver::new(pid);
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("comp") => {
            let (index, uid) = (args[1].parse().unwrap(), args[2].parse().unwrap());
            let record = extra::locate_competition(&driver, index, uid)?;
            println!("{}", serde_json::to_string_pretty(&extra::read_extras(&driver, &record)?).unwrap());
            match extra::read_native_prizes(&driver, &record) {
                Ok(tables) => println!("{}", serde_json::to_string_pretty(&tables).unwrap()),
                Err(e) => println!("prizes unavailable: {e}"),
            }
            match extra::read_league_tables(&driver, &record) {
                Ok(tables) => println!("{}", serde_json::to_string_pretty(&tables).unwrap()),
                Err(e) => println!("table unavailable: {e}"),
            }
        }
        Some("award") => {
            let start = std::time::Instant::now();
            let (total, page) = native_award::search_awards(&driver, args.get(1).map_or("", String::as_str), 0, 10)?;
            println!("{total} matches in {:?}\n{}", start.elapsed(), serde_json::to_string_pretty(&page).unwrap());
        }
        Some("award-info") => {
            let address = native_award::locate_award(&driver, args[1].parse().unwrap(), args[2].parse().unwrap())?;
            println!("{}", serde_json::to_string_pretty(&native_award::read_award(&driver, address)?).unwrap());
        }
        _ => eprintln!("usage: competition_extra comp <index> <uid> | award <query> | award-info <index> <uid>"),
    }
    Ok(())
}
