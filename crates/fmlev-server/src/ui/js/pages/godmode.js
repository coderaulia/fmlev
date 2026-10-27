    // God Mode Actions
    async function applyGodModeAll() {
      showToast("Preparing pending presets across all entities...", "info");
      const actions = [
        () => applyPlayerPreset("goat"), applyBothFeetGodMode,
        () => applyClubPreset("heal_squad"), () => applyClubPreset("morale_squad"),
        () => applyClubPreset("sugar_daddy"), () => applyClubPreset("max_facilities"),
        applyInvincibleBoard, () => applyNationPreset("golden_generation"),
        () => applyCompetitionPreset("relax_rules")
      ];
      for (const action of actions) {
        if (!await action()) return false;
      }
      showToast("Presets staged. Click Save on each affected tab to apply them.", "info");
      return true;
    }

    async function applyBothFeetGodMode() {
      if (!state.activePlayer) return;
      if (!state.activePlayer.positions) state.activePlayer.positions = {};
      state.activePlayer.positions.left_foot = 20;
      state.activePlayer.positions.right_foot = 20;
      renderPositionsTab();
      state.playerDirtyFields.add("positions");
      markUnsaved();
      return true;
    }

    async function applyInvincibleBoard() {
      if (!state.activeClub) return;
      state.activeClub.board_patience = 20;
      const bp = document.getElementById("club-board-patience");
      if (bp) {
        bp.value = 20;
        updateDisp("disp-patience", 20);
      }
      const fc = document.getElementById("club-fan-confidence");
      if (fc) fc.value = 100;
      const bc = document.getElementById("club-board-confidence");
      if (bc) bc.value = 100;
      const tr = document.getElementById("club-transfer-retention");
      if (tr) tr.value = 100;
      updateDispPercent("disp-fan-conf", 100);
      updateDispPercent("disp-board-conf", 100);
      updateDispPercent("disp-retention", 100);
      state.clubDirty = true;
      showToast("Board changes pending. Click Save on the Club tab.", "info");
      return true;
    }

    async function applyMassivePrizeMoney() {
      if (!state.activeCompetition) return;
      state.activeCompetition.winner_prize_money = 500000000;
      state.activeCompetition.tv_rights_pool = 3000000000;
      const pm = document.getElementById("comp-prize-money");
      if (pm) setMoneyInput(pm, 500000000);
      const tv = document.getElementById("comp-tv-rights");
      if (tv) setMoneyInput(tv, 3000000000);
      state.competitionDirty = true;
      showToast("Prize changes pending. Click Save on the Competition tab.", "info");
      return true;
    }

    async function applyCleanNationGovernance() {
      if (!state.activeNation) return;
      state.activeNation.corruption_index = 0;
      state.activeNation.game_importance = 20;
      const nc = document.getElementById("nation-corruption");
      if (nc) nc.value = 0;
      const ng = document.getElementById("nation-game-importance");
      if (ng) ng.value = 20;
      updateDisp("disp-nation-corruption", 0);
      updateDisp("disp-nation-importance", 20);
      state.nationDirty = true;
      showToast("Nation changes pending. Click Save on the Nation tab.", "info");
      return true;
    }

    // Apply Player Preset
    async function applyPlayerPreset(preset) {
      return queuePreset("player", preset);
    }

