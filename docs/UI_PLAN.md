# UI Plan: an FM26-style, easier editor

Status: implemented and integrated (2026-10-27), U0–U6. Native persistence gaps remain
explicitly unavailable or experimental; this work adds no native offsets or new write capability.

## Completion and verification

- FM26-style tokens, sidebar/history, filtered global search, Player sub-tabs, four Staff ability views,
  Club/Nation/Competition editors, Settings and read-only Diagnostics use the manifest bundle.
- One shared Load/Review/Save/Discard bar. Squad number, contract extension, familiarity and registration
  actions stage identity-bound requests; no field change or action draft writes before Save.
- The integration audit fixed missing action/role/release-clause handlers, profile facts, goalkeeper
  attribute layout, stale Player Save callback, contract gates, draft loss on screen changes and save races.
- U6: compact navigation remains available, controls/save actions wrap, text contrast meets 4.5:1,
  reduced motion is respected, keyboard editing works, and all editors use consistent card spacing.
- User spacing follow-ups: Cheats use two balanced columns, padded rows, separated status labels and
  aligned action buttons; Quick Actions group Presets, Squad Number, Contract and Familiarity into cards.
  Overlay mode stacks these groups, keeps 12–16px content padding and 36px primary control targets.
- Verification: 124 Node tests, workspace compile, 153 Rust tests (1 ignored) and 58 C++ synthetic tests. Browser review covers desktop and
  compact Player/Quick Actions/Cheats plus other editor navigation. The two README screenshots are
  explicitly demo captures (FM26 was closed); no live writes or new persistence claims were made.

Reference: FM26 player profile (Federico Chiesa) and club profile (Juventus) screenshots from the user's game.

## 1. Goals

1. **Looks like FM26.** Someone who knows the game can find any field where they expect it: same page
   structure, same attribute layout, same colours for values.
2. **Easier to use.** One way to find things (search), one way to edit (click the value), one place to
   save (a save bar), and clear status for every control (verified, experimental or unavailable).
3. **No change to the safety rules.** Native writes stay transactional, unverified fields stay
   disabled with a reason, and an untouched control never writes (the Phase 11 rule).

Constraints: the manifest sources assemble into one embedded HTML response with plain HTML/CSS/JS (no frontend build pipeline, per `AGENTS.md` rule 1). Existing element IDs and function names that the Node tests use are kept, or the
tests are updated in the same commit.

## 2. What FM26 does that we copy

| FM26 pattern (from the screenshots) | Today in FMLE | Change |
| --- | --- | --- |
| Left sidebar with icon + label per area (Home, Squad, Staff, Club Info, Finances…) | 10 tabs in one top row | Left sidebar: Player, Staff, Club, Nation, Competition, Cheats, Settings, Diagnostics |
| Top context bar: number + name, positions and club as subtitle, back/forward | Name inside each card | One context header per entity with name, subtitle (positions · club), back/forward history |
| Sub-tabs under the header (General, Contract, Transfer, Development…) | Separate top-level tabs (Positions, Contract) | Sub-tabs inside the entity: Player → Profile · Positions · Contract · Registration · Role presets |
| Three attribute columns (Technical, Mental, Physical) with tight rows, alternating shading, value right-aligned | Attribute boxes in groups | Same three-column table; goalkeepers get the Goalkeeping column instead of Technical |
| Value colour by level; key attributes for the selected role highlighted | Tier colours exist (`--fm-elite`…) | Match FM's colour bands; highlight the key/secondary attributes of the role selected in Role Presets |
| Left profile card: picture, CA/PA stars, reputation, contract, value, height/weight, feet | Spread across tabs | One profile card on the left of every player page; units and money via the Settings formatter |
| Pitch with position dots + role list with stars (Roles panel) | Pitch editor exists | Keep the pitch; add the FM-style role list next to it, fed by Role Presets |
| Staff profile: Preferred jobs list, ability tabs (Head Coach, Coaching, Scouting, Medical), labels like "Very Good" | Flat Staff tab | Staff page with the same tabs and labels; number shown next to the label |
| Club profile: info grid (position, founded, value), facility stars, finance summary | Club tab with forms | Club page in the same grid; facilities as stars + 1–20 value; finance summary box |

Not copied: player photos and club badges (not available without FM's graphics; use initials and a
neutral silhouette), live match/news content.

## 3. Interaction model

- **Global search in the header** across players, staff, clubs, nations and competitions, with type
  chips. Selecting a result opens its page. Replaces the per-tab search boxes.
- **Click a value to edit it.** Values look like FM's read-only numbers until clicked; then an inline
  1–20 stepper (or the right input) appears. Escape cancels, Enter keeps the edit.
- **One save bar.** A sticky bar at the bottom: "3 unsaved changes · Review · Save · Discard". Review
  opens a diff (field, old → new), the same pattern as Role Presets. Replaces scattered Save buttons. Every edit remains pending until explicit Save.
- **Status everywhere, one style.** Every editable value carries one of three marks: verified (no mark),
  experimental (small amber dot, opt-in), unavailable (greyed, tooltip says why). The "unavailable"
  banners per card go away.
- **Pending actions** (presets, transfers) appear in the same review list before Save.
- **Keyboard:** `/` focuses search, `Ctrl+S` saves, arrow keys move between attribute cells.

## 4. Phases

Each phase is one commit with its tests, as the other phases in `AGENTS.md`.

| # | Phase | Contents | Tests |
| --- | --- | --- | --- |
| U0 | Design tokens | FM26 palette (navy surfaces, teal accent, FM value colour bands), type scale, spacing; light theme dropped or kept as an option | Node: tokens defined, both value bands map 1–20 |
| U1 | App shell | Sidebar, context header with history, sub-tab row, global search; old tabs mapped into the new structure | Node: navigation reaches every former tab; search result opens the right page |
| U2 | Player page | Profile card + three-column attribute table + Positions/Roles + Contract + Registration Doctor as sub-tabs | Node: all existing save tests pass with the new markup; untouched cells never write |
| U3 | Save bar | Unified pending changes, review diff, save/discard, `Ctrl+S`; remove autosave-on-change | Node: only changed requests per entity, only changed fields, discard restores values |
| U4 | Staff page | Preferred jobs, Head Coach/Coaching/Scouting/Medical tabs, label + number, experimental edit through the save bar | Node: labels match the 1–20 bands; edits go through the experimental gate |
| U5 | Club, nation, competition pages | Info grid, facility stars, finance box, nation rules card, competition rules and prizes | Node: live-editor gating and changed-fields rules still hold |
| U6 | Polish | Keyboard, focus states, contrast check, compact overlay mode on the new shell, narrow-window layout | Node + manual check in the browser pane at overlay and desktop widths |

## 5. Decisions (user, 2026-10-27)

1. **Fidelity:** about 80% close to FM26 (layout, value colours, density); keep FMLE branding.
2. **Language:** English only.
3. **Saving:** no autosave anywhere and no autosave label. Every change needs an explicit Save. Each
   page has one Load (read from the game) and one Save button, e.g. club money uses the same pair.
4. **Value colours:** FM's 1–20 bands.

## 5b. File structure (done in step 1)

- `src/ui/index.html`: document shell with `/*@STYLES*/`, `<!--@PAGES-->` and `/*@SCRIPTS*/` markers.
- `src/ui/manifest.json`: style, page and script files in load order. `build.rs` embeds every listed
  file with `include_str!`, so adding a file needs no Rust change; `ui_bundle.rs` assembles the
  document once in memory (one request, no disk access at runtime).
- `src/ui/styles/*.css`, `src/ui/pages/*.html` (one per page), `src/ui/js/core/*.js` (shared) and
  `src/ui/js/pages/*.js` (one per page). Scripts are classic scripts concatenated in manifest order.
- `tests/helpers/ui.cjs` assembles the same document for the Node tests.

## 6. Ongoing risks

- The assembled asset is large; edit the focused manifest sources, never an assembled copy.
- Classic scripts share globals. The handler audit checks every static inline callback and unique element ID.
- DOM tests do not replace browser layout checks or native simulation evidence. Keep those claims separate.
