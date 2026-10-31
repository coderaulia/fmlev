    // Render Club Data
    function displayClub(club) {
      state.activeClub = JSON.parse(JSON.stringify(club));
      if (displayEntityAvailability("club", club)) {
        renderClubOverview(club);
        renderClubFacilities(state.activeClub);
        return;
      }
      renderClubOverview(club);
      const status = document.getElementById("save-club-status-indicator");
      if (status) status.innerText = "Offline demo";

      const titleEl = document.getElementById("club-title");
      if (titleEl) titleEl.innerText = club.club_name || "Club";
      const lEl = document.getElementById("club-league-badge");
      if (lEl) lEl.innerText = club.league_name || "League";
      renderClubReputation(club);

      // Finances & Stadium
      const bal = document.getElementById("club-balance");
      if (bal) setMoneyInput(bal, club.balance ?? 100000000);
      const tb = document.getElementById("club-transfer-budget");
      if (tb) setMoneyInput(tb, club.transfer_budget ?? 50000000);
      const wb = document.getElementById("club-wage-budget");
      if (wb) setMoneyInput(wb, club.wage_budget ?? 2500000);
      const mw = document.getElementById("club-max-wage");
      if (mw) setMoneyInput(mw, club.max_wage_allowance ?? 600000);
      const sn = document.getElementById("club-stadium-name");
      if (sn) sn.value = club.stadium_name || "Club Stadium";
      const sc = document.getElementById("club-stadium-cap");
      if (sc) sc.value = club.stadium_capacity ?? 45000;
      const se = document.getElementById("club-stadium-exp");
      if (se) se.value = club.stadium_expansion_cap ?? 60000;

      // 6 Facilities Matrix
      setFacVal("fac-training", "fac-training-disp", club.training_facilities ?? 18);
      setFacVal("fac-youth", "fac-youth-disp", club.youth_facilities ?? 18);
      setFacVal("fac-recruitment", "fac-recruitment-disp", club.youth_recruitment ?? 18);
      setFacVal("fac-coaching", "fac-coaching-disp", club.junior_coaching ?? 18);
      setFacVal("fac-data-analysis", "fac-data-analysis-disp", club.data_analysis_facilities ?? 18);
      setFacVal("fac-corporate", "fac-corporate-disp", club.corporate_facilities ?? 18);

      // Board & Ownership
      const sdSel = document.getElementById("club-sugar-daddy");
      if (sdSel) {
        sdSel.value = club.sugar_daddy_type || "None";
        updateSugarDaddyBadge(club.sugar_daddy_type || "None");
      }

      const bp = document.getElementById("club-board-patience");
      if (bp) {
        bp.value = club.board_patience ?? 15;
        updateDisp("disp-patience", club.board_patience ?? 15);
      }

      const fc = document.getElementById("club-fan-confidence");
      if (fc) {
        fc.value = club.fan_confidence ?? 85;
        const disp = document.getElementById("disp-fan-conf");
        if (disp) disp.innerText = (club.fan_confidence ?? 85) + "%";
      }

      const bc = document.getElementById("club-board-confidence");
      if (bc) {
        bc.value = club.board_confidence ?? 88;
        const disp = document.getElementById("disp-board-conf");
        if (disp) disp.innerText = (club.board_confidence ?? 88) + "%";
      }

      const tr = document.getElementById("club-transfer-retention");
      if (tr) {
        tr.value = club.transfer_retention_pct ?? 100;
        const disp = document.getElementById("disp-retention");
        if (disp) disp.innerText = (club.transfer_retention_pct ?? 100) + "%";
      }

      // Affiliated clubs
      renderAffiliatesList(club.affiliates || []);

      // Update God Mode Target Badges
      const godClubTarget = document.getElementById("god-club-target");
      if (godClubTarget) godClubTarget.innerText = `Target: ${club.club_name || "Active Club"}`;
      const godSquadTarget = document.getElementById("god-squad-target");
      if (godSquadTarget) godSquadTarget.innerText = `Target: ${club.club_name || "Club"} 1st Team`;
      renderClubFacilities(club);
      refreshPendingActionIndicators();
    }

    // The facility sliders start from binding caches. Load the verified native values once per club
    // (and again after a save) so untouched controls match the game and are never written back.
    let nativeClubSyncedId = null;
    let clubNativeRecord = null;
    async function syncClubNative(clubId) {
      if (!state.connected || !clubId || state.clubDirty) return false;
      if (nativeClubSyncedId === clubId) return true;
      nativeClubSyncedId = clubId;
      try {
        const res = await fetch("/api/club/native", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ club_id: clubId })
        });
        const data = await readJson(res);
        if (!res.ok) { nativeClubSyncedId = null; return false; }
        if (state.clubDirty || state.activeClub?.club_id !== clubId) return;
        const club = data.club;
        clubNativeRecord = { ...club, club_id: clubId };
        Object.assign(state.activeClub, {
          training_facilities: club.training_facilities, youth_facilities: club.youth_facilities,
          junior_coaching: club.junior_coaching, youth_recruitment: club.youth_recruitment,
          data_analysis_facilities: club.data_analysis_facilities, corporate_facilities: club.corporate_facilities
        });
        renderClubFacilities(state.activeClub);
        return true;
      } catch (e) { nativeClubSyncedId = null; return false; }
    }


    const CLUB_FACILITIES = [
      ["fac-training", "training_facilities", "Training facilities"],
      ["fac-youth", "youth_facilities", "Youth facilities"],
      ["fac-recruitment", "youth_recruitment", "Youth recruitment"],
      ["fac-coaching", "junior_coaching", "Junior coaching"],
      ["fac-data-analysis", "data_analysis_facilities", "Data analysis facilities"],
      ["fac-corporate", "corporate_facilities", "Corporate facilities"],
    ];

    function updateClubFacilityDisplay(id, value) {
      const valid = Number.isInteger(Number(value)) && Number(value) >= 1 && Number(value) <= 20;
      const display = document.getElementById(id + "-disp");
      if (display) {
        display.innerText = valid ? String(value) : "—";
        display.className = `facility-val ${valueBand(valid ? Number(value) : null)}`;
      }
      const stars = document.getElementById(id + "-stars");
      if (stars) {
        const filled = valid ? Math.round(Number(value) / 4) : 0;
        stars.innerText = valid ? "★".repeat(filled) + "☆".repeat(5 - filled) : "—";
        stars.title = valid ? `${value} / 20 (stars rounded)` : "Native value unavailable";
      }
    }

    // Club reputation (native main-team reputation, 1-10000). Connected mode shows only the value read from the
    // game; nothing is invented when it is unreadable.
    function renderClubReputation(club) {
      const native = state.connected ? (clubNativeRecord?.club_id === club.club_id ? clubNativeRecord : null) : club;
      const value = native?.reputation;
      const valid = Number.isInteger(value) && value >= 1 && value <= 10000;
      const num = document.getElementById("club-rep-num");
      if (num) num.innerText = valid ? `${value.toLocaleString()} / 10,000` : "Unavailable";
      const stars = document.getElementById("club-rep-stars");
      if (stars) stars.innerText = valid ? getReputationStars(value) : "—";
      const input = document.getElementById("club-reputation");
      if (input) {
        input.disabled = !valid;
        input.value = valid ? value : "";
        input.title = valid ? "Native main-team reputation (1-10000). Pending until Save." : "Native value unavailable. Use Load from game to retry.";
      }
      if (native && valid && state.connected && state.activeClub) state.activeClub.reputation = value;
    }

    // Typing updates the display only; the value is written by Save through the club update route.
    function clubReputationInput(raw) {
      const value = Number(raw);
      const valid = String(raw).trim() !== "" && Number.isInteger(value) && value >= 1 && value <= 10000;
      const num = document.getElementById("club-rep-num");
      if (num) num.innerText = valid ? `${value.toLocaleString()} / 10,000` : "Invalid";
      const stars = document.getElementById("club-rep-stars");
      if (stars) stars.innerText = valid ? getReputationStars(value) : "—";
      markClubUnsaved();
    }

    function renderClubFacilities(club) {
      const native = clubNativeRecord?.club_id === club.club_id ? clubNativeRecord : null;
      for (const [id, field] of CLUB_FACILITIES) {
        const value = state.connected ? native?.[field] : club[field];
        const valid = Number.isInteger(value) && value >= 1 && value <= 20;
        const input = document.getElementById(id);
        if (input) {
          input.disabled = !valid;
          input.value = valid ? value : "";
          input.title = valid ? `${value} / 20` : "Native value unavailable. Use Load from game to retry.";
          const row = input.closest?.(".facility-row");
          for (const button of row?.querySelectorAll?.("button") || []) button.disabled = !valid;
        }
        if (native && state.activeClub) state.activeClub[field] = value;
        updateClubFacilityDisplay(id, value);
      }
      renderClubReputation(club);
    }

    function renderClubOverview(club) {
      if (state.loadedMoney?.club_id !== club.club_id) {
        for (const id of ["club-money-balance", "club-money-transfer"]) {
          const input = document.getElementById(id);
          if (input) { input.value = ""; input.disabled = true; }
        }
      }
      const value = (id, v) => {
        const node = document.getElementById(id);
        if (node) node.innerText = v === undefined || v === null || v === "" || v === 0 ? "Unavailable" : String(v);
      };
      value("club-info-id", club.club_id);
      value("club-info-stadium", club.stadium_name);
      value("club-info-capacity", club.stadium_capacity > 0 ? club.stadium_capacity.toLocaleString() : null);
      value("club-info-expansion", club.stadium_expansion_cap > 0 ? club.stadium_expansion_cap.toLocaleString() : null);
      if (typeof Shell !== "undefined") Shell.setContext({ section: "club", kind: "club", id: club.club_id,
        title: club.club_name || "Club unavailable", subtitle: club.league_name || "" });
    }
