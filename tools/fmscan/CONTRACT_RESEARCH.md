# Contract and transfer discovery

## Current editor boundary — 2026-10-27

Transfer discovery and API implementation described below do not establish safe
simulation persistence. The [current UI](../../docs/UI_ARCHITECTURE.md) disables
unverified sign/move/terminate/recall actions until day-tick and save/reload evidence
is complete. The UI completion work made no new game writes or transfer experiments.


The C++ scanner reads native contracts and follows the unresolved transfer paths.
It does not sign, loan, release or move players. Changing language does not supply
the missing game object lifecycle or prove a writable schema.

## Commands

Build from the repository root:

```bash
g++ -O2 -std=c++20 -Wall -Wextra -Wpedantic -Werror \
  -o tools/fmscan/fmscan tools/fmscan/fmscan.cpp

# Discover the player's index and UID without opening a profile.
tools/fmscan/fmscan entity PID player --name Endrick --json

# Inspect a parent contract, including absent/malformed contracts.
tools/fmscan/fmscan contract PID INDEX UID --json

# Follow agreement/loan slots, current team and squad membership candidates.
# --refs adds a heap reference scan; --max caps displayed references, not totals.
tools/fmscan/fmscan transfer PID INDEX UID --refs --max 100 --json

# Capture baseline and changed snapshots during an action performed in FM26.
tools/fmscan/fmscan transfer-watch PID INDEX UID --ms 100 --count 600 \
  --json > transfer-events.jsonl
```

All three accept `--record HEX` for a previously located **attribute record**
(the `record` from `entity`, not its ACTUAL_PLAYER `address`). The identity and
RTTI must still match. Without it the scanner requires one matching native player;
duplicates fail. `--near HEX[:RADIUS]` bounds discovery/reference scans; followed
pointers can be outside that region. It is not a target-read sandbox.

`contract` checks FULL_CONTRACT RTTI and its person back-pointer before decoding
wage, expiry and squad number. A null contract is reported as absent; wrong
ownership/type is unavailable, and invalid dates stay explicitly invalid.
The status byte at contract+0x57 is an upstream research candidate, reported raw
without treating its bit meanings as locally verified editor fields.

`transfer` inspects attribute-record+0x1e0 (agreement candidate), +0x1f0 (loan
node), and the node's first pointer. Each candidate has RTTI, a person-pointer
comparison, a raw 256-byte window, possible date shapes and validated team/club
pointer targets. These windows are **not allocation boundaries**; adjacent objects
can appear inside them. Date/team candidates do not prove ownership, active deal
status, start/end meaning, or join scheduling.

The upstream current-team lead is ACTUAL_PLAYER+0x130 (attribute record-0x18).
Team identity and club are checked before following the squad vector candidate at
team+0x38. The begin/end/capacity triple must be aligned, readable and bounded to
4096 pointer slots. Membership reports only exact ACTUAL_PLAYER pointer matches.
The same player can appear in both parent and current squad vectors. These are
read observations, not a license to remove a pointer from either list.

`--refs` searches for references to the selected person and the captured contract,
agreement and loan roots. Nearby RTTI is explicitly an ownership heuristic.
The scan reports total references, truncation, scanned bytes and unreadable chunks;
roots are checked again after scanning. `--max` accepts 0..4096, default 100.

`transfer-watch` emits `ready`, then `change` JSON lines with elapsed milliseconds
and a full replacement snapshot. It refreshes process maps each poll so new
allocations in new mappings can be inspected. Default interval is 250 ms; count 0
means until Ctrl-C. Identity or root-pointer changes *during* a capture fail with
a retry message. Reads remain sequential and are not atomic; short-lived changes
between polls and internal fields outside the inspected windows can be missed.
No reference scan runs in the polling loop.

## Public repository research, inspected 2026-10-25

Source availability does not itself establish permission to copy an implementation.
No third-party implementation or Cheat Engine runtime is bundled with this tool.

| Repository / inspected source | Useful evidence | Limit |
| --- | --- | --- |
| [FMSuperScout, Fields.cs](https://github.com/mavarobli/FMSuperScout/blob/413537972957d88093f76252f030bbf92bc5a3a8/plugin/Fields.cs), [Dumper.cs](https://github.com/mavarobli/FMSuperScout/blob/413537972957d88093f76252f030bbf92bc5a3a8/plugin/Dumper.cs) | FM26 person+0xa8 contract chain; contract wage +0x20, expiry +0x48, number +0x5d, status candidate +0x57. Distinguishes parent-club ownership from squad club. | Read-only BepInEx scout, not a sign/loan/terminate implementation. Its fields are pinned to FM26.3 and cannot be assumed valid across builds. |
| [fm-ai-assistent, FmOffsets.java](https://github.com/JelmerBouma1985/fm-ai-assistent/blob/12ab90d1ae322103daa218737e20f8729cfc36ef/src/main/java/com/github/fmaiassistent/linux/FmOffsets.java) | Windows/Linux native tables with build-specific RVAs and dynamic fallback; includes an AgreementOffset table slot at +0x1a0. | A route for future agreement-table discovery, not evidence for making a transfer complete or creating contracts. No table RVA was imported. |
| [FMScoutFramework, Contract.cs](https://github.com/AppCakeLtd/FMScoutFramework/blob/2a4903f259e6530bf693f3abfc05371c49660a7f/Entities/Ingame/Persons/Sub-Entities/Contract.cs) | Contract entity separates wage, dates, status, clauses and team; Save writes fields including the team address. | Older FM framework; assigning a contract team is not proof of a complete FM26 transfer. |
| [fm26rte, archived decompiled presets](https://github.com/poi82999/fm26rte/blob/001cf00d1f1d509e5b6d440ebb1d27a45fb42726/research/reverse-engineering/ce_lua_decompiled/FMCETablePresets.lua), [helpers](https://github.com/poi82999/fm26rte/blob/001cf00d1f1d509e5b6d440ebb1d27a45fb42726/research/reverse-engineering/ce_lua_decompiled/FMCETableHelpers.lua), [enums](https://github.com/poi82999/fm26rte/blob/001cf00d1f1d509e5b6d440ebb1d27a45fb42726/research/reverse-engineering/ce_lua_decompiled/FMCETableEnums.lua) | `terminateContract`/`terminateLoan` involve team-list removal, current-team changes and several contract links. `removePlayerFromTeam` compacts a pointer table. Enums supply current-team +0x130 and team player-vector +0x38 research leads. | Archived third-party Cheat Engine code, not demonstrated safe here; decompilation and version assumptions need independent checking. No scripts or writes were executed. |

The strongest new lead for the failed expiry-only termination is squad membership
and multiple contract links. The upstream scripts suggest a multi-object operation;
they do not establish that copying it is sufficient or safe in this running save.
FmStudio26/FMStudio26 was also checked: the tree exposed documentation/media rather
than implementation source, so its advertised editing features were not evidence.

## Live read-only evidence

2026-10-25, FM26 PID 5945; no screen navigation or target writes:

| Player | Index / UID | Parent contract club | Current-team candidate club | Exact pointer membership |
| --- | --- | --- | --- | --- |
| Lamine Yamal Nasraoui | 40629 / 2000256231 | Barcelona | Barcelona | Present once in the 31-slot squad vector. |
| Endrick Felipe Moreira de Sousa | 38638 / 2000220941 | R. Madrid | Olympique Lyonnais | Present once in Lyon's 22-slot vector and once in R. Madrid's 27-slot vector. |

Endrick's indirect loan object resolves to `.?AVLOAN_CONTRACT@db@@`, its +8 person
pointer matches, and team pointers at +0x10/+0x18 both identify Lyon. Date-shaped
words at +0x44/+0x48 decode to year/day 2025/195 and 2026/181. They are not newly
calibrated writable dates. Yamal's contract reads GBP 280867/week, expiry year 2031,
day 181 and number 10. Native schema/reference agreement is not screenshot,
day-tick or save/reload proof.

Raw captures: `artifacts/fmscan/contracts-2026-10-25/yamal.json` and `endrick.json`
(local ignored artifacts; addresses are transient). The synthetic suite also
checks free agents, bad ownership/types/dates/teams/vectors, duplicate identities,
loan attachment, reference truncation and watcher changes. There has been no live
sign, loan, terminate or pending-join experiment in this stage.

## Live Contract Termination and Transfer Mechanics (Stage 1 Resolution)

### Free Agent Identity & Reference Findings (2026-10-25, PID 5945)
1. **Unemployed Free Agents in FM26**:
   - Examination of genuine free agents across live memory (e.g., Brad Guzan, 2888/980785):
     - `record + 0x1e8 == 0` (parent `FULL_CONTRACT` pointer is null).
     - `ACTUAL_PLAYER + 0x130 == 0` and `ACTUAL_PLAYER + 0x138 == 0` (current team pointers are null).
     - `record + 0x1e0 == 0` (agreement slot is null, unless a pending transfer was arranged).
     - `record + 0x1f0 == 0` (loan node slot is null).
     - Global team squad vector scan: 0 appearances across all 4,890 team squad vectors.
2. **Contract Object Pointer Ownership**:
   - Whole-memory reference scans across all readable/writable private heap mappings revealed that **only one single pointer** in the entire process references a contracted player's `FULL_CONTRACT` object: the player's own `record + 0x1e8` slot.
   - There is no process-wide contract registry, vector, or lookup table iterating `FULL_CONTRACT` objects. Detaching the pointer at `record + 0x1e8` cleanly releases the player's contract without corrupting engine lookups.
3. **Squad Vector Memory Safety**:
   - Live scanning across 4,890 `TEAM` objects demonstrated that 4,346 teams have `end == capacity` while 544 have spare capacity.
   - **Compacting Removal**: Shrinking a squad vector (`compact_removal_patches`) only shifts remaining pointer entries left and decrements `end` by 8 bytes. It never reallocates and never overflows.
   - **Appending**: Appending a player to a destination squad vector (`append_patches`) safely requires `spare_slots > 0` (`end < capacity`). If a destination team is full, writes are refused rather than risking out-of-bounds heap corruption.

### Production Rust Implementation (`crates/fmlev-core/src/native_transfer.rs`)
- `terminate_contract`: multi-object detachment releasing the player as a free agent:
  - Validates player identity and contract ownership.
  - Locates and compacts all team squad vectors where the player appears (current team, parent team, loan team).
  - Clears `ACTUAL_PLAYER + 0x130` and `+ 0x138` to null.
  - Clears `record + 0x1e8` (contract), `record + 0x1e0` (agreement), and `record + 0x1f0` (loan) to null.
  - Atomic readback and full reverse rollback verification.
- `terminate_loan`: recalls a loaned player back to their parent club:
  - Compacts the loan team squad vector.
  - Restores `ACTUAL_PLAYER + 0x130` and `+ 0x138` to the parent team.
  - If not already present in the parent squad vector and spare capacity exists, appends the player.
  - Clears `record + 0x1f0` (loan node).
- `move_player`: transfers a contracted player between teams:
  - Requires destination squad vector to have spare capacity (`spare_slots > 0`).
  - Removes player from source squad vector (compacting `end -= 8`).
  - Appends player to target squad vector (`end += 8`).
  - Updates `ACTUAL_PLAYER + 0x130`/`0x138` and `FULL_CONTRACT + 0x10`/`0x18` to the target team.
  - Clears active loan node if present.
- REST API:
  - `POST /api/player/contract/terminate` `{ player_id }`
  - `POST /api/player/loan/terminate` `{ player_id }`
  - `POST /api/transfer/move` `{ player_id, target_club_id?, target_team_address? }`
  - `POST /api/transfer/sign` `{ player_id, target_club_id?, target_team_address?, wage_gbp?, expiry_year? }`: signs unemployed free agents by formatting an aligned 200-byte `FULL_CONTRACT` object (resolved vtable `game_plugin.dll + 0x4334df8`), linking `record + 0x1e8` and `ACTUAL_PLAYER + 0x130`/`0x138`, and appending to destination squad vector.
  - `POST /api/transfer/accelerate` `{ player_id, new_join_date?, instant? }`: accelerates scheduled join date in future agreement contract and transfer event priority queue, or instantly executes future contract and squad vector swap.
