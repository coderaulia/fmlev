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
- **Club writes**: UI-binding writes are caches the game overwrites (verified in game: balance and transfer budget reverted). Only the transfer budget has a verified native path (`native_club.rs`: `[transfer, transfer, a, b, c, balance]` i32 block found by shape + live bindings). The balance word is derived by the game and must not be written; balance should be edited through income/expenditure once those native fields are mapped. Facility ratings and other club fields are rejected rather than reported as saved.
- **Removed**: the fake "Clear Promises" action (it only set condition/sharpness).
- **Diagnostics (read-only)**: `live_dump.rs` + examples `dump_live_bindings`, `probe_native_record`, `inspect_injury`.
- **Competitions**: native record found by vtable + index + unique ID (index at +0x8, uid at +0xc/+0x10, name pointers at +0x40/+0x48/+0x50, record stride 0x1c0). Rules/prize values are not in the UI bindings and not yet mapped.
- **NEVER edit a player's injury vector** (player + 0x248, begin +0x18 / end +0x20). A nulled/emptied vector for a player with a current injury crashes FM26 on the next day processing (`game_plugin.dll+0x33af065`, `mov rcx,[begin]`) and the damage is saved into the career. Repair used: point the vector at a healed record copied from another player's list, Continue, then Save As a new file.
