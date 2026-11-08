//! Read-only: prints the native embargo block and financial statements of clubs by id.
//! Usage: cargo run --release -p fmlev-core --example club_finance_read -- <club_id>...
use fmlev_core::native_club::NativeClubRecord;
use fmlev_core::native_club_finance::{read_embargo, read_statements};
use fmlev_mem::{find_fm_process, LinuxMemoryDriver};

fn main() -> fmlev_core::Result<()> {
    let pid = find_fm_process().map_err(|e| fmlev_core::CoreError::InvalidValue(e.to_string()))?.pid;
    let driver = LinuxMemoryDriver::new(pid);
    for id in std::env::args().skip(1).filter_map(|v| v.parse::<i32>().ok()) {
        let record = NativeClubRecord::discover(&driver, id)?;
        println!("club {id} {:?}", record.read_name(&driver));
        println!("  embargo: {:?}", read_embargo(&driver, &record));
        match read_statements(&driver, &record) {
            Ok(statements) => {
                for period in statements.periods {
                    println!("  {} available={} all_zero={} reason={:?}", period.key, period.available, period.all_zero, period.reason);
                    let join = |fields: &[fmlev_core::native_club_finance::StatementField]| {
                        fields.iter().filter(|f| f.value != 0).map(|f| format!("{}={}", f.key, f.value)).collect::<Vec<_>>().join(" ")
                    };
                    println!("    income: {}", join(&period.income));
                    println!("    expenditure: {}", join(&period.expenditure));
                    println!("    unmapped income {:?} expenditure {:?}", period.unmapped_income, period.unmapped_expenditure);
                }
            }
            Err(error) => println!("  statements error: {error}"),
        }
    }
    Ok(())
}
