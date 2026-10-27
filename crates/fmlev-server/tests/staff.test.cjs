const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const vm = require('node:vm');
const html = readFileSync(require('node:path').join(__dirname, '../src/ui.html'), 'utf8');
const script = html.split('<script>')[1].split('</script>')[0];

function ui(fetch) {
  const els = {};
  const context = vm.createContext({
    els, localStorage: { getItem: () => null, setItem() {} }, fetch, console,
    setTimeout() { return 1; }, clearTimeout() {}, window: { addEventListener() {} },
    document: { addEventListener() {}, querySelectorAll: () => [], getElementById: id => els[id] || null },
  });
  vm.runInContext(script + `
    var messages = [];
    showToast = (message, type) => messages.push({ message, type });
    els["staff-query"] = { value: "guardiola" }; els["staff-results"] = { innerHTML: "" }; els["staff-detail"] = { innerHTML: "" };
  `, context);
  return { run: code => vm.runInContext(code, context) };
}

test('staff editing is off by default and marked experimental', () => {
  const section = html.split('<section id="tab-staff"')[1].split('</section>')[0];
  assert.doesNotMatch(section, /type="number"|>Save/);
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
  assert.match(detail, /Attacking<\/span><b>18<\/b>/);
  assert.match(detail, /Physiotherapy<\/span><span[^>]*>Unavailable/);
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

function loaded(app) {
  app.run(`staffState.selected = { staff: { index: 79, uid: 1120, name: 'Pep' }, verification: 'x', attributes: [
    { group: 'coaching', field: 'tactical', value: 18, native: 89, reason: null },
    { group: 'mental', field: 'authority', value: 15, native: null, reason: null },
    { group: 'medical', field: 'physiotherapy', value: null, native: null, reason: 'out of range 0' } ] }`);
}

test('read-only view has no inputs; enabling editing renders inputs except for unavailable fields', () => {
  const app = ui(async () => ({}));
  loaded(app);
  app.run('renderStaffDetail(staffState.selected)');
  assert.doesNotMatch(app.run('els["staff-detail"].innerHTML'), /<input|Save staff/);
  app.run('toggleStaffEditing(true)');
  const html = app.run('els["staff-detail"].innerHTML');
  assert.match(html, /id="staff-attr-tactical" type="number" min="1" max="20" value="18"/);
  assert.doesNotMatch(html, /staff-attr-physiotherapy/);
  assert.match(html, /Save staff attributes \(experimental\)/);
});

test('save sends only changed fields with the experimental opt-in and identity', async () => {
  let body;
  const app = ui(async (_url, request) => { body = JSON.parse(request.body); return { ok: true, text: async () => '', json: async () => ({ message: '1 staff attribute(s) written', attributes: null }) }; });
  loaded(app);
  app.run('staffState.editing = true');
  app.run('els["staff-attr-tactical"] = { value: "12" }; els["staff-attr-authority"] = { value: "15" }');
  assert.equal(await app.run('saveStaffAttributes()'), true);
  assert.deepEqual(body, { staff_id: 79, staff_uid: 1120, attributes: { tactical: 12 }, experimental: true });
});

test('save refuses out-of-range input, no-op edits and when editing is off', async () => {
  let calls = 0;
  const app = ui(async () => { calls++; return { ok: true, json: async () => ({}), text: async () => '' }; });
  loaded(app);
  app.run('els["staff-attr-tactical"] = { value: "25" }; els["staff-attr-authority"] = { value: "15" }');
  assert.equal(await app.run('saveStaffAttributes()'), false, 'editing off');
  app.run('staffState.editing = true');
  assert.equal(await app.run('saveStaffAttributes()'), false);
  assert.match(app.run('messages.at(-1).message'), /1–20/);
  app.run('els["staff-attr-tactical"].value = "18"');
  assert.equal(await app.run('saveStaffAttributes()'), false);
  assert.match(app.run('messages.at(-1).message'), /Nothing changed/);
  assert.equal(calls, 0);
});

test('server rejection keeps the loaded values', async () => {
  const app = ui(async () => ({ ok: false, json: async () => ({ message: 'Transaction failed and was rolled back cleanly' }), text: async () => '' }));
  loaded(app);
  app.run('staffState.editing = true; els["staff-attr-tactical"] = { value: "12" }; els["staff-attr-authority"] = { value: "15" }');
  assert.equal(await app.run('saveStaffAttributes()'), false);
  assert.match(app.run('messages.at(-1).message'), /rolled back/);
  assert.equal(app.run('staffState.selected.attributes[0].value'), 18);
});
