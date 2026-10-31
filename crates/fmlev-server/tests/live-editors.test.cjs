const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const path = require('node:path');
const { html } = require('./helpers/ui.cjs');
const router = readFileSync(path.join(__dirname, '../src/routes/mod.rs'), 'utf8');

// Connected mode hides every direct child of the club/nation/competition tabs except marked live editors,
// so each live editor card must be a direct child of its tab section and carry data-live-editor.
function depthAtCard(section, cardId) {
  const start = html.indexOf(`<section id="${section}"`);
  const end = html.indexOf('</section>', start);
  const card = html.indexOf(`id="${cardId}"`, start);
  assert.ok(card > start && card < end, `${cardId} is inside ${section}`);
  const before = html.slice(start, html.lastIndexOf('<div', card));
  const opens = (before.match(/<div[\s>]/g) || []).length;
  const closes = (before.match(/<\/div>/g) || []).length;
  return opens - closes;
}

for (const [section, card] of [
  ['tab-club', 'club-money-card'],
  ['tab-club', 'club-facilities-card'],
  ['tab-nation', 'nation-youth-card'],
  ['tab-nation', 'nation-macro-card'],
  ['tab-competition', 'competition-reputation-card'],
  ['tab-competition', 'registration-card'],
  ['tab-competition', 'prize-card']
]) {
  test(`${card} stays visible while connected`, () => {
    assert.equal(depthAtCard(section, card), 0, `${card} must be a direct child of ${section}`);
    assert.match(html, new RegExp(`id="${card}"[^>]*data-live-editor="true"`));
  });
}

test('connected-mode hiding skips marked live editors', () => {
  assert.match(html, /child\.hasAttribute\("data-live-editor"\)/);
});

test('every live editor endpoint the UI calls exists in the router', () => {
  for (const route of [
    '/api/player/squad-number',
    '/api/player/contract/extend',
    '/api/player/familiarity',
    '/api/club/money',
    '/api/club/native',
    '/api/nation/native',
    '/api/competition/native',
    '/api/club/update',
    '/api/nation/update',
    '/api/competition/update',
    '/api/competition/prizes',
    '/api/competition/registration',
    '/api/player/contract/terminate',
    '/api/transfer/move',
    '/api/transfer/sign',
  ]) {
    assert.ok(html.includes(`"${route}"`), `UI calls ${route}`);
    assert.ok(router.includes(`"${route}"`), `router serves ${route}`);
  }
  // Terminate, move and sign start disabled and are enabled by script (attached game, open player, filled form); loan recall stays off.
  for (const id of ['btn-terminate-contract', 'btn-move-player', 'btn-sign-free-agent'])
    assert.match(html, new RegExp(`id="${id}" disabled class=`), `${id} starts disabled`);
  assert.match(html, /id="btn-terminate-loan" disabled data-unverified="true"/, 'Loan recall stays unavailable');
});

test('live editor actions and shared save adapters call functions that exist', () => {
  for (const fn of [
    'saveSquadNumber',
    'extendContract',
    'signFreeAgent',
    'terminateContract',
    'terminateLoan',
    'movePlayer',
    'maxFamiliarity',
    'loadClubMoney',
    'saveClubMoney',
    'saveClubToGame',
    'saveNationToGame',
    'saveCompetitionToGame',
    'loadPrizeTable',
    'savePrizeTable',
    'setRegistration',
  ]) {
    assert.match(html, new RegExp(`function ${fn}\\(`), `${fn} is defined`);
  }
  // Player actions are staged; Club writes run through its single shared Save pair.
  for (const fn of ['signFreeAgent', 'terminateContract', 'terminateLoan', 'movePlayer', 'stageCompetitionRegistration']) {
    assert.match(html, new RegExp(`onclick="${fn}\\(`), `${fn} is wired to a button`);
  }
  assert.match(html, /onchange="stageSquadNumber\(/);
  assert.match(html, /onclick="stageContractExtension\(/);
  assert.match(html, /onclick="stageFamiliarity\(/);
  assert.match(html, /SaveBar\.register\("club", \{ load: loadClubPage, save: saveClubPage/);
  assert.match(html, /SaveBar\.register\("nation", \{ load: loadNationPage/);
  assert.match(html, /SaveBar\.register\("competition", \{ load: loadCompetitionPage/);
  assert.match(html, /onclick="SaveBar\.save\(\)"/);
});
