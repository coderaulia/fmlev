# AGENTS.md — Autonomous Agent Guidelines & Progress Tracking

Welcome to the **FMLE by Vanaila (Football Manager Live Editor)** codebase. This document outlines mandatory engineering rules, architectural conventions, and progress logs for AI agents working in this repository.

---

## 🛑 Strict Engineering Rules

All agents operating in this repository must strictly adhere to the following rules:

### 1. Maximum File Size Constraint (1,500 Lines)
- **Hard Limit**: No source code file may exceed **1,500 lines of code (LOC)**.
- **Context Window & Maintainability**: Large monolithic files must be modularized into domain submodules (e.g., using subdirectories with `mod.rs` and focused files such as `routes/`, `inspector.rs`, `mutations.rs`, etc.).
- **Asset Exception**: Embedded static web bundles (such as `crates/fmlev-server/src/ui.html`) remain unified assets unless explicit instructions are given to implement a separate build pipeline.

### 2. Commit Each Stage and Phase
- **Atomic Phase Commits**: Commit changes after completing each verified stage or phase.
- **Never Commit Broken Work**: Never `git commit` if the workspace has compilation errors, broken tests, or incomplete refactoring.
- **Explicit Push Requirement**: **Never `git push` or trigger deployment unless the user explicitly requests it in the prompt.**

### 3. Verification Protocol
Before marking any task or phase complete, agents must run and pass all test suites:
```bash
# 1. Typecheck and compile all workspace crates, targets, and examples
cargo check --workspace --all-targets --examples

# 2. Run all Rust unit and integration tests
cargo test --workspace

# 3. Run all UI and Node.js DOM tests
node --test crates/fmlev-server/tests/*.test.cjs
```

### 4. C++ Discovery Tooling (`tools/fmscan/`) — HARD RULES
The C++ scanner proved to be the fastest way to find and validate native offsets (screen-independent entity search, RTTI naming, contract/transfer graphs, editor reads). These rules keep that advantage safe.

1. **Discovery in C++, product in Rust.** New offsets/layouts are found and cross-checked with `fmscan` first. Only Rust (`fmlev-core`/`fmlev-server`) ships features, UI and HTTP routes. Never put C++ in the server/UI runtime path and never shell out to `fmscan` from the product without an explicit user decision.
2. **`fmscan` is strictly read-only.** It must never gain a write API (`process_vm_writev`, `/proc/pid/mem` write, ptrace poke). It writes only requested snapshot/address files. Writes belong to Rust (`live_write.rs`: readback + rollback) or the separate, opt-in `fmwrite` (plan → apply → restore, allowlisted scalar fields, test save only, `--experimental` gate for unproven fields).
3. **Evidence ladder before anything reaches the UI.** An offset is promoted only after: (a) identity validated by RTTI/vtable + index + UID + ownership back-pointer (never by address or screen position alone); (b) read agrees between C++ and Rust on a live game; (c) for writes, an in-game screen check after a day tick **and** save/reload on a throwaway save. Missing proof → route returns 501, UI shows "unavailable", docs say "unconfirmed".
4. **Version guard.** Numeric layouts require the known PE timestamp (currently `0x6a229e8a`) plus object guards (vtable, bounds, back-pointers, ranges). On mismatch fail closed with a reason; never guess or fall back to defaults.
5. **No invented data.** Unreadable chunks, absent links and unsupported fields are reported explicitly (null + reason). Never substitute zeros or UI defaults. Heuristics (vector stride, date shapes, nearby RTTI) must be labelled heuristic in output and docs.
6. **UI wiring contract.** Every UI feature backed by a native read/write: Rust function with unit tests → REST route with strict validation (identity selectors, `{enabled/value}` bodies, reject unknown fields) → `ui.html` control showing verified/unverified state → Node DOM test. Writes must be transactional (snapshot, write, readback, rollback on mismatch) and idempotent where possible. Display note: FM26 often shows native changes only after reopening the page or the next day tick.
7. **Forbidden regions stay forbidden.** Never edit a player's injury vector (player+0x248), never write UI-binding caches or the heap `[allocated, available, .., balance]` block, never null/empty a pointer or vector the game iterates, never create game-allocated objects (contracts, loan agreements) by raw memory writes. Pointer swaps between lists/teams are unverified until mapped with `graph`.
8. **Tool quality gates.** Build with `g++ -O2 -std=c++20 -Wall -Wextra -Wpedantic -Werror`; every new command/field gets a synthetic-process test (`python3 -m unittest discover -s tools/fmscan/tests -v`) and `--json` output (addresses/64-bit values as hex strings). Source files ≤ 1,500 LOC (headers included). Built binaries (`fmscan`, `fmwrite`) are git-ignored.
9. **Reproducibility.** Every discovery gets a log entry in this file (or `docs/`): command used, live values observed, what was **not** proven. Benchmarks must be matched workloads, and results must not be generalised ("C++ vs Rust") beyond what was measured.
10. **Safety on the user's saves.** Any experiment that writes uses a throwaway save, then Save As; keep the plan/restore file; if FM26 behaves oddly after a write, stop and restore before continuing. Stage-end work is committed locally; never push without an explicit request.

---

## 🏗️ Architecture & Modular Layout

```
crates/
├── fmlev-cli/           # CLI launcher, stage probing, and process supervisor
├── fmlev-core/          # Memory structures, models, live write engine, and context manager
│   └── src/
│       ├── context/     # Modularized context engine (All files < 1,300 LOC)
│       │   ├── default_registries.rs  # Offline default registries (clubs, players, nations, competitions)
│       │   ├── inspector.rs           # IL2CPP memory extractors, typed value decoding, binding helpers
│       │   ├── mod.rs                 # GameContextManager, SearchResults, state lifecycle, public re-exports
│       │   ├── mutations.rs           # Live & offline mutation logic (players, clubs, nations, competitions)
│       │   └── tests.rs               # Context engine test suite
│       ├── live_write.rs              # Authoritative memory write engine and transaction safety
│       ├── models.rs                  # Domain data structures (Player, Attributes, Positions, etc.)
│       └── native_player.rs           # Native memory layout offsets and verification
├── fmlev-il2cpp/        # IL2CPP runtime metadata extraction and RVA calculation
├── fmlev-mem/           # Linux process memory scanning and /proc/pid/mem drivers
├── fmlev-overlay/       # In-game HUD overlay and terminal renderers
└── fmlev-server/        # Embedded Axum HTTP server & REST API
    └── src/
        ├── lib.rs              # Crate root, start_server, and public re-exports
        ├── routes/
        │   ├── mod.rs          # Router initialization and error helpers
        │   ├── context.rs      # Entity queries, search, presets, and updates
        │   └── memory.rs       # Process attach, memory read/write/scan, godmode, heal
        ├── state.rs            # ServerState, AppState, and request/response DTOs
        ├── tests.rs            # Server integration & unit tests
        └── ui.html             # Embedded single-page web UI asset
```

---

## 📈 Progress & Change Log

### Phase 1: Swarm Refactoring to ≤ 1,500 LOC (Completed)
- **Target**: Deconstruct large monolithic files exceeding 1,500 lines to optimize for LLM context windows and maintainability.
- **Results**:
  - `crates/fmlev-core/src/context.rs` (4,905 LOC) → modularized into `crates/fmlev-core/src/context/` (5 files, max 1,286 LOC).
  - `crates/fmlev-server/src/lib.rs` (2,449 LOC) → modularized into `crates/fmlev-server/src/routes/`, `state.rs`, `tests.rs`, and streamlined `lib.rs` (6 files, max 834 LOC).
  - Highest LOC across all `.rs` files is now 1,328 LOC (`crates/fmlev-core/src/models.rs`).
- **Verification**:
  - `cargo check --workspace --all-targets --examples`: 0 warnings, 0 errors.
  - `cargo test --workspace`: 108 tests passing (107 passed, 1 ignored, 0 failed).
  - `node --test crates/fmlev-server/tests/*.test.cjs`: 26 passed, 0 failed.
- **Initialized**: `AGENTS.md` guidelines established.

### Phase 2: Writable Club Details & Facilities, Safe Player Heal & Clear Promises (Completed)
- **Target**: Enable writable editor for Club Details & Facilities with transactional readback & rollback, and add Player Heal (100% Fitness) and Clear Promises actions.
- **Results**:
  - Fixed property string decoding bug across `live_club.rs`, `live_nation.rs`, and `live_competition.rs`.
  - Added dynamic 1..20 facility ratings extraction from `DynamicReference` (`0x3235_2020`).
  - Implemented `write_live_club` with atomic readback and rollback verification for `OverallBalance` and facility ratings.
  - Replaced crash-inducing injury pointer zeroing with safe 100% condition & match sharpness restoration (10,000 units), eliminating fatigue safely without corrupting simulation objects.
  - Added `remove_promises` action across engine, REST API, and web UI.
- **Verification**:
  - `cargo check --workspace --all-targets --examples`: 0 warnings, 0 errors.
  - `cargo test --workspace`: 108 tests passing.
  - `node --test crates/fmlev-server/tests/*.test.cjs`: 26 passed, 0 failed.
  - All files strictly $\le$ 1,328 LOC.

### Phase 3: Club Native Transfer Budget, Binding Diagnostics, Injury Crash Postmortem (Completed)
- **Club writes**: UI-binding writes are caches the game overwrites (verified in game: balance and transfer budget reverted). (Superseded: see the native finance object entry below.) Facility ratings and other club fields are rejected rather than reported as saved.
- **Removed**: the fake "Clear Promises" action (it only set condition/sharpness).
- **Diagnostics (read-only)**: `live_dump.rs` + examples `dump_live_bindings`, `probe_native_record`, `inspect_injury`.
- **Competitions**: native record found by vtable + index + unique ID (index at +0x8, uid at +0xc/+0x10, name pointers at +0x40/+0x48/+0x50, record stride 0x1c0). Rules/prize values are not in the UI bindings and not yet mapped.
- **NEVER edit a player's injury vector** (player + 0x248, begin +0x18 / end +0x20). A nulled/emptied vector for a player with a current injury crashes FM26 on the next day processing (`game_plugin.dll+0x33af065`, `mov rcx,[begin]`) and the damage is saved into the career. Repair used: point the vector at a healed record copied from another player's list, Continue, then Save As a new file.
- **Competition prize money (display-verified)**: `native_competition.rs` + `POST /api/competition/prizes` read/write the league prize table (an `i32` raw-GBP array, one amount per position, then a zero or unrelated word after it). The table is identified by team count and the first/last prize shown on Competition > Rules, with an optional raw-GBP `tolerance` for rounded displays (e.g. £53.1M); zero or several matches are rejected. Verified readable for the Super League (18 positions, exact) and the Premier League (20 positions, tolerance 60000). Edits show on the Rules page after leaving and reopening it. **Unconfirmed**: the array sits among UI model entries, so the season-end payout source is not proven; do not claim it changes real payouts until observed at season rollover.
- **Display currency**: raw club/competition money is GBP. The game's display currency is a setting (it was USD at x1.3508; now GBP, so screen numbers equal raw values).
- **Club balance and transfer budget (verified)**: native club record (vtable, index at +8, uid at +0xC/+0x10) -> finance object at +0x150 (back-pointer at +8). Balance = i32 at finance+0x14; available transfer budget = i32 at finance+0x7cc (allocated at +0x7d0). Verified in game for Persib and Real Madrid through `POST /api/club/money {club_id, balance?, transfer_budget?}` (empty body reads). Changes show after reopening Club > Finances; balance survives day ticks, month-end persistence unconfirmed. The heap `[allocated, available, .., balance]` block, UI bindings and the summary table are caches the game rewrites: never write them.
- **Squad numbers (verified)**: u8 at player contract + 0x5d (255 = none); contract team object at contract + 0x10. Verified against the squad screen for 11 players, a direct write, and an API swap (Kurzawa 3<->2 with player 21887). FM26 shows changes after the next day tick. No native uniqueness map exists, so `write_squad_number` swaps with the teammate (same contract team) wearing the number in one transaction with rollback. API: `POST /api/player/squad-number {player_id, number}` (0/null clears); the player's profile must be open.
- **Tactical familiarity (verified)**: per-player object at player record - 0x48 (null until created) with 8 vectors at +0x00/0x20/.../0xe0 (begin,end,cap). Seven hold u32 entries whose low 14 bits are the value (0..=10000); the eighth holds 16-byte per-formation entries with a u16 value at +0xe. Setting all to 10000 is exactly the game's own TEAM_MAXIMISE_EXISTING_TACTICAL_FAMILIARITY action (traced in game_plugin.dll); verified in game for one player and a 118-player squad. Team/player averages are derived. `POST /api/player/familiarity {player_id, squad}`.
- **Register anytime (verified)**: two parts. (1) Manager flag: bit 0x100 of the dword at X+0x404 on the human manager object (X = first element of the vector at [game_plugin+0x4e360f8]->+0x18, validated by HUMAN_NON_PLAYER vftable at X+0x450); enough for continental competitions. (2) Domestic leagues: competition record +0xb0 -> ACTUAL_COMP, [+0x208] stage, [stage+0x1e0] vector object (begin/end at +0x60/+0x68), stage index byte at ACT+0x31; entry+0x9a = 1 means registration currently allowed (entry+0xcd is the competition type). Verified in game for La Liga (Registration Open, Confirm enabled) and restored exactly. Not shared between competitions; rebuilt on new season/stage change, so re-apply. `POST /api/competition/registration {comp_index, comp_uid, enabled}`; disabling needs the original byte saved in the same server session. Squad-number-anytime is bit 0x200 of the same dword but its gate is not traced yet.
- **Competition rules/windows**: located as editor-format data nodes ('sqsr' lists of {type, cmty, valu}; windows as 'wnd0'/'wnd1' calendar events) but writing the node (La Liga max squad 25->26) did not change the Rules page, so they are source data, not the live copy. Nation reputation u16 at +0x1e2 of the nation record did not change the displayed label. Neither has an editor.
- **Contract recon (Stage 0)**: contract record at player +0x1e8: team object at +0x10/+0x18 (a *team* such as first team or B team, not the club; teams have their own index/uid), wage u32 at +0x20, expiry u32 at +0x48 (year<<16 | flags 0xfe00 | 1-based day of year), squad number +0x5d, a second embedded block at +0xc8 (probably national team). A loaned-out player's +0x1e8 contract is the unchanged parent contract; loan state lives in separate deal records not yet mapped (so move/loan stages are not implemented). Diagnostics: examples `inspect_contract`, `find_refs`.
- **Extend contract**: `POST /api/player/contract/extend {player_id, years?}` writes expiry year + N (readback, rollback); verified in memory. It extends the parent contract for loaned players.
- **Terminate**: NOT implemented. Setting expiry to the current game date did not release a player after the day passed (FM26 releases through scheduled events). `POST /api/player/contract/terminate` returns 501.
- **Sign / loan (investigated, NOT implemented)**: snapshot diff of a free-agent signing (player 36392 -> R. Madrid U19s): the record's +0x1e8 contract pointer went from null to a freshly allocated contract object (plus new pointers at +0x1d8/+0x1e0/+0x250 and ~50 new references to the person object). A loan-out (Endrick -> Lyon) leaves the parent contract untouched and adds a pointer at record+0x1f0 to a node whose first qword points to a loan-agreement object: vtable (RTTI), person pointer at +0x8, loan team object at +0x10/+0x18, dates around +0x44..+0x4c (start/end words in the contract date format). These objects are allocated by the game; memory writes cannot create them, and a pointer swap of an existing contract between teams is unverified (needs squad-list mapping). `POST /api/transfer/sign` and `/api/transfer/loan` return 501. Diagnostic: example `snapshot_player` (before/after diff).
- **Pending transfers (investigated)**: an arranged permanent transfer (Cortina, Levante -> R. Madrid, joining 30/12/2025) leaves the contract and team pointer untouched; the only record change is a new pointer at player record +0x1e0 to a game-allocated agreement object (also set for the free-agent signing). The join date is not stored in that object, so completing a pending deal early by editing a date is not possible with what is mapped. Scanner: `tools/fmscan` (C++, read-only; snap/diff with noise filter/refs/find/dump).

### Phase 4: Read-only C++ Discovery Tools (Completed)
- Added `struct`, `vec`, `rtti`, `graph`, `watch`, `diffset` and `--json` for all scanner commands. Existing snapshot layout and diff filters remain supported.
- Split scanner into focused headers, all below 1,500 LOC; no target-process write API. Vector stride, date shapes and nearby RTTI ownership remain explicitly heuristic.
- Verification: 9 synthetic process tests pass with a warnings-as-errors C++ build; workspace check passes; Rust tests 109 passed, 1 ignored; UI tests 32 passed.
- Read-only FM26 proof: manager-list vector decoded and known manager vtable resolved to `.?AVHUMAN_NON_PLAYER@db@@`, including Proton anonymous PE sections. No live writes or new transfer offsets validated.
- Usage and limitations: `tools/fmscan/README.md`. Sign/loan/terminate experiments remain separate pending work.

### Phase 5: Screen-independent Native Entity Search (Completed)
- Added `fmscan entity PID player|club|nation|competition --name TEXT [--related] --json`, with index/UID selectors. Native vtables come from DLL RTTI; names and linked records come from native memory, with no open-screen/UI-binding requirement.
- Related player chain validates parent-contract ownership, team, club, nation and domestic competition identities. Club searches validate their team-vector bounds and team-owner back-pointer. Missing/invalid links remain explicitly unavailable; loan destinations are not inferred.
- Live proof without opening game screens: Lamine Yamal Nasraoui (40629/2000256231), Barcelona (1074/1708), Spain (170/796), Spanish First Division (51/67). Independent full scans agreed with the pointer chain. No live writes.
- Verification: 17 synthetic process tests, warnings-as-errors C++ build, workspace check, Rust 109 passed/1 ignored, and UI 32 passed. All new source files remain below 1,500 LOC.

### Phase 6: Native Player Editor Dump and Matched Benchmark (Completed)
- `entity ... player --editor` prints the 60 native editor attributes, positions/feet, CA/PA, names/identity and supported contract/context fields. Unsupported fields remain unavailable; no UI defaults or screen navigation.
- Added guarded read-only `player-data` diagnostics in C++ and a Rust example using the actual memory driver and production attribute decoder. Snapshot decoding does not calibrate or authorize writes.
- Matched optimized benchmark (seven alternating trials, warmups/build excluded): full identity scan C++ 919.67 ms / Rust 799.39 ms; located-record read/decode C++ 8.53 us / Rust 9.67 us. Payloads/checksums and scanned workload matched. This is not the HTTP/UI path or a universal language result.
- Verification: 21 process tests, warnings-as-errors C++ build, workspace check, Rust 109 passed/1 ignored, and UI 32 passed. Live Yamal CA/PA 175/194 and all 60 attributes agree between readers; no live writes or screenshot/save-reload proof.

### Contract and Transfer Discovery Tools (Completed read-only stage)
- Added C++ `contract`, `transfer` and `transfer-watch` commands for native identity-based inspection, absent/invalid contract diagnostics, agreement/loan roots, current-team/squad candidates, inbound references and changed snapshots. No target write path was added.
- Public source research: FMSuperScout, fm-ai-assistent, FMScoutFramework and archived Cheat Engine research in fm26rte. The latter supplies multi-object termination and squad-list leads; it is not validated write logic for this save. Findings and pinned links: `tools/fmscan/CONTRACT_RESEARCH.md`.
- Live read-only proof: Yamal's parent/current club is Barcelona; Endrick's parent contract is R. Madrid, current-team candidate is Lyon and the indirect object is LOAN_CONTRACT with a matching person pointer. Exact Endrick pointers occur in both parent and current squad vectors; do not assume single-list ownership.
- Verification: 10 new synthetic contract/transfer tests; full current C++ suite 42 passed with warnings-as-errors builds, workspace check passed, Rust 109 passed/1 ignored, UI 32 passed. New source files remain below 1,500 LOC. No live sign/loan/terminate action or save/reload proof; those writes remain unresolved.

### Native Club, Nation and Competition Editor Reads (Completed read-only stage)
- Extended C++ `entity ... --editor` to clubs, nations and competitions: facilities/finance/ground data, nation gender-specific youth/reputation and economic factor/raw development code, competition reputation and stage registration gate. New numeric layouts require known PE timestamp 0x6a229e8a plus native type/identity/ownership/range guards.
- Native serialization code identifies the new field offsets. Main stadium and training ground are distinct; men's/women's values remain separate; native competition reputation is 1..200. Unsupported editor fields stay null with reasons. No extra Rust/UI write support.
- Live screen-independent proof: Barcelona training 19/youth 20/corporate 19, Camp Nou 62500/105000 capacity/expansion; Spain men youth 145/women 160, reputations 9011/9300; Spanish First Division reputation 178 and stage gate 0. Three native readbacks agreed, with no game-memory writes.
- Verification: 12 new entity tests, current combined suite 43 passed, workspace check passes, Rust 109 passed/1 ignored, UI 32 passed. Coverage, evidence and remaining WIP: `docs/NATIVE_ENTITY_READS.md`.

### Phase 7: Porting Verified Native Club, Nation and Competition Writes to Rust (Completed)
- Ported verified native layouts from C++ (`tools/fmscan/entity_details.hpp` and `write_transaction.hpp`) into pure Rust `crates/fmlev-core`:
  - `native_club.rs`: Expanded `NativeClubRecord` to read/write details facilities (training +0x118, youth +0x123, junior coaching +0x124, youth recruitment +0x125), corporate facilities (+0x8b5), reputations (main-team +0xa8, world +0x166), and stadium capacities (+0x7c/+0x8c) with full transactional rollback.
  - `native_nation.rs`: Added native nation discovery, dual men's/women's container resolution (+0x108/+0x110), container youth rating (+0x864, u16 1..200), national team reputation (+0xa8), economic factor (+0x1f0), and development code (+0x1ec).
  - `native_competition.rs`: Added `NativeCompetitionRecord` to read/write native competition reputation (+0x188, u16 1..200) with scale mapping and rollback.
  - `context/mutations.rs` & `live_club.rs`: Wired native club facilities, nation details, and competition reputation into `GameContextManager` live mutations.
- Verification:
  - `cargo check --workspace --all-targets --examples`: 0 warnings, 0 errors.
  - `cargo test --workspace`: 116 tests passing (115 passed, 1 ignored, 0 failed).
  - `node --test crates/fmlev-server/tests/*.test.cjs`: 32 passed, 0 failed.
  - All source files strictly $\le$ 1,328 LOC.

### Phase 8: Native Contract Termination, Loan Recall, and Player Moves (Completed)
- **Investigation & Memory Verification on PID 5945**:
  - Investigated native contract and transfer data structures on live FM26 PID 5945 using C++ `fmscan` and Python memory analysis.
  - Inspected real free agents (Brad Guzan 2888/980785 and others) and confirmed exact native state: `record + 0x1e8 == 0` (contract absent), `ACTUAL_PLAYER + 0x130 == 0` and `+ 0x138 == 0` (current team absent), `record + 0x1e0 == 0` (agreement absent), `record + 0x1f0 == 0` (loan absent), and zero appearances in any team squad vector (`team + 0x38`).
  - Heap-wide reference scans across all `rw-p` memory regions confirmed that only a single pointer in the entire process references a player's `FULL_CONTRACT` object (the player's own `record + 0x1e8` slot). No global contract registry or iteration vector exists.
  - Squad vector analysis across 4,890 live teams revealed 4,346 teams have `end == capacity` while 544 have spare capacity. Removing a player and compacting remaining elements (`end -= 8`) never requires reallocation. Appending to a destination squad vector (`append_patches`) safely requires `spare_slots > 0` (`end < capacity`), preventing out-of-bounds heap corruption.
- **C++ Discovery Enhancements (`tools/fmscan/`)**:
  - `entities.hpp`: relaxed `read_entity` team duplicate UID check so reserve / youth / U19 teams (which have `uid != uid2`) resolve properly, linking youth players (e.g. Cortina at Levante U19s) to their parent club.
  - `contracts.hpp`: squad membership candidate now extracts `capacity_count` and `spare_slots`.
- **Rust Implementation (`crates/fmlev-core/` and `crates/fmlev-server/`)**:
  - `native_transfer.rs`: Implemented `SquadVector` (bounds checking, element reading, compaction removal, append), `NativeEdit`, and transactional atomic writes with readback and full reverse rollback.
  - Added `terminate_contract`: detaches parent contract, clears team pointers (`0x130`/`0x138`), clears agreements and loans, and removes player from all squad vectors.
  - Added `terminate_loan`: removes player from loan team squad vector, restores current team pointers to parent team, and clears loan node.
  - Added `move_player`: verifies spare capacity in destination squad vector, removes from source squad vector, appends to target squad vector, updates contract team pointers (`contract + 0x10`/`0x18`) and player team pointers (`0x130`/`0x138`).
  - Added REST API routes: `POST /api/player/contract/terminate`, `POST /api/player/loan/terminate`, `POST /api/transfer/move`, and informative `POST /api/transfer/sign`.
  - Added UI controls and action buttons in `ui.html` for contract termination, loan recall, and club transfer.
- **Verification**:
  - `cargo check --workspace --all-targets --examples`: 0 warnings, 0 errors.
  - `cargo test --workspace`: 120 tests passing (119 passed, 1 ignored, 0 failed).
  - `node --test crates/fmlev-server/tests/*.test.cjs`: 32 passed, 0 failed.
  - `python3 -m unittest discover -s tools/fmscan/tests -v`: 57 tests passed, 0 failed.
  - All source files strictly $\le$ 1,223 LOC.

### Phase 9: Native Club Facilities, Nation Macro/Youth, and Competition Live UI Wiring (Completed)
- **Web UI & Connected Mode Gating (`crates/fmlev-server/src/ui.html`)**:
  - Connected mode decoupling: decoupled `club-facilities-card`, `nation-youth-card`, `nation-macro-card`, and `competition-reputation-card` as depth-0 direct children with `data-live-editor="true"`.
  - Live club facilities: enabled inputs for training, youth, junior coaching, youth recruitment, and corporate facilities; unverified data analysis facility remains disabled.
  - Live nation macro & youth: enabled youth rating slider (1–200), reputation input, state of development, and economic factor controls; unverified FA power/tax/corruption controls remain disabled.
  - Live competition reputation: added native reputation input and reactive star rating updater (1–200 native scale).
  - Strict partial write contract: connected-mode saves explicitly declare only verified edited fields, preserving unedited baseline fields to prevent transaction rollback rejections.
- **Test Suite Updates**:
  - `live-editors.test.cjs`: verified direct depth-0 hierarchy for all 7 live cards and verified endpoint and function bindings.
  - `entity-audit.test.cjs`: verified live editor controls remain enabled while unverified controls remain disabled in connected mode.
- **Verification**:
  - `cargo check --workspace --all-targets --examples`: 0 warnings, 0 errors.
  - `cargo test --workspace`: 120 tests passing (119 passed, 1 ignored, 0 failed).
  - `node --test crates/fmlev-server/tests/*.test.cjs`: 37 passed, 0 failed.
  - `python3 -m unittest discover -s tools/fmscan/tests -v`: 57 passed, 0 failed.
  - All source files strictly $\le$ 1,328 LOC.

### Phase 10: Free Agent Signing and Pending Transfer Join Date Acceleration (Completed)
- **Frontier 1 — Free Agent Signing (`POST /api/transfer/sign`)**:
  - Dissected exact native layout of `FULL_CONTRACT` (size 200 bytes / 0xc8):
    - Vtable: `game_plugin.dll + 0x4334df8` (`0x6fffdba64df8`), resolved dynamically via `resolve_contract_vtable`.
    - Person back-pointer at `+0x08`: validated by RTTI `.?AVFULL_CONTRACT@db@@` and person owner comparison (`actual_player + 0x288`).
    - Parent team pointers at `+0x10` and `+0x18`: points to target `.?AVTEAM@db@@`.
    - Weekly wage (GBP) at `+0x20` (u32).
    - Contract subtype at `+0x24`: `0x0001000b`.
    - Packed expiry date at `+0x48`: `(year << 16) | 181`.
    - Squad number at `+0x5d` (255 = none, or 1..99).
    - Null clause vectors at `+0x68` and `+0x80`: cross-scan across 2,000 contracts proved that 365 native contracts have null `(0, 0, 0)` vectors, establishing that auxiliary heap vector allocations are strictly optional.
  - Developed `find_zero_contract_slot`: safely locates an aligned 200-byte block of pure zeros in the entity mapping (`0x138980000..0x138990000`) in ~30 microseconds.
  - Implemented `sign_free_agent`: validates player is an unemployed free agent, verifies destination team has spare capacity (`spare_slots > 0`), populates the 200-byte contract block, points `record + 0x1e8` to the contract, points `ACTUAL_PLAYER + 0x130`/`0x138` to the target team, and appends the player to the target squad vector with atomic readback and reverse rollback.
  - Live game proof on FM26 (PID 5945): Brad Guzan (2888/980785) signed to FC Lorient (2005) at 5,000 GBP/week, 2027 expiry; verified by `fmscan contract` (`rtti_and_person_back_pointer_validated`) and `fmscan transfer` (squad member matched), then restored 100% cleanly.
- **Frontier 2 — Pending Transfer Join Date Acceleration (`complete_pending_transfer` & `accelerate_pending_transfer`)**:
  - Traced arranged transfer pointer chain: `record + 0x1e0` -> agreement vector `0x3bd677f70` -> entry `0x136591288` -> future `FULL_CONTRACT` at `+0x10` (`0x1388da468` for Borja Cortina, Levante -> Real Madrid).
  - Whole-heap date search located the game's **Transfer Event Queue** at `0x2603fd...`: a chronologically sorted priority queue with stride 0x30 holding `{tag, id, person_ptr, packed_join_date, transfer_id}`.
  - Implemented `complete_pending_transfer`: instantly switches `record + 0x1e8` to the pre-allocated future contract, clears `record + 0x1e0`, updates `ACTUAL_PLAYER` team pointers, removes from source squad vector, appends to target squad vector, and clears the transfer queue entry.
  - Implemented `accelerate_pending_transfer`: modifies the scheduled join date in the future contract (+0x44) and in the transfer queue entry (+0x10), accelerating join scheduling.
  - Live game proof on FM26 (PID 5945): Accelerated join date for Borja Cortina, verified with `fmscan transfer`, and verified exact reverse rollback.
- **REST API Wiring**:
  - `POST /api/transfer/sign`: accepts `{ player_id, target_club_id?, target_team_address?, wage_gbp?, expiry_year? }`.
  - `POST /api/transfer/accelerate`: accepts `{ player_id, new_join_date?, instant? }`.
- **Verification**:
  - `cargo check --workspace --all-targets --examples`: 0 warnings, 0 errors.
  - `cargo test --workspace`: 118 Rust tests passed (117 passed, 1 ignored, 0 failed).
  - `node --test crates/fmlev-server/tests/*.test.cjs`: 37 passed, 0 failed.
  - `python3 -m unittest discover -s tools/fmscan/tests -v`: 57 passed, 0 failed.
  - All source files strictly $\le$ 1,251 LOC (hard limit: 1,500).

### Phase 10: Squad-Number-Anytime Domestic Gate and Active Competition Stage Rules (Completed)
- **Frontier 3: Squad-Number-Anytime Gate Reversal**:
  - Reverse engineered squad number gate in `game_plugin.dll`:
    - Disassembled rejection check at `0x182e2af79`: after testing stage flags (`+0x58` bit 1), max squad number (`+0x60`), and window dates, the game checks human manager `manager + 0x404` bit `0x200` (`test byte ptr [rax + 0x405], 2`).
    - Bit `0x200` is the authoritative universal override: when set, FM26 jumps directly to `0x182e2ae8a` and executes squad number submission `0x182e3b570`, completely bypassing closed domestic windows.
    - Traced all 5 checking sites (`+0x2e2af7d`, `+0x2f2fff1`, `+0x2f300b1`, `+0x2f30170`, `+0x2e2a0ce`) across edit sessions and domestic squad handlers.
  - Implemented `SQUAD_NUMBER_BIT = 0x200` and `set_squad_number_anytime` in `native_registration.rs` with transactional readback verification and rollback.
  - Unified registration anytime to enable both registration (`0x100`) and squad numbers (`0x200`) on the manager alongside domestic stage gate `+0x9a`.
- **Frontier 4: Active Competition Stage Rules & Live Simulation Engine**:
  - Traced complete property getter for live competition stage rule entry at `game_plugin.dll + 0x22e7410`, driven by a 77-entry jump table at RVA `0x45eaddc` in `.edata`.
  - Discovered live simulation rule layout in `entry` (`comp + 0xb0 -> act + 0x208 -> stage + 0x1e0 -> vec[stage_index]`):
    - `+0x98`: live squad registration cap (`u8`, 25 in La Liga/EPL/Serie A, 99 in Bundesliga/Scotland/France).
    - `+0x99`: homegrown player quota (`u8`, 8 in La Liga/EPL/Serie A/Bundesliga, 255 if unconstrained).
    - `+0x9a`: stage registration gate (`u8`, 1 = open, 0 = closed).
    - `+0x9b`: max squad number cap (`u8`, 49 in Bundesliga [DFL cap], 255 = standard).
    - `+0xcd`: competition type (`u8`, 8 = domestic league).
    - `+0xd6`: secondary squad registration cap (`u8`, synchronized with `+0x98`).
  - Distinguished live rules engine from static database source:
    - Editor-format `'sqsr'` and `'ydtm'`/`'strl'` nodes (at `act + 0x38`, `stage + 0xd8..0x3a0`) are static database snapshots read once at game creation.
    - Stage rule entry at `vec[stage_index]` is the active, dynamic structure queried during match setup and registration processing.
  - Ported stage squad rules to pure Rust in `crates/fmlev-core/src/native_competition.rs`:
    - Added `NativeStageSquadRules`, `stage_rule_entry`, `read_stage_squad_rules`, and `write_stage_squad_cap` with atomic dual-location updates (`+0x98` and `+0xd6`) and rollback on mismatch.
  - Updated C++ scanner `tools/fmscan/entity_details.hpp` to extract `max_squad_size` and `min_homegrown_nation` with field evidence.
  - Live verified on PID 5945: tested transactional write and readback of squad cap on La Liga (`0x2623a27e0`), restoring values cleanly.
- **Verification**:
  - `cargo check --workspace --all-targets --examples`: 0 warnings, 0 errors.
  - `cargo test --workspace`: 123 tests passing (122 passed, 1 ignored, 0 failed).
  - `node --test crates/fmlev-server/tests/*.test.cjs`: 37 passed, 0 failed.
  - `python3 -m unittest discover -s tools/fmscan/tests -v`: 57 passed, 0 failed.
  - All source files strictly $\le$ 1,076 LOC.

### Phase 11: Frontier 5 — Unmapped Club & Nation Memory Fields (Completed)
- **Data Analysis Facilities**:
  - Traced Club Details (`cint = club + 0x100`) offset `+0x11e` (`u8`, range 1..=20) to native serializer RVA `0xea7123` with FourCC tag dispatch `0x4379696d` ("Cyim" at `0xea6aeb`).
  - Live verified on PID 5945: Barcelona = 18, Real Madrid = 17, Atletico Madrid = 16, Europa Barcelona = 12, Man City = 14.
  - Implemented transactional read/write with readback and rollback in `native_club.rs`, wired to `write_live_club`, enabled in `LIVE_CLUB_WRITABLE`, and exposed in UI.
- **Sugar Daddy Benefactor Status**:
  - Traced `CLUB_FINANCE` (`club + 0x150`) offset `+0x3c` (`u8`, enum 0..=3: 0 = None, 1 = Foreground, 2 = Background, 3 = Underwriter) to native serializer RVA `0x1636ea7` (`movsbl 0x3c(%rsi), %ebx`).
  - Live verified on PID 5945: Man City = 1, PSG = 1, Chelsea = 3, Barcelona = 0, Real Madrid = 0.
  - Implemented transactional read/write with readback and rollback in `native_club.rs`, wired to `write_live_club`, enabled in `LIVE_CLUB_WRITABLE`, and exposed in UI.
- **Nation Population**:
  - Traced `nation` record offset `+0x1e4` (`u32` / DWORD, range 1..=2,000,000,000).
  - Live verified on PID 5945: Spain (796) = 49,570,725; England (765) = 57,690,300; Brazil (1651) = 211,998,573; Germany (771) = 83,516,593; France (769) = 68,551,653; Andorra (753) = 81,938; San Marino (792) = 33,977.
  - Implemented transactional read/write with readback and rollback in `native_nation.rs`, added `population` to `NationDetails` across model, default registries, inspector, and UI.
- **Verification**:
  - `cargo check --workspace --all-targets --examples`: 0 warnings, 0 errors.
  - `cargo test --workspace`: 123 Rust tests passed (122 passed, 1 ignored, 0 failed).
  - `node --test crates/fmlev-server/tests/*.test.cjs`: 37 passed, 0 failed.
  - `python3 -m unittest discover -s tools/fmscan/tests -v`: 57 passed, 0 failed.
  - All modified source files strictly $\le$ 1,333 LOC (hard limit: 1,500 LOC).

### Phase 11: In-game Verification of Native Editors and Stale-Cache Save Bug (Completed)
- **C++ `fmwrite`** (`tools/fmscan`, opt-in, plan -> apply -> restore, allowlisted scalars, `--experimental` gate) committed with tests (14 write tests; 57 C++ tests total). Live use: club training facilities 18 -> 10 on R. Madrid (club 1089/1736): label changed to "average", survived a day tick and save/reload. One unintended R. Madrid balance apply/restore happened during a check; restored byte-exact, not screen-checked.
- **Bug found by in-game test (and fixed)**: the web UI filled facility sliders from binding caches and fallback defaults (e.g. recruitment 0 -> 18, corporate cache 20), and Save wrote every control, changing untouched native fields (corporate 19 -> 20, recruitment 19 -> 18). Also the live sync re-applied binding-cache ratings over native values each tick. Fixes:
  - `write_live_club`/`update_live_nation_checked`/`update_live_competition_checked` write only fields that differ from the loaded copy; unchanged fields keep the native value.
  - New read-only routes `POST /api/club/native`, `/api/nation/native`, `/api/competition/native` load verified native values into the editors once per entity (and again after a save). Clubs read natively are excluded from binding-cache overwrites (`native_synced_clubs`).
  - Live presets (+$50M, Sugar Daddy, Wipe Debt, Golden Generation, Relax Rules) are refused on live sessions (they wrote UI-binding caches); Max Facilities goes through the native club path and leaves data analysis alone.
  - Limitation: setting a control to exactly the loaded value writes nothing.
- **Discovery fixes**: nation discovery now requires a validated national-team container (index/uid pattern also matched 19 other record types); competition discovery is pinned to the `.?AVCOMP@db@@` vtable (`game_plugin.dll` + 0x44bfe68, same RVA style as the manager vtable) and a structure check. Diagnostic example `list_id_candidates <id>` lists same-id records with vtables.
- **Verified in game (screen + day tick)**: club facilities via UI (Youth 19 -> 15 shows "Great youth facilities"; Training to 20 shows "State of the art"); competition reputation via UI/API (178 -> 120 still "Good"; 20 shows "Poor"; restored 178); Register Anytime (user-confirmed).
- **Written and persisting, no on-screen label to confirm**: nation men's youth rating (145 -> 165, persisted after Continue).
- **Not yet verified in game**: nation reputation/economic factor/development/population, club reputation and stadium capacity, `fmwrite` nation/competition paths, save/reload for the UI-driven writes, and **all Phase 8 and Phase 10 transfer actions** (terminate contract, loan recall, move, free-agent signing, pending-transfer acceleration). Those actions null/redirect pointers or create game objects by raw writes, which Rule 7 forbids until proven; the README rows are marked unverified.
- **Verification**: cargo check clean; cargo test passing; Node 37 passed; C++ 57 passed with `-Werror`.

### Phase 11b: Swarm UI/Route Audit and Transfer Tests (Completed)
- **Transfer actions verified in game on a throwaway save** (screen check + day tick, no crash): free-agent signing (Tommaso Carrozza -> Las Palmas), terminate contract, move (Las Palmas -> S. Gijón B). Squad vectors are mostly full (4,346 of 4,890 teams), so signing/moving only works into teams with spare slots (`examples/list_spare_teams`); the route refuses cleanly otherwise. Loan recall and pending-transfer acceleration remain untested. Save/reload for the transfer actions was not separately confirmed.
- **Fixed from the audits**: rollback accounting in `apply_transaction` (a leading no-op edit shifted the rollback range; regression test added), swallowed squad-vector errors in terminate/loan/move now propagate, `SquadVector::read` pins the TEAM vtable (RVA 0x44c13a8), sign validates wage/expiry year, `deny_unknown_fields` on the mutating request structs, loan stub returns 501, benefactor type and nation population are never written live (no verified round trip), registry money is no longer overwritten by binding caches for natively read clubs, saves re-read native values afterwards, UI: tolerant JSON error handling, accurate save toasts, undefined adjust* handlers defined, fabricated fallback club removed, transfer buttons disabled until attached with a profile open, money save sends only changed fields, squad number rejects empty input, dirty flags reset on entity change and ignore money/prize/affiliate inputs.
- **Known issues (not fixed, from the audits)**: handlers hold the global write lock during full-process scans, so polling can stall other requests; raw `write_memory`/heal/finances routes have no ownership or rollback guard; `attach` accepts a malformed body; terminate and sign write pointers/objects the game also references elsewhere (club registrations, transfer lists), a crash risk that day-tick tests so far did not show; `find_zero_contract_slot` and the transfer-queue lookup use run-specific hardcoded address ranges; register-anytime disable clears both manager bits regardless of the original; player-tab controls without a native path (release clause, happiness, name/age) are enabled and fail the whole save; god-mode presets are always enabled; update_* routes return 500 for validation errors. Treat the transfer actions as experimental and use a backup save.
- Overlay plan: `docs/OVERLAY_PLAN.md`.

### Phase 12: Screen-Independent Native Player Discovery, CA/PA, 60 Attributes & Opposition Player Editing (Completed)
- **Target**: Enable screen-independent reading and editing of CA/PA, all 60 native attributes, positions, contract details, and club associations for ANY player in FM26 (including opposition players, scouted players, and free agents without requiring their profile to be open on screen in FM26).
- **Root Cause**:
  - `verified_native_player(id)` in `crates/fmlev-core/src/context/mutations.rs` previously relied entirely on `read_person_native_fingerprint`, which reads Unity/IL2CPP `BindingSubsystem`. Opposition players not open on screen in FM26 have no `UniqueId`/`Footedness` in `BindingSubsystem`, returning `Error: InvalidValue("Open the player's profile: required UniqueId binding unavailable")`.
  - `verify_attributes` previously required `observed` attributes from UI bindings to match native memory. If `observed` was empty or masked, `verify_attributes` failed, leaving `attributes_verified = false`.
  - With `attributes_verified = false`, reading CA and PA was blocked, leaving CA/PA at 0 (`—`), and player writes were rejected with `"Open this player's Attributes screen in FM26"`.
  - In addition, the running server process had not been restarted with the latest release binary.
- **Implementations**:
  - `crates/fmlev-core/src/native_player.rs`:
    - Added `discover_by_index(driver: &LinuxMemoryDriver, index: i32) -> Result<Self>`: Scans `rw-p` memory for `index` at `0x148`, validates duplicate UID at `0x14c`/`0x150`, validates readonly/executable player and person vtables, 15 positions in `1..=20`, and scaled feet in `5..=100`.
    - Added `verify_native_attributes(driver: &LinuxMemoryDriver) -> Result<()>`: Directly decodes and validates all 60 native attributes in `1..=20` and checks CA/PA in `0..=200`, setting `attributes_verified = true`.
    - Updated `verify_attributes`: If observed UI bindings are empty/masked, automatically falls back to `verify_native_attributes(driver)`.
    - Updated `refresh_player_fields`: Decodes contract `team -> club` pointer chain (`contract + 0x10 -> team + 0x30 -> club`), resolving authoritative `club_id` and native `club_name` automatically.
  - `crates/fmlev-core/src/native_club.rs`:
    - Added `read_name(&self, driver: &LinuxMemoryDriver) -> Option<String>` to read length-prefixed UTF-8 strings from `club + 0xc0`.
  - `crates/fmlev-core/src/context/club_money.rs`:
    - Updated `read_club_native`: If the club is not yet in `club_registry`, instantiates template `ClubFinances`, populates native name via `record.read_name(driver)`, balance, transfer budget, all 6 facilities, reputations, stadium capacities, and sugar daddy status.
  - `crates/fmlev-core/src/context/mutations.rs`:
    - Updated `verified_native_player`: Tries live bindings first; if absent, cleanly falls back to screen-independent `NativePlayerRecord::discover_by_index(&session.driver, id)`.
  - `crates/fmlev-core/src/context/mod.rs`:
    - Updated `select_player(id)`: In live FM26 mode, if not yet in `player_registry`, creates template player and resolves native record, refreshing positions, all 60 attributes, CA, PA, and native name. Automatically loads the player's club natively via `select_club(player.club_id)`.
    - Updated `select_club(id)`: In live FM26 mode, automatically invokes `read_club_native(id)` so opposition clubs are discovered and populated natively.
  - `crates/fmlev-server/src/ui.html`:
    - Updated `jumpToPlayerClub()`: Directly checks if `state.activePlayer.club_id` is present (> 0) and invokes `selectSearchedClub(clubId)` directly.
- **Verification**:
  - `cargo check --workspace --all-targets --examples`: 0 warnings, 0 errors.
  - `cargo test --workspace`: 124 Rust tests passed (123 passed, 1 ignored, 0 failed).
  - `node --test crates/fmlev-server/tests/*.test.cjs`: 37 passed, 0 failed.
  - `python3 -m unittest discover -s tools/fmscan/tests -v`: 57 passed, 0 failed.
  - All modified files strictly $\le$ 1,374 LOC (hard limit: 1,500 LOC).
  - Live game tests on PID 6152:
    - Maarten Paes (21493): Discovered screen-independently with CA 129, PA 140, all 60 attributes, AFC Ajax (574). Live write: handling 11 -> 12, CA 129 -> 130 succeeded, read back verified, restored cleanly to 11 / 129.
    - Ole Romeny (21787): Discovered screen-independently with CA 110, PA 124, Finishing 9, Pace 12, Oxford United (386), balance £1,046,570, transfer budget £687,792.


### Phase 13: Display Settings (Plan A) (Completed)
- `fmlev-core/src/settings.rs`: `DisplaySettings {currency, measurement, wage_period}`, fixed dated rate table (no custom/live rates), persisted to `${XDG_CONFIG_HOME:-~/.config}/fmlev/settings.json`; missing file = GBP/metric/weekly, corrupt file = error. `GET/POST /api/settings` (`deny_unknown_fields`, unknown currency = 400).
- `ui.html`: single formatter/parser (`formatMoney`, `setMoneyInput`/`readMoneyInput`, height/weight/distance), Settings tab, no hardcoded currency symbols. Money inputs (`data-money`) keep the raw native baseline, so an untouched control returns the exact native value. Club wage budget / max wage labelled "raw period unverified" and never period-converted.
- Not done: reading FM26's own display currency (A4, discovery only); height/weight/distance formatters exist but no screen shows those fields yet.
- Verification: cargo check clean; cargo test passing; Node 42 passed. Display only, no game-memory writes, so no in-game proof needed.
