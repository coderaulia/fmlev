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

    // Sends one transfer request for the open player (after confirmation), then reloads that player from the game.
    async function runTransferAction(url, extra, confirmText) {
      const player = state.activePlayer;
      if (!state.connected || !player) { showToast("Attach to a game and open a player first", "warning"); return false; }
      if (typeof canReplaceDraft === "function" && !canReplaceDraft("player")) return false;
      if (typeof confirm === "function" && !confirm(confirmText)) return false;
      try {
        const res = await fetch(url, {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ player_id: player.id, ...extra })
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
      if (club === null || !state.connected) { note.innerText = ""; fillReleaseSelect(null); return null; }
      try {
        const res = await fetch("/api/club/squad-space", {
          method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ club_id: club })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Squad space unavailable");
        if (document.getElementById("transfer-target-club-id")?.value !== String(club)) return null;
        note.innerText = `First team: ${data.players} players, ${data.spare_slots} spare slot${data.spare_slots === 1 ? "" : "s"}${data.spare_slots === 0 ? " (full: pick a player to release below)" : ""}`;
        fillReleaseSelect(data);
        return data;
      } catch (error) {
        fillReleaseSelect(null);
        note.innerText = `Squad space unavailable: ${error.message}`;
        return null;
      }
    }

    // A full squad list cannot grow, so the user picks one destination player to release as a free agent.
    function fillReleaseSelect(data) {
      const select = document.getElementById("transfer-release-player");
      if (!select) return;
      const full = !!data && data.spare_slots === 0 && Array.isArray(data.members) && data.members.length > 0;
      select.style.display = full ? "" : "none";
      select.innerHTML = "";
      if (!full) return;
      const none = document.createElement("option");
      none.value = ""; none.textContent = "Release which player to make room? *";
      select.appendChild(none);
      for (const member of data.members) {
        const option = document.createElement("option");
        option.value = String(member.id);
        option.textContent = `${member.name || "Player"} (id ${member.id})`;
        select.appendChild(option);
      }
      select.onchange = () => { if (typeof updateTransferButtons === "function") updateTransferButtons(); };
    }
    function transferReleaseId() {
      const select = document.getElementById("transfer-release-player");
      if (!select || select.style.display === "none") return null;
      const id = Number(select.value);
      return Number.isInteger(id) && id > 0 ? id : null;
    }
    function releaseNote() {
      const select = document.getElementById("transfer-release-player");
      const id = transferReleaseId();
      if (id === null || !select) return "The destination squad has a free slot.";
      return `The destination squad is full, so ${select.options[select.selectedIndex].textContent} is released as a free agent first.`;
    }
    function releaseRequired() {
      const select = document.getElementById("transfer-release-player");
      return !!select && select.style.display !== "none" && transferReleaseId() === null;
    }

    async function terminateContract() {
      const name = state.activePlayer?.name || "this player";
      return runTransferAction("/api/player/contract/terminate", {},
        `Release ${name} as a free agent? This detaches the contract and removes the player from every squad. Keep a backup save.`);
    }
    async function terminateLoan() { return unavailableTransfer(); }
    // One reading of the transfer form. Club, yearly wage and expiry year are required for Move and Sign; the fee is
    // optional (Move only). The game stores a weekly wage: the yearly amount divided by 52, rounded to whole GBP.
    function transferFormState() {
      const read = id => { const raw = String(document.getElementById(id)?.value ?? "").trim(); return raw === "" ? null : Number(raw); };
      const club = read("transfer-target-club-id"), yearly = read("transfer-sign-wage"), year = read("transfer-sign-year"), fee = read("transfer-fee");
      const wage = yearly === null ? null : Math.round(yearly / 52);
      const missing = [];
      if (releaseRequired()) missing.push("player to release (the destination squad is full)");
      if (!(Number.isInteger(club) && club > 0)) missing.push("target club ID");
      if (!(Number.isInteger(yearly) && yearly >= 26 && wage >= 1 && wage <= 2147483647)) missing.push("yearly wage");
      if (!(Number.isInteger(year) && year >= 2000 && year <= 2100)) missing.push("expiry year");
      const feeValid = fee === null || (Number.isInteger(fee) && fee >= 1 && fee <= 2147483647);
      return { club, yearly, wage, year, fee, feeValid, missing };
    }
    function refuseMissing(form) {
      showToast(`Required: ${form.missing.join(", ")}. All three must be filled, or the contract would be incomplete.`, "warning");
      return false;
    }
    async function movePlayer() {
      const form = transferFormState();
      if (form.missing.length) return refuseMissing(form);
      if (!form.feeValid) { showToast("The transfer fee must be a whole number of raw GBP from 1 to 2,147,483,647 (or empty for no fee)", "warning"); return false; }
      const name = state.activePlayer?.name || "this player";
      const feeText = form.fee === null ? "" : ` The fee of ${form.fee.toLocaleString()} moves from the buying club's balance to the selling club's.`;
      const body = { target_club_id: form.club, wage_gbp: form.wage, expiry_year: form.year };
      if (form.fee !== null) body.fee_gbp = form.fee;
      const releaseId = transferReleaseId();
      if (releaseId !== null) body.release_player_id = releaseId;
      return runTransferAction("/api/transfer/move", body,
        `Move ${name} to club ${form.club} on ${form.yearly.toLocaleString()} per year (${form.wage.toLocaleString()} per week) until ${form.year}?${feeText} His current wage and expiry are replaced. ${releaseNote()} Keep a backup save.`);
    }
    async function signFreeAgent() {
      const form = transferFormState();
      if (form.missing.length) return refuseMissing(form);
      const name = state.activePlayer?.name || "this player";
      const signBody = { target_club_id: form.club, wage_gbp: form.wage, expiry_year: form.year };
      const signRelease = transferReleaseId();
      if (signRelease !== null) signBody.release_player_id = signRelease;
      return runTransferAction("/api/transfer/sign", signBody,
        `Sign ${name} to club ${form.club} at ${form.yearly.toLocaleString()} per year (${form.wage.toLocaleString()} per week) until ${form.year}? The player must be a free agent and the squad needs a spare slot. Keep a backup save.`);
    }
