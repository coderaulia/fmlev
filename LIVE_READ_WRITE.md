# Live read/write verification — 24 October 2026

FM26's IL2CPP `PersonReference`, `ClubReference`, `NationReference`, and
`CompReference` are handles, not simulation records. The previous domain writers
used test-layout offsets against those handles. They must not be used for live edits.

## Support status

| Feature | Live reads | Live writes |
| --- | --- | --- |
| Player names, IDs, profile data | Bindings plus bounded native first/surname reads for discovery | Not supported |
| Player attributes | Native values take precedence after layout calibration | 60 technical/mental/physical/GK/hidden/personality fields |
| Position aptitudes and feet | Numeric bindings; validated native records take precedence | Native record write, identity validation, readback and rollback |
| CA/PA | Validated native u16 fields; 1..200 | Native writes with readback/rollback |
| Contract wages and expiry | Ownership-checked full contract; weekly wage in GBP | Weekly wage and expiry year, preserving month/day/flags |
| Contract termination | Native full contract, team squad vectors | Multi-object detachment releasing player as free agent with rollback |
| Loan recall | Native loan node and squad vector | Recalls player from loan, restores parent team pointers, clears loan node |
| Player move between clubs | Source and target team squad vectors, contract team pointers | Moves contracted player between squads (requires destination spare capacity) |
| Squad numbers | Contract team and player squad number (+0x5d) | Sets squad number (1-99) and atomically swaps with teammate wearing it |
| Tactical familiarity | 8 native vectors at player -0x48 | Sets all vectors to 10000 (game's MAXIMISE action) |
| Condition & sharpness | Native player +0xc0 / +0xc8 | Restores condition and match sharpness to 10,000 units (safe heal) |
| Club balance & transfer budget | Native finance object at club +0x150 | Direct transactional write to balance (+0x14) and transfer budget (+0x7cc) |
| Club facilities | Club details +0x100 and finance +0x8b5 | Training, youth, coaching, recruitment, and corporate facilities (1-20) |
| Club reputations & stadium | First team +0xa8, club +0x166, stadium +0x7c/+0x8c | First-team/world reputations (1-10000) and stadium capacity/expansion |
| Nation youth rating & macro | Containers at nation +0x108/+0x110, nation +0x1f0/+0x1ec | Youth rating (1-200), reputation (1-10000), economic factor, development code |
| Competition reputation & prizes | Native comp +0x188, raw GBP prize table | Reputation (1-200 native scale), league finishing position prize table |
| Register anytime | Human manager +0x404, comp stage rule entry +0x9a | Toggles manager bit 0x100 and domestic stage gate byte 0x9a=1 |
| Raw memory tools | Explicit address and type | Explicit address/type, range and transfer validation |
| Offline demonstration data | Fixtures | Fixtures only; does not modify FM26 |

Unsupported saves return errors and do not mark a cached edit as persisted.
Entity fields without bindings must not be interpreted as verified game values.
Native direct and context save/poll/readback round-trips passed against Kurzawa in
PID 53451; original ratings were restored immediately. No save-file reload or
simulation-advance durability test has been performed.

## Saving

Autosave is disabled. Field changes remain pending until the relevant Save button
is clicked. Presets are staged for an explicit Save as well. Previously stored
autosave preferences cannot enable background writes.

## Native positions and footedness

Observed live records for Zulkifli Lukmansyah (index 65304, unique ID 2000448177)
and Layvin Kurzawa (index 38141, unique ID 85010869) establish:

- Position bytes at player + `0x08`, ordered by masks
  `1,2,8,16,4,128,512,1024,256,4096,8192,2048,16384,64,32`.
- Left/right foot at `0x2f`/`0x30`, scaled by five.
- Native person subobject at `0x140`, index at `0x148`, duplicate unique IDs at
  `0x14c` and `0x150`.

Discovery scans for index and duplicate IDs, requires native vtable pointers in
read-only memory pointing into executable memory, and correlates both feet and
at least 14 of 15 position values. At most one stale UI row is permitted because
Kurzawa's previous UI-only edit left WBL 20 while the native rating was 18.
Ambiguous or unmatched candidates are rejected. No process-specific player address
is compiled into the writer. Cached records validate process, index, IDs and both
vtables before every operation. SW is preserved because it has no editor field.

A manual edit patches only fields changed from its UI baseline onto freshly read
native positions. Thus an untouched stale UI WBL rating does not overwrite native
WBL. Non-position player changes are rejected before any write.

## Prior singleton corruption

The old numeric writer mutated shared `TypedValue.s_commonInts` payloads. This
session contained corrupt constants 9→11, 10→16 and 19→18. With explicit user
approval, exactly those three payloads were restored and verified. The original
bytes and object identities are recorded in `/tmp/fmlev-commonints-backup.json`.

The new cache writer only replaces validated binding references; it never edits
shared numeric payloads. Numeric reads can recover the canonical integer from
its pool index. Changing bindings does not itself persist simulation data.

## Verification result

- Workspace/all-targets: 88 Rust tests passed.
- Embedded UI: 13 JavaScript behavioral tests passed, including forced manual
  save, queued presets, entity identity, and failed-save recovery.
- Native direct and context save/poll/readback/restore passed against the live game.
- `git diff --check` passed.

The running server was not restarted or deployed. Existing uncommitted work was
preserved; no commit or push was performed.

## Verification commands

```bash
rtk cargo test --workspace --all-targets
rtk proxy node --test crates/fmlev-server/tests/ui-save.test.cjs
# Read-only native discovery (open the corresponding profile in FM26 first):
rtk cargo run --release -p fmlev-core --example verify_native_positions -- 38141
```

The diagnostic optionally accepts `--round-trip` or `--context-round-trip` to
explicitly test an edit and restore all original positions and feet immediately.
Those flags write live memory. Its default mode is read-only.

The existing `verify_live_context` example is now read-only as well.

## Follow-up: Zulkifli live mismatch

A fresh check against FM26 PID 53451 found the editor process PID 217907 was
still running the deleted 15:27 executable, while the release binary on disk was
built at 15:51. Its `/api/context` returned Zulkifli (index 65304) with DL 20,
AMR 20, WBR 14 and left foot 14. Independent native discovery and current game
bindings agreed on DL 1, AMR 13, WBR 13 and left foot 8 (DR/right foot 20).

The current `--context-round-trip` diagnostic passed against Zulkifli: context
save, native readback, context poll and restoration of all original positions
and feet. Workspace tests (88) and UI tests (13) passed again. This establishes
the stale running editor mismatch and current native write behavior; visual
in-game confirmation and save/reload durability remain unverified. The user subsequently authorized replacing the running editor.

## Authorized restart and launcher controls

The old server was stopped and the current release started on port 41789.
The running HTTP `/api/context` now agrees with the native Zulkifli record:
DL 1, AMR 13, WBR 13, left foot 8 and DR/right foot 20. An HTTP player Save
changed DR 20 to 19, a fresh context poll confirmed 19, and an HTTP Save restored
all original positions and feet. Visual in-game confirmation and save/reload
durability are still unverified.

`start.sh` now supports start/restart/stop/status and optional `--background`
through `tools/server_control.py`. It inspects the listener's executable, detects
old binaries, serializes lifecycle commands, builds before stopping the old
server, and refuses to stop unrelated port owners. Foreground launchers remain
attached, including when reusing an existing server. Seven lifecycle regression
tests pass. Live checks passed for duplicate prevention, Ctrl+C on a new server,
Ctrl+C on an existing server, and background restart. The server was left running
in background mode with output in `~/.local/state/fmlev/server.log`.

## Native attribute editing and loaded-player discovery

The former positions-only guard rejected legitimate attribute edits, including
Dion Markx's Passing. A native writer now supports 34 technical, mental and
physical fields in the player block at `+0x17..+0x4c`. Ratings use a native
0..100 scale and displayed `max(1, (raw + 2) / 5)`. Initial calibration checks
all 34 values against observed game bindings, as well as the separately verified
identity/positions/feet layout. This first implementation was subsequently extended as described below.

Player Save requests can include `changed_fields` (for example
`["attributes.passing"]`). The server merges only these fields into its latest
snapshot, so stale CA, names or contracts cannot hitchhike on an attribute edit.
The UI also stops silently replacing CA with an attribute-based recommendation.
Unsupported explicit edits report their exact field paths before any writes.
Mixed position/attribute saves validate every edit up front, verify native
readback, and roll back exact original bytes on failure, preserving fractional
native ratings. Live polling reads native attributes rather than stale bindings.

With one calibrated player, **Scan Loaded Players** (`POST /api/players/discover`)
scans the validated native player vtable, then checks each record's person
vtable, index, duplicate UID, position/foot ranges, attribute ranges and bounded
native first/surname strings. Ambiguous indices are excluded. The live scan in
PID 53451 indexed **49,467** loaded players, including Lionel Messi. This is a
scan of loaded records, not proof that every player in every database is loaded.
The type and addresses are learned from the running process; no player-specific
address or index is compiled into the discovery or write paths. Search dropdown
responses are capped at 100 matches and expose the total match count.

Dion (index 54762, UID 2000249370): HTTP Save Passing 14 -> 15 changed native
player+0x1e from 70 -> 75. Only that byte changed across a 512-byte snapshot;
three context polls retained 15. Deliberately stale CA and wage values in the
request were ignored because they were not explicit edits. Original bytes were
restored and Passing returned to 14. Selection/write/readback also passed using
the indexed player cache. Game UI visual confirmation and save/reload durability
remain unverified. No commit or push was performed for this follow-up.

## Extended player fields

Reference evidence: [FMSuperScout native field map](https://github.com/mavarobli/FMSuperScout/blob/6af51d4959790ade94e4a575f5555338dc90a851/plugin/Fields.cs).
The reference player base is 0x148 before this project's validated player
subobject: positions and attribute block locations line up exactly. The source
also resolves the previously ambiguous Composure (`+0x4b`) versus Concentration
(`+0x4c`) ordering. No third-party implementation code was copied.

The attribute schema now exposes 60 fields, including Marking, Long Throws,
Punching, Dirtiness and Controversy, which previously had no model/UI fields.
Performance and hidden playing attributes use the native 0..100 scale;
personality bytes are unscaled 0..20. CA/PA are checked u16 values at
`+0x11c/+0x11e`. Native data supersedes cached bindings on selection and polling.

Full contracts are followed through player+0x1e8. Every access checks the
contract vtable, writable object region, and backlink to this player's person
subobject. Weekly wage is a u32 GBP value at contract+0x20; expiry is a packed
calendar date at +0x48. Year edits retain the month/day and packed flags, with
February 29 clamped to February 28 in non-leap years. Missing/unowned/invalid
contracts fail before writes. Mixed attribute/CA/PA/contract saves use a single
checked transaction and exact-byte rollback. Names, nationality/transfers,
fitness/morale, release clauses and squad-status/happiness changes, and other
entity writers remain outside the verified write schema; an explicit edit to
one reports its precise field path instead of pretending success.

### Extended live proof

In PID 53451, after indexing 49,467 loaded players, a mixed HTTP Save on Dion
changed Passing 14->15, Handling 3->4, Professionalism 12->13, CA/PA 80/104->81/105,
weekly wage GBP 542->543, and expiry 2028-05-31->2029-05-31. Independent native
player and contract snapshots showed only the requested field bytes changed;
three context polls retained every edit. All original player and contract bytes
were restored exactly, and the previous selection/follow mode was restored.
The recovery snapshot is local at `/tmp/fmlev-dion-extended-backup.json`.

Automated proof: 101 Rust tests, 16 UI behavior tests, and 7 launcher tests.
Coverage includes mixed attribute/ability/contract writes, contract ownership
rejection before any write, exact fractional-byte rollback, calendar/flag
preservation, explicit changed-field merging, all 60 UI attribute controls,
identity and range checks, and native-type peer discovery. The editor is running
the updated release. This follow-up remains uncommitted and unpushed.


## Update — 25 October 2026 (native club and competition fields)

| Feature | Live reads | Live writes |
| --- | --- | --- |
| Club balance and transfer budget | Native finance object | Native write with readback/rollback; verified in game (balance month-end persistence unconfirmed) |
| Club income, expenditure, facilities | Bindings | Not supported: UI caches are overwritten by the game; native sources not located |
| Competition prize money | Native i32 table (identified by Rules-page values) | Native write with readback/rollback; verified on the Rules page, season-end payout unconfirmed |
| Competition rules (subs, foreign, squad) | Not located | Not supported |
| Tactical familiarity | Per-player object at record - 0x48 | Native write to the maximum (same as the game's action); verified for a player and a whole squad |
| Register anytime | Manager flag + per-competition stage rule byte | Native write, reversible; verified for La Liga and the Champions League |
| Squad number | Native contract + 0x5d | Native write with teammate swap, readback/rollback; verified in game (visible after next day) |

Never edit a player's injury vector (player + 0x248): a null or emptied vector for an injured player crashes FM26 at `game_plugin.dll+0x33af065` and the damage is saved into the career.

## Contracts and transfers (Phase 4)

- **Extend contract (verified in memory):** `POST /api/player/contract/extend {player_id, years?}` writes the expiry word at contract + 0x48 (year in the high 16 bits, flags `0xfe00`, 1-based day of year), with readback and rollback.
- **Terminate (not implemented):** setting expiry to the in-game date did not release the player after the date passed. The route returns 501.
- **Sign / loan (not implemented):** a signing allocates a new contract (record + 0x1e8) and an agreement object (record + 0x1e0); a loan adds an agreement object (record + 0x1f0) holding the person, the loan team and start/end dates. Memory writes cannot create these. A contract's team pointer (+0x10) is a team object (first team, B team, U19s), not the club. Both routes return 501.
- **Diagnostics:** `tools/fmscan` and the `inspect_contract`, `find_refs`, `snapshot_player` examples.
