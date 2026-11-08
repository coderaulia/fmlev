    // ---- Club bank loans and sponsor deals (native reads; scalar writes are EXPERIMENTAL and unverified in game). ----
    // Writes need the unchecked "Enable experimental ..." box, send experimental: true and only the fields that changed.
    // The lists are never resized: there is no remove; "Zero amounts" keeps the loan and sets its amounts to 0.
    const clubDebts = { clubId: null, loans: null, sponsors: null, listPresent: true, editing: false };

    function debtDate(day, year) {
      if (!Number.isInteger(day) || !Number.isInteger(year)) return "Unavailable";
      const date = new Date(Date.UTC(year, 0, 1) + (day - 1) * 86400000);
      const pad = n => String(n).padStart(2, "0");
      return `${pad(date.getUTCDate())}/${pad(date.getUTCMonth() + 1)}/${year}`;
    }
    function debtMoney(raw) { return formatMoney(raw); }
    function debtConfirm(message) { return typeof confirm === "function" ? confirm(message) : true; }

    async function clubDebtRequest(url, body) {
      const res = await fetch(url, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
      const data = await readJson(res);
      if (!res.ok) throw new Error(data.message || "Request failed");
      return data;
    }
    function debtClubId() {
      const id = state.activeClub?.club_id;
      if (!id) throw new Error("Select a club first");
      return id;
    }

    function toggleClubDebtEditing(on) {
      clubDebts.editing = !!on;
      renderClubLoans();
      renderClubSponsors();
    }

    function debtInput(id, value, label) {
      return `<input id="${id}" type="number" min="0" class="form-control debt-input" value="${escapeHtml(String(value))}" aria-label="${escapeHtml(label)}" title="Raw GBP">`;
    }

    function renderClubLoans() {
      const box = document.getElementById("club-loans-body");
      if (!box) return;
      const loans = clubDebts.loans;
      if (loans === null) { box.innerHTML = `<p class="unverified-note">Press Read loans to read this club's bank loans from the game.</p>`; return; }
      if (!loans.length) { box.innerHTML = `<p class="hint">This club has no bank loans in the game's data.</p>`; return; }
      const edit = clubDebts.editing;
      const rows = loans.map(l => {
        const i = l.index;
        const cell = (field, label) => edit ? debtInput(`loan-${i}-${field}`, l[field], label) : debtMoney(l[field]);
        return `<tr data-loan-index="${i}">
          <td>${i + 1}</td><td title="Loan type codes are not mapped">Code ${l.loan_type}</td>
          <td>${cell("original_debt", "Original debt")}</td><td>${cell("monthly_repayment", "Monthly repayment")}</td>
          <td>${cell("monthly_interest", "Monthly interest")}</td><td>${cell("conditional_repayment", "Conditional repayment")}</td>
          <td>${debtDate(l.start_day, l.start_year)}</td>
          <td>${edit ? debtInput(`loan-${i}-end_year`, l.end_year, "End year") : debtDate(l.end_day, l.end_year)}</td>
          <td>${edit ? `<label><input id="loan-${i}-include_in_ffp" type="checkbox"${l.include_in_ffp ? " checked" : ""}> FFP</label>
                        <label><input id="loan-${i}-interest_only" type="checkbox"${l.interest_only ? " checked" : ""}> Interest only</label>`
                     : `${l.include_in_ffp ? "In FFP" : "Not in FFP"}${l.interest_only ? ", interest only" : ""}`}</td>
          <td>${edit ? `<button class="btn btn-primary" onclick="applyClubLoan(${i})">Apply</button>
                        <button class="btn" onclick="zeroClubLoan(${i})" title="Sets the amounts to 0; the loan stays in the list">Zero amounts</button>` : ""}</td></tr>`;
      }).join("");
      box.innerHTML = `<table class="debt-table"><thead><tr><th>#</th><th>Type</th><th>Original debt</th><th>Monthly repayment</th><th>Monthly interest</th>
        <th>Conditional repayment</th><th>Start</th><th>End</th><th>Flags</th><th></th></tr></thead><tbody>${rows}</tbody></table>`;
    }

    function renderClubSponsors() {
      const box = document.getElementById("club-sponsors-body");
      if (!box) return;
      const sponsors = clubDebts.sponsors;
      if (sponsors === null) { box.innerHTML = `<p class="unverified-note">Press Read sponsors to read this club's sponsor deals from the game.</p>`; return; }
      if (!clubDebts.listPresent) { box.innerHTML = `<p class="hint">The game holds no sponsor list for this club.</p>`; return; }
      if (!sponsors.length) { box.innerHTML = `<p class="hint">This club has no sponsor deals.</p>`; return; }
      const edit = clubDebts.editing;
      const rows = sponsors.map(s => {
        const i = s.index;
        const label = s.type_label_candidate ? `${escapeHtml(s.type_label_candidate)} (unverified)` : "Unmapped";
        const annual = s.annual_value === null ? "Not stored"
          : edit ? debtInput(`sponsor-${i}-annual_value`, s.annual_value, "Annual value") : debtMoney(s.annual_value);
        const total = edit ? debtInput(`sponsor-${i}-total_value`, s.total_value, "Total value") : debtMoney(s.total_value);
        return `<tr data-sponsor-index="${i}"><td>${i + 1}</td><td title="Type code ${s.type_code}">${label}</td>
          <td>${annual}</td><td>${total}</td>
          <td>${debtDate(s.start_day, s.start_year)}${s.dates_inverted ? " (ends before it starts, as stored)" : ""}</td><td>${debtDate(s.end_day, s.end_year)}</td>
          <td>${edit ? `<button class="btn btn-primary" onclick="applyClubSponsor(${i})">Apply</button>` : ""}</td></tr>`;
      }).join("");
      box.innerHTML = `<table class="debt-table"><thead><tr><th>#</th><th>Type</th><th>Annual value</th><th>Total value</th><th>Start</th><th>End</th><th></th></tr></thead>
        <tbody>${rows}</tbody></table>`;
    }

    async function loadClubLoans() {
      try {
        const clubId = debtClubId();
        const data = await clubDebtRequest("/api/club/loans", { club_id: clubId });
        if (state.activeClub?.club_id !== clubId) return false;
        clubDebts.clubId = clubId;
        clubDebts.loans = data.loans;
        renderClubLoans();
        return true;
      } catch (error) { clubDebts.loans = null; renderClubLoans(); showToast(error.message, "error"); return false; }
    }
    async function loadClubSponsors() {
      try {
        const clubId = debtClubId();
        const data = await clubDebtRequest("/api/club/sponsors", { club_id: clubId });
        if (state.activeClub?.club_id !== clubId) return false;
        clubDebts.clubId = clubId;
        clubDebts.sponsors = data.sponsors;
        clubDebts.listPresent = data.list_present !== false;
        renderClubSponsors();
        return true;
      } catch (error) { clubDebts.sponsors = null; renderClubSponsors(); showToast(error.message, "error"); return false; }
    }

    function debtNumber(id) {
      const el = document.getElementById(id);
      const text = el ? String(el.value).trim() : "";
      if (!/^\d+$/.test(text)) return null;
      const n = Number(text);
      return n <= 2147483647 ? n : null;
    }

    // Only fields that differ from what was read are sent.
    function loanChanges(loan) {
      const i = loan.index;
      const body = {};
      for (const field of ["original_debt", "monthly_repayment", "monthly_interest", "conditional_repayment", "end_year"]) {
        const n = debtNumber(`loan-${i}-${field}`);
        if (n === null) throw new Error(`${field.replace(/_/g, " ")} must be a whole number of 0 or more`);
        if (n !== loan[field]) body[field] = n;
      }
      for (const field of ["include_in_ffp", "interest_only"]) {
        const el = document.getElementById(`loan-${i}-${field}`);
        if (el && !!el.checked !== loan[field]) body[field] = !!el.checked;
      }
      return body;
    }

    async function sendLoanUpdate(loan, changes, summary) {
      const clubId = debtClubId();
      if (!clubDebts.editing) throw new Error("Tick the experimental box first");
      if (!debtConfirm(`${summary}\n\nExperimental: unverified in game. Use a throwaway save.`)) return false;
      const data = await clubDebtRequest("/api/club/loans/update", {
        club_id: clubId, loan_index: loan.index, loan_address: loan.address, ...changes, experimental: true,
      });
      showToast(data.message || "Loan written", "success");
      return loadClubLoans();
    }
    async function applyClubLoan(index) {
      try {
        const loan = clubDebts.loans?.[index];
        if (!loan) throw new Error("Read the loans first");
        const changes = loanChanges(loan);
        if (!Object.keys(changes).length) throw new Error("Nothing changed on this loan");
        return await sendLoanUpdate(loan, changes, `Write ${Object.keys(changes).join(", ")} on loan ${index + 1}?`);
      } catch (error) { showToast(error.message, "error"); return false; }
    }
    async function zeroClubLoan(index) {
      try {
        const loan = clubDebts.loans?.[index];
        if (!loan) throw new Error("Read the loans first");
        return await sendLoanUpdate(loan, { clear: true }, `Set the debt, repayment, interest and conditional repayment of loan ${index + 1} to 0? The loan stays in the list.`);
      } catch (error) { showToast(error.message, "error"); return false; }
    }

    async function applyClubSponsor(index) {
      try {
        const sponsor = clubDebts.sponsors?.[index];
        if (!sponsor) throw new Error("Read the sponsors first");
        if (!clubDebts.editing) throw new Error("Tick the experimental box first");
        const body = {};
        const annual = sponsor.annual_value === null ? null : debtNumber(`sponsor-${index}-annual_value`);
        const total = debtNumber(`sponsor-${index}-total_value`);
        if (sponsor.annual_value !== null) {
          if (annual === null) throw new Error("Annual value must be a whole number of 0 or more");
          if (annual !== sponsor.annual_value) body.annual_value = annual;
        }
        if (total === null) throw new Error("Total value must be a whole number of 0 or more");
        if (total !== sponsor.total_value) body.total_value = total;
        if (!Object.keys(body).length) throw new Error("Nothing changed on this deal");
        if (!debtConfirm(`Write ${Object.keys(body).join(", ")} on sponsor deal ${index + 1}?\n\nExperimental: unverified in game. Use a throwaway save.`)) return false;
        const clubId = debtClubId();
        const data = await clubDebtRequest("/api/club/sponsors/update", {
          club_id: clubId, sponsor_index: index, sponsor_address: sponsor.address, ...body, experimental: true,
        });
        showToast(data.message || "Sponsor written", "success");
        return loadClubSponsors();
      } catch (error) { showToast(error.message, "error"); return false; }
    }
