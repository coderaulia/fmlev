    // A baseline-relative aid, not FM's position/foot-weighted CA calculation.
    function recommendedCA() {
      const player = state.activePlayer;
      const baseline = state.caEstimateBaseline;
      if (!player || !baseline || player.id !== baseline.id ||
          !Number.isInteger(baseline.current_ability) || baseline.current_ability < 1 || baseline.current_ability > 200) return null;
      const keys = [...techAttrs, ...mentalAttrs, ...physAttrs, ...gkAttrs].map(([key]) => key);
      const valid = value => Number.isInteger(value) && value >= 1 && value <= 20;
      if (!keys.every(key => valid(player.attributes?.[key]) && valid(baseline.attributes?.[key]))) return null;
      const delta = keys.reduce((sum, key) => sum + player.attributes[key] - baseline.attributes[key], 0);
      return Math.max(1, Math.min(200, Math.round(baseline.current_ability + delta * 10 / keys.length)));
    }

    function updateRecommendedCA() {
      const rec = recommendedCA();
      const label = document.getElementById("ca-suggestion");
      const button = document.getElementById("apply-ca-suggestion");
      if (label) label.innerText = rec === null
        ? "CA estimate unavailable: complete attributes and a valid live CA are required."
        : `Approximate CA: ${rec}. Attribute changes only; FM's position and foot weights are not reproduced.`;
      if (button) button.disabled = rec === null || rec === state.activePlayer.current_ability;
      return rec;
    }

    function applyRecommendedCA() {
      const rec = recommendedCA();
      if (rec === null || rec === state.activePlayer.current_ability) return;
      handleCAChange(String(rec));
      updateRecommendedCA();
    }

    const populatePlayerForm = displayPlayer;

    function handleCAInput(val) {
      dirtyPlayerFields.add("current_ability");
      markUnsaved();
      const trimmed = String(val || "").trim();
      const num = parseInt(trimmed, 10);
      const starCa = document.getElementById("star-rating-ca");
      if (trimmed !== "" && trimmed !== "—" && !isNaN(num) && num > 0) {
        const clamped = Math.max(1, Math.min(200, num));
        if (state.activePlayer) state.activePlayer.current_ability = clamped;
        if (starCa) starCa.innerText = getStarString(clamped);
      } else {
        if (state.activePlayer) state.activePlayer.current_ability = 0;
        if (starCa) starCa.innerText = "—";
      }
    }

    function handleCAChange(val) {
      const inputCa = document.getElementById("input-ca");
      const starCa = document.getElementById("star-rating-ca");
      const trimmed = String(val || "").trim();
      const num = parseInt(trimmed, 10);
      if (trimmed !== "" && trimmed !== "—" && !isNaN(num) && num > 0) {
        dirtyPlayerFields.add("current_ability");
        markUnsaved();
        const clamped = Math.max(1, Math.min(200, num));
        if (state.activePlayer) state.activePlayer.current_ability = clamped;
        if (inputCa) inputCa.value = clamped;
        if (starCa) starCa.innerText = getStarString(clamped);
      } else {
                if (state.activePlayer) state.activePlayer.current_ability = 0;
        if (inputCa) inputCa.value = "—";
        if (starCa) starCa.innerText = "—";
      }
    }

    function handlePAInput(val) {
      dirtyPlayerFields.add("potential_ability");
      markUnsaved();
      const trimmed = String(val || "").trim();
      const num = parseInt(trimmed, 10);
      const starPa = document.getElementById("star-rating-pa");
      if (trimmed !== "" && trimmed !== "—" && !isNaN(num) && num > 0) {
        const clamped = Math.max(1, Math.min(200, num));
        if (state.activePlayer) state.activePlayer.potential_ability = clamped;
        if (starPa) starPa.innerText = getStarString(clamped);
      } else {
        if (state.activePlayer) state.activePlayer.potential_ability = 0;
        if (starPa) starPa.innerText = "—";
      }
    }

    function handlePAChange(val) {
      const inputPa = document.getElementById("input-pa");
      const starPa = document.getElementById("star-rating-pa");
      const trimmed = String(val || "").trim();
      const num = parseInt(trimmed, 10);
      if (trimmed !== "" && trimmed !== "—" && !isNaN(num) && num > 0) {
        dirtyPlayerFields.add("potential_ability");
        markUnsaved();
        const clamped = Math.max(1, Math.min(200, num));
        if (state.activePlayer) state.activePlayer.potential_ability = clamped;
        if (inputPa) inputPa.value = clamped;
        if (starPa) starPa.innerText = getStarString(clamped);
      } else {
                if (state.activePlayer) state.activePlayer.potential_ability = 0;
        if (inputPa) inputPa.value = "—";
        if (starPa) starPa.innerText = "—";
      }
    }

    function promptEditName() {
      if (!state.activePlayer) return;
      const newName = prompt("Edit Player Name:", state.activePlayer.name || "");
      if (newName && newName.trim()) {
        state.playerDirtyFields.add("name");
        handlePlayerFieldChange("name", newName.trim());
      }
    }

    function markSaved(timestamp) {
      const timeStr = timestamp || new Date().toLocaleTimeString();
      state.playerDirty = false;
      state.lastSavedTime = timeStr;

      const badge = document.getElementById("save-status-indicator");
      if (badge) {
        badge.className = "pill-badge badge-fit";
        badge.innerText = `✅ Saved [${timeStr}]`;
      }
      const label = document.getElementById("last-saved-time");
      if (label) {
        label.innerText = `✅ Saved at [${timeStr}]`;
        label.style.color = "var(--accent-emerald)";
      }
    }

    function markUnsaved() {
      state.playerDirty = true;
      const badge = document.getElementById("save-status-indicator");
      if (badge) {
        badge.className = "pill-badge badge-morale";
        badge.innerText = `⏳ Unsaved Changes...`;
      }
      const label = document.getElementById("last-saved-time");
      if (label) {
        label.innerText = `Unsaved changes...`;
        label.style.color = "var(--accent-gold)";
      }
    }

    function handlePlayerFieldChange(field, val) {
      if (!state.activePlayer) return;
      state.playerDirtyFields.add(field);
      markUnsaved();

      if (field === "age") {
        const num = parseInt(val, 10) || 0;
        state.activePlayer.age = num;
        const el = document.getElementById("player-age");
        if (el) {
          if (num > 0) {
            el.innerText = `${num} yrs`;
            el.className = "pill-badge badge-nat";
          } else {
            el.innerText = "—";
            el.className = "pill-badge badge-muted";
          }
        }
      } else if (field === "name") {
        state.activePlayer.name = val;
        const el = document.getElementById("player-name");
        if (el) el.innerText = val || "Player Name";
        const cEl = document.getElementById("contract-player-name");
        if (cEl) cEl.innerText = `${val} — Contract`;
        const posEl = document.getElementById("pos-player-name");
        if (posEl) posEl.innerText = `${val} — Positions & Roles`;
        const godTarget = document.getElementById("god-player-target");
        if (godTarget) godTarget.innerText = `Target: ${val}`;
      } else if (field === "nationality") {
        const str = (val || "").trim();
        state.activePlayer.nationality = str;
        const el = document.getElementById("player-nat");
        if (el) {
          if (str) {
            el.innerText = `${getCountryFlag(str)} ${str}`;
            el.className = "pill-badge badge-nat";
          } else {
            el.innerText = "Unknown (live)";
            el.className = "pill-badge badge-muted";
          }
        }
      } else if (field === "club_name") {
        const str = (val || "").trim();
        state.activePlayer.club_name = str;
        const el = document.getElementById("player-club");
        if (el) {
          if (str) {
            el.innerText = str;
            el.className = "pill-badge badge-club";
          } else {
            el.innerText = "Unknown (live)";
            el.className = "pill-badge badge-muted";
          }
        }
        const cEl = document.getElementById("contract-club-name");
        if (cEl) cEl.innerText = str || "Unknown (live)";
        const posEl = document.getElementById("pos-club-name");
        if (posEl) posEl.innerText = str || "Unknown (live)";
      } else if (field === "position") {
        state.activePlayer.position = val;
        const el = document.getElementById("player-pos");
        if (el) el.innerText = val;
        const posBadge = document.getElementById("pos-summary-badge");
        if (posBadge) posBadge.innerText = val;
      }

    }

    async function jumpToPlayerClub() {
      if (!state.activePlayer) {
        showToast("No active player loaded", "warning");
        return;
      }
      const clubId = state.activePlayer.club_id;
      const clubName = state.activePlayer.club_name;
      if (clubId && clubId > 0) {
        showToast(`Loading club details for ${clubName || "Club #" + clubId}...`, "info");
        await selectSearchedClub(clubId);
        return;
      }
      if (!clubName || clubName === "Free Agent" || clubName === "Unknown (live)") {
        showToast("Player club is not exposed or player is Free Agent", "info");
        return;
      }

      showToast(`Loading club details for ${clubName}...`, "info");

      try {
        const res = await fetch(`/api/search?q=${encodeURIComponent(clubName)}`);
        if (res.ok) {
          const data = await readJson(res);
          const clubs = data.clubs || [];
          const found = clubs.find(c => c.club_name.toLowerCase() === clubName.toLowerCase()) || clubs[0];
          if (found) {
            await selectSearchedClub(found.club_id);
            return;
          }
        }
      } catch (e) {
        console.warn("Search club error:", e);
      }

      showToast(`Club "${clubName}" was not found in the loaded game data.`, "warning");
      return;
    }

