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
│       ├── locator_cache.rs           # Validated cache of native record locations (scan once, reuse after identity check)
│       ├── native_catalog.rs          # RTTI enumeration for the search index
│       ├── native_contract_terms.rs   # Contract squad status and release clause (read verified, write experimental)
│       ├── native_staff.rs            # Staff records: attributes, information, contract
│       ├── native_transfer.rs         # Squad vectors, contracts, transfers, map-based slot/queue lookups
│       ├── open_staff.rs              # Staff profile open in FM26 (from UI bindings)
│       ├── search_index.rs            # In-memory search index (name, club for staff)
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
        ├── ui/                 # Manifest-driven HTML, CSS and classic JS sources
        └── ui_bundle.rs        # Single embedded HTML response assembly
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

### Phase 14: Role Presets (Plan B) (Completed, in-game proof pending)
- `fmlev-core/src/role_presets.rs`: 13 roles (incl. a goalkeeper role) and 3 levels (floors key/secondary/position: 14/12/15, 16/14/18, 18/16/20). Proposals only raise values; a field reading 0 (not read from the game) refuses the whole proposal. Level 3 defaults to CA `raise_to_fit`, levels 1-2 to `keep`; user can override per request.
- `POST /api/player/role-preset {player_id, role_id, level, ca_mode?}` returns a field diff and writes nothing; `GET /api/roles` lists roles. UI: Positions tab card previews the diff, stages it into `state.activePlayer` (dirty fields only for proposed fields) and the normal player Save writes it (verified native path, rollback).
- **Not proven**: (1) the role names and key/secondary lists are the standard FM role model, NOT compared with FM26's role screens (plan B1); every proposal and the UI badge say "role data unverified". (2) The CA increase (1.5 CA per attribute point) is a heuristic, not the game's formula, and is labelled as an estimate. (3) The in-game proof (apply DLP level 2/3 on a throwaway save, check Attributes/Positions after a day tick, save/reload) has not been done.
- Verification: cargo check clean, cargo test passing, Node 45 passed.

### Plan C discovery log: nationality rules (read-only, 2026-10-26, NOT yet verified in game)
- Method: disassembled the NATION serializer in `game_plugin.dll` (image base 0x180000000, RVA 0xf7c600..) and read all 251 nation records from PID 5690 with `fmscan dump`. No writes.
- Candidate layout (nation record): byte `+0x1ed` = years to gain nationality (tag `Nygn`, store at RVA 0xf7cfde); byte `+0x1eb` = gain-nationality type (tag `Ngct`, store at RVA 0xf7cbf5; live values only 1 and 2, 128 vs 123 nations); bit `0x02` of byte `+0x1e9` (word `+0x1e8` bit `0x200`, tag `Nndn`, RVA 0xf7d483 -> 0xf7da09) = does NOT allow dual nationality (set for 12 nations incl. Japan, China PR, India, Indonesia, Kazakhstan, Cuba, Latvia, Ukraine).
- Live values: Spain years 10, Italy 10, England 5, Germany 5, Brazil 4, Ghana 6, Andorra 20, San Marino 30, Japan 5; years distribution includes 0 (historic nations), 99 and 255 (12 nations).
- **Not proven**: that these bytes are what the Nation screen shows (needs screen comparison), the type enum labels (1 vs 2 = Accumulative/Continuous order unknown), the meaning of 255/99, and that edits change in-game behavior. No Rust/UI work done yet.

### Plan D/E/F discovery log (2026-10-27, static only, NOT verified)
- FM26 (PID 5907) was at the main menu with no save loaded: `fmscan entity` returned 0 validated records for nations and players, so no live reads were possible. No code changed.
- Static lead from `strings` on `game_plugin.dll` (RTTI names, `db` namespace): staff record candidates `.?AVACTUAL_NON_PLAYER@db@@` (like ACTUAL_PLAYER), `.?AVNON_PLAYER_TENDENCY_TEMPLATE@db@@` (staff tendencies), `.?AVJOB@db@@`, `.?AVSTARTING_CLUB_STAFF_JOB_STATUS@db@@`, `.?AVEXTERN_STAFF_NUMBERS@db@@`; eligibility leads `.?AVWORK_PERMIT_EXEMPTION_INFO@db@@`, `.?AVMAX_PLAYERS_MOVED_BETWEEN_MAIN_CLUB_AND_AFFILIATE_CLUB_INFO@db@@`. No RTTI name mentions preferred moves, so that layout needs a live diff.
- **Not proven**: every layout, size and relation for D, E and F. Next step needs a loaded save.

### Phase 15: Registration Doctor (Plan D, read-only) and Staff/PPM discovery (2026-10-27)
- **D (done, read-only)**: `native_registration::read_registration_gates` + `eligibility::evaluate` + `POST /api/player/eligibility {player_id, comp_index, comp_uid, game_message?}` + Contract-tab "Registration Doctor" card with a copyable support report. Reports only gates whose data is mapped (registration window from stage gate +0x9a and manager bit 0x100; squad cap, home-grown quota and squad-number cap as information). Everything else (registered count, home-grown status, continental registration type, non-EU slots, work permit, appearance limits) is `unknown`, never `ok`. Live read on PID 5907, La Liga 51/67 (no writes): type 8, gate closed, bypass bits off, cap 25, home-grown 8, number cap 255; matches the earlier mapping. Not done: discovery of the unknown gates, and Serie A / continental live tests.
- **F discovery (read-only, PID 5907, not yet a feature)**: vtable of `.?AVACTUAL_NON_PLAYER@db@@` sub-object is at record+0x100 (RVA 0x4565018; second COL at +0x128, RVA 0x4564fc8, found by parsing RTTI in the DLL file). Heap scan found 24,397 such objects; the database ones (heap addresses, stride 0x240, e.g. 0x1377e0040 Frank Dauwen) carry identity `(index, uid, uid)` at record+0x108 and pooled first/last name pointers at record+0x150/+0x158 (same pooled-string shape as players). A byte block at record+0x10..0x4f (values 1..20 then larger values, e.g. Paolo Maldini 2/45) and 8 bytes at record+0x170 look like attributes. **Not proven**: which bytes are which attribute, scale, CA/PA, date of birth, nationality, job/contract links, the many non-database copies of this vtable.
- **E (preferred moves)**: no RTTI name mentions moves; the only tag hit was `pPpM` (RVA ~0x182471aa8) which is an object-factory node, not a record field. A live comparison against known profiles is needed. **Not proven**: any layout.
- Verification: cargo check clean, cargo test passing (99 core, 22 server), Node 47 passed.

### Plan E/F discovery update from user screenshots (2026-10-27, read-only, PID 5907, NOT verified)
- **E, preferred moves (strong lead)**: a 64-bit field at `ACTUAL_PLAYER address + 0x348` (player record = address + 0x148, so record + 0x200) is a bitset. It is the only 8-byte window in the first 0x1400 bytes whose popcounts and overlaps match three profiles read from the screen: Mbappé (Tries Tricks, Cuts Inside From Both Wings, Runs With Ball Often) = bits {35, 39, 51}; Yamal (Curls Ball, Uses Outside Of Foot, Tries Tricks, Runs With Ball Often, Cuts Inside From Right Wing) = bits {11, 14, 35, 51, 58}; Haaland (Likes To Try To Beat Offside Trap, Knocks Ball Past Opponent, Gets Crowd Going, Tries First Time Shots) = bits {13, 27, 41, 42}. Bits 35/51 are Tries Tricks / Runs With Ball Often in unknown order; bit 39 = Cuts Inside From Both Wings; bits 11/14/58 = Curls Ball / Uses Outside Of Foot / Cuts Inside From Right Wing in unknown order; Haaland's four are unordered. **Not proven**: the full bit-to-move table, whether any bit or value range is invalid, and write behavior. The language template (`data/game_simulation/templates/user_folder/languages/example.ltf`) has the move names but not their ids.
- **F, staff**: 23,696 staff records read (stride 0x240, identity at +0x108, names at +0x150/+0x158). The screen shows only ability labels (Unsuited ... Outstanding, Elite), not numbers. Two profiles (Paolo Maldini 2/45, Pep Guardiola 79/1120) were compared with the bytes at record+0x10..0x4f and +0x170; no label-consistent mapping was found (e.g. coaching bytes do not rank like the labels), so those blocks are NOT confirmed as attributes. Needs the label-to-number scale and more profiles.

### Staff scanner (fmscan `entity ... staff`, read-only, 2026-10-27)
- `fmscan entity PID staff [--name|--id|--uid] [--editor] --json`: ACTUAL_NON_PLAYER records found by the vtable of the +0x100 sub-object, ownership-checked by the +0x128 sub-object vtable, identity `(index, uid, uid)` at sub+8 (record+0x108), pooled first/last names at record+0x150/+0x158. Records that fail any check (including the many non-database copies of the vtable) are skipped; duplicate identities are refused.
- Live PID 5907: 24,480 validated staff records; `--name Guardiola` -> Pep Guardiola 79/1120 (record 0x1377eafc0) and José Luis Guardiola 109828/2002080321.
- `--editor` prints the raw candidate blocks at record+0x10 (64 bytes) and +0x170 (8 bytes), labelled `unmapped_raw_bytes_heuristic`; attributes are reported unavailable. No write support.
- Tests: new synthetic staff test (identity, names, raw block, copy without +0x128 owner rejected, uid mismatch rejected, `--related` refused); C++ suite 58 passed with `-Werror`.

### Staff attribute mapping (read-only, 2026-10-27, two profiles, partly verified)
- Method: with a staff profile open, `dump_live_bindings` (now decodes list elements) shows `AttributesCoaching` / `AttributesCoachingMental` / `AttributesCoachingKnowledge` as `{name, ptvl (1..20 display value), Strn (label), Auey (attribute id)}`. Values for Pep Guardiola (79/1120) and Paolo Maldini (2/45) were matched against both staff records (no writes).
- Value ranges over 24,386 staff records: record+0x10..0x29 (except +0x1d) are 1..20; +0x1d and +0x2a..0x43 are 1..100 (native scale, display = native/5 rounded; nearest vs ceiling not yet distinguished); +0x170..0x177 are 1..20 (person block).
- Offsets matching both profiles (unique unless noted): Goalkeeping +0x2b, Judging Player Ability +0x2c, Judging Player Potential +0x2d, People Management +0x2e, Tactical Knowledge +0x31, Attacking +0x32, Defending +0x33, Fitness +0x34, Possession +0x35, Technical +0x36, Negotiating +0x41, Judging Staff Ability +0x42, Set Pieces +0x43 (native); Working With Youngsters +0x1c, Authority +0x14, Adaptability +0x170 (1..20 raw). Tactical/Motivating fit both +0x2f and +0x37: assigned Tactical +0x37 / Motivating +0x2f by the consecutive attribute ids (heuristic). Determination fits +0x1d (native) and +0x175 (raw): unresolved.
- **Not proven**: the remaining native bytes (+0x2a, +0x30, +0x38..+0x40; likely medical/scouting/data attributes), the rounding rule, and any write behavior.
- **Update (third profile, Nana Supriyatna 22412/41002451, user's club coach)**: every one of the 19 Head Coach tab attributes now has exactly one offset consistent with all three profiles. Resolved: Tactical +0x37, Motivating +0x2f, Determination +0x1d (native). Display rule: `max(1, round_nearest(native / 5))` (native 1 -> 1 Negotiating; native 16 -> 3 Tactical Knowledge). `fmscan entity ... staff --editor` now prints the 19 attributes (value, native, scale, out-of-range -> null + reason); its output equals the on-screen values for all three profiles. Still unmapped: native bytes +0x2a, +0x30, +0x38..+0x40 and the 1..20 bytes at +0x10..+0x29 not listed above (scouting/medical/data candidates). No write support. C++ suite 58 passed.
- **Update (Scouting/Medical tabs, Guardiola + Supriyatna)**: Analyzing Data (id 18) +0x3c, Physiotherapy (id 17) +0x30, Sports Science (id 20) +0x3f, all native 1..100; each the only unmapped offset fitting both profiles. All 22 attributes shown on the staff attribute tabs are now mapped and `fmscan --editor` output equals the screen for every captured profile. Native bytes +0x2a, +0x38..+0x3b, +0x3d, +0x3e, +0x40 are shown on no tab and stay unnamed. Next: Rust read path (plan F1/F2), then writes only after the evidence ladder.
- **Verification (user's scout and physio)**: Wahyu Pahabol 107110 (Scouting tab, 11 attributes) and Benidektus Adi Prianto 53432 (Medical tab, 12 attributes incl. Physiotherapy 15 = native 75 at +0x30, Sports Science 11 = native 55 at +0x3f) match the scanner with zero mismatches. Analyzing Data +0x3c has only been seen at display 1 (native 5) so far; a staff member with a higher value would strengthen it.
- **Verification (second scout)**: Yohanis Hera 59949 (Scouting tab, 11 attributes) matches with zero mismatches, including Analyzing Data 4 = native 20 at +0x3c and Negotiating 10 = native 52 (52/5 = 10.4 -> 10, nearest rounding). All 22 staff attributes now verified against six profiles.

### Phase 16: Read-only Staff Reader in Rust and Staff Tab (Plan F1/F2 reads, 2026-10-27)
- **Seventh profile**: performance analyst Luís Viegas 14079 (Scouting tab) matches with zero mismatches (Analyzing Data 10 = native 51 at +0x3c).
- `fmlev-core/src/native_staff.rs`: `NativeStaffRecord::{discover_by_index, search, validate, read_attributes}`; vtables pinned to game_plugin.dll RVAs 0x4565018 (+0x100) / 0x4564fc8 (+0x128), PE 0x6a229e8a; identity (index, uid, uid), pooled names; duplicate ids refused; out-of-range bytes -> `None` + reason. Unit tests use in-process fakes for validation/selection (a self-scan finds stale copies of the fakes in freed scan buffers, so the full scan is checked live instead).
- Routes: `POST /api/staff/search {query, max?}`, `POST /api/staff/native {staff_id, staff_uid}` (read lock, `deny_unknown_fields`, uid must match). UI: read-only Staff tab (search, grouped attributes, no inputs). Example `staff_data` (search/read).
- Live: Rust example and `fmscan` agree on all 22 attributes for all seven profiles (PID 5907). The local HTTP server was not run live (port 41789 was in use by the user's `fmlev-cli`).
- **Not done**: staff writes (need a screen check after a day tick and save/reload on a throwaway save), contracts/career plans/relationships/knowledge/tendencies.
- Verification: cargo check clean; cargo test passing (106 core, 23 server); Node 50 passed; C++ 58 passed.
- **Experimental staff writes (2026-10-27)**: `NativeStaffRecord::write_attributes` sets display values 1..=20 in one `apply_transaction` (pre-check, write, readback, rollback). Native fields are written as `value * 5`; attributes already showing the requested value are skipped (exact native byte kept); unknown/duplicate fields, values outside 1..=20 and fields currently out of range refuse the whole write. `POST /api/staff/update {staff_id, staff_uid, attributes:{field:value}, experimental}` returns 501 unless `experimental: true`. UI: unchecked "Enable experimental staff editing" checkbox; Save sends only changed fields. **Not proven in game**: no live write has been made yet; needs the screen check after a day tick and save/reload on the throwaway save before it can be called verified. Tests: core 108, server 23, Node 54.
- **In-game proof (staff write, throwaway save, PID 5907)**: Nana Supriyatna 22412 Tactical 6 -> 10 through the UI (native 28 -> 50 at +0x37, readback OK). FM26 showed "Average" on his profile; the user reported it was still "Average" after the requested day tick and save/reload. This proves one native field; raw 1..20 fields (Working With Youngsters, Authority, Adaptability) are not yet tried in game, so the experimental opt-in stays.
- **Launcher**: `./start.sh` (and `start`) now always runs `cargo build --release` first; a running server on an outdated binary is replaced, a current one is reused; a failed build leaves the running server alone. `tools/test_server_control.py` 9 passed.


### FM26-style UI U0–U6 integration (2026-10-27)
- Manifest-driven shell, tokens and page sources preserve one embedded response without a frontend build pipeline.
- Player profile/goalkeeper columns, roles, staged actions, four Staff views, entity native editors, Settings and Diagnostics are wired.
- Unified Load/Review/Save/Discard; no field-change writes. In-flight edits and identity-bound pending actions are retained.
- Compact mode keeps navigation, stacks panels/action groups, wraps the save bar and uses readable padding and larger targets.
- Cheats have balanced columns, padded rows, separated status badges and aligned buttons. Quick Actions use labeled groups.
- Audited every static inline handler and element ID; fixed stale save callbacks, missing handlers, capability gates and profile facts.
- Unverified transfer controls and unsupported cheats stay disabled. Population remains read-only. No native safety guard was weakened.
- Verification: workspace check; Rust 153 passed / 1 ignored; Node UI 124 passed; C++ synthetic process suite 58 passed. Browser review in normal and compact layouts. README includes two labeled demo screenshots, not new native write proof.
- Current UI source contract and evidence: docs/UI_ARCHITECTURE.md and docs/UI_PLAN.md. Historical native findings above remain historical.


## PPM bit table

Workstream E (preferred player moves), read-only. Session 2026-10-28, branch `worktree-agent-ad6a798a3808e1c98`.

- **Live access this session: none.** FM26 was not running (no game process; `ptrace_scope` = 1, not 0), so no new bitsets were read and no bit was newly proven. All evidence below is the three on-screen profiles recorded in "Plan E/F discovery update from user screenshots" (Mbappé, Yamal, Haaland). Nothing was written to any process.
- **Field**: 64-bit bitset at player record + 0x200 (ACTUAL_PLAYER + 0x348; "record" = the attribute/person view used by `NativePlayerRecord` and `fmscan`, embedded at ACTUAL_PLAYER + 0x148).
- **Verified bits (1)**: bit 39 = *Cuts Inside From Both Wings*. Proof by set difference: Mbappé's profile has it and his bits are {35, 39, 51}; Yamal {11, 14, 35, 51, 58} and Haaland {13, 27, 41, 42} do not list the move and have bit 39 clear. 35 and 51 are the only bits shared by Mbappé and Yamal, so they are exactly {Tries Tricks, Runs With Ball Often}.
- **Narrowed, NOT verified (stay `unknown`)**: {35, 51} = Tries Tricks / Runs With Ball Often (order unknown); {11, 14, 58} = Curls Ball / Uses Outside Of Foot / Cuts Inside From Right Wing (order unknown); {13, 27, 41, 42} = Likes To Try To Beat Offside Trap / Knocks Ball Past Opponent / Gets Crowd Going / Tries First Time Shots (order unknown). Every other bit (0..63) has no evidence.
- **Dead end**: the language file `example.ltf` marks move titles with `[COMMENT: player preferred move]`, but its string ids (e.g. Runs With Ball Often = 297402) are not a contiguous list and are not bit indexes; they cannot give the ordering.
- **Next evidence needed (live game)**: read the bitset (`fmscan entity PID player --name X --editor`) for players whose screen PPM lists are known. A profile with exactly one PPM that is also in a narrowed set resolves it; GK profiles (Comes For Crosses, Distribution moves) and Runs With Ball Down Right/Left strikers would each pin bits by intersection. Mark a bit verified only after it is the unique difference between two profiles or confirmed by a single-PPM profile.
- **Shipped (read only)**: `fmscan entity ... player --editor` prints/returns `preferred_moves` (`bitset`, `set_bits[{bit,name|null,verified}]`, `named_moves`, `unnamed_set_bits`, status `partial_bit_table`); Rust `NativePlayerRecord::read_preferred_moves`, `preferred_moves` module and an optional `preferred_moves` field on the `Player` payload (omitted when unreadable); `player.js` shows named moves plus "N not yet identified". Tests: C++ synthetic process (bit 39 named, bits 3/35 unnamed, empty bitset), Rust decode/read/serde tests, Node display test.
- **Writes: unsupported.** There is no move-write route, and none is added (it would return 501): the bit table is incomplete, bit validity ranges are unknown, and no in-game readback or save/reload proof exists. C++ vs Rust live parity (evidence-ladder step b) for this field is **not yet done** because the game was not running.

## Nation rules

### Plan C read path (2026-10-28, read-only)
- **No live game in this session**: FM26 was not running (no process mapped `game_plugin.dll`), so the cross-check across all nations could not be repeated and nothing was promoted to "verified". The labels below come from the earlier PID 5690 read recorded in "Plan C discovery log" plus the serializer trace; they are not new live evidence.
- **Scanner**: `fmscan entity PID nation --editor --json` now adds `nationality_years` (+0x1ed, u8), `gain_nationality_type_native_code` (+0x1eb, u8, accepted range 1..2), `dual_nationality_allowed` (bit 0x02 of +0x1e9 set = NOT allowed, so the output is the negation) and a `field_confidence` array. `gain_nationality_type` (the label) stays null: enum order unknown. PE timestamp 0x6a229e8a is required, as for the other new numeric layouts. Synthetic tests in `tools/fmscan/tests/test_entities.py` (two nations, bad type code, unknown build, report script); C++ suite builds with `-Werror`.
- **Distribution report** (read-only, run when the game is loaded): `python3 tools/fmscan/nation_rules.py PID` lists every nation's years/type/dual/economic/development, value distributions and agreement with hints (Spain 10, Italy 10, England 5, Andorra 20, San Marino 30, Japan 5). It never prints "verified"; it prints `probable` for years only when at least 5 of the 6 hints agree and more than 3 distinct values exist.
- **Confidence now**: years `probable` (earlier read: Spain 10, Italy 10, England 5, Brazil 4, Ghana 6, Andorra 20, San Marino 30, Japan 5 match real-world rules; 0, 99 and 255 for 12 nations unexplained; no screen comparison). Gain type code `unverified` (only 1 and 2 seen, 128 vs 123 nations, which one is Accumulative/Continuous unknown). Dual nationality bit `unverified` (12 flagged nations incl. Japan, China PR, India, Indonesia, Kazakhstan, Cuba, Latvia, Ukraine look plausible, polarity inferred). Economic factor (+0x1f0) and development code (+0x1ec) `unverified`: serializer tags `Necf`/`Nsod`, not compared with a screen; development labels unknown.
- **Rust**: `fmlev-core/src/native_nation_rules.rs` reads the raw candidates after identity/ownership and PE-timestamp guards and builds `NationalityRules`; a field exposes a value only when its constant in that file (`YEARS_CONFIDENCE`, `GAIN_TYPE_CONFIDENCE`, `DUAL_CONFIDENCE`) is `Verified`. All three are below that, so `POST /api/nation/native` returns `nationality_rules` with every value null and a reason. Flip a constant only after the screen comparison and a matching distribution. No write path exists; edits to these fields are not accepted anywhere.
- **UI**: the Nation tab "Nationality rules" card is read-only (no inputs), shows verified values and "Unavailable: reason" for the rest; it stages no changes.
- **Not proven**: that these bytes are what the Nation screen shows, the type labels, the meaning of 0/99/255 years, development/economic semantics, any behavior change in game, and every write.

## Staff contract/info

### Plan F3 / Information tab (2026-10-28, nothing new verified)
- **No live game this session**: FM26 was not running (no Proton/game process, `game_plugin.dll` in no `/proc/*/maps`), so no live read, range survey or screen comparison was possible. Nothing below counts as verified; static work only. No game memory was touched.
- **Information candidates** (offsets from the task brief, relative to the staff record `R`; **not cross-checked anywhere in this repo**): reputation u16 `R+0xd4/+0xd6/+0xd8` (meaning/order unknown, neutral names), CA u16 `+0xda`, PA u16 `+0xdc`, birth year u16 `+0x18a` (year only, no day/month). `fmscan entity PID staff ... --editor` now prints them under `editor_data.information` with status `candidate_unverified` and range guards (reputation 0..=10000, CA/PA 0..=200, year 1900..=2100; out of range -> null + reason). Promote one only after (a) a live range survey over all staff records and (b) a match to the open staff profile for several people, then flip `verified` in `STAFF_INFORMATION` (`native_staff.rs`).
- **Contract: no staff contract link is mapped.** Staff identity is the `+0x100` person sub-object (player contracts store the person sub-object as owner; for staff that would be `R+0x100`, **unproven**). New read-only survey `editor_data.contract_discovery` (heuristic): every heap-pointer slot in the first 0x240 bytes of the record that RTTI names, directly or as a loan-style node (first qword), with the `+8` person back-pointer check; a `.?AVFULL_CONTRACT@db@@` hit is decoded with the **player** layout (wage `+0x20`, expiry `+0x48`, team `+0x10`) labelled `player_layout_decode_not_verified_for_staff`. Run it on a live save for a staff member whose contract is known on screen (club coach: wage and expiry) and compare; also look for `JOB`/`STARTING_CLUB_STAFF_JOB_STATUS` RTTI hits (leads from the Plan D/E/F static log).
- **Static lead, abandoned**: the `ACTUAL_NON_PLAYER` vtables at RVA `0x4565018` (+0x100) / `0x4564fc8` (+0x128) are the only two with RTTI for this type (COL offsets 0x100/0x128, no offset-0 COL); no serializer found, so the candidates got no static corroboration either.
- **Product**: `POST /api/staff/information` and `POST /api/staff/contract` (`deny_unknown_fields`, identity by id+uid) return every candidate/contract field as `unavailable` with a reason while `verified` is false; identity (name, id, uid) is the only verified content. `POST /api/staff/information/update` and `/contract/update` always return 501. UI: Staff tab sub-tabs Information and Contract, read-only, no inputs, "Unavailable" + reason per field.
- **Not proven / not done**: every Information offset, reputation meaning, the contract chain (wage, expiry, role/job, club vs national-team), any write. Career Plans, Relationships, Knowledge, Swap Staff, Formations/Tendencies untouched.
- Tests: new synthetic fmscan test `test_staff_information_candidates_and_contract_survey`, Rust unit tests for unavailable/verified/range handling, server route test, Node tests for both sub-tabs.

### Live verification 2026-10-28 (owner, PID 5668, throwaway save): staff birth year and contract promoted
- Profiles compared with `fmscan entity PID staff --name X --editor` and the Rust example `staff_data info <id> <uid>` (both agree): Vicente Engonga 690/14652 (Spain U21s coach, no contract, born 1965), Antonio Pintus 4460/4001686 (R. Madrid fitness coach, £245K p/a to 30 Jun 2026, born 1962), Iván Ortega 20414/67055850 (R. Madrid physio, £345K p/a to 30 Jun 2027, born 1981).
- **Verified (3 profiles, flipped in `native_staff.rs`)**: year of birth `R+0x18a`; contract link `R+0x1a8` -> `FULL_CONTRACT` (vtable RVA 0x4334df8, person back-pointer `+0x8 == R+0x100`), weekly wage u32 `+0x20` (raw 4706 x 52 = £244,712 shown as £245K; 6641 x 52 = £345,332 shown as £345K), expiry u32 `+0x48` = `(year << 16) | day-of-year` (day 181 = 30 June). A null link means no contract (screen shows N/A). Guards: PE timestamp 0x6a229e8a, vtable, back-pointer and range checks.
- **Not proven**: reputation (the three u16 at +0xd4/+0xd6/+0xd8 read 4746/4747/2248 for "Obscure", 7750/7750/6250 for "National", 8699/8699/8200 for "Continental": order consistent, which slot and thresholds unknown), CA/PA (the screen shows no numbers), club job, role, national team job, any contract for non-R. Madrid clubs or the human manager's own staff, and every staff contract write (still 501).
- Search index (plan G step 3 and 4): counts, build time 1.85 s, query p95 8.6 ms, six sampled entities matched `fmscan` on a 49,070 player save; a second save (39,514 players) rebuilt the index under 10 s (user-confirmed in the UI). Preferred moves (step 5): Mbappé bitset 0x8008800000000, bit 39 = Cuts Inside From Both Wings still the only named bit. Nation rules (step 6): the game's Nation screen shows no nationality rules, so those candidates cannot be screen-verified there and stay unverified.

### Staff tab: club in search, club search, recents (2026-10-28)
- Staff index entries now carry the club (`detail`) from the validated contract chain (`R+0x1a8` contract, owner back-pointer `R+0x100`, team `+0x10`, club `team+0x30`); no link means no club, never a guess. Staff search matches name or club ("madrid" lists the club's staff, name matches rank first); player search stays name-only. `POST /api/staff/search` returns a `clubs` map by index. UI: Club column, "Recently opened" list (browser localStorage), and Load with nothing selected reopens the last staff member.
- Live (PID 5668): `madrid` returned 50 staff, 49 with a club (Getafe, Lleida, R. Madrid, R. Madrid B/C, A. Madrid B, ...); Antonio Pintus shows R. Madrid, matching the game screen. Staff whose contract is absent (e.g. national-team coaches) show no club.
- **Open staff profile (read-only, 2026-10-28)**: with a staff profile on screen the game binds `AttributesCoaching*`, `AttributesMedical` and `AttributesBlockInitialStaffSelection` on that person's `PersonReference` with `IsPlayer = 0`; the reference id is the native staff index (Iván Ortega: id 20414 = staff index 20414, Physio tab). A bindings dump taken while the FMLE window had focus (no staff profile showing) held none of them. `fmlev-core/src/open_staff.rs` collects ids that carry a staff-only binding and an explicit `IsPlayer = 0`; `POST /api/staff/open {}` resolves the id natively (index lookup, then identity check) and returns 404 when none is open and 409 when several are bound. UI: "Open in game" button, and Load opens the open profile first, else reloads the selection or the last recent. Live: returned Iván Ortega 20414/67055850 with his profile open.
- **Not proven**: the bindings are a UI cache, so a profile that was closed recently may still be bound (hint only, identity is revalidated natively); only the Medical tab was open when seen, and the Head Coach, Coaching and Scouting tabs bind the same `AttributesCoaching*` names but were not each tested; behaviour with two profiles open (409) is covered by unit tests only.

### Registration doctor live check, La Liga (runbook step 8, 2026-10-28, PID 5668)
- `POST /api/player/eligibility` for Mbappé (25145) against Spanish First Division 51/67 returned: type 8, gate closed, manager bits off, cap 25, home-grown 8, number cap 255 (none).
- Game screen (Competition registration, user screenshots): "Registration Restricted", "Max. of 1 players allowed to be registered", maximum squad size 25/25, minimum squad size 15/15, outfield players max 23 (23/23), non-EU players max 3 (1/3), squad salary cap £238,206,000 p/a (£227M used).
- **Matches**: closed gate = "Registration Restricted"; squad cap 25.
- **Not matched / new**: the game lists no home-grown rule on this screen, so native `+0x99 = 8` stays unconfirmed (doctor text now says so). "Max. of 1 players allowed to be registered" means a restricted window still allows one registration, so the doctor's "blocked" is worded as the gate being closed with a limited number of registrations possible (the limit is not mapped). Not read by the doctor: non-EU max 3 and usage, outfield max 23, squad salary cap, minimum squad size 15, the squad number cap (not shown anywhere on the screen).
- Not done: Serie A / continental competitions, the "1 allowed" counter offset, the other rules above.

### Role presets live check (runbook step 9, 2026-10-28, PID 5668, throwaway save)
- Player: Patrick Berg 18735/53099570 (Bodø/Glimt, not the user's club, so the game shows scout ranges). Preset: Deep-Lying Playmaker, Level 3 Elite, CA "raise to fit", applied from the Positions tab and saved with the normal player Save.
- Before (screen ranges): First Touch 12-16, Composure 13-17, Decisions 11-15, Passing 9-15, Technique 12-16, Vision 14-18, Tackling 13-17, Balance 9-13; CA about 143 (user's recollection, not recorded). After (screen ranges): 16-20, 16-20, 16-20, 14-20, 16-20, 18-20, 16-20, 12-16.
- Memory after the write (`fmscan entity ... --editor`): First Touch, Passing, Technique, Composure, Decisions, Vision 18; Teamwork 20; Tackling 16; Balance 16; Anticipation 17; Positioning 17; CA 189, PA 189. Every game range contains the memory value. The preset preview then reported "No changes needed".
- After one day tick (28/9 -> 29/9/2025): memory and ranges unchanged. After the user saved and reloaded the game: values persisted (user-confirmed in the FMLE Player tab and the game).
- **Proven**: a Level 3 role preset write persists through a day tick and a save/reload.
- **Not proven**: exact values (scout ranges only; use a player at the user's own club); Levels 1 and 2; that the role names and key/secondary lists (DLP key: First Touch, Passing, Technique, Composure, Decisions, Teamwork, Vision; secondary: Tackling, Anticipation, Positioning, Balance) match FM26's role screens, so the "role data unverified" badge stays; the CA rule (the heuristic took CA from about 143 to 189 and PA to 189; it is not the game's formula and the jump is large).

### Owner confirmations (2026-10-28, reported by the user, no new commands run)
- **Staff attribute writes**: the user reports staff attribute editing worked in game the last time it was tried; no values, player or day-tick/reload detail were recorded, so the earlier "one field (Tactical) proven" entry is the only measured evidence. The experimental opt-in checkbox stays.
- **Role presets**: the user considers further testing unnecessary (Level 3 proven on one player; Levels 1-2, exact numbers and role lists left unchecked by choice).
- **Competition prize money**: the user reports the edited prize table works in game; no season-end observation values were recorded.
- **Loan signing**: stays 501. A loan agreement is a game-allocated object (AGENTS.md Phase 3 notes) and no layout for creating one is proven.

### Audit follow-ups from the Phase 11b "known issues" list (2026-10-28)
- **Fixed (tests added)**:
  - `update_*`, preset and quick-action routes now map core errors by kind (`core_status`): invalid input 400, unknown player 404, game not in the expected state 409, memory/runtime failure 500 (they returned 500 for validation errors).
  - `POST /api/attach` rejects a malformed or unknown-field body with 400; an empty body still means "find FM26" (a bad body used to attach to whatever was running).
  - `POST /api/mem/write` accepts only 1..4096 bytes inside one writable mapping and writes them as one transaction (pre-read, readback, rollback); it returns the previous bytes. `POST /api/actions/heal` goes through the same guard. `POST /api/actions/finances` returns 501: its caller-supplied offsets hit UI-binding caches the game rewrites; `POST /api/club/money` is the verified route.
  - Register Anytime: disabling now restores the manager bits (`0x100`/`0x200`) the manager had before the first enable instead of clearing both, and leaves them alone while another competition is still enabled. The saved originals are now `{gate, manager_bits}` per competition.
  - Free-slot and transfer-queue lookups no longer use run-specific address constants. Live read-only check (PID 5668): all 51,759 `FULL_CONTRACT` objects sit in one writable mapping (0x1373a0000..0x13fb50000) at stride 0xc8 with a zero tail; `find_zero_contract_slot` picked `0x138c85e30` (last contract 0x138c851b0 + 16 slots), all zero, in 1.5 s, equal to an independent Python survey. The contract vtable now comes only from the mapped `game_plugin.dll` (the address fallback constant is gone). The transfer queue is found by entry shape (person pointer, packed date, transfer id) plus an ordered neighbour, and more than one plausible match is an error.
- **Already fixed earlier (UI)**: player-tab controls without a native path and the unverified cheats are disabled in connected mode (`playerFieldWritable`, `data-unverified` buttons).
- **Not fixed**: (1) [resolved, see "Global-lock scans removed" below] context methods for club, nation, competition, prize-table and player discovery scanned memory while the global state lock was held. (2) The free contract slot is a heuristic: the game's own allocation counter is not updated, so a later in-game signing could reuse the slot. Free-agent signing stays experimental and is not re-tested live after this change (only the read-only slot lookup was). (3) The new transfer-queue lookup was only tested synthetically: no pending transfer was available to check it against the game.
- Loan signing stays 501 (the loan agreement is a game-allocated object; no creation layout is proven).

### Global-lock scans removed (2026-10-28, follow-up to the audit list)
- **Cause**: club, nation, competition, prize-table and player discovery ran full-memory scans (about 1.2 s) inside `GameContextManager` methods while the global state write lock was held, so any request needing the lock waited.
- **Change**: `fmlev-core/src/locator_cache.rs` is a process-wide cache keyed by (kind, pid, id). `NativeClubRecord`, `NativeNationRecord`, `NativeCompetitionRecord`, `NativePlayerRecord::discover_by_index`, `NativeStaffRecord::discover_by_index` and the registration `competition_record` lookup now return the cached record only after their own identity validation (the same check an edit performs) passes again; otherwise they scan and replace it. Failures are never cached; attach clears the process's entries. The routes in `routes/context.rs` call `warm()` before taking the lock, which runs the scan on a worker thread, so the in-lock call is a cache hit. The prize-table route needs only the driver, so it now runs entirely off the lock (`competition_prizes_with`).
- **Live (PID 5668)**: `POST /api/club/native` for R. Madrid took 1.24 s cold and 0.0 s cached; during a cold 1.37 s read of another club, 2,774 `/api/status` calls completed with a maximum latency of 47 ms (nothing waited on the scan). The same measurement was not taken on the old build.
- **Caveats**: a cached record is trusted while its identity and vtables validate; a duplicate copy of a record created after the first scan is not noticed until the cache entry fails validation (the full scan's "exactly one" rule runs only on a miss). Methods called without a warm-up (internal polling paths) still scan under the lock on their first call per record. Tests: four cache unit tests, nothing exercises the lock timing automatically.

### Cheats built from verified writes, native stadium read (2026-10-31)
- **Cheats (UI only, no new route)**: four cards that were disabled are now buttons that stage values in the same inputs a manual edit uses, so every write still goes through Save and its native readback and rollback path. Clear Negative Balance raises a negative balance to 0 (loans and interest are not stored in a verified field, so they are not touched). Big Money raises the balance to 1,000,000,000 and the transfer budget to 500,000,000 raw GBP, never lowering them and capped at the 32-bit limit; benefactor status is not changed. Both need the club's native money loaded first (`/api/club/money`). Prize Table x10 multiplies the loaded league prize table by 10 (capped at 2,147,483,647; the Save rules about a positive first/last prize still apply) through `/api/competition/prizes`. Wonderkid Forge raises PA to 195 through the normal PA edit (never lowers it; CA untouched). Offline (not connected) the older preset paths are kept. Tests: `cheats-native.test.cjs` (8 tests, including that staging sends no request and that Save sends only `/api/club/money`).
- **Not proven in game**: these cheats were not clicked in the game; they only stage values for writes that were verified earlier. Values of the size Big Money uses (1 billion balance) were not tried in game, so the game may clamp or display them oddly. The season-end payout source of the prize table is still unconfirmed. Remaining disabled cheats (Max Morale, Wipe Bans, Unlock IGE, Invincible Board, Golden Gen, Relax Rules, Clean Governance) have no native path.
- **Native stadium name (read-only)**: `NativeClubRecord::read_stadium_name` reads `stadium + 0x40` (a length-prefixed string, else two pooled name references), mirroring `fmscan entity ... club --editor`. Live (PID 4496): Rust and the scanner agree for R. Madrid (Santiago Bernabeu, 83186/95000), Barcelona (Camp Nou, 62500/105000), Getafe (Coliseum, 16500/18500), R. Madrid B, a lower-league Getafe, and both report no stadium for Espana Barcelona (club 1991). `read_club_native` now stores an unreadable stadium as an empty name and 0 capacities, so the Club page shows "Unavailable" instead of the old placeholder name and 45,000/60,000 capacities. Clubs without a details subrecord (club 1991) still fail closed with "Club details subrecord not mapped". Example: `stadium_read`.

### Transfer actions in the UI, club reputation editor (2026-11-01)
- **Transfers**: the three actions with an earlier in-game screen check and day tick (terminate contract, move, sign free agent) are now buttons on the Contract tab. They stay disabled until the game is attached, a player is open and the "experimental" checkbox is ticked; each asks for confirmation, sends `experimental: true`, then reloads the player from the game. All five transfer routes (terminate, loan terminate, move, sign, accelerate) now return 501 unless the body has `experimental: true`, and reject unknown fields. Sign requires an explicit wage and expiry year in the UI (no invented defaults). Loan recall keeps a disabled button (no in-game test); accelerate has no UI. New read-only `POST /api/club/squad-space {club_id}` returns the first team's players, capacity and spare slots (a transfer needs a spare slot; live on the current save the checked clubs were all full). Tests: `transfer-ui.test.cjs` (7), route tests for the opt-in gate and squad space.
- **Not proven**: no transfer was run in this session. Save/reload persistence of terminate, sign and move was never confirmed, and loan recall and accelerate were never run in game, so the evidence ladder is still incomplete; the buttons are labelled experimental for that reason. Signing still uses the heuristic free contract slot (allocation counter not updated).
- **Club reputation**: the Club header has a number box (1-10,000) for the native main-team reputation (`team + 0xa8`), loaded with the native club read and written by the existing `write_live_club` path (changed field only, readback and rollback, already in `LIVE_CLUB_WRITABLE`). The old `|| 8000` placeholder in the display is removed; an unreadable value shows Unavailable and the box stays disabled. Live read (PID 4496, previous save): R. Madrid 9095, Barcelona 9094, Getafe 6949, FK Bodo/Glimt 6611 for `reputation`; the second field `world_reputation` (`club + 0x166`, e.g. 6574 for both Madrid and Barcelona) is read by the scanner but is not shown or editable. Tests: three club-page tests (native-only display, pending/reviewed edit sent through `/api/club/update`, invalid values refused).
- **Not proven**: which of the two reputation fields the game's club screen label reflects, whether the edit changes the displayed label, and persistence after a day tick or reload.
- **RESOLVED (2026-11-01, verified in game by the user)**: the edit works. The game's club screen shows a band (`ClubReference.ReputationScope`: value 80, label Continental for Persib Bandung, club 2313, native reputation 6500). Setting Persib to 9500 changed the label to Worldwide and setting it back to 6500 restored Continental (screens after reopening the club page). The earlier "not working" reports came from small changes that stay inside one band (Getafe 6948 to 6949). Band cut-offs other than these two points are unknown, so the UI shows only the number plus a hint. The text below is the earlier investigation.
- **Update (2026-11-01, user report, superseded)**: the club reputation edit does not show in the game (the user also saw the game continue to the next day). A direct API test on Getafe (club 1081) changed `reputation` 6948 to 6949 and back and the scanner read both values from memory, so the write path works and the field is not what the game's club screen reads (or it is recomputed). Open: dump the bindings with a club page open, find the reputation property and match it to `team + 0xa8`, `club + 0x166` or another field; until then the box should be treated as not working.

### Contract squad status, release clause and happiness (2026-11-01)
- **Method**: with players open on the game's Contract screen, the UI bindings (`dump_live_bindings`) gave ground truth (happiness levels, counts, squad number) and the contract bytes were read from memory (`FULL_CONTRACT`, 0xc8 bytes). Players compared: Bruno Iglesias 30123 (Fringe Player, GBP 280K p/a, release clause 43M), Kylian Mbappe 25145 (Star Player, 864M clause), Francisco Garcia 22492 (Squad Player, clause present), plus the whole R. Madrid first-team list against the game's squad screen (Playing Time column).
- **Squad status (read verified on screens)**: u8 at contract `+0x54` (`+0x56` normally holds the same code). 1 Star Player (Mbappe, Courtois), 2 Important Player (Bellingham, Vinicius, Bernat), 3 Regular Starter (Camavinga, Valverde, Rudiger, Tchouameni, Alexander-Arnold, Rodrygo, Militao), 4 Squad Player (Mastantuono, Asencio, Guler, Carreras), 7 Fringe Player (Bruno Iglesias). Codes 9, 10, 11, 20, 22 exist (e.g. Lunin 20, Palacios 10) but their labels were not seen on screen, so the UI shows "Code N" and refuses to write them. Juan Bernat has `+0x54` = 2 and `+0x56` = 1 and the screen shows Important Player, so `+0x54` is the agreed status; what `+0x56` means is unknown (a write moves `+0x56` only when it already equalled `+0x54`).
- **Release clause (read verified)**: the contract's clause list at `+0x68` (begin, end, capacity) holds 8-byte entries `{u32 value, u16 aux, u16 type}`; the entry with type 0 is the minimum-fee release clause (Mbappe 864,206,784 = GBP 864M, Bruno 43,000,420 then 43,210,340 as the game recomputed it, both shown as GBP 43M). Other entries are bonuses (types 32/33/34/38) and the promotion raise (type 4, value in `aux`). Contracts with no type-0 entry have no release clause.
- **Writes (not proven in game)**: the Contract tab can change an existing release clause value (1..2,147,483,647) and the squad status (the five labelled codes) through the normal player Save (changed field only, readback and rollback). Adding or removing a clause is refused because it would resize a list the game iterates. Players with a real loan agreement (loan node at `+0x1f0` whose first pointer is an agreement owned by the player; Ansu Fati on the live save) show the terms as unavailable because the screens then show the loan contract. Not checked in game: that an edit changes the Contract screen, survives a day tick or save/reload, or that `+0x56` can be left unchanged.
- **Happiness (not found)**: the game's overall happiness level (`HierarchyOverallHappinessLevel`: 55 for Fran Garcia "Happy", 80 for Mbappe and Bruno) and playing-time happiness (21 Mbappe, 28 Bruno) match no byte, u16, u32 or float in the first 0x900 bytes of the three player objects, and no pointed-to object held the playing-time pair. It is derived by the game from several factors, so happiness stays unavailable with that reason. The playing-time happiness labels for 15 R. Madrid players (Delighted, Very Happy, Happy, Satisfied) were recorded from the squad screen but no field correlated with them.
- **UI fix**: the Contract tab used to show invented values in live mode (`Regular Starter`, `Content`, release clause 0). It now shows only native values and Unavailable otherwise. A contract created by signing a free agent has status code 0 (the signing code writes only wage, expiry, team and owner), shown as "Code 0".
- **Tests**: `native_contract_terms` unit tests (6), a player round-trip test (`squad_status_and_release_clause_read_write_and_guard_loans`), `contract-terms.test.cjs` (7). `native_player` tests moved to `native_player_tests.rs` to stay under 1,500 lines.

### Transfer fee on Move (2026-11-01)
- **What**: `POST /api/transfer/move` accepts an optional `fee_gbp` (raw GBP, 1..2,147,483,647). When set, the same native transaction that moves the player also subtracts the fee from the buying club's balance and adds it to the selling club's (`club + 0x150` finance object, i32 at `+0x14`, finance back-pointer checked, overflow refused, both clubs must be resolvable and different). Transfer budgets are not changed. UI: optional "Fee (raw GBP)" box next to Move player; the confirm dialog states the fee. Sign (free agents) has no fee.
- **Evidence**: the balance field and finance object are the verified native club money path (`POST /api/club/money`). Live read-only check (PID 4496): the selling club (Girona, Blind's current team) and a buying club (Dewa United Banten FC) resolved from team pointers, with finance back-pointers valid. Tests: `native_club::balance_edit` unit test, a UI test for the optional fee field. **Not run**: no transfer with a fee was executed; whether the game shows the fee in its transfer history, or reverts balances at month end, is unknown.
- **Not covered**: a core test of `move_player` with fees on fake squads (the fake setup is large); transfer budget changes; installment or sell-on terms.

## Search index (plan G)

### Implementation (2026-10-28, NOT yet verified live)
- **Enumeration**: `fmlev-core/src/native_catalog.rs` ports the fmscan `NativeCatalog` and `read_entity` checks to Rust. It resolves `ACTUAL_PLAYER`, `ACTUAL_NON_PLAYER`, `CLUB`, `TEAM`, `NATION`, `COMP` and `FULL_CONTRACT` vtables by MSVC RTTI in game_plugin.dll (no RVA compiled in), then makes one read-only pass over writable private memory. Players need no open Attributes screen. Links (player to club through the parent contract and team, club and competition to nation) are set only when the target was validated in the same pass. Ids found at more than one address are dropped.
- **Index and queries**: `search_index.rs` folds case and accents, requires every token, ranks exact, then prefix, then word prefix, then substring, matches numeric id or uid, and pages with totals.
- **Server**: the index lives in `ServerState::search`, outside the global state lock. It builds on a worker thread at attach, for a new PID, or on `POST /api/search/rebuild`; `GET /api/search/status` reports progress. `/api/search` serves the index and falls back to registries. Staff routes use the index (revalidated by identity) and scan on a worker thread otherwise. Loaded-player discovery is split into seed, scan and merge, so no scan holds the global lock.
- **Safety**: index entries are never write targets. Opening a result revalidates the record; a failure drops the entry.
- **Tests**: synthetic in-process enumeration (`native_catalog::tests`), query engine unit tests, HTTP tests for paging, kind filter, status and answering during a build, and Node tests for the UI chip, paging and recent selections.
- **Not proven (superseded by the 2026-10-28 live check below)**: counts, names and links against a live save, build time on a full database, and query p95. Run `cargo run --release -p fmlev-core --example search_index_parity -- PID 20 <query>` and compare samples with `fmscan entity`. A save loaded in the same process is detected by a spot check of 12 entries per kind (at most every 10 s, on search calls) and triggers a rebuild; use Rebuild index if it is missed.


## Live verification — tonight's runbook (2026-10-28, owner)

Nothing from plan workstreams C, E, F, B, D or G is verified live yet. Run these in order on a throwaway save, then log each result (command, values, what is still not proven) in this file and flip the matching `verified` flag in Rust. Full table: `plan.md` section 5.

1. Start FM26 through Steam, load a throwaway save, and wait for the main screen.
2. Run `FMLE_FIX_PTRACE=yes ./start.sh` (or answer `y` at the prompt). Check that it prints `ptrace_scope is now 0` and `FMLE running`.
3. Search index: open the UI, watch the index chip reach "ready", then run `cargo run --release -p fmlev-core --example search_index_parity -- PID 20 mbappe` (PID from `pgrep -f FootballManager` or `./start.sh status`). Compare 3–5 sampled index/uid/name lines per kind with `tools/fmscan/fmscan entity PID <kind> --name "<name>"`. Log counts, build time and query p95.
4. Search in the UI: search an accent-free name; open one player, staff member, club, nation and competition from the results. Load a different save in the same session and check that the chip rebuilds within about 10 s of the next search.
5. Preferred moves: `fmscan entity PID player --name "Mbappé" --editor`; bit 39 must be set and the screen must show "Cuts Inside From Both Wings". Then compare bitsets of players whose PPM screens differ by exactly one move, and name a bit only when one difference isolates it.
6. Nation rules: `python3 tools/fmscan/nation_rules.py PID`, then open the Nation screen for Spain, Italy, England and Japan. Years to gain nationality (+0x1ed) must match. Find two nations with different gain types (+0x1eb) and a nation that forbids dual nationality (bit 0x02 of +0x1e9) to fix the labels and polarity. Behaviour proof: advance time until a player gains a nationality as the rules predict.
7. Staff information and contract: `fmscan entity PID staff --name "<coach>" --editor` with that coach's profile and contract screens open. Compare reputations, CA/PA and birth year (Information) and wage and expiry (Contract, `contract_discovery`) with the screens.
8. Registration doctor: Player → Registration for a La Liga player; the gates must match the game's registration screen.
9. Role presets: apply Level 1, 2 and 3 to a test player, advance one day, save and reload; attributes must persist, and the role lists must match FM26's role screens.
10. Write the results into this file, README support table and `LIVE_READ_WRITE.md`. Commit locally; push only when you ask.

Undo for step 2 if wanted: `sudo sysctl -w kernel.yama.ptrace_scope=1`, and delete `/etc/sysctl.d/99-fmle-ptrace.conf` if you used `persist`.
