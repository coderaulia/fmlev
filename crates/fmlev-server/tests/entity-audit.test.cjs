const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const vm = require('node:vm');
const { html } = require('./helpers/ui.cjs');
const { script } = require('./helpers/ui.cjs');

function app() {
  const nodes = new Map();
  function element(id = '') {
    return { id, hidden: false, disabled: false, dataset: {}, style: {}, children: [], innerText: '',
      setAttribute() {},
      hasAttribute(name) { return Object.hasOwn(this.dataset, name.replace(/^data-/, '').replace(/-([a-z])/g, (_, c) => c.toUpperCase())); },
      appendChild(child) { this.children.push(child); nodes.set(child.id, child); },
      append(...children) { for (const child of children) this.appendChild(child); },
      replaceChildren() { this.children = []; },
      querySelectorAll() { return this.controls || []; },
      get firstElementChild() { return this.children[0]; }
    };
  }
  for (const kind of ['club', 'nation', 'competition']) {
    const panel = element(`tab-${kind}`);
    panel.children = [element(), element(), element()];
    panel.children[1].style.display = 'grid';
    panel.children[2].hidden = true;
    panel.controls = [element(), element(), element()];
    panel.controls[2].disabled = true;
    nodes.set(panel.id, panel);
  }
  for (const id of ['club-title', 'club-league-badge', 'club-rep-num', 'club-rep-stars', 'save-club-status-indicator',
    'nation-name', 'nation-code', 'nation-continent', 'nation-flag-avatar', 'nation-rep-num', 'nation-rep-stars',
    'comp-name', 'comp-short-name', 'comp-nation', 'comp-rep-num', 'comp-rep-stars']) nodes.set(id, element(id));
  const context = vm.createContext({ console, setTimeout() {}, clearTimeout() {},
    localStorage: { getItem: () => null, setItem() {} }, window: { addEventListener() {} },
    document: { addEventListener() {}, getElementById: id => nodes.get(id) || null, createElement: () => element() }
  });
  vm.runInContext(script, context);
  return { run: code => vm.runInContext(code, context), nodes };
}

for (const [kind, display, record, title, expected] of [
  ['club', 'displayClub', { club_id: 2313, club_name: 'Persib', reputation: 0 }, 'club-title', 'Persib'],
  ['nation', 'displayNation', { id: 0, name: '', reputation: 0 }, 'nation-name', 'Nation unavailable'],
  ['competition', 'displayCompetition', { id: 0, name: '', reputation: 0 }, 'comp-name', 'Competition unavailable']
]) {
  test(`live ${kind} hides unverified values and disables editing while retaining identity`, () => {
    const ui = app();
    ui.run(`state.connected = true; ${display}(${JSON.stringify(record)});`);
    assert.equal(ui.nodes.get(title).innerText, expected);
    const prefix = kind === 'competition' ? 'comp' : kind;
    assert.equal(ui.nodes.get(`${prefix}-rep-num`).innerText, 'Unavailable');
    const panel = ui.nodes.get(`tab-${kind}`);
    assert.equal(panel.children[0].hidden, false);
    assert.equal(panel.children[1].hidden, true);
    assert.equal(panel.children[1].style.display, 'none', 'inline grid layouts cannot expose fixtures');
    assert.equal(panel.controls.every(control => control.disabled), true);
    const banner = ui.nodes.get(`${kind}-live-unavailable`);
    assert.equal(banner.hidden, false);
    assert.match(banner.children[0].textContent, /Editing is unavailable/);
    ui.run(`${display}(${JSON.stringify(record)});`);
    assert.equal(panel.children.length, 4, 'polls reuse the banner');
  });

  test(`${kind} fixtures recover their original controls and visibility after disconnect`, () => {
    const ui = app();
    ui.run(`state.connected = true; ${display}(${JSON.stringify(record)}); state.connected = false; displayEntityAvailability('${kind}', {});`);
    const panel = ui.nodes.get(`tab-${kind}`);
    assert.equal(panel.children[1].hidden, false);
    assert.equal(panel.children[1].style.display, 'grid');
    assert.equal(panel.children[2].hidden, true, 'original hidden content stays hidden');
    assert.deepEqual(panel.controls.map(control => control.disabled), [false, false, true]);
    assert.equal(ui.nodes.get(`${kind}-live-unavailable`).hidden, true);
  });
}

test('verified competition overview renders identity facts without exposing rule controls', () => {
  const ui = app();
  ui.run(`state.connected = true; state.liveCompetitions = {3295: {id:3295,name:'Super League',number_of_teams:18,season_name:'2025/26',asia_reputation_position:26,reputation_description:'Decent'}}; displayCompetition({id:3295,name:'Super League',reputation:0});`);
  const banner = ui.nodes.get('competition-live-unavailable');
  const facts = banner.children[1].children.map(node => node.textContent);
  assert.deepEqual(facts, ['Clubs', '18', 'Season', '2025/26', 'Asia ranking', '26', 'Reputation', 'Decent']);
  assert.equal(ui.nodes.get('tab-competition').controls.every(control => control.disabled), true);
});

test('verified live editor controls remain enabled while unverified controls are disabled in connected mode', () => {
  const ui = app();
  const panel = ui.nodes.get('tab-club');
  const verifiedControl = {
    id: 'fac-training', disabled: false, dataset: {}, style: {},
    closest(sel) { return sel === '[data-live-editor]' ? {} : null; },
    hasAttribute() { return false; },
    getAttribute() { return null; }
  };
  const unverifiedControl = {
    id: 'club-board-patience', disabled: false, dataset: { unverified: 'true' }, style: {},
    closest(sel) { return sel === '[data-live-editor]' ? {} : null; },
    hasAttribute(attr) { return attr === 'data-unverified'; },
    getAttribute() { return null; }
  };
  panel.controls = [verifiedControl, unverifiedControl];
  ui.run(`state.connected = true; displayClub({ club_id: 2313, club_name: 'Persib', reputation: 7000 });`);
  assert.equal(verifiedControl.disabled, false, 'verified live controls must stay enabled');
  assert.equal(unverifiedControl.disabled, true, 'unverified controls must be disabled');
});
