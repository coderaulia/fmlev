    // Native actions are identity-bound drafts. Only Save dispatches the queued request.
    function stagePlayerAction(url, body, label) {
      if (!state.activePlayer) { showToast("Select a player first", "warning"); return false; }
      const key = `player:${state.activePlayer.id}`;
      const queue = (pendingActions.get(key) || []).filter(action => action.url !== url);
      if (queue.length) pendingActions.set(key, queue); else pendingActions.delete(key);
      const result = queuePendingAction("player", url, { player_id: state.activePlayer.id, ...body }, label);
      SaveBar.markDirty("player");
      return result;
    }
    function stageSquadNumber() {
      const raw = String(document.getElementById("squad-number-input")?.value ?? "").trim();
      const number = Number(raw);
      if (!raw || !Number.isInteger(number) || number < 0 || number > 99) { showToast("Squad number must be 0–99 (0 clears it)", "warning"); return false; }
      return stagePlayerAction("/api/player/squad-number", { number: number || null }, `Squad number: ${number || "none"}`);
    }
    function stageContractExtension(years = 1) {
      if (!Number.isInteger(years) || years < 1 || years > 10) return false;
      return stagePlayerAction("/api/player/contract/extend", { years }, `Extend parent contract ${years} year(s)`);
    }
    function stageFamiliarity(squad) {
      return stagePlayerAction("/api/player/familiarity", { squad: !!squad }, squad ? "Max squad familiarity" : "Max player familiarity");
    }
    async function saveSquadNumber() { return stageSquadNumber(); }
    async function extendContract(years) { return stageContractExtension(years); }
    async function maxFamiliarity(squad) { return stageFamiliarity(squad); }
    async function setRegistration(enabled) { return stageCompetitionRegistration(enabled); }
    function unavailableTransfer() {
      showToast("Unavailable: transfer writes have not passed the day-tick and save/reload evidence checks", "warning");
      return false;
    }
    async function terminateContract() { return unavailableTransfer(); }
    async function terminateLoan() { return unavailableTransfer(); }
    async function movePlayer() { return unavailableTransfer(); }
    async function signFreeAgent() { return unavailableTransfer(); }
