const { test } = require('node:test');
const assert = require('node:assert/strict');
const { createUi } = require('./helpers/fake-dom.cjs');
const { read } = require('./helpers/ui.cjs');

const embargo = {
  club_id: 7, any_set: true, age_restriction: 20, flags: 4, flag_names: ['global_transfer_ban'],
  start: { raw: 1, empty: true, year: null, day: null }, end: { raw: 2, empty: false, year: 2027, day: 151 },
  appeal: { raw: 1, empty: true, year: null, day: null },
};
const field = (key, label, value) => ({ key, label, offset: 0, value });
const period = (key, extra = {}) => ({
  key, label: key, available: true, reason: null, all_zero: false,
  income: [field('gate_receipts', 'Gate receipts', 949377), field('other', 'Other', 0)],
  expenditure: [field('player_wages', 'Player wages', 3559283)],
  unmapped_income: [], unmapped_expenditure: [], ...extra,
});
const statements = {
  club_id: 7,
  periods: [period('this_month'), period('last_month'), period('this_season'),
    period('last_season', { available: false, reason: 'Field player_wages holds -5: not live data', income: [], expenditure: [] })],
};

function app(handler) {
  const ids = ['ce-summary', 'ce-status', 'ce-experimental', 'ce-apply', 'ce-lift', 'cs-body', 'ce-start-year', 'ce-start-day',
    'ce-end-year', 'ce-end-day', 'ce-appeal-year', 'ce-appeal-day', 'ce-age', 'ce-flags'];
  const fetch = async (url, init) => handler(url, init && init.body ? JSON.parse(init.body) : undefined);
  const ui = createUi({ fetch, ids });
  ui.run('state.activeClub = { club_id: 7, club_name: "Club" }');
  return ui;
}

const ok = data => ({ ok: true, json: async () => data });
const normal = async url => ok(url.endsWith('statements') ? { status: 'ok', statements } : { status: 'ok', embargo });

test('Club page has the embargo and statement cards, wired to existing globals', () => {
  const html = read('pages/club.html');
  assert.match(html, /id="club-embargo-card"/);
  assert.match(html, /id="club-statements-card"/);
  assert.match(html, /id="ce-experimental" type="checkbox"/);
  assert.doesNotMatch(html, /id="ce-experimental"[^>]*checked/);
});

test('loading reads embargo and statements and shows them', async () => {
  const ui = app(normal);
  assert.equal(await ui.run('loadClubFinanceExtras()'), true);
  assert.deepEqual(ui.requests.map(r => r.url).sort(), ['/api/club/embargo', '/api/club/finances/statements']);
  assert.match(ui.els['ce-summary'].innerText, /End 2027, day 151/);
  assert.match(ui.els['ce-summary'].innerText, /global_transfer_ban/);
  assert.match(ui.els['cs-body'].innerHTML, /Gate receipts/);
  assert.doesNotMatch(ui.els['cs-body'].innerHTML, /<td>Other<\/td>/, 'zero values are not listed');
});

test('an unavailable period shows its reason, never zeros', async () => {
  const ui = app(normal);
  await ui.run('loadClubFinanceExtras()');
  ui.run('selectClubStatementPeriod("last_season")');
  assert.match(ui.els['cs-body'].innerHTML, /Unavailable: Field player_wages holds -5/);
  assert.doesNotMatch(ui.els['cs-body'].innerHTML, /<table/);
});

test('a failed embargo read says unavailable and keeps the edit buttons disabled', async () => {
  const ui = app(async url => url.endsWith('statements') ? ok({ statements }) : { ok: false, json: async () => ({ message: 'Embargo date word is invalid' }) });
  assert.equal(await ui.run('loadClubFinanceExtras()'), false);
  assert.match(ui.els['ce-status'].innerText, /Unavailable\. Embargo: Embargo date word is invalid/);
  ui.els['ce-experimental'].checked = true;
  ui.run('updateClubEmbargoButtons()');
  assert.equal(ui.els['ce-apply'].disabled, true);
});

test('embargo edits need the experimental box, valid fields and a confirmation, and send the opt-in flag', async () => {
  const ui = app(async (url, body) => url.endsWith('/update') ? ok({ status: 'ok', embargo: { ...embargo, age_restriction: 18 } }) : normal(url, body));
  await ui.run('loadClubFinanceExtras()');
  assert.equal(ui.els['ce-apply'].disabled, true, 'disabled until the experimental box is ticked');
  ui.els['ce-experimental'].checked = true;
  ui.run('updateClubEmbargoButtons()');
  assert.equal(ui.els['ce-apply'].disabled, false);
  // Nothing filled in: refused, no request.
  const before = ui.requests.length;
  assert.equal(await ui.run('applyClubEmbargo()'), false);
  // Half a date: refused.
  ui.els['ce-end-year'].value = '2028';
  assert.equal(await ui.run('applyClubEmbargo()'), false);
  assert.equal(ui.requests.length, before);
  ui.els['ce-end-day'].value = '100';
  ui.els['ce-age'].value = '18';
  ui.run('globalThis.confirm = () => true');
  assert.equal(await ui.run('applyClubEmbargo()'), true);
  const sent = ui.requests.at(-1);
  assert.equal(sent.url, '/api/club/embargo/update');
  assert.deepEqual(sent.body, { club_id: 7, end: { year: 2028, day: 100 }, age_restriction: 18, experimental: true });
});

test('lifting an embargo clears dates, age and flags only after confirmation', async () => {
  const ui = app(async (url, body) => url.endsWith('/update') ? ok({ status: 'ok', embargo: { ...embargo, any_set: false } }) : normal(url, body));
  await ui.run('loadClubFinanceExtras()');
  ui.els['ce-experimental'].checked = true;
  ui.run('globalThis.confirm = () => false');
  const before = ui.requests.length;
  assert.equal(await ui.run('liftClubEmbargo()'), false);
  assert.equal(ui.requests.length, before, 'declined confirmation sends nothing');
  ui.run('globalThis.confirm = () => true');
  assert.equal(await ui.run('liftClubEmbargo()'), true);
  assert.deepEqual(ui.requests.at(-1).body, {
    club_id: 7, start: { clear: true }, end: { clear: true }, appeal: { clear: true }, age_restriction: 0, flags: 0, experimental: true,
  });
});

test('flag bit 0x40 and out-of-range ages are refused client side', async () => {
  const ui = app(normal);
  await ui.run('loadClubFinanceExtras()');
  ui.els['ce-experimental'].checked = true;
  const before = ui.requests.length;
  ui.els['ce-flags'].value = '64';
  assert.equal(await ui.run('applyClubEmbargo()'), false);
  ui.els['ce-flags'].value = '';
  ui.els['ce-age'].value = '99';
  assert.equal(await ui.run('applyClubEmbargo()'), false);
  assert.equal(ui.requests.length, before);
});
