const { test } = require('node:test');
const assert = require('node:assert/strict');
const { html } = require('./helpers/ui.cjs');
const { createUi } = require('./helpers/fake-dom.cjs');

test('Player tools edit native morale with the UID and the last read value', async () => {
  const ids = ['player-morale-value', 'player-morale-apply', 'player-morale-status'];
  const ui = createUi({
    ids, tabs: ['tab-player'], extra: { confirm: () => true },
    fetch: async (_, request) => ({ ok: true, json: async () => ({
      player_id: 5, player_uid: 500, value: request.method === 'PUT' ? 20 : 12,
    }) }),
  });
  assert.match(html, /id="player-morale-editor"/);
  ui.run("state.connected = true; state.activePlayer = { id: 5 }; clearPlayerMorale();");
  assert.equal(ui.els['player-morale-apply'].disabled, true);
  assert.equal(await ui.run('loadPlayerMorale()'), true);
  assert.equal(ui.els['player-morale-value'].value, 12);
  ui.els['player-morale-value'].value = '20';
  ui.run('state.activePlayer = { id: 6 };');
  assert.equal(await ui.run('applyPlayerMorale()'), false);
  ui.run('state.activePlayer = { id: 5 };');
  assert.equal(await ui.run('applyPlayerMorale()'), true);
  assert.deepEqual(ui.requests.map(r => [r.url, r.body]), [
    ['/api/player/morale', { player_id: 5 }],
    ['/api/player/morale', { player_id: 5, player_uid: 500, expected: 12, value: 20 }],
  ]);
});
