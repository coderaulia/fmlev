#!/usr/bin/env python3
"""Read-only nation rules report built on `fmscan entity PID nation --editor`.

Lists the nationality-rule candidates (years, gain type, dual nationality), plus
economic factor and development code, for every nation the scanner can validate,
then summarises value distributions and compares a few well-known nations with
real-world residency rules. The hints are context, not ground truth: this tool
never reports a field as "verified" because that also needs a screen comparison
and, for behaviour, a game-time proof. It writes nothing to the game process.

    nation_rules.py PID [--fmscan PATH] [--near ADDR:RADIUS] [--json]
"""
import argparse
import collections
import json
import subprocess
import sys
from pathlib import Path

# Name -> expected years. Real-world hints (and values seen on an earlier live
# read); disagreement does not disprove a field, agreement does not prove it.
HINTS = {"Spain": 10, "Italy": 10, "England": 5, "Andorra": 20,
         "San Marino": 30, "Japan": 5}
MIN_HINT_AGREEMENT = 5


def run_scan(binary, pid, near):
    cmd = [str(binary), "entity", str(pid), "nation", "--editor", "--max", "1000", "--json"]
    if near:
        cmd += ["--near", near]
    proc = subprocess.run(cmd, capture_output=True, text=True, timeout=300)
    if proc.returncode != 0:
        raise SystemExit(f"fmscan failed: {proc.stderr.strip() or proc.stdout.strip()}")
    return json.loads(proc.stdout)


def summarise(scan):
    rows = []
    for match in scan["matches"]:
        d = match["editor_data"]
        rows.append({
            "name": match["name"], "index": match["index"],
            "years": d.get("nationality_years"),
            "type_code": d.get("gain_nationality_type_native_code"),
            "dual_allowed": d.get("dual_nationality_allowed"),
            "economic_factor": d.get("economic_factor"),
            "development_code": d.get("state_of_development_native_code"),
        })
    dist = {
        "years": collections.Counter(str(r["years"]) for r in rows),
        "type_code": collections.Counter(str(r["type_code"]) for r in rows),
        "dual_nationality_allowed": collections.Counter(
            "null" if r["dual_allowed"] is None else str(r["dual_allowed"]).lower() for r in rows),
        "economic_factor": collections.Counter(str(r["economic_factor"]) for r in rows),
        "development_code": collections.Counter(str(r["development_code"]) for r in rows),
    }
    hints = {}
    by_name = {r["name"]: r for r in rows}
    for name, expected in HINTS.items():
        row = by_name.get(name)
        if row is None:
            hints[name] = {"status": "not_found", "expected": expected}
        elif row["years"] is None:
            hints[name] = {"status": "unreadable", "expected": expected}
        else:
            hints[name] = {"status": "agree" if row["years"] == expected else "disagree",
                           "expected": expected, "observed": row["years"]}
    agree = sum(1 for h in hints.values() if h["status"] == "agree")
    readable = [r["years"] for r in rows if r["years"] is not None]
    plausible = bool(readable) and len(set(readable)) > 3
    verdict = {
        "nationality_years": "probable" if agree >= MIN_HINT_AGREEMENT and plausible else "unverified",
        "gain_nationality_type_native_code": "unverified",
        "dual_nationality_allowed": "unverified",
        "economic_factor": "unverified",
        "development_code": "unverified",
        "hint_agreement": f"{agree}/{len(HINTS)}",
        "note": "'verified' needs the value compared with the in-game Nation screen "
                "and, for behaviour, a game-time proof; this report cannot grant it.",
    }
    return {"nations": len(rows), "rows": rows,
            "distribution": {k: dict(sorted(v.items())) for k, v in dist.items()},
            "hints": hints, "verdict": verdict, "read_only": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("pid")
    parser.add_argument("--fmscan", default=Path(__file__).with_name("fmscan"))
    parser.add_argument("--near")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    report = summarise(run_scan(args.fmscan, args.pid, args.near))
    if args.json:
        print(json.dumps(report, indent=2))
        return 0
    print(f"{report['nations']} nation record(s) read (read-only)")
    for r in report["rows"]:
        print(f"{r['name']:<28} years={r['years']} type={r['type_code']} "
              f"dual={r['dual_allowed']} econ={r['economic_factor']} dev={r['development_code']}")
    for key, counts in report["distribution"].items():
        print(f"distribution {key}: {counts}")
    for name, hint in report["hints"].items():
        print(f"hint {name}: {hint}")
    print("confidence:", json.dumps(report["verdict"], indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
