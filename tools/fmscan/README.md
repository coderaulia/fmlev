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

## Interpretation limits

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
