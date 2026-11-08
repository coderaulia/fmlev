const { test } = require('node:test');
const assert = require('node:assert/strict');
const { html, read } = require('./helpers/ui.cjs');
const { createUi } = require('./helpers/fake-dom.cjs');

const ok = data => ({ ok: true, json: async () => data });
const deal = (extra = {}) => ({
  direction: 'out',
  deal: {
    address: 9807949248, deal_type: 1, deal_type_label: 'transfer',
    player: { kind: 'player', id: 25145, uid: 28123456, name: 'Kylian Mbappé', unavailable_reason: null },
    buying_team: { kind: 'team', club_id: 369, club_name: 'Manchester City' },
    selling_team: { kind: 'team', club_id: 1089, club_name: 'R. Madrid' },
    fee_gbp: 92818912, join_date: { iso: '2026-01-04', year: 2026, day: 4 },
    status_label: 'Offer Made', status_text: 'Offer Made',
    clauses: { present: true, instalments: [{ amount_gbp: 29402316, months: 24, count: 2 }], performance: [{ amount_gbp: 41575136, threshold: 20 }], unavailable_reason: null },
    ...extra,
  },
});
const IDS = ['club-deals-rows', 'club-deals-status', 'club-deals-experimental', 'club-deal-fee-0', 'player-deals-rows', 'player-deals-status', 'player-deals-experimental', 'player-deal-fee-0'];

function app(fetch, { confirmAnswer = true } = {}) {
  const ui = createUi({ fetch, ids: IDS, tabs: ['tab-club'], extra: { confirm: () => confirmAnswer } });
  ui.run("state.connected = true; state.activeClub = { club_id: 1089 };");
  return ui;
}
const reader = (deals = [deal()]) => async url => (url === '/api/deals/club' ? ok({ status: 'ok', deals, scanned: { candidates: deals.length } }) : ok({ message: 'Deal updated in memory.' }));

test('the card is a direct, marked live-editor child of the club tab and the script is registered', () => {
  assert.match(html, /id="club-deals-card" data-live-editor="true"/);
  const start = html.indexOf('<section id="tab-club"');
  const end = html.indexOf('</section>', start);
  const card = html.indexOf('id="club-deals-card"', start);
  assert.ok(card > start && card < end);
  const before = html.slice(start, html.lastIndexOf('<div', card));
  assert.equal((before.match(/<div[\s>]/g) || []).length - (before.match(/<\/div>/g) || []).length, 0, 'depth 0 inside tab-club');
  assert.match(read('manifest.json'), /js\/pages\/club-deals\.js/);
  assert.match(read('js/core/input-guard.js'), /#club-deals-card/, 'deal inputs do not dirty the club form');
  assert.match(html, /id="club-deals-experimental" type="checkbox"(?![^>]*checked)/, 'the experimental box starts unchecked');
});

test('loading lists the club deals with labels, raw status for unknown combinations and a clause summary', async () => {
  const unknown = deal({ status_label: null, status_text: 'Status 0xff02 / +0x84 3', join_date: null });
  const ui = app(reader([deal(), unknown]));
  const data = await ui.run('loadClubDeals()');
  assert.equal(data.deals.length, 2);
  assert.deepEqual(ui.requests.map(r => [r.url, r.body]), [['/api/deals/club', { club_id: 1089 }]]);
  const rows = ui.els['club-deals-rows'].innerHTML;
  assert.match(rows, /Offer Made/);
  assert.match(rows, /Status 0xff02 \/ \+0x84 3 \(unknown\)/);
  assert.match(rows, /Kylian Mbapp/);
  assert.match(rows, /2 instalments \(24 months\)/);
  assert.match(rows, /after 20/);
  assert.match(rows, /Unavailable/, 'a deal without a join date shows Unavailable');
  assert.match(ui.els['club-deals-status'].innerText, /2 deals/);
});

test('without a game or a club nothing is requested', async () => {
  const detached = app(reader());
  detached.run('state.connected = false');
  assert.equal(await detached.run('loadClubDeals()'), null);
  assert.equal(detached.requests.length, 0);
  const noClub = app(reader());
  noClub.run('state.activeClub = null');
  assert.equal(await noClub.run('loadClubDeals()'), null);
  assert.match(noClub.els['club-deals-status'].innerText, /Select a club/);
  assert.equal(noClub.requests.length, 0);
});

test('a failed read shows Unavailable with the reason and no rows', async () => {
  const ui = app(async () => ({ ok: false, json: async () => ({ message: 'Unknown game_plugin.dll build' }) }));
  assert.equal(await ui.run('loadClubDeals()'), null);
  assert.match(ui.els['club-deals-status'].innerText, /Unavailable: Unknown game_plugin\.dll build/);
  assert.match(ui.els['club-deals-rows'].innerHTML, /No deals loaded/);
});

test('editing is disabled until the experimental box is ticked', async () => {
  const ui = app(reader());
  await ui.run('loadClubDeals()');
  assert.match(ui.els['club-deals-rows'].innerHTML, /<button[^>]*disabled/);
  assert.equal(await ui.run('applyClubDeal(0)'), false);
  assert.match(ui.run('messages.at(-1).message'), /Enable experimental/);
  ui.els['club-deals-experimental'].checked = true;
  ui.run('updateClubDealControls()');
  assert.doesNotMatch(ui.els['club-deals-rows'].innerHTML, /<button[^>]*disabled/);
});

test('Apply sends the fee and baseline identity, then reloads', async () => {
  const ui = app(reader());
  await ui.run('loadClubDeals()');
  ui.els['club-deals-experimental'].checked = true;
  ui.els['club-deal-fee-0'].value = '50000000';
  assert.equal(await ui.run('applyClubDeal(0)'), true);
  const call = ui.requests.find(r => r.url === '/api/deals/update');
  assert.deepEqual(call.body, { deal_address: 9807949248, player_id: 25145, player_uid: 28123456,
    expected_fee_gbp: 92818912, experimental: true, fee_gbp: 50000000 });
  assert.equal(ui.requests.filter(r => r.url === '/api/deals/club').length, 2);
  assert.match(ui.els['club-deals-rows'].innerHTML, /Scheduling edit unavailable/);
  assert.doesNotMatch(ui.els['club-deals-rows'].innerHTML, /type="date"/);
});

test('unchanged, invalid and declined edits send nothing', async () => {
  const ui = app(reader());
  await ui.run('loadClubDeals()');
  ui.els['club-deals-experimental'].checked = true;
  ui.els['club-deal-fee-0'].value = '92818912';
  assert.equal(await ui.run('applyClubDeal(0)'), false);
  assert.match(ui.run('messages.at(-1).message'), /Nothing changed/);
  ui.els['club-deal-fee-0'].value = '-5';
  assert.equal(await ui.run('applyClubDeal(0)'), false);
  const declined = app(reader(), { confirmAnswer: false });
  await declined.run('loadClubDeals()');
  declined.els['club-deals-experimental'].checked = true;
  declined.els['club-deal-fee-0'].value = '7';
  assert.equal(await declined.run('applyClubDeal(0)'), false);
  assert.equal(declined.requests.some(r => r.url === '/api/deals/update'), false);
  assert.equal(ui.requests.some(r => r.url === '/api/deals/update'), false);
});

test('staff deals have no player id and cannot be edited', async () => {
  const staff = deal({ player: { kind: 'staff', id: null, name: 'Coach', unavailable_reason: "staff member's deal: not a player, so it has no player id" } });
  const ui = app(reader([staff]));
  await ui.run('loadClubDeals()');
  ui.els['club-deals-experimental'].checked = true;
  ui.run('updateClubDealControls()');
  assert.match(ui.els['club-deals-rows'].innerHTML, /not a player/);
  assert.equal(await ui.run('applyClubDeal(0)'), false);
  assert.equal(ui.requests.some(r => r.url === '/api/deals/update'), false);
});

test('Contract lists and edits only the selected player’s deals', async () => {
  const ui = app(async url => url === '/api/deals/player'
    ? ok({ deals: [deal().deal] })
    : ok({ message: 'Saved' }));
  ui.run("state.activePlayer = { id: 25145 };");
  await ui.run("loadClubDeals('player')");
  assert.match(html, /id="player-deals-card"/);
  assert.deepEqual(ui.requests[0].body, { player_id: 25145 });
  assert.match(ui.els['player-deals-rows'].innerHTML, /Manchester City/);
  ui.els['player-deals-experimental'].checked = true;
  ui.els['player-deal-fee-0'].value = '1';
  ui.run("state.activePlayer = { id: 99 };");
  assert.equal(await ui.run("applyClubDeal(0, 'player')"), false);
  assert.equal(ui.requests.length, 1);
  ui.run("state.activePlayer = { id: 25145 };");
  assert.equal(await ui.run("applyClubDeal(0, 'player')"), true);
  assert.equal(ui.requests[1].body.expected_fee_gbp, 92818912);
  assert.equal(ui.requests[1].body.fee_gbp, 1);
});
