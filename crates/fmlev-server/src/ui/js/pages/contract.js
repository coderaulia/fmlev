    // Contract Management
    function adjustWage(delta) {
      const el = document.getElementById("contract-wage");
      adjustMoneyInput("contract-wage", delta, 1000);
      autoSaveContract("wage_per_week");
    }

    function autoSaveContract(fieldName) {
      if (fieldName) {
        state.playerDirtyFields.add("contract." + fieldName);
      } else {
        state.playerDirtyFields.add("contract");
      }
      markUnsaved();
    }

    async function saveContractToGame() {
      return applyPlayerChanges();
    }


    // Fills release clause, squad status and happiness. Connected mode shows only what was read from the game's
    // contract (nothing is invented); offline fixtures keep their demonstration values.
    function renderContractTerms(player) {
      const live = !!state.connected;
      const contract = player.contract;
      // Read-only facts about the native read; kept current even when the player's other fields are not replaced.
      if (state.activePlayer) {
        state.activePlayer.contract = state.activePlayer.contract || {};
        for (const key of ["squad_status_code", "release_clause_present", "terms_note"]) state.activePlayer.contract[key] = contract[key];
      }
      const cRel = document.getElementById("contract-release");
      if (cRel && document.activeElement !== cRel && !state.playerDirtyFields.has("contract.release_clause")) {
        const present = !live || contract.release_clause_present === true;
        if (present) setMoneyInput(cRel, contract.release_clause || 0); else { cRel.value = ""; }
        cRel.placeholder = !live ? "Unavailable" : contract.release_clause_present === false ? "No release clause" : "Unavailable";
        state.activePlayer.contract.release_clause = present ? (contract.release_clause || 0) : 0;
      }
      const cStat = document.getElementById("contract-status");
      if (cStat && document.activeElement !== cStat && !state.playerDirtyFields.has("contract.squad_status")) {
        const status = live ? (contract.squad_status || "") : (contract.squad_status || "Regular Starter");
        // A native code without a confirmed label (for example "Code 10") gets its own read-only option.
        if (status && !Array.from(cStat.options || []).some(option => option.value === status)) {
          const extra = document.createElement ? document.createElement("option") : null;
          if (extra) { extra.value = status; extra.textContent = status; extra.dataset.extra = "true"; cStat.appendChild(extra); }
        }
        cStat.value = status;
        state.activePlayer.contract.squad_status = status;
      }
      const cHap = document.getElementById("contract-happiness");
      if (cHap && document.activeElement !== cHap && !state.playerDirtyFields.has("contract.happiness")) {
        // Happiness is derived by the game; there is no native value to show in connected mode.
        const happiness = live ? "" : (contract.happiness || "Content");
        cHap.value = happiness;
        state.activePlayer.contract.happiness = happiness;
      }
    }

    function setReleaseClauseFree() {
      const input = document.getElementById("contract-release");
      if (state.connected) { showToast("A release clause of 0 is not accepted natively; enter an amount of 1 or more.", "warning"); return false; }
      if (!input || input.disabled || !playerFieldWritable("contract.release_clause")) return false;
      setMoneyInput(input, 0);
      autoSaveContract("release_clause");
      return true;
    }
    // Squad status codes whose labels were confirmed on the game's screens; other options are offline-only.
    const VERIFIED_SQUAD_STATUSES = ["Star Player", "Important Player", "Regular Starter", "Squad Player", "Fringe Player"];
    const CONTRACT_GATES = [
      ["contract-wage", "wage_per_week"], ["contract-expiry", "expiry_year"],
      ["contract-release", "release_clause", "contract-release-note"],
      ["contract-status", "squad_status", "contract-status-note"],
      ["contract-happiness", "happiness", "contract-happiness-note"],
    ];
    function contractGateReason(field) {
      const contract = state.activePlayer?.contract || {};
      if (field === "happiness") return "Unavailable: the game derives happiness from several factors (playing time, hierarchy, promises); there is no native value to edit";
      if (contract.terms_note && (field === "release_clause" || field === "squad_status")) return `Unavailable: ${contract.terms_note}`;
      if (field === "release_clause" && contract.release_clause_present === false)
        return "This contract has no release clause. Adding or removing one is not supported; only an existing clause can be edited";
      return "Unavailable: no verified native write path";
    }
    function applyContractGates() {
      for (const [id, field, note] of CONTRACT_GATES) {
        const input = document.getElementById(id), allowed = playerFieldWritable("contract." + field);
        const reason = contractGateReason(field);
        if (input) { input.disabled = !allowed; input.title = allowed ? "Pending until Save" : reason; }
        const label = note && document.getElementById(note);
        if (label) { label.hidden = allowed; label.innerText = allowed ? "" : reason; }
      }
      // Connected mode offers only the squad statuses confirmed against the game (plus the current native value).
      const status = document.getElementById("contract-status");
      for (const option of Array.from(status?.options || [])) {
        option.disabled = !!state.connected && option.value !== "" && !VERIFIED_SQUAD_STATUSES.includes(option.value) && option.value !== status.value;
      }
    }
