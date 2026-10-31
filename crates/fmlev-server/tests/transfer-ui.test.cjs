const { test } = require('node:test');
const assert = require('node:assert/strict');
const { createUi } = require('./helpers/fake-dom.cjs');
const ok = data => ({ ok: true, json: async () => data });

const IDS = ['transfer-fee', 'transfer-missing-note', 'btn-terminate-contract', 'btn-terminate-loan', 'btn-move-player', 'btn-sign-free-agent',
  'transfer-target-club-id', 'transfer-sign-wage', 'transfer-sign-year', 'savebar', 'savebar-status', 'savebar-error'];

function app(fetch = async () => ok({ message: 'done' }), { confirmAnswer = true } = {}) {
  const ui = createUi({ fetch, ids: IDS, tabs: ['tab-contract'], extra: { confirm: () => confirmAnswer } });
  ui.run("state.connected = true; state.activePlayer = { id: 42, name: 'Test Player' }; Shell.init();");
  return ui;
}
const fill = (ui, { club = '1094', wage = '260000', year = '2028' } = {}) => {
  ui.els['transfer-target-club-id'].value = club; ui.els['transfer-sign-wage'].value = wage; ui.els['transfer-sign-year'].value = year;
};
const opt = ui => ui.run('updateTransferButtons()'); // no opt-in any more: just refresh the button state

test('terminate asks for confirmation, sends the request and reloads the player', async () => {
  const ui = app(async (url) => (url === '/api/context/select' ? ok({ player: { id: 42, name: 'Test Player', club_id: 0 } }) : ok({ message: 'Player contract terminated.' })));
  opt(ui);
  ui.run('displayPlayer = player => { state.activePlayer = player; }');
  assert.equal(await ui.run('terminateContract()'), true);
  const call = ui.requests.find(r => r.url === '/api/player/contract/terminate');
  assert.deepEqual(call.body, { player_id: 42 });
  assert.ok(ui.requests.some(r => r.url === '/api/context/select' && r.body.player_id === 42), 'the player is reloaded');
  assert.match(ui.run('messages.at(-1).message'), /Advance a day/);
});

test('declining the confirmation, no player or no game sends nothing', async () => {
  const declined = app(undefined, { confirmAnswer: false });
  opt(declined);
  assert.equal(await declined.run('terminateContract()'), false);
  assert.equal(declined.requests.length, 0);
  const noPlayer = app();
  noPlayer.run('state.activePlayer = null');
  assert.equal(await noPlayer.run('terminateContract()'), false);
  assert.match(noPlayer.run('messages.at(-1).message'), /open a player first/i);
  const detached = app();
  detached.run('state.connected = false');
  assert.equal(await detached.run('terminateContract()'), false);
  assert.equal(detached.requests.length, 0);
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

const FIELDS = ['transfer-target-club-id', 'transfer-sign-wage', 'transfer-sign-year'];
const ENABLED = ui => ['btn-terminate-contract', 'btn-move-player', 'btn-sign-free-agent'].map(id => !ui.els[id].disabled);

test('Move and Sign stay disabled until the target club, yearly wage and expiry year are all filled; loan recall never', () => {
  const ui = app();
  opt(ui);
  assert.deepEqual(ENABLED(ui), [true, false, false], 'terminate needs no fields; Move and Sign need the form');
  assert.match(ui.els['btn-move-player'].title, /Fill the required fields: target club ID, yearly wage, expiry year/);
  assert.match(ui.els['transfer-missing-note'].innerText, /Required before Move or Sign: target club ID, yearly wage, expiry year/);
  for (const [id, value] of [['transfer-target-club-id', '1094'], ['transfer-sign-wage', '260000']]) {
    ui.els[id].value = value; ui.run('updateTransferButtons()');
    assert.deepEqual(ENABLED(ui), [true, false, false], `still incomplete after ${id}`);
  }
  assert.match(ui.els['transfer-missing-note'].innerText, /expiry year/);
  ui.els['transfer-sign-year'].value = '2028'; ui.run('updateTransferButtons()');
  assert.deepEqual(ENABLED(ui), [true, true, true]);
  assert.equal(ui.els['transfer-missing-note'].innerText, '');
  assert.equal(ui.els['btn-terminate-loan'].disabled, true);
  ui.run('state.connected = false; updateTransferButtons()');
  assert.match(ui.els['btn-move-player'].title, /Attach to a game/);
  ui.run('state.connected = true; state.activePlayer = null; updateTransferButtons()');
  assert.match(ui.els['btn-sign-free-agent'].title, /Open a player/);
});

test('an invalid fee also disables Move but not Sign', () => {
  const ui = app();
  opt(ui);
  fill(ui);
  ui.els['transfer-fee'].value = '0';
  ui.run('updateTransferButtons()');
  assert.equal(ui.els['btn-move-player'].disabled, true);
  assert.match(ui.els['btn-move-player'].title, /fee must be a whole number/);
  assert.equal(ui.els['btn-sign-free-agent'].disabled, false);
  ui.els['transfer-fee'].value = '';
  ui.run('updateTransferButtons()');
  assert.equal(ui.els['btn-move-player'].disabled, false);
});

test('Move and Sign refuse an incomplete form and list what is missing', async () => {
  const ui = app();
  opt(ui);
  ui.run('displayPlayer = () => {}');
  for (const action of ['movePlayer', 'signFreeAgent']) {
    assert.equal(await ui.run(`${action}()`), false, action);
    assert.match(ui.run('messages.at(-1).message'), /Required: target club ID, yearly wage, expiry year/);
  }
  fill(ui, { year: '' });
  assert.equal(await ui.run('movePlayer()'), false);
  assert.match(ui.run('messages.at(-1).message'), /Required: expiry year\./);
  for (const [field, bad] of [['transfer-target-club-id', '0'], ['transfer-target-club-id', '1.5'], ['transfer-sign-wage', '20'], ['transfer-sign-wage', 'abc'], ['transfer-sign-year', '1999'], ['transfer-sign-year', '2101']]) {
    fill(ui); ui.els[field].value = bad;
    assert.equal(await ui.run('signFreeAgent()'), false, `${field}=${bad}`);
  }
  assert.equal(ui.requests.length, 0, 'an incomplete form never reaches the server');
});

test('Move sends the new wage (yearly / 52) and expiry with the club, plus the fee only when entered', async () => {
  const ui = app();
  opt(ui);
  ui.run('displayPlayer = () => {}');
  fill(ui, { wage: '4160000' }); // 80,000 per week
  assert.equal(await ui.run('movePlayer()'), true);
  assert.deepEqual(ui.requests.find(r => r.url === '/api/transfer/move').body,
    { player_id: 42, target_club_id: 1094, wage_gbp: 80000, expiry_year: 2028 });
  ui.requests.length = 0;
  ui.els['transfer-fee'].value = '15000000';
  assert.equal(await ui.run('movePlayer()'), true);
  assert.deepEqual(ui.requests.find(r => r.url === '/api/transfer/move').body,
    { player_id: 42, target_club_id: 1094, wage_gbp: 80000, expiry_year: 2028, fee_gbp: 15000000 });
  ui.requests.length = 0;
  for (const bad of ['0', '-5', '1.5', '2147483648', 'abc']) {
    ui.els['transfer-fee'].value = bad;
    assert.equal(await ui.run('movePlayer()'), false, `fee ${bad}`);
  }
  assert.equal(ui.requests.length, 0);
});

test('Sign sends the stored weekly wage (yearly / 52, rounded), the expiry year and the club', async () => {
  const ui = app();
  opt(ui);
  ui.run('displayPlayer = () => {}');
  fill(ui, { wage: '245000' }); // 4711.5 per week rounds to 4712
  assert.equal(await ui.run('signFreeAgent()'), true);
  assert.deepEqual(ui.requests.find(r => r.url === '/api/transfer/sign').body,
    { player_id: 42, target_club_id: 1094, wage_gbp: 4712, expiry_year: 2028 });
});

test('the confirm text names the contract terms that will be written', async () => {
  let asked = '';
  const ui = createUi({ fetch: async () => ok({ message: 'done' }), ids: IDS, tabs: ['tab-contract'], extra: { confirm: text => { asked = text; return true; } } });
  ui.run("state.connected = true; state.activePlayer = { id: 42, name: 'Test Player' }; Shell.init(); displayPlayer = () => {}");
  opt(ui);
  fill(ui, { wage: '260000' });
  await ui.run('movePlayer()');
  assert.match(asked, /260,000 per year \(5,000 per week\) until 2028/);
  assert.match(asked, /current wage and expiry are replaced/);
});
