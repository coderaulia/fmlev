    // God Mode Actions
    async function applyGodModeAll() {
      showToast("Preparing pending presets across all entities...", "info");
      const actions = [
        ...(state.activePlayer ? [() => applyPlayerPreset("goat"), applyBothFeetGodMode] : []),
        ...(state.activeClub ? [() => applyClubPreset("heal_squad"), () => applyClubPreset("max_facilities")] : [])
      ];
      if (!actions.length) { showToast("Select a player or club first", "warning"); return false; }
      for (const action of actions) {
        if (!await action()) return false;
      }
      showToast("Presets staged. Press Save on each affected page to apply them.", "info");
      return true;
    }

    async function applyBothFeetGodMode() {
      if (!state.activePlayer) return;
      if (!state.activePlayer.positions) state.activePlayer.positions = {};
      state.activePlayer.positions.left_foot = 20;
      state.activePlayer.positions.right_foot = 20;
      renderPositionsTab();
      state.playerDirtyFields.add("positions");
      SaveBar.markDirty("player");
      return true;
    }

    async function applyInvincibleBoard() {
      if (!state.activeClub) return;
      state.activeClub.board_patience = 20;
      const bp = document.getElementById("club-board-patience");
      if (bp) {
        bp.value = 20;
        updateDisp("disp-patience", 20);
      }
      const fc = document.getElementById("club-fan-confidence");
      if (fc) fc.value = 100;
      const bc = document.getElementById("club-board-confidence");
      if (bc) bc.value = 100;
      const tr = document.getElementById("club-transfer-retention");
      if (tr) tr.value = 100;
      updateDispPercent("disp-fan-conf", 100);
      updateDispPercent("disp-board-conf", 100);
      updateDispPercent("disp-retention", 100);
      state.clubDirty = true;
      SaveBar.markDirty("club");
      showToast("Board changes pending. Press Save on the Club page.", "info");
      return true;
    }

    // ---- Cheats built only from verified native writes. They stage values in the same inputs a manual edit
    // uses; nothing is written until Save on the page they affect, so every write keeps its readback and rollback.
    const CHEAT_WEALTH_BALANCE = 1000000000;
    const CHEAT_WEALTH_BUDGET = 500000000;
    const CHEAT_PRIZE_FACTOR = 10;
    const CHEAT_WONDERKID_PA = 195;
    const INT32_MAX = 2147483647;

    // Club balance and transfer budget inputs, only when "Load from game" has read this club's native money.
    function clubMoneyInputs() {
      const club = state.activeClub;
      const loaded = state.loadedMoney && club && state.loadedMoney.club_id === club.club_id;
      const balance = document.getElementById("club-money-balance"), budget = document.getElementById("club-money-transfer");
      if (!loaded || !balance || !budget || balance.disabled || budget.disabled) return null;
      return { balance, budget, club };
    }

    function stageClubMoney(compute, nothingToDo, done) {
      if (!state.activeClub) { showToast("Select a club first", "warning"); return false; }
      const inputs = clubMoneyInputs();
      if (!inputs) { showToast("Open the Club page and press Load in the bottom bar first: the native balance has to be read before it can change.", "warning"); return false; }
      const current = { balance: readMoneyInput(inputs.balance, null), budget: readMoneyInput(inputs.budget, null) };
      if (!Number.isInteger(current.balance) || !Number.isInteger(current.budget)) { showToast("Club money is not loaded.", "warning"); return false; }
      const next = compute(current);
      if (next.balance === current.balance && next.budget === current.budget) { showToast(nothingToDo, "info"); return false; }
      setMoneyInput(inputs.balance, next.balance);
      setMoneyInput(inputs.budget, next.budget);
      SaveBar.markDirty("club");
      showToast(done, "info");
      return true;
    }

    // Raises a negative balance to 0. Loans, debentures and interest are not stored in a verified field, so they are not touched.
    async function applyWipeDebt() {
      if (!state.connected) return applyClubPreset("wipe_debt");
      return stageClubMoney(
        c => ({ balance: Math.max(c.balance, 0), budget: c.budget }),
        "The balance is not negative; nothing to wipe.",
        "Balance raised to 0. Press Save on the Club page."
      );
    }

    // Large, not infinite: raises balance and transfer budget to fixed amounts (never lowers them). Benefactor status is not written.
    async function applyInfiniteWealth() {
      if (!state.connected) return applyClubPreset("sugar_daddy");
      return stageClubMoney(
        c => ({ balance: Math.min(Math.max(c.balance, CHEAT_WEALTH_BALANCE), INT32_MAX), budget: Math.min(Math.max(c.budget, CHEAT_WEALTH_BUDGET), INT32_MAX) }),
        "Balance and transfer budget are already at or above these amounts.",
        "Balance and transfer budget raised. Press Save on the Club page."
      );
    }

    // Multiplies every amount of the loaded prize table (Competition page) by 10, capped at the 32-bit limit.
    async function applyMassivePrizeMoney() {
      if (!state.activeCompetition) { showToast("Select a competition first", "warning"); return false; }
      if (!state.connected) {
        state.activeCompetition.winner_prize_money = 500000000;
        state.activeCompetition.tv_rights_pool = 3000000000;
        const pm = document.getElementById("comp-prize-money");
        if (pm) setMoneyInput(pm, 500000000);
        const tv = document.getElementById("comp-tv-rights");
        if (tv) setMoneyInput(tv, 3000000000);
        state.competitionDirty = true;
        SaveBar.markDirty("competition");
        showToast("Prize changes pending. Press Save on the Competition page.", "info");
        return true;
      }
      const list = document.getElementById("prize-list");
      const loaded = typeof prizeBaseline !== "undefined" && prizeBaseline && prizeBaseline.id === state.activeCompetition.id;
      if (!list || list.disabled || !loaded) { showToast("Load this competition's prize table first (Competition page: enter the Rules-page criteria, then Load in the bottom bar).", "warning"); return false; }
      const amounts = list.value.trim().split(/\s+/).filter(Boolean).map(Number);
      if (!amounts.length || amounts.some(v => !Number.isInteger(v) || v < 0)) { showToast("The prize table is not loaded.", "warning"); return false; }
      const next = amounts.map(v => Math.min(v * CHEAT_PRIZE_FACTOR, INT32_MAX));
      if (next.every((v, i) => v === amounts[i])) { showToast("The prize table is already at the cap.", "info"); return false; }
      list.value = next.join("\n");
      SaveBar.markDirty("competition");
      showToast(`Prize table x${CHEAT_PRIZE_FACTOR} pending. Press Save on the Competition page; the season-end payout source is unconfirmed.`, "info");
      return true;
    }

    // Raises potential ability to 195 (never lowers it) through the normal PA edit; CA and attributes are untouched.
    async function applyWonderkid() {
      const player = state.activePlayer;
      if (!player) { showToast("Select a player first", "warning"); return false; }
      if (!state.connected) return applyPlayerPreset("wonderkid");
      if (!playerFieldWritable("potential_ability")) { showToast("Potential ability cannot be written for this player.", "warning"); return false; }
      if (player.potential_ability >= CHEAT_WONDERKID_PA) { showToast(`Potential ability is already ${player.potential_ability}.`, "info"); return false; }
      handlePAChange(String(CHEAT_WONDERKID_PA));
      showToast(`Potential ability ${CHEAT_WONDERKID_PA} pending. Press Save on the Player page.`, "info");
      return true;
    }

    async function applyCleanNationGovernance() {
      if (!state.activeNation) return;
      state.activeNation.corruption_index = 0;
      state.activeNation.game_importance = 20;
      const nc = document.getElementById("nation-corruption");
      if (nc) nc.value = 0;
      const ng = document.getElementById("nation-game-importance");
      if (ng) ng.value = 20;
      updateDisp("disp-nation-corruption", 0);
      updateDisp("disp-nation-importance", 20);
      state.nationDirty = true;
      SaveBar.markDirty("nation");
      showToast("Nation changes pending. Press Save on the Nation page.", "info");
      return true;
    }

    // Apply Player Preset
    async function applyPlayerPreset(preset) {
      return queuePreset("player", preset);
    }


    // ---- Diagnostics page: connection, module bases and loaded modules (read-only) ----
    // One Load button (SaveBar) re-reads /api/status and /api/maps. There is nothing to save.
    function diagEscape(value) {
      return String(value ?? "").replace(/[&<>"']/g, ch => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[ch]);
    }
    function diagSetText(id, text) {
      const el = document.getElementById(id);
      if (el) el.innerText = text;
    }
    function diagHexSize(start, end) {
      const a = parseInt(String(start), 16), b = parseInt(String(end), 16);
      if (!Number.isFinite(a) || !Number.isFinite(b) || b < a) return "unavailable";
      const kb = (b - a) / 1024;
      return kb >= 1024 ? `${(kb / 1024).toFixed(1)} MB` : `${Math.round(kb)} KB`;
    }
    function renderDiagStatus(status) {
      if (!status) return;
      diagSetText("diag-exe", status.exe_path || "unavailable");
      diagSetText("diag-proton", status.attached ? (status.is_proton ? "Steam Proton / Wine" : "Linux native") : "Not attached");
      diagSetText("diag-prefix", status.wine_prefix || "unavailable");
      diagSetText("diag-yama", status.yama_diagnostics || "unavailable");
      const rows = [["GameAssembly", status.game_assembly_base], ["UnityPlayer", status.unity_player_base], ["fm.exe", status.fm_exe_base]];
      const bases = document.getElementById("diag-bases");
      if (bases) bases.innerHTML = rows.map(([name, base]) =>
        `<tr><td>${diagEscape(name)}</td><td class="mono">${base ? diagEscape(base) : "unavailable"}</td></tr>`).join("");
    }
    function renderDiagMaps(maps) {
      const body = document.getElementById("diag-maps");
      const modules = maps && Array.isArray(maps.modules) ? maps.modules : [];
      diagSetText("diag-maps-count", maps && maps.total_mappings !== undefined ? `${modules.length} modules / ${maps.total_mappings} mappings` : "-");
      if (!body) return;
      body.innerHTML = modules.length ? modules.map(m =>
        `<tr><td>${diagEscape(m.name)}</td><td class="mono">${diagEscape(m.start)} - ${diagEscape(m.end)}</td><td class="mono">${diagEscape(m.perms)}</td><td class="num">${diagSetSize(m)}</td><td class="mono">${diagEscape(m.path)}</td></tr>`).join("")
        : `<tr><td colspan="5" class="unverified-note">${diagEscape(maps && maps.message ? maps.message : "No module data. Attach to FM26 first.")}</td></tr>`;
    }
    function diagSetSize(m) { return diagEscape(diagHexSize(m.start, m.end)); }

    async function loadDiagnostics() {
      try {
        const [statusRes, mapsRes] = await Promise.all([fetch("/api/status"), fetch("/api/maps")]);
        const status = await statusRes.json();
        if (!statusRes.ok) return false;
        renderDiagStatus(status);
        let maps = null;
        try { maps = await mapsRes.json(); } catch (error) { maps = null; }
        renderDiagMaps(mapsRes.ok ? maps : { message: maps && maps.message });
        return true;
      } catch (error) {
        showToast(`Diagnostics could not be read: ${error.message}`, "error");
        return false;
      }
    }

    SaveBar.register("diagnostics", { load: loadDiagnostics });

    // Read the diagnostics once, the first time the page is opened (read-only; the Load button re-reads).
    let diagnosticsAutoLoaded = false;
    if (typeof Shell !== "undefined" && Shell.onNavigate) {
      Shell.onNavigate(tabId => {
        if (tabId === "tab-status" && !diagnosticsAutoLoaded) { diagnosticsAutoLoaded = true; loadDiagnostics(); }
      });
    }
