// Scout page: filters become /api/scout parameters, names open the Player or Staff tab, no writes.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { createUi } = require('./helpers/fake-dom.cjs');
const { read, manifest } = require('./helpers/ui.cjs');

const IDS = ['scout-kind', 'scout-q', 'scout-club', 'scout-nation', 'scout-generated', 'scout-position', 'scout-pos-min', 'scout-ca-min', 'scout-ca-max', 'scout-pa-min', 'scout-pa-max',
  'scout-sort', 'scout-summary', 'scout-results', 'scout-prev', 'scout-next', 'tab-scout'];
const ok = body => ({ ok: true, status: 200, json: async () => body, text: async () => '' });

function scoutUi(route) {
  const app = createUi({ ids: IDS, extra: { URLSearchParams }, fetch: async (url, init) => route(url, init) });
  app.els['scout-kind'].value = 'players';
  app.els['scout-pos-min'].value = '15';
  app.els['scout-sort'].value = 'name';
  return app;
}

test('scout page is in the manifest and sidebar', () => {
  assert.ok(manifest.pages.includes('pages/scout.html'));
  assert.ok(manifest.scripts.includes('js/pages/scout.js'));
  assert.match(read('pages/scout.html'), /id="tab-scout"/);
  assert.match(read('js/core/shell.js'), /id: "scout", label: "Scout"/);
});

test('player filters become query parameters and results link to the Player tab', async () => {
  const urls = [];
  const app = scoutUi(url => {
    urls.push(url);
    return ok({ kind: 'players', total: 1, offset: 0, limit: 50, items: [{ id: 7, uid: 70, name: 'A <b>Striker', club_name: 'Getafe', nation: 'Spain', generated: true, ca: 130, pa: null, positions: [{ position: 'st', rating: 20 }] }] });
  });
  Object.assign(app.els['scout-q'], { value: 'strik' });
  app.els['scout-position'].value = 'st';
  app.els['scout-ca-min'].value = '120';
  app.els['scout-nation'].value = 'spain';
  app.els['scout-generated'].value = 'false';
  app.els['scout-sort'].value = 'ca';
  await app.run('runScout()');
  const query = new URL(urls[0], 'http://x').searchParams;
  assert.equal(query.get('kind'), 'players');
  assert.equal(query.get('q'), 'strik');
  assert.equal(query.get('position'), 'st');
  assert.equal(query.get('pos_min'), '15');
  assert.equal(query.get('ca_min'), '120');
  assert.equal(query.get('nation'), 'spain');
  assert.equal(query.get('generated'), 'false');
  assert.equal(query.get('sort'), 'ca');
  assert.equal(query.has('pa_min'), false);
  const html = app.els['scout-results'].innerHTML;
  assert.match(html, /openScoutPlayer\(7\)/);
  assert.doesNotMatch(html, /<b>/, 'names are escaped');
  assert.match(html, /ST 20/);
  assert.match(html, /Spain/);
  assert.match(html, /scout-tag/);
  assert.equal(app.els['scout-summary'].textContent, '1-1 of 1');
});

test('staff search sends only name and club and opens the Staff tab', async () => {
  const urls = [];
  const app = scoutUi(url => {
    urls.push(url);
    return ok({ kind: 'staff', total: 1, offset: 0, limit: 50, items: [{ index: 5, uid: 99, name: 'Coach', club_name: null }] });
  });
  app.els['scout-kind'].value = 'staff';
  app.els['scout-ca-min'].value = '100';
  app.els['scout-nation'].value = 'spain';
  app.els['scout-generated'].value = 'true';
  app.els['scout-club'].value = 'madrid';
  await app.run('runScout()');
  const query = new URL(urls[0], 'http://x').searchParams;
  assert.equal(query.get('kind'), 'staff');
  assert.equal(query.get('club'), 'madrid');
  assert.equal(query.has('ca_min'), false);
  assert.equal(query.has('nation') || query.has('generated'), false);
  assert.match(app.els['scout-results'].innerHTML, /openScoutStaff\(5, 99\)/);
});

test('a failed request shows the server message', async () => {
  const app = scoutUi(() => ({ ok: false, status: 409, json: async () => ({ message: 'The search index is not ready' }), text: async () => '' }));
  await app.run('runScout()');
  assert.match(app.els['scout-results'].innerHTML, /not ready/);
});

test('selecting a player updates the header title even while pinned', async () => {
  const app = createUi({
    ids: ['context-title', 'context-subtitle'],
    fetch: async () => ok({ player: { id: 9, name: 'New Player', position: 'ST (C)', club_name: 'Persib', attributes: {}, positions: {}, contract: {} } }),
  });
  app.run('Shell.setContext({ section: "player", kind: "player", id: 1, title: "Old Player" }); Shell.state.section = "player";');
  app.run('displayPlayer = () => { state.activePlayer = { id: 9, name: "New Player", position: "ST (C)", club_name: "Persib" }; }; canReplaceDraft = () => true; clearSearch = () => {}; switchTab = () => {}; updateSyncUI = () => {}; rememberSearchSelection = () => {};');
  await app.run('selectSearchedPlayer(9)');
  assert.equal(app.run('Shell.state.contexts.player.title'), 'New Player');
});
