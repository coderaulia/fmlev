    // Transfer/contract actions need a live attached game, an open player and the explicit experimental opt-in.
    // Loan recall has no in-game test yet, so it stays disabled.
    function transferExperimentalOn() { return !!document.getElementById("transfer-experimental")?.checked; }
    function updateTransferButtons() {
      const ready = !!state.connected && !!state.activePlayer && transferExperimentalOn();
      const why = !state.connected ? "Attach to a game first" : !state.activePlayer ? "Open a player first"
        : "Tick the experimental checkbox first";
      for (const id of ["btn-terminate-contract", "btn-move-player", "btn-sign-free-agent"]) {
        const el = document.getElementById(id);
        if (el) { el.disabled = !ready; el.title = ready ? "Experimental: throwaway save only" : `Unavailable: ${why}`; }
      }
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

