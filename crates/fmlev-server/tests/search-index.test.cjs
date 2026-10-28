// Search index UI: status chip, rebuild, paged results, staff source, unknown badges and recent selections.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { createUi, element } = require('./helpers/fake-dom.cjs');

const IDS = ['global-search', 'search-results-dropdown', 'search-index-chip', 'search-clear-btn', 'search-filters', 'rebuild-index-btn', 'tab-staff'];
const ok = (body, status = 200) => ({ ok: status >= 200 && status < 300, status, json: async () => body, text: async () => '' });

function searchUi(route, { storage } = {}) {
  const timers = [];
  const extra = { setTimeout: (fn, ms) => { timers.push(ms); return timers.length; } };
  if (storage) extra.localStorage = storage;
  const app = createUi({ ids: IDS, extra, fetch: async (url, init) => route(url, init) || ok({}) });
  app.timers = timers;
  // Rows parsed from the rendered markup so keyboard navigation and Enter work like the browser; cached per render.
  const dropdown = app.els['search-results-dropdown'];
  let cache = { html: null, items: [] };
  dropdown.querySelectorAll = () => {
    if (cache.html === dropdown.innerHTML) return cache.items;
    const items = [];
    for (const [, attrs] of dropdown.innerHTML.matchAll(/<div class="search-item(?: [^"]*)?"([^>]*)>/g)) {
      const item = element();
      const attr = name => (new RegExp(`${name}="([^"]*)"`).exec(attrs) || [])[1];
      item.dataset = { kind: attr('data-kind'), pos: attr('data-pos'), more: attr('data-more') };
      const onclick = attr('onclick');
      item.click = () => { app.clicked = app.run(onclick); };
      items.push(item);
    }
    cache = { html: dropdown.innerHTML, items };
    return items;
  };
  app.key = key => app.run(`handleSearchKey({ key: ${JSON.stringify(key)}, preventDefault() {} })`);
  app.html = () => dropdown.innerHTML;
  return app;
}

const players = (from, count) => Array.from({ length: count }, (_, i) => ({ id: from + i, name: `Player ${from + i}`, club_name: 'City' }));
const totals = (over = {}) => ({ players: 0, staff: 0, clubs: 0, nations: 0, competitions: 0, ...over });

// ---- Status chip ----
test('index chip shows building, ready, error and idle states and polls at 2 s / 30 s', async () => {
  let status;
  const app = searchUi(url => (url === '/api/search/status' ? ok(status) : null));
  const chip = app.els['search-index-chip'];

  status = { state: 'building', stage: 'players', counts: totals({ players: 41200 }), built_at: null, duration_ms: null, error: null, pid: 9, duplicates_dropped: 0 };
  await app.run('refreshSearchIndexStatus()');
  assert.equal(chip.hidden, false);
  assert.equal(chip.textContent, 'Indexing… players');
  assert.equal(chip.dataset.state, 'building');
  assert.match(chip.title, /41,200 players/);
  assert.equal(app.timers.at(-1), 2000);

  status = { state: 'ready', stage: null, counts: totals({ players: 1204, staff: 30 }), built_at: Math.floor(Date.now() / 1000) - 120, duration_ms: 5300, error: null, pid: 9, duplicates_dropped: 3 };
  await app.run('refreshSearchIndexStatus()');
  assert.equal(chip.textContent, 'Index ready · 1,204 players · 2 min ago');
  assert.equal(chip.dataset.state, 'ready');
  assert.match(chip.title, /built in 5\.3 s; 3 duplicates dropped/);
  assert.equal(app.timers.at(-1), 30000);

  status = { state: 'error', stage: null, counts: totals(), built_at: null, duration_ms: null, error: 'RTTI vtable not found', pid: 9, duplicates_dropped: 0 };
  await app.run('refreshSearchIndexStatus()');
  assert.equal(chip.textContent, 'Index error');
  assert.equal(chip.title, 'RTTI vtable not found');

  status = { state: 'idle', stage: null, counts: totals(), built_at: null, duration_ms: null, error: null, pid: null, duplicates_dropped: 0 };
  await app.run('refreshSearchIndexStatus()');
  assert.equal(chip.textContent, 'Not indexed');
  assert.equal(chip.dataset.state, 'idle');
});

test('status polling failures are silent; a server without the route stops polling', async () => {
  let mode = 'throw';
  const app = searchUi(url => {
    if (url !== '/api/search/status') return null;
    if (mode === 'throw') throw new Error('connection refused');
    return { ok: false, status: 404, json: async () => ({}), text: async () => '' };
  });
  await app.run('refreshSearchIndexStatus()');
  assert.deepEqual(app.run('messages'), []);
  assert.equal(app.timers.length, 1, 'a network failure retries later');
  mode = '404';
  await app.run('refreshSearchIndexStatus()');
  assert.equal(app.timers.length, 1, 'no further poll is scheduled');
  assert.equal(app.els['search-index-chip'].hidden, true);
  assert.deepEqual(app.run('messages'), []);
});

test('Rebuild index posts to /api/search/rebuild and toasts the server message on 409 and 400', async () => {
  let reply = ok({ status: 'started' }, 202);
  const app = searchUi(url => (url === '/api/search/rebuild' ? reply : null));
  await app.run('rebuildSearchIndex()');
  assert.equal(app.requests.at(-1).url, '/api/search/rebuild');
  assert.equal(app.els['search-index-chip'].textContent, 'Indexing…');
  assert.equal(app.timers.at(-1), 2000);

  reply = ok({ status: 'error', message: 'A rebuild is already running' }, 409);
  await app.run('rebuildSearchIndex()');
  assert.deepEqual(app.run('messages.at(-1)'), { message: 'A rebuild is already running', type: 'info' });

  reply = ok({ status: 'error', message: 'Not attached to FM26' }, 400);
  await app.run('rebuildSearchIndex()');
  assert.deepEqual(app.run('messages.at(-1)'), { message: 'Not attached to FM26', type: 'error' });
  assert.equal(app.els['rebuild-index-btn'].disabled, false);
  assert.equal(app.run('typeof discoverLoadedPlayers'), 'function');
});

// ---- Grouped, paged results ----
test('sections show totals and Show more requests the next page of that kind', async () => {
  const app = searchUi(url => {
    if (url === '/api/search?q=ma') return ok({ players: players(0, 25), staff: [], clubs: [], nations: [], competitions: [], totals: totals({ players: 1204 }), source: 'index' });
    if (url === '/api/search?q=ma&kind=players&limit=25&offset=25') return ok({ players: players(25, 25), totals: totals({ players: 1204 }), source: 'index' });
    return null;
  });
  await app.run('performSearch("ma")');
  assert.match(app.html(), /Players \(25 of 1,204\)/);
  assert.match(app.html(), /onclick="loadMoreSearchResults\('players'\)">Show more players \(1,179 more\)/);
  await app.run("loadMoreSearchResults('players')");
  assert.equal(app.requests.at(-1).url, '/api/search?q=ma&kind=players&limit=25&offset=25');
  assert.match(app.html(), /Players \(50 of 1,204\)/);
  assert.match(app.html(), /selectSearchedPlayer\(49\)/);
  assert.equal((app.html().match(/selectSearchedPlayer\(/g) || []).length, 50);
});

test('filter chips pass kind to the server and Show more works from the keyboard', async () => {
  const app = searchUi(url => {
    if (url === '/api/search?q=ma&kind=players') return ok({ players: players(0, 2), totals: totals({ players: 3 }), source: 'index' });
    if (url === '/api/search?q=ma&kind=players&limit=25&offset=2') return ok({ players: players(2, 1), totals: totals({ players: 3 }), source: 'index' });
    return null;
  });
  app.els['global-search'].value = 'ma';
  await app.run('setSearchFilter("players")');
  assert.equal(app.requests.at(-1).url, '/api/search?q=ma&kind=players');
  app.key('ArrowUp'); // wraps to the last row: Show more
  app.key('Enter');
  await app.clicked;
  assert.equal(app.requests.at(-1).url, '/api/search?q=ma&kind=players&limit=25&offset=2');
  assert.doesNotMatch(app.html(), /Show more/);
  const items = app.els['search-results-dropdown'].querySelectorAll('.search-item');
  const highlighted = items.filter(item => item.classList.contains('highlighted'));
  assert.deepEqual(highlighted.map(item => item.dataset.pos), ['2'], 'focus lands on the first new row');
  app.key('ArrowUp');
  assert.equal(items.findIndex(item => item.classList.contains('highlighted')), 1);
});

// ---- Staff source ----
test('staff come from the index when /api/search returns them; no native rescan', async () => {
  const app = searchUi(url => (url.startsWith('/api/search?') ? ok({ players: [], staff: [{ index: 79, uid: 1120, name: 'Pep <b>G</b>' }], clubs: [], nations: [], competitions: [], totals: totals({ staff: 1 }), source: 'index' }) : null));
  await app.run('performSearch("pep")');
  assert.equal(app.requests.some(r => r.url === '/api/staff/search'), false);
  assert.match(app.html(), /Staff \(1\)/);
  assert.match(app.html(), /selectSearchedStaff\(79, 1120\)/);
  assert.match(app.html(), /Pep &lt;b&gt;G/);
});

test('without data.staff the UI falls back to POST /api/staff/search', async () => {
  const app = searchUi(url => {
    if (url.startsWith('/api/search?')) return ok({ players: [], clubs: [], nations: [], competitions: [] });
    if (url === '/api/staff/search') return ok({ staff: [{ index: 5, uid: 6, name: 'Fallback Coach' }] });
    return null;
  });
  await app.run('performSearch("coach")');
  assert.deepEqual(app.requests.find(r => r.url === '/api/staff/search').body, { query: 'coach' });
  assert.match(app.html(), /selectSearchedStaff\(5, 6\)/);
  assert.doesNotMatch(app.html(), /Show more/, 'the fallback scan cannot page');
});

// ---- Unknown values ----
test('index hits hide unknown reputation, youth, codes and subs instead of showing zeros', async () => {
  const app = searchUi(url => (url.startsWith('/api/search?') ? ok({
    players: [{ id: 1, name: 'Index Hit', club_name: 'Real Club', nationality: '', position: '', current_ability: 0, potential_ability: 0 }],
    staff: [],
    clubs: [{ club_id: 2, club_name: 'Quiet FC', league_name: '', reputation: 0 }],
    nations: [{ id: 3, name: 'Nowhere', three_letter_code: '', continent: '', reputation: 0, youth_rating: null }],
    competitions: [{ id: 4, name: 'Cup', short_name: '', nation_name: '', reputation: 0, max_substitutes: null, max_bench_size: 0 }],
    totals: totals({ players: 1, clubs: 1, nations: 1, competitions: 1 }), source: 'index',
  }) : null));
  await app.run('performSearch("x")');
  const html = app.html();
  assert.match(html, /CA — \/ PA —/);
  assert.doesNotMatch(html, /CA 0|PA 0/);
  assert.doesNotMatch(html, /Unknown \(live\)/);
  assert.match(html, /Real Club/);
  assert.doesNotMatch(html, /Rep:|Unavailable|Youth|Subs|International|\(\)<\/span>/);
});

// ---- Empty results ----
test('empty results explain indexing state and never point at the Attributes screen when ready', async () => {
  let index;
  const app = searchUi(url => (url.startsWith('/api/search?') ? ok({ players: [], staff: [], clubs: [], nations: [], competitions: [], totals: totals(), index, discovery_error: 'Open a player Attributes screen first' }) : null));
  index = { state: 'building', stage: 'clubs', counts: totals() };
  await app.run('performSearch("zz")');
  assert.match(app.html(), /still building \(clubs\)/);
  index = { state: 'idle', counts: totals() };
  await app.run('performSearch("zz")');
  assert.match(app.html(), /onclick="rebuildSearchIndex\(\)">Rebuild index/);
  index = { state: 'error', error: 'scan failed', counts: totals() };
  await app.run('performSearch("zz")');
  assert.match(app.html(), /Indexing failed: scan failed/);
  index = { state: 'ready', counts: totals({ players: 9 }) };
  await app.run('performSearch("zz")');
  assert.doesNotMatch(app.html(), /Attributes|Rebuild/);
  assert.match(app.html(), /No players, staff, clubs, nations or competitions match "zz"/);
});

// ---- Recent selections ----
test('recent selections work in memory when localStorage throws', async () => {
  const thrower = { getItem() { throw new Error('denied'); }, setItem() { throw new Error('denied'); } };
  const app = searchUi(() => null, { storage: thrower });
  for (let i = 1; i <= 10; i++) app.run(`rememberSearchSelection({ kind: 'player', id: ${i}, name: 'P<${i}>' })`);
  app.run("rememberSearchSelection({ kind: 'staff', id: 79, uid: 1120, name: 'Pep' })");
  app.run("rememberSearchSelection({ kind: 'player', id: 10, name: 'P<10>' })"); // re-opened: moves to the top, no duplicate
  assert.equal(app.run('showRecentSelections()'), true);
  const html = app.html();
  assert.equal((html.match(/openRecentSelection\(/g) || []).length, 8);
  assert.match(html, /P&lt;10&gt;[\s\S]*Pep[\s\S]*P&lt;9&gt;/);
  assert.doesNotMatch(html, /P&lt;2&gt;/);

  app.run("var opened = []; selectSearchedStaff = (id, uid) => opened.push([id, uid]); selectSearchedPlayer = id => opened.push([id])");
  app.key('ArrowDown');
  app.key('ArrowDown');
  app.key('Enter');
  assert.deepEqual(app.run('opened'), [[79, 1120]]);
});

test('recent selections persist to storage, show only for an empty box and survive corrupt data', () => {
  const store = {};
  const storage = { getItem: key => store[key] ?? null, setItem: (key, value) => { store[key] = value; } };
  const first = searchUi(() => null, { storage });
  first.run("rememberSearchSelection({ kind: 'club', id: 4, name: 'Club Four' })");
  assert.deepEqual(JSON.parse(store['fmle.search.recent']), [{ kind: 'club', id: 4, uid: null, name: 'Club Four' }]);

  const second = searchUi(() => null, { storage });
  second.els['global-search'].value = 'abc';
  assert.equal(second.run('showRecentSelections()'), false);
  second.els['global-search'].value = '';
  assert.equal(second.run('showRecentSelections()'), true);
  assert.match(second.html(), /Club Four/);

  store['fmle.search.recent'] = '{not json';
  const third = searchUi(() => null, { storage });
  assert.equal(third.run('showRecentSelections()'), false);
  assert.equal(third.els['search-results-dropdown'].style.display, 'none');
});

test('opening a search result records it as a recent selection', async () => {
  const app = searchUi(() => null);
  app.run("selectStaff = async (id, uid) => { staffState.selected = { staff: { index: id, uid, name: 'Pep Guardiola' } }; }; Shell.setContext = () => {}; switchTab = () => {}");
  await app.run('selectSearchedStaff(79, 1120)');
  assert.deepEqual(app.run('readRecentSelections()'), [{ kind: 'staff', id: 79, uid: 1120, name: 'Pep Guardiola' }]);
});
