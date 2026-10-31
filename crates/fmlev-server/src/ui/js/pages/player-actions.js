    // Transfer/contract actions need a live attached game, an open player and the explicit experimental opt-in.
    // Loan recall has no in-game test yet, so it stays disabled.
    function transferExperimentalOn() { return !!document.getElementById("transfer-experimental")?.checked; }
    function updateTransferButtons() {
      const ready = !!state.connected && !!state.activePlayer && transferExperimentalOn();
      const why = !state.connected ? "Attach to a game first" : !state.activePlayer ? "Open a player first"
        : "Tick the experimental checkbox first";
      const form = transferFormState();
      const terminate = document.getElementById("btn-terminate-contract");
      if (terminate) { terminate.disabled = !ready; terminate.title = ready ? "Experimental: throwaway save only" : `Unavailable: ${why}`; }
      for (const [id, needsFee] of [["btn-move-player", true], ["btn-sign-free-agent", false]]) {
        const el = document.getElementById(id);
        if (!el) continue;
        const blocked = !ready ? why : form.missing.length ? `Fill the required fields: ${form.missing.join(", ")}` : (needsFee && !form.feeValid ? "The fee must be a whole number of raw GBP from 1 to 2,147,483,647 (or empty)" : "");
        el.disabled = !!blocked;
        el.title = blocked ? `Unavailable: ${blocked}` : "Experimental: throwaway save only";
      }
      // Mark empty or invalid required fields and say which are missing.
      const fields = [["transfer-target-club-id", "target club ID"], ["transfer-sign-wage", "yearly wage"], ["transfer-sign-year", "expiry year"]];
      for (const [id, label] of fields) {
        const input = document.getElementById(id);
        if (input && input.classList) input.classList.toggle("field-missing", form.missing.includes(label));
      }
      const fee = document.getElementById("transfer-fee");
      if (fee && fee.classList) fee.classList.toggle("field-missing", !form.feeValid);
      const note = document.getElementById("transfer-missing-note");
      if (note) note.innerText = ready && form.missing.length ? `Required before Move or Sign: ${form.missing.join(", ")}.` : "";
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

