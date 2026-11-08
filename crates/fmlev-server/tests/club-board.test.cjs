const { test } = require('node:test');
const assert = require('node:assert/strict');
const { createUi } = require('./helpers/fake-dom.cjs');
const { read } = require('./helpers/ui.cjs');

const objective = (over = {}) => ({
  index: 0, class: 'club_vision', type_code: 150, type_label: 'Play attacking football', label_status: 'catalog_unverified',
  target_id: null, target_kind_code: null, value: 21, importance: 4, status_code: 1,
  start: { iso: '2025-07-14' }, deadline: { year: 2026, day: 118, iso: '2026-04-28' }, notes: [], ...over,
});
const board = list => ({ status: 'ok', board: { club_id: 7, chain: { human_manager_details: true }, objectives: list } });
const reply = (map) => async (url) => {
  const hit = map[url];
  if (!hit) return { ok: false, json: async () => ({ message: 'no route' }) };
  return { ok: hit.ok !== false, json: async () => hit.body };
};

function app(map = {}) {
  const ui = createUi({
    fetch: reply(map),
    ids: ['board-table-body', 'board-note', 'board-experimental', 'board-status-current', 'board-status-select', 'board-status-apply', 'savebar', 'savebar-status', 'savebar-error'],
    tabs: ['tab-club'],
  });
  ui.run("state.connected = true; state.activeClub = { club_id: 7, club_name: 'Club' };");
  return ui;
}

test('the card is a direct, live-editor child of the club page and is labelled experimental', () => {
  const html = read('pages/club.html');
  assert.match(html, /id="board-card"[^>]*data-live-editor="true"/);
  assert.match(html, /Objective importance and club status appeared in FM26/);
  assert.match(html, /<input id="board-experimental" type="checkbox"/);
  assert.match(html, /id="board-status-apply" class="btn btn-sm" disabled/);
});

test('reading renders one row per objective and sends only a club id', async () => {
  const ui = app({ '/api/club/board': { body: board([objective(), objective({ index: 1, value: null, deadline: null })]) } });
  assert.equal(await ui.run('loadClubBoard()'), true);
  assert.deepEqual(ui.requests.map(r => [r.url, r.body]), [['/api/club/board', { club_id: 7 }]]);
  assert.equal(ui.els['board-table-body'].children.length, 2);
  assert.equal(ui.run('boardRows.length'), 2);
  assert.equal(ui.run('boardRows[1].value.disabled'), true, 'no value to edit');
  assert.equal(ui.run('boardRows[1].deadline.disabled'), true, 'no deadline can be added');
  assert.match(ui.els['board-note'].innerText, /unverified/);
});

test('an unreadable board is shown as unavailable with the reason', async () => {
  const ui = app({ '/api/club/board': { ok: false, body: { message: 'This club has no board objective data' } } });
  assert.equal(await ui.run('loadClubBoard()'), false);
  assert.match(ui.els['board-note'].innerText, /^Unavailable: This club has no board/);
  assert.equal(ui.run('boardRows.length'), 0);
});

test('edit buttons stay disabled until the experimental box is ticked', async () => {
  const ui = app({ '/api/club/board': { body: board([objective()]) } });
  await ui.run('loadClubBoard()');
  assert.equal(ui.run('boardRows[0].button.disabled'), true);
  ui.els['board-experimental'].checked = true;
  ui.run('updateBoardButtons()');
  assert.equal(ui.run('boardRows[0].button.disabled'), false);
  assert.equal(await ui.run('applyBoardObjective(0)'), false, 'nothing changed, nothing sent');
  assert.equal(ui.requests.length, 1);
});

test('apply without the experimental box sends nothing', async () => {
  const ui = app({ '/api/club/board': { body: board([objective()]) } });
  await ui.run('loadClubBoard()');
  ui.run("boardRows[0].importance.value = '9'");
  assert.equal(await ui.run('applyBoardObjective(0)'), false);
  assert.equal(ui.requests.length, 1);
  assert.match(ui.run('messages.at(-1)').message, /Enable experimental/);
});

test('update bodies carry only changed fields and the identity check type', () => {
  const ui = app();
  const body = ui.run(`boardUpdateBody(7, ${JSON.stringify(objective())}, { importance: '9', value: '21', deadline: '2027-04-10', status: '1' })`);
  assert.deepEqual(body, {
    club_id: 7, objective_index: 0, type_code: 150, experimental: true, importance: 9, deadline_year: 2027, deadline_day: 100,
  });
  assert.throws(() => ui.run(`boardUpdateBody(7, ${JSON.stringify(objective())}, { importance: '4', value: '21', deadline: '2026-04-28', status: '1' })`), /Nothing changed/);
  assert.throws(() => ui.run(`boardUpdateBody(7, ${JSON.stringify(objective())}, { importance: '11', value: '', deadline: '', status: '' })`), /1 to 10/);
  assert.throws(() => ui.run(`boardUpdateBody(7, ${JSON.stringify(objective())}, { importance: '', value: '', deadline: '2027-02-30', status: '' })`), /real date/);
  assert.deepEqual(ui.run('isoToYearDay("2028-12-31")'), { year: 2028, day: 366 });
});

test('a confirmed edit posts the update then reloads the board', async () => {
  const ui = app({
    '/api/club/board': { body: board([objective()]) },
    '/api/club/board/update': { body: { status: 'ok' } },
  });
  await ui.run('loadClubBoard()');
  ui.els['board-experimental'].checked = true;
  ui.run("boardRows[0].importance.value = '9'");
  assert.equal(await ui.run('applyBoardObjective(0)'), true);
  const update = ui.requests.find(r => r.url === '/api/club/board/update');
  assert.deepEqual(update.body, { club_id: 7, objective_index: 0, type_code: 150, experimental: true, importance: 9 });
  assert.equal(ui.requests.filter(r => r.url === '/api/club/board').length, 2, 'reloaded after the write');
});

test('club status: unreadable codes are unavailable, readable ones can be applied only when experimental', async () => {
  const ui = app({
    '/api/club/status': { body: { status: 'ok', club_status: { club_id: 7, code: 3, label: 'Amateur', reason: null } } },
    '/api/club/status/update': { body: { status: 'ok' } },
  });
  assert.equal(await ui.run('loadClubStatusInfo()'), true);
  assert.match(ui.els['board-status-current'].innerText, /Amateur \(code 3, label inferred\)/);
  assert.equal(ui.els['board-status-apply'].disabled, true);
  ui.els['board-experimental'].checked = true;
  ui.run('updateBoardButtons()');
  assert.equal(ui.els['board-status-apply'].disabled, false);
  ui.els['board-status-select'].value = '1';
  assert.equal(await ui.run('applyClubStatus()'), true);
  assert.deepEqual(ui.requests.find(r => r.url === '/api/club/status/update').body, { club_id: 7, status: 1, experimental: true });

  const special = app({ '/api/club/status': { body: { status: 'ok', club_status: { club_id: 7, code: null, label: null, reason: 'status byte 24 is not one of 1..=3' } } } });
  await special.run('loadClubStatusInfo()');
  assert.match(special.els['board-status-current'].innerText, /^Unavailable: status byte 24/);
  special.els['board-experimental'].checked = true;
  special.run('updateBoardButtons()');
  assert.equal(special.els['board-status-apply'].disabled, true);
});
