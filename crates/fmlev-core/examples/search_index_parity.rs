//! Read-only: builds the search index for a running FM26 and prints counts, timings and samples to
//! compare with `tools/fmscan/fmscan entity PID <kind> --name <name>` (plan G6 parity check).
//! Usage: cargo run --release -p fmlev-core --example search_index_parity -- PID [SAMPLES] [QUERY]
use std::time::Instant;

use fmlev_core::native_catalog::{enumerate, enumerate_process, RttiCatalog};
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
    println!(
        "candidates={} duplicates_dropped={} build_ms={build_ms} list_scoped={}",
        found.candidates, found.duplicates_dropped, found.list_scoped
    );
    // Parity of the list-scoped pass against a full scan of all writable memory.
    let maps = fmlev_mem::parse_process_maps(pid)?;
    let catalog = RttiCatalog::discover(&driver, &maps)?;
    let t = Instant::now();
    let full = enumerate(&driver, &catalog, &maps);
    println!("full_scan_ms={} full_entries={} scoped_entries={}", t.elapsed().as_millis(), full.entries.len(), found.entries.len());
    let key = |e: &fmlev_core::search_index::IndexEntry| (e.kind.plural(), e.id, e.uid, e.address);
    let scoped: std::collections::HashSet<_> = found.entries.iter().map(key).collect();
    let all: std::collections::HashSet<_> = full.entries.iter().map(key).collect();
    for (label, a, b) in [("only_in_full", &all, &scoped), ("only_in_scoped", &scoped, &all)] {
        let diff: Vec<_> = a.difference(b).collect();
        println!("{label}={} {:?}", diff.len(), diff.iter().take(5).collect::<Vec<_>>());
    }
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
                println!("hit {} total={} {}", kind.plural(), page.total, serde_json::to_string(&hit).unwrap_or_default());
            }
        }
    }
    Ok(())
}
