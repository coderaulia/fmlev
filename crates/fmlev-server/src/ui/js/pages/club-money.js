    // ---- Club money (balance + transfer budget). Native read/write through /api/club/money. ----
    async function clubMoneyRequest(body) {
      if (!state.activeClub) throw new Error("Select a club first");
      const clubId = state.activeClub.club_id;
      const ids = ["club-money-balance", "club-money-transfer"];
      const before = ids.map(id => document.getElementById(id)?.value);
      const res = await fetch("/api/club/money", {
        method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ club_id: clubId, ...body })
      });
      const data = await readJson(res);
      if (!res.ok) throw new Error(data.message || "Club money request failed");
      if (state.activeClub?.club_id !== clubId) throw new Error("Club changed while reading money; load the selected club again");
      if (!Number.isInteger(data.club?.balance) || !Number.isInteger(data.club?.transfer_budget))
        throw new Error("Native club money is unavailable");
      ids.forEach((id, index) => {
        const input = document.getElementById(id);
        if (!input) return;
        const current = input.value;
        setMoneyInput(input, data.club[index === 0 ? "balance" : "transfer_budget"]);
        if (current !== before[index]) input.value = current;
        input.disabled = false;
      });
      state.loadedMoney = { club_id: data.club.club_id, balance: data.club.balance, transfer_budget: data.club.transfer_budget };
      return data;
    }
    async function loadClubMoney() {
      try { await clubMoneyRequest({}); showToast("Club money read from the game.", "success"); return true; }
      catch (error) { showToast(error.message, "error"); return false; }
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
        return true;
      } catch (error) { showToast(error.message, "error"); return false; }
    }

    function clubMoneyChanges() {
      const loaded = state.loadedMoney;
      if (!loaded || loaded.club_id !== state.activeClub?.club_id) return [];
      return [["club-money-balance", "balance", "Balance"], ["club-money-transfer", "transfer_budget", "Transfer budget"]]
        .flatMap(([id, field, label]) => {
          const input = document.getElementById(id);
          if (!input || input.disabled) return [];
          const value = readMoneyInput(input, null);
          return value === loaded[field] ? [] : [{ label, from: loaded[field], to: value }];
        });
    }

    function clubPageChanges() {
      const club = state.activeClub;
      if (!club) return [];
      const changes = clubMoneyChanges();
      for (const [id, field, label] of CLUB_FACILITIES) {
        const input = document.getElementById(id);
        if (input && !input.disabled && Number(input.value) !== club[field])
          changes.push({ label, from: club[field], to: input.value });
      }
      const repInput = document.getElementById("club-reputation");
      if (repInput && !repInput.disabled && Number(repInput.value) !== club.reputation)
        changes.push({ label: "Club reputation", from: club.reputation, to: repInput.value });
      if (state.clubDirty && !changes.length) changes.push({ label: "Club details" });
      for (const action of pendingActions.get(`club:${club.club_id}`) || []) changes.push({ label: action.label });
      return changes;
    }

    async function loadClubPage() {
      const id = state.activeClub?.club_id;
      if (!id) throw new Error("Select a club first");
      const res = await fetch("/api/context/select", { method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ club_id: id }) });
      const data = await readJson(res);
      if (!res.ok) throw new Error(data.message || "Club load failed");
      if (state.activeClub?.club_id !== id) throw new Error("Club changed while loading; retry for the selected club");
      const club = data.club || data.active_club;
      if (!club) throw new Error("No club returned by Load from game");
      state.clubDirty = false;
      pendingActions.delete(`club:${id}`);
      nativeClubSyncedId = null;
      displayClub(club);
      // displayClub may start a read-only native refresh. Explicit Load also waits for its own read.
      nativeClubSyncedId = null;
      if (state.connected && !await syncClubNative(id)) return false;
      return loadClubMoney();
    }

    async function saveClubPage() {
      const id = state.activeClub?.club_id;
      if (!id) return false;
      const queued = [...(pendingActions.get(`club:${id}`) || [])];
      const moneyDraft = JSON.stringify(clubMoneyChanges());
      // Manual edits (including money) finish before any queued preset/affiliate request.
      if (state.clubDirty && !await writeClubChanges()) return false;
      if (state.activeClub?.club_id !== id || state.clubDirty) return false;
      if (JSON.stringify(clubMoneyChanges()) !== moneyDraft) return false;
      if (clubMoneyChanges().length && !await saveClubMoney()) return false;
      if (state.activeClub?.club_id !== id || clubMoneyChanges().length) return false;
      const currentQueue = pendingActions.get(`club:${id}`) || [];
      if (queued.length !== currentQueue.length || queued.some((action, i) => action !== currentQueue[i])) return false;
      return saveClubToGame();
    }

    SaveBar.register("club", { load: loadClubPage, save: saveClubPage, discard: loadClubPage, changes: clubPageChanges });
