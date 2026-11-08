    // Club transfer embargo (read + EXPERIMENTAL edit) and financial statements (read only).
    // Reads run on Club > Load from game and the card's Read button; the embargo edit is an explicit
    // action button with a confirm dialog (like the transfer actions), never a field-change write.
    let clubFinanceState = { clubId: null, embargo: null, statements: null, period: "this_season" };

    async function clubFinancePost(url, body) {
      const res = await fetch(url, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
      const data = await readJson(res);
      if (!res.ok) throw new Error(data.message || data.error || "Request failed");
      return data;
    }

    function embargoDateText(d) {
      if (!d) return "Unavailable";
      return d.empty ? "None" : `${d.year}, day ${d.day}`;
    }

    function renderClubEmbargo() {
      const e = clubFinanceState.embargo;
      const summary = document.getElementById("ce-summary");
      if (summary) {
        summary.innerText = !e ? "Not loaded" : e.any_set
          ? `Start ${embargoDateText(e.start)} · End ${embargoDateText(e.end)} · Appeal ${embargoDateText(e.appeal)} · Age restriction ${e.age_restriction || "none"} · Flags 0x${e.flags.toString(16)} (${e.flag_names.join(", ") || "none"})`
          : "No embargo (no dates, no flags)";
      }
      updateClubEmbargoButtons();
    }

    function updateClubEmbargoButtons() {
      const on = !!document.getElementById("ce-experimental")?.checked && !!clubFinanceState.embargo;
      for (const id of ["ce-apply", "ce-lift"]) {
        const el = document.getElementById(id);
        if (el) el.disabled = !on;
      }
      for (const id of ["ce-start-year", "ce-start-day", "ce-end-year", "ce-end-day", "ce-appeal-year", "ce-appeal-day", "ce-age", "ce-flags"]) {
        const el = document.getElementById(id);
        if (el) el.disabled = !on;
      }
    }

    function clubStatementFieldRows(fields) {
      return fields.filter(f => f.value !== 0).map(f =>
        `<tr><td>${escapeHtml(f.label)}</td><td class="num">${escapeHtml(formatMoney(f.value, { kind: "money" }))}</td></tr>`).join("")
        || `<tr><td colspan="2" class="unverified-note">All zero</td></tr>`;
    }

    function renderClubStatements() {
      const body = document.getElementById("cs-body");
      if (!body) return;
      const statements = clubFinanceState.statements;
      if (!statements) { body.innerHTML = `<p class="unverified-note">Not loaded</p>`; return; }
      const period = statements.periods.find(p => p.key === clubFinanceState.period) || statements.periods[0];
      if (!period.available) {
        body.innerHTML = `<p class="unverified-note">Unavailable: ${escapeHtml(period.reason || "no data")}</p>`;
        return;
      }
      const unmapped = [...period.unmapped_income, ...period.unmapped_expenditure];
      body.innerHTML = `<div style="display:grid;grid-template-columns:1fr 1fr;gap:12px;">
        <table class="fm-table"><thead><tr><th>Income</th><th class="num">Raw GBP</th></tr></thead><tbody>${clubStatementFieldRows(period.income)}</tbody></table>
        <table class="fm-table"><thead><tr><th>Expenditure</th><th class="num">Raw GBP</th></tr></thead><tbody>${clubStatementFieldRows(period.expenditure)}</tbody></table>
      </div>${period.all_zero ? `<p class="unverified-note">Every value in this period is zero (for example last season in a first-season save).</p>` : ""}
      ${unmapped.length ? `<p class="unverified-note">${unmapped.length} nonzero value(s) have no label yet and are not shown.</p>` : ""}`;
    }

    function selectClubStatementPeriod(key) {
      clubFinanceState.period = key;
      renderClubStatements();
    }

    async function loadClubFinanceExtras(id = state.activeClub?.club_id) {
      if (!id) throw new Error("Select a club first");
      const status = document.getElementById("ce-status");
      if (status) status.innerText = "Reading...";
      clubFinanceState = { ...clubFinanceState, clubId: id, embargo: null, statements: null };
      renderClubEmbargo();
      renderClubStatements();
      const [embargo, statements] = await Promise.allSettled([
        clubFinancePost("/api/club/embargo", { club_id: id }),
        clubFinancePost("/api/club/finances/statements", { club_id: id }),
      ]);
      if (state.activeClub?.club_id !== id) return false;
      const errors = [];
      if (embargo.status === "fulfilled") clubFinanceState.embargo = embargo.value.embargo; else errors.push(`Embargo: ${embargo.reason.message}`);
      if (statements.status === "fulfilled") clubFinanceState.statements = statements.value.statements; else errors.push(`Statements: ${statements.reason.message}`);
      renderClubEmbargo();
      renderClubStatements();
      if (status) status.innerText = errors.length ? `Unavailable. ${errors.join(" ")}` : "Read from the game.";
      return !errors.length;
    }

    function clubEmbargoDateEdit(prefix) {
      const read = id => String(document.getElementById(id)?.value ?? "").trim();
      const year = read(`ce-${prefix}-year`), day = read(`ce-${prefix}-day`);
      if (!year && !day) return undefined;
      if (!year || !day) throw new Error(`Enter both year and day for the ${prefix} date, or leave both empty`);
      const y = Number(year), d = Number(day);
      if (!Number.isInteger(y) || y < 1900 || y > 2200 || !Number.isInteger(d) || d < 1 || d > 366) throw new Error(`The ${prefix} date needs a year 1900-2200 and a day of year 1-366`);
      return { year: y, day: d };
    }

    function clubEmbargoEditBody() {
      const body = {};
      for (const prefix of ["start", "end", "appeal"]) {
        const edit = clubEmbargoDateEdit(prefix);
        if (edit) body[prefix] = edit;
      }
      const ageText = String(document.getElementById("ce-age")?.value ?? "").trim();
      if (ageText) {
        const age = Number(ageText);
        if (!Number.isInteger(age) || age < 0 || age > 40) throw new Error("Age restriction must be a whole number 0-40");
        body.age_restriction = age;
      }
      const flagText = String(document.getElementById("ce-flags")?.value ?? "").trim();
      if (flagText) {
        const flags = Number(flagText);
        if (!Number.isInteger(flags) || flags < 0 || flags > 255 || (flags & 0x40)) throw new Error("Flags must be a whole number 0-255 without bit 0x40");
        body.flags = flags;
      }
      if (!Object.keys(body).length) throw new Error("Fill at least one embargo field");
      return body;
    }

    async function sendClubEmbargoEdit(edit, question) {
      const id = state.activeClub?.club_id;
      if (!id || clubFinanceState.clubId !== id || !clubFinanceState.embargo) throw new Error("Read this club's embargo first");
      if (!document.getElementById("ce-experimental")?.checked) throw new Error("Tick the experimental checkbox first");
      if (typeof confirm === "function" && !confirm(question)) return false;
      const data = await clubFinancePost("/api/club/embargo/update", { club_id: id, ...edit, experimental: true });
      if (state.activeClub?.club_id !== id) return false;
      clubFinanceState.embargo = data.embargo;
      renderClubEmbargo();
      const status = document.getElementById("ce-status");
      if (status) status.innerText = "Written and read back. Experimental: reopen the club in FM26 to check it.";
      showToast("Embargo written (experimental)", "success");
      return true;
    }

    async function applyClubEmbargo() {
      try {
        const edit = clubEmbargoEditBody();
        return await sendClubEmbargoEdit(edit, `Write these embargo fields for club ${state.activeClub?.club_id}? Experimental and unverified in game; keep a backup save.`);
      } catch (error) { showToast(error.message, "error"); return false; }
    }

    async function liftClubEmbargo() {
      try {
        const clear = { clear: true };
        return await sendClubEmbargoEdit({ start: clear, end: clear, appeal: clear, age_restriction: 0, flags: 0 },
          `Clear all embargo dates, the age restriction and the flags for club ${state.activeClub?.club_id}? Experimental and unverified in game; keep a backup save.`);
      } catch (error) { showToast(error.message, "error"); return false; }
    }

    async function readClubFinanceExtras() {
      try { return await loadClubFinanceExtras(); } catch (error) { showToast(error.message, "error"); return false; }
    }
