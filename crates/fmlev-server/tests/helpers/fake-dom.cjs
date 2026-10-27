// Minimal fake browser for the concatenated UI script (vm sandbox). Elements are created on demand for ids in `ids`.
const vm = require('node:vm');
const { script } = require('./ui.cjs');

function element(id) {
  const classes = new Set();
  const listeners = {};
  const el = {
    id, innerHTML: '', textContent: '', innerText: '', value: '', disabled: false, hidden: false, checked: false,
    style: {}, dataset: {}, children: [], focused: false, selected: false,
    classList: {
      add: (...names) => names.forEach(n => classes.add(n)),
      remove: (...names) => names.forEach(n => classes.delete(n)),
      contains: name => classes.has(name),
      toggle: (name, on) => { if (on === undefined ? !classes.has(name) : on) classes.add(name); else classes.delete(name); },
    },
    focus() { el.focused = true; }, select() { el.selected = true; },
    addEventListener(name, fn) { (listeners[name] ||= []).push(fn); },
    click() { (listeners.click || []).forEach(fn => fn({ target: el })); },
    closest: () => null,
  };
  return el;
}

// ids: elements that exist; tabs: ids of .tab-view sections. Returns { run, els, fire, requests, messages, document }.
function createUi({ fetch, ids = [], tabs = [], extra = {} } = {}) {
  const els = {};
  for (const id of [...ids, ...tabs]) els[id] = element(id);
  const handlers = {};
  const requests = [];
  const document = {
    activeElement: null,
    body: element('body'),
    addEventListener(name, fn) { (handlers[name] ||= []).push(fn); },
    getElementById: id => els[id] || null,
    querySelectorAll: selector => (selector === '.tab-view' ? tabs.map(id => els[id]) : []),
    querySelector: () => null,
  };
  const winHandlers = {};
  const sandbox = {
    els, document, console, requests,
    localStorage: { getItem: () => null, setItem() {} },
    fetch: async (url, init) => { requests.push({ url, body: init && init.body ? JSON.parse(init.body) : undefined }); return fetch(url, init); },
    window: { addEventListener: (name, fn) => { (winHandlers[name] ||= []).push(fn); } },
    setTimeout: () => 1, clearTimeout() {}, setInterval: () => 1,
    ...extra,
  };
  const context = vm.createContext(sandbox);
  vm.runInContext(script + `\nvar messages = []; showToast = (message, type) => messages.push({ message, type });`, context);
  return {
    els, requests, document, winHandlers,
    // Objects are JSON-cloned so deepEqual works across the vm realm; promises pass through untouched.
    run: code => {
      const result = vm.runInContext(code, context);
      return result && typeof result === 'object' && typeof result.then !== 'function' ? JSON.parse(JSON.stringify(result)) : result;
    },
    fire: (name, event) => (handlers[name] || []).forEach(fn => fn(event)),
    element,
  };
}

module.exports = { createUi, element };
