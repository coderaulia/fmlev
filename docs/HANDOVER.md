# FMLE Handover

## State (Completed 2026-10-25)

Verified in game and shipped in Rust production runtime (`fmlev-core` + `fmlev-server` + `ui.html`):
- **Player & Medical**: 60 attributes, positions & feet, CA/PA, weekly wage, contract extension, safe heal (10,000 condition/sharpness units without touching injury vector `+0x248`).
- **Contract & Squads**: Contract extension (`POST /api/player/contract/extend`), squad number assignment & teammate swapping (`POST /api/player/squad-number`), tactical familiarity max (`POST /api/player/familiarity`).
- **Contract Termination & Transfers**:
  - `POST /api/player/contract/terminate`: Releases player as free agent via multi-object detachment (nulls `record + 0x1e8`, clears `0x130`/`0x138`, compacts all owning squad vectors `team + 0x38`, clears agreement/loan slots).
  - `POST /api/player/loan/terminate`: Recalls player from loan, compacts loan team squad vector, restores parent team pointers, and clears loan node (`+0x1f0`).
  - `POST /api/transfer/move`: Transfers player between squads, verifying destination spare capacity (`spare_slots > 0`), compacting source squad vector, appending to destination squad vector, and updating contract/team pointers.
- **Club Details & Facilities**: Balance, transfer budget, training/youth/coaching/recruitment facilities (`details + 0x100`), corporate facilities (`finance + 0x8b5`), team & world reputations, main stadium capacity/expansion.
- **Nation Macro & Youth**: Youth rating (1–200), reputation (1–10,000), economic factor (`+0x1f0`), state of development (`+0x1ec`).
- **Competition**: Native reputation (1–200, `comp + 0x188`), league prize table (`POST /api/competition/prizes`), Register Anytime (manager flag bit `0x100` + domestic stage gate byte `0x9a = 1`).
- **Web UI & Test Suite**: All verified live cards decoupled at top level; connected-mode enables verified controls while disabling unverified fields; 120 Rust unit tests, 37 Node DOM tests, and 57 C++ synthetic tests passing.

## Remaining Gaps & Next Targets

1. **Signing Unemployed Free Agents (`POST /api/transfer/sign`)**:
   - Signing an unemployed player requires the game engine to allocate a new `FULL_CONTRACT` C++ object at `record + 0x1e8` and establish ~50 inbound references. External memory writes cannot safely instantiate this object from outside the engine's custom heap allocator (returns an informative 400 error).
2. **Forcing Arranged/Pending Deals to Join Early**:
   - Arranged future transfers (e.g. Cortina joining R. Madrid) only populate the agreement node at `+0x1e0`. The join date is stored elsewhere (likely in the `AgreementOffset` table from `fm-ai-assistent`), so forcing an arranged deal to join immediately remains unmapped.
3. **Squad-Number-Anytime Gate (bit 0x200)**:
   - Domestic leagues check squad numbers against official windows. Bit `0x200` of the dword at `manager + 0x404` was identified, but its exact check routine in `game_plugin.dll` is not yet traced.
4. **Active Competition Rules & Calendar Windows**:
   - In-match rules (substitutions, bench size, VAR) and squad constraints (foreign player limits, squad size caps) exist as static 'sqsr' data nodes; editing them does not update the active simulation engine. Transfer window open/close dates exist as calendar events ('wnd0'/'wnd1') and are not yet writable.
5. **Unmapped Entity Fields**:
   - Club: Data analysis facilities, sugar daddy status, board patience, fan confidence, affiliate club pointer chain.
   - Nation: Flat tax rate, corruption index, languages, population, FIFA world ranking points.

## Rules
Max 1,500 LOC/file; verification trio from `AGENTS.md`; commit locally, never push unless asked; test risky writes on a throwaway save, then Save As.
