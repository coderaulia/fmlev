    // Transfer/contract actions need a live attached game and an open player profile.
    function updateTransferButtons() {
      const enabled = !!state.connected && !!state.activePlayer;
      for (const id of ["btn-terminate-contract", "btn-terminate-loan", "btn-move-player", "btn-sign-free-agent"]) {
        const el = document.getElementById(id);
        if (el) {
          el.disabled = !enabled;
          el.title = enabled ? "Tested in game on a throwaway save; use a backup save" : "Attach to FM26 and open the player's profile first";
        }
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

    let nativeNationSyncedId = null, nativeCompetitionSyncedId = null;
    async function syncNationNative(id) {
      if (!state.connected || !id || nativeNationSyncedId === id || state.nationDirty) return;
      nativeNationSyncedId = id;
      try {
        const res = await fetch("/api/nation/native", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ nation_id: id })
        });
        const data = await readJson(res);
        if (!res.ok) { nativeNationSyncedId = null; return; }
        if (state.nationDirty || state.activeNation?.id !== id) return;
        const n = data.nation;
        Object.assign(state.activeNation, {
          youth_rating: n.youth_rating, reputation: n.reputation, economic_factor: n.economic_factor,
          state_of_development: n.state_of_development, population: n.population
        });
        const set = (el, v) => { const e = document.getElementById(el); if (e) e.value = v; };
        set("nation-youth-slider", n.youth_rating);
        const yv = document.getElementById("nation-youth-val"); if (yv) yv.innerText = n.youth_rating;
        updateYouthRatingDisplay(n.youth_rating);
        set("nation-reputation", n.reputation);
        setNationStatUI("nation-economic", "disp-nation-econ", n.economic_factor);
        set("nation-development", n.state_of_development);
        set("nation-population", n.population);
      } catch (e) { nativeNationSyncedId = null; }
    }
    async function syncCompetitionNative(id) {
      if (!state.connected || !id || nativeCompetitionSyncedId === id || state.competitionDirty) return;
      nativeCompetitionSyncedId = id;
      try {
        const res = await fetch("/api/competition/native", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ competition_id: id })
        });
        const data = await readJson(res);
        if (!res.ok) { nativeCompetitionSyncedId = null; return; }
        if (state.competitionDirty || state.activeCompetition?.id !== id) return;
        state.activeCompetition.reputation = data.competition.reputation;
        const el = document.getElementById("comp-reputation");
        if (el) el.value = data.competition.reputation;
      } catch (e) { nativeCompetitionSyncedId = null; }
    }

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

