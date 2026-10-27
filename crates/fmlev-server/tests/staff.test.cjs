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

test('staff tab is read-only: no inputs or save controls for attributes', () => {
  const section = html.split('<section id="tab-staff"')[1].split('</section>')[0];
  assert.doesNotMatch(section, /type="number"|Save|onchange/);
  assert.match(section, /Read-only/);
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
