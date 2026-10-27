// Assembles the UI exactly like src/ui_bundle.rs (index.html + manifest.json), for DOM tests.
const { readFileSync } = require('node:fs');
const path = require('node:path');

const root = path.join(__dirname, '../../src/ui');
const read = file => readFileSync(path.join(root, file), 'utf8');
const manifest = JSON.parse(read('manifest.json'));
const join = group => manifest[group].map(read).join('');

const script = join('scripts');
const html = read('index.html')
  .replace('/*@STYLES*/\n', () => join('styles'))
  .replace('<!--@PAGES-->\n', () => join('pages'))
  .replace('/*@SCRIPTS*/\n', () => script);

module.exports = { html, script, manifest, read, root };
