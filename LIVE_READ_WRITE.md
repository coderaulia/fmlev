# Live read/write verification — 24 October 2026

FM26's IL2CPP `PersonReference`, `ClubReference`, `NationReference`, and
`CompReference` are handles, not simulation records. The previous domain writers
used test-layout offsets against those handles. They must not be used for live edits.

## Support status

| Feature | Live reads | Live writes |
| --- | --- | --- |
| Player names, IDs, profile data | Available bindings only | Not supported |
| Player attributes | Numeric bindings, including `m_ctx` row fallback | Not supported: native offsets unverified |
| Position aptitudes and feet | Numeric bindings; validated native records take precedence | Native record write, identity validation, readback and rollback |
| CA/PA, condition, morale, contracts | Available bindings only; zero/empty means unavailable | Not supported: native offsets unverified |
| Clubs, nations, competitions, affiliates | Available bindings only | Not supported: native offsets unverified |
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
