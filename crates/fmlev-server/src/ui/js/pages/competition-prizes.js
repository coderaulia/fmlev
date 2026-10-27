    // Prize selectors are raw GBP and belong to one successfully loaded competition/table.
    let prizeBaseline = null;
    function prizeSelectors() {
      const read = id => String(document.getElementById(id)?.value ?? "").trim();
      const texts = ["prize-teams", "prize-first", "prize-last", "prize-tol"].map(read);
      const values = texts.map(Number);
      if (!texts[0] || !Number.isInteger(values[0]) || values[0] < 2 || values[0] > 64) throw new Error("Enter a team count from 2 to 64");
      for (const i of [1, 2]) if (!texts[i] || !Number.isInteger(values[i]) || values[i] < 1 || values[i] > 2147483647) throw new Error("Enter the first and last prize in raw GBP");
      if (values[2] > values[1]) throw new Error("Last prize must not exceed the first prize");
      if (!Number.isInteger(values[3]) || values[3] < 0 || values[3] > 1000000) throw new Error("Tolerance must be a whole GBP amount from 0 to 1,000,000");
      return { teams: values[0], first_prize: values[1], last_prize: values[2], tolerance: values[3] };
    }
    function prizeConfigured() { return ["prize-teams", "prize-first", "prize-last", "prize-tol"].some(id => String(document.getElementById(id)?.value ?? "").trim()); }
    function prizeFingerprint(body) { return JSON.stringify(body); }
    async function prizeRequest(prizes, selectors = prizeSelectors()) {
      const res = await fetch("/api/competition/prizes", { method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify(prizes ? { ...selectors, prizes } : selectors) });
      const data = await readJson(res);
      if (!res.ok) throw new Error(data.message || "Prize request failed");
      if (!Array.isArray(data.prizes) || data.prizes.length !== selectors.teams || data.prizes.some(v => !Number.isInteger(v) || v < 0 || v > 2147483647)) throw new Error("Prize response has no valid table");
      return data;
    }
    function renderPrizeAvailability() {
      const input = document.getElementById("prize-list");
      const loaded = !!prizeBaseline && !!state.activeCompetition && prizeBaseline.id === state.activeCompetition.id;
      if (input) { input.disabled = !loaded; if (!loaded) input.value = ""; }
      const status = document.getElementById("prize-load-status");
      if (status) status.innerText = loaded ? "Loaded table. Edits remain pending until Save." : "Enter the matching criteria, then use Load from game in the bottom bar.";
    }
    function acceptPrizeTable(id, selectors, data) {
      prizeBaseline = { id, selectors: { ...selectors }, prizes: [...data.prizes] };
      const input = document.getElementById("prize-list");
      if (input) input.value = data.prizes.join("\n");
      renderPrizeAvailability();
    }
    async function loadPrizeTable() {
      const id = state.activeCompetition?.id;
      if (!id) throw new Error("Select a competition first");
      const selectors = prizeSelectors(), signature = prizeFingerprint(selectors);
      const data = await prizeRequest(undefined, selectors);
      if (state.activeCompetition?.id !== id || signature !== prizeFingerprint(prizeSelectors())) throw new Error("Competition or table criteria changed while loading");
      acceptPrizeTable(id, selectors, data);
      return true;
    }
    function prizeChanges() {
      if (!prizeBaseline || !state.activeCompetition || prizeBaseline.id !== state.activeCompetition.id) return [];
      const input = document.getElementById("prize-list");
      const raw = input?.value || "", values = raw.trim().split(/\s+/).filter(Boolean).map(Number);
      if (values.length === prizeBaseline.prizes.length && values.every((v, i) => v === prizeBaseline.prizes[i])) return [];
      return [{ label: "Prize table (raw GBP)", from: prizeBaseline.prizes.join(", "), to: raw.trim() }];
    }
    async function savePrizeTable() {
      const id = state.activeCompetition?.id;
      if (!prizeBaseline || !id || prizeBaseline.id !== id) throw new Error("Load this competition prize table before editing");
      const selectors = prizeSelectors(), signature = prizeFingerprint(selectors);
      if (signature !== prizeFingerprint(prizeBaseline.selectors)) throw new Error("Prize criteria changed. Load the matching table again before Save.");
      const input = document.getElementById("prize-list"), text = input.value;
      const amounts = text.trim().split(/\s+/).filter(Boolean).map(Number);
      if (amounts.length !== selectors.teams || amounts.some(v => !Number.isInteger(v) || v < 0 || v > 2147483647)) throw new Error(`Enter exactly ${selectors.teams} whole GBP amounts from 0 to 2,147,483,647`);
      if (amounts[0] <= 0 || amounts.at(-1) <= 0 || amounts.at(-1) > amounts[0]) throw new Error("First and last prizes must stay positive, with last no greater than first, so the table can be matched again");
      const data = await prizeRequest(amounts, selectors);
      if (state.activeCompetition?.id !== id) return false;
      const next = { ...selectors, first_prize: data.prizes[0], last_prize: data.prizes.at(-1) };
      prizeBaseline = { id, selectors: next, prizes: [...data.prizes] };
      if (signature === prizeFingerprint(prizeSelectors())) {
        const first = document.getElementById("prize-first"), last = document.getElementById("prize-last");
        if (first) first.value = next.first_prize;
        if (last) last.value = next.last_prize;
      }
      if (input.value === text) input.value = data.prizes.join("\n");
      return prizeChanges().length === 0;
    }
    function competitionPageChanges() {
      const changes = [...competitionFieldChanges(), ...prizeChanges()];
      for (const action of pendingActions.get(`competition:${state.activeCompetition?.id}`) || []) changes.push({ label: action.label });
      return changes;
    }
    async function loadCompetitionPage() {
      const id = state.activeCompetition?.id;
      if (!id) throw new Error("Select a competition first");
      const selectors = prizeConfigured() ? prizeSelectors() : null;
      const epoch = ++competitionReadEpoch;
      const res = await fetch(state.connected ? "/api/competition/native" : "/api/context/select", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ competition_id: id }) });
      const data = await readJson(res), comp = data.competition || data.active_competition;
      if (!res.ok) throw new Error(data.message || "Competition load failed");
      const prizes = selectors ? await prizeRequest(undefined, selectors) : null;
      if (state.activeCompetition?.id !== id || comp?.id !== id || epoch !== competitionReadEpoch) throw new Error("Competition changed while loading");
      if (selectors && prizeFingerprint(selectors) !== prizeFingerprint(prizeSelectors())) throw new Error("Prize criteria changed while loading");
      competitionBaseline = { ...comp }; nativeCompetitionSyncedId = id;
      state.competitionDirty = false;
      pendingActions.delete(`competition:${id}`);
      if (prizes) acceptPrizeTable(id, selectors, prizes); else prizeBaseline = null;
      displayCompetition(comp);
      return true;
    }
    async function discardCompetitionPage() {
      // Discard uses loaded baselines, so changing lookup criteria cannot block reverting edits.
      if (competitionBaseline?.id !== state.activeCompetition?.id) return loadCompetitionPage();
      state.activeCompetition.reputation = competitionBaseline.reputation;
      state.competitionDirty = false;
      pendingActions.delete(`competition:${state.activeCompetition.id}`);
      renderCompetitionValues();
      if (prizeBaseline?.id === state.activeCompetition.id) {
        const s = prizeBaseline.selectors;
        for (const [id, field] of [["prize-teams", "teams"], ["prize-first", "first_prize"], ["prize-last", "last_prize"], ["prize-tol", "tolerance"]]) {
          const input = document.getElementById(id); if (input) input.value = s[field];
        }
        acceptPrizeTable(state.activeCompetition.id, s, { prizes: prizeBaseline.prizes });
      }
      return true;
    }
    async function saveCompetitionPage() {
      const id = state.activeCompetition?.id;
      if (!id) return false;
      const queued = [...(pendingActions.get(`competition:${id}`) || [])];
      const prizeDraft = document.getElementById("prize-list")?.value;
      if (competitionFieldChanges().length && !await writeCompetitionChanges()) return false;
      if (state.activeCompetition?.id !== id || competitionFieldChanges().length) return false;
      if (document.getElementById("prize-list")?.value !== prizeDraft) return false;
      // No reputation update is sent merely because the prize textarea changed.
      state.competitionDirty = false;
      if (prizeChanges().length && !await savePrizeTable()) return false;
      if (state.activeCompetition?.id !== id || competitionFieldChanges().length || prizeChanges().length) return false;
      const queue = pendingActions.get(`competition:${id}`) || [];
      if (queued.length !== queue.length || queued.some((action, i) => action !== queue[i])) return false;
      return saveCompetitionToGame();
    }
    SaveBar.register("competition", { load: loadCompetitionPage, discard: discardCompetitionPage, save: saveCompetitionPage, changes: competitionPageChanges });
