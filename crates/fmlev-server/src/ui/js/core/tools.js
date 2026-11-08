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

    // Target club picker: the user types a club name, the search index lists matching clubs, and picking one
    // stores the native club index in the hidden #transfer-target-club-id. Typing again clears the pick, so a
    // stale id is never sent. A bare number is still accepted as a club index.
    let transferClubSearchSeq = 0;
    let transferClubSearchTimer = null;
    function setTransferClub(id, label) {
      const hidden = document.getElementById("transfer-target-club-id");
      if (hidden) hidden.value = id ? String(id) : "";
      const search = document.getElementById("transfer-target-club-search");
      if (search && label !== undefined) search.value = label;
      updateTransferButtons();
      loadTransferSpace();
    }
    function hideTransferClubResults() {
      const list = document.getElementById("transfer-target-club-results");
      if (list) list.style.display = "none";
    }
    function clubLabel(club) {
      const name = club.club_name || club.name || `Club ${club.club_id}`;
      return club.league_name ? `${name} (${club.league_name})` : name;
    }
    function searchTransferClubs(text) {
      const query = String(text ?? "").trim();
      const typedId = /^\d+$/.test(query) ? Number(query) : null;
      setTransferClub(typedId && typedId > 0 ? typedId : null);
      clearTimeout(transferClubSearchTimer);
      const seq = ++transferClubSearchSeq;
      if (query.length < 2 || typedId !== null) { hideTransferClubResults(); return Promise.resolve([]); }
      return new Promise(resolve => {
        transferClubSearchTimer = setTimeout(async () => {
          try {
            const res = await fetch(`/api/search?q=${encodeURIComponent(query)}&kind=clubs&limit=12`);
            const data = await readJson(res);
            if (seq !== transferClubSearchSeq) return resolve([]);
            resolve(showTransferClubResults(res.ok ? (data.clubs || []) : []));
          } catch (error) { if (seq === transferClubSearchSeq) hideTransferClubResults(); resolve([]); }
        }, 200);
      });
    }
    function showTransferClubResults(clubs) {
      const list = document.getElementById("transfer-target-club-results");
      if (!list) return clubs;
      list.innerHTML = "";
      const usable = clubs.filter(club => Number.isInteger(club.club_id) && club.club_id > 0);
      for (const club of usable) {
        const option = document.createElement("option");
        option.value = String(club.club_id);
        option.textContent = clubLabel(club);
        list.appendChild(option);
      }
      if (!usable.length) {
        const none = document.createElement("option");
        none.value = ""; none.disabled = true; none.textContent = "No club found (rebuild the index if it was just created)";
        list.appendChild(none);
      }
      list.style.display = "";
      return usable;
    }
    function pickTransferClub(value) {
      const id = Number(value);
      if (!Number.isInteger(id) || id <= 0) return;
      const list = document.getElementById("transfer-target-club-results");
      const option = list ? Array.from(list.options || []).find(o => o.value === String(id)) : null;
      hideTransferClubResults();
      setTransferClub(id, option ? option.textContent : String(id));
    }
    // Arrow down moves into the list, Enter picks the first match, Escape closes the list.
    function transferClubKey(event) {
      const list = document.getElementById("transfer-target-club-results");
      if (!list || list.style.display === "none") return;
      if (event.key === "Escape") { hideTransferClubResults(); return; }
      if (event.key === "ArrowDown" && typeof list.focus === "function") { event.preventDefault(); list.selectedIndex = 0; list.focus(); }
      if (event.key === "Enter") {
        const first = Array.from(list.options || []).find(o => o.value);
        if (first) { event.preventDefault(); pickTransferClub(first.value); }
      }
    }
    function transferClubName() {
      const search = document.getElementById("transfer-target-club-search")?.value;
      const club = transferTargetClubId();
      return search && club !== null && search.trim() !== String(club) ? search.trim() : `club ${club}`;
    }

    // Read-only: shows the target club's first-team squad space (a transfer needs at least one spare slot).
    let fullTransferClub = null;
    async function loadTransferSpace() {
      const note = document.getElementById("transfer-space-note");
      if (!note) return null;
      const club = transferTargetClubId();
      if (club === null || !state.connected) { note.innerText = ""; fullTransferClub = null; return null; }
      try {
        const res = await fetch("/api/club/squad-space", {
          method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ club_id: club })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Squad space unavailable");
        if (document.getElementById("transfer-target-club-id")?.value !== String(club)) return null;
        note.innerText = `First team: ${data.players} players, ${data.spare_slots} spare slot${data.spare_slots === 1 ? "" : "s"}${data.spare_slots === 0 ? " (full)" : ""}`;
        fullTransferClub = data.spare_slots === 0 ? club : null;
        updateTransferButtons();
        return data;
      } catch (error) {
        if (transferTargetClubId() !== club) return null;
        fullTransferClub = null;
        updateTransferButtons();
        note.innerText = `Squad space unavailable: ${error.message}`;
        return null;
      }
    }

    async function terminateContract() {
      const name = state.activePlayer?.name || "this player";
      return runTransferAction("/api/player/contract/terminate", {},
        `Release ${name} as a free agent? This detaches the contract and removes the player from every squad. Keep a backup save.`);
    }
    async function terminateLoan() { return unavailableTransfer(); }
    // Club, yearly wage and expiry year are required for Move. The fee is optional.
    // The game stores a weekly wage: the yearly amount divided by 52, rounded to whole GBP.
    function transferFormState() {
      const read = id => { const raw = String(document.getElementById(id)?.value ?? "").trim(); return raw === "" ? null : Number(raw); };
      const club = read("transfer-target-club-id"), yearly = read("transfer-sign-wage"), year = read("transfer-sign-year"), fee = read("transfer-fee");
      const wage = yearly === null ? null : Math.round(yearly / 52);
      const missing = [];
      if (!(Number.isInteger(club) && club > 0)) missing.push("target club");
      if (!(Number.isInteger(yearly) && yearly >= 26 && wage >= 1 && wage <= 2147483647)) missing.push("yearly wage");
      if (!(Number.isInteger(year) && year >= 2000 && year <= 2100)) missing.push("expiry year");
      const feeValid = fee === null || (Number.isInteger(fee) && fee >= 1 && fee <= 2147483647);
      return { club, yearly, wage, year, fee, feeValid, missing, full: club !== null && club === fullTransferClub };
    }
    function refuseMissing(form) {
      showToast(`Required: ${form.missing.join(", ")}. All three must be filled, or the contract would be incomplete.`, "warning");
      return false;
    }
    async function movePlayer() {
      const form = transferFormState();
      if (form.missing.length) return refuseMissing(form);
      if (form.full) { showToast("The destination first team is full.", "warning"); return false; }
      if (!form.feeValid) { showToast("The transfer fee must be a whole number of raw GBP from 1 to 2,147,483,647 (or empty for no fee)", "warning"); return false; }
      const name = state.activePlayer?.name || "this player";
      const feeText = form.fee === null ? "" : ` The fee of ${form.fee.toLocaleString()} moves from the buying club's balance to the selling club's.`;
      const body = { target_club_id: form.club, wage_gbp: form.wage, expiry_year: form.year };
      if (form.fee !== null) body.fee_gbp = form.fee;
      return runTransferAction("/api/transfer/move", body,
        `Move ${name} to ${transferClubName()} on ${form.yearly.toLocaleString()} per year (${form.wage.toLocaleString()} per week) until ${form.year}?${feeText} His current wage and expiry are replaced. Keep a backup save.`);
    }
