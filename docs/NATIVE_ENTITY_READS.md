# Native club, nation and competition reads

## UI status — 2026-10-27

The [manifest UI](UI_ARCHITECTURE.md) now exposes guarded entity editors through one
explicit Save workflow. Readable fields are not automatically writable: population is
read-only, unsupported rules/governance remain unavailable, and unverified transfer
buttons are disabled. The evidence below is historical discovery evidence; the UI refresh
adds no new offsets or live persistence proof.


2026-10-25: read-only C++ additions after auditing `README.md`,
`LIVE_READ_WRITE.md`, `docs/HANDOVER.md`, `AGENTS.md`, the native Rust modules and
the UI models. There is no separate WIP file in this checkout. The fixture models
describe more fields than the verified native readers support.

```bash
tools/fmscan/fmscan entity PID club --name Barcelona --editor --json
tools/fmscan/fmscan entity PID nation --name Spain --editor --json
tools/fmscan/fmscan entity PID competition --name "Spanish First Division" --editor --json
```

No matching game screen, binding subsystem, prior object address or Rust editor
calibration is required. Plain text output is available without `--json`.
Each `editor_data` contains field-level source addresses/evidence and an explicit
`unavailable` list. These exports do not add Rust API/UI integration or write support.

## What now reads independently of the screen

| Group | Native C++ fields |
| --- | --- |
| Club facilities | Training, youth, junior coaching, youth recruitment, data analysis, corporate |
| Club identity/context | Nation, league, abbreviation, founding year, main-team reputation, separate world reputation |
| Club money & benefactor | Balance, available/allocated transfer budgets, transfer-retention percentage, sugar daddy benefactor status; raw wage budget, maximum wage, wage use, use with bonuses |
| Club grounds | Main team's stadium name, capacity and expansion capacity; separate training-ground name |
| Nation | Code, continent, economic factor, population, raw development-state code |
| Nation by gender | Men's and women's youth ratings and senior-team reputations, separately |
| Competition | Short name/code, nation, continent, native reputation, current stage registration gate |

The wage period has not been compared with the UI; wage values are named
`*_raw_gbp` and are not presented as weekly or monthly budgets. The development
state is a raw native code; the UI fixture's enum labels are not inferred.

## Ownership and scale findings

- A club's first team comes from its `+0x18` vector. The team's `+0x30` must point
  back to that club. Main-team reputation is the u16 at team `+0xa8`; the separate
  world reputation is u16 at club `+0x166`.
- Club `+0x100` owns a non-polymorphic details record. Its readable heap range,
  abbreviation string, founding year, expected rating ranges and stable owner
  pointer are checked before publishing facilities. This is not a managed handle.
- Club `+0x150` finance must have the expected native RTTI and point back to the
  club at `+8`. Advanced finance fields require `CLUB_FINANCE`, not its smaller
  base-class record. Money is raw GBP; negative balances are valid.
- Club `+0x140` is a training ground in the inspected layout. Main stadium comes
  from the first team's `+0x78`. Treating these as the same field gives the wrong
  ground. Both targets require native stadium RTTI and duplicate IDs.
- Nation `+0x108`/`+0x110` own separate men's/women's national-team containers.
  Each container requires `NATIONAL_TEAM_CONTAINER` RTTI, duplicate UID, and a
  `+0x170` nation back-pointer. Senior-team reputation follows the container's
  first-team vector with a team-to-container back-pointer.
- Youth rating is a **u16** at container `+0x864`, not a byte or a single nation
  field. The top-level nation `youth_rating` and `reputation` stay null; callers
  must choose `men` or `women`.
- Competition reputation is native **1..200**. It is not multiplied to satisfy the
  UI fixture model's 1..10000 assumption. The registration gate describes only
  the current stage entry; a human-manager override is not evaluated.

## Native code evidence

The running `game_plugin.dll` PE timestamp is `0x6a229e8a`. New numeric layouts
require that timestamp as well as the per-object guards. Unknown builds report
these fields unavailable; they do not guess offsets. Existing verified native
balance reads retain their finance ownership checks.

Read-only analysis of the DLL's native serialization routines established:

| Native field | Owner / offset | Native code RVA evidence |
| --- | --- | --- |
| Training facilities | Club details +0x118 | 0xea6f6d, numeric read linked to `training` |
| Youth facilities | Club details +0x123 | 0xea7159, linked to `youth_facilities` |
| Junior coaching | Club details +0x124 | 0xea6b05, linked to `youth_coaching` |
| Youth recruitment | Club details +0x125 | 0xea6e9b, linked to `youth_recruitment` |
| Data analysis facilities | Club details +0x11e | 0xea7123, FourCC tag `0x4379696d` ("Cyim") at 0xea6aeb |
| Corporate facilities | Full finance +0x8b5 | 0xef46c9, linked to `corporate_facilities` |
| Sugar daddy benefactor | Full finance +0x3c | 0x1636ea7, linked to `sugar_daddy` (0=None, 1=Foreground, 2=Background, 3=Underwriter) |
| Main-team reputation | First team +0xa8 | Club 0xef25b1; national container 0x1199308 |
| World reputation | Club +0x166 | 0xef49ba, linked to `world_reputation` |
| Youth rating | National-team container +0x864, WORD | 0x1198b0d and 0x1198b56, linked to `youth_rating` |
| Economic factor | Nation +0x1f0, BYTE | 0xf7c7b3/0xf7c7cb, native tag `Necf` |
| Population | Nation +0x1e4, DWORD | Direct population count (range 1..2,000,000,000) |
| Development state raw code | Nation +0x1ec, BYTE | 0xf7c7ee/0xf7cd6b, native tag `Nsod` |
| Stadium capacity | Main stadium +0x7c, DWORD | 0x204dca9/0x204dcbc, named capacity field |
| Expansion capacity | Main stadium +0x8c, DWORD | 0x204db63/0x204db79, named expansion field |
| Competition reputation | Competition +0x188, WORD | 0xfa4651/0xfa4668, named reputation field |

The economic-factor/development naming follows the native tags; development enum
labels are not verified. Field identification is code-correlated, not screenshot
proof. The readers do not execute these game routines or write to their fields.

Additional club identity and finance layout leads were cross-checked with the
[FM Season Hub reverse-engineering report](https://github.com/chilliesdev/fmseasonhub-re/blob/main/docs/02-memory-reader-offsets-plugins.md)
and its [layout reference](https://github.com/chilliesdev/fmseasonhub-re/blob/main/macos/src/offsets_embutidos.py).
Those references are research leads; live type/owner/range checks and the local
native-code analysis determine what this tool publishes. No third-party
implementation was copied.

## Live proof

FM26 PID 5945, without opening or navigating game screens:

| Entity | Read values |
| --- | --- |
| Barcelona 1074 / 1708 | Training 19, youth 20, junior coaching 20, recruitment 20, data analysis 18, corporate 19; sugar daddy None (0); team reputation 9098, world reputation 7557 |
| Barcelona grounds | Camp Nou: capacity 62,500, expansion 105,000; training ground is Ciutat Esportiva Joan Gamper |
| Barcelona finance | Balance GBP 14,117,374; available transfer budget GBP 13,336,559; allocation GBP 17,284,136; retention 30% |
| Spain 170 / 796 | Men's youth 145/reputation 9011; women's youth 160/reputation 9300; economic factor 17; population 49,570,725; development raw code 1 |
| Spanish First Division 51 / 67 | Native reputation 178/200, current stage registration gate 0 |

Full native discovery plus three fresh bounded readbacks agreed. Snapshots:
`artifacts/fmscan/barcelona-editor.json`, `spain-editor.json`, and
`spanish-first-division-editor.json`. Addresses and values are observations of
this running save and may become stale.

## Still WIP

| Area | Explicitly unavailable |
| --- | --- |
| Club | Board patience/confidence, fan confidence, affiliate terms |
| Nation | Flat UI development enum, flat tax rate, corruption index, national facility averages, spoken languages; gender-specific game importance, FA financial power, FIFA rank/points |
| Competition | Active substitutions/bench/windows, VAR, squad/foreign/homegrown limits, cards-for-ban, season team count, authoritative payout/TV-rights values |

Supporter patience is not board patience. Men's values are not women's values.
Editor-format rule nodes and display prize caches are not proven simulation
sources. Those distinctions are preserved in the exports rather than filled with
fixture defaults. Free agent signing, pending transfer acceleration, and contract termination are now supported in the Rust runtime.

## Staff information and contract (2026-10-28, read-only)

Verified against three on-screen staff profiles (Engonga, Pintus, Ortega) with `fmscan entity PID staff --editor` and
the Rust `staff_data info` example (both agree), PE timestamp 0x6a229e8a:

| Field | Location (staff record `R`) | Evidence |
| --- | --- | --- |
| Year of birth | u16 `R+0x18a` | 3 of 3 profiles |
| Contract link | pointer `R+0x1a8` to `FULL_CONTRACT`, owner back-pointer `+0x8 == R+0x100`; null = no contract | Engonga "N/A", two contracted staff |
| Weekly wage | u32 `FULL_CONTRACT+0x20` (x52 per year) | £245K and £345K p/a |
| Contract expiry | u32 `+0x48` = `(year << 16) | day-of-year` | 30/6/2026 and 30/6/2027 |
| Club | contract `+0x10` team, team `+0x30` club | used by the search index |

Not proven: the reputation slots (`+0xd4/+0xd6/+0xd8`, ordering only), CA/PA (`+0xda/+0xdc`, no on-screen numbers),
club job, role, national team job. The open staff profile is found from the staff-only UI bindings
(`AttributesCoaching*`, `AttributesMedical`, `AttributesBlockInitialStaffSelection` with `IsPlayer = 0`; the reference
id is the staff index).

## Verification

- 12 entity-process tests cover values/scales, gender separation, field range
  failures, finance/container/team ownership, unknown builds, invalid stages and
  the stadium/training-ground distinction.
- Current combined C++ process suite: 57 passed, with warnings-as-errors builds.
- Workspace check passes; Rust 123 passed / 1 ignored; UI 37 passed.
- All modified/new source files remain strictly below 1,333 lines.
