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
      showToast("Unavailable: loan recall has no in-game test yet", "warning");
      return false;
    }

    // Sends one experimental transfer request for the open player, then reloads that player from the game.
    async function runTransferAction(url, extra, confirmText) {
      const player = state.activePlayer;
      if (!state.connected || !player) { showToast("Attach to a game and open a player first", "warning"); return false; }
      if (!transferExperimentalOn()) { showToast("Tick the experimental checkbox first", "warning"); return false; }
      if (typeof canReplaceDraft === "function" && !canReplaceDraft("player")) return false;
      if (typeof confirm === "function" && !confirm(confirmText)) return false;
      try {
        const res = await fetch(url, {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ player_id: player.id, ...extra, experimental: true })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Transfer action failed");
        showToast(`${data.message} Advance a day and check the squad screens.`, "success");
        await reloadPlayerAfterTransfer(player.id);
        return true;
      } catch (error) {
        showToast(`Transfer action failed: ${error.message}`, "error");
        return false;
      }
    }

    async function reloadPlayerAfterTransfer(playerId) {
      try {
        const res = await fetch("/api/context/select", {
          method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ player_id: playerId })
        });
        const data = await readJson(res);
        const fresh = data.player || data.active_player;
        if (res.ok && fresh) { state.activePlayer = null; displayPlayer(fresh); }
      } catch (error) { /* the action itself succeeded; Load from game refreshes the profile */ }
    }

    function transferTargetClubId() {
      const raw = document.getElementById("transfer-target-club-id")?.value;
      const id = Number(raw);
      return String(raw ?? "").trim() !== "" && Number.isInteger(id) && id > 0 ? id : null;
    }

    // Read-only: shows the target club's first-team squad space (a transfer needs at least one spare slot).
    async function loadTransferSpace() {
      const note = document.getElementById("transfer-space-note");
      if (!note) return null;
      const club = transferTargetClubId();
      if (club === null || !state.connected) { note.innerText = ""; return null; }
      try {
        const res = await fetch("/api/club/squad-space", {
          method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ club_id: club })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Squad space unavailable");
        if (document.getElementById("transfer-target-club-id")?.value !== String(club)) return null;
        note.innerText = `First team: ${data.players} players, ${data.spare_slots} spare slot${data.spare_slots === 1 ? "" : "s"}${data.spare_slots === 0 ? " (full: transfers will be refused)" : ""}`;
        return data;
      } catch (error) {
        note.innerText = `Squad space unavailable: ${error.message}`;
        return null;
      }
    }

    async function terminateContract() {
      const name = state.activePlayer?.name || "this player";
      return runTransferAction("/api/player/contract/terminate", {},
        `Release ${name} as a free agent? This detaches the contract and removes the player from every squad. Use a throwaway save.`);
    }
    async function terminateLoan() { return unavailableTransfer(); }
    async function movePlayer() {
      const club = transferTargetClubId();
      if (club === null) { showToast("Enter the target club ID (a positive whole number)", "warning"); return false; }
      const feeRaw = String(document.getElementById("transfer-fee")?.value ?? "").trim();
      const fee = feeRaw === "" ? null : Number(feeRaw);
      if (fee !== null && (!Number.isInteger(fee) || fee < 1 || fee > 2147483647)) { showToast("The transfer fee must be a whole number of raw GBP from 1 to 2,147,483,647 (or empty for no fee)", "warning"); return false; }
      const name = state.activePlayer?.name || "this player";
      const feeText = fee === null ? "" : ` The fee of ${fee.toLocaleString()} moves from the buying club's balance to the selling club's.`;
      return runTransferAction("/api/transfer/move", fee === null ? { target_club_id: club } : { target_club_id: club, fee_gbp: fee },
        `Move ${name} to club ${club}?${feeText} The destination first-team squad needs a spare slot. Use a throwaway save.`);
    }
    async function signFreeAgent() {
      const club = transferTargetClubId();
      if (club === null) { showToast("Enter the target club ID (a positive whole number)", "warning"); return false; }
      const wage = Number(document.getElementById("transfer-sign-wage")?.value);
      const year = Number(document.getElementById("transfer-sign-year")?.value);
      if (!Number.isInteger(wage) || wage < 1 || wage > 2147483647) { showToast("Enter the weekly wage as a whole number of raw GBP", "warning"); return false; }
      if (!Number.isInteger(year) || year < 2000 || year > 2100) { showToast("Enter the contract expiry year (2000 to 2100)", "warning"); return false; }
      const name = state.activePlayer?.name || "this player";
      return runTransferAction("/api/transfer/sign", { target_club_id: club, wage_gbp: wage, expiry_year: year },
        `Sign ${name} to club ${club} at ${wage.toLocaleString()} per week until ${year}? The player must be a free agent and the squad needs a spare slot. Use a throwaway save.`);
    }
