const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const vm = require('node:vm');
const script = readFileSync(require('node:path').join(__dirname, '../src/ui.html'), 'utf8').split('<script>')[1].split('</script>')[0];

function ui(fetch) {
  const els = {};
  const context = vm.createContext({
    els,
    localStorage: { getItem: () => null, setItem() {} }, fetch, console,
    setTimeout() { return 1; }, clearTimeout() {},
    window: { addEventListener() {} },
    document: { addEventListener() {}, querySelectorAll: () => [], getElementById: id => els[id] || null },
  });
  vm.runInContext(script + `
    var messages = [];
    showToast = (message, type) => messages.push({ message, type });
    renderPositionsTab = () => {};
    updateRecommendedCA = () => {};
    updateAttrDisplay = () => {};
    state.activePlayer = { id: 73, name: "Test", attributes: { passing: 10, vision: 9 }, positions: { dm: 1, mc: 20 }, current_ability: 100, potential_ability: 120 };
    state.playerDirtyFields.clear();
    els["role-select"] = { value: "deep_lying_playmaker" };
    els["role-ca-mode"] = { value: "" };
    els["role-diff"] = { innerHTML: "" };
    els["role-stage-btn"] = { style: {} };
  `, context);
  return { run: code => vm.runInContext(code, context) };
}

const proposal = {
  status: 'ok', player_id: 73,
  proposal: {
    role_id: 'deep_lying_playmaker', role_name: 'Deep-Lying Playmaker', level: 3, ca_mode: 'raise_to_fit',
    role_data_verified: false, ca_is_heuristic: true,
    changes: [
      { field: 'positions.dm', old: 1, new: 20 },
      { field: 'attributes.passing', old: 10, new: 18 },
      { field: 'current_ability', old: 100, new: 112 },
      { field: 'potential_ability', old: 120, new: 120 + 0 },
    ].filter(c => c.new !== c.old),
  },
};

test('preview does not mutate the player; staging marks only the proposed fields dirty', async () => {
  let requested;
  const app = ui(async (_url, request) => { requested = JSON.parse(request.body); return { ok: true, json: async () => proposal, text: async () => '' }; });
  await app.run('previewRolePreset(3)');
  assert.deepEqual(requested, { player_id: 73, role_id: 'deep_lying_playmaker', level: 3 });
  assert.equal(app.run('state.activePlayer.attributes.passing'), 10, 'preview alone changes nothing');
  assert.equal(app.run('state.playerDirtyFields.size'), 0);
  assert.equal(app.run('els["role-stage-btn"].style.display'), '');

  assert.equal(app.run('stageRoleProposal()'), true);
  assert.equal(app.run('state.activePlayer.attributes.passing'), 18);
  assert.equal(app.run('state.activePlayer.positions.dm'), 20);
  assert.equal(app.run('state.activePlayer.positions.mc'), 20, 'untouched position stays');
  assert.equal(app.run('state.activePlayer.attributes.vision'), 9, 'untouched attribute stays');
  assert.equal(app.run('state.activePlayer.current_ability'), 112);
  assert.deepEqual(Array.from(app.run('Array.from(state.playerDirtyFields).sort()')), ['attr_passing', 'current_ability', 'positions']);
});

test('staging is refused when another player is open', async () => {
  const app = ui(async () => ({ ok: true, json: async () => proposal, text: async () => '' }));
  await app.run('previewRolePreset(3)');
  app.run('state.activePlayer = { id: 99, attributes: { passing: 5 }, positions: {} }');
  assert.equal(app.run('stageRoleProposal()'), false);
  assert.equal(app.run('state.activePlayer.attributes.passing'), 5);
  assert.equal(app.run('state.playerDirtyFields.size'), 0);
});

test('server rejection is shown and nothing is staged', async () => {
  const app = ui(async () => ({ ok: false, json: async () => ({ message: 'attributes.finishing has not been read from the game' }), text: async () => '' }));
  await app.run('previewRolePreset(2)');
  assert.match(app.run('messages[0].message'), /not been read/);
  assert.equal(app.run('roleState.proposal'), null);
});
