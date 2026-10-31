const { test } = require('node:test');
const assert = require('node:assert/strict');
const { createUi } = require('./helpers/fake-dom.cjs');
const ok = data => ({ ok: true, json: async () => data });

const IDS = ['transfer-fee', 'btn-terminate-contract', 'btn-terminate-loan', 'btn-move-player', 'btn-sign-free-agent', 'transfer-experimental',
  'transfer-target-club-id', 'transfer-sign-wage', 'transfer-sign-year', 'savebar', 'savebar-status', 'savebar-error'];

function app(fetch = async () => ok({ message: 'done' }), { confirmAnswer = true } = {}) {
  const ui = createUi({ fetch, ids: IDS, tabs: ['tab-contract'], extra: { confirm: () => confirmAnswer } });
  ui.run("state.connected = true; state.activePlayer = { id: 42, name: 'Test Player' }; Shell.init();");
  return ui;
}
const opt = (ui, on = true) => { ui.els['transfer-experimental'].checked = on; ui.run('updateTransferButtons()'); };

test('transfer buttons are disabled until attached, a player is open and the opt-in is ticked; loan recall never', () => {
  const ui = app();
  ui.run('updateTransferButtons()');
  for (const id of ['btn-terminate-contract', 'btn-move-player', 'btn-sign-free-agent']) assert.equal(ui.els[id].disabled, true, id);
  opt(ui);
  for (const id of ['btn-terminate-contract', 'btn-move-player', 'btn-sign-free-agent']) assert.equal(ui.els[id].disabled, false, id);
  assert.equal(ui.els['btn-terminate-loan'].disabled, true);
  ui.run('state.connected = false; updateTransferButtons()');
  assert.equal(ui.els['btn-terminate-contract'].disabled, true);
  assert.match(ui.els['btn-terminate-contract'].title, /Attach to a game/);
  ui.run('state.connected = true; state.activePlayer = null; updateTransferButtons()');
  assert.match(ui.els['btn-move-player'].title, /Open a player/);
});

test('terminate asks for confirmation, sends the experimental opt-in and reloads the player', async () => {
  const ui = app(async (url) => (url === '/api/context/select' ? ok({ player: { id: 42, name: 'Test Player', club_id: 0 } }) : ok({ message: 'Player contract terminated.' })));
  opt(ui);
  ui.run('displayPlayer = player => { state.activePlayer = player; }');
  assert.equal(await ui.run('terminateContract()'), true);
  const call = ui.requests.find(r => r.url === '/api/player/contract/terminate');
  assert.deepEqual(call.body, { player_id: 42, experimental: true });
  assert.ok(ui.requests.some(r => r.url === '/api/context/select' && r.body.player_id === 42), 'the player is reloaded');
  assert.match(ui.run('messages.at(-1).message'), /Advance a day/);
});

test('declining the confirmation or leaving the opt-in off sends nothing', async () => {
  const declined = app(undefined, { confirmAnswer: false });
  opt(declined);
  assert.equal(await declined.run('terminateContract()'), false);
  assert.equal(declined.requests.length, 0);
  const off = app();
  assert.equal(await off.run('terminateContract()'), false);
  assert.equal(off.requests.length, 0);
  assert.match(off.run('messages.at(-1).message'), /experimental checkbox/);
});

test('move needs a valid target club id and sends it with the opt-in', async () => {
  const ui = app();
  opt(ui);
  ui.run('displayPlayer = () => {}');
  for (const bad of ['', '0', '-3', '1.5']) {
    ui.els['transfer-target-club-id'].value = bad;
    assert.equal(await ui.run('movePlayer()'), false, bad);
  }
  assert.equal(ui.requests.length, 0);
  ui.els['transfer-target-club-id'].value = '1094';
  assert.equal(await ui.run('movePlayer()'), true);
  assert.deepEqual(ui.requests.find(r => r.url === '/api/transfer/move').body, { player_id: 42, target_club_id: 1094, experimental: true });
});

test('sign requires an explicit wage and expiry year (no invented defaults)', async () => {
  const ui = app();
  opt(ui);
  ui.run('displayPlayer = () => {}');
  ui.els['transfer-target-club-id'].value = '1094';
  assert.equal(await ui.run('signFreeAgent()'), false, 'no wage or year');
  ui.els['transfer-sign-wage'].value = '5000';
  assert.equal(await ui.run('signFreeAgent()'), false, 'no year');
  ui.els['transfer-sign-year'].value = '1999';
  assert.equal(await ui.run('signFreeAgent()'), false, 'year out of range');
  assert.equal(ui.requests.length, 0);
  ui.els['transfer-sign-year'].value = '2028';
  assert.equal(await ui.run('signFreeAgent()'), true);
  assert.deepEqual(ui.requests.find(r => r.url === '/api/transfer/sign').body,
    { player_id: 42, target_club_id: 1094, wage_gbp: 5000, expiry_year: 2028, experimental: true });
});

test('a server refusal is shown and the player is not reloaded; loan recall stays unavailable', async () => {
  const ui = app(async () => ({ ok: false, json: async () => ({ message: 'Target team squad vector is at capacity' }) }));
  opt(ui);
  assert.equal(await ui.run('terminateContract()'), false);
  assert.match(ui.run('messages.at(-1).message'), /at capacity/);
  assert.equal(ui.requests.some(r => r.url === '/api/context/select'), false);
  assert.equal(await ui.run('terminateLoan()'), false);
  assert.match(ui.run('messages.at(-1).message'), /no in-game test/);
});

test('target club squad space is read-only and shown next to the club id', async () => {
  const ui = createUi({
    fetch: async url => (url === '/api/club/squad-space' ? ok({ status: 'success', club_id: 1094, players: 21, capacity: 30, spare_slots: 9 }) : ok({})),
    ids: [...IDS, 'transfer-space-note'], tabs: ['tab-contract'],
  });
  ui.run('state.connected = true');
  ui.els['transfer-target-club-id'].value = '1094';
  const data = await ui.run('loadTransferSpace()');
  assert.equal(data.spare_slots, 9);
  assert.match(ui.els['transfer-space-note'].innerText, /21 players, 9 spare slots/);
  assert.deepEqual(ui.requests.map(r => r.url), ['/api/club/squad-space']);
  ui.els['transfer-target-club-id'].value = 'abc';
  assert.equal(await ui.run('loadTransferSpace()'), null);
  assert.equal(ui.els['transfer-space-note'].innerText, '');
  assert.equal(ui.requests.length, 1, 'an invalid id makes no request');
});

test('move sends an optional transfer fee only when one is entered, and rejects invalid fees', async () => {
  const ui = app();
  opt(ui);
  ui.run('displayPlayer = () => {}');
  ui.els['transfer-target-club-id'].value = '1094';
  for (const bad of ['0', '-5', '1.5', '2147483648', 'abc']) {
    ui.els['transfer-fee'].value = bad;
    assert.equal(await ui.run('movePlayer()'), false, bad);
  }
  assert.equal(ui.requests.length, 0);
  ui.els['transfer-fee'].value = '15000000';
  assert.equal(await ui.run('movePlayer()'), true);
  assert.deepEqual(ui.requests.find(r => r.url === '/api/transfer/move').body, { player_id: 42, target_club_id: 1094, fee_gbp: 15000000, experimental: true });
  ui.requests.length = 0;
  ui.els['transfer-fee'].value = '';
  assert.equal(await ui.run('movePlayer()'), true);
  assert.deepEqual(ui.requests.find(r => r.url === '/api/transfer/move').body, { player_id: 42, target_club_id: 1094, experimental: true }, 'no fee field without a fee');
});
