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

    async function applyMassivePrizeMoney() {
      if (!state.activeCompetition) return;
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
