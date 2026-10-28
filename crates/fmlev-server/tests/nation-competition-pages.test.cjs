const { test } = require("node:test");
const assert = require("node:assert/strict");
const { createUi } = require("./helpers/fake-dom.cjs");
const { read } = require("./helpers/ui.cjs");
const ok = data => ({ ok: true, json: async () => data });
const nation = { id: 4, name: "Test Nation", youth_rating: 145, reputation: 8000, economic_factor: 15, state_of_development: 0, population: 123, tax_rate: 0 };
const competition = { id: 8, name: "Test League", reputation: 8000, max_foreign_players: 0, winner_prize_money: 0 };
const ids = ["nation-youth-slider", "nation-youth-val", "nation-reputation", "nation-economic", "nation-development", "nation-population", "disp-nation-econ",
  "comp-reputation", "prize-teams", "prize-first", "prize-last", "prize-tol", "prize-list", "prize-load-status", "savebar", "savebar-status", "savebar-error"];
function app(fetch) {
  const ui = createUi({ ids, fetch: fetch || (async url => url === "/api/nation/native" ? ok({ nation }) : url === "/api/competition/native" ? ok({ competition }) : url === "/api/competition/prizes" ? ok({ prizes: [100, 50] }) : ok({})) });
  ui.run("displayEntityAvailability = () => true; state.connected = true; state.activeNation = " + JSON.stringify(nation) + "; state.activeCompetition = " + JSON.stringify(competition) + "; Shell.init()");
  return ui;
}
function criteria(ui) {
  for (const [id, value] of [["prize-teams", "2"], ["prize-first", "100"], ["prize-last", "50"], ["prize-tol", "0"]]) ui.els[id].value = value;
}

test("Nation and Competition have no per-card Load or Save buttons", () => {
  for (const page of ["nation", "competition"]) assert.doesNotMatch(read(`pages/${page}.html`), /onclick="(?:saveNationToGame|saveCompetitionToGame|loadPrizeTable|savePrizeTable)\(/);
  assert.match(read("pages/nation.html"), /id="nation-population" disabled data-unverified="true"/);
  assert.match(read("pages/competition.html"), /onclick="stageCompetitionRegistration\(true\)"/);
});

test("Nation preserves untouched zero values and only sends a request on explicit Save", async () => {
  const ui = app();
  ui.run("switchTab('tab-nation')");
  assert.equal(await ui.run("SaveBar.load()"), true);
  assert.equal(Number(ui.els["nation-development"].value), 0);
  assert.equal(ui.els["nation-population"].disabled, true);
  ui.els["nation-youth-slider"].value = "150";
  ui.run("handleYouthRatingInput(150)");
  assert.equal(ui.requests.length, 1);
  assert.deepEqual(ui.run("nationFieldChanges()"), [{ label: "Youth rating", field: "youth_rating", from: 145, to: "150" }]);
  assert.equal(await ui.run("SaveBar.save()"), true);
  assert.deepEqual(ui.requests[1].body, { ...nation, youth_rating: 150 });
  assert.equal(ui.run("nationFieldChanges().length"), 0);
});

test("invalid Nation edits never write or dispatch queued actions", async () => {
  const ui = app();
  await ui.run("loadNationPage()");
  ui.run("switchTab('tab-nation'); queuePreset('nation', 'golden_generation')");
  ui.els["nation-youth-slider"].value = "";
  ui.run("markNationDirty()");
  assert.equal(await ui.run("SaveBar.save()"), false);
  assert.equal(ui.requests.length, 1);
  assert.equal(ui.run("pendingActions.get('nation:4').length"), 1);
});

test("Nation Discard restores baseline and clears queued actions without a game write", async () => {
  const ui = app();
  await ui.run("loadNationPage()");
  ui.els["nation-reputation"].value = "9000";
  ui.run("markNationDirty(); queuePreset('nation', 'golden_generation'); switchTab('tab-nation')");
  assert.equal(await ui.run("SaveBar.discard()"), true);
  assert.equal(Number(ui.els["nation-reputation"].value), 8000);
  assert.equal(ui.run("pendingActions.has('nation:4')"), false);
  assert.equal(ui.requests.some(r => r.url === "/api/nation/update"), false);
});

test("late Nation native reads cannot populate a different entity", async () => {
  let release;
  const ui = app(() => new Promise(resolve => { release = resolve; }));
  const pending = ui.run("syncNationNative(4)");
  ui.run("state.activeNation = { id: 5, name: 'Other' }");
  release(ok({ nation }));
  assert.equal(await pending, false);
  assert.equal(ui.run("nationBaseline"), null);
});

test("Competition registration only stages an identity-bound request until Save", async () => {
  const ui = app();
  ui.run("switchTab('tab-competition'); state.liveCompetitions = {8: {unique_id: 88}}; stageCompetitionRegistration(true); stageCompetitionRegistration(false)");
  assert.equal(ui.requests.length, 0);
  assert.equal(ui.run("pendingActions.get('competition:8').length"), 1);
  assert.equal(await ui.run("SaveBar.save()"), true);
  assert.deepEqual(ui.requests.map(r => r.body), [{ comp_index: 8, comp_uid: 88, enabled: false }]);
});

test("reputation-only Save preserves unedited Competition fields and skips prize requests", async () => {
  const ui = app();
  ui.run("switchTab('tab-competition')");
  await ui.run("SaveBar.load()");
  ui.els["comp-reputation"].value = "8500";
  ui.run("handleCompRepChange()");
  assert.equal(await ui.run("SaveBar.save()"), true);
  assert.deepEqual(ui.requests[1].body, { ...competition, reputation: 8500 });
  assert.equal(ui.requests.some(r => r.url === "/api/competition/prizes"), false);
});

test("one Load reads reputation and prize table; prize-only Save skips reputation update", async () => {
  const ui = app(async (url, init) => url === "/api/competition/native" ? ok({ competition }) : ok({ prizes: init?.body && JSON.parse(init.body).prizes || [100, 50] }));
  criteria(ui);
  ui.run("switchTab('tab-competition')");
  assert.equal(await ui.run("SaveBar.load()"), true);
  assert.equal(ui.els["prize-list"].disabled, false);
  ui.els["prize-list"].value = "120\n60";
  ui.run("SaveBar.markDirty('competition')");
  assert.equal(ui.requests.length, 2);
  assert.equal(await ui.run("SaveBar.save()"), true);
  assert.deepEqual(ui.requests[2].body, { teams: 2, first_prize: 100, last_prize: 50, tolerance: 0, prizes: [120, 60] });
  assert.equal(ui.requests.some(r => r.url === "/api/competition/update"), false);
  assert.equal(Number(ui.els["prize-first"].value), 120);
  assert.equal(ui.run("prizeChanges().length"), 0);
});

test("changed prize matching criteria prevent writes and keep registration queued", async () => {
  const ui = app(); criteria(ui);
  await ui.run("loadCompetitionPage()");
  ui.els["prize-list"].value = "120\n60";
  ui.els["prize-first"].value = "999";
  ui.run("switchTab('tab-competition'); state.liveCompetitions = {8: {unique_id: 88}}; stageCompetitionRegistration(true)");
  assert.equal(await ui.run("SaveBar.save()"), false);
  assert.equal(ui.requests.length, 2);
  assert.equal(ui.run("pendingActions.get('competition:8').length"), 1);
});

test("failed prize writes remain dirty and do not dispatch registration", async () => {
  let fail = false;
  const ui = app(async url => fail ? { ok: false, json: async () => ({ message: "Readback mismatch" }) } : url === "/api/competition/native" ? ok({ competition }) : ok({ prizes: [100, 50] }));
  criteria(ui); await ui.run("loadCompetitionPage()");
  ui.els["prize-list"].value = "120\n60";
  ui.run("switchTab('tab-competition'); state.liveCompetitions = {8: {unique_id: 88}}; stageCompetitionRegistration(true)");
  fail = true;
  assert.equal(await ui.run("SaveBar.save()"), false);
  assert.equal(ui.run("prizeChanges().length"), 1);
  assert.equal(ui.run("pendingActions.get('competition:8').length"), 1);
});

test("Competition Discard restores the loaded prize table even if matching criteria are invalid", async () => {
  const ui = app(); criteria(ui); await ui.run("loadCompetitionPage()");
  ui.els["prize-list"].value = "120\n60"; ui.els["prize-first"].value = "";
  ui.run("switchTab('tab-competition')");
  assert.equal(await ui.run("SaveBar.discard()"), true);
  assert.equal(ui.els["prize-list"].value, "100\n50");
  assert.equal(Number(ui.els["prize-first"].value), 100);
  assert.equal(ui.requests.length, 2);
});

test("a context switch disables the previous Competition prize editor", async () => {
  const ui = app(); criteria(ui); await ui.run("loadPrizeTable()");
  ui.run("displayCompetition({ id: 9, name: 'Other', reputation: 0 })");
  assert.equal(ui.els["prize-list"].disabled, true);
  assert.equal(ui.els["prize-list"].value, "");
});

test("a newer Nation edit survives an in-flight save and delays queued actions", async () => {
  let release;
  const ui = app(async url => url === "/api/nation/native" ? ok({ nation }) : new Promise(resolve => { release = resolve; }));
  await ui.run("loadNationPage();");
  ui.els["nation-reputation"].value = "8500";
  ui.run("switchTab('tab-nation'); markNationDirty(); queuePreset('nation', 'golden_generation')");
  const save = ui.run("SaveBar.save()");
  ui.els["nation-reputation"].value = "9000";
  ui.run("markNationDirty()");
  release(ok({}));
  assert.equal(await save, false);
  assert.equal(ui.run("state.nationDirty"), true);
  assert.equal(ui.run("nationFieldChanges()[0].from"), 8500);
  assert.equal(ui.run("pendingActions.get('nation:4').length"), 1);
  assert.equal(ui.requests.length, 2);
});

test("prize edits entered during a reputation save need another Save", async () => {
  let release;
  const ui = app(async url => url === "/api/competition/native" ? ok({ competition }) : url === "/api/competition/prizes" ? ok({ prizes: [100, 50] }) : new Promise(resolve => { release = resolve; }));
  criteria(ui); await ui.run("loadCompetitionPage()");
  ui.els["comp-reputation"].value = "8500";
  ui.run("switchTab('tab-competition'); handleCompRepChange(8500)");
  const save = ui.run("SaveBar.save()");
  ui.els["prize-list"].value = "120\n60";
  ui.run("SaveBar.markDirty('competition')");
  release(ok({}));
  assert.equal(await save, false);
  assert.equal(ui.requests.filter(r => r.body?.prizes).length, 0);
  assert.equal(ui.run("prizeChanges().length"), 1);
});

const ruleIds = ["nation-rule-years", "nation-rule-type", "nation-rule-dual"];
const unavailable = reason => ({ status: "unavailable", confidence: "probable", value: null, reason });
function ruleApp(rules) {
  const ui = createUi({ ids: [...ids, ...ruleIds], fetch: async url => url === "/api/nation/native" ? ok({ nation, nationality_rules: rules }) : ok({}) });
  ui.run("displayEntityAvailability = () => true; state.connected = true; state.activeNation = " + JSON.stringify(nation) + "; Shell.init()");
  return ui;
}

test("Nationality rules card shows unavailable reasons and stages nothing for unverified fields", async () => {
  const ui = ruleApp({
    years_to_gain_nationality: unavailable("candidate layout is Probable: not verified"),
    gain_nationality_type_code: unavailable("candidate layout is Unverified"),
    dual_nationality_allowed: unavailable("candidate layout is Unverified"),
    write_support: "none",
  });
  ui.run("switchTab('tab-nation')");
  assert.equal(await ui.run("SaveBar.load()"), true);
  assert.match(ui.els["nation-rule-years"].innerText, /^Unavailable: candidate layout is Probable/);
  assert.doesNotMatch(ui.els["nation-rule-years"].innerText, /\d+ years/);
  assert.match(ui.els["nation-rule-dual"].innerText, /^Unavailable/);
  assert.deepEqual(ui.run("nationFieldChanges()"), []);
  assert.equal(ui.requests.length, 1);
});

test("Nationality rules card shows only verified values, read-only", async () => {
  const ui = ruleApp({
    years_to_gain_nationality: { status: "verified", confidence: "verified", value: 10, reason: null },
    gain_nationality_type_code: unavailable("labels unknown"),
    dual_nationality_allowed: { status: "verified", confidence: "verified", value: false, reason: null },
  });
  ui.run("switchTab('tab-nation')");
  await ui.run("SaveBar.load()");
  assert.equal(ui.els["nation-rule-years"].innerText, "10 years");
  assert.equal(ui.els["nation-rule-dual"].innerText, "No");
  assert.match(ui.els["nation-rule-type"].innerText, /^Unavailable: labels unknown/);
  assert.equal(ui.run("nationFieldChanges().length"), 0);
});

test("Nationality rules card has no inputs and a switched nation drops the previous rules", async () => {
  assert.doesNotMatch(read("pages/nation.html").split('id="nation-rules-card"')[1], /<(?:input|select|button)/);
  const ui = ruleApp({ years_to_gain_nationality: { status: "verified", confidence: "verified", value: 10, reason: null } });
  ui.run("switchTab('tab-nation')");
  await ui.run("SaveBar.load()");
  ui.run("displayNation({ id: 9, name: 'Other' })");
  assert.match(ui.els["nation-rule-years"].innerText, /^Unavailable/);
});
