const { test } = require('node:test');
const assert = require('node:assert/strict');
const { createUi } = require('./helpers/fake-dom.cjs');

const TABS = ['tab-player', 'tab-positions', 'tab-contract', 'tab-club', 'tab-nation', 'tab-competition', 'tab-godmode', 'tab-staff', 'tab-settings', 'tab-status'];
const IDS = ['sidebar-nav', 'subtabs', 'context-title', 'context-subtitle', 'context-back', 'context-forward', 'savebar', 'savebar-status',
  'savebar-review', 'savebar-load', 'savebar-discard', 'savebar-save', 'savebar-error', 'savebar-popover',
  'global-search', 'search-results-dropdown', 'search-clear-btn'];
const ok = body => ({ ok: true, json: async () => body, text: async () => '' });

function ui(fetch = async () => ok({})) {
  const app = createUi({ fetch, ids: IDS, tabs: TABS });
  app.run('Shell.init()');
  return app;
}
const active = app => TABS.filter(id => app.els[id].classList.contains('active'));

test('sidebar lists every section and switchTab shows one page', () => {
  const app = ui();
  const nav = app.els['sidebar-nav'].innerHTML;
  for (const label of ['Player', 'Staff', 'Club', 'Nation', 'Competition', 'Cheats', 'Settings', 'Diagnostics']) assert.match(nav, new RegExp(`<span>${label}</span>`));
  assert.deepEqual(active(app), ['tab-player']);
  app.run("switchTab('tab-club')");
  assert.deepEqual(active(app), ['tab-club']);
  assert.match(app.els['sidebar-nav'].innerHTML, /nav-item active" data-section="club"/);
  assert.equal(app.els['subtabs'].hidden, true);
  assert.equal(app.run('Shell.current().section'), 'club');
});

test('player sub-pages are sub-tabs of Player', () => {
  const app = ui();
  assert.equal(app.els['subtabs'].hidden, false);
  assert.match(app.els['subtabs'].innerHTML, /Attributes[\s\S]*Positions &amp; Roles[\s\S]*Contract &amp; Transfer/);
  app.run("switchTab('tab-contract')");
  assert.deepEqual(active(app), ['tab-contract']);
  assert.match(app.els['subtabs'].innerHTML, /subtab active"[^>]*>Contract/);
  app.run("Shell.selectSubTab('tab-positions')");
  assert.deepEqual(active(app), ['tab-positions']);
  assert.equal(app.run('Shell.current().section'), 'player');
});

test('pages can fill the sub-tab row and handle selection themselves', () => {
  const app = ui();
  app.run("switchTab('tab-club'); var picked = []; Shell.setSubTabs([{ id: 'finances', label: 'Finances' }, { id: 'facilities', label: 'Facilities' }], 'finances', { onSelect: id => picked.push(id) })");
  assert.equal(app.els['subtabs'].hidden, false);
  assert.match(app.els['subtabs'].innerHTML, /subtab active"[^>]*>Finances/);
  app.run("Shell.selectSubTab('facilities')");
  assert.deepEqual(app.run('picked'), ['facilities']);
  assert.match(app.els['subtabs'].innerHTML, /subtab active"[^>]*>Facilities/);
  assert.deepEqual(active(app), ['tab-club']);
});

test('context header shows the entity and keeps back/forward history', () => {
  const app = ui();
  app.run("var opened = []; Shell.setContext({ section: 'player', title: 'Erling Haaland', subtitle: 'ST · Manchester City', open: () => opened.push('haaland') })");
  assert.equal(app.els['context-title'].textContent, 'Erling Haaland');
  assert.equal(app.els['context-subtitle'].textContent, 'ST · Manchester City');
  // Polling the same entity adds no history entry.
  app.run("Shell.setContext({ section: 'player', title: 'Erling Haaland', subtitle: 'ST · Manchester City' })");
  assert.equal(app.run('Shell.state.history.length'), 1);
  app.run("Shell.setContext({ section: 'player', title: 'Lamine Yamal', subtitle: 'RW · Barcelona', open: () => opened.push('yamal') })");
  assert.equal(app.els['context-back'].disabled, false);
  assert.equal(app.els['context-forward'].disabled, true);
  app.run('Shell.back()');
  assert.equal(app.els['context-title'].textContent, 'Erling Haaland');
  assert.deepEqual(app.run('opened'), ['haaland']);
  assert.equal(app.els['context-forward'].disabled, false);
  app.run('Shell.forward()');
  assert.equal(app.els['context-title'].textContent, 'Lamine Yamal');
  // Other sections keep their own header.
  app.run("Shell.setContext({ section: 'club', title: 'Barcelona', subtitle: 'La Liga' }); switchTab('tab-club')");
  assert.equal(app.els['context-title'].textContent, 'Barcelona');
  app.run("switchTab('tab-player')");
  assert.equal(app.els['context-title'].textContent, 'Lamine Yamal');
  app.run("switchTab('tab-settings')");
  assert.equal(app.els['context-title'].textContent, 'Settings');
});

test('the context poll fills the header without extra history', () => {
  const app = ui();
  app.run("state.activePlayer = { id: 5, name: 'Ole Romeny', position: 'ST (C)', club_name: 'Oxford United' }; Shell.syncEntities(state); Shell.syncEntities(state)");
  assert.equal(app.els['context-title'].textContent, 'Ole Romeny');
  assert.equal(app.els['context-subtitle'].textContent, 'ST (C) · Oxford United');
  assert.equal(app.run('Shell.state.history.length'), 1);
});

// ---- Global search ----
function searchApp({ staffOk = true } = {}) {
  return ui(async (url) => {
    if (url.startsWith('/api/search')) return ok({ players: [{ id: 7, name: 'Pep Player', position: 'ST', club_name: 'City', nationality: 'Spain' }], clubs: [], nations: [], competitions: [] });
    if (url === '/api/staff/search') return staffOk ? ok({ staff: [{ index: 79, uid: 1120, name: 'Pep <b>Guardiola</b>' }] }) : { ok: false, json: async () => ({ message: 'Attach first' }), text: async () => '' };
    return ok({});
  });
}

test('search queries players, clubs, nations, competitions and staff together', async () => {
  const app = searchApp();
  await app.run('performSearch("pep")');
  const urls = app.requests.map(r => r.url);
  assert.ok(urls.includes('/api/search?q=pep'));
  assert.deepEqual(app.requests.find(r => r.url === '/api/staff/search').body, { query: 'pep' });
  const html = app.els['search-results-dropdown'].innerHTML;
  assert.match(html, /selectSearchedPlayer\(7\)/);
  assert.match(html, /selectSearchedStaff\(79, 1120\)/);
  assert.match(html, /Pep &lt;b&gt;Guardiola/);
});

test('a staff search failure still shows the other results', async () => {
  const app = searchApp({ staffOk: false });
  await app.run('performSearch("pep")');
  const html = app.els['search-results-dropdown'].innerHTML;
  assert.match(html, /selectSearchedPlayer\(7\)/);
  assert.doesNotMatch(html, /selectSearchedStaff/);
});

test('one-letter queries skip the staff search', async () => {
  const app = searchApp();
  await app.run('performSearch("p")');
  assert.equal(app.requests.some(r => r.url === '/api/staff/search'), false);
});

test('selecting a staff result opens the Staff page and loads that member', async () => {
  const app = searchApp();
  app.run("var staffCalls = []; selectStaff = async (id, uid) => { staffCalls.push([id, uid]); staffState.selected = { staff: { index: id, uid, name: 'Pep Guardiola' } }; }");
  await app.run('selectSearchedStaff(79, 1120)');
  assert.deepEqual(active(app), ['tab-staff']);
  assert.deepEqual(app.run('staffCalls'), [[79, 1120]]);
  assert.equal(app.els['context-title'].textContent, 'Pep Guardiola');
  assert.equal(app.els['global-search'].value, '');
});

test('"/" focuses search unless typing in a field', () => {
  const app = ui();
  let prevented = false;
  app.fire('keydown', { key: '/', target: { tagName: 'DIV' }, preventDefault() { prevented = true; } });
  assert.equal(app.els['global-search'].focused, true);
  assert.equal(prevented, true);
  app.els['global-search'].focused = false;
  app.fire('keydown', { key: '/', target: { tagName: 'INPUT' }, preventDefault() { throw new Error('must not intercept typing'); } });
  assert.equal(app.els['global-search'].focused, false);
});

// ---- Save bar ----
function barApp(handlers = {}) {
  const app = ui();
  app.run(`
    var log = [];
    var edits = [];
    SaveBar.register('club', {
      load: async () => { log.push('load'); },
      save: async () => { log.push('save'); ${handlers.fail ? 'return false;' : ''} ${handlers.throws ? "throw new Error('Transaction rolled back');" : ''} edits.length = 0; },
      discard: async () => { log.push('discard'); edits.length = 0; },
      changes: () => edits.slice(),
    });
    switchTab('tab-club');
  `);
  return app;
}

test('the bar is bound to the active page and starts clean', () => {
  const app = ui();
  app.run("switchTab('tab-godmode')");
  assert.equal(app.els['savebar'].hidden, true, 'pages without Load/Save do not show the bar');
  app.run("SaveBar.register('club', { save: async () => true }); switchTab('tab-club')");
  assert.equal(app.els['savebar'].hidden, false);
  assert.equal(app.els['savebar-status'].textContent, 'No unsaved changes');
  assert.equal(app.els['savebar-save'].disabled, true);
  assert.equal(app.els['savebar-discard'].disabled, true);
  assert.equal(app.els['savebar-load'].hidden, true, 'no load handler registered');
  app.run("switchTab('tab-staff')");
  assert.equal(app.els['savebar'].hidden, false, 'Staff registers its own Load/Save adapter');
  assert.equal(app.els['savebar-load'].hidden, false);
  assert.equal(app.els['savebar-save'].disabled, true);
});

test('editing never sends a request; only Save does', async () => {
  const app = barApp();
  app.run("edits.push({ label: 'Training facilities', from: 18, to: 10 }); SaveBar.markDirty('club')");
  assert.equal(app.els['savebar-status'].textContent, '1 unsaved change');
  assert.equal(app.els['savebar-save'].disabled, false);
  assert.equal(app.requests.length, 0);
  assert.deepEqual(app.run('log'), []);
  assert.equal(await app.run('SaveBar.save()'), true);
  assert.deepEqual(app.run('log'), ['save']);
  assert.equal(app.els['savebar-status'].textContent, 'No unsaved changes');
});

test('Review lists old and new values', () => {
  const app = barApp();
  app.run("edits.push({ label: 'Training', from: 18, to: 10 }, { label: 'Balance', from: 100, to: 250 }); SaveBar.markDirty('club')");
  assert.equal(app.els['savebar-status'].textContent, '2 unsaved changes');
  assert.equal(app.els['savebar-review'].hidden, false);
  app.run('SaveBar.toggleReview()');
  const pop = app.els['savebar-popover'];
  assert.equal(pop.hidden, false);
  assert.match(pop.innerHTML, /Training[\s\S]*review-from">18[\s\S]*review-to">10/);
  assert.match(pop.innerHTML, /Balance[\s\S]*review-from">100[\s\S]*review-to">250/);
});

test('a failed save keeps the dirty state and shows the error', async () => {
  const thrown = barApp({ throws: true });
  thrown.run("edits.push({ label: 'x', from: 1, to: 2 }); SaveBar.markDirty('club')");
  assert.equal(await thrown.run('SaveBar.save()'), false);
  assert.equal(thrown.run("SaveBar.isDirty('club')"), true);
  assert.equal(thrown.els['savebar-save'].disabled, false);
  assert.match(thrown.els['savebar-error'].textContent, /Transaction rolled back/);
  assert.equal(thrown.run("messages.some(m => m.type === 'error' && /rolled back/.test(m.message))"), true);
  const refused = barApp({ fail: true });
  refused.run("edits.push({ label: 'x', from: 1, to: 2 }); SaveBar.markDirty('club')");
  assert.equal(await refused.run('SaveBar.save()'), false);
  assert.equal(refused.run("SaveBar.isDirty('club')"), true);
  assert.match(refused.els['savebar-error'].textContent, /changes are kept/);
  assert.equal(refused.run("messages.some(m => m.type === 'success')"), false);
});

test('a page that flagged dirty without a change list still counts and saves', async () => {
  const app = ui();
  app.run("var saved = 0; SaveBar.register('nation', { save: async () => { saved++; } }); switchTab('tab-nation'); SaveBar.markDirty('nation')");
  assert.equal(app.els['savebar-status'].textContent, '1 unsaved change');
  assert.equal(await app.run('SaveBar.save()'), true);
  assert.equal(app.run('saved'), 1);
  assert.equal(app.run("SaveBar.isDirty('nation')"), false);
});

test('Ctrl+S saves the active page, and does nothing when clean', async () => {
  const app = barApp();
  let prevented = 0;
  const press = () => app.fire('keydown', { key: 's', ctrlKey: true, target: {}, preventDefault() { prevented++; } });
  press();
  await new Promise(r => setImmediate(r));
  assert.deepEqual(app.run('log'), []);
  app.run("edits.push({ label: 'x' }); SaveBar.markDirty('club')");
  press();
  await new Promise(r => setImmediate(r));
  assert.deepEqual(app.run('log'), ['save']);
  assert.equal(prevented, 2, 'the browser save dialog is always suppressed');
});

test('Discard reverts through the page handler and clears the state', async () => {
  const app = barApp();
  app.run("edits.push({ label: 'x' }); SaveBar.markDirty('club')");
  assert.equal(await app.run('SaveBar.discard()'), true);
  assert.deepEqual(app.run('log'), ['discard']);
  assert.equal(app.run("SaveBar.isDirty('club')"), false);
  assert.equal(app.els['savebar-status'].textContent, 'No unsaved changes');
});

test('the single Load button calls the page load() and nothing else', async () => {
  const app = barApp();
  assert.equal(app.els['savebar-load'].hidden, false);
  assert.equal(await app.run('SaveBar.load()'), true);
  assert.deepEqual(app.run('log'), ['load']);
  assert.equal(app.requests.length, 0);
});

test('edits made while a save is running stay dirty', async () => {
  const app = ui();
  app.run("var release; SaveBar.register('club', { save: () => new Promise(r => { release = r; }) }); switchTab('tab-club'); SaveBar.markDirty('club')");
  const pending = app.run("SaveBar.save()");
  app.run("SaveBar.markDirty('club'); release(true)");
  await pending;
  assert.equal(app.run("SaveBar.isDirty('club')"), true);
});

test('legacy entity flags drive the bar until a page registers itself', () => {
  const app = ui();
  app.run('SaveBar.init()');
  app.run("state.activeClub = { club_id: 1, club_name: 'X' }; state.clubDirty = true; switchTab('tab-club')");
  assert.equal(app.els['savebar-status'].textContent, '1 unsaved change');
});

test('no autosave controls or settings remain in the markup or core scripts', () => {
  const { html, manifest, read } = require('./helpers/ui.cjs');
  const core = manifest.scripts.filter(file => file.startsWith('js/core/')).map(read).join('');
  assert.doesNotMatch(core, /autoApply|auto-apply|localStorage\.\w+\(['"]auto/i);
  assert.doesNotMatch(html.split('<script>')[0], /Autosave|Auto-?[Ss]ave\b|Auto-?[Aa]pply|autosave-|toggle-autosave/);
});

test('valueBand maps FM 1-20 values onto the four colour bands', () => {
  const app = ui();
  const band = v => app.run(`valueBand(${v})`);
  assert.deepEqual([1, 5, 6, 10, 11, 15, 16, 20].map(band), ['band-1-5', 'band-1-5', 'band-6-10', 'band-6-10', 'band-11-15', 'band-11-15', 'band-16-20', 'band-16-20']);
  assert.deepEqual(['0', '21', 'null', 'undefined', '""', 'NaN'].map(band), Array(6).fill('band-none'));
  assert.equal(app.run('valueBand(190, 200)'), 'band-16-20');
  assert.equal(app.run('valueBand(80, 200)'), 'band-6-10');
});

test('Settings page registers one Load/Save pair and tracks edits as pending changes', async () => {
  const app = ui(async url => url === '/api/settings' ? ok({ settings: { currency: 'GBP', measurement: 'metric', wage_period: 'weekly' }, currencies: [{ code: 'GBP', symbol: '£', decimals: 0, rate_per_gbp: 1 }], rates_date: '2026' }) : ok({}));
  app.els['settings-currency'] = app.element('settings-currency');
  app.els['settings-currency'].value = 'GBP';
  app.run("switchTab('tab-settings')");
  assert.equal(app.els['savebar'].hidden, false);
  assert.equal(await app.run('SaveBar.load()'), true);
  assert.deepEqual(app.run('settingsChanges()'), []);
  app.els['settings-currency'].value = 'USD';
  assert.deepEqual(app.run('settingsChanges()'), [{ label: 'Currency', from: 'GBP', to: 'USD' }]);
  assert.equal(app.requests.filter(r => r.body && r.url === '/api/settings').length, 0, 'editing sends nothing');
});
