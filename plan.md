# Plan: Staff editor, player/nation enhancements and display settings

Status (2026-10-27): A (settings), B (role presets) and D (read-only registration doctor) implemented. B still needs the in-game proof and an FM26 role-list check. C: discovery logged in `AGENTS.md`, waiting for screen comparison. E: PPM bitset found, bit-to-move table incomplete. F: staff records located, attributes not mapped.
Date: 2026-10-26.
Scope: six workstreams (A–F below). Each one is shippable on its own and gets its own phase commit.

All work follows `AGENTS.md`: discovery in `tools/fmscan` (read-only C++), product code in Rust,
the evidence ladder before any field reaches the UI, transactional writes with readback and
rollback, source files ≤ 1,500 LOC, and the three verification suites before each commit.
Anything without proof returns 501 from its route, shows "unavailable" in the UI and is
marked "unconfirmed" in the docs.

---

## 1. Baseline (what already exists)

| Area | Already verified | Gap for this plan |
| --- | --- | --- |
| Player attributes, CA/PA, positions, feet | Native reads + writes (`native_player.rs`), screen-independent discovery | Role presets only need to compose these writes |
| Registration | Manager bits `0x100`/`0x200` at manager+0x404, stage gate +0x9a, squad cap +0x98/+0xd6, homegrown quota +0x99, comp type +0xcd | No per-player eligibility read, no explanation of *why* registration fails |
| Preferred moves | Nothing | New discovery |
| Nation | Economic factor (+0x1f0), development raw code (+0x1ec), population (+0x1e4), youth/reputation per gender | Development enum labels unverified; economic factor and development not verified in game; nationality rules unmapped |
| Staff | Only the human manager object (`HUMAN_NON_PLAYER` RTTI, manager+0x404 flags) | Everything: discovery, model, routes, UI tab |
| Money/units display | Raw values are GBP; contract wage is weekly GBP; UI label hardcodes `USD ($)` and `$ / week` | No settings model; club wage-budget period still unverified |

---

## 2. Delivery order (recommended)

| # | Workstream | Game-memory risk | Why this order |
| --- | --- | --- | --- |
| A | Settings: currency, measurement, wage units | None (display only) | Fixes wrong `USD` labels now; every later tab uses the formatter |
| B | Player role presets (3 levels) | Low (reuses verified writes) | No new offsets |
| C | Nation enhancements | Low to medium | Two of five fields already read natively |
| D | Player registration & eligibility doctor | Read-only first | Mostly diagnostics on top of known stage-rule layout |
| E | Player preferred moves | Medium (new layout) | Needs discovery, small surface |
| F | Staff editor (8 tabs) | Medium | New entity type; largest discovery effort; split into sub-phases; Swap Staff read-only |

---

## A. Settings: currency, measurement and wage units

Goal: the user chooses how values are **displayed and entered**. Native storage stays raw GBP
and native units. Settings never write game memory.

### A1. Settings model and persistence
- New `crates/fmlev-core/src/settings.rs`: `DisplaySettings { currency, measurement, wage_period }`.
  - `currency`: ISO code (GBP, EUR, USD, IDR, …) with symbol and decimals.
  - Rates: **fixed table only** (decision 2026-10-26). One shipped, dated rate per GBP for each currency. No custom rates, no matching of FM26's in-game rate, no network fetch.
  - `measurement`: `metric` | `imperial` (height cm vs ft/in, weight kg vs lb, distance km vs miles).
  - `wage_period`: `weekly` | `monthly` | `yearly`.
- Persist to `${XDG_CONFIG_HOME:-~/.config}/fmlev/settings.json`; missing file → defaults (GBP, metric, weekly).
- Routes: `GET /api/settings`, `POST /api/settings` (`deny_unknown_fields`, validated codes and rates > 0).

### A2. One formatter/parser in the UI
- Add a single `formatMoney(rawGbp, {period})` / `parseMoney(text) -> rawGbp` and `formatHeight`/`formatWeight`/`formatDistance` pair in `ui.html`.
- Replace every hardcoded `$`, `USD ($)`, `$ / week`, `+50k` step and `toLocaleString()` money display with the formatter. Step buttons step in display units.
- New "Settings" tab (or gear dialog in the header) with currency, measurement and wage-period selectors, the fixed rate shown read-only with its date, and a live preview line.

### A3. No phantom writes (lesson from the Phase 11 stale-cache bug)
- Each money/unit control keeps its **raw native baseline** in a data attribute. Save compares the parsed raw value with the baseline; if the user did not edit the control, nothing is sent, even when currency rounding makes the display differ.
- Weekly → monthly/yearly conversion uses FM's convention (yearly = weekly × 52, monthly = yearly / 12); parse back rounds to whole GBP per week and shows the effective stored value after save.
- Club wage budget/max wage: label them "raw (period unverified)" until their period is compared with the in-game Finances screen (open item from `docs/NATIVE_ENTITY_READS.md`). Do not apply the wage-period conversion to them before that check.

### A4. Optional: read FM26's own display currency
- FM26 stores its display currency as a game preference. Read-only discovery only; offer "match game currency" if found. Changing the game's preference from the editor is out of scope.

### Tests / done
- Rust: settings load/save/defaults/validation. Node DOM: every money input round-trips raw → display → raw without change; untouched controls produce no write; labels follow settings.
- Done when no hardcoded currency symbol remains in `ui.html` and the Phase 11 "untouched control writes nothing" tests still pass.

---

## B. Player position and role presets (3 levels)

Goal: one click raises a player toward a role profile, e.g. *Deep-Lying Playmaker* Level 1/2/3.
Uses only the verified attribute, CA/PA and position write paths.

### B1. Role data
- New data file `crates/fmlev-core/src/role_presets.rs` (or embedded JSON) holding, per role:
  `id`, `name`, `positions` (position keys that must reach a minimum aptitude), `key` attributes, `secondary` attributes.
- Role names and key/secondary attribute lists must be taken from FM26's own role screens (FM26 split roles into in/out-of-possession roles). Do not reuse an older FM role table without checking it against the game.
- Example shape (attribute names match `PlayerAttributes`; final lists confirmed in game):

| Role | Position aptitude | Key | Secondary |
| --- | --- | --- | --- |
| Deep-Lying Playmaker | DM, MC ≥ level target | passing, vision, composure, decisions, teamwork, technique, first_touch | anticipation, positioning, balance |

### B2. Levels
| Level | Key attributes | Secondary | Position aptitude |
| --- | --- | --- | --- |
| 1 — Competent | ≥ 14 | ≥ 12 | ≥ 15 (Accomplished) |
| 2 — Strong | ≥ 16 | ≥ 14 | ≥ 18 |
| 3 — Elite | ≥ 18 | ≥ 16 | 20 (Natural) |

- Presets only **raise** values to the floor; they never lower an attribute or position.
- CA handling (decision 2026-10-26): **Level 3 raises CA and PA by default** (`raise to fit`: CA rises so the new profile fits; PA rises to at least the new CA). Levels 1 and 2 default to `keep`, with `raise to fit` as an opt-in. The user can switch the mode for any level before Save.
- CA/PA changes always appear in the diff preview before Save, so the rule "UI never silently replaces CA" still holds.
- Goalkeeper roles use the goalkeeping attribute block.

### B3. Wiring
- `POST /api/player/role-preset {player_id, role_id, level, ca_mode}` returns a **proposal** (field → old/new) without writing.
- The UI shows a diff, stages it as a pending action, and Save sends it through the existing `changed_fields` player save (single transaction, exact-byte rollback).
- UI: role picker grouped by position on the Positions tab, three level buttons, diff preview.

### Tests / done
- Rust: floor-only logic, no lowering, CA modes (Level 3 default raises CA/PA, Levels 1–2 default keep), GK roles, unknown role/level rejected. Node: proposal preview and staged Save send only changed fields.
- Live: apply DLP Level 2 and Level 3 to one player on a throwaway save, check Attributes and Positions screens after a day tick, save/reload, restore.

---

## C. Nation enhancements

| Field | Status today | Work |
| --- | --- | --- |
| Development | Raw code at nation+0x1ec (`Nsod`), labels not verified | Verify label mapping in game; then expose as enum with labels |
| Economic factor | nation+0x1f0 (`Necf`) read/write in Rust, not verified in game | In-game verification only |
| Years to gain nationality | Unmapped | Discovery |
| Gain nationality type (Accumulative / Continuous) | Unmapped | Discovery |
| Does not allow dual nationality | Unmapped | Discovery |

### C1. Discovery (read-only, `fmscan`)
- Start from the nation serializer near RVA `0xf7c7b3..0xf7cd6b` (where `Necf` and `Nsod` are read) and list the other FourCC tags it reads. Nationality rules are expected to be small scalars (u8 years, u8/enum type, bool/bit for dual nationality) on the nation record or a sub-object.
- Cross-check across several nations whose values differ in the FM26 pre-game editor or Nation > Information screen. Record every value observed and what was not proven.
- Extend `fmscan entity ... nation --editor` with `nationality_years`, `nationality_type`, `dual_nationality_allowed` plus evidence and synthetic-process tests.

### C2. Rust read/write
- Extend `native_nation.rs` and `NationDetails`: `nationality_years` (range from the editor, expected 0..=255), `nationality_type` (enum), `dual_nationality_allowed` (bool), and `development` as a labeled enum once verified.
- Writes go through `update_live_nation_checked` (changed fields only, readback, rollback).
- `/api/nation/native` returns the new fields; unknown values are `null` with a reason.

### C3. UI
- Nation tab card "Nationality rules": years (number), type (select), dual nationality (toggle), each showing verified/unverified state. Development becomes a labeled select.

### C4. In-game proof
- These rules act only when the game processes a nationality change, which can take months of game time.
- Decision 2026-10-26: "verified" requires **both** proofs:
  1. Value proof: the new value shows in game where the game displays it, and survives a day tick and save/reload.
  2. Behavior proof: on a throwaway save, a foreign player at a club in the edited nation gains (or, for the dual-nationality flag, keeps or loses) nationality according to the new rule after enough game time.
- Until the behavior proof is done, the fields ship as writable but marked "value verified, behavior unconfirmed" in the UI and docs.

---

## D. Player registration and eligibility doctor

Goal: when a player cannot be registered, show the exact blocking reason and the fix the
editor can apply (if any). Mostly read-only.

| Problem | Detection | Fix offered |
| --- | --- | --- |
| Registration window closed | Stage gate +0x9a = 0 and manager bit 0x100 clear | Existing **Allow Squad Registration Anytime** (link to the toggle) |
| Continental squad registration missing | Competition type +0xcd and the competition's registration type (to discover) | Guidance: check that competition's registration type and deadline; enable Register Anytime |
| Italian non-EU quota (Serie A) | Player nationality + EU status, club's non-EU signing slots for the season (to discover) | Step-by-step Serie A guidance in the panel; write support only after discovery and proof |
| Work permit required | Read-only | None. Copy the exact in-game eligibility message into a support report |
| Appearance limit or other eligibility rule | Read-only | None. Record competition and exact message in the report |
| Too few home-grown players | Stage rule +0x99 quota; club-/nation-trained status from the player's training history (to discover) | Guidance: nationality alone does not make a player home-grown; training history decides |

### D1. Reads
- `POST /api/player/eligibility {player_id, comp_index, comp_uid}` → per-gate `{status: ok|blocked|unknown, evidence, fix?}`. Unknown gates are reported as unknown, never as ok.
- Discovery tasks (read-only `fmscan`): competition registration type, non-EU slot counters, player training-history vector (club/nation-trained flags), work-permit status if stored on the player.

### D2. Support report
- "Copy support report" button: JSON/text with player id/uid, competition id/uid/type, every gate value, the user-pasted in-game message, build PE timestamp, fmlev version. No personal data beyond game ids.

### D3. UI
- Panel on the Player tab (or Contract tab) with competition picker, gate list, fix buttons and the report box.

### D4. Test target
- Decision 2026-10-26: live testing uses **La Liga** (Spanish First Division, 51/67, whose stage gate, squad cap 25 and home-grown quota 8 are already mapped). Gates that La Liga cannot exercise (Serie A non-EU quota, continental registration) stay "unconfirmed" until tested.

---

## E. Player preferred moves (add / remove)

### E1. Discovery
- Find where preferred moves live on the player record. Likely layouts: a bitfield (one bit per move) or a small vector of move ids. Use `fmscan diffset` while adding/removing one move in the FM26 pre-game editor or after a "learn move" event, and `struct`/`graph` to confirm ownership.
- Use the FMSuperScout field map (already cited in `LIVE_READ_WRITE.md`) as a lead only. Build the full move-id → name table from the game.

### E2. Write rule
- **Bitfield:** scalar toggle with readback/rollback. Allowed.
- **Vector:** only edit entries inside the existing allocation (replace one id with another). Adding beyond capacity or allocating is forbidden by Rule 7; removal must not leave the game iterating an inconsistent vector. If the layout is a vector without spare capacity, ship read + replace only.

### E3. Wiring
- Add `preferred_moves: Vec<MoveId>` to `Player` (read), `POST /api/player/moves {player_id, add[], remove[]}`, a checklist on the Player tab, Node tests, in-game check on the player's profile after a day tick and save/reload.

---

## F. Staff editor

New top-level **Staff** tab with eight sub-tabs. Split into sub-phases; each sub-phase ships
read-only first, writes only after the evidence ladder.

| Sub-tab | Content | Read | Write (target) |
| --- | --- | --- | --- |
| Information | Personal details, staff attributes | F1 | F2 (attributes) |
| Contract | Club and national-team contracts | F3 | F3 (wage/expiry, if same contract layout as players) |
| Career Plans | Preferences, desired jobs and their conditions | F5 | Read-only unless scalar |
| Swap Staff | Swap between eligible club / national-team jobs | F6 | None (read-only) |
| Training Rating | Coaching star ratings per training area | F4 | Via underlying attributes |
| Formations/Tendencies | Preferred formations, staff tendencies | F4 | F4 (scalars) |
| Relationships | Relationships with people, teams, nations | F5 | Edit existing entries' values only |
| Knowledge | Country knowledge entries and values | F5 | Edit existing entries' values only |

### F1. Discovery and identity (read-only)
- Non-player staff use a person record like players (`PERSON` sub-object with index/uid). Find the AI staff RTTI (expected a `NON_PLAYER`-family `db` type; the human manager is `HUMAN_NON_PLAYER`).
- Add `fmscan entity PID staff --name TEXT [--editor] --json` with vtable + index + duplicate UID + ownership checks, and synthetic tests.
- Rust `crates/fmlev-core/src/native_staff.rs`: `NativeStaffRecord::discover_by_index`, name, date of birth, nationality, current job/club. Search integration in `/api/search` (type `staff`).

### F2. Information: staff attributes
- Map the staff attribute block (coaching, mental, knowledge, medical and data groups; exact list taken from FM26's staff profile, not assumed). Determine the scale (1..20 display vs native 0..100 like players).
- Writes: same pattern as player attributes (changed fields only, one transaction, exact-byte rollback). Also staff CA/PA if present.

### F3. Contract
- Follow the staff contract pointer(s): club job and national-team job. If RTTI is `FULL_CONTRACT`, reuse the existing wage/expiry writer and ownership checks; otherwise map a new layout.
- Show job title/role, wage (via settings formatter), start/expiry. Termination and new contracts are out of scope (game-allocated objects).

### F4. Training Rating and Formations/Tendencies
- Training Rating: first determine whether star ratings are **stored** or **computed** from attributes. If computed, display the computed value and make edits through the underlying attributes ("raise to N stars" proposal, like role presets). If stored, it is a cache: do not write it.
- Formations/Tendencies: preferred and second formation (ids → names table) and tendency scalars. Scalar writes with validation.

### F5. Career Plans, Relationships, Knowledge (vectors)
- Map each vector with `fmscan vec`/`graph`: element stride, owner back-pointers, value ranges.
- Ship read-only lists first. Value edits are allowed only inside existing entries (e.g. knowledge 0..100 for an existing country). Adding/removing entries is out of scope unless spare capacity and game iteration rules are proven (Rule 7).

### F6. Swap Staff (read-only)
- Decision 2026-10-26: **read-only**. No swap write is built.
- Swapping two staff between jobs would change contract owners, team staff lists and possibly national-team containers; Rule 7 keeps such pointer swaps out of scope.
- Ship a read-only view: each staff member's current club and national-team jobs and the eligible swap candidates (same job type, eligible club/nation), with the reason a candidate is not eligible.

### F7. UI and routes
- Routes in a new `crates/fmlev-server/src/routes/staff.rs`: `POST /api/staff/native`, `/api/staff/update`, `/api/staff/contract`, `/api/staff/eligibility` (read-only swap candidates). All with `deny_unknown_fields` and identity selectors (`staff_id` + uid).
- `ui.html`: Staff tab with the eight sub-tabs, each control showing verified/unverified state; unverified sub-tabs render read-only with "unavailable" reasons.
- Node DOM tests per sub-tab; live-editor depth-0 card rules from Phase 9.

---

## 3. Per-phase checklist

1. Discovery logged (command, live values, what was not proven) in `AGENTS.md` or `docs/`.
2. Rust function + unit tests; files ≤ 1,500 LOC.
3. Route with strict validation; 501 for unproven writes.
4. UI control with verified/unverified state + Node test.
5. `cargo check --workspace --all-targets --examples`, `cargo test --workspace`, `node --test crates/fmlev-server/tests/*.test.cjs`, and the `fmscan` suite when C++ changed.
6. In-game proof on a throwaway save (screen check, day tick, save/reload) before the README support table says "verified".
7. Update `README.md` support table, `LIVE_READ_WRITE.md`, `AGENTS.md` phase log; commit the phase locally (no push).

## 4. Decisions (2026-10-26)

| Question | Decision | Section |
| --- | --- | --- |
| Currency rate source | Fixed shipped table only | A1 |
| Role preset CA/PA | Level 3 raises CA and PA by default; Levels 1–2 keep CA unless the user opts in | B2 |
| Swap Staff | Read-only | F6 |
| Registration doctor test competition | La Liga | D4 |
| Nationality rules proof | Both value proof and behavior proof before "verified" | C4 |
