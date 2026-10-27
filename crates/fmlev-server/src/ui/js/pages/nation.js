    // Nation Details Management
    function displayNation(nation) {
      state.activeNation = JSON.parse(JSON.stringify(nation));
      if (displayEntityAvailability("nation", nation)) return;

      document.getElementById("nation-name").innerText = nation.name || "Nation";
      document.getElementById("nation-code").innerText = nation.three_letter_code || "NAT";
      document.getElementById("nation-continent").innerText = nation.continent || "Continent";
      document.getElementById("nation-flag-avatar").innerText = getCountryFlag(nation.name);

      document.getElementById("nation-rep-num").innerText = `${(nation.reputation || 7500).toLocaleString()} / 10,000`;
      document.getElementById("nation-rep-stars").innerText = getReputationStars(nation.reputation || 7500);

      // Youth Rating Slider & Gauge
      const yr = nation.youth_rating ?? 120;
      document.getElementById("nation-youth-val").innerText = yr;
      document.getElementById("nation-youth-slider").value = yr;
      updateYouthRatingDisplay(yr);

      // Macro & Governance
      document.getElementById("nation-reputation").value = nation.reputation ?? 7500;
      document.getElementById("nation-development").value = nation.state_of_development ?? 2;

      setNationStatUI("nation-economic", "disp-nation-econ", nation.economic_factor ?? 15);
      setNationStatUI("nation-fa-power", "disp-nation-fa", nation.fa_financial_power ?? 15);

      const tax = nation.tax_rate ?? 25;
      document.getElementById("nation-tax-rate").value = tax;
      document.getElementById("disp-nation-tax").innerText = `${tax}%`;

      if (document.getElementById("nation-population")) {
        document.getElementById("nation-population").value = nation.population ?? 50000000;
      }

      setNationStatUI("nation-corruption", "disp-nation-corruption", nation.corruption_index ?? 2);
      setNationStatUI("nation-game-importance", "disp-nation-importance", nation.game_importance ?? 18);

      // Infrastructure Averages
      setNationStatUI("nation-fac-avg", "disp-nation-fac-avg", nation.youth_facilities_avg ?? 15);
      setNationStatUI("nation-coaching-avg", "disp-nation-coaching-avg", nation.youth_coaching_avg ?? 15);
      setNationStatUI("nation-rec-avg", "disp-nation-rec-avg", nation.youth_recruitment_avg ?? 15);

      // Update God Mode Target Badge
      const godNationTarget = document.getElementById("god-nation-target");
      if (godNationTarget) godNationTarget.innerText = `Target: ${nation.name || "Active Nation"}`;
      refreshPendingActionIndicators();
    }

    function setNationStatUI(sliderId, dispId, val) {
      const s = document.getElementById(sliderId);
      const d = document.getElementById(dispId);
      if (s) s.value = val;
      if (d) {
        d.innerText = val;
        d.className = `facility-val ${getTierClass(val)}`;
      }
    }

    function handleYouthRatingInput(val) {
      state.nationDirty = true;
      const yr = Math.max(1, Math.min(200, parseInt(val) || 1));
      document.getElementById("nation-youth-val").innerText = yr;
      updateYouthRatingDisplay(yr);
      if (state.activeNation) state.activeNation.youth_rating = yr;
    }

    function updateYouthRatingDisplay(yr) {
      const pct = (yr / 200) * 100;
      const fill = document.getElementById("nation-youth-gauge-fill");
      if (fill) fill.style.width = `${pct}%`;

      const badge = document.getElementById("nation-youth-tier");
      if (!badge) return;

      if (yr >= 176) {
        badge.className = "rating-tier-badge tier-wonderkid";
        badge.innerText = "Tier 1: Wonderkid Hotbed / World Dominance 🌟";
      } else if (yr >= 146) {
        badge.className = "rating-tier-badge tier-elite";
        badge.innerText = "Tier 2: Elite Football Power ⚡";
      } else if (yr >= 111) {
        badge.className = "rating-tier-badge tier-good";
        badge.innerText = "Tier 3: Established Talent Pool 🏆";
      } else if (yr >= 61) {
        badge.className = "rating-tier-badge tier-avg";
        badge.innerText = "Tier 4: Developing / Modest ⚽";
      } else {
        badge.className = "rating-tier-badge tier-low";
        badge.innerText = "Tier 5: Obscure / Grassroots 🌱";
      }
    }

    function stepNationStat(key, delta) {
      state.nationDirty = true;
      if (!state.activeNation) return;
      const current = state.activeNation[key] ?? 10;
      const maxVal = key === "youth_rating" ? 200 : (key === "tax_rate" ? 100 : 20);
      const minVal = key === "corruption_index" || key === "tax_rate" ? 0 : 1;
      const next = Math.max(minVal, Math.min(maxVal, current + delta));
      state.activeNation[key] = next;

      if (key === "youth_rating") {
        document.getElementById("nation-youth-slider").value = next;
        handleYouthRatingInput(next);
      } else if (key === "tax_rate") {
        document.getElementById("nation-tax-rate").value = next;
        document.getElementById("disp-nation-tax").innerText = `${next}%`;
      } else {
        const idMap = {
          economic_factor: ["nation-economic", "disp-nation-econ"],
          fa_financial_power: ["nation-fa-power", "disp-nation-fa"],
          corruption_index: ["nation-corruption", "disp-nation-corruption"],
          game_importance: ["nation-game-importance", "disp-nation-importance"],
          youth_facilities_avg: ["nation-fac-avg", "disp-nation-fac-avg"],
          youth_coaching_avg: ["nation-coaching-avg", "disp-nation-coaching-avg"],
          youth_recruitment_avg: ["nation-rec-avg", "disp-nation-rec-avg"]
        };
        const mapping = idMap[key];
        if (mapping) {
          setNationStatUI(mapping[0], mapping[1], next);
        }
      }
    }

    function handleNationRepChange(val) {
      state.nationDirty = true;
      const parsedRep = parseInt(val);
      if (!Number.isFinite(parsedRep)) return;
      const num = Math.max(1, Math.min(10000, parsedRep));
      document.getElementById("nation-rep-num").innerText = `${num.toLocaleString()} / 10,000`;
      document.getElementById("nation-rep-stars").innerText = getReputationStars(num);
      if (state.activeNation) state.activeNation.reputation = num;
    }

    function debouncedSaveNation() {
      state.nationDirty = true;
    }

    async function writeNationChanges(isQuiet = false) {
      if (!state.activeNation) return false;
      state.nationDirty = true;

      if (state.connected) {
        state.activeNation.reputation = numberInput(document.getElementById("nation-reputation")?.value, state.activeNation.reputation || 7500);
        state.activeNation.youth_rating = numberInput(document.getElementById("nation-youth-slider")?.value, state.activeNation.youth_rating || 120);
        state.activeNation.state_of_development = numberInput(document.getElementById("nation-development")?.value, state.activeNation.state_of_development ?? 2);
        state.activeNation.economic_factor = numberInput(document.getElementById("nation-economic")?.value, state.activeNation.economic_factor || 15);
        state.activeNation.population = numberInput(document.getElementById("nation-population")?.value, state.activeNation.population || 50000000);
      } else {
        state.activeNation.reputation = numberInput(document.getElementById("nation-reputation")?.value, 7500);
        state.activeNation.youth_rating = numberInput(document.getElementById("nation-youth-slider")?.value, 120);
        state.activeNation.state_of_development = numberInput(document.getElementById("nation-development")?.value, 2);
        state.activeNation.economic_factor = numberInput(document.getElementById("nation-economic")?.value, 15);
        state.activeNation.population = numberInput(document.getElementById("nation-population")?.value, 50000000);
        state.activeNation.fa_financial_power = numberInput(document.getElementById("nation-fa-power")?.value, 15);
        state.activeNation.tax_rate = numberInput(document.getElementById("nation-tax-rate")?.value, 25);
        state.activeNation.corruption_index = numberInput(document.getElementById("nation-corruption")?.value, 2);
        state.activeNation.game_importance = numberInput(document.getElementById("nation-game-importance")?.value, 18);
        state.activeNation.youth_facilities_avg = numberInput(document.getElementById("nation-fac-avg")?.value, 15);
        state.activeNation.youth_coaching_avg = numberInput(document.getElementById("nation-coaching-avg")?.value, 15);
        state.activeNation.youth_recruitment_avg = numberInput((document.getElementById("nation-recruitment-avg") || document.getElementById("nation-rec-avg"))?.value, 15);
      }

      const submittedEntity = JSON.stringify(state.activeNation);
      try {
        const res = await fetch("/api/nation/update", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: submittedEntity
        });
        const data = await readJson(res);
        if (res.ok) {
          if (JSON.stringify(state.activeNation) === submittedEntity) {
            state.nationDirty = false;
            nativeNationSyncedId = null;
          }
          if (!isQuiet) showToast(`Saved ${state.activeNation.name}: youth rating, reputation, economic factor, development and population are written natively; governance fields are not live-writable.`, "success");
          return true;
        } else {
          showToast(data.message || "Failed to save nation", "error");
        }
      } catch (e) {
        showToast("Nation save error: " + e.message, "error");
      }
      return false;
    }

    async function applyNationPreset(preset) {
      return queuePreset("nation", preset);
    }

