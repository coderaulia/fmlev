// Player date of birth: native {year, day of year} shown in a date input, edited only through Save.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const { createUi } = require("./helpers/fake-dom.cjs");
const { read } = require("./helpers/ui.cjs");

function app() {
  const ui = createUi({ ids: ["input-dob", "fact-age", "savebar", "savebar-status", "savebar-error"], fetch: async () => ({ ok: true, status: 200, json: async () => ({}), text: async () => "" }) });
  ui.run("state.connected = true; globalThis.toasts = []; showToast = (m, k) => toasts.push([m, k]);");
  return ui;
}

test("player page has a date of birth input wired to handleDobChange", () => {
  assert.match(read("pages/player.html"), /id="input-dob"[^>]*type="date"/);
  assert.match(read("pages/player.html"), /onchange="handleDobChange\(this.value\)"/);
  assert.match(read("pages/player.html"), /onblur="handleDobBlur\(this\)"/);
});

test("day of year converts to and from the date input, including leap years", () => {
  const ui = app();
  assert.equal(ui.run("dobToInput({ year: 1998, day: 91 })"), "1998-04-01");
  assert.equal(ui.run("dobToInput({ year: 2000, day: 203 })"), "2000-07-21");
  assert.deepEqual(ui.run("inputToDob('2007-07-13')"), { year: 2007, day: 194 });
  assert.deepEqual(ui.run("inputToDob('2000-12-31')"), { year: 2000, day: 366 });
  assert.equal(ui.run("inputToDob('2001-02-29')"), null);
  assert.equal(ui.run("inputToDob('1899-12-31')"), null);
  assert.equal(ui.run("dobToInput(undefined)"), "");
});

test("the input is enabled only when the server lists date_of_birth as writable", () => {
  const ui = app();
  ui.run("state.playerWritableFields = []; renderDob({ date_of_birth: { year: 1998, day: 91 } })");
  assert.equal(ui.els["input-dob"].disabled, true);
  assert.equal(ui.els["input-dob"].value, "1998-04-01");
  ui.run("state.playerWritableFields = ['date_of_birth']; renderDob({ date_of_birth: { year: 1998, day: 91 } })");
  assert.equal(ui.els["input-dob"].disabled, false);
  ui.run("renderDob({})");
  assert.equal(ui.els["input-dob"].disabled, true, "unreadable date of birth stays disabled");
});

test("a change stages the field as dirty and reverting clears it", () => {
  const ui = app();
  ui.run("state.activePlayer = { id: 9, date_of_birth: { year: 1998, day: 91 } }; state.caEstimateBaseline = { id: 9, date_of_birth: { year: 1998, day: 91 } };");
  ui.run("handleDobChange('2001-01-15')");
  assert.deepEqual(ui.run("state.activePlayer.date_of_birth"), { year: 2001, day: 15 });
  assert.equal(ui.run("state.playerDirtyFields.has('date_of_birth')"), true);
  ui.run("handleDobChange('1998-04-01')");
  assert.equal(ui.run("state.playerDirtyFields.has('date_of_birth')"), false);
  ui.run("handleDobChange('not a date')");
  assert.deepEqual(ui.run("state.activePlayer.date_of_birth"), { year: 1998, day: 91 }, "invalid input changes nothing");
  assert.equal(ui.run("toasts.length"), 0, "no warning while typing");
});

test("typing a year keystroke by keystroke never resets the input or warns", () => {
  const ui = app();
  ui.run("state.activePlayer = { id: 9, date_of_birth: { year: 2010, day: 161 } }; state.caEstimateBaseline = { id: 9, date_of_birth: { year: 2010, day: 161 } };");
  for (const partial of ["0002-06-10", "0020-06-10", "0201-06-10", "2011-06-10"]) {
    ui.els["input-dob"].value = partial;
    ui.run(`handleDobChange('${partial}')`);
    assert.equal(ui.els["input-dob"].value, partial, "the typed value stays in the input");
  }
  assert.deepEqual(ui.run("state.activePlayer.date_of_birth"), { year: 2011, day: 161 });
  assert.equal(ui.run("state.playerDirtyFields.has('date_of_birth')"), true);
  assert.equal(ui.run("toasts.length"), 0);
});

test("blur reverts an unfinished date to the staged value with one warning", () => {
  const ui = app();
  ui.run("state.activePlayer = { id: 9, date_of_birth: { year: 2010, day: 161 } };");
  ui.els["input-dob"].value = "0201-06-10";
  ui.run("handleDobBlur(document.getElementById('input-dob'))");
  assert.equal(ui.els["input-dob"].value, "2010-06-10");
  assert.equal(ui.run("toasts.length"), 1);
  ui.run("handleDobBlur(document.getElementById('input-dob'))");
  assert.equal(ui.run("toasts.length"), 1, "a valid date on blur does not warn");
});
