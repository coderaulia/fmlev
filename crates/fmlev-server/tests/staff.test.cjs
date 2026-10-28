const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const vm = require('node:vm');
const { html } = require('./helpers/ui.cjs');
const { script } = require('./helpers/ui.cjs');

function ui(fetch, store = {}) {
  const els = {};
  const context = vm.createContext({
    els, localStorage: { getItem: key => (key in store ? store[key] : null), setItem(key, value) { store[key] = String(value); } }, fetch, console,
    setTimeout() { return 1; }, clearTimeout() {}, window: { addEventListener() {} },
    document: { addEventListener() {}, querySelectorAll: () => [], getElementById: id => els[id] || null },
  });
  vm.runInContext(script + `
    var messages = [];
    showToast = (message, type) => messages.push({ message, type });
    els["staff-query"] = { value: "guardiola" }; els["staff-results"] = { innerHTML: "" }; els["staff-detail"] = { innerHTML: "" }; els["staff-profile"] = { hidden: true }; els["staff-header"] = { innerHTML: "" };
  `, context);
  return { run: code => { const r = vm.runInContext(code, context); return r && typeof r === 'object' && typeof r.then !== 'function' ? JSON.parse(JSON.stringify(r)) : r; } };
}

test('staff editing is off by default and marked experimental', () => {
  const section = html.split('<section id="tab-staff"')[1].split('</section>')[0];
  assert.doesNotMatch(section, /type="number"|>Save|>Load|autosave/i);
  assert.match(section, /experimental/i);
  assert.match(section, /<input id="staff-experimental" type="checkbox" onchange="[^"]*">/, 'checkbox starts unchecked');
});

test('search lists unique staff and selecting reads by id and uid', async () => {
  const calls = [];
  const app = ui(async (url, request) => {
    calls.push({ url, body: JSON.parse(request.body) });
    if (url === '/api/staff/search') return { ok: true, json: async () => ({ staff: [{ index: 79, uid: 1120, name: 'Pep <b>Guardiola</b>' }] }), text: async () => '' };
    return { ok: true, text: async () => '', json: async () => ({
      staff: { index: 79, uid: 1120, name: 'Pep Guardiola' }, verification: 'Read-only.',
      attributes: [
        { group: 'coaching', field: 'attacking', value: 18, native: 89, reason: null },
        { group: 'medical', field: 'physiotherapy', value: null, native: null, reason: 'out of range 0' },
      ] }) };
  });
  await app.run('searchStaff()');
  assert.deepEqual(calls[0], { url: '/api/staff/search', body: { query: 'guardiola' } });
  assert.match(app.run('els["staff-results"].innerHTML'), /Pep &lt;b&gt;Guardiola/);
  await app.run('selectStaff(79, 1120)');
  assert.deepEqual(calls[1].body, { staff_id: 79, staff_uid: 1120 });
  const detail = app.run('els["staff-detail"].innerHTML');
  assert.match(detail, /Attacking<\/span><b class="attr-line-val band-16-20">18<\/b>/);
  assert.match(detail, /Outstanding/);
});

test('short queries and errors do not render data', async () => {
  const app = ui(async () => ({ ok: false, json: async () => ({ message: 'Attach to FM26 with a save loaded first' }), text: async () => '' }));
  app.run('els["staff-query"].value = "g"');
  await app.run('searchStaff()');
  assert.match(app.run('messages[0].message'), /2 letters/);
  await app.run('selectStaff(79, 1120)');
  assert.match(app.run('messages[1].message'), /Attach/);
  assert.equal(app.run('staffState.selected'), null);
});


function loaded(app, tab) {
  app.run(`staffState.selected = { staff: { index: 79, uid: 1120, name: 'Pep' }, verification: 'x', attributes: [
    { group: 'coaching', field: 'tactical', value: 18, native: 89, reason: null },
    { group: 'mental', field: 'authority', value: 15, native: null, reason: null },
    { group: 'medical', field: 'physiotherapy', value: null, native: null, reason: 'out of range 0' },
    { group: 'medical', field: 'sports_science', value: 7, native: null, reason: null } ] }; staffState.tab = '${tab || 'head_coach'}'`);
}

test('ability labels follow the measured FM26 bands', () => {
  const app = ui(async () => ({}));
  const labels = app.run('[1,3,4,6,7,9,10,11,12,14,15,17,18,19,20].map(staffAbilityLabel)');
  assert.deepEqual(labels, ['Unsuited','Unsuited','Reasonable','Reasonable','Competent','Competent','Average','Average','Good','Good','Very Good','Very Good','Outstanding','Outstanding','Elite']);
});

test('read-only view shows label and number, no inputs', () => {
  const app = ui(async () => ({}));
  loaded(app);
  app.run('renderStaffDetail(staffState.selected)');
  const out = app.run('els["staff-detail"].innerHTML');
  assert.doesNotMatch(out, /<input|Save staff/);
  assert.match(out, /Tactical<\/span><b class="attr-line-val band-16-20">18/);
  assert.match(out, /Outstanding/);
  assert.match(out, /Authority/);
});

test('ability tabs show their groups and unavailable fields stay visible with a reason', () => {
  const app = ui(async () => ({}));
  loaded(app, 'medical');
  app.run('renderStaffDetail(staffState.selected)');
  const out = app.run('els["staff-detail"].innerHTML');
  assert.match(out, /Physiotherapy<\/div>/);
  assert.match(out, /Sports Science<\/div>/);
  assert.match(out, /Unavailable[^]*out of range 0/);
  assert.doesNotMatch(out, /Tactical/);
});

test('enabling editing renders steppers except for unavailable fields', () => {
  const app = ui(async () => ({}));
  loaded(app);
  app.run('toggleStaffEditing(true)');
  const out = app.run('els["staff-detail"].innerHTML');
  assert.match(out, /id="staff-attr-tactical" type="number" min="1" max="20" value="18"/);
  assert.match(out, /class="stepper"/);
  assert.doesNotMatch(out, /Save staff attributes/);
  loaded(app, 'medical');
  app.run('renderStaffDetail(staffState.selected)');
  assert.doesNotMatch(app.run('els["staff-detail"].innerHTML'), /staff-attr-physiotherapy/);
});

test('editing only records changes; nothing is requested until save', () => {
  let calls = 0;
  const app = ui(async () => { calls++; return {}; });
  loaded(app);
  app.run('staffState.editing = true');
  app.run('staffSetEdit("tactical", "12")');
  assert.deepEqual(app.run('staffChanges()'), [{ label: 'Tactical', from: 18, to: 12 }]);
  app.run('staffSetEdit("tactical", "18")');
  assert.deepEqual(app.run('staffChanges()'), []);
  app.run('staffStep("tactical", 1)');
  assert.deepEqual(app.run('staffState.edits'), { tactical: 19 });
  assert.equal(calls, 0);
});

const UNVERIFIED = 'unverified: offset is a candidate from the task brief';
function extraFetch(calls, overrides = {}) {
  return async (url, request) => {
    calls.push({ url, body: JSON.parse(request.body) });
    const json = overrides[url] || (url === '/api/staff/information'
      ? { verification: 'Identity is validated.', fields: [
          { field: 'current_ability', label: 'Current ability', status: 'unavailable', value: null, reason: UNVERIFIED },
          { field: 'birth_year', label: 'Year of birth', status: 'verified', value: 1971, reason: null }] }
      : { contract: { status: 'unavailable', reason: 'staff contract link is not mapped', fields: [
          { field: 'weekly_wage', label: 'Wage', status: 'unavailable', value: null, reason: 'staff contract link is not mapped' }] } });
    return { ok: true, text: async () => '', json: async () => ({ status: 'ok', ...json }) };
  };
}

test('Information tab shows verified identity and unavailable candidates, with no inputs', async () => {
  const calls = [];
  const app = ui(extraFetch(calls));
  loaded(app);
  app.run('staffState.editing = true');
  await app.run('setStaffTab("information")');
  assert.deepEqual(calls[0], { url: '/api/staff/information', body: { staff_id: 79, staff_uid: 1120 } });
  const out = app.run('els["staff-detail"].innerHTML');
  assert.match(out, /Identity \(verified\)/);
  assert.match(out, /Unique ID<\/span><b class="attr-line-val">1120/);
  assert.match(out, /Current ability<\/span><span class="attr-line-val">Unavailable[^]*unverified: offset is a candidate/);
  assert.match(out, /Year of birth<\/span><b class="attr-line-val">1971/);
  assert.doesNotMatch(out, /<input|stepper|Tactical/);
  await app.run('setStaffTab("information")');
  assert.equal(calls.length, 1, 'loaded once per selected staff member');
});

test('Contract tab is read-only and every field is unavailable with the reason', async () => {
  const calls = [];
  const app = ui(extraFetch(calls));
  loaded(app);
  await app.run('setStaffTab("contract")');
  assert.equal(calls[0].url, '/api/staff/contract');
  const out = app.run('els["staff-detail"].innerHTML');
  assert.match(out, /Wage<\/span><span class="attr-line-val">Unavailable[^]*not mapped/);
  assert.match(out, /Read-only/);
  assert.doesNotMatch(out, /<input|<b class="attr-line-val">\d/);
});

test('Information errors are shown as unavailable and a new selection drops cached tab data', async () => {
  const app = ui(async () => ({ ok: false, json: async () => ({ message: 'Attach to FM26 with a save loaded first' }), text: async () => '' }));
  loaded(app);
  await app.run('setStaffTab("information")');
  assert.match(app.run('els["staff-detail"].innerHTML'), /Unavailable[^]*Attach to FM26/);
  const calls = [];
  const app2 = ui(extraFetch(calls, { '/api/staff/native': {} }));
  loaded(app2);
  await app2.run('setStaffTab("contract")');
  app2.run('staffState.tab = "head_coach"');
  assert.equal(app2.run('Object.keys(staffState.extra)').length, 1);
  app2.run('staffState.selected = { staff: { index: 80, uid: 5, name: "Other" }, attributes: [] }; staffState.extra = {}');
  await app2.run('setStaffTab("contract")');
  assert.deepEqual(calls[1].body, { staff_id: 80, staff_uid: 5 });
});

test('staff page registers with the save bar and offers Load', () => {
  const app = ui(async () => ({}));
  assert.equal(app.run('typeof staffChanges'), 'function');
});

test('save sends only changed fields with the experimental opt-in and identity', async () => {
  let body;
  const app = ui(async (_url, request) => { body = JSON.parse(request.body); return { ok: true, text: async () => '', json: async () => ({ message: '1 staff attribute(s) written', attributes: [{ group: 'coaching', field: 'tactical', value: 12, native: 1, reason: null }] }) }; });
  loaded(app);
  app.run('staffState.editing = true; staffState.edits = { tactical: 12, authority: 15 }');
  assert.equal(await app.run('saveStaffAttributes()'), true);
  assert.deepEqual(body, { staff_id: 79, staff_uid: 1120, attributes: { tactical: 12 }, experimental: true });
  assert.deepEqual(app.run('staffState.edits'), {});
  assert.equal(app.run('staffState.selected.attributes[0].value'), 12);
});

test('save refuses out-of-range input, no-op edits and when editing is off', async () => {
  let calls = 0;
  const app = ui(async () => { calls++; return { ok: true, json: async () => ({}), text: async () => '' }; });
  loaded(app);
  app.run('staffState.edits = { tactical: 25 }');
  assert.equal(await app.run('saveStaffAttributes()'), false, 'editing off');
  app.run('staffState.editing = true');
  assert.equal(await app.run('saveStaffAttributes()'), false);
  assert.match(app.run('messages.at(-1).message'), /1-20/);
  app.run('staffState.edits = { tactical: 18 }');
  assert.equal(await app.run('saveStaffAttributes()'), false);
  assert.match(app.run('messages.at(-1).message'), /Nothing changed/);
  assert.equal(calls, 0);
});

test('turning editing off discards pending edits', () => {
  const app = ui(async () => ({}));
  loaded(app);
  app.run('staffState.editing = true; staffState.edits = { tactical: 12 }; toggleStaffEditing(false)');
  assert.deepEqual(app.run('staffState.edits'), {});
});

test('server rejection keeps the loaded values and the pending edits', async () => {
  const app = ui(async () => ({ ok: false, json: async () => ({ message: 'Transaction failed and was rolled back cleanly' }), text: async () => '' }));
  loaded(app);
  app.run('staffState.editing = true; staffState.edits = { tactical: 12 }');
  assert.equal(await app.run('saveStaffAttributes()'), false);
  assert.match(app.run('messages.at(-1).message'), /rolled back/);
  assert.equal(app.run('staffState.selected.attributes[0].value'), 18);
  assert.deepEqual(app.run('staffState.edits'), { tactical: 12 });
});

const NO_OPEN_PROFILE = { ok: false, status: 404, text: async () => '', json: async () => ({ message: 'No staff profile is open in FM26' }) };

test('load re-reads the selected member and drops edits', async () => {
  const calls = [];
  const app = ui(async (url, request) => {
    if (url === '/api/staff/open') return NO_OPEN_PROFILE;
    calls.push(JSON.parse(request.body));
    return { ok: true, text: async () => '', json: async () => ({ staff: { index: 79, uid: 1120, name: 'Pep' }, verification: '', attributes: [{ group: 'coaching', field: 'tactical', value: 18, native: 1, reason: null }] }) };
  });
  loaded(app);
  app.run('staffState.edits = { tactical: 12 }');
  assert.equal(await app.run('loadStaffFromGame()'), true);
  assert.deepEqual(calls[0], { staff_id: 79, staff_uid: 1120 });
  assert.deepEqual(app.run('staffState.edits'), {});
});

test('staff edits made while saving remain pending', async () => {
  let finish;
  const app = ui(() => new Promise(resolve => { finish = resolve; }));
  loaded(app);
  app.run('staffState.editing = true; staffState.edits = {tactical: 12}');
  const pending = app.run('saveStaffAttributes()');
  app.run('staffState.edits.tactical = 15');
  finish({ok: true, json: async () => ({attributes: [{group: 'coaching', field: 'tactical', value: 12}]})});
  assert.equal(await pending, false);
  assert.deepEqual(app.run('staffState.edits'), {tactical: 15});
});

test('a staff save response cannot replace a different selected staff member', async () => {
  let finish;
  const app = ui(() => new Promise(resolve => { finish = resolve; }));
  loaded(app);
  app.run('staffState.editing = true; staffState.edits = {tactical: 12}');
  const pending = app.run('saveStaffAttributes()');
  app.run('staffState.selected = {staff:{index:80,uid:1121,name:"Other"},attributes:[]}');
  finish({ok: true, json: async () => ({attributes: []})});
  assert.equal(await pending, false);
  assert.equal(app.run('staffState.selected.staff.index'), 80);
});

test('search shows the club and remembers opened staff; Load reopens the last one', async () => {
  const store = {};
  const calls = [];
  const fetch = async (url, request) => {
    calls.push({ url, body: JSON.parse(request.body) });
    if (url === '/api/staff/open') return NO_OPEN_PROFILE;
    if (url === '/api/staff/search') {
      return { ok: true, text: async () => '', json: async () => ({ staff: [{ index: 4460, uid: 4001686, name: 'Antonio Pintus' }], clubs: { 4460: 'R. Madrid' } }) };
    }
    return { ok: true, text: async () => '', json: async () => ({ staff: { index: 4460, uid: 4001686, name: 'Antonio Pintus' }, attributes: [] }) };
  };
  const app = ui(fetch, store);
  await app.run('searchStaff()');
  const table = app.run('els["staff-results"].innerHTML');
  assert.match(table, /<th>Club<\/th>/);
  assert.match(table, /Antonio Pintus<\/td><td>R\. Madrid<\/td>/);
  await app.run('selectStaff(4460, 4001686)');
  assert.deepEqual(JSON.parse(store['fmle.staff.recent']), [{ index: 4460, uid: 4001686, name: 'Antonio Pintus', club: 'R. Madrid' }]);

  // A fresh page with nothing selected lists the recent staff and Load reopens the last one.
  const calls2 = [];
  const app2 = ui(async (url, request) => { calls2.push({ url, body: JSON.parse(request.body) }); return fetch(url, request); }, store);
  app2.run('showStaffRecents()');
  assert.match(app2.run('els["staff-results"].innerHTML'), /Recently opened[^]*Antonio Pintus<\/td><td>R\. Madrid/);
  assert.equal(await app2.run('loadStaffFromGame()'), true);
  assert.deepEqual(calls2.map(c => c.url), ['/api/staff/open', '/api/staff/native']);
  assert.deepEqual(calls2[1], { url: '/api/staff/native', body: { staff_id: 4460, staff_uid: 4001686 } });
});

test('Load with nothing selected and no recents tells the user to search', async () => {
  const app = ui(async url => {
    if (url === '/api/staff/open') return NO_OPEN_PROFILE;
    throw new Error('no other request expected');
  });
  assert.equal(await app.run('loadStaffFromGame()'), false);
  assert.match(app.run('messages[0].message'), /name or club/);
});

test('recents ignore corrupt or unavailable storage', () => {
  const app = ui(async () => ({}), { 'fmle.staff.recent': '{not json' });
  assert.deepEqual(app.run('staffRecent()'), []);
  const app2 = ui(async () => ({}), { 'fmle.staff.recent': JSON.stringify([{ index: 1, uid: 2, name: 'Ok' }, { index: 'x' }]) });
  assert.equal(app2.run('staffRecent().length'), 1);
});

test('Load opens the staff profile that is open in the game, even when another is selected', async () => {
  const calls = [];
  const store = {};
  const app = ui(async (url, request) => {
    calls.push({ url, body: JSON.parse(request.body) });
    if (url === '/api/staff/open') return { ok: true, status: 200, text: async () => '', json: async () => ({ status: 'ok', staff: { index: 20414, uid: 67055850, name: 'Iván Ortega' } }) };
    return { ok: true, text: async () => '', json: async () => ({ staff: { index: 20414, uid: 67055850, name: 'Iván Ortega' }, attributes: [] }) };
  }, store);
  loaded(app);
  assert.equal(app.run('staffState.selected.staff.index'), 79);
  assert.equal(await app.run('loadStaffFromGame()'), true);
  assert.deepEqual(calls.map(c => c.url), ['/api/staff/open', '/api/staff/native']);
  assert.deepEqual(calls[1].body, { staff_id: 20414, staff_uid: 67055850 });
  assert.equal(app.run('staffState.selected.staff.name'), 'Iván Ortega');
});

test('Open in game says so when no staff profile is open, and errors are surfaced', async () => {
  const app = ui(async () => NO_OPEN_PROFILE);
  assert.equal(await app.run('openStaffFromGame()'), null);
  assert.match(app.run('messages[0].message'), /No staff profile is open/);
  assert.match(html.split('<section id="tab-staff"')[1], /onclick="openStaffFromGame\(\)"/);
  const failing = ui(async () => ({ ok: false, status: 500, text: async () => '', json: async () => ({ message: 'boom' }) }));
  assert.equal(await failing.run('openStaffFromGame()'), null);
  assert.equal(failing.run('messages[0].type'), 'error');
});
