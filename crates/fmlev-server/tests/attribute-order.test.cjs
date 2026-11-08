// Player attribute editor follows FM26's own attribute screen order, with a Set Pieces sub-heading.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { createUi } = require('./helpers/fake-dom.cjs');

const ids = ['list-technical', 'list-mental', 'list-physical', 'list-hidden', 'list-alt', 'attr-head-main', 'attr-alt-title', 'attr-alt-head'];
const order = html => [...html.matchAll(/class="attr-line-name">([^<]+)</g)].map(m => m[1]);

test('technical, set pieces, mental and physical match the FM26 screen order', () => {
  const ui = createUi({ ids });
  ui.run('buildAttributeLists(false)');
  const tech = ui.els['list-technical'].innerHTML;
  assert.deepEqual(order(tech), ['Crossing', 'Dribbling', 'Finishing', 'First Touch', 'Heading', 'Long Shots', 'Marking', 'Passing', 'Tackling', 'Technique',
    'Corners', 'Free Kick Taking', 'Long Throws', 'Penalty Taking']);
  assert.ok(tech.indexOf('Set Pieces') > tech.indexOf('>Technique<') && tech.indexOf('Set Pieces') < tech.indexOf('>Corners<'), 'Set Pieces heading sits between Technique and Corners');
  assert.deepEqual(order(ui.els['list-mental'].innerHTML), ['Aggression', 'Anticipation', 'Bravery', 'Composure', 'Concentration', 'Decisions', 'Determination',
    'Flair', 'Leadership', 'Off The Ball', 'Positioning', 'Teamwork', 'Vision', 'Work Rate']);
  assert.deepEqual(order(ui.els['list-physical'].innerHTML), ['Acceleration', 'Agility', 'Balance', 'Jumping Reach', 'Natural Fitness', 'Pace', 'Stamina', 'Strength']);
});

test('goalkeepers see goalkeeping in FM26 order and keep every attribute key once', () => {
  const ui = createUi({ ids });
  ui.run('buildAttributeLists(true)');
  assert.deepEqual(order(ui.els['list-technical'].innerHTML), ['Aerial Reach', 'Command Of Area', 'Communication', 'Eccentricity', 'Handling', 'Kicking',
    'One On Ones', 'Punching (Tendency)', 'Reflexes', 'Rushing Out (Tendency)', 'Throwing']);
  const all = ['list-technical', 'list-mental', 'list-physical', 'list-hidden', 'list-alt'].flatMap(id => [...ui.els[id].innerHTML.matchAll(/id="attr-val-([a-z_]+)"/g)].map(m => m[1]));
  assert.equal(new Set(all).size, all.length, 'no attribute input is rendered twice');
  assert.equal(all.length, 60);
});
