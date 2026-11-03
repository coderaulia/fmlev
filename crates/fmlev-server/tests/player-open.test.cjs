// Player "Load from game" and "Open in game" load the profile FM26 is showing, not the editor's selection.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const { createUi } = require("./helpers/fake-dom.cjs");
const { read } = require("./helpers/ui.cjs");
const reply = (status, data) => ({ ok: status < 400, status, json: async () => data, text: async () => "" });
const ids = ["savebar", "savebar-status", "savebar-error"];

function app(route) {
  const ui = createUi({ ids, fetch: async (url, init) => { ui.requests.push(url); return route(url, init); } });
  ui.requests = [];
  ui.run("state.connected = true; state.activePlayer = { id: 1, name: 'Selected' }; globalThis.shown = []; displayPlayer = p => { shown.push(p.id); state.activePlayer = p; }; Shell.init(); switchTab('tab-player')");
  return ui;
}

test("player page has an Open in game button", () => {
  assert.match(read("pages/player.html"), /onclick="openPlayerFromGame\(\)"/);
});

test("Load from game opens the player shown in FM26", async () => {
  const ui = app(url => url === "/api/player/open" ? reply(200, { player: { id: 42259, name: "Rafael Struick" } }) : reply(200, {}));
  assert.equal(await ui.run("SaveBar.load()"), true);
  assert.deepEqual(ui.requests, ["/api/player/open"]);
  assert.deepEqual(ui.run("shown"), [42259]);
  assert.equal(ui.run("state.isPinned"), true);
});

test("Load from game falls back to the selected player when FM26 shows no player", async () => {
  const ui = app(url => url === "/api/player/open"
    ? reply(404, { message: "No player profile is open in FM26" })
    : reply(200, { player: { id: 1, name: "Selected" } }));
  assert.equal(await ui.run("SaveBar.load()"), true);
  assert.deepEqual(ui.requests, ["/api/player/open", "/api/context/select"]);
  assert.deepEqual(ui.run("shown"), [1]);
});
