const { test } = require('node:test');
const assert = require('node:assert/strict');
const { createUi } = require('./helpers/fake-dom.cjs');
const { html, read } = require('./helpers/ui.cjs');
const ok = body => ({ ok: true, json: async () => body });

test('every static inline handler resolves in the assembled application', () => {
  const app = createUi();
  const markup = html.split('<script>')[0];
  for (const [, handler] of markup.matchAll(/\bon(?:click|change|input|blur|focus|keydown)="([^"]*)"/g)) {
    for (const [, name] of handler.matchAll(/(?<![\w.])([A-Za-z_$][\w$]*)\s*\(/g)) {
      if (name === 'if' || name === 'Number') continue;
      assert.equal(app.run(`typeof ${name}`), 'function', `${name} resolves: ${handler}`);
    }
  }
  const ids = [...markup.matchAll(/\bid="([^"]+)"/g)].map(match => match[1]);
  assert.equal(new Set(ids).size, ids.length, 'rendered document IDs are unique');
});

test('registration belongs to Player even when reached from another sidebar section', () => {
  const app = createUi({ ids: ['sidebar-nav'], tabs: ['tab-player', 'tab-club', 'tab-registration'] });
  app.run("Shell.init(); switchTab('tab-club'); switchTab('tab-registration')");
  assert.equal(app.run('Shell.current().section'), 'player');
  assert.match(app.els['sidebar-nav'].innerHTML, /aria-label="Player" title="Player" aria-current="page"/);
});

test('squad number, extension and familiarity are drafts until Player Save', async () => {
  const app = createUi({ ids: ['squad-number-input'], fetch: async () => ok({}) });
  app.run('state.activePlayer = { id: 7 }; SaveBar.init(); SaveBar.setActivePage("player")');
  app.els['squad-number-input'].value = '12';
  assert.equal(app.run('stageSquadNumber()'), true);
  app.els['squad-number-input'].value = '14';
  app.run('stageSquadNumber(); stageContractExtension(1); stageFamiliarity(false)');
  assert.equal(app.requests.length, 0);
  assert.equal(app.run('pendingActions.get("player:7").length'), 3);
  assert.equal(await app.run('SaveBar.save()'), true);
  assert.deepEqual(app.requests.map(request => request.body), [
    { player_id: 7, number: 14 }, { player_id: 7, years: 1 }, { player_id: 7, squad: false },
  ]);
});

test('roles populate the list and highlight selection without writing', async () => {
  const app = createUi({ ids: ['role-select', 'role-list', 'attr-line-passing'], fetch: async () => ok({
    roles: [{ id: 'dlp', name: 'Deep Lying Playmaker', group: 'Midfield', positions: ['mc'], key: ['passing'], secondary: [] }],
    role_data_verified: false,
  }) });
  app.run('state.activePlayer = { id: 7, positions: { mc: 18 } }');
  await app.run('loadRoles()');
  app.run('selectRole("dlp")');
  assert.match(app.els['role-list'].innerHTML, /18 \/ 20/);
  assert.equal(app.els['attr-line-passing'].classList.contains('key-attr'), true);
  app.run('clearRoleSelection()');
  assert.equal(app.els['attr-line-passing'].classList.contains('key-attr'), false);
  assert.equal(app.requests.length, 1);
});

test('Escape restores the attribute at focus and Enter keeps the edit without saving', () => {
  const app = createUi({ ids: ['attr-val-passing', 'attr-line-passing'] });
  app.run('state.activePlayer = {id: 7, attributes: {passing: 12}}; state.caEstimateBaseline = {id: 7, attributes: {passing: 12}}; beginAttrEdit("passing"); handleAttrInput("passing", "15", true)');
  app.run('handleAttrKey({key:"Escape",preventDefault(){}}, "passing")');
  assert.equal(app.run('state.activePlayer.attributes.passing'), 12);
  assert.equal(app.run('dirtyPlayerFields.size'), 0);
  app.els['attr-val-passing'].value = '14';
  app.run('handleAttrKey({key:"Enter",preventDefault(){}}, "passing")');
  assert.equal(app.run('state.activePlayer.attributes.passing'), 14);
  assert.equal(app.requests.length, 0);
});

test('search chips filter result kinds without fabricating unread ratings', async () => {
  const app = createUi({ ids: ['global-search', 'search-results-dropdown'], fetch: async url => ok(url === '/api/staff/search' ? {staff: []} : {
    players: [{id: 7, name: 'Test Player'}], clubs: [{club_id: 1, club_name: 'Test Club', reputation: 0}],
  }) });
  app.els['global-search'].value = 'test';
  await app.run('setSearchFilter("clubs")');
  assert.match(app.els['search-results-dropdown'].innerHTML, /Test Club/);
  assert.doesNotMatch(app.els['search-results-dropdown'].innerHTML, /Test Player|8,000/);
  assert.match(app.els['search-results-dropdown'].innerHTML, /Unavailable/);
});

test('context polling keeps a dirty selected entity when FM changes screen', async () => {
  const app = createUi({ fetch: async () => ok({ connected: true, active_player: {id: 8, name: 'Other'} }) });
  app.run('state.activePlayer = {id:7, name:"Draft", attributes:{passing:12}}; dirtyPlayerFields.add("attr_passing"); SaveBar.init()');
  await app.run('fetchContext()');
  assert.equal(app.run('state.activePlayer.id'), 7);
  assert.equal(app.run('dirtyPlayerFields.has("attr_passing")'), true);
});

test('compact shell keeps navigation and honours reduced-motion preference', () => {
  const css = read('styles/shell.css');
  assert.doesNotMatch(css, /body\.mode-compact-overlay #sidebar \{ display: none/);
  assert.match(css, /body\.mode-compact-overlay #sidebar-nav \{ display: flex; overflow-x: auto/);
  assert.match(css, /prefers-reduced-motion: reduce/);
});

test('normal text and FM value bands have accessible contrast against card surfaces', () => {
  const tokens = read('styles/tokens.css').split('body[data-band-scheme=')[0];
  const color = name => tokens.match(new RegExp(`${name}: (#[0-9a-f]{6})`, 'i'))[1];
  const luminance = hex => {
    const rgb = [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16) / 255)
      .map(v => v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4);
    return rgb[0] * .2126 + rgb[1] * .7152 + rgb[2] * .0722;
  };
  for (const name of ['--text-main', '--text-muted', '--text-dim', '--band-1-5', '--band-6-10', '--band-11-15', '--band-16-20']) {
    const a = luminance(color(name)), b = luminance(color('--bg-card'));
    assert.ok((Math.max(a, b) + .05) / (Math.min(a, b) + .05) >= 4.5, `${name} meets 4.5:1`);
  }
});

test('profile facts show read values while unavailable facts and contract writes stay explicit', () => {
  const app = createUi({ ids: ['player-avatar', 'player-sub', 'fact-age', 'fact-nat', 'fact-club', 'fact-contract', 'fact-rep', 'contract-wage', 'contract-release'] });
  app.run('renderPlayerFacts({name:"Test Player",age:24,nationality:"England",position:"ST",club_name:"Club",contract:{wage_per_week:100,expiry_year:2027}})');
  assert.equal(app.els['player-avatar'].innerText, 'TP');
  assert.equal(app.els['fact-age'].innerText, '24 years');
  assert.match(app.els['fact-contract'].innerText, /week.*2027/);
  assert.equal(app.els['fact-rep'].innerText, 'Unavailable');
  app.run('state.connected = true; state.playerWritableFields = ["contract.wage_per_week"]; applyContractGates()');
  assert.equal(app.els['contract-wage'].disabled, false);
  assert.equal(app.els['contract-release'].disabled, true);
});

test('preferred moves are shown read-only, naming only verified bits', () => {
  const app = createUi({ ids: ['fact-ppm'] });
  app.run('renderPreferredMoves({preferred_moves:{bitset:"0x8800000008",named_moves:["Cuts Inside From Both Wings"],unnamed_set_bits:2}})');
  assert.equal(app.els['fact-ppm'].innerText, 'Cuts Inside From Both Wings, 2 not yet identified');
  assert.match(app.els['fact-ppm'].title, /incomplete/);
  app.run('renderPreferredMoves({preferred_moves:{bitset:"0x0",named_moves:[],unnamed_set_bits:0}})');
  assert.equal(app.els['fact-ppm'].innerText, 'None set');
  app.run('renderPreferredMoves({})');
  assert.equal(app.els['fact-ppm'].innerText, 'Unavailable');
});
