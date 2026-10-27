    // Render Club Data
    function displayClub(club) {
      state.activeClub = JSON.parse(JSON.stringify(club));
      if (displayEntityAvailability("club", club)) return;

      const titleEl = document.getElementById("club-title");
      if (titleEl) titleEl.innerText = club.club_name || "Club";
      const lEl = document.getElementById("club-league-badge");
      if (lEl) lEl.innerText = club.league_name || "League";
      const repEl = document.getElementById("club-rep-num");
      if (repEl) repEl.innerText = `${(club.reputation || 8000).toLocaleString()} / 10,000`;
      const starsEl = document.getElementById("club-rep-stars");
      if (starsEl) starsEl.innerText = getReputationStars(club.reputation || 8000);

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
      refreshPendingActionIndicators();
    }

    // The facility sliders start from binding caches. Load the verified native values once per club
    // (and again after a save) so untouched controls match the game and are never written back.
    let nativeClubSyncedId = null;
    async function syncClubNative(clubId) {
      if (!state.connected || !clubId || nativeClubSyncedId === clubId || state.clubDirty) return;
      nativeClubSyncedId = clubId;
      try {
        const res = await fetch("/api/club/native", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ club_id: clubId })
        });
        const data = await readJson(res);
        if (!res.ok) { nativeClubSyncedId = null; return; }
        if (state.clubDirty || state.activeClub?.club_id !== clubId) return;
        const club = data.club;
        Object.assign(state.activeClub, {
          training_facilities: club.training_facilities, youth_facilities: club.youth_facilities,
          junior_coaching: club.junior_coaching, youth_recruitment: club.youth_recruitment,
          data_analysis_facilities: club.data_analysis_facilities, corporate_facilities: club.corporate_facilities
        });
        setFacVal("fac-training", "fac-training-disp", club.training_facilities);
        setFacVal("fac-youth", "fac-youth-disp", club.youth_facilities);
        setFacVal("fac-recruitment", "fac-recruitment-disp", club.youth_recruitment);
        setFacVal("fac-coaching", "fac-coaching-disp", club.junior_coaching);
        setFacVal("fac-corporate", "fac-corporate-disp", club.corporate_facilities);
        if (club.data_analysis_facilities > 0) setFacVal("fac-data-analysis", "fac-data-analysis-disp", club.data_analysis_facilities);
      } catch (e) { nativeClubSyncedId = null; }
    }

