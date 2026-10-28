//! Read-only: builds the search index for a running FM26 and prints counts, timings and samples to
//! compare with `tools/fmscan/fmscan entity PID <kind> --name <name>` (plan G6 parity check).
//! Usage: cargo run --release -p fmlev-core --example search_index_parity -- PID [SAMPLES] [QUERY]
use std::time::Instant;

use fmlev_core::native_catalog::enumerate_process;
use fmlev_core::search_index::{EntityKind, SearchIndex};
use fmlev_mem::LinuxMemoryDriver;

fn main() -> fmlev_core::Result<()> {
    let mut args = std::env::args().skip(1);
    let pid: i32 = args.next().and_then(|p| p.parse().ok()).expect("usage: search_index_parity PID [SAMPLES] [QUERY]");
    let samples: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(20);
    let query = args.next();
    let driver = LinuxMemoryDriver::new(pid);

    let started = Instant::now();
    let found = enumerate_process(&driver)?;
    let build_ms = started.elapsed().as_millis() as u64;
    println!("candidates={} duplicates_dropped={} build_ms={build_ms}", found.candidates, found.duplicates_dropped);
    let index = SearchIndex::new(pid, found.entries, found.duplicates_dropped, build_ms);
    for (kind, count) in index.counts() {
        println!("count {kind}={count}");
    }
    for kind in EntityKind::ALL {
        let entries = index.entries(kind);
        let step = (entries.len() / samples.max(1)).max(1);
        for entry in entries.iter().step_by(step).take(samples) {
            println!(
                "sample {} index={} uid={} name={:?} detail={:?} address={:#x}",
                kind.plural(), entry.id, entry.uid, entry.name, entry.detail, entry.address
            );
        }
    }
    if let Some(query) = query {
        let mut times = Vec::new();
        for _ in 0..50 {
            let t = Instant::now();
            std::hint::black_box(index.query(&query, None, 25, 0));
            times.push(t.elapsed().as_micros());
        }
        times.sort_unstable();
        println!("query {query:?} p50_us={} p95_us={}", times[times.len() / 2], times[times.len() * 95 / 100]);
        for (kind, page) in index.query(&query, None, 5, 0) {
            for hit in page.hits {
                println!("hit {} total={} index={} name={:?}", kind.plural(), page.total, hit.id, hit.name);
            }
        }
    }
    Ok(())
}
