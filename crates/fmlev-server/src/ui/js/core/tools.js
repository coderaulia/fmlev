    // Toast helper



    async function clubMoneyRequest(body) {
      if (!state.activeClub) throw new Error("Select a club first");
      const res = await fetch("/api/club/money", {
        method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ club_id: state.activeClub.club_id, ...body })
      });
      const data = await readJson(res);
      if (!res.ok) throw new Error(data.message || "Club money request failed");
      setMoneyInput(document.getElementById("club-money-balance"), data.club.balance);
      setMoneyInput(document.getElementById("club-money-transfer"), data.club.transfer_budget);
      state.loadedMoney = { club_id: data.club.club_id, balance: data.club.balance, transfer_budget: data.club.transfer_budget };
      return data;
    }
    async function loadClubMoney() {
      try { await clubMoneyRequest({}); showToast("Club money read from the game.", "success"); }
      catch (error) { showToast(error.message, "error"); }
    }
    async function saveClubMoney() {
      try {
        const read = id => readMoneyInput(document.getElementById(id), null);
        const body = { balance: read("club-money-balance"), transfer_budget: read("club-money-transfer") };
        if ([body.balance, body.transfer_budget].some(v => v !== null && !Number.isInteger(v))) throw new Error("Amounts must be whole numbers");
        if ([body.balance, body.transfer_budget].some(v => v !== null && Math.abs(v) > 2147483647)) throw new Error("Amounts must fit in 32 bits (max 2,147,483,647)");
        const loaded = state.loadedMoney && state.loadedMoney.club_id === state.activeClub?.club_id ? state.loadedMoney : null;
        if (loaded) {
          // Only send what changed since "Load from game"; the other field keeps its native value.
          if (body.balance === loaded.balance) body.balance = null;
          if (body.transfer_budget === loaded.transfer_budget) body.transfer_budget = null;
        }
        if (body.balance === null && body.transfer_budget === null) throw new Error("Nothing changed. Edit a value, or press Load from game to read the current ones.");
        const data = await clubMoneyRequest(body);
        showToast(data.message || "Done", "success");
      } catch (error) { showToast(error.message, "error"); }
    }

    async function maxFamiliarity(squad) {
      try {
        if (!state.activePlayer) throw new Error("Select a player first");
        const res = await fetch("/api/player/familiarity", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ player_id: state.activePlayer.id, squad })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Familiarity request failed");
        showToast(data.message || "Done", "success");
      } catch (error) { showToast(error.message, "error"); }
    }

    async function setRegistration(enabled) {
      try {
        const comp = state.activeCompetition;
        const live = comp ? (state.liveCompetitions || {})[comp.id] : null;
        if (!comp || !live || !live.unique_id) throw new Error("Open the competition's Rules page in FM26 first");
        const res = await fetch("/api/competition/registration", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ comp_index: comp.id, comp_uid: live.unique_id, enabled })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Registration request failed");
        showToast(data.message || "Done", "success");
      } catch (error) { showToast(error.message, "error"); }
    }
    async function extendContract(years) {
      try {
        if (!state.activePlayer) throw new Error("Select a player first");
        const res = await fetch("/api/player/contract/extend", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ player_id: state.activePlayer.id, years })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Contract extension failed");
        showToast(data.message || "Done", "success");
      } catch (error) { showToast(error.message, "error"); }
    }
    async function terminateContract() {
      try {
        if (!state.activePlayer) throw new Error("Select a player first");
        if (typeof confirm === "function" && !confirm(`Terminate ${state.activePlayer.name}'s contract and release as free agent?`)) return;
        const res = await fetch("/api/player/contract/terminate", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ player_id: state.activePlayer.id })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Contract termination failed");
        showToast(data.message || "Done", "success");
        if (typeof fetchContext === "function") await fetchContext();
      } catch (error) { showToast(error.message, "error"); }
    }
    async function terminateLoan() {
      try {
        if (!state.activePlayer) throw new Error("Select a player first");
        if (typeof confirm === "function" && !confirm(`Recall ${state.activePlayer.name} from loan back to parent club?`)) return;
        const res = await fetch("/api/player/loan/terminate", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ player_id: state.activePlayer.id })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Loan termination failed");
        showToast(data.message || "Done", "success");
        if (typeof fetchContext === "function") await fetchContext();
      } catch (error) { showToast(error.message, "error"); }
    }
    async function signFreeAgent() {
      try {
        if (!state.activePlayer) throw new Error("Select a player first");
        const clubInput = document.getElementById("transfer-target-club-id");
        const clubId = clubInput ? Number(clubInput.value) : 0;
        if (!clubId || !Number.isInteger(clubId)) throw new Error("Enter a valid target Club ID");
        if (typeof confirm === "function" && !confirm(`Sign free agent ${state.activePlayer.name} to club ${clubId}'s first team?`)) return;
        const res = await fetch("/api/transfer/sign", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ player_id: state.activePlayer.id, target_club_id: clubId })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Signing failed");
        showToast(data.message || "Done", "success");
        if (typeof fetchContext === "function") await fetchContext();
      } catch (error) { showToast(error.message, "error"); }
    }
    async function movePlayer() {
      try {
        if (!state.activePlayer) throw new Error("Select a player first");
        const clubInput = document.getElementById("transfer-target-club-id");
        const clubId = clubInput ? Number(clubInput.value) : 0;
        if (!clubId || !Number.isInteger(clubId)) throw new Error("Enter a valid target Club ID");
        if (typeof confirm === "function" && !confirm(`Move ${state.activePlayer.name} to club ${clubId}'s first team?`)) return;
        const res = await fetch("/api/transfer/move", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ player_id: state.activePlayer.id, target_club_id: clubId })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Player move failed");
        showToast(data.message || "Done", "success");
        if (typeof fetchContext === "function") await fetchContext();
      } catch (error) { showToast(error.message, "error"); }
    }
    async function saveSquadNumber() {
      try {
        if (!state.activePlayer) throw new Error("Select a player first");
        const rawText = document.getElementById("squad-number-input").value.trim();
        if (rawText === "") throw new Error("Enter a squad number (use 0 to clear it)");
        const raw = Number(rawText);
        if (!Number.isInteger(raw) || raw < 0 || raw > 99) throw new Error("Squad number must be 0-99");
        const res = await fetch("/api/player/squad-number", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ player_id: state.activePlayer.id, number: raw || null })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Squad number save failed");
        showToast(data.message || "Done", "success");
      } catch (error) { showToast(error.message, "error"); }
    }
    async function prizeRequest(prizes) {
      for (const [id, label] of [["prize-teams", "team count"], ["prize-first", "first prize"], ["prize-last", "last prize"]]) {
        const v = Number(document.getElementById(id).value);
        if (!Number.isInteger(v) || v <= 0) throw new Error(`Enter the ${label} shown on the Rules page first`);
      }
      const body = {
        teams: Number(document.getElementById("prize-teams").value),
        first_prize: Number(document.getElementById("prize-first").value),
        last_prize: Number(document.getElementById("prize-last").value),
        tolerance: Number(document.getElementById("prize-tol").value || 0)
      };
      if (prizes) body.prizes = prizes;
      const res = await fetch("/api/competition/prizes", {
        method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body)
      });
      const data = await readJson(res);
      if (!res.ok) throw new Error(data.message || "Prize request failed");
      return data;
    }
    async function loadPrizeTable() {
      try {
        const data = await prizeRequest();
        document.getElementById("prize-list").value = data.prizes.join("\n");
        showToast(data.message || "Done", "success");
      } catch (error) { showToast(error.message, "error"); }
    }
    async function savePrizeTable() {
      try {
        const amounts = document.getElementById("prize-list").value.split(/\s+/).filter(Boolean).map(Number);
        if (!amounts.length) throw new Error("Enter one prize amount per line, or use Load from game first");
        if (amounts.some(v => !Number.isInteger(v) || v < 0)) throw new Error("Prize amounts must be whole numbers");
        const data = await prizeRequest(amounts);
        document.getElementById("prize-list").value = data.prizes.join("\n");
        showToast(data.message || "Done", "success");
      } catch (error) { showToast(error.message, "error"); }
    }
