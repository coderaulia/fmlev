    // Transfer/contract actions need a live attached game and an open player (each asks for confirmation).
    // Loan recall has no in-game test yet, so it stays disabled.
    function updateTransferButtons() {
      const ready = !!state.connected && !!state.activePlayer;
      const why = !state.connected ? "Attach to a game first" : "Open a player first";
      const form = transferFormState();
      const terminate = document.getElementById("btn-terminate-contract");
      if (terminate) { terminate.disabled = !ready; terminate.title = ready ? "Asks for confirmation" : `Unavailable: ${why}`; }
      const move = document.getElementById("btn-move-player");
      if (move) {
        const blocked = !ready ? why : form.missing.length ? `Fill the required fields: ${form.missing.join(", ")}` : form.full ? "The destination first team is full" : (!form.feeValid ? "The fee must be a whole number of raw GBP from 1 to 2,147,483,647 (or empty)" : "");
        move.disabled = !!blocked;
        move.title = blocked ? `Unavailable: ${blocked}` : "Asks for confirmation";
      }
      // Mark empty or invalid required fields and say which are missing.
      const fields = [["transfer-target-club-id", "target club"], ["transfer-target-club-search", "target club"], ["transfer-sign-wage", "yearly wage"], ["transfer-sign-year", "expiry year"]];
      for (const [id, label] of fields) {
        const input = document.getElementById(id);
        if (input && input.classList) input.classList.toggle("field-missing", form.missing.includes(label));
      }
      const fee = document.getElementById("transfer-fee");
      if (fee && fee.classList) fee.classList.toggle("field-missing", !form.feeValid);
      const note = document.getElementById("transfer-missing-note");
      if (note) note.innerText = ready && form.missing.length ? `Required before Move: ${form.missing.join(", ")}.` : "";
      const loan = document.getElementById("btn-terminate-loan");
      if (loan) { loan.disabled = true; loan.title = "Unavailable: loan recall has no in-game test yet"; }
    }
    function adjustClubField(id, delta) {
      const el = document.getElementById(id);
      if (!el || el.disabled) return;
      adjustMoneyInput(id, delta, 0);
      markClubUnsaved();
    }
    function adjustClubBalance(delta) { adjustClubField("club-balance", delta); }
    function adjustTransferBudget(delta) { adjustClubField("club-transfer-budget", delta); }
    function adjustWageBudget(delta) { adjustClubField("club-wage-budget", delta); }

    // Nation and competition native refreshes are owned by their page scripts.

    function setFacVal(sliderId, dispId, val) {
      const s = document.getElementById(sliderId);
      const d = document.getElementById(dispId);
      if (s) s.value = val;
      if (d) {
        d.innerText = val;
        d.className = `facility-val ${getTierClass(val)}`;
      }
    }

    function updateFacVal(dispId, val) {
      const d = document.getElementById(dispId);
      if (d) {
        d.innerText = val;
        d.className = `facility-val ${getTierClass(val)}`;
      }
    }

    function getStarString(ability) {
      const val = parseInt(ability, 10) || 0;
      if (val <= 0) return "—";
      const stars = Math.min(5, Math.max(1, Math.round(val / 40)));
      return "★".repeat(stars) + "☆".repeat(5 - stars);
    }

    function getReputationStars(rep) {
      const stars = Math.min(5, Math.max(1, Math.round(rep / 2000)));
      return "★".repeat(stars) + "☆".repeat(5 - stars);
    }

    function getCountryFlag(nat) {
      if (!nat) return "🌐";
      const map = {
        "Norway": "🇳🇴",
        "England": "🏴󠁧󠁢󠁥󠁮󠁧󠁿",
        "Spain": "🇪🇸",
        "France": "🇫🇷",
        "Brazil": "🇧🇷",
        "Germany": "🇩🇪",
        "Argentina": "🇦🇷",
        "Portugal": "🇵🇹",
        "Netherlands": "🇳🇱",
        "Italy": "🇮🇹",
        "Belgium": "🇧🇪",
        "Croatia": "🇭🇷",
        "Denmark": "🇩🇰",
        "Sweden": "🇸🇪",
        "Scotland": "🏴󠁧󠁢󠁳󠁣󠁴󠁿",
        "Wales": "🏴󠁧󠁢󠁷󠁬󠁳󠁿",
        "United States": "🇺🇸",
        "USA": "🇺🇸",
        "Japan": "🇯🇵",
      };
      return map[nat] || "🌐";
    }



    let playerMoraleRead = null;
    let playerMoraleBusy = false;

    function renderPlayerMorale() {
      const current = playerMoraleRead?.player_id === state.activePlayer?.id ? playerMoraleRead : null;
      const input = document.getElementById("player-morale-value");
      const apply = document.getElementById("player-morale-apply");
      const status = document.getElementById("player-morale-status");
      const available = state.connected && current && !playerMoraleBusy;
      if (input) { input.disabled = !available; input.value = current?.value ?? ""; }
      if (apply) apply.disabled = !available;
      if (status) status.innerText = current ? `Live morale: ${current.value}/20` : "Read morale from FM26 to edit.";
    }

    function clearPlayerMorale() {
      playerMoraleRead = null;
      renderPlayerMorale();
    }

    async function requestPlayerMorale(method, body) {
      const res = await fetch("/api/player/morale", {
        method, headers: { "Content-Type": "application/json" }, body: JSON.stringify(body)
      });
      const data = await readJson(res);
      if (!res.ok) throw new Error(data.message || "Morale request failed.");
      if (data.player_id !== body.player_id || !(data.player_uid > 0) || !Number.isInteger(data.value)
          || data.value < 1 || data.value > 20) throw new Error("Native morale response was invalid.");
      return data;
    }

    async function loadPlayerMorale() {
      const id = state.activePlayer?.id;
      if (playerMoraleBusy || !state.connected || !id) return false;
      playerMoraleBusy = true;
      clearPlayerMorale();
      try {
        const data = await requestPlayerMorale("POST", { player_id: id });
        if (!state.connected || state.activePlayer?.id !== id) return false;
        playerMoraleRead = data;
        return true;
      } catch (error) {
        showToast(error.message, "error");
        return false;
      } finally {
        playerMoraleBusy = false;
        renderPlayerMorale();
      }
    }

    async function applyPlayerMorale() {
      const current = playerMoraleRead;
      if (playerMoraleBusy || !state.connected || !current || current.player_id !== state.activePlayer?.id) return false;
      const value = Number(document.getElementById("player-morale-value")?.value);
      if (!Number.isInteger(value) || value < 1 || value > 20) { showToast("Enter morale from 1 to 20.", "warning"); return false; }
      if (value === current.value) return false;
      if (typeof confirm === "function" && !confirm(`Set ${state.activePlayer.name || "this player's"} morale to ${value}/20 now?`)) return false;
      playerMoraleBusy = true;
      try {
        const data = await requestPlayerMorale("PUT", {
          player_id: current.player_id, player_uid: current.player_uid, expected: current.value, value
        });
        if (state.connected && state.activePlayer?.id === current.player_id) playerMoraleRead = data;
        showToast("Player morale saved.", "success");
        return true;
      } catch (error) {
        if (state.activePlayer?.id === current.player_id) playerMoraleRead = null;
        showToast(error.message, "error");
        return false;
      } finally {
        playerMoraleBusy = false;
        renderPlayerMorale();
      }
    }
