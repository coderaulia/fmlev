const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const path = require('node:path');
const html = readFileSync(path.join(__dirname, '../src/ui.html'), 'utf8');
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

for (const [section, card] of [['tab-club', 'club-money-card'], ['tab-competition', 'registration-card'], ['tab-competition', 'prize-card']]) {
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
    '/api/player/contract/terminate',
    '/api/player/loan/terminate',
    '/api/transfer/move',
    '/api/player/familiarity',
    '/api/club/money',
    '/api/competition/prizes',
    '/api/competition/registration',
  ]) {
    assert.ok(html.includes(`"${route}"`), `UI calls ${route}`);
    assert.ok(router.includes(`"${route}"`), `router serves ${route}`);
  }
});

test('live editor buttons call functions that exist', () => {
  for (const fn of [
    'saveSquadNumber',
    'extendContract',
    'terminateContract',
    'terminateLoan',
    'movePlayer',
    'maxFamiliarity',
    'loadClubMoney',
    'saveClubMoney',
    'loadPrizeTable',
    'savePrizeTable',
    'setRegistration',
  ]) {
    assert.match(html, new RegExp(`function ${fn}\\(`), `${fn} is defined`);
    assert.match(html, new RegExp(`onclick="${fn}\\(`), `${fn} is wired to a button`);
  }
});
