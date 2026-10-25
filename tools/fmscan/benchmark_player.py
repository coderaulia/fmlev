"""Compare matching optimized native read workloads, excluding compilation.

Build fmscan and cargo build --release -p fmlev-core --example player_data first.
The common payload omits UI-only/unknown fields and RTTI catalog/name discovery.
"""
import argparse
import json
from pathlib import Path
import platform
import statistics
import subprocess
import time

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]


def summarize(samples):
    return {"median_ms": statistics.median(samples) / 1e6,
            "min_ms": min(samples) / 1e6, "max_ms": max(samples) / 1e6}


def run(args):
    # Build before any timers; use the default target CPU and optimization level 3 in both.
    subprocess.run(["g++", "-O3", "-std=c++20", "-Wall", "-Wextra", "-Wpedantic", "-Werror",
                    "-o", str(HERE / "fmscan"), str(HERE / "fmscan.cpp")], check=True)
    subprocess.run(["cargo", "build", "--release", "-p", "fmlev-core", "--example", "player_data"],
                   check=True, cwd=REPO, capture_output=True)
    commands = {"cpp": [str(HERE / "fmscan"), "player-data"],
                "rust": [str(REPO / "target/release/examples/player_data")]}
    expected = None
    expected_checksum = None

    def measure(language, mode):
        nonlocal expected, expected_checksum
        argv = commands[language] + [str(args.pid), str(args.index), str(args.uid), "--json"]
        if mode == "located_read":
            argv += ["--record", args.record, "--iterations", str(args.iterations)]
        started = time.perf_counter_ns()
        proc = subprocess.run(argv, capture_output=True, text=True, check=True, timeout=60)
        wall = time.perf_counter_ns() - started
        result = json.loads(proc.stdout)
        if expected is None:
            expected = result["player"]
            expected_checksum = result["checksum"] // result["iterations"]
        if result["player"] != expected:
            raise RuntimeError("payload mismatch or player changed: benchmark invalid")
        if result["checksum"] != expected_checksum * result["iterations"]:
            raise RuntimeError("checksum mismatch or player changed during a repeated-read loop")
        return {"language": language, "mode": mode, "wall_ns": wall,
                **{k: result[k] for k in ["scan_ns", "read_ns", "iterations", "checksum", "scanned_bytes", "unreadable_chunks"]}}

    rows = []
    for mode in ["full_identity_scan", "located_read"]:
        # One excluded warm-up per binary and mode; these are not cold-cache measurements.
        for language in commands:
            measure(language, mode)
        for trial in range(args.trials):
            for language in (["cpp", "rust"] if trial % 2 == 0 else ["rust", "cpp"]):
                rows.append({"trial": trial, **measure(language, mode)})
    summary = {}
    for mode in ["full_identity_scan", "located_read"]:
        summary[mode] = {}
        for language in commands:
            samples = [r for r in rows if r["mode"] == mode and r["language"] == language]
            summary[mode][language] = {
                "process_wall": summarize([r["wall_ns"] for r in samples]),
                "scan": summarize([r["scan_ns"] for r in samples]),
                "per_player_read": summarize([r["read_ns"] / r["iterations"] for r in samples])}
        key = "scan" if mode == "full_identity_scan" else "per_player_read"
        cpp = summary[mode]["cpp"][key]["median_ms"]
        rust = summary[mode]["rust"][key]["median_ms"]
        summary[mode]["faster"] = "cpp" if cpp < rust else "rust"
        summary[mode]["speed_ratio"] = max(cpp, rust) / min(cpp, rust)
    scan_work = {(r["scanned_bytes"], r["unreadable_chunks"]) for r in rows if r["mode"] == "full_identity_scan"}
    if len(scan_work) != 1:
        raise RuntimeError("memory map/readability changed between scans: rerun for a matched workload")
    cpu = next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines() if line.startswith("model name")), "unknown")
    report = {"pid": args.pid, "index": args.index, "uid": args.uid, "record": args.record,
              "trials": args.trials, "iterations_per_located_trial": args.iterations,
              "environment": {"platform": platform.platform(), "cpu": cpu,
                              "builds": "g++ -O3 -std=c++20; cargo build --release (opt-level=3); compilation excluded",
                              "cpp": subprocess.check_output(["g++", "--version"], text=True).splitlines()[0],
                              "rust": subprocess.check_output(["rustc", "--version"], text=True).strip()},
              "method": "Alternating optimized binaries, one excluded warmup each; same 1 MiB anonymous/heap scan and 512-byte player/96-byte contract snapshots; serialization outside internal timers. Located read excludes scan and startup. No writes/UI navigation.",
              "limits": "Compares these implementations on this running save, not intrinsic language speed. Identity selectors are supplied after native name discovery. RTTI/name catalog discovery, HTTP and UI rendering are not timed. Cache-warm repeat runs, not cold process memory.",
              "summary": summary, "raw_trials": rows, "player": expected}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pid", type=int, required=True)
    parser.add_argument("--index", type=int, required=True)
    parser.add_argument("--uid", type=int, required=True)
    parser.add_argument("--record", required=True)
    parser.add_argument("--trials", type=int, default=7)
    parser.add_argument("--iterations", type=int, default=1000)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.trials < 3 or not 1 <= args.iterations <= 1_000_000:
        parser.error("trials >= 3 and iterations 1..1000000 required")
    run(args)
