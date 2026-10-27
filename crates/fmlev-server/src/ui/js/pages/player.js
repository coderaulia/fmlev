    // Render Player Data
    function displayPlayer(player) {
      if (!player) return;
      if (attrLayoutGk !== playerIsGoalkeeper(player)) buildAttributeLists(playerIsGoalkeeper(player));
      const isSamePlayer = state.activePlayer && state.activePlayer.id === player.id;
      state.caEstimateBaseline = JSON.parse(JSON.stringify(player));
      if (!isSamePlayer) {
        state.activePlayer = JSON.parse(JSON.stringify(player));
        state.playerDirtyFields.clear();
        state.playerDirty = false;
      } else {
        if (!state.activePlayer) state.activePlayer = {};
        for (const [k, v] of Object.entries(player)) {
          if (k === "attributes" || k === "contract" || k === "positions") continue;
          if (!state.playerDirtyFields.has(k)) {
            state.activePlayer[k] = v;
          }
        }
      }

      // Live Badge & Sync Indicator
      const isLive = isLivePlayer(player);
      const liveBadge = document.getElementById("player-live-badge");
      if (liveBadge) liveBadge.style.display = isLive ? "inline-flex" : "none";
      updateSyncedAgoDisplay();

      // Name
      const editName = document.getElementById("edit-player-name");
      const isNameDirty = state.playerDirtyFields.has("name") || (editName && document.activeElement === editName);
      if (!isNameDirty) {
        state.activePlayer.name = player.name || "";
        const dispName = player.name || "Unknown Player";
        document.getElementById("player-name").innerText = dispName;
        const cPlayerName = document.getElementById("contract-player-name");
        if (cPlayerName) cPlayerName.innerText = `${dispName} — Contract`;
        if (editName) editName.value = player.name || "";
        const godPlayerTarget = document.getElementById("god-player-target");
        if (godPlayerTarget) godPlayerTarget.innerText = `Target: ${dispName}`;
      }

      // Age: 0 -> "—"
      const editAge = document.getElementById("edit-player-age");
      const isAgeDirty = state.playerDirtyFields.has("age") || (editAge && document.activeElement === editAge);
      if (!isAgeDirty) {
        const ageNum = parseInt(player.age, 10) || 0;
        state.activePlayer.age = ageNum;
        const ageEl = document.getElementById("player-age");
        if (ageEl) {
          if (ageNum > 0) {
            ageEl.innerText = `${ageNum} yrs`;
            ageEl.className = "pill-badge badge-nat";
          } else {
            ageEl.innerText = "—";
            ageEl.className = "pill-badge badge-muted";
          }
        }
        if (editAge) {
          editAge.value = ageNum > 0 ? ageNum : "";
          editAge.placeholder = ageNum > 0 ? "Age" : "—";
        }
      }

      // Nationality: empty -> "Unknown (live)" muted styling
      const editNat = document.getElementById("edit-player-nat");
      const isNatDirty = state.playerDirtyFields.has("nationality") || (editNat && document.activeElement === editNat);
      if (!isNatDirty) {
        const natStr = (player.nationality || "").trim();
        state.activePlayer.nationality = natStr;
        const natEl = document.getElementById("player-nat");
        if (natEl) {
          if (natStr) {
            natEl.innerText = `${getCountryFlag(natStr)} ${natStr}`;
            natEl.className = "pill-badge badge-nat";
          } else {
            natEl.innerText = "Unknown (live)";
            natEl.className = "pill-badge badge-muted";
          }
        }
        if (editNat) {
          editNat.value = natStr;
          editNat.placeholder = natStr ? "Nationality" : "Unknown (live)";
        }
      }

      // Position
      const editPos = document.getElementById("edit-player-pos");
      const isPosDirty = state.playerDirtyFields.has("position") || (editPos && document.activeElement === editPos);
      if (!isPosDirty) {
        const posStr = player.position || "MC";
        state.activePlayer.position = posStr;
        const posEl = document.getElementById("player-pos");
        if (posEl) posEl.innerText = posStr;
        if (editPos) editPos.value = posStr;
      }

      // Club: empty -> "Unknown (live)" muted styling
      const editClub = document.getElementById("edit-player-club");
      const isClubDirty = state.playerDirtyFields.has("club_name") || (editClub && document.activeElement === editClub);
      if (!isClubDirty) {
        const clubStr = (player.club_name || "").trim();
        state.activePlayer.club_name = clubStr;
        const clubEl = document.getElementById("player-club");
        const cClubName = document.getElementById("contract-club-name");
        const pClubName = document.getElementById("pos-club-name");
        if (clubEl) {
          if (clubStr) {
            clubEl.innerText = clubStr;
            clubEl.className = "pill-badge badge-club";
          } else {
            clubEl.innerText = "Unknown (live)";
            clubEl.className = "pill-badge badge-muted";
          }
        }
        if (cClubName) cClubName.innerText = clubStr || "Unknown (live)";
        if (pClubName) pClubName.innerText = clubStr || "Unknown (live)";
        if (editClub) {
          editClub.value = clubStr;
          editClub.placeholder = clubStr ? "Club Name (e.g. Persib)" : "Unknown (live)";
        }
      }

      // Morale & Fitness
      const morale = player.morale_label || (player.morale >= 90 ? "Superb" : "Good");
      document.getElementById("player-morale").innerText = `🔥 ${morale}`;

      const isInjured = player.is_injured;
      const injEl = document.getElementById("player-injury");
      if (isInjured) {
        injEl.className = "pill-badge badge-injured";
        injEl.innerText = "🩹 Injured";
      } else {
        injEl.className = "pill-badge badge-fit";
        injEl.innerText = "💚 Match Fit";
      }

      // Condition & Sharpness
      const condPct = Math.round((player.condition ?? 0) / 100);
      const sharpPct = Math.round((player.sharpness ?? 0) / 100);
      document.getElementById("disp-condition-pct").innerText = `${condPct}%`;
      document.getElementById("disp-sharpness-pct").innerText = `${sharpPct}%`;
      document.getElementById("fill-condition").style.width = `${condPct}%`;
      document.getElementById("fill-sharpness").style.width = `${sharpPct}%`;

      // CA / PA: 0 -> "—"
      const inputCa = document.getElementById("input-ca");
      const isCaDirty = state.playerDirtyFields.has("current_ability") || (inputCa && document.activeElement === inputCa);
      if (!isCaDirty) {
        const ca = parseInt(player.current_ability, 10) || 0;
        state.activePlayer.current_ability = ca;
        if (inputCa) inputCa.value = ca > 0 ? ca : "—";
        const starCa = document.getElementById("star-rating-ca");
        if (starCa) starCa.innerText = ca > 0 ? getStarString(ca) : "—";
      }

      const inputPa = document.getElementById("input-pa");
      const isPaDirty = state.playerDirtyFields.has("potential_ability") || (inputPa && document.activeElement === inputPa);
      if (!isPaDirty) {
        const pa = parseInt(player.potential_ability, 10) || 0;
        state.activePlayer.potential_ability = pa;
        if (inputPa) inputPa.value = pa > 0 ? pa : "—";
        const starPa = document.getElementById("star-rating-pa");
        if (starPa) starPa.innerText = pa > 0 ? getStarString(pa) : "—";
      }

      // Attributes: 0 -> "—" (unexposed)
      if (!state.activePlayer.attributes) state.activePlayer.attributes = {};
      for (const key of allAttrKeys) {
        const attrDirtyKey = "attr_" + key;
        const attrEl = document.getElementById("attr-val-" + key);
        const isAttrFocused = attrEl && document.activeElement === attrEl;
        if (!dirtyPlayerFields.has(attrDirtyKey) && !isAttrFocused) {
          const rawVal = player.attributes ? player.attributes[key] : 0;
          const val = parseInt(rawVal, 10) || 0;
          state.activePlayer.attributes[key] = val;
          updateAttrDisplay(key, val, true);
        }
      }

      // Contract tab inputs
      if (player.contract) {
        if (!state.activePlayer.contract) state.activePlayer.contract = {};
        const cWage = document.getElementById("contract-wage");
        if (cWage && document.activeElement !== cWage && !state.playerDirtyFields.has("contract.wage_per_week")) {
          setMoneyInput(cWage, player.contract.wage_per_week ?? 0);
          state.activePlayer.contract.wage_per_week = player.contract.wage_per_week ?? 0;
        }
        const cExp = document.getElementById("contract-expiry");
        if (cExp && document.activeElement !== cExp && !state.playerDirtyFields.has("contract.expiry_year")) {
          cExp.value = player.contract.expiry_year ?? 0;
          state.activePlayer.contract.expiry_year = player.contract.expiry_year ?? 0;
        }
        const cRel = document.getElementById("contract-release");
        if (cRel && document.activeElement !== cRel && !state.playerDirtyFields.has("contract.release_clause")) {
          setMoneyInput(cRel, player.contract.release_clause || 0);
          state.activePlayer.contract.release_clause = player.contract.release_clause || 0;
        }
        const cStat = document.getElementById("contract-status");
        if (cStat && document.activeElement !== cStat && !state.playerDirtyFields.has("contract.squad_status")) {
          cStat.value = player.contract.squad_status || "Regular Starter";
          state.activePlayer.contract.squad_status = player.contract.squad_status || "Regular Starter";
        }
        const cHap = document.getElementById("contract-happiness");
        if (cHap && document.activeElement !== cHap && !state.playerDirtyFields.has("contract.happiness")) {
          cHap.value = player.contract.happiness || "Content";
          state.activePlayer.contract.happiness = player.contract.happiness || "Content";
        }
      }

      // Positions & Foot ability tab
      if (player.positions && !state.playerDirtyFields.has("positions")) {
        state.activePlayer.positions = JSON.parse(JSON.stringify(player.positions));
      }
      renderPositionsTab();
      renderRoleList();
      applyAbilityGates();
      applyContractGates();
      renderPlayerFacts(state.activePlayer);
      updateRecommendedCA();
      refreshPendingActionIndicators();
    }

    function renderPlayerFacts(player) {
      const text = (id, value) => { const el = document.getElementById(id); if (el) el.innerText = value || "Unavailable"; };
      const name = String(player.name || "").trim();
      text("player-avatar", name ? name.split(/\s+/).filter(Boolean).map(part => part[0]).slice(0, 2).join("").toUpperCase() : "?");
      text("player-sub", [player.position, player.club_name].filter(Boolean).join(" · "));
      text("fact-age", player.age > 0 ? `${player.age} years` : null);
      text("fact-nat", player.nationality);
      text("fact-club", player.club_name);
      const contract = player.contract;
      text("fact-contract", contract?.expiry_year > 0 ? `${contract.wage_per_week > 0 ? formatMoney(contract.wage_per_week, {kind: "wage", compact: true}) + " · " : ""}${contract.expiry_year}` : null);
      text("fact-feet", getFootSummaryString(player.positions?.left_foot, player.positions?.right_foot));
      // Reputation, value and physical measurements are not provided by this Player API.
      for (const id of ["fact-rep", "fact-value", "fact-body"]) text(id, null);
    }

    // Live non-player native records are not verified. Keep identity visible, but
    // never present fixture values or unavailable zeros as game data.
    function renderVerifiedOverview(kind, record, banner) {
      const id = kind === "club" ? record.club_id : record.id;
      const snapshots = kind === "club" ? state.liveClubs : kind === "nation" ? state.liveNations : state.liveCompetitions;
      const snapshot = snapshots?.[id];
      const rows = [];
      const add = (label, value) => {
        if (value !== null && value !== undefined && value !== "") rows.push([label, String(value)]);
      };
      if (snapshot) {
        if (kind === "club") {
          add("Stadium", snapshot.stadium_name);
          add("Capacity", snapshot.stadium_capacity_text);
          add("Built", snapshot.stadium_year_built);
          add("Owner", snapshot.stadium_owner);
          add("Training", snapshot.training_description);
          add("Youth", snapshot.youth_description);
          add("Corporate", snapshot.corporate_description);
          add("Remaining wage budget", snapshot.remaining_wage_budget_text);
          add("Current wage spending", snapshot.current_wage_spending_text);
          add("Balance (raw game units)", snapshot.balance_game_units);
          add("Transfer budget (raw game units)", snapshot.transfer_budget_game_units);
        } else if (kind === "nation") {
          add("Continent", snapshot.continent);
          add("Population", snapshot.population?.toLocaleString());
          add("Languages", snapshot.spoken_languages);
          add("Men's federation founded", snapshot.men?.federation_founded);
          add("Men's ranking", snapshot.men?.fifa_rank);
          add("Men's points", snapshot.men?.fifa_points?.toFixed(0));
          add("Men's national stadium", snapshot.men?.national_stadium);
          add("Women's federation founded", snapshot.women?.federation_founded);
          add("Women's ranking", snapshot.women?.fifa_rank);
          add("Women's points", snapshot.women?.fifa_points?.toFixed(0));
          add("Women's national stadium", snapshot.women?.national_stadium);
        } else {
          add("Clubs", snapshot.number_of_teams);
          add("Season", snapshot.season_name);
          add("Asia ranking", snapshot.asia_reputation_position);
          add("Reputation", snapshot.reputation_description);
          add("Nation ID", snapshot.nation_id);
          add("Holder team ID", snapshot.holder_team_id);
        }
      }
      banner.replaceChildren();
      const message = document.createElement("div");
      message.textContent = snapshot ? "Verified live overview. Editing these fields is unavailable until native records are validated." : "No verified live overview is loaded for this selection. Open its screen in FM26 to read available values. Editing is unavailable.";
      banner.appendChild(message);
      if (rows.length) {
        const list = document.createElement("dl");
        list.style.cssText = "display:grid;grid-template-columns:max-content 1fr;gap:8px 18px;margin:14px 0 0";
        for (const [label, value] of rows) {
          const term = document.createElement("dt"); term.textContent = label;
          const detail = document.createElement("dd"); detail.textContent = value; detail.style.margin = "0";
          list.append(term, detail);
        }
        banner.appendChild(list);
      }
    }

    function displayEntityAvailability(kind, record) {
      const panel = document.getElementById(`tab-${kind}`);
      if (!panel) return state.connected;
      const bannerId = `${kind}-live-unavailable`;
      let banner = document.getElementById(bannerId);
      if (!banner) {
        banner = document.createElement("div");
        banner.id = bannerId;
        banner.className = "card";
        banner.setAttribute("role", "status");
        banner.style.cssText = "margin:16px 0;padding:20px;border-color:var(--accent-gold);color:var(--text-muted);line-height:1.6";
        panel.appendChild(banner);
      }
      banner.hidden = !state.connected;
      const header = panel.firstElementChild;
      for (const child of panel.children) {
        if (child === header || child === banner || child.hasAttribute("data-live-editor")) continue;
        if (state.connected) {
          if (!child.hasAttribute("data-entity-original-hidden")) {
            child.dataset.entityOriginalHidden = String(child.hidden);
            child.dataset.entityOriginalDisplay = child.style.display || "";
          }
          child.hidden = true;
          child.style.display = "none";
        } else if (child.hasAttribute("data-entity-original-hidden")) {
          child.hidden = child.dataset.entityOriginalHidden === "true";
          child.style.display = child.dataset.entityOriginalDisplay;
          delete child.dataset.entityOriginalHidden;
          delete child.dataset.entityOriginalDisplay;
        }
      }
      for (const input of panel.querySelectorAll("input, select, button, textarea")) {
        if (state.connected) {
          if (input.closest && input.closest("[data-live-editor]") && !input.hasAttribute("data-unverified")) {
            continue;
          }
          if (input.getAttribute && (
            input.getAttribute("onclick")?.includes("saveClubToGame") ||
            input.getAttribute("onclick")?.includes("saveNationToGame") ||
            input.getAttribute("onclick")?.includes("saveCompetitionToGame")
          )) {
            continue;
          }
          if (!input.hasAttribute("data-entity-original-disabled")) input.dataset.entityOriginalDisabled = String(input.disabled);
          input.disabled = true;
        } else if (input.hasAttribute("data-entity-original-disabled")) {
          input.disabled = input.dataset.entityOriginalDisabled === "true";
          delete input.dataset.entityOriginalDisabled;
        }
      }
      if (!state.connected) return false;
      renderVerifiedOverview(kind, record, banner);
      const text = (id, value) => { const el = document.getElementById(id); if (el) el.innerText = value; };
      if (kind === "club") {
        text("club-title", record.club_name || "Club unavailable");
        text("club-league-badge", record.league_name || "League unavailable");
        text("club-rep-num", record.reputation ? `${record.reputation.toLocaleString()} / 10,000` : "Unavailable");
        text("club-rep-stars", record.reputation ? getReputationStars(record.reputation) : "—");
        text("save-club-status-indicator", "✅ Synced with FM26");
        setFacVal("fac-training", "fac-training-disp", record.training_facilities || 18);
        setFacVal("fac-youth", "fac-youth-disp", record.youth_facilities || 18);
        setFacVal("fac-recruitment", "fac-recruitment-disp", record.youth_recruitment || 18);
        setFacVal("fac-coaching", "fac-coaching-disp", record.junior_coaching || 18);
        setFacVal("fac-corporate", "fac-corporate-disp", record.corporate_facilities || 19);
        syncClubNative(record.club_id);
      } else if (kind === "nation") {
        text("nation-name", record.name || "Nation unavailable");
        text("nation-code", record.three_letter_code || "—");
        text("nation-continent", record.continent || "Unavailable");
        text("nation-flag-avatar", getCountryFlag(record.name));
        text("nation-rep-num", record.reputation ? `${record.reputation.toLocaleString()} / 10,000` : "Unavailable");
        text("nation-rep-stars", record.reputation ? getReputationStars(record.reputation) : "—");
        if (record.youth_rating) {
          const yr = record.youth_rating;
          const yrVal = document.getElementById("nation-youth-val");
          if (yrVal) yrVal.innerText = yr;
          const yrSlider = document.getElementById("nation-youth-slider");
          if (yrSlider) yrSlider.value = yr;
          updateYouthRatingDisplay(yr);
        }
        if (record.reputation) {
          const repInput = document.getElementById("nation-reputation");
          if (repInput) repInput.value = record.reputation;
        }
        if (record.economic_factor) {
          setNationStatUI("nation-economic", "disp-nation-econ", record.economic_factor);
        }
        if (record.state_of_development !== undefined) {
          const dev = document.getElementById("nation-development");
          if (dev) dev.value = record.state_of_development;
        }
        syncNationNative(record.id);
      } else {
        text("comp-name", record.name || "Competition unavailable");
        text("comp-short-name", record.short_name || "—");
        text("comp-nation", record.nation_name || "Unavailable");
        text("comp-rep-num", record.reputation ? `${record.reputation.toLocaleString()} / 10,000` : "Unavailable");
        text("comp-rep-stars", record.reputation ? getReputationStars(record.reputation) : "—");
        if (record.reputation) {
          const compRep = document.getElementById("comp-reputation");
          if (compRep) compRep.value = record.reputation;
        }
        syncCompetitionNative(record.id);
      }
      return true;
    }

