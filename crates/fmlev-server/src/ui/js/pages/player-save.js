    // Compatibility for older controls: edits always require an explicit Save.
    function toggleAutoApply() {
      // Legacy handler: edits always require an explicit Save.
    }

    function debouncedSavePlayer() {
      markUnsaved();
    }

    // Apply Player Changes to Backend & Memory
    async function writePlayerChanges() {
      if (!state.activePlayer) return false;
      // All player Save buttons include contract edits without replacing unedited fields.
      const contractInputs = {
        wage_per_week: "contract-wage", expiry_year: "contract-expiry",
        release_clause: "contract-release", squad_status: "contract-status", happiness: "contract-happiness"
      };
      for (const [field, inputId] of Object.entries(contractInputs)) {
        if (!state.playerDirtyFields.has(`contract.${field}`) && !state.playerDirtyFields.has("contract")) continue;
        const input = document.getElementById(inputId);
        if (!input) continue;
        if (!state.activePlayer.contract) state.activePlayer.contract = {};
        state.activePlayer.contract[field] = ["squad_status", "happiness"].includes(field)
          ? input.value : (["wage_per_week", "release_clause"].includes(field) ? readMoneyInput(input, 0) : numberInput(input.value, 0));
      }

      // Ensure 0/empty values stay 0 rather than fake defaults or NaN
      const ageVal = parseInt(state.activePlayer.age, 10);
      state.activePlayer.age = (!isNaN(ageVal) && ageVal > 0) ? ageVal : 0;

      const caVal = parseInt(state.activePlayer.current_ability, 10);
      state.activePlayer.current_ability = (!isNaN(caVal) && caVal > 0) ? Math.min(200, caVal) : 0;

      const paVal = parseInt(state.activePlayer.potential_ability, 10);
      state.activePlayer.potential_ability = (!isNaN(paVal) && paVal > 0) ? Math.min(200, paVal) : 0;

      if (!state.activePlayer.attributes) state.activePlayer.attributes = {};
      for (const key of allAttrKeys) {
        const raw = state.activePlayer.attributes[key];
        const v = parseInt(raw, 10);
        state.activePlayer.attributes[key] = (!isNaN(v) && v >= 1 && v <= 20) ? v : 0;
      }

      const submittedPlayer = JSON.stringify(state.activePlayer);
      const submittedId = state.activePlayer.id;
      try {
        const res = await fetch("/api/player/update", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            ...JSON.parse(submittedPlayer),
            changed_fields: Array.from(state.playerDirtyFields, field =>
              field.startsWith("attr_") ? `attributes.${field.slice(5)}` : field)
          })
        });
        const data = await readJson(res);
        if (res.ok) {
          // An older request must not erase edits made while it was in flight.
          if (state.activePlayer.id === submittedId && JSON.stringify(state.activePlayer) === submittedPlayer) {
            state.playerDirtyFields.clear();
            if (data.player || data.active_player) displayPlayer(data.player || data.active_player);
            state.playerDirty = false;
            SaveBar.clear("player");
          }
          showToast(data.message || "Player saved.", "success");
          return true;
        } else {
          showToast(data.message || "Failed to save player", "error");
        }
      } catch (e) {
        showToast("Save error: " + e.message, "error");
      }
      return false;
    }

    const savePlayer = applyPlayerChanges;

