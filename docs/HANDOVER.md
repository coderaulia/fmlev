# FMLE Handover

## State (original baseline HEAD 92c1093; C++ tooling completed below)
Verified in game, shipped: club balance/transfer budget, squad numbers (swap), tactical familiarity max, register-anytime (manager flag + domestic stage entry+0x9a), competition prizes, contract extend. Details: `AGENTS.md` Phase 3.

Not implemented (endpoints return 501):
- `POST /api/player/contract/terminate` — expiry=today does not release; FM26 releases via scheduled events.
- `POST /api/transfer/sign`, `/api/transfer/loan` — game allocates contract (+0x1e8) and loan-agreement (+0x1f0 -> node -> agreement) objects; memory writes cannot create them.
- Squad-number-anytime gate (bit 0x200 of dword at manager X+0x404) not traced.
- Never edit a player's injury vector (player+0x248): crashes FM26.

## C++ tooling plan (`tools/fmscan/`, read-only, Rust stays the main tool)
Implemented on 2026-10-25: all seven tooling items below. Usage, JSON behavior,
heuristic limits and synthetic/live validation: `tools/fmscan/README.md`.
Build remains `g++ -O2 -std=c++20 -o fmscan fmscan.cpp` from `tools/fmscan/`;
focused headers keep every source file below 1,500 lines. Run the C++ process tests
with `python3 -m unittest discover -s tools/fmscan/tests -v`.

Existing: `fmscan snap|diff|refs|find|dump` (build: `g++ -O2 -std=c++20 -o fmscan fmscan.cpp`; uses process_vm_readv, rw-private regions, 8-byte words, `--exclude` noise file, `--to32`).

Goal: faster discovery loop than Rust examples; Rust only receives verified offsets.

1. **`fmscan watch <pid> <hex_addr> <len> [--ms N]`** — poll a range, print changed words live (see what a day tick / UI action touches).
2. **`fmscan struct <pid> <hex_addr> [--depth N]`** — dump record at +0..+0x400 annotating each qword: heap pointer (+ its first qword/vtable), small int, float, date-shaped u32 (year 2000..2200 in high 16). Follow pointers to depth N with cycle guard.
3. **`fmscan rtti <pid> <hex_vtable|addr>`** — resolve MSVC RTTI type name from vtable (needs module base from /proc/pid/maps of game_plugin.dll) so unknown objects are named.
4. **`fmscan vec <pid> <hex_addr>`** — decode begin/end/cap triples, element count and stride guess, dump elements.
5. **`fmscan graph <pid> <hex_obj> --in/--out`** — inbound refs (generalise `refs`, grouped by owning region) and outbound pointers; used to find which lists contain a person/contract (needed for sign/loan list membership).
6. **`fmscan diffset`** — chain snap/noise/diff in one command (`--action-wait`) and emit a ranked candidate list (address, old->new, nearest vtable).
7. **JSON output (`--json`)** for all commands so Rust examples/tests can consume results.
8. Safety rule: C++ tool never writes; any write experiment stays in Rust (`live_write.rs`, readback + rollback).

Suggested order: 2 -> 4 -> 3 -> 5 (unblocks sign/loan), then 1, 6, 7.

Read-only live smoke proof: manager-list vector decoded successfully and manager
vtable resolved to `.?AVHUMAN_NON_PLAYER@db@@`. Proton maps PE sections anonymously;
the RTTI resolver validates the named PE header at the COL-derived image base.
Sign/loan/terminate investigations below remain pending; tools do not prove safe writes.

### Screen-independent named discovery (2026-10-25)
`fmscan entity PID player|club|nation|competition --name TEXT [--related] --json`
now discovers native RTTI types from the DLL and scans loaded records without UI
bindings, an open profile, a known object address or prior editor calibration.
`--id` and `--uid` selectors are also supported. Expected type, duplicate UIDs,
player person/attribute layout and contract ownership are validated. Related links
carry pointer offsets; missing or invalid targets are explicitly unavailable.

Live proof: Lamine Yamal Nasraoui (index 40629, UID 2000256231) -> native parent
contract -> team 1279 -> Barcelona (1074/1708) -> Spain (170/796), plus team ->
Spanish First Division (51/67) -> the same Spain record. Independent full club,
nation and competition searches agreed. No game screen was opened and no target
memory was written. The native player scan validated 39,489 records, one matching
name, with 20 unreadable chunks reported. This is not an exhaustive save census.
See `tools/fmscan/README.md` for command semantics and version/loan limitations.

### Full supported player editor dump and benchmark (2026-10-25)
`entity ... player --editor` prints name/IDs, 60 attributes, positions/feet,
CA/PA, nationality, parent-contract club, competition, wage/expiry and squad number.
Unsupported UI fields are explicitly unavailable; no game screen is required.
`--json` exports the same supported data. Live Yamal CA/PA 175/194, weekly wage
GBP 280867, expiry 2031, number 10; all common C++/Rust payload fields matched.

`player-data PID INDEX UID [--record HEX] [--iterations N]` is the common read-only
identity-scan/read diagnostic. Rust counterpart: example `player_data`; it uses
the real memory driver and production attribute decoder without changing writer
calibration. `tools/fmscan/benchmark_player.py` builds both at optimization level 3,
alternates seven trials, excludes compilation/warmups, verifies payload/checksum
and scan-work equality, and reports wall and internal times separately.
Live medians: internal full scan C++ 919.67 ms / Rust 799.39 ms; known-record read
C++ 8.53 us / Rust 9.67 us. This compares matched diagnostic implementations, not
the HTTP UI refresh path or RTTI/name discovery, and is not a universal language claim.
Verification: 21 C++/Rust process tests plus the required workspace/UI trio pass.

## Next-session targets
Native nonplayer editor reads were expanded on 2026-10-25: `entity ... --editor`
now supports club, nation and competition. See `docs/NATIVE_ENTITY_READS.md` for
field coverage, native-code evidence, live values and remaining WIP. Key findings:
club facilities live in separate details/finance records; nation youth/reputation
are gender-specific; main stadium is a first-team link, distinct from the training
ground; competition reputation is 1..200 and its stage gate is not all registration
policy. New numeric layouts require PE timestamp 0x6a229e8a plus object guards.
43 combined process tests and the Rust/UI verification trio pass. No live writes
or game-screen navigation occurred, and active competition rule/prize-source
mapping remains unresolved.

1. Sign/loan: with `struct`/`graph`, snapshot a free-agent signing and a loan-in; list every object that gains a reference (club player list, team list, contract, loan node). Test pointer-swap of an existing contract between teams on a throwaway save.
2. Terminate: find the scheduled-event queue entry for contract release; or find the release routine in game_plugin.dll.
3. Squad-number-anytime gate (0x200).

## Rules
Max 1,500 LOC/file; verification trio from `AGENTS.md`; commit locally, never push unless asked; test risky writes on a throwaway save, then Save As.
