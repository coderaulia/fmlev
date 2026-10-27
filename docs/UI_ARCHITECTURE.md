# UI Architecture

The web UI is plain HTML, CSS and classic JavaScript under `crates/fmlev-server/src/ui/`. No build tools, no frameworks, no network assets. `build.rs` embeds every file listed in `manifest.json`, so adding a file only needs a manifest entry. Scripts are concatenated in manifest order into one `<script>` (one shared global scope; inline `onclick` handlers call globals).

User decisions this structure implements: about 80% FM26 look, English only, **no autosave anywhere**, one Load and one Save per page, FM value colour bands.

## File map

```
ui/
  index.html            document shell: sidebar, top bar, context header, sub-tab row, <main> pages, save bar
  manifest.json         styles, pages, scripts in load order
  styles/
    tokens.css          :root palette, value-band tokens, spacing, radius, type scale
    base.css            reset, body, scrollbars, .tab-view visibility, utilities
    components.css      buttons, pills, cards/panels, tables, attribute table, bands, forms, stepper, toast
    shell.css           sidebar, top bar, search, context header, sub-tabs, save bar, overlay mode
    pages/              player.css, staff.css, club.css (club+nation+competition), tools.css (cheats+settings+diagnostics)
  pages/                one <section id="tab-..." class="tab-view"> per page
  js/core/              shared code (helpers, http, format, state, shell, savebar, app, sync, pin-attach, search, input-guard, tools)
  js/pages/             per-page code
```

### Load order (manifest `scripts`)

core: `helpers, http, format, state, shell, savebar, app, sync, pin-attach, search, input-guard, tools`
pages: `staff, eligibility, role-presets, attributes, player, club, player-actions, player-ca, player-save, godmode, contract, positions, club-edit, club-money, nation, competition, competition-prizes, settings`

Top-level code must only declare functions and constants: tests run the whole script in a vm sandbox with a fake `document` (`getElementById` returns stubs or `null`, `querySelectorAll` returns `[]`), so every DOM access must tolerate `null` and missing APIs. Use `document.querySelectorAll?.(...)` style guards in code that tests exercise. `SaveBar.register(...)` at load time is fine (core loads first).

## Pages and sections

| Sidebar item | Shell section id (= SaveBar page id) | Tab sections |
|---|---|---|
| Player | `player` | `tab-player` (Attributes), `tab-positions`, `tab-contract` (sub-tabs) |
| Staff | `staff` | `tab-staff` |
| Club | `club` | `tab-club` |
| Nation | `nation` | `tab-nation` |
| Competition | `competition` | `tab-competition` |
| Cheats | `cheats` | `tab-godmode` |
| Settings | `settings` | `tab-settings` |
| Diagnostics | `diagnostics` | `tab-status` |

`switchTab(tabId)` is the navigation API and still works for every `tab-*` id.

The Club page now registers its own SaveBar adapter in `js/pages/club-money.js`:
`loadClubPage`, `saveClubPage`, and `clubPageChanges`. It replaces the legacy Club adapter.
Load reads the selected context, native facilities and native money; Discard reloads those values and
removes that club's queued actions. Save finishes manual facility/money edits before queued actions.
There are no per-card Club Load/Save buttons. Money inputs stay disabled until a successful native read.
`club.js` owns facility metadata, native-read caching, stars/value bands and the information grid;
unread facility ratings display a dash and cannot be edited. Other pages must not mutate that cache.

Nation owns `syncNationNative`, its field baseline, and its SaveBar adapter in `js/pages/nation.js`.
The supported form fields are youth rating, reputation, economic factor and the raw development code;
population is read-only. Disabled/unread fields are preserved in submitted records. `loadNationPage`
reads before replacing drafts; Discard uses that reload path. Native refreshes reject stale identities
and reads superseded by an explicit Load or Save.

Competition owns `syncCompetitionNative` and reputation editing in `js/pages/competition.js`.
`stageCompetitionRegistration` queues the existing registration endpoint with the current index/UID;
the button itself makes no request. `competition-prizes.js` registers the Competition SaveBar adapter.
Its single Load always reads reputation and optionally reads prizes when lookup criteria are supplied.
Prize baselines bind the competition ID, raw-GBP lookup criteria and complete loaded table. Changed
criteria require another Load; prize-only saves never send a reputation update. Save applies manual
reputation/prize edits before queued actions; changes made during a request wait for another Save.
Discard restores loaded reputation/prizes and removes that competition's queued actions without a write.
The native helper definitions were moved out of `player-actions.js` into their owning page scripts.

## Shell API (`js/core/shell.js`)

```js
switchTab("tab-club");                       // same as Shell.navigate("tab-club")

// Entity name and subtitle in the context header. Pass `section` so a poll that updates
// the player while the user is on the Club page does not touch the Club header.
Shell.setContext({ section: "club", title: "Barcelona", subtitle: "La Liga", kind: "club", id: 1074 });
// `open` (optional) re-opens the entity for back/forward; without it, kind player|club|nation|competition
// calls selectSearched*(id). One history entry is added per distinct title per section.
Shell.setContext({ section: "staff", title: "Pep Guardiola", subtitle: "Staff", open: () => selectStaff(79, 1120) });

// Sub-tab row. `onSelect` per item or per call; default for ids starting "tab-" is switchTab(id).
Shell.setSubTabs([{ id: "money", label: "Finances" }, { id: "facilities", label: "Facilities" }], "money",
                 { section: "club", onSelect: id => showClubPane(id) });
Shell.setActiveSubTab("facilities", "club");

Shell.back(); Shell.forward();               // entity history
Shell.onNavigate((tabId, section) => {});    // listener
Shell.syncEntities(state);                   // called by fetchContext(); derives headers from state.active*
```

The context poll already fills the header for player/club/nation/competition (title and subtitle from `name`, `position`, `club_name`, `league_name`, `continent`, `nation_name`); pages may override with `setContext`. Keys handled globally by `Shell.handleKey`: `/` focuses search (unless typing), `Ctrl/Cmd+S` saves the active page, `Escape` closes the Review popover.

### Global search (`js/core/search.js`)

`performSearch` queries `/api/search?q=` (players, clubs, nations, competitions) and `POST /api/staff/search {query}` (2+ letters) in parallel; a staff failure never hides the other results. Staff results call `selectSearchedStaff(index, uid)`: opens `tab-staff`, calls `selectStaff(id, uid)` and sets the header. Arrow keys move through results, Enter opens one.

## SaveBar API (`js/core/savebar.js`)

One sticky bar for the active page: `N unsaved changes · Review · Discard · Save`, plus the single **Load from game** button.

```js
SaveBar.register("club", {
  load:    async () => { /* read current values from the game into the page */ },   // optional; hides Load if absent
  save:    async () => { /* write edits; return false or throw on failure */ },
  discard: async () => { /* revert edits to last loaded values; defaults to load() */ },
  changes: () => [{ label: "Training facilities", from: 18, to: 10 }],              // drives count + Review list
});
SaveBar.markDirty("club");   // an input changed (call from oninput/onchange handlers; never save there)
SaveBar.clear("club");       // after the page reloaded its values on its own
await SaveBar.save();        // what the Save button and Ctrl+S call (active page)
await SaveBar.load(); await SaveBar.discard();
```

Rules:
- Nothing sends a request until Save. A page is dirty when `changes()` is non-empty or `markDirty` was called since the last successful save. With only `markDirty`, the count shows "1 unsaved change".
- `save()` returning `false` or throwing keeps the dirty state and shows the error in the bar (thrown messages also toast). Edits made while a save is running stay dirty.
- Load confirms before replacing dirty edits (if `confirm` exists); Discard explicitly reverts them.
- `pageId` = Shell section id. Pages without an adapter (Cheats) hide the bar. Diagnostics registers a read-only Load adapter; Staff has its experimental Save adapter.
- Compatibility: until a page registers itself, `player/club/nation/competition` use an adapter over `state.playerDirtyFields`, `state.clubDirty`, ... and `pendingActions`, saving through `applyPlayerChanges` / `saveClubToGame` / `saveNationToGame` / `saveCompetitionToGame` and loading via `/api/context/select`. A page that calls `SaveBar.register` replaces its adapter. The bar re-renders every 500 ms so legacy flag changes show up.
- `autoSave*`, `debouncedSave*`, `toggleAutoApply` no longer save. They only mark dirty and exist for old markup; page agents should replace them with `SaveBar.markDirty(...)`.

## Value bands and CSS

Tokens (`tokens.css`): surfaces `--bg-dark --bg-sidebar --bg-header --bg-card --bg-card-alt --bg-input --bg-row-alt --bg-row-hover`, borders `--border-color --border-light`, text `--text-main --text-muted --text-dim`, accent `--accent --accent-hover --accent-contrast --accent-bg` (teal), status `--success --warning --danger --info`, bands `--band-1-5 --band-6-10 --band-11-15 --band-16-20 (+ -bg)`, spacing `--space-1..5`, radius `--radius-sm/md/lg`, type `--fs-xs..2xl`. Legacy names (`--accent-cyan`, `--fm-elite`, ...) stay defined.

Value helper (`js/core/format.js`): `valueBand(v)` returns `band-1-5 | band-6-10 | band-11-15 | band-16-20 | band-none`; `valueBand(v, 200)` scales another range onto 1-20.

```js
`<span class="attr-line-val ${valueBand(attr)}">${attr}</span>`
```

Colour scheme: `body[data-band-scheme="legacy"]` swaps the bands to the old red/grey/amber/green. `applyBandScheme("fm26"|"legacy")` previews it; `js/pages/settings.js` persists the choice only on Save (localStorage key `fmle.bandScheme`). The Settings control id is `settings-band-scheme`; its `onchange` should call `markSettingsDirty()`.

Component classes (`components.css`): buttons `.btn .btn-primary .btn-ghost .btn-danger .btn-sm`; `.pill .pill-accent .pill-warn .pill-danger` plus legacy `.pill-badge .badge-*`; `.card`, `.panel .panel-head .panel-body`, `.notice (.warn .error)`, `.unverified-note`; table `.fm-table` (alternating rows, `.num`); FM three-column attribute table `.attr-table > .attr-col > .attr-col-head + .attr-line` with `.attr-line-name`, `.attr-line-val`, `.key-attr` (row highlight), `.unavailable`; bands `.band-1-5 .band-6-10 .band-11-15 .band-16-20 .band-none` (+ `.band-bg`); forms `.form-grid .form-group .form-label .form-control (.changed)`; `.stepper`, `.seg`; `.toast`. Shell classes: `.nav-item`, `.subtab`, `.review-row`, `#savebar`.

Unverified or unavailable fields stay `disabled`/read-only with a visible reason (`.unverified-note`); never show fixtures or defaults as game data. Untouched money inputs must keep raw baselines (`setMoneyInput` / `readMoneyInput`).

## SOURCE OWNERSHIP

| Owner | Files |
|---|---|
| Player | `pages/player.html`, `pages/positions.html`, `pages/contract.html`; `js/pages/{attributes,player,player-ca,player-actions,player-save,contract,positions,role-presets,eligibility}.js`; `styles/pages/player.css`; tests `ui-save.test.cjs`, `role-presets.test.cjs`, `eligibility.test.cjs`, new `player-page.test.cjs` |
| Staff | `pages/staff.html`, `js/pages/staff.js`, `styles/pages/staff.css`, `tests/staff.test.cjs` |
| Entities | `pages/club.html`, `pages/nation.html`, `pages/competition.html`; `js/pages/{club,club-edit,club-money,nation,competition,competition-prizes}.js`; `styles/pages/club.css`; tests `live-editors.test.cjs`, `entity-audit.test.cjs`, new `club-pages.test.cjs` |
| Tools | `pages/godmode.html`, `pages/settings.html`, `pages/status.html`; `js/pages/{godmode,settings}.js`; `styles/pages/tools.css`; tests `settings.test.cjs`, new `tools-pages.test.cjs` |
| Shared (read-only for page agents) | `index.html`, `manifest.json`, `js/core/*`, `styles/{tokens,base,components,shell}.css`, `tests/helpers/*`, `tests/shell.test.cjs` |

Page agents needing a shared change (new token, component class, core hook) report it instead of editing shared files. Tests can use `tests/helpers/fake-dom.cjs` (`createUi({ fetch, ids, tabs })`) for a vm sandbox with elements, multi-listener events and recorded requests.

## Finished shell and spacing contract (2026-10-27)

The compact shell retains the sidebar as a horizontal scrollable navigation row. Main content uses
24px desktop padding and 16px vertical/12px horizontal compact padding; panels use 16–20px body
padding, 16–20px gaps, wrapping controls and visible focus outlines. Cheats use two columns on desktop,
one on small/overlay layouts; their status badges occupy a separate line. Quick Actions use a full-width
preset group and three action groups, stacked in compact mode. Primary compact controls are at least
36px tall. Search type chips filter all five entity kinds without inventing missing ratings.

Attributes support Up/Down focus, Enter to keep an edit and Escape to restore its value at focus.
The Staff section has Head Coach, Coaching, Scouting and Medical views. Role lists show native position
familiarity, not a fabricated role-suitability score. Missing contract write capabilities fail closed.
Unverified transfers and unsupported cheats remain disabled; no direct action button performs a game write.

Validation includes every static inline callback, unique IDs, staged request bodies, race/identity checks,
contrast ratios and normal/compact browser review. See `tests/ui-wiring.test.cjs` and the per-page tests.

Overlay notifications stay centered inside the compact editor rather than at the desktop edge.
The active navigation item scrolls into view on navigation or when compact mode is enabled.
Player summary facts bind to fields present in the API; missing reputation/value/measurements remain
unavailable. Unsupported contract controls and unread position/attribute values cannot become writes.
