# FMLE Handover

## State (Completed 2026-10-25)

Verified in game and shipped in Rust production runtime (`fmlev-core` + `fmlev-server` + `ui.html`):
- **Player & Medical**: 60 attributes, positions & feet, CA/PA, weekly wage, contract extension, safe heal (10,000 condition/sharpness units without touching injury vector `+0x248`).
- **Contract & Squads**: Contract extension (`POST /api/player/contract/extend`), squad number assignment & teammate swapping (`POST /api/player/squad-number`), tactical familiarity max (`POST /api/player/familiarity`).
- **Contract Termination & Transfers**:
  - `POST /api/player/contract/terminate`: Releases player as free agent via multi-object detachment (nulls `record + 0x1e8`, clears `0x130`/`0x138`, compacts all owning squad vectors `team + 0x38`, clears agreement/loan slots).
  - `POST /api/player/loan/terminate`: Recalls player from loan, compacts loan team squad vector, restores parent team pointers, and clears loan node (`+0x1f0`).
  - `POST /api/transfer/move`: Transfers player between squads, verifying destination spare capacity (`spare_slots > 0`), compacting source squad vector, appending to destination squad vector, and updating contract/team pointers.
  - `POST /api/transfer/sign`: Signs unemployed free agents by formatting an aligned 200-byte `FULL_CONTRACT` object (resolved vtable `game_plugin.dll + 0x4334df8`, wage, expiry, team pointers) and appending to destination squad vector.
  - `POST /api/transfer/accelerate`: Modifies scheduled join date in agreement contract (+0x44) and the transfer event priority queue (+0x10), or immediately switches the player contract and squad vectors.
- **Squad Number Anytime Override**: Universal override via human manager `manager + 0x404` bit `0x200` (`[manager + 0x405] & 2`) traced across all 5 rejection sites in `game_plugin.dll`, enabling squad number assignment outside domestic windows.
- **Club Details, Facilities & Benefactor**: Balance, transfer budget, training/youth/coaching/recruitment facilities (`details + 0x100`), data analysis facilities (`details + 0x11e`), corporate facilities (`finance + 0x8b5`), sugar daddy benefactor status (`finance + 0x3c`: None, Foreground, Background, Underwriter), team & world reputations, main stadium capacity/expansion.
- **Nation Macro, Youth & Population**: Youth rating (1–200), reputation (1–10,000), economic factor (`+0x1f0`), state of development (`+0x1ec`), population (`nation + 0x1e4`).
- **Competition & Dynamic Stage Rules**:
  - Native reputation (1–200, `comp + 0x188`), league prize table (`POST /api/competition/prizes`).
  - Register Anytime: manager flag bit `0x100` + domestic stage gate byte `0x9a = 1`.
  - Dynamic Stage Squad Rules (`POST /api/competition/stage-rules`): queries and modifies active simulation engine `stage_rule_entry` (`+0x98` squad cap and `+0xd6` secondary squad cap).
- **Web UI & Test Suite**: All verified live cards decoupled at top level; connected-mode enables verified controls while disabling unverified fields; 123 Rust unit tests, 37 Node DOM tests, and 57 C++ synthetic tests passing.

## Remaining Gaps & Next Targets

1. **Creating Loan Agreements from Scratch**:
   - While loan termination/recall is fully supported (`POST /api/player/loan/terminate`), creating a brand-new loan deal between two clubs from scratch involves generating a multi-party `LOAN_CONTRACT` object and linked parent/loanee clause structures.
2. **In-Match Simulation Rules**:
   - Dynamic registration squad caps are editable, but active in-match simulation parameters (substitutions count, bench size, VAR toggle) reside in static competition rule blueprints ('sqsr' / 'strl' nodes).
3. **Unmapped Entity Fields**:
   - Club: Board patience and fan confidence remain unmapped.
   - Nation: Flat tax rate, corruption index, languages, and FIFA world ranking points.

## Rules
Max 1,500 LOC/file; verification trio from `AGENTS.md`; commit locally, never push unless asked; test risky writes on a throwaway save, then Save As.
