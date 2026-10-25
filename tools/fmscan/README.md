# fmscan

Read-only C++20 discovery tools for FM26 under Proton. Rust remains the editor and
the only place for verified writes, readback and rollback. The scanner uses
`process_vm_readv`; it never writes to the target process. It writes only requested
snapshot/address files and temporary snapshots used by `diffset`.

```bash
cd tools/fmscan
g++ -O2 -std=c++20 -Wall -Wextra -Wpedantic -Werror -o fmscan fmscan.cpp
./fmscan --help
python3 -m unittest discover -s tests -v
```

The scanner needs permission to read the target. Run as the appropriate user;
Linux ptrace policy can still deny access. Tests launch a synthetic process that
explicitly permits scanner reads; they do not require FM26 or elevated privileges.

## Commands

`PID` is decimal. Addresses, `--near` radii and diff filter values are hex, with
optional `0x`. Other integers are decimal or `0x` prefixed. All commands accept
`--json`; normal commands emit one JSON object, while `watch` emits JSON lines.
Errors go to stderr with a nonzero exit status, and are JSON when requested.
Addresses and 64-bit memory values are hex strings to avoid JSON precision loss.

| Command | Purpose |
| --- | --- |
| `snap PID FILE [--near ADDR:RADIUS]` | Save readable anonymous/heap `rw-p` mappings. |
| `diff PID FILE` | Compare snapshot with live memory in 8-byte words. |
| `refs PID PTR` | Find aligned pointer slots. |
| `find PID i32\|u32\|u64\|f32 VALUE` | Find aligned values. |
| `dump PID ADDR LEN` | Read exact bytes; length limited to 16 MiB. |
| `struct PID ADDR [--depth N] [--max OBJECTS]` | Annotate 0x400 bytes per object, recursively following heap pointers with a cycle guard. Default depth 0, max 64 objects; limits 8 and 4096. |
| `vec PID ADDR [--stride N] [--max N]` | Validate a begin/end/capacity triple and dump elements. |
| `rtti PID VTABLE_OR_OBJECT` | Resolve an MSVC x64 RTTI decorated type name in `game_plugin.dll`. |
| `contract PID INDEX UID [--record HEX]` | Inspect an owned parent contract, including absent/invalid diagnostics. |
| `transfer PID INDEX UID [--record HEX] [--refs] [--max N]` | Inspect agreement/loan roots, current team and squad membership candidates; optionally scan inbound references. |
| `transfer-watch PID INDEX UID [--record HEX] [--ms N] [--count POLLS]` | Emit baseline and changed contract/transfer snapshots as JSON lines. |
| `entity PID player\|club\|nation\|competition [--name TEXT] [--id N] [--uid N] [--related]` | Discover native named records without opening any FM26 screen. |
| `graph PID OBJECT --in --out` | Group inbound slots by region and annotate outbound mapped pointers. Either direction or both is supported. |
| `watch PID ADDR LEN [--ms N] [--count POLLS]` | Poll a range and print changed words, including a trailing partial word. |
| `diffset PID [--noise-ms N] [--action-wait SECONDS]` | Sample noise, snapshot baseline, wait for action, exclude noise and rank changed words. |

`diff`, `refs`, `find`, `graph` and `diffset` accept `--near ADDR[:RADIUS]`
(default radius 0x100) and `--max N` (default 400). Search and graph count all hits
but cap displayed results. `graph --len N` controls outbound inspection size
(default 0x400, max 1 MiB). `vec --max N` defaults to 64, with a limit of 4096.

`diff` supports `--from HEX`, `--to HEX`, `--to32 HEX`, `--exclude FILE` and
`--save-changed FILE`. The latter saves every changed word address before filters,
one hex address per line, for later exclusion. Input and output files must differ.
Snapshots retain the original binary layout: repeated native little-endian
`uint64 address, uint64 byte_length, byte[byte_length]` chunks (max 1 MiB).
Unreadable chunks are skipped, never compared against invented zeros. Corrupt or
truncated snapshots fail explicitly. The diff ignores fewer than eight trailing
bytes in a chunk. Snapshots are sequential reads, not atomic game state captures.

`watch` defaults to 250 ms and runs until Ctrl-C; `--count N` bounds polling for
scripts (0 means unlimited). Its JSON stream starts with `event: "ready"`, then
emits `event: "change"` with address, old/new values, byte width and elapsed ms.
An unreadable range stops the watch with an error.

## Discovery workflow

Named entity discovery requires only the game PID, with no prior address, active
profile, UI binding or editor calibration:

```bash
./fmscan entity PID player --name "Lamine Yamal" --related --json
./fmscan entity PID club --name Barcelona --related --json
./fmscan entity PID nation --name Spain --json
./fmscan entity PID competition --name "Spanish First Division" --related --json
```

`entity` reads the loaded DLL's MSVC RTTI to discover native class vtables, then
scans heap memory for records with matching types and duplicate UIDs. Native UTF-8
name records supply the names. Player identity also requires the actual-player
person subobject, executable attribute vtable, position range and scaled foot
ratings. No UI subsystem, cached player list or fixed module RVA is used.

Name matching is a case-insensitive substring for ASCII characters; non-ASCII
UTF-8 is preserved and matched byte-for-byte. `--id` is the runtime index;
`--uid` is the database unique ID. Selectors combine with AND. `--max` defaults to
20 and is limited to 1000; total matches and truncation are reported separately.
`--near` optionally bounds the heap scan, but is not needed for discovery.
Duplicate matching identities fail even when outside the displayed result limit.
`validated_records` counts structurally valid records in scanned readable memory,
not an exhaustive census of every player in a save.

`--related` follows player -> parent contract -> team -> club -> nation, plus
team -> domestic competition -> competition nation. Each target requires the
expected RTTI class and matching duplicate UIDs. The contract must point back to
the player's person subobject. Club searches use the first team in the club's
native team vector, with bounds and team-owner back-pointer checks. Missing,
unmapped or invalid links appear under `related.unavailable`; they are not guessed.
Loan destinations and additional continental/cup competitions are not inferred.
These are guarded FM26 read layouts, not write authorization or universal schemas
for future game versions. A structurally incompatible record is rejected.

### Player editor data and Rust comparison

`--editor` also supports **club, nation and competition** records. See
[native entity field coverage, ownership evidence and WIP](../../docs/NATIVE_ENTITY_READS.md).
The new numeric layouts check the native DLL's PE timestamp; unknown builds leave
those fields unavailable. Examples:

```bash
./fmscan entity PID club --name Barcelona --editor --json
./fmscan entity PID nation --name Spain --editor --json
./fmscan entity PID competition --name "Spanish First Division" --editor --json
```

Club exports include facilities, guarded finance values, reputation and main
stadium capacity/expansion. Nation youth/reputation are separate under `men` and
`women`; economic factor and a raw development-state code are also readable.
Competition reputation keeps its native 1..200 scale; its stage registration gate
does not claim to evaluate manager overrides. Unmapped active rules remain null.

```bash
# Print every supported editor field in readable text, without opening FM screens.
./fmscan entity PID player --name "Lamine Yamal" --editor
# Same export as structured JSON (editor_data inside each match).
./fmscan entity PID player --name "Lamine Yamal" --editor --json

# Scan by known runtime index and UID, then dump the common native data payload.
./fmscan player-data PID INDEX UID --json
# Refresh an already located attribute record (not the ACTUAL_PLAYER object base).
./fmscan player-data PID INDEX UID --record HEX_NATIVE_RECORD --iterations 1000 --json
```

The editor export contains the name and IDs, all 60 editor attributes, 15 position
aptitudes (including SW), feet, CA/PA, nationality, parent-contract club, domestic
competition, weekly wage in GBP, contract expiry year/packed date and squad number.
Age, condition, sharpness, morale, injury/suspension state, release clause, squad
status and happiness are explicitly null/unavailable. The editor's fallback values
are not substituted for unmapped native fields. Position text is derived from
aptitudes of at least 18, not copied from a binding. CA stars and recommended-CA UI
decorations are not native fields and are not exported as game values.

`player-data` is a read-only, guarded identity-based diagnostic. It does not perform
RTTI name discovery or alter the Rust writer's calibration flags. Its common payload
is exactly reproduced by `crates/fmlev-core/examples/player_data.rs`, using FMLE's
real Linux memory driver and production attribute decoder. Both read a 512-byte
native player snapshot and a 96-byte contract snapshot, validate duplicate IDs,
vtable code pointers, attribute ranges, date and contract ownership, and reread
identity/contract pointer before publishing. Invalid attributes/dates fail rather
than returning a partial record. The benchmark's core payload omits the extra
RTTI-linked nationality/club/competition fields and all unsupported fields.

From the repository root:

```bash
python3 tools/fmscan/benchmark_player.py --pid PID --index INDEX --uid UID \
  --record HEX_NATIVE_RECORD --trials 7 --iterations 1000 \
  --output artifacts/fmscan/player-benchmark.json
```

The script builds C++ with `g++ -O3` and Rust with Cargo release (optimization level
3) before timing. It alternates binaries, excludes one warm-up per mode/binary,
compares every payload and checksum, and requires identical scanned-byte/error
counts. Full discovery uses supplied index/UID, matching anonymous/heap mappings,
1 MiB chunks and aligned identity search in both implementations. The original
RTTI/name search, HTTP, UI rendering and compilation are outside these timings.
Known-record reads repeat 1,000 times per trial, with JSON serialization outside
the internal timer. Process wall times, min/median/max and raw trials are retained.
These are warm-cache measurements of these implementations, not intrinsic language
speed or a benchmark of the existing HTTP editor refresh path.

Live results on 2026-10-25 (Intel Core i3-10100; seven trials):

| Matching workload | C++ median | Rust median |
| --- | ---: | ---: |
| Full identity scan, internal | 919.67 ms | 799.39 ms |
| Full scan + output, process wall time | 946.92 ms | 823.57 ms |
| Located record read/decode, per player | 8.53 us | 9.67 us |

Both scanned 4,716,535,808 readable bytes, reported 20 unreadable chunks and returned
identical supported data. Rust won the full scan; C++ won the short repeated-read
loop. Results vary with scheduling, residency, game activity and implementation.
Live CA/PA were 175/194, wage GBP 280,867/week, expiry year 2031 and squad number 10.
No game screen was opened, memory written or screenshot parity claimed.

Live read-only proof on 2026-10-25, without opening or navigating any game screen:

| Native entity | Index | UID |
| --- | ---: | ---: |
| Lamine Yamal Nasraoui | 40629 | 2000256231 |
| Barcelona | 1074 | 1708 |
| Spain | 170 | 796 |
| Spanish First Division | 51 | 67 |

The initial full player scan validated 39,489 records and found one matching name;
20 unreadable 1 MiB chunks were reported. Separate full native scans found Barcelona,
Spain and the competition, and agreed with the player's pointer-chain results.
This proves screen-independent native discovery in this running save. It does not
claim screenshot comparison, save/reload durability, or safe writes.

```bash
# Inspect a known native record and its immediate heap objects.
./fmscan struct PID HEX_OBJECT --depth 1 --json
./fmscan rtti PID HEX_VTABLE --json
./fmscan vec PID HEX_VECTOR_HEADER --stride 8 --json
./fmscan graph PID HEX_PERSON --in --out --json

# Compare an action with background noise removed.
./fmscan diffset PID --noise-ms 2000 --action-wait 10 --json
# Without --action-wait, perform the action and press Enter at the prompt.
# Add --near HEX_OBJECT:1000 to limit reads and snapshot disk usage.

# Watch a record during a day tick or UI action.
./fmscan watch PID HEX_OBJECT 1024 --ms 100 --json
```

`diffset` sends phase prompts to stderr, keeping stdout machine-readable. It uses
unique temporary snapshots and removes them on completion, errors and handled
interrupts. Full scans may require several GB of temporary disk space. Background
noise defaults to a 1000 ms sample. `--exclude FILE` adds known noise addresses.

Candidates are ranked by a transparent heuristic: +100 for a preceding valid RTTI
object within 0x400 bytes in the same region, +20 if the new value is a mapped
pointer, +10 for a zero-to-nonzero change. Ties sort by ascending address. A nearby
vtable is a clue; it does not establish ownership. Changes at sampled noisy slots
are excluded even if the later action also touches them.

## fmwrite (experimental transactional writer)

`fmscan` stays read-only. `fmwrite` is a separate executable that writes only
allowlisted scalar fields of a club, nation or competition, using a journaled plan.

```bash
g++ -O2 -std=c++20 -Wall -Wextra -Wpedantic -Werror -o fmwrite fmwrite.cpp
./fmwrite plan PID club|nation|competition INDEX UID --set field=value,... --plan NEW_FILE
./fmwrite show PLAN_FILE
./fmwrite apply PLAN_FILE [--experimental]
./fmwrite restore PLAN_FILE
```

- `plan` is read-only: it needs exactly one native identity match (index + UID),
  the known DLL build (PE timestamp 0x6a229e8a), an in-range value and a private-heap
  target. The plan file is created exclusively (never overwritten) and fsynced.
- `apply` takes a per-process lock, rechecks process start time, identity and field
  owner, writes each field with `process_vm_writev`, reads it back, then polls three
  times. Any failure rolls back in reverse order; the journal records
  `applying/applied/rolled_back/rollback_failed`. `restore` writes the `before`
  values back and resumes partial scalar writes.
- Club facility writes are refused unless the club subrecord has a single owner.
- Allowlist: club `balance`, `transfer_budget` (no flag); club facilities/reputation,
  nation youth rating/reputation/economic factor and competition reputation need
  `--experimental` because game visibility and durability are not verified.
  Strings, pointers, rules/prizes, contracts and transfers are rejected.
- Use a throwaway save. `plan` and `show` never write; `apply` and `restore` do.
- Tested by `tests/test_writes.py` against a synthetic process (fault injection in
  `tests/write_faults.cpp`). Live use so far: one R. Madrid balance apply/restore,
  restored to the original bytes; not checked on screen.

## Interpretation limits

Contract/transfer commands, upstream source research, live Endrick/Yamal evidence,
and the next experiment are documented in [CONTRACT_RESEARCH.md](CONTRACT_RESEARCH.md).
They are read-only diagnostics; sign, loan and terminate writes remain unresolved.

- Structure output labels mapped pointers and small integers; nonpointers include
  both u32/i32/float halves and possible packed dates. Dates are shape matches,
  not proof of field meaning. Child objects are limited to heap mappings.
- Vector triples cannot prove an element stride. Output lists possible divisors
  and defaults to a raw qword view with `element_count: null`. Use `--stride` only
  when independently known. Raw element byte dumps are limited to 64 bytes each.
- RTTI validates the DLL's PE32+ image and COL self/type RVAs. Proton's anonymous
  PE sections are supported. Names remain decorated; unrelated DLLs, stripped
  RTTI, and objects whose vtable is at a different offset are not inferred.
- Graph inbound grouping identifies memory regions, not authoritative squad,
  contract or team ownership. Scans use the process map list captured at startup;
  new mappings require a new command. Read failures are counted in search/diff/
  graph output; struct reports unreadable words.
- Sign/loan/terminate writes and injury-vector changes are outside this tool.

## Validation

The synthetic process tests cover every command, both RTTI input forms, anonymous
Proton-style PE sections, cyclic pointers, vector bounds/stride ambiguity, filters,
snapshot corruption, partial-word watch events, noise exclusion and candidate
ranking. The scanner is built with warnings as errors during tests.

Read-only FM26 smoke check on 2026-10-25: the live manager list decoded as one
qword entry, and the known manager secondary vtable resolved to
`.?AVHUMAN_NON_PLAYER@db@@`. No game state was changed. This proves live reading
and RTTI resolution; it does not validate any new write offset or transfer action.
