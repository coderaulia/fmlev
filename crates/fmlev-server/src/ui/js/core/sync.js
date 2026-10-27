    // Fetch Context from Backend
    async function fetchContext() {
      try {
        const res = await fetch("/api/context");
        if (!res.ok) return;
        const data = await readJson(res);

        state.connected = data.connected;
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
          if (!isSamePlayer) {
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
          if (!isSameClub) state.clubDirty = false;
          if (!isSameClub || (!state.clubDirty && !isInputFocused())) {
            displayClub(club);
          }
        }

        if (data.active_nation || data.nation) {
          const nation = data.active_nation || data.nation;
          const isSameNation = state.activeNation && state.activeNation.id === nation.id;
          if (!isSameNation) state.nationDirty = false;
          if (!isSameNation || (!state.nationDirty && !isInputFocused())) {
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
        pill.className = "sync-indicator synced";
        const agoSecs = state.lastContextSyncTime ? Math.max(0, Math.floor((Date.now() - state.lastContextSyncTime) / 1000)) : 0;
        const syncAgoStr = `Synced ${agoSecs}s ago`;
        text.innerText = state.isPinned
          ? `🟢 ${syncAgoStr} • 📌 Pinned`
          : `🟢 ${syncAgoStr} • Auto-Following FM26`;
        if (diagConn) diagConn.innerText = `Connected (PID: ${state.pid})`;
      } else {
        pill.className = "sync-indicator unsynced";
        text.innerText = "🔴 Game Not Detected • Waiting for FM26...";
        if (diagConn) diagConn.innerText = "Disconnected (Waiting for FM26 process)";
      }

      if (diagPid) diagPid.innerText = state.pid || "None";

      if (state.isPinned) {
        pinBtn.classList.add("btn-pinned");
        pinBtn.innerText = "📌 Pinned (Click to Unpin)";
      } else {
        pinBtn.classList.remove("btn-pinned");
        pinBtn.innerText = "📌 Pin Player";
      }
    }

