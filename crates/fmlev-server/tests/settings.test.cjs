const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const vm = require('node:vm');
const { html } = require('./helpers/ui.cjs');
const { script } = require('./helpers/ui.cjs');

function ui() {
  const context = vm.createContext({
    localStorage: { getItem: () => null, setItem() {} },
    fetch: async () => ({ ok: true, json: async () => ({}) }), console,
    setTimeout() { return 1; }, clearTimeout() {},
    window: { addEventListener() {} },
    document: { addEventListener() {}, getElementById: () => null, querySelectorAll: () => [] },
  });
  vm.runInContext(script, context);
  vm.runInContext(`
    currencyTable = [
      { code: "GBP", symbol: "£", decimals: 0, rate_per_gbp: 1 },
      { code: "USD", symbol: "$", decimals: 0, rate_per_gbp: 1.3508 },
      { code: "IDR", symbol: "Rp ", decimals: 0, rate_per_gbp: 22400 },
    ];
  `, context);
  return code => vm.runInContext(code, context);
}
const input = money => ({ value: '', dataset: { money }, disabled: false });

test('no hardcoded currency symbols remain in the UI markup', () => {
  const markup = html.split('<script>')[0];
  assert.doesNotMatch(markup, /\$\d|\(\$\)|\$ \/|USD/);
});

test('formatter follows currency and wage period', () => {
  const run = ui();
  assert.equal(run('formatMoney(400000)'), '£400,000');
  run('displaySettings.currency = "USD"');
  assert.equal(run('formatMoney(100)'), '$135');
  assert.equal(run('formatMoney(50000000, { compact: true })'), '$67.54M');
  run('displaySettings.wage_period = "yearly"; displaySettings.currency = "GBP"');
  assert.equal(run('formatMoney(1000, { kind: "wage" })'), '£52,000 / year');
  assert.equal(run('formatMoney(1000)'), '£1,000');
  assert.equal(run('formatMoney(null)'), 'Unavailable');
});

test('untouched money inputs return the exact raw baseline, edited ones parse back', () => {
  const run = ui();
  run('displaySettings.currency = "USD"');
  run('globalThis.el = { value: "", dataset: { money: "money" } }; setMoneyInput(el, 123457)');
  assert.equal(run('el.value'), '166766');
  // Untouched: returns the native baseline without re-deriving it from the shown text.
  run('displaySettings.currency = "IDR"');
  assert.equal(run('readMoneyInput(el, 0)'), 123457);
  // Edited: the typed text is parsed in the current display units.
  run('el.value = "22400000"');
  assert.equal(run('readMoneyInput(el, 0)'), 1000);
});

test('edited input parses back to raw units, and settings changes keep raw value', () => {
  const run = ui();
  run('displaySettings.currency = "IDR"; displaySettings.wage_period = "monthly"');
  run(`globalThis.w = { value: "", dataset: { money: "wage" } }; setMoneyInput(w, 520000);`);
  assert.equal(run('readMoneyInput(w, 0)'), 520000);
  run('w.value = String(Number(w.value) + 22400 * 52 / 12 * 1000)');
  assert.equal(run('readMoneyInput(w, 0)'), 521000);
  // switch to GBP/weekly: same raw value shows as 520000
  run('displaySettings.currency = "GBP"; displaySettings.wage_period = "weekly"; setMoneyInput(w, 520000)');
  assert.equal(run('w.value'), '520000');
});

test('measurement formatting', () => {
  const run = ui();
  assert.equal(run('metersToHeight(185)'), '185 cm');
  run('displaySettings.measurement = "imperial"');
  assert.equal(run('metersToHeight(185)'), `6'1"`);
  assert.equal(run('formatWeight(80)'), '176 lb');
});
