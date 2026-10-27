    // Competition Details Management
    function handleCompRepChange(val) {
      state.competitionDirty = true;
      const num = Math.max(1, Math.min(10000, parseInt(val) || 5000));
      const repNum = document.getElementById("comp-rep-num");
      if (repNum) repNum.innerText = `${num.toLocaleString()} / 10,000`;
      const repStars = document.getElementById("comp-rep-stars");
      if (repStars) repStars.innerText = getReputationStars(num);
      if (state.activeCompetition) state.activeCompetition.reputation = num;
    }

    function displayCompetition(comp) {
      state.activeCompetition = JSON.parse(JSON.stringify(comp));
      if (displayEntityAvailability("competition", comp)) return;

      document.getElementById("comp-name").innerText = comp.name || "Competition";
      document.getElementById("comp-short-name").innerText = comp.short_name || "COMP";
      document.getElementById("comp-nation").innerText = comp.nation_name || "International";

      document.getElementById("comp-rep-num").innerText = `${(comp.reputation || 8000).toLocaleString()} / 10,000`;
      document.getElementById("comp-rep-stars").innerText = getReputationStars(comp.reputation || 8000);
      const compRep = document.getElementById("comp-reputation");
      if (compRep) compRep.value = comp.reputation || 8000;

      // Financial prizes
      setMoneyInput(document.getElementById("comp-prize-money"), comp.winner_prize_money ?? 40000000);
      setMoneyInput(document.getElementById("comp-tv-rights"), comp.tv_rights_pool ?? 2000000000);

      // Match rules
      document.getElementById("comp-max-subs").value = comp.max_substitutes ?? 5;
      document.getElementById("comp-bench-size").value = comp.max_bench_size ?? 9;
      document.getElementById("comp-sub-windows").value = comp.substitution_windows ?? 3;
      document.getElementById("comp-yellow-ban").value = comp.yellow_cards_for_ban ?? 5;

      const varCheck = document.getElementById("comp-var-active");
      if (varCheck) {
        varCheck.checked = comp.var_active !== false;
        handleVarToggle(varCheck.checked);
      }

      // Squad rules
      document.getElementById("comp-squad-size").value = comp.max_squad_size ?? 25;
      document.getElementById("comp-foreign-limit").value = comp.max_foreign_players ?? 17;
      document.getElementById("comp-hg-club").value = comp.min_homegrown_club ?? 4;
      document.getElementById("comp-hg-nation").value = comp.min_homegrown_nation ?? 8;
      refreshPendingActionIndicators();
    }

    function stepCompField(id, delta, min, max) {
      state.competitionDirty = true;
      const input = document.getElementById(id);
      if (!input) return;
      const current = parseInt(input.value) || 0;
      const next = Math.max(min, Math.min(max, current + delta));
      input.value = next;
    }

    function handleVarToggle(active) {
      const badge = document.getElementById("comp-var-status");
      if (!badge) return;
      if (active) {
        badge.className = "pill-badge badge-fit";
        badge.innerText = "🟢 VAR Active";
      } else {
        badge.className = "pill-badge badge-injured";
        badge.innerText = "⚪ VAR Disabled";
      }
    }

    async function writeCompetitionChanges() {
      if (!state.activeCompetition) return false;
      state.competitionDirty = true;

      const compRepInput = document.getElementById("comp-reputation");
      if (compRepInput) {
        state.activeCompetition.reputation = numberInput(compRepInput.value, state.activeCompetition.reputation || 8000);
      }

      if (!state.connected) {
        state.activeCompetition.winner_prize_money = readMoneyInput(document.getElementById("comp-prize-money"), 0);
        state.activeCompetition.tv_rights_pool = readMoneyInput(document.getElementById("comp-tv-rights"), 0);

        state.activeCompetition.max_substitutes = numberInput(document.getElementById("comp-max-subs")?.value, 5);
        state.activeCompetition.max_bench_size = numberInput(document.getElementById("comp-bench-size")?.value, 9);
        state.activeCompetition.substitution_windows = numberInput(document.getElementById("comp-sub-windows")?.value, 3);
        state.activeCompetition.yellow_cards_for_ban = numberInput(document.getElementById("comp-yellow-ban")?.value, 5);
        const varActive = document.getElementById("comp-var-active");
        if (varActive) {
          state.activeCompetition.var_active = varActive.checked;
        }

        state.activeCompetition.max_squad_size = numberInput(document.getElementById("comp-squad-size")?.value, 25);
        state.activeCompetition.max_foreign_players = numberInput(document.getElementById("comp-foreign-limit")?.value, 17);
        state.activeCompetition.min_homegrown_club = numberInput(document.getElementById("comp-hg-club")?.value, 0);
        state.activeCompetition.min_homegrown_nation = numberInput(document.getElementById("comp-hg-nation")?.value, 0);
      }

      showToast("Saving competition...", "info");
      const submittedEntity = JSON.stringify(state.activeCompetition);
      try {
        const res = await fetch("/api/competition/update", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: submittedEntity
        });
        const data = await readJson(res);
        if (res.ok) {
          if (JSON.stringify(state.activeCompetition) === submittedEntity) {
            state.competitionDirty = false;
            nativeCompetitionSyncedId = null;
          }
          showToast(`Saved ${state.activeCompetition.name} native competition values (reputation). Rules and prize money are saved separately.`, "success");
          return true;
        } else {
          showToast(data.message || "Failed to save competition rules", "error");
        }
      } catch (e) {
        showToast("Competition save error: " + e.message, "error");
      }
      return false;
    }

    async function applyCompetitionPreset(preset) {
      return queuePreset("competition", preset);
    }

