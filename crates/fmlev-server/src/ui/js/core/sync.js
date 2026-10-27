    // Fetch Context from Backend
    async function fetchContext() {
      try {
        const res = await fetch("/api/context");
        if (!res.ok) return;
        const data = await readJson(res);

        state.connected = data.connected;
        for (const button of document.querySelectorAll?.('[data-live-unavailable="true"]') || []) {
          button.disabled = !!state.connected;
          button.title = state.connected ? "Unavailable: no verified live write path" : "Offline demonstration only";
        }
        updateTransferButtons();
        state.liveClubs = data.live_clubs || {};
        state.liveNations = data.live_nations || {};
        state.liveCompetitions = data.live_competitions || {};
        state.playerWritableFields = data.player_writable_fields ?? null;
        const support = document.getElementById("player-write-support");
        if (support) {
          const fields = state.playerWritableFields;
          const count = fields?.filter(field => field.startsWith("attributes.")).length || 0;
          support.innerText = !data.connected ? "Game disconnected. Displayed fixtures do not change FM26." : fields === null ? "" :
            `Live Save: ${count} native player attributes${fields.includes("positions") ? ", positions and feet" : ""}${fields.includes("current_ability") ? ", CA/PA" : ""}${fields.includes("contract.wage_per_week") ? ", weekly wage (GBP) and expiry year" : ""}. Names, transfers, fitness/morale and other entity data still require separate write paths.`;
        }
        state.pid = data.pid;
        state.isPinned = data.is_pinned;
        state.syncMode = data.sync_mode;
        state.screenState = data.screen_state;
        state.lastContextSyncTime = Date.now();

        updateSyncUI();
        updateSyncedAgoDisplay();

        if (data.active_player || data.player) {
          const player = data.active_player || data.player;
          const isSamePlayer = state.activePlayer && state.activePlayer.id === player.id;
          if (!isSamePlayer && state.activePlayer && SaveBar.isDirty("player")) {
            // Keep the selected draft until Save/Discard or an explicit selection.
          } else if (!isSamePlayer) {
            state.playerDirtyFields.clear();
            state.playerDirty = false;
            displayPlayer(player);
          } else {
            // Same player: displayPlayer protects any dirty fields or focused inputs
            displayPlayer(player);
          }
        }

        if (data.active_club || data.club) {
          const club = data.active_club || data.club;
          const isSameClub = state.activeClub && state.activeClub.club_id === club.club_id;
          const keepClubDraft = !isSameClub && state.activeClub && SaveBar.isDirty("club");
          if (!isSameClub && !keepClubDraft) state.clubDirty = false;
          if (!keepClubDraft && (!isSameClub || (!state.clubDirty && !isInputFocused()))) {
            displayClub(club);
          }
        }

        if (data.active_nation || data.nation) {
          const nation = data.active_nation || data.nation;
          const isSameNation = state.activeNation && state.activeNation.id === nation.id;
          const keepNationDraft = !isSameNation && state.activeNation && SaveBar.isDirty("nation");
          if (!isSameNation && !keepNationDraft) state.nationDirty = false;
          if (!keepNationDraft && (!isSameNation || (!state.nationDirty && !isInputFocused()))) {
            displayNation(nation);
          }
        }

        if (data.active_competition || data.competition) {
          const comp = data.active_competition || data.competition;
          const isSameComp = state.activeCompetition && state.activeCompetition.id === comp.id;
          if (!isSameComp) state.competitionDirty = false;
          if (!isSameComp || (!state.competitionDirty && !isInputFocused())) {
            displayCompetition(comp);
          }
        }
        Shell.syncEntities(state);
      } catch (e) {
        console.warn("Context fetch error:", e);
      }
    }

    function updateSyncUI() {
      const pill = document.getElementById("sync-pill");
      const text = document.getElementById("sync-status-text");
      const pinBtn = document.getElementById("pin-btn");
      const diagConn = document.getElementById("diag-conn-state");
      const diagPid = document.getElementById("diag-pid");

      if (state.connected) {
        if (pill) pill.className = "sync-indicator synced";
        const agoSecs = state.lastContextSyncTime ? Math.max(0, Math.floor((Date.now() - state.lastContextSyncTime) / 1000)) : 0;
        const syncAgoStr = `Synced ${agoSecs}s ago`;
        if (text) text.innerText = state.isPinned
          ? `${syncAgoStr} \u00b7 Pinned`
          : `${syncAgoStr} \u00b7 Auto-following FM26`;
        if (diagConn) diagConn.innerText = `Connected (PID: ${state.pid})`;
      } else {
        if (pill) pill.className = "sync-indicator unsynced";
        if (text) text.innerText = "Game not detected \u00b7 Waiting for FM26...";
        if (diagConn) diagConn.innerText = "Disconnected (Waiting for FM26 process)";
      }

      if (diagPid) diagPid.innerText = state.pid || "None";

      if (!pinBtn) return;
      if (state.isPinned) {
        pinBtn.classList.add("btn-pinned");
        pinBtn.innerText = "Pinned (click to unpin)";
      } else {
        pinBtn.classList.remove("btn-pinned");
        pinBtn.innerText = "Pin Player";
      }
    }

