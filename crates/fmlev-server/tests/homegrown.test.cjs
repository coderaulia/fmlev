const { test } = require('node:test');
const assert = require('node:assert/strict');
const { createUi } = require('./helpers/fake-dom.cjs');
const { read } = require('./helpers/ui.cjs');
const ok = data => ({ ok: true, json: async () => data });
const fail = (status, message) => ({ ok: false, status, json: async () => ({ message }) });

const IDS = ['homegrown-clubs', 'homegrown-nations', 'homegrown-note', 'homegrown-experimental', 'btn-homegrown-club', 'btn-homegrown-nation'];
const READ = {
  editing_available: true,
  clubs: [{ id: 369, name: 'Manchester City' }], nations: [{ id: 139, name: 'England' }], unresolved_targets: 0, spare_slots: 0,
  current_club: { id: 369, name: 'Manchester City' }, first_nationality: { id: 139, name: 'England' },
};

function app(fetch, { confirmAnswer = true } = {}) {
  const ui = createUi({ fetch, ids: IDS, tabs: ['tab-contract'], extra: { confirm: () => confirmAnswer } });
  ui.run("state.connected = true; state.activePlayer = { id: 10173, name: 'Phil Foden' };");
  return ui;
}

test('the card and script are registered in the page and manifest', () => {
  assert.match(read('pages/contract.html'), /Trained at Club \(HGC\)/);
  assert.match(read('pages/contract.html'), /Trained at Nation \(HGN\)/);
  assert.match(read('pages/contract.html'), /allocates memory in the game; throwaway save only/);
  assert.ok(JSON.parse(read('manifest.json')).scripts.includes('js/pages/homegrown.js'));
});

test('refresh shows the recorded clubs and nations; buttons stay off until the checkbox is ticked', async () => {
  const ui = app(async () => ok(READ));
  await ui.run('refreshHomegrown()');
  assert.deepEqual(ui.requests.map(r => [r.url, r.body]), [['/api/player/homegrown', { player_id: 10173 }]]);
  assert.match(ui.els['homegrown-clubs'].innerText, /Manchester City \(#369\)/);
  assert.match(ui.els['homegrown-nations'].innerText, /England/);
  assert.equal(ui.els['btn-homegrown-club'].disabled, true);
  assert.match(ui.els['btn-homegrown-club'].title, /experimental checkbox/);
  ui.els['homegrown-experimental'].checked = true;
  ui.run('updateHomegrownButtons()');
  assert.equal(ui.els['btn-homegrown-club'].disabled, false);
  assert.equal(ui.els['btn-homegrown-nation'].disabled, false);
});

test('a failed read shows Unavailable and keeps the buttons disabled, with one attempt per player', async () => {
  const ui = app(async () => fail(400, 'Player has no relationship block'));
  ui.els['homegrown-experimental'].checked = true;
  await ui.run('refreshHomegrown()');
  assert.equal(ui.els['homegrown-clubs'].innerText, 'Unavailable');
  assert.equal(ui.els['btn-homegrown-club'].disabled, true);
  assert.match(ui.els['btn-homegrown-club'].title, /unverified in FM26/);
  assert.equal(ui.run('homegrownState.loadedFor'), 10173);
});

test('HGC and HGN send the explicit current flags with experimental: true after confirmation', async () => {
  const ui = app(async url => (url === '/api/player/homegrown/set' ? ok({ ...READ, message: 'written' }) : ok(READ)));
  await ui.run('refreshHomegrown()');
  ui.els['homegrown-experimental'].checked = true;
  assert.equal(await ui.run('trainedAtClub()'), true);
  const call = ui.requests.find(r => r.url === '/api/player/homegrown/set');
  assert.deepEqual(call.body, { player_id: 10173, experimental: true, club_current: true });
  assert.equal(await ui.run('trainedAtNation()'), true);
  assert.deepEqual(ui.requests.filter(r => r.url === '/api/player/homegrown/set').at(-1).body, { player_id: 10173, experimental: true, nation_current: true });
});

test('declining, no checkbox, unresolved target or detached game send nothing', async () => {
  const declined = app(async () => ok(READ), { confirmAnswer: false });
  await declined.run('refreshHomegrown()');
  declined.els['homegrown-experimental'].checked = true;
  assert.equal(await declined.run('trainedAtClub()'), false);
  const unticked = app(async () => ok(READ));
  await unticked.run('refreshHomegrown()');
  assert.equal(await unticked.run('trainedAtClub()'), false);
  assert.match(unticked.run('messages.at(-1).message'), /experimental checkbox/);
  const unresolved = app(async () => ok({ ...READ, current_club: null }));
  await unresolved.run('refreshHomegrown()');
  unresolved.els['homegrown-experimental'].checked = true;
  assert.equal(await unresolved.run('trainedAtClub()'), false);
  const detached = app(async () => ok(READ));
  detached.run('state.connected = false');
  assert.equal(await detached.run('trainedAtClub()'), false);
  for (const ui of [declined, unticked, unresolved, detached]) assert.ok(!ui.requests.some(r => r.url.endsWith('/set')));
});

test('a server error is reported and leaves the card consistent', async () => {
  const ui = app(async url => (url.endsWith('/set') ? fail(400, 'Allocation failed') : ok(READ)));
  await ui.run('refreshHomegrown()');
  ui.els['homegrown-experimental'].checked = true;
  assert.equal(await ui.run('trainedAtNation()'), false);
  assert.match(ui.run('messages.at(-1).message'), /Allocation failed/);
  assert.equal(ui.run('homegrownState.busy'), false);
});

test('an unverified server keeps homegrown edits unavailable even with the checkbox ticked', async () => {
  const ui = app(async () => ok({ ...READ, editing_available: false }));
  await ui.run('refreshHomegrown()');
  assert.equal(ui.els['homegrown-experimental'].disabled, true);
  ui.els['homegrown-experimental'].checked = true;
  assert.equal(ui.els['btn-homegrown-club'].disabled, true);
  assert.equal(ui.els['btn-homegrown-nation'].disabled, true);
  assert.equal(await ui.run('trainedAtClub()'), false);
  assert.equal(ui.requests.length, 1);
});
