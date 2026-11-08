const { test } = require("node:test");
const assert = require("node:assert/strict");
const { createUi } = require("./helpers/fake-dom.cjs");
const { read } = require("./helpers/ui.cjs");

const ok = data => ({ ok: true, json: async () => data });
const bad = (status, message) => ({ ok: false, status, statusText: "x", json: async () => ({ message }) });
const field = value => ({ value, reason: value == null ? "outside the range seen" : null });
const details = { original_reputation: field(187), trophy_type: field(6), lowest_category: field(null), pitch_min_length: field(1050), pitch_min_width: field(680), pitch_max_length: field(1050), pitch_max_width: field(680) };
const award = { index: 0, uid: 102406, name: field("World Golden Ball"), short_name: field("World Golden Ball"), names_share_buffer: true,
  reputation_candidate_a: field(200), reputation_candidate_b: field(200), kind_code: field(37) };
const ids = ["compx-enable", "compx-status", "compx-orig-rep", "compx-trophy", "compx-lowcat", "compx-min-length", "compx-min-width", "compx-max-length", "compx-max-width",
  "compx-apply-details", "compx-prize-list", "compx-prize-note", "compx-apply-prizes", "compx-table-body", "compx-table-note", "compx-award-query", "compx-award-results",
  "compx-award-status", "compx-award-name", "compx-award-short", "compx-award-rep-a", "compx-award-rep-b", "compx-apply-award"];

function app(routes = {}) {
  const fetch = async (url, init) => {
    const handler = routes[url.split("?")[0]];
    return handler ? handler(init && init.body ? JSON.parse(init.body) : undefined) : ok({});
  };
  const ui = createUi({ ids, fetch, extra: { confirm: () => true } });
  ui.run("state.activeCompetition = { id: 7 }; state.liveCompetitions = { 7: { unique_id: 11 } }");
  return ui;
}

test("page markup carries the experimental warnings and every control starts disabled", () => {
  const html = read("pages/competition.html");
  assert.match(html, /id="compx-enable" type="checkbox"/);
  assert.doesNotMatch(html, /id="compx-enable"[^>]*checked/);
  assert.match(html, /unverified in game/);
  for (const id of ["compx-apply-details", "compx-apply-prizes", "compx-apply-award", "compx-orig-rep", "compx-award-name"]) {
    assert.match(html, new RegExp(`id="${id}"[^>]*disabled`), id);
  }
});

test("details load shows unavailable fields as locked and edits send only changed fields", async () => {
  const ui = app({ "/api/competition/details": () => ok({ details }), "/api/competition/details/update": body => ok({ message: "1 field(s) written", changed: ["trophy type"], details: { ...details, trophy_type: field(body.trophy_type) } }) });
  assert.equal(await ui.run("compxLoadDetails()"), true);
  assert.deepEqual(ui.requests[0].body, { comp_index: 7, comp_uid: 11 });
  assert.equal(ui.els["compx-orig-rep"].value, 187);
  assert.equal(ui.els["compx-lowcat"].value, "");
  assert.match(ui.els["compx-status"].innerText, /lowest_category: outside the range seen/);
  // Locked until the experimental box is ticked; the unreadable field stays locked even then.
  assert.equal(ui.els["compx-apply-details"].disabled, true);
  ui.els["compx-enable"].checked = true;
  ui.run("compxRenderEnabled()");
  assert.equal(ui.els["compx-apply-details"].disabled, false);
  assert.equal(ui.els["compx-trophy"].disabled, false);
  assert.equal(ui.els["compx-lowcat"].disabled, true);
  ui.els["compx-trophy"].value = "2";
  assert.equal(await ui.run("compxApplyDetails()"), true);
  assert.deepEqual(ui.requests[1].body, { comp_index: 7, comp_uid: 11, trophy_type: 2, experimental: true });
  assert.equal(ui.els["compx-trophy"].value, 2);
});

test("an unchanged form, a missing checkbox or a missing unique id never send a write", async () => {
  const ui = app({ "/api/competition/details": () => ok({ details }) });
  await ui.run("compxLoadDetails()");
  assert.equal(await ui.run("compxApplyDetails()"), false);
  ui.els["compx-enable"].checked = true;
  ui.run("compxRenderEnabled()");
  assert.equal(await ui.run("compxApplyDetails()"), false);
  assert.match(ui.els["compx-status"].innerText, /Nothing changed/);
  ui.run("state.liveCompetitions = {}");
  assert.equal(await ui.run("compxLoadDetails()"), false);
  assert.match(ui.els["compx-status"].innerText, /unique id/);
  assert.equal(ui.requests.length, 1);
});

test("native prize table loads, validates the count and writes the stage it was read from", async () => {
  const table = { stage_index: 0, positions: 3, amounts: [300, 200, 100] };
  const ui = app({ "/api/competition/prizes/native": () => ok({ tables: [table] }), "/api/competition/prizes/native/update": body => ok({ message: "Prize table written", table: { ...table, amounts: body.prizes } }) });
  assert.equal(await ui.run("compxLoadPrizes()"), true);
  assert.equal(ui.els["compx-prize-list"].value, "300\n200\n100");
  ui.els["compx-enable"].checked = true;
  ui.run("compxRenderEnabled()");
  ui.els["compx-prize-list"].value = "300\n200";
  assert.equal(await ui.run("compxApplyPrizes()"), false);
  ui.els["compx-prize-list"].value = "310\n200\n100";
  assert.equal(await ui.run("compxApplyPrizes()"), true);
  assert.deepEqual(ui.requests.at(-1).body, { comp_index: 7, comp_uid: 11, stage_index: 0, prizes: [310, 200, 100], experimental: true });
});

test("a league without a native prize table reports the reason and stays locked", async () => {
  const ui = app({ "/api/competition/prizes/native": () => bad(400, "This league has no native prize table (pointer is null)") });
  assert.equal(await ui.run("compxLoadPrizes()"), false);
  assert.match(ui.els["compx-prize-note"].innerText, /no native prize table/);
  assert.equal(ui.els["compx-apply-prizes"].disabled, true);
});

test("league table renders rows, escapes names and marks inconsistent points", async () => {
  const rows = [
    { position: 1, club_name: "A <b>x</b>", played: 3, won: 2, drawn: 1, lost: 0, goals_for: 5, goals_against: 2, points: 7, consistent: true },
    { position: 2, club_name: null, played: 3, won: 1, drawn: 0, lost: 2, goals_for: 1, goals_against: 4, points: 5, consistent: false },
  ];
  const ui = app({ "/api/competition/table": () => ok({ tables: [{ stage_index: 0, rows }] }) });
  assert.equal(await ui.run("compxLoadTable()"), true);
  const html = ui.els["compx-table-body"].innerHTML;
  assert.match(html, /A &lt;b&gt;x&lt;\/b&gt;/);
  assert.doesNotMatch(html, /<b>x/);
  assert.match(html, /Unavailable/);
  assert.match(html, /5 \*/);
});

test("awards search, open and edit only changed fields within the current name length", async () => {
  const ui = app({
    "/api/awards": () => ok({ total: 1, awards: [{ index: 0, uid: 102406, name: "World <Golden> Ball" }] }),
    "/api/awards/get": () => ok({ award }),
    "/api/awards/update": body => ok({ message: "2 award field(s) written", award: { ...award, name: field(body.name), reputation_candidate_a: field(body.reputation_candidate_a) } }),
  });
  ui.els["compx-award-query"].value = "golden";
  assert.equal(await ui.run("compxSearchAwards()"), true);
  assert.match(ui.els["compx-award-results"].innerHTML, /World &lt;Golden&gt; Ball/);
  assert.equal(await ui.run("compxPickAward(0, 102406)"), true);
  assert.equal(ui.els["compx-award-name"].value, "World Golden Ball");
  assert.match(ui.els["compx-award-status"].innerText, /share one buffer/);
  ui.els["compx-enable"].checked = true;
  ui.run("compxRenderEnabled()");
  assert.equal(ui.els["compx-award-rep-a"].disabled, false);
  ui.els["compx-award-name"].value = "World Golden Ball Extra";
  assert.equal(await ui.run("compxApplyAward()"), false);
  assert.match(ui.els["compx-status"].innerText, /longer/);
  ui.els["compx-award-name"].value = "World Gold Ball";
  ui.els["compx-award-rep-a"].value = "150";
  assert.equal(await ui.run("compxApplyAward()"), true);
  assert.deepEqual(ui.requests.at(-1).body, { index: 0, uid: 102406, name: "World Gold Ball", reputation_candidate_a: 150, experimental: true });
});

test("award reputation candidates outside 0-200 are refused before any request", async () => {
  const ui = app({ "/api/awards/get": () => ok({ award }) });
  await ui.run("compxPickAward(0, 102406)");
  ui.els["compx-enable"].checked = true;
  ui.run("compxRenderEnabled()");
  ui.els["compx-award-rep-b"].value = "250";
  assert.equal(await ui.run("compxApplyAward()"), false);
  assert.equal(ui.requests.length, 1);
});
