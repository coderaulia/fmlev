    // Nation controls use native baselines. A missing value never becomes an editable default.
    const NATION_FIELDS = [
      ["nation-youth-slider", "youth_rating", "Youth rating", 1, 200],
      ["nation-reputation", "reputation", "International reputation", 1, 10000],
      ["nation-economic", "economic_factor", "Economic factor", 1, 20],
      ["nation-development", "state_of_development", "Development code", 0, 2],
    ];
    let nativeNationSyncedId = null;
    let nationBaseline = null;
    let nationRead = null;
    let nationReadEpoch = 0;

    function renderNationValues() {
      const nation = state.activeNation;
      const base = nationBaseline?.id === nation?.id ? nationBaseline : null;
      const banner = document.getElementById("nation-live-unavailable");
      if (banner && base) banner.hidden = true;
      for (const [id, field, label, min, max] of NATION_FIELDS) {
        const input = document.getElementById(id);
        const value = base?.[field];
        const valid = Number.isInteger(value) && value >= min && value <= max;
        if (input) {
          input.disabled = !valid;
          input.value = valid ? value : "";
          input.title = valid ? label : "Native value unavailable. Use Load from game to retry.";
          for (const button of input.parentElement?.querySelectorAll?.("button") || []) button.disabled = !valid;
        }
      }
      const text = (id, value) => { const el = document.getElementById(id); if (el) el.innerText = value; };
      const rep = base?.reputation;
      text("nation-rep-num", rep > 0 ? `${rep.toLocaleString()} / 10,000` : "Unavailable");
      text("nation-rep-stars", rep > 0 ? getReputationStars(rep) : "—");
      updateYouthRatingDisplay(base?.youth_rating);
      setNationStatUI("nation-economic", "disp-nation-econ", base?.economic_factor);
      const population = document.getElementById("nation-population");
      if (population) { population.value = nation?.population > 0 ? nation.population : ""; population.disabled = true; }
    }

    function displayNation(nation) {
      state.activeNation = JSON.parse(JSON.stringify(nation));
      if (!state.connected) nationBaseline = { ...nation };
      displayEntityAvailability("nation", nation);
      const set = (id, value) => { const el = document.getElementById(id); if (el) el.innerText = value; };
      set("nation-name", nation.name || "Nation unavailable");
      set("nation-code", nation.three_letter_code || "—");
      set("nation-continent", nation.continent || "Unavailable");
      set("nation-flag-avatar", getCountryFlag(nation.name));
      if (nationBaseline?.id === nation.id) for (const [, field] of NATION_FIELDS) state.activeNation[field] = nationBaseline[field];
      renderNationValues();
      Shell.setContext({ section: "nation", kind: "nation", id: nation.id, title: nation.name || "Nation unavailable", subtitle: nation.continent || "" });
      refreshPendingActionIndicators();
    }

    async function syncNationNative(id) {
      if (!state.connected || !id || state.nationDirty) return false;
      if (nationBaseline?.id === id && nativeNationSyncedId === id) return true;
      if (nationRead?.id === id) return nationRead.promise;
      const epoch = nationReadEpoch;
      const promise = (async () => {
        try {
          const res = await fetch("/api/nation/native", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ nation_id: id }) });
          const data = await readJson(res);
          if (!res.ok) throw new Error(data.message || "Native nation read failed");
          if (state.activeNation?.id !== id || state.nationDirty || epoch !== nationReadEpoch) return false;
          if (data.nation?.id !== id) throw new Error("Native nation identity did not match");
          nationBaseline = { ...data.nation };
          nativeNationSyncedId = id;
          for (const [, field] of NATION_FIELDS) state.activeNation[field] = data.nation[field];
          renderNationValues();
          return true;
        } catch (error) { nativeNationSyncedId = null; return false; }
      })();
      nationRead = { id, promise };
      try { return await promise; } finally { if (nationRead?.promise === promise) nationRead = null; }
    }

    function setNationStatUI(id, dispId, value) {
      const valid = Number.isInteger(value) && value >= 1 && value <= 20;
      const input = document.getElementById(id), display = document.getElementById(dispId);
      if (input) input.value = valid ? value : "";
      if (display) { display.innerText = valid ? String(value) : "—"; display.className = `facility-val ${valueBand(value)}`; }
    }
    function markNationDirty() { state.nationDirty = true; SaveBar.markDirty("nation"); }
    function debouncedSaveNation() { markNationDirty(); }
    function handleYouthRatingInput(value) { markNationDirty(); updateYouthRatingDisplay(Number(value)); }
    function handleNationRepChange(value) {
      markNationDirty();
      const rep = Number(value);
      const text = document.getElementById("nation-rep-num"), stars = document.getElementById("nation-rep-stars");
      if (text) text.innerText = Number.isInteger(rep) && rep > 0 && rep <= 10000 ? `${rep.toLocaleString()} / 10,000` : "Unavailable";
      if (stars) stars.innerText = rep > 0 && rep <= 10000 ? getReputationStars(rep) : "—";
    }
    function updateYouthRatingDisplay(value) {
      const valid = Number.isInteger(value) && value >= 1 && value <= 200;
      const display = document.getElementById("nation-youth-val"), fill = document.getElementById("nation-youth-gauge-fill"), badge = document.getElementById("nation-youth-tier");
      if (display) { display.innerText = valid ? String(value) : "—"; display.className = valueBand(valid ? value : null, 200); }
      if (fill) fill.style.width = valid ? `${value / 2}%` : "0%";
      if (badge) badge.innerText = valid ? "Native rating · 1–200" : "Native value unavailable";
    }
    function stepNationStat(key, delta) {
      const field = NATION_FIELDS.find(([, name]) => name === key);
      if (!field) return;
      const [id, , , min, max] = field;
      const input = document.getElementById(id);
      if (!input || input.disabled || input.value === "") return;
      input.value = Math.max(min, Math.min(max, Number(input.value) + delta));
      markNationDirty();
      if (key === "youth_rating") updateYouthRatingDisplay(Number(input.value));
      if (key === "economic_factor") setNationStatUI(id, "disp-nation-econ", Number(input.value));
    }
    function nationFieldChanges() {
      if (!nationBaseline || !state.activeNation || nationBaseline.id !== state.activeNation.id) return [];
      return NATION_FIELDS.flatMap(([id, field, label]) => {
        const input = document.getElementById(id);
        if (!input || input.disabled || Number(input.value) === nationBaseline[field] && input.value !== "") return [];
        return [{ label, field, from: nationBaseline[field], to: input.value }];
      });
    }
    function nationPageChanges() {
      const changes = nationFieldChanges();
      for (const action of pendingActions.get(`nation:${state.activeNation?.id}`) || []) changes.push({ label: action.label });
      return changes;
    }
    async function writeNationChanges() {
      if (!state.activeNation || nationBaseline?.id !== state.activeNation.id) return false;
      const changes = nationFieldChanges();
      if (!changes.length) { state.nationDirty = false; return true; }
      const id = state.activeNation.id, submitted = { ...state.activeNation };
      nationReadEpoch++;
      for (const change of changes) {
        const [, , label, min, max] = NATION_FIELDS.find(([, name]) => name === change.field);
        const value = Number(change.to);
        if (change.to === "" || !Number.isInteger(value) || value < min || value > max) throw new Error(`${label} must be a whole number from ${min} to ${max}`);
        submitted[change.field] = value;
      }
      const res = await fetch("/api/nation/update", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(submitted) });
      const data = await readJson(res);
      if (!res.ok) throw new Error(data.message || "Nation save failed");
      if (state.activeNation?.id !== id) return false;
      for (const change of changes) nationBaseline[change.field] = submitted[change.field];
      Object.assign(state.activeNation, submitted);
      state.nationDirty = nationFieldChanges().length > 0;
      return !state.nationDirty;
    }
    async function loadNationPage() {
      const id = state.activeNation?.id;
      if (!id) throw new Error("Select a nation first");
      const epoch = ++nationReadEpoch;
      const res = await fetch(state.connected ? "/api/nation/native" : "/api/context/select", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ nation_id: id }) });
      const data = await readJson(res), nation = data.nation || data.active_nation;
      if (!res.ok) throw new Error(data.message || "Nation load failed");
      if (state.activeNation?.id !== id || nation?.id !== id || epoch !== nationReadEpoch) throw new Error("Nation changed while loading; retry for the selected nation");
      nationBaseline = { ...nation }; nativeNationSyncedId = id;
      state.nationDirty = false;
      pendingActions.delete(`nation:${id}`);
      displayNation(nation);
      return true;
    }
    async function applyNationPreset(preset) { return queuePreset("nation", preset); }
    SaveBar.register("nation", { load: loadNationPage, discard: loadNationPage, save: () => saveNationToGame(), changes: nationPageChanges });
