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
