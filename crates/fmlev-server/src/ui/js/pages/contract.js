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


    function setReleaseClauseFree() {
      const input = document.getElementById("contract-release");
      if (!input || input.disabled || !playerFieldWritable("contract.release_clause")) return false;
      setMoneyInput(input, 0);
      autoSaveContract("release_clause");
      return true;
    }
    function applyContractGates() {
      for (const [id, field, note] of [
        ["contract-wage", "wage_per_week"], ["contract-expiry", "expiry_year"],
        ["contract-release", "release_clause", "contract-release-note"],
        ["contract-status", "squad_status", "contract-status-note"],
        ["contract-happiness", "happiness", "contract-happiness-note"],
      ]) {
        const input = document.getElementById(id), allowed = playerFieldWritable("contract." + field);
        if (input) { input.disabled = !allowed; input.title = allowed ? "Pending until Save" : "Unavailable: no verified native write path"; }
        const label = note && document.getElementById(note);
        if (label) { label.hidden = allowed; label.innerText = allowed ? "" : "Unavailable: no verified native write path"; }
      }
    }
