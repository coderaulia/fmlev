    // Transfer/contract actions need a live attached game and an open player profile.
    function updateTransferButtons() {
      for (const id of ["btn-terminate-contract", "btn-terminate-loan", "btn-move-player", "btn-sign-free-agent"]) {
        const el = document.getElementById(id);
        if (el) { el.disabled = true; el.title = "Unavailable: day-tick and save/reload evidence is incomplete"; }
      }
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

