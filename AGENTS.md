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
