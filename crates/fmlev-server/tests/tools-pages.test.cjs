const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const path = require('node:path');
const { createUi } = require('./helpers/fake-dom.cjs');

const pages = path.join(__dirname, '..', 'src', 'ui', 'pages');
const read = name => readFileSync(path.join(pages, name), 'utf8');
const TABS = ['tab-godmode', 'tab-settings', 'tab-status'];

test('tools pages have no autosave, no page-level save/load buttons and no non-English text', () => {
  for (const name of ['godmode.html', 'settings.html', 'status.html']) {
    const text = read(name);
    assert.doesNotMatch(text, /autosave|auto-save|saveDisplaySettings\(\)/i, name);
    assert.doesNotMatch(text, /Load from game|>\s*Save\s*</, name);
    assert.doesNotMatch(text, /[^\x00-\x7F]/, `${name} must be plain ASCII English`);
  }
  assert.match(read('settings.html'), /onchange="markSettingsDirty\(\)"/);
});

test('every cheat has a badge and exactly one action button', () => {
  const rows = read('godmode.html').split('class="cheat-row"').slice(1);
  assert.ok(rows.length >= 16);
  for (const row of rows) {
    const body = row.split('cheat-row')[0];
    assert.match(body, /pill-accent">Working<|pill-warn">In development</);
    assert.equal((body.match(/<button/g) || []).length, 1);
  }
  for (const id of ['god-player-target', 'god-squad-target', 'god-club-target', 'god-nation-target']) {
    assert.ok(read('godmode.html').includes(`id="${id}"`));
  }
});

test('settings edits only mark dirty, Save posts once, colour choice is per viewer', async () => {
  const ui = createUi({
    ids: ['settings-currency', 'settings-measurement', 'settings-wage-period', 'settings-band-scheme', 'settings-rate', 'settings-preview'],
    tabs: TABS,
    fetch: async () => ({ ok: true, json: async () => ({ settings: { currency: 'USD', measurement: 'metric', wage_period: 'weekly' },
      currencies: [{ code: 'GBP', symbol: '£', decimals: 0, rate_per_gbp: 1 }, { code: 'USD', symbol: '$', decimals: 0, rate_per_gbp: 1.35 }], rates_date: '2026' }) }),
  });
  ui.run('displaySettings = { currency: "GBP", measurement: "metric", wage_period: "weekly" }; savedBandScheme = "fm26";');
  ui.els['settings-currency'].value = 'USD'; ui.els['settings-measurement'].value = 'metric'; ui.els['settings-wage-period'].value = 'weekly';
  ui.els['settings-band-scheme'].value = 'legacy';
  ui.run('markSettingsDirty()');
  assert.equal(ui.requests.length, 0, 'editing must not send requests');
  assert.equal(ui.document.body.dataset.bandScheme, 'legacy', 'colours preview immediately');
  const changed = ui.run('settingsChanges()').map(c => c.label);
  assert.deepEqual(changed, ['Currency', 'Value colours']);
  assert.equal(await ui.run('saveDisplaySettings()'), true);
  const posts = ui.requests.filter(r => r.body);
  assert.equal(posts.length, 1);
  assert.deepEqual(posts[0].body, { currency: 'USD', measurement: 'metric', wage_period: 'weekly' });
  assert.equal(ui.run('savedBandScheme'), 'legacy');
});

test('diagnostics register a Load only and render status and modules', async () => {
  const ui = createUi({
    ids: ['diag-exe', 'diag-proton', 'diag-prefix', 'diag-yama', 'diag-bases', 'diag-maps', 'diag-maps-count'],
    tabs: TABS,
    fetch: async url => url === '/api/status'
      ? { ok: true, json: async () => ({ attached: true, pid: 42, exe_path: '/x/fm.exe', is_proton: true, wine_prefix: '/p', game_assembly_base: '0x1000', unity_player_base: null, fm_exe_base: null, yama_diagnostics: 'ok' }) }
      : { ok: true, json: async () => ({ total_mappings: 9, modules: [{ name: 'a.dll', path: '/a.dll', start: '0x1000', end: '0x3000', perms: 'r-xp' }] }) },
  });
  assert.equal(ui.run('typeof loadDiagnostics'), 'function');
  assert.equal(await ui.run('loadDiagnostics()'), true);
  assert.match(ui.els['diag-bases'].innerHTML, /0x1000/);
  assert.match(ui.els['diag-bases'].innerHTML, /unavailable/);
  assert.match(ui.els['diag-maps'].innerHTML, /a\.dll/);
  assert.match(ui.els['diag-maps'].innerHTML, /8 KB/);
  assert.equal(ui.els['diag-proton'].innerText, 'Steam Proton / Wine');
  assert.equal(ui.requests.filter(r => r.body).length, 0);
});
