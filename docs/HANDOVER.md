# FMLE Handover

## Update — 2026-11-01 (transfer actions in the UI, club reputation)

- Contract tab: Terminate contract, Move player and Sign free agent work from the UI behind an experimental checkbox, a confirm dialog and `experimental: true` (the routes return 501 without it). Loan recall stays disabled (no in-game test). The target-club field shows the club's spare squad slots (`POST /api/club/squad-space`, read-only). Terminate, sign and move had a screen check and day tick on a throwaway save earlier; a save/reload was not confirmed.
- Club header has a reputation box (native main-team reputation, 1-10,000) saved through the existing club update path; the invented 8000 placeholder is gone. Not yet checked against the game screen.

## Update — 2026-10-31 (cheats from verified writes, native stadium name)

- Four cheats that were disabled now stage values for writes that were already verified (Clear Negative Balance, Big Money, Prize Table x10, Wonderkid Forge); nothing is written until Save. Not clicked in game yet.
- The Club page reads the stadium name natively; unreadable stadiums show Unavailable instead of a placeholder. Rust and `fmscan` agree on live clubs (PID 4496).
- Still disabled: Max Morale, Wipe Bans, Unlock IGE, Invincible Board, Golden Gen, Relax Rules, Clean Governance (no native path), and the transfer buttons (need a save/reload proof in game).

## Update — 2026-10-28 (live runbook, audit follow-ups, lock fix)

- **Live runbook (PID 5668, throwaway saves)**: search index, preferred-move bit 39, staff birth year and contract (wage, expiry), the La Liga registration doctor and a Level 3 role preset were each compared with the game screens. Details, what stayed unproven and the commands are in `AGENTS.md` and `LIVE_READ_WRITE.md` (2026-10-28 sections). Nation rules cannot be checked on screen (the Nation screen shows none) and staff reputation and CA/PA have no on-screen numbers.
- **Owner-reported (no new measurements)**: staff attribute edits, the competition prize table and role presets worked in game; no further role-preset testing is needed.
- **Staff tab**: search shows the club and also matches club names, "Open in game" and Load use the staff profile open in FM26 (`POST /api/staff/open`), recents are remembered in the browser.
- **Safety and robustness**: strict `POST /api/attach`; raw memory writes are bounds-checked and transactional; raw finance writes return 501; core errors map to 400/404/409; Register Anytime restores the original manager bits; the contract slot and transfer queue are found from process maps (no fixed addresses; the vtable address fallback is gone).
- **Performance**: native record lookups are cached after an identity check and warmed outside the global lock (`locator_cache.rs`); a cold club read is about 1.2 s and a cached one is instant, and status polling no longer waits on scans.
- **Still open**: loan signing (501), staff reputation/CA/PA/job/role, nation-rule labels, role lists vs FM26's role screens, the registration doctor's non-EU/salary/minimum-squad rules, and the free contract slot heuristic (the game's allocation counter is not updated).
- **Counts**: Rust 201 passed / 1 ignored, UI 148 passed, C++ synthetic suite 65 passed.

## Current UI handover — 2026-10-27

U0–U6 of [UI_PLAN.md](UI_PLAN.md) are integrated. Sources live under
`crates/fmlev-server/src/ui/`; edit the manifest files rather than a generated HTML asset.
The shared bar owns Load/Review/Save/Discard, including staged player and registration
actions. Compact navigation, card spacing, keyboard editing, role highlighting and all
inline handlers are checked. Staff has four ability views and explicit experimental edits.
Unverified transfers/cheats and Nation population writes are unavailable in the UI.
The historical native capability list below is not a claim that those UI controls are enabled.
Current architecture: [UI_ARCHITECTURE.md](UI_ARCHITECTURE.md). README has two labeled demo
screenshots because FM26 was closed during final capture. No new game-write evidence.


## State (Completed 2026-10-25)

Verified in game and shipped in Rust production runtime (`fmlev-core` + `fmlev-server` + `ui.html`):
- **Screen-Independent Discovery**: Full database discovery (`NativePlayerRecord::discover_by_index` and `NativeClubRecord::discover`) enables selecting, inspecting, and editing any player or club (including opposition teams, scouted players, and free agents) without needing their profile or page to be open on screen in FM26.
- **Player & Medical**: 60 attributes, positions & feet, CA/PA, weekly wage, contract extension, safe heal (10,000 condition/sharpness units without touching injury vector `+0x248`). Native attributes verified directly (`verify_native_attributes`) and contract-team-club pointers automatically resolved.
- **Opposition Club Details & Facilities**: Auto-synced when viewing any player or via `jumpToPlayerClub()`. Balance, transfer budget, training/youth/coaching/recruitment/data analysis/corporate facilities, reputations, and stadium capacities are editable live.
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

0. **Verify transfer actions in game** (terminate, loan recall, move, sign, accelerate) one at a time on a throwaway save; until then they are unverified (AGENTS.md Phase 11).

1. **Creating Loan Agreements from Scratch**:
   - While loan termination/recall is fully supported (`POST /api/player/loan/terminate`), creating a brand-new loan deal between two clubs from scratch involves generating a multi-party `LOAN_CONTRACT` object and linked parent/loanee clause structures.
2. **In-Match Simulation Rules**:
   - Dynamic registration squad caps are editable, but active in-match simulation parameters (substitutions count, bench size, VAR toggle) reside in static competition rule blueprints ('sqsr' / 'strl' nodes).
3. **Unmapped Entity Fields**:
   - Club: Board patience and fan confidence remain unmapped.
   - Nation: Flat tax rate, corruption index, languages, and FIFA world ranking points.

## Rules
Max 1,500 LOC/file; verification trio from `AGENTS.md`; commit locally, never push unless asked; test risky writes on a throwaway save, then Save As.
