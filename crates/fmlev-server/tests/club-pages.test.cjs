const { test } = require('node:test');
const assert = require('node:assert/strict');
const { createUi } = require('./helpers/fake-dom.cjs');
const { read } = require('./helpers/ui.cjs');
const ok = data => ({ ok: true, json: async () => data });

function app(fetch = async () => ok({ club: { club_id: 7, balance: 123456789, transfer_budget: 500 } })) {
  const ui = createUi({ fetch, ids: ['club-money-balance', 'club-money-transfer', 'fac-training',
    'fac-training-disp', 'fac-training-stars', 'savebar', 'savebar-status', 'savebar-error'], tabs: ['tab-club'] });
  ui.run("state.activeClub = { club_id: 7, club_name: 'Club', training_facilities: 18 }; Shell.init(); switchTab('tab-club')");
  return ui;
}

test('Club uses the shared Load/Save pair, without card Save buttons', () => {
  assert.doesNotMatch(read('pages/club.html'), /onclick="(?:saveClubToGame|loadClubMoney|saveClubMoney)\(/);
  assert.match(read('pages/club.html'), /id="club-overview-card" data-live-editor="true"/);
});

test('money edits are reviewed and send only changed raw amounts on explicit Save', async () => {
  const ui = app();
  assert.equal(await ui.run('loadClubMoney()'), true);
  ui.els['club-money-transfer'].value = '800';
  ui.run("SaveBar.markDirty('club')");
  assert.deepEqual(ui.run('clubMoneyChanges()'), [{ label: 'Transfer budget', from: 500, to: 800 }]);
  assert.equal(ui.requests.length, 1, 'editing made no request');
  assert.equal(await ui.run('SaveBar.save()'), true);
  assert.deepEqual(ui.requests[1].body, { club_id: 7, balance: null, transfer_budget: 800 });
  assert.equal(ui.requests.some(r => r.url === '/api/club/update'), false);
});

test('failed money saves retain changes and block queued actions', async () => {
  let fail = false;
  const ui = app(async () => fail ? { ok: false, json: async () => ({ message: 'Readback failed' }) }
    : ok({ club: { club_id: 7, balance: 123456789, transfer_budget: 500 } }));
  await ui.run('loadClubMoney()');
  ui.els['club-money-transfer'].value = '900';
  ui.run("queuePreset('club', 'max_facilities'); SaveBar.markDirty('club')");
  fail = true;
  assert.equal(await ui.run('SaveBar.save()'), false);
  assert.equal(ui.run("SaveBar.isDirty('club')"), true);
  assert.equal(ui.run("pendingActions.get('club:7').length"), 1);
  assert.equal(ui.requests.some(r => r.url === '/api/club/preset'), false);
});

test('late money reads cannot overwrite a different selected club', async () => {
  let release;
  const ui = app(() => new Promise(resolve => { release = resolve; }));
  const load = ui.run('loadClubMoney()');
  ui.run("state.activeClub = { club_id: 8, club_name: 'Other' }");
  release(ok({ club: { club_id: 7, balance: 1, transfer_budget: 2 } }));
  assert.equal(await load, false);
  assert.equal(ui.els['club-money-balance'].value, '');
  assert.equal(ui.run('state.loadedMoney'), undefined);
});

test('missing native facilities show unavailable and cannot be stepped', () => {
  const ui = app();
  ui.run('state.connected = true; renderClubFacilities(state.activeClub)');
  assert.equal(ui.els['fac-training'].disabled, true);
  assert.equal(ui.els['fac-training-disp'].innerText, '—');
  ui.run("stepClubFac('fac-training', 1)");
  assert.equal(ui.run('state.clubDirty'), false);
});

test('native facility ratings get stars and FM value bands without writing', () => {
  const ui = app();
  ui.run("state.connected = true; clubNativeRecord = { club_id: 7, training_facilities: 18 }; renderClubFacilities(state.activeClub)");
  assert.equal(ui.els['fac-training'].disabled, false);
  assert.equal(ui.els['fac-training-disp'].innerText, '18');
  assert.match(ui.els['fac-training-disp'].className, /band-16-20/);
  assert.equal(ui.els['fac-training-stars'].innerText, '★★★★★');
  assert.equal(ui.requests.length, 0);
});

test('a facility save preserves unavailable fields rather than inventing ratings', async () => {
  const ui = app(async () => ok({}));
  ui.run('state.connected = true; state.activeClub.data_analysis_facilities = 0');
  ui.els['fac-training'].value = '19';
  ui.run('markClubUnsaved()');
  assert.equal(await ui.run('SaveBar.save()'), true);
  assert.equal(ui.requests.length, 1);
  assert.equal(ui.requests[0].url, '/api/club/update');
  assert.equal(ui.requests[0].body.training_facilities, 19);
  assert.equal(ui.requests[0].body.data_analysis_facilities, 0);
  assert.equal(Object.hasOwn(ui.requests[0].body, 'youth_facilities'), false);
});

test('Discard reloads club and money and drops queued actions without a write', async () => {
  const ui = app(async url => ok({ club: { club_id: 7, club_name: 'Club', balance: 100, transfer_budget: 200 } }));
  ui.run("displayClub = club => { state.activeClub = club; }; queuePreset('club', 'max_facilities'); state.clubDirty = true");
  assert.equal(await ui.run('SaveBar.discard()'), true);
  assert.equal(ui.run('state.clubDirty'), false);
  assert.equal(ui.run("pendingActions.has('club:7')"), false);
  assert.equal(ui.els['club-money-balance'].value, '100');
  assert.deepEqual(ui.requests.map(r => r.url), ['/api/context/select', '/api/club/money']);
  assert.deepEqual(ui.requests[1].body, { club_id: 7 });
});

test('facility edits made during Save stay pending and stop queued actions', async () => {
  let release;
  const ui = app(() => new Promise(resolve => { release = resolve; }));
  ui.document.querySelectorAll = selector => selector.startsWith('#tab-club input') ? [ui.els['fac-training']] : [];
  ui.els['fac-training'].value = '19';
  ui.run("markClubUnsaved(); queuePreset('club', 'max_facilities')");
  const save = ui.run('SaveBar.save()');
  ui.els['fac-training'].value = '20';
  ui.run('markClubUnsaved()');
  release(ok({}));
  assert.equal(await save, false);
  assert.equal(ui.run('state.clubDirty'), true);
  assert.equal(ui.run("SaveBar.isDirty('club')"), true);
  assert.equal(ui.requests.length, 1);
  assert.equal(ui.requests[0].body.training_facilities, 19);
});

test('actions staged during a money save wait for the next explicit Save', async () => {
  let release;
  const ui = app(() => new Promise(resolve => { release = resolve; }));
  ui.run('state.loadedMoney = { club_id: 7, balance: 100, transfer_budget: 200 }');
  ui.els['club-money-balance'].value = '100';
  ui.els['club-money-transfer'].value = '300';
  ui.run("SaveBar.markDirty('club')");
  const save = ui.run('SaveBar.save()');
  ui.run("queuePreset('club', 'max_facilities')");
  release(ok({ club: { club_id: 7, balance: 100, transfer_budget: 300 } }));
  assert.equal(await save, false);
  assert.equal(ui.requests.length, 1);
  assert.equal(ui.run("pendingActions.get('club:7').length"), 1);
});

test('club info shows the native stadium name and capacities, and Unavailable for anything unread', () => {
  const ui = createUi({ fetch: async () => ok({}), ids: ['club-info-id', 'club-info-stadium', 'club-info-capacity', 'club-info-expansion', 'club-money-balance', 'club-money-transfer'], tabs: ['tab-club'] });
  ui.run("renderClubOverview({ club_id: 1079, club_name: 'Barcelona', stadium_name: 'Camp Nou', stadium_capacity: 62500, stadium_expansion_cap: 105000 })");
  assert.equal(ui.els['club-info-stadium'].innerText, 'Camp Nou');
  assert.equal(ui.els['club-info-capacity'].innerText, (62500).toLocaleString());
  assert.equal(ui.els['club-info-expansion'].innerText, (105000).toLocaleString());
  // The core now sends an empty name and 0 for an unreadable stadium instead of a placeholder.
  ui.run("renderClubOverview({ club_id: 1991, club_name: 'Espana Barcelona', stadium_name: '', stadium_capacity: 0, stadium_expansion_cap: 0 })");
  for (const id of ['club-info-stadium', 'club-info-capacity', 'club-info-expansion']) assert.equal(ui.els[id].innerText, 'Unavailable', id);
});
