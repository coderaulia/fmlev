    // Competition reputation is editable; rule and prize support have separate guarded paths.
    let nativeCompetitionSyncedId = null;
    let competitionBaseline = null;
    let competitionRead = null;
    let competitionReadEpoch = 0;
    function renderCompetitionValues() {
      const comp = state.activeCompetition;
      const value = competitionBaseline?.id === comp?.id ? competitionBaseline.reputation : null;
      const valid = Number.isInteger(value) && value >= 1 && value <= 10000;
      const banner = document.getElementById("competition-live-unavailable");
      if (banner && valid) banner.hidden = true;
      const input = document.getElementById("comp-reputation");
      if (input) { input.value = valid ? value : ""; input.disabled = !valid; }
      const set = (id, text) => { const el = document.getElementById(id); if (el) el.innerText = text; };
      set("comp-rep-num", valid ? `${value.toLocaleString()} / 10,000` : "Unavailable");
      set("comp-rep-stars", valid ? getReputationStars(value) : "—");
      set("competition-rule-status", "Match and squad rule edits are unavailable here. Registration actions are staged below.");
      const live = state.liveCompetitions?.[comp?.id];
      set("competition-info-teams", live?.number_of_teams > 0 ? live.number_of_teams : "Unavailable");
      set("competition-info-season", live?.season_name || "Unavailable");
    }
    function displayCompetition(comp) {
      state.activeCompetition = JSON.parse(JSON.stringify(comp));
      if (!state.connected) competitionBaseline = { ...comp };
      displayEntityAvailability("competition", comp);
      const set = (id, text) => { const el = document.getElementById(id); if (el) el.innerText = text; };
      set("comp-name", comp.name || "Competition unavailable");
      set("comp-short-name", comp.short_name || "—");
      set("comp-nation", comp.nation_name || "Unavailable");
      if (competitionBaseline?.id === comp.id) state.activeCompetition.reputation = competitionBaseline.reputation;
      renderCompetitionValues();
      // A prize baseline is bound to the competition and cannot follow a context switch.
      if (typeof renderPrizeAvailability === "function") renderPrizeAvailability();
      Shell.setContext({ section: "competition", kind: "competition", id: comp.id, title: comp.name || "Competition unavailable", subtitle: comp.nation_name || "" });
      refreshPendingActionIndicators();
    }
    async function syncCompetitionNative(id) {
      if (!state.connected || !id || state.competitionDirty) return false;
      if (competitionBaseline?.id === id && nativeCompetitionSyncedId === id) return true;
      if (competitionRead?.id === id) return competitionRead.promise;
      const epoch = competitionReadEpoch;
      const promise = (async () => {
        try {
          const res = await fetch("/api/competition/native", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ competition_id: id }) });
          const data = await readJson(res);
          if (!res.ok || data.competition?.id !== id) throw new Error(data.message || "Native competition read failed");
          if (state.activeCompetition?.id !== id || state.competitionDirty || epoch !== competitionReadEpoch) return false;
          competitionBaseline = { ...data.competition };
          nativeCompetitionSyncedId = id;
          state.activeCompetition.reputation = data.competition.reputation;
          renderCompetitionValues();
          return true;
        } catch (error) { nativeCompetitionSyncedId = null; return false; }
      })();
      competitionRead = { id, promise };
      try { return await promise; } finally { if (competitionRead?.promise === promise) competitionRead = null; }
    }
    function handleCompRepChange(value) {
      state.competitionDirty = true; SaveBar.markDirty("competition");
      const rep = Number(value), text = document.getElementById("comp-rep-num"), stars = document.getElementById("comp-rep-stars");
      if (text) text.innerText = Number.isInteger(rep) && rep > 0 && rep <= 10000 ? `${rep.toLocaleString()} / 10,000` : "Unavailable";
      if (stars) stars.innerText = rep > 0 && rep <= 10000 ? getReputationStars(rep) : "—";
    }
    function competitionFieldChanges() {
      const input = document.getElementById("comp-reputation");
      if (!competitionBaseline || !state.activeCompetition || competitionBaseline.id !== state.activeCompetition.id || !input || input.disabled) return [];
      if (input.value !== "" && Number(input.value) === competitionBaseline.reputation) return [];
      return [{ label: "Competition reputation", from: competitionBaseline.reputation, to: input.value }];
    }
    async function writeCompetitionChanges() {
      const changes = competitionFieldChanges();
      if (!changes.length) { state.competitionDirty = false; return true; }
      const input = document.getElementById("comp-reputation"), value = Number(input.value);
      if (input.value === "" || !Number.isInteger(value) || value < 1 || value > 10000) throw new Error("Reputation must be a whole number from 1 to 10,000");
      const id = state.activeCompetition.id, submitted = { ...state.activeCompetition, reputation: value };
      competitionReadEpoch++;
      const res = await fetch("/api/competition/update", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(submitted) });
      const data = await readJson(res);
      if (!res.ok) throw new Error(data.message || "Competition save failed");
      if (state.activeCompetition?.id !== id) return false;
      competitionBaseline.reputation = value;
      state.activeCompetition.reputation = value;
      state.competitionDirty = competitionFieldChanges().length > 0;
      return !state.competitionDirty;
    }
    function stageCompetitionRegistration(enabled) {
      const comp = state.activeCompetition, live = state.liveCompetitions?.[comp?.id];
      if (!comp || !live?.unique_id) { showToast("Open the competition Rules page in FM26 first", "warning"); return false; }
      const key = `competition:${comp.id}`;
      // The latest desired state replaces an older staged registration request.
      const queue = (pendingActions.get(key) || []).filter(action => action.url !== "/api/competition/registration");
      if (queue.length) pendingActions.set(key, queue); else pendingActions.delete(key);
      queuePendingAction("competition", "/api/competition/registration", { comp_index: comp.id, comp_uid: live.unique_id, enabled },
        enabled ? "Allow registration anytime" : "Restore registration rules");
      SaveBar.markDirty("competition");
      return true;
    }
    // Kept for legacy callers. Unmapped rules do not acquire a write path through this page.
    function stepCompField() { showToast("Rule editing is unavailable", "warning"); }
    function handleVarToggle() {}
    async function applyCompetitionPreset(preset) { return queuePreset("competition", preset); }
