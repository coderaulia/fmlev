const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const vm = require('node:vm');
const script = readFileSync(require('node:path').join(__dirname, '../src/ui.html'), 'utf8').split('<script>')[1].split('</script>')[0];

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
    state.activePlayer = { id: 55161 };
    els["elig-comp-index"] = { value: "51" }; els["elig-comp-uid"] = { value: "67" };
    els["elig-message"] = { value: "Work permit missing" }; els["elig-results"] = { innerHTML: "" };
  `, context);
  return { run: code => vm.runInContext(code, context) };
}

const reply = {
  status: 'ok', player_note: 'Player-specific gates are unknown.',
  results: [
    { id: 'registration_window', title: 'Registration window', status: 'blocked', evidence: 'closed', fix: 'Turn on Register Anytime' },
    { id: 'work_permit', title: 'Work permit', status: 'unknown', evidence: 'not mapped', fix: null },
  ],
  support_report: { fmlev_version: '0.1.0', player_id: 55161 },
};

test('check sends only identity selectors and renders unknown gates as unknown', async () => {
  let body;
  const app = ui(async (_url, request) => { body = JSON.parse(request.body); return { ok: true, json: async () => reply, text: async () => '' }; });
  await app.run('checkEligibility()');
  assert.deepEqual(body, { player_id: 55161, comp_index: 51, comp_uid: 67, game_message: 'Work permit missing' });
  const html = app.run('els["elig-results"].innerHTML');
  assert.match(html, /⛔.*Registration window/);
  assert.match(html, /❔.*Work permit/);
  assert.doesNotMatch(html, /✅/);
});

test('invalid competition ids and server errors never produce a report', async () => {
  const app = ui(async () => ({ ok: false, json: async () => ({ message: 'Attach to FM26 first' }), text: async () => '' }));
  app.run('els["elig-comp-index"].value = ""');
  await app.run('checkEligibility()');
  assert.match(app.run('messages[0].message'), /competition index/);
  app.run('els["elig-comp-index"].value = "51"');
  await app.run('checkEligibility()');
  assert.match(app.run('messages[1].message'), /Attach/);
  assert.equal(app.run('eligibilityState.report'), null);
});
