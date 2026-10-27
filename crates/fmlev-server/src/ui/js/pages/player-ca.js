    // ---- Player CA/PA editing, dirty tracking for the Save bar, and the club jump ----

    // A baseline-relative aid, not FM's position/foot-weighted CA calculation.
    // It only ever suggests a value: CA changes only when the user presses "Stage estimate".
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

    // ---- Dirty tracking: the Save bar is the only way edits reach the game ----
    // A field is dirty only while it differs from the value last read from the game.
    function playerBaseline() {
      const base = state.caEstimateBaseline;
      return base && state.activePlayer && base.id === state.activePlayer.id ? base : null;
    }

    function trackAbilityEdit(field, value) {
      const base = playerBaseline();
      const original = base ? base[field] : undefined;
      if (original !== undefined && original > 0 && original === value) dirtyPlayerFields.delete(field);
      else dirtyPlayerFields.add(field);
      markUnsaved();
    }

    // Keep the Save bar in step with the dirty set: dirty while edits or staged actions exist, clean otherwise.
    function syncPlayerDirty() {
      const id = state.activePlayer ? state.activePlayer.id : null;
      const staged = id === null ? [] : (pendingActions.get(`player:${id}`) || []);
      const dirty = dirtyPlayerFields.size > 0 || staged.length > 0;
      state.playerDirty = dirty;
      if (typeof SaveBar === "undefined") return;
      if (dirty) SaveBar.markDirty("player");
      else SaveBar.clear("player");
    }

    function markUnsaved() { syncPlayerDirty(); }

    function parseAbility(val) {
      const trimmed = String(val ?? "").trim();
      const num = parseInt(trimmed, 10);
      return trimmed !== "" && trimmed !== "—" && !isNaN(num) && num > 0 ? Math.max(1, Math.min(200, num)) : null;
    }

    function setAbilityStars(starId, value) {
      const star = document.getElementById(starId);
      if (star) star.innerText = value > 0 ? getStarString(value) : "—";
    }

    // Typing: update the model only for a valid value; blur or change restores the shown value for invalid text.
    function handleAbilityInput(field, inputId, starId, val, commit) {
      if (!state.activePlayer) return;
      const num = parseAbility(val);
      const input = document.getElementById(inputId);
      if (num === null) {
        if (commit && input) input.value = state.activePlayer[field] > 0 ? state.activePlayer[field] : "—";
        return;
      }
      state.activePlayer[field] = num;
      if (commit && input) input.value = num;
      setAbilityStars(starId, num);
      trackAbilityEdit(field, num);
      updateRecommendedCA();
    }

    function handleCAInput(val) { handleAbilityInput("current_ability", "input-ca", "star-rating-ca", val, false); }
    function handleCAChange(val) { handleAbilityInput("current_ability", "input-ca", "star-rating-ca", val, true); }
    function handlePAInput(val) { handleAbilityInput("potential_ability", "input-pa", "star-rating-pa", val, false); }
    function handlePAChange(val) { handleAbilityInput("potential_ability", "input-pa", "star-rating-pa", val, true); }

    // ---- Writable gates: controls without a verified native write path stay disabled with a reason ----
    function playerFieldWritable(field) {
      const fields = state.playerWritableFields;
      return !state.connected || Array.isArray(fields) && fields.includes(field);
    }

    function applyAbilityGates() {
      for (const [field, id] of [["current_ability", "input-ca"], ["potential_ability", "input-pa"]]) {
        const el = document.getElementById(id);
        if (!el) continue;
        const ok = playerFieldWritable(field);
        el.disabled = !ok;
        el.title = ok ? "Edits are written when you click Save." : "No verified write path for this field.";
      }
    }

    // ---- Club jump ----
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
        showToast("Player club is not exposed or player is a free agent", "info");
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
    }
