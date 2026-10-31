const { test } = require('node:test');
const assert = require('node:assert/strict');
const { createUi } = require('./helpers/fake-dom.cjs');
const { read } = require('./helpers/ui.cjs');
const ok = data => ({ ok: true, json: async () => data });

const IDS = ['club-money-balance', 'club-money-transfer', 'prize-list', 'input-pa', 'star-rating-pa', 'savebar', 'savebar-status', 'savebar-error'];

function app(fetch = async () => ok({})) {
  const ui = createUi({ fetch, ids: IDS, tabs: ['tab-club'] });
  ui.run("state.connected = true; state.activeClub = { club_id: 7, club_name: 'Club' }; Shell.init();");
  return ui;
}

const lastMessage = ui => ui.run('messages.at(-1)');

test('the four cheats are enabled buttons that stage instead of writing', () => {
  const html = read('pages/godmode.html');
  for (const fn of ['applyWipeDebt', 'applyInfiniteWealth', 'applyMassivePrizeMoney', 'applyWonderkid']) {
    assert.match(html, new RegExp(`<button class="btn btn-sm btn-primary" onclick="${fn}\\(\\)"`), fn);
    assert.doesNotMatch(html, new RegExp(`disabled[^>]*onclick="${fn}\\(\\)"`), `${fn} must not be disabled`);
  }
  assert.doesNotMatch(html, /Load from game/, 'tools pages keep the shared bottom-bar Load');
});

test('Clear Negative Balance raises a negative balance to 0, stages it and sends nothing', async () => {
  const ui = app();
  ui.run("state.loadedMoney = { club_id: 7, balance: -5000, transfer_budget: 200 }");
  ui.els['club-money-balance'].disabled = false; ui.els['club-money-transfer'].disabled = false;
  ui.els['club-money-balance'].value = '-5000'; ui.els['club-money-transfer'].value = '200';
  assert.equal(await ui.run('applyWipeDebt()'), true);
  assert.equal(ui.els['club-money-balance'].value, '0');
  assert.equal(ui.els['club-money-transfer'].value, '200', 'the budget is untouched');
  assert.equal(ui.run("SaveBar.isDirty('club')"), true);
  assert.equal(ui.requests.length, 0, 'staging must not write');
  assert.deepEqual(ui.run('clubMoneyChanges()'), [{ label: 'Balance', from: -5000, to: 0 }]);
});

test('Clear Negative Balance does nothing for a non-negative balance', async () => {
  const ui = app();
  ui.run("state.loadedMoney = { club_id: 7, balance: 900, transfer_budget: 200 }");
  ui.els['club-money-balance'].disabled = false; ui.els['club-money-transfer'].disabled = false;
  ui.els['club-money-balance'].value = '900'; ui.els['club-money-transfer'].value = '200';
  assert.equal(await ui.run('applyWipeDebt()'), false);
  assert.equal(ui.els['club-money-balance'].value, '900');
  assert.match(lastMessage(ui).message, /not negative/);
  assert.equal(ui.run("SaveBar.isDirty('club')"), false);
});

test('Big Money raises balance and budget, never lowers them, and respects the 32-bit limit', async () => {
  const ui = app();
  ui.run("state.loadedMoney = { club_id: 7, balance: 100, transfer_budget: 2000000000 }");
  ui.els['club-money-balance'].disabled = false; ui.els['club-money-transfer'].disabled = false;
  ui.els['club-money-balance'].value = '100'; ui.els['club-money-transfer'].value = '2000000000';
  assert.equal(await ui.run('applyInfiniteWealth()'), true);
  assert.equal(ui.els['club-money-balance'].value, '1000000000');
  assert.equal(ui.els['club-money-transfer'].value, '2000000000', 'a larger budget is kept');
  assert.equal(ui.requests.length, 0);
  assert.equal(await ui.run('applyInfiniteWealth()'), false, 'a second press has nothing to raise');
  assert.match(lastMessage(ui).message, /already at or above/);
});

test('club cheats refuse until the native money was loaded for this club', async () => {
  const ui = app();
  assert.equal(await ui.run('applyWipeDebt()'), false);
  assert.match(lastMessage(ui).message, /Load in the bottom bar/);
  ui.els['club-money-balance'].disabled = false; ui.els['club-money-transfer'].disabled = false;
  ui.run("state.loadedMoney = { club_id: 99, balance: -1, transfer_budget: 1 }");
  assert.equal(await ui.run('applyInfiniteWealth()'), false, 'money loaded for a different club does not count');
  const none = createUi({ fetch: async () => ok({}), ids: IDS, tabs: ['tab-club'] });
  none.run('state.connected = true');
  assert.equal(await none.run('applyInfiniteWealth()'), false);
  assert.match(lastMessage(none).message, /Select a club first/);
});

test('staged money reaches the server only through the Save bar, as a native /api/club/money body', async () => {
  const ui = app(async () => ok({ club: { club_id: 7, balance: 0, transfer_budget: 200 } }));
  ui.run("state.loadedMoney = { club_id: 7, balance: -5000, transfer_budget: 200 }");
  ui.els['club-money-balance'].disabled = false; ui.els['club-money-transfer'].disabled = false;
  ui.els['club-money-balance'].value = '-5000'; ui.els['club-money-transfer'].value = '200';
  await ui.run('applyWipeDebt()');
  ui.run("Shell.init(); switchTab('tab-club')");
  assert.equal(await ui.run('SaveBar.save()'), true);
  const money = ui.requests.filter(r => r.url === '/api/club/money');
  assert.equal(money.length, 1);
  assert.deepEqual(money[0].body, { club_id: 7, balance: 0, transfer_budget: null });
  assert.equal(ui.requests.some(r => r.url === '/api/club/preset'), false, 'no preset route is used');
});

test('Prize Table x10 multiplies a loaded table, caps at 32 bits and needs the table loaded', async () => {
  const ui = app();
  ui.run("state.activeCompetition = { id: 51 }");
  assert.equal(await ui.run('applyMassivePrizeMoney()'), false);
  assert.match(lastMessage(ui).message, /Load this competition's prize table/);
  ui.run("prizeBaseline = { id: 51, selectors: {}, prizes: [1000, 500] }");
  ui.els['prize-list'].disabled = false;
  ui.els['prize-list'].value = '300000000\n500';
  assert.equal(await ui.run('applyMassivePrizeMoney()'), true);
  assert.equal(ui.els['prize-list'].value, '2147483647\n5000', 'capped and multiplied');
  assert.equal(ui.run("SaveBar.isDirty('competition')"), true);
  assert.equal(ui.requests.length, 0);
  ui.els['prize-list'].value = '2147483647\n2147483647';
  assert.equal(await ui.run('applyMassivePrizeMoney()'), false, 'already at the cap');
  ui.run("prizeBaseline = { id: 52, selectors: {}, prizes: [1] }");
  assert.equal(await ui.run('applyMassivePrizeMoney()'), false, 'a table loaded for another competition does not count');
});

test('Wonderkid Forge raises PA to 195 through the normal PA edit and never lowers it', async () => {
  const ui = app();
  ui.run("state.activePlayer = { id: 3, name: 'Kid', potential_ability: 140, current_ability: 120 }; state.playerWritableFields = ['potential_ability', 'current_ability']");
  assert.equal(await ui.run('applyWonderkid()'), true);
  assert.equal(ui.run('state.activePlayer.potential_ability'), 195);
  assert.equal(ui.run('state.activePlayer.current_ability'), 120, 'CA is untouched');
  assert.equal(ui.requests.length, 0, 'staging must not write');
  ui.run("state.activePlayer.potential_ability = 199");
  assert.equal(await ui.run('applyWonderkid()'), false);
  assert.equal(ui.run('state.activePlayer.potential_ability'), 199);
  assert.match(lastMessage(ui).message, /already 199/);
  ui.run("state.activePlayer.potential_ability = 100; state.playerWritableFields = []");
  assert.equal(await ui.run('applyWonderkid()'), false, 'PA must be writable for this player');
  assert.equal(ui.run('state.activePlayer.potential_ability'), 100);
});
