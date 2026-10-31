const { test } = require('node:test');
const assert = require('node:assert/strict');
const { createUi } = require('./helpers/fake-dom.cjs');
const ok = data => ({ ok: true, json: async () => data });

const IDS = ['contract-release', 'contract-release-note', 'contract-status', 'contract-status-note',
  'contract-happiness', 'contract-happiness-note', 'contract-wage', 'contract-expiry', 'savebar', 'savebar-status', 'savebar-error'];

function app(fetch = async () => ok({})) {
  const ui = createUi({ fetch, ids: IDS, tabs: ['tab-contract'] });
  ui.run("state.connected = true; state.activePlayer = { id: 5, name: 'P', contract: {} }; Shell.init();");
  return ui;
}
const render = (ui, contract) => ui.run(`renderContractTerms({ contract: ${JSON.stringify(contract)} })`);

test('connected mode shows the native squad status and release clause and never the old fallbacks', () => {
  const ui = app();
  render(ui, { squad_status: 'Fringe Player', squad_status_code: 7, release_clause: 43000420, release_clause_present: true, happiness: 'Content' });
  assert.equal(ui.els['contract-status'].value, 'Fringe Player');
  assert.equal(ui.run('readMoneyInput(els["contract-release"], null)'), 43000420);
  assert.equal(ui.run('state.activePlayer.contract.release_clause'), 43000420);
  assert.equal(ui.els['contract-happiness'].value, '', 'happiness is never shown from the game');
  assert.equal(ui.run('state.activePlayer.contract.happiness'), '');
});

test('unavailable terms stay empty instead of Regular Starter / Content', () => {
  const ui = app();
  render(ui, { squad_status: '', release_clause: 0, release_clause_present: null, terms_note: 'on loan: the game shows the loan contract, not this one' });
  assert.equal(ui.els['contract-status'].value, '');
  assert.equal(ui.els['contract-release'].value, '');
  assert.equal(ui.els['contract-release'].placeholder, 'Unavailable');
  assert.equal(ui.run('state.activePlayer.contract.squad_status'), '');
  ui.run('state.playerWritableFields = ["contract.wage_per_week"]; applyContractGates()');
  assert.equal(ui.els['contract-status'].disabled, true);
  assert.match(ui.els['contract-status-note'].innerText, /on loan/);
  assert.match(ui.els['contract-release-note'].innerText, /on loan/);
});

test('a contract without a release clause says so and cannot be given one', () => {
  const ui = app();
  render(ui, { squad_status: 'Squad Player', squad_status_code: 4, release_clause: 0, release_clause_present: false });
  assert.equal(ui.els['contract-release'].value, '');
  assert.equal(ui.els['contract-release'].placeholder, 'No release clause');
  ui.run('state.activePlayer.contract = { squad_status_code: 4, release_clause_present: false }; state.playerWritableFields = ["contract.squad_status"]; applyContractGates()');
  assert.equal(ui.els['contract-release'].disabled, true);
  assert.match(ui.els['contract-release-note'].innerText, /no release clause/i);
  assert.equal(ui.els['contract-status'].disabled, false);
});

test('happiness is explained as derived and stays disabled', () => {
  const ui = app();
  ui.run('state.playerWritableFields = ["contract.squad_status", "contract.release_clause"]; applyContractGates()');
  assert.equal(ui.els['contract-happiness'].disabled, true);
  assert.match(ui.els['contract-happiness-note'].innerText, /derives happiness from several factors/);
});

test('offline fixtures keep their demonstration values', () => {
  const ui = createUi({ fetch: async () => ok({}), ids: IDS, tabs: ['tab-contract'] });
  ui.run("state.connected = false; state.activePlayer = { id: 1, contract: {} }");
  render(ui, { squad_status: '', happiness: '', release_clause: 0 });
  assert.equal(ui.els['contract-status'].value, 'Regular Starter');
  assert.equal(ui.els['contract-happiness'].value, 'Content');
});

test('a zero release clause is not staged in connected mode', () => {
  const ui = app();
  ui.run('state.playerWritableFields = ["contract.release_clause"]');
  assert.equal(ui.run('setReleaseClauseFree()'), false);
  assert.match(ui.run('messages.at(-1).message'), /not accepted natively/);
});

test('squad status and release clause edits are saved through the player update route as changed fields', async () => {
  const ui = app(async () => ok({ player: { id: 5, name: 'P', contract: { squad_status: 'Star Player' } } }));
  render(ui, { squad_status: 'Squad Player', squad_status_code: 4, release_clause: 1000000, release_clause_present: true });
  ui.run('state.playerWritableFields = ["contract.squad_status", "contract.release_clause"]; applyContractGates()');
  ui.els['contract-status'].value = 'Star Player';
  ui.run("state.playerDirtyFields.add('contract.squad_status')");
  assert.equal(ui.requests.length, 0, 'editing makes no request');
  await ui.run('writePlayerChanges()');
  const update = ui.requests.find(r => r.url === '/api/player/update');
  assert.ok(update, 'the save goes through /api/player/update');
  assert.equal(update.body.contract.squad_status, 'Star Player');
  assert.equal(update.body.contract.release_clause, 1000000, 'the untouched clause is sent unchanged');
});
