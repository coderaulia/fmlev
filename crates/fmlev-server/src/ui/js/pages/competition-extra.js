    // Competition extras and awards. Native reads; edits require opt-in and confirmation.
    // Each editor has its own Apply button.
    let compxDetailsBaseline = null;
    let compxPrizeBaseline = null;
    let compxAward = null;
    const COMPX_DETAIL_FIELDS = [
      ["original_reputation", "compx-orig-rep"], ["trophy_type", "compx-trophy"], ["lowest_category", "compx-lowcat"],
      ["pitch_min_length", "compx-min-length"], ["pitch_min_width", "compx-min-width"],
      ["pitch_max_length", "compx-max-length"], ["pitch_max_width", "compx-max-width"],
    ];
    function compxSetText(id, text) { const el = document.getElementById(id); if (el) el.innerText = text; }
    function compxEnabled() { return !!document.getElementById("compx-enable")?.checked; }
    function compxIdentity() {
      const comp = state.activeCompetition, live = state.liveCompetitions?.[comp?.id];
      if (!comp?.id) throw new Error("Select a competition first");
      if (!live?.unique_id) throw new Error("Open this competition's Rules page in FM26 once so its unique id is known");
      return { comp_index: comp.id, comp_uid: live.unique_id };
    }
    function compxMatches(identity) {
      return !!identity && identity.comp_index === state.activeCompetition?.id
        && identity?.comp_uid === state.liveCompetitions?.[identity?.comp_index]?.unique_id;
    }
    async function compxPost(url, body) {
      const res = await fetch(url, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
      const data = await readJson(res);
      if (!res.ok) throw new Error(data.message || `Request failed (${res.status})`);
      return data;
    }
    function compxUtf8Length(text) { return unescape(encodeURIComponent(String(text))).length; }
    function compxConfirm(text) { return typeof confirm !== "function" || confirm(text); }
    function compxFail(error) { compxSetText("compx-status", error.message); showToast(error.message, "error"); return false; }
    function compxRenderEnabled() {
      const on = compxEnabled();
      const details = on && compxMatches(compxDetailsBaseline?.identity);
      const prizes = on && compxMatches(compxPrizeBaseline?.identity);
      const set = (id, enabled) => { const el = document.getElementById(id); if (el) el.disabled = !enabled; };
      set("compx-apply-details", details);
      set("compx-apply-prizes", prizes);
      set("compx-apply-award", on && !!compxAward);
      for (const [field, id] of COMPX_DETAIL_FIELDS) set(id, details && compxDetailsBaseline?.[field] != null);
      for (const id of ["compx-prize-list"]) set(id, prizes);
      for (const [id, key] of [["compx-award-name", "name"], ["compx-award-short", "short_name"], ["compx-award-rep-a", "reputation_candidate_a"], ["compx-award-rep-b", "reputation_candidate_b"]]) {
        set(id, on && compxAward?.[key]?.value != null);
      }
    }
    function compxFillDetails(details, identity) {
      compxDetailsBaseline = { identity };
      const notes = [];
      for (const [field, id] of COMPX_DETAIL_FIELDS) {
        const f = details[field], input = document.getElementById(id);
        compxDetailsBaseline[field] = f?.value ?? null;
        if (input) input.value = f?.value ?? "";
        if (f?.value == null && f?.reason) notes.push(`${field}: ${f.reason}`);
      }
      compxSetText("compx-status", notes.length ? `Loaded. Unavailable: ${notes.join("; ")}` : "Details loaded from game memory (read live, edits unverified in game).");
      compxRenderEnabled();
    }
    async function compxLoadDetails() {
      try {
        const identity = compxIdentity();
        const data = await compxPost("/api/competition/details", identity);
        if (!compxMatches(identity)) return false;
        compxFillDetails(data.details, identity);
        return true;
      } catch (error) { return compxFail(error); }
    }
    function compxDetailEdits() {
      const edits = {};
      for (const [field, id] of COMPX_DETAIL_FIELDS) {
        const input = document.getElementById(id);
        if (!input || input.disabled || compxDetailsBaseline?.[field] == null) continue;
        const text = String(input.value).trim();
        if (text === "" || !Number.isInteger(Number(text))) throw new Error(`${field.replace(/_/g, " ")} must be a whole number`);
        if (Number(text) !== compxDetailsBaseline[field]) edits[field] = Number(text);
      }
      return edits;
    }
    async function compxApplyDetails() {
      try {
        if (!compxEnabled()) throw new Error("Tick the experimental checkbox first");
        const identity = compxDetailsBaseline?.identity;
        if (!compxMatches(identity)) throw new Error("Read details for the selected competition first");
        const edits = compxDetailEdits();
        if (!Object.keys(edits).length) throw new Error("Nothing changed");
        if (!compxConfirm(`Write ${Object.keys(edits).join(", ")} to game memory? This is unverified in game; use a throwaway save.`)) return false;
        const data = await compxPost("/api/competition/details/update", { ...identity, ...edits, experimental: true });
        if (!compxMatches(identity)) return true;
        compxFillDetails(data.details, identity);
        compxSetText("compx-status", `${data.message} (experimental, unverified in game)`);
        showToast("Competition details written", "success");
        return true;
      } catch (error) { return compxFail(error); }
    }
    async function compxLoadPrizes() {
      const previous = compxPrizeBaseline;
      let identity;
      try {
        identity = compxIdentity();
        const data = await compxPost("/api/competition/prizes/native", identity);
        if (!compxMatches(identity)) return false;
        const table = data.tables?.[0];
        if (!table) throw new Error("No native prize table for this competition");
        compxPrizeBaseline = { identity, stage_index: table.stage_index, amounts: [...table.amounts], several: data.tables.length > 1 };
        const input = document.getElementById("compx-prize-list");
        if (input) input.value = table.amounts.join("\n");
        compxSetText("compx-prize-note", `${table.positions} positions at stage ${table.stage_index}${compxPrizeBaseline.several ? " (several league stages; only the first is shown)" : ""}. Read from memory, season-end payout source unconfirmed.`);
        compxRenderEnabled();
        return true;
      } catch (error) {
        if (compxPrizeBaseline === previous && (!identity || compxMatches(identity))) {
          compxPrizeBaseline = null;
          compxRenderEnabled();
          compxSetText("compx-prize-note", `Unavailable: ${error.message}`);
        }
        return compxFail(error);
      }
    }
    async function compxApplyPrizes() {
      try {
        if (!compxEnabled()) throw new Error("Tick the experimental checkbox first");
        const baseline = compxPrizeBaseline;
        if (!compxMatches(baseline?.identity)) throw new Error("Read the native prize table for the selected competition first");
        const raw = document.getElementById("compx-prize-list")?.value || "";
        const amounts = raw.trim().split(/\s+/).filter(Boolean).map(Number);
        if (amounts.length !== baseline.amounts.length || amounts.some(v => !Number.isInteger(v) || v < 0 || v > 2000000000)) {
          throw new Error(`Enter exactly ${baseline.amounts.length} whole GBP amounts from 0 to 2,000,000,000`);
        }
        if (amounts.every((v, i) => v === baseline.amounts[i])) throw new Error("Nothing changed");
        if (!compxConfirm("Write the prize table to game memory? The edited value can appear in Rules. The actual season-end payout is unconfirmed.")) return false;
        const data = await compxPost("/api/competition/prizes/native/update", { ...baseline.identity, stage_index: baseline.stage_index, prizes: amounts, experimental: true });
        if (!compxMatches(baseline.identity) || compxPrizeBaseline !== baseline) return true;
        compxPrizeBaseline = { ...baseline, amounts: [...data.table.amounts] };
        const input = document.getElementById("compx-prize-list");
        if (input) input.value = data.table.amounts.join("\n");
        compxSetText("compx-prize-note", data.message);
        showToast("Prize table written", "success");
        return true;
      } catch (error) { return compxFail(error); }
    }
    function compxRenderTable(tables) {
      const body = document.getElementById("compx-table-body");
      if (!body) return;
      const rows = tables[0]?.rows || [];
      body.innerHTML = rows.map(r => {
        const club = r.club_name ? escapeHtml(r.club_name) : `<span class="unavailable">Unavailable</span>`;
        const flag = r.consistent ? "" : ` title="Points or played do not match won/drawn/lost (a deduction, or an unmapped layout)"`;
        return `<tr><td class="num">${r.position}</td><td>${club}</td><td class="num">${r.played}</td><td class="num">${r.won}</td><td class="num">${r.drawn}</td><td class="num">${r.lost}</td><td class="num">${r.goals_for}</td><td class="num">${r.goals_against}</td><td class="num"${flag}>${r.points}${r.consistent ? "" : " *"}</td></tr>`;
      }).join("");
      compxSetText("compx-table-note", `${rows.length} teams${tables.length > 1 ? `, first of ${tables.length} league stages` : ""}. Read-only, heuristic row layout.`);
    }
    async function compxLoadTable() {
      try {
        const data = await compxPost("/api/competition/table", compxIdentity());
        compxRenderTable(data.tables || []);
        return true;
      } catch (error) {
        compxSetText("compx-table-note", `Unavailable: ${error.message}`);
        return compxFail(error);
      }
    }
    async function compxSearchAwards() {
      try {
        const q = String(document.getElementById("compx-award-query")?.value || "").trim();
        const res = await fetch(`/api/awards?q=${encodeURIComponent(q)}&limit=25`);
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || `Request failed (${res.status})`);
        const list = document.getElementById("compx-award-results");
        if (list) {
          list.innerHTML = (data.awards || []).map(a =>
            `<li><button class="btn btn-sm" onclick="compxPickAward(${Number(a.index)}, ${Number(a.uid)})">${escapeHtml(a.name || "Unnamed")}</button> <span class="hint">#${Number(a.index)} · uid ${Number(a.uid)}</span></li>`).join("");
        }
        compxSetText("compx-award-status", `${data.total} award(s) match${data.total > 25 ? " (first 25 shown)" : ""}.`);
        return true;
      } catch (error) { return compxFail(error); }
    }
    function compxFillAward(award) {
      compxAward = award;
      const set = (id, f) => { const el = document.getElementById(id); if (el) el.value = f?.value ?? ""; };
      set("compx-award-name", award.name); set("compx-award-short", award.short_name);
      set("compx-award-rep-a", award.reputation_candidate_a); set("compx-award-rep-b", award.reputation_candidate_b);
      const shared = award.names_share_buffer ? " Name and short name share one buffer (editing one changes both)." : "";
      compxSetText("compx-award-status", `Award #${award.index} (uid ${award.uid}). Names can only be shortened or kept the same length.${shared}`);
      compxRenderEnabled();
    }
    async function compxPickAward(index, uid) {
      try {
        const data = await compxPost("/api/awards/get", { index, uid });
        compxFillAward(data.award);
        return true;
      } catch (error) { return compxFail(error); }
    }
    async function compxApplyAward() {
      try {
        if (!compxEnabled()) throw new Error("Tick the experimental checkbox first");
        if (!compxAward) throw new Error("Pick an award first");
        const body = { index: compxAward.index, uid: compxAward.uid, experimental: true };
        const read = id => String(document.getElementById(id)?.value ?? "");
        for (const [key, id] of [["name", "compx-award-name"], ["short_name", "compx-award-short"]]) {
          const value = read(id);
          if (compxAward[key]?.value != null && value !== compxAward[key].value) {
            if (!value.trim()) throw new Error("A name cannot be empty");
            if (compxUtf8Length(value) > compxUtf8Length(compxAward[key].value)) throw new Error("A name cannot be longer than the current one");
            body[key] = value;
          }
        }
        for (const [key, id] of [["reputation_candidate_a", "compx-award-rep-a"], ["reputation_candidate_b", "compx-award-rep-b"]]) {
          const text = read(id).trim();
          if (compxAward[key]?.value == null || text === String(compxAward[key].value)) continue;
          const n = Number(text);
          if (text === "" || !Number.isInteger(n) || n < 0 || n > 200) throw new Error("Reputation candidates are whole numbers from 0 to 200");
          body[key] = n;
        }
        if (Object.keys(body).length === 3) throw new Error("Nothing changed");
        if (!compxConfirm("Write this award to game memory? Unverified in game; use a throwaway save.")) return false;
        const data = await compxPost("/api/awards/update", body);
        compxFillAward(data.award);
        compxSetText("compx-award-status", `${data.message} (experimental, unverified in game)`);
        showToast("Award written", "success");
        return true;
      } catch (error) { return compxFail(error); }
    }
