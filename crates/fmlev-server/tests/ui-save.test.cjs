const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const vm = require('node:vm');
const script = readFileSync(require('node:path').join(__dirname, '../src/ui.html'), 'utf8').split('<script>')[1].split('</script>')[0];

function ui(fetch) {
  const eventHandlers = {};
  const scheduledCallbacks = [];
  const context = vm.createContext({
    eventHandlers, scheduledCallbacks,
    localStorage: { getItem: () => '{"autoApply":true,"autosave":true}', setItem() {} },
    fetch, console, setTimeout: callback => { scheduledCallbacks.push(callback); return scheduledCallbacks.length; }, clearTimeout() {},
    window: { addEventListener() {} },
    document: { addEventListener(name, callback) { eventHandlers[name] = callback; }, getElementById: () => null },
  });
  vm.runInContext(script + `
    const messages = [];
    showToast = (message, type) => messages.push({ message, type });
    renderPositionsTab = () => {};
    displayPlayer = player => { state.activePlayer = JSON.parse(JSON.stringify(player)); };
    state.activePlayer = { id: 73, name: "Zulkifli", attributes: {}, positions: { dl: 15, left_foot: 15 } };
    state.playerDirtyFields.add("positions");
    state.playerDirty = true;
  `, context);
  return { run: code => vm.runInContext(code, context) };
}

test('failed position saves preserve dirty fields and never announce success', async () => {
  const app = ui(async () => ({ ok: false, json: async () => ({ message: 'Native binding unavailable' }) }));
  assert.equal(await app.run('savePositionsToGame()'), false);
  assert.equal(app.run('state.playerDirtyFields.has("positions")'), true);
  assert.equal(app.run('messages.some(message => message.type === "success")'), false);
});

test('successful saves display the acknowledged player', async () => {
  const app = ui(async () => ({ ok: true, json: async () => ({ player: { id: 73, name: 'Acknowledged', positions: { dl: 14 } } }) }));
  assert.equal(await app.run('applyPlayerChanges()'), true);
  assert.equal(app.run('state.activePlayer.name'), 'Acknowledged');
  assert.equal(app.run('state.playerDirtyFields.size'), 0);
});

test('an older save response does not clear newer edits', async () => {
  let finish;
  const app = ui(() => new Promise(resolve => { finish = resolve; }));
  const pending = app.run('applyPlayerChanges()');
  app.run('state.activePlayer.positions.dl = 16; state.playerDirtyFields.add("positions")');
  finish({ ok: true, json: async () => ({ player: { id: 73, positions: { dl: 15 } } }) });
  await pending;
  assert.equal(app.run('state.activePlayer.positions.dl'), 16);
  assert.equal(app.run('state.playerDirtyFields.has("positions")'), true);
});

test('zero values survive numeric input conversion', () => {
  const app = ui(() => {});
  assert.equal(app.run('numberInput("0", 25)'), 0);
  assert.equal(app.run('numberInput("", 25)'), 25);
});

test('compound presets stop after the first failure', async () => {
  const app = ui(() => {});
  app.run('let calls = 0; applyPlayerPreset = async () => { calls++; return false; }; applyBothFeetGodMode = async () => { calls++; return true; }');
  assert.equal(await app.run('applyGodModeAll()'), false);
  assert.equal(app.run('calls'), 1);
  assert.equal(app.run('messages.some(message => message.type === "success")'), false);
});

test('manual entity edits stay dirty after focus leaves an input', () => {
  const app = ui(() => {});
  app.run('eventHandlers.input({ target: { closest: selector => selector === "#tab-nation" } })');
  assert.equal(app.run('state.nationDirty'), true);
  app.run('eventHandlers.change({ target: { closest: selector => selector === "#tab-competition" } })');
  assert.equal(app.run('state.competitionDirty'), true);
});

test('missing foot ratings display as unknown instead of invented strength', () => {
  const app = ui(() => {});
  assert.equal(app.run('getFootSummaryString(0, 0)'), 'Foot ratings unavailable');
  assert.equal(app.run('getFootTierDesc(0)'), 'Unknown (not exposed)');
});

test('editing never auto-saves even with stored autosave settings or a legacy toggle', async () => {
  let requests = 0;
  const app = ui(async (_url, request) => {
    requests++;
    return { ok: true, json: async () => ({ player: JSON.parse(request.body) }) };
  });
  assert.equal(app.run('state.autoApply'), false);
  app.run('toggleAutoApply(true)');
  assert.equal(app.run('state.autoApply'), false);
  app.run(`
    stepPosition("dl", 1);
    handleFootSlider("left", 16);
    stepAttr("finishing", 1);
    handleCAChange("180");
    autoSaveContract("wage_per_week");
    autoSaveClub();
    debouncedSavePlayer();
    debouncedSaveNation();
    for (const callback of scheduledCallbacks.splice(0)) callback();
  `);
  assert.equal(requests, 0);
  assert.equal(app.run('state.playerDirtyFields.has("positions")'), true);
  assert.equal(await app.run('applyPlayerChanges()'), true);
  assert.equal(requests, 1);
});

test('preset buttons only stage requests and Save writes edits before presets', async () => {
  const requests = [];
  const app = ui(async (url, request) => {
    requests.push({ url, body: JSON.parse(request.body) });
    return { ok: true, json: async () => ({ status: 'success' }) };
  });
  assert.equal(await app.run('applyPlayerPreset("goat")'), true);
  assert.equal(await app.run('applyPositionPreset("both_feet_20")'), true);
  assert.equal(requests.length, 0);
  assert.equal(await app.run('applyPlayerChanges()'), true);
  assert.deepEqual(requests.map(request => request.url), [
    '/api/player/update', '/api/player/preset', '/api/player/preset'
  ]);
  assert.equal(requests[1].body.player_id, 73);
  assert.equal(app.run('pendingActions.size'), 0);
});

test('pending presets survive switching profiles and never apply to another player', async () => {
  const requests = [];
  const app = ui(async (url, request) => {
    requests.push({ url, body: JSON.parse(request.body) });
    return { ok: true, json: async () => ({ status: 'success' }) };
  });
  app.run('state.playerDirtyFields.clear()');
  await app.run('applyPlayerPreset("goat")');
  app.run('const originalPlayer = state.activePlayer; state.activePlayer = { id: 74, name: "Other player" }');
  await app.run('applyPlayerChanges()');
  assert.equal(requests.length, 0);
  app.run('state.activePlayer = originalPlayer');
  await app.run('applyPlayerChanges()');
  assert.equal(requests.length, 1);
  assert.equal(requests[0].body.player_id, 73);
});

test('failed pending actions remain queued for an explicit retry', async () => {
  let succeed = false;
  let requests = 0;
  const app = ui(async () => {
    requests++;
    return { ok: succeed, json: async () => ({ message: 'No native binding' }) };
  });
  app.run('state.playerDirtyFields.clear()');
  await app.run('applyPlayerPreset("goat")');
  assert.equal(await app.run('applyPlayerChanges()'), false);
  assert.equal(requests, 1);
  assert.equal(app.run('pendingActions.get("player:73").length'), 1);
  succeed = true;
  assert.equal(await app.run('applyPlayerChanges()'), true);
  assert.equal(requests, 2);
  assert.equal(app.run('pendingActions.size'), 0);
});

test('club nation competition and affiliate actions all require their Save button', async () => {
  const requests = [];
  const app = ui(async (url, request) => {
    requests.push({ url, body: JSON.parse(request.body) });
    return { ok: true, json: async () => ({ status: 'success' }) };
  });
  app.run('state.activeClub = { club_id: 1001 }; state.activeNation = { id: 765 }; state.activeCompetition = { id: 11 }');
  await app.run('applyClubPreset("heal_squad")');
  await app.run('removeAffiliate(8888)');
  await app.run('applyNationPreset("golden_generation")');
  await app.run('applyCompetitionPreset("relax_rules")');
  assert.equal(requests.length, 0);
  await app.run('saveClubToGame()');
  assert.equal(requests.length, 2);
  assert.equal(requests[1].body.club_id, 1001);
  await app.run('saveNationToGame()');
  await app.run('saveCompetitionToGame()');
  assert.equal(requests.length, 4);
  assert.equal(requests[2].body.nation_id, 765);
  assert.equal(requests[3].body.competition_id, 11);
});

test('every player Save includes edited contract fields and preserves untouched fields', async () => {
  let submitted;
  const app = ui(async (_url, request) => {
    submitted = JSON.parse(request.body);
    return { ok: true, json: async () => ({ status: 'success' }) };
  });
  app.run(`
    state.activePlayer.contract = { wage_per_week: 1000, expiry_year: 2030 };
    document.getElementById = id => id === "contract-wage" ? { value: "0" } : null;
    autoSaveContract("wage_per_week");
  `);
  assert.equal(submitted, undefined);
  assert.equal(await app.run('applyPlayerChanges()'), true);
  assert.equal(submitted.contract.wage_per_week, 0);
  assert.equal(submitted.contract.expiry_year, 2030);
});


test('passing Save explicitly declares only user edits', async () => {
  let submitted;
  const app = ui(async (_url, request) => {
    submitted = JSON.parse(request.body);
    return { ok: true, json: async () => ({ status: 'success' }) };
  });
  app.run(`state.playerDirtyFields.clear(); state.activePlayer.attributes.passing = 15; state.playerDirtyFields.add("attr_passing")`);
  assert.equal(await app.run('applyPlayerChanges()'), true);
  assert.deepEqual(submitted.changed_fields, ['attributes.passing']);
  assert.equal(submitted.attributes.passing, 15);
});

test('attribute recommendations never silently mutate CA', () => {
  const app = ui(() => {});
  app.run('state.activePlayer.current_ability = 90; state.activePlayer.attributes.passing = 14');
  app.run('updateRecommendedCA()');
  assert.equal(app.run('state.activePlayer.current_ability'), 90);
  assert.equal(app.run('state.playerDirtyFields.has("current_ability")'), false);
});


test('all sixty native attributes have editing controls', () => {
  const app = ui(() => {});
  assert.equal(app.run('allAttrKeys.length'), 60);
  assert.equal(app.run('new Set(allAttrKeys).size'), 60);
  for (const key of ['marking', 'long_throws', 'punching', 'dirtiness', 'controversy', 'adaptability', 'sportsmanship', 'versatility']) {
    assert.equal(app.run(`allAttrKeys.includes(${JSON.stringify(key)})`), true);
  }
});
