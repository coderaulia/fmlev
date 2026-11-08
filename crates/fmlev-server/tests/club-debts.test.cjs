const { test } = require('node:test');
const assert = require('node:assert/strict');
const { createUi } = require('./helpers/fake-dom.cjs');
const { read } = require('./helpers/ui.cjs');
const ok = data => ({ ok: true, json: async () => data });

const loan = { index: 0, address: '0x131528db0', original_debt: 1000, monthly_repayment: 10, monthly_interest: 2, conditional_repayment: 0,
  start_day: 100, start_year: 2023, end_day: 181, end_year: 2033, loan_type: 1, include_in_ffp: true, interest_only: false };
const sponsor = { index: 0, address: '0x129de5a48', start_day: 182, start_year: 2022, end_day: 181, end_year: 2026, total_value: 400,
  annual_value: 100, type_code: 257, type_base: 1, type_flag_0x100: true, type_label_candidate: 'Kit sponsor', type_label_status: 'unverified', dates_inverted: false };

function app(extraFetch) {
  const fetch = async (url, init) => {
    const custom = extraFetch && extraFetch(url, init && init.body ? JSON.parse(init.body) : undefined);
    if (custom) return custom;
    if (url === '/api/club/loans') return ok({ loans: [loan] });
    if (url === '/api/club/sponsors') return ok({ list_present: true, sponsors: [sponsor] });
    return ok({ message: 'written' });
  };
  const ui = createUi({ fetch, ids: ['club-loans-body', 'club-sponsors-body', 'club-debts-experimental', 'toast-container',
    'loan-0-original_debt', 'loan-0-monthly_repayment', 'loan-0-monthly_interest', 'loan-0-conditional_repayment', 'loan-0-end_year',
    'loan-0-include_in_ffp', 'loan-0-interest_only', 'sponsor-0-annual_value', 'sponsor-0-total_value'] });
  ui.run("state.activeClub = { club_id: 7, club_name: 'Club' }");
  return ui;
}

test('the card is a labelled experimental, unchecked-by-default live editor', () => {
  const html = read('pages/club.html');
  assert.match(html, /id="club-debts-card" data-live-editor="true"/);
  assert.match(html, /<input id="club-debts-experimental" type="checkbox" onchange="toggleClubDebtEditing\(this.checked\)">/);
  assert.match(html, /unverified in game/i);
  assert.match(read('manifest.json'), /club-debts\.js/);
});

test('reads render read-only values, unmapped types and no edit controls until the opt-in is ticked', async () => {
  const ui = app();
  assert.equal(await ui.run('loadClubLoans()'), true);
  assert.equal(await ui.run('loadClubSponsors()'), true);
  assert.deepEqual(ui.requests.map(r => r.url), ['/api/club/loans', '/api/club/sponsors']);
  const loans = ui.els['club-loans-body'].innerHTML;
  assert.match(loans, /Code 1/);
  assert.match(loans, /12\/05\/2033|\d\d\/\d\d\/2033/);
  assert.doesNotMatch(loans, /<input/);
  assert.doesNotMatch(loans, /Apply/);
  const sponsors = ui.els['club-sponsors-body'].innerHTML;
  assert.match(sponsors, /Kit sponsor \(unverified\)/);
  ui.run('toggleClubDebtEditing(true)');
  assert.match(ui.els['club-loans-body'].innerHTML, /applyClubLoan\(0\)/);
  assert.match(ui.els['club-loans-body'].innerHTML, /Zero amounts/);
  assert.doesNotMatch(ui.els['club-loans-body'].innerHTML, /remove|delete/i, 'a loan can never be removed');
  assert.match(ui.els['club-sponsors-body'].innerHTML, /applyClubSponsor\(0\)/);
});

test('unreadable or empty lists show a reason and invent nothing', async () => {
  const ui = app(url => url === '/api/club/loans' ? { ok: false, json: async () => ({ message: 'Loan list vector failed its shape checks' }) }
    : url === '/api/club/sponsors' ? ok({ list_present: false, sponsors: [] }) : null);
  assert.equal(await ui.run('loadClubLoans()'), false);
  assert.match(ui.els['club-loans-body'].innerHTML, /Press Read loans/);
  assert.ok(ui.run('messages.some(m => /shape checks/.test(m.message))'));
  await ui.run('loadClubSponsors()');
  assert.match(ui.els['club-sponsors-body'].innerHTML, /no sponsor list/);
});

test('loan apply sends only changed fields with identity and the experimental flag; unchanged is refused', async () => {
  const ui = app();
  await ui.run('loadClubLoans()');
  ui.run('toggleClubDebtEditing(true)');
  const vals = { original_debt: '1000', monthly_repayment: '55', monthly_interest: '2', conditional_repayment: '0', end_year: '2033' };
  for (const [k, v] of Object.entries(vals)) ui.els[`loan-0-${k}`].value = v;
  ui.els['loan-0-include_in_ffp'].checked = false;
  ui.els['loan-0-interest_only'].checked = false;
  assert.equal(await ui.run('applyClubLoan(0)'), true);
  const write = ui.requests.find(r => r.url === '/api/club/loans/update');
  assert.deepEqual(write.body, { club_id: 7, loan_index: 0, loan_address: '0x131528db0', monthly_repayment: 55, include_in_ffp: false, experimental: true });
  ui.els['loan-0-monthly_repayment'].value = '10';
  ui.els['loan-0-include_in_ffp'].checked = true;
  const before = ui.requests.length;
  assert.equal(await ui.run('applyClubLoan(0)'), false);
  assert.equal(ui.requests.length, before, 'nothing sent when nothing changed');
  ui.els['loan-0-monthly_repayment'].value = '-3';
  assert.equal(await ui.run('applyClubLoan(0)'), false);
});

test('zero amounts sends clear, never a removal, and needs the opt-in', async () => {
  const ui = app();
  await ui.run('loadClubLoans()');
  assert.equal(await ui.run('zeroClubLoan(0)'), false, 'refused while the experimental box is off');
  assert.equal(ui.requests.some(r => r.url === '/api/club/loans/update'), false);
  ui.run('toggleClubDebtEditing(true)');
  assert.equal(await ui.run('zeroClubLoan(0)'), true);
  assert.deepEqual(ui.requests.find(r => r.url === '/api/club/loans/update').body,
    { club_id: 7, loan_index: 0, loan_address: '0x131528db0', clear: true, experimental: true });
});

test('sponsor apply sends changed values only and a declined confirmation sends nothing', async () => {
  const ui = app();
  await ui.run('loadClubSponsors()');
  ui.run('toggleClubDebtEditing(true)');
  ui.els['sponsor-0-annual_value'].value = '150';
  ui.els['sponsor-0-total_value'].value = '400';
  assert.equal(await ui.run('applyClubSponsor(0)'), true);
  assert.deepEqual(ui.requests.find(r => r.url === '/api/club/sponsors/update').body,
    { club_id: 7, sponsor_index: 0, sponsor_address: '0x129de5a48', annual_value: 150, experimental: true });
  const declined = createUi({ fetch: async () => ok({ list_present: true, sponsors: [sponsor] }),
    ids: ['club-sponsors-body', 'sponsor-0-annual_value', 'sponsor-0-total_value'], extra: { confirm: () => false } });
  declined.run("state.activeClub = { club_id: 7 }");
  await declined.run('loadClubSponsors()');
  declined.run('toggleClubDebtEditing(true)');
  declined.els['sponsor-0-annual_value'].value = '150';
  declined.els['sponsor-0-total_value'].value = '400';
  assert.equal(await declined.run('applyClubSponsor(0)'), false);
  assert.equal(declined.requests.some(r => r.url === '/api/club/sponsors/update'), false);
});
