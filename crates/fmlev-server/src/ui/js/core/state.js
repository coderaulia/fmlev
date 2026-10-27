    // Global Application State
    const dirtyPlayerFields = new Set();

    let state = {
      connected: false,
      playerWritableFields: null,
      pid: null,
      isPinned: false,
      autoApply: false,
      activePlayer: null,
      activeClub: null,
      activeNation: null,
      activeCompetition: null,
      liveClubs: {},
      liveNations: {},
      liveCompetitions: {},
      syncMode: "auto_follow",
      screenState: "player",
      isOverlayMode: false,
      playerDirty: false,
      playerDirtyFields: dirtyPlayerFields,
      lastContextSyncTime: null,
      clubDirty: false,
      nationDirty: false,
      competitionDirty: false,
      lastSavedTime: null,
    };

    // Pending requests are bound to entity identity and survive read-only polling.
    const pendingActions = new Map();
    const savingEntities = new Set();
    const entityControls = {
      player: { active: "activePlayer", id: "id", dirty: () => state.playerDirtyFields.size > 0,
        write: () => writePlayerChanges(), display: player => displayPlayer(player) },
      club: { active: "activeClub", id: "club_id", dirty: () => state.clubDirty,
        write: () => writeClubChanges(), display: club => displayClub(club) },
      nation: { active: "activeNation", id: "id", dirty: () => state.nationDirty,
        write: () => writeNationChanges(), display: nation => displayNation(nation) },
      competition: { active: "activeCompetition", id: "id", dirty: () => state.competitionDirty,
        write: () => writeCompetitionChanges(), display: competition => displayCompetition(competition) },
    };

    function refreshPendingActionIndicators() {
      const selectors = {
        player: '[onclick="applyPlayerChanges()"], [onclick="savePositionsToGame()"], [onclick="saveContractToGame()"]',
        club: '[onclick="saveClubToGame()"]', nation: '[onclick="saveNationToGame()"]',
        competition: '[onclick="saveCompetitionToGame()"]'
      };
      for (const [kind, selector] of Object.entries(selectors)) {
        const control = entityControls[kind];
        const entity = state[control.active];
        const actions = entity ? (pendingActions.get(`${kind}:${entity[control.id]}`) || []) : [];
        for (const button of document.querySelectorAll?.(selector) || []) {
          if (!button.dataset.saveLabel) button.dataset.saveLabel = button.textContent.trim();
          button.textContent = button.dataset.saveLabel + (actions.length ? ` (${actions.length} pending actions)` : "");
          button.title = actions.length ? `Save manual edits first, then: ${actions.map(action => action.label).join(", ")}` : "Save pending edits";
        }
      }
    }

    function queuePendingAction(kind, url, body, label) {
      const control = entityControls[kind];
      const entity = state[control.active];
      if (!entity) return false;
      const key = `${kind}:${entity[control.id]}`;
      const queue = pendingActions.get(key) || [];
      queue.push({ url, body: JSON.parse(JSON.stringify(body)), label });
      pendingActions.set(key, queue);
      refreshPendingActionIndicators();
      showToast(`${label} pending for ${entity.name || entity.club_name || kind}. Click Save on its tab. Manual edits save first, then pending actions.`, "info");
      return true;
    }

    function queuePreset(kind, preset) {
      const control = entityControls[kind];
      const entity = state[control.active];
      if (!entity) return false;
      return queuePendingAction(kind, `/api/${kind}/preset`, {
        preset, [`${kind}_id`]: entity[control.id]
      }, `Preset '${preset}'`);
    }

    async function saveEntityChanges(kind) {
      const control = entityControls[kind];
      const entity = state[control.active];
      if (!entity) return false;
      const id = entity[control.id];
      const key = `${kind}:${id}`;
      if (savingEntities.has(key)) return false;
      savingEntities.add(key);
      // Only actions present when Save was clicked are authorized by this click.
      const actions = [...(pendingActions.get(key) || [])];
      try {
        if (control.dirty() && !await control.write()) return false;
        // Edits made while the save was in flight remain pending.
        if (state[control.active]?.[control.id] === id && control.dirty()) return false;
        for (const action of actions) {
          const res = await fetch(action.url, {
            method: "POST", headers: { "Content-Type": "application/json" },
            body: JSON.stringify(action.body)
          });
          const data = await readJson(res);
          if (!res.ok) throw new Error(data.message || "Pending action failed");
          const queue = pendingActions.get(key) || [];
          const at = queue.indexOf(action);
          if (at >= 0) queue.splice(at, 1);
          if (queue.length === 0) pendingActions.delete(key);
          const acknowledged = data[kind] || data[`active_${kind}`];
          if (acknowledged && state[control.active]?.[control.id] === id && !control.dirty()) {
            control.display(acknowledged);
          }
          showToast(data.message || `${action.label} saved.`, "success");
        }
        return true;
      } catch (error) {
        showToast(`Save failed: ${error.message}. Remaining actions are pending.`, "error");
        return false;
      } finally {
        savingEntities.delete(key);
        refreshPendingActionIndicators();
      }
    }

    async function applyPlayerChanges() { return saveEntityChanges("player"); }
    async function saveClubToGame() { return saveEntityChanges("club"); }
    async function saveNationToGame() { return saveEntityChanges("nation"); }
    async function saveCompetitionToGame() { return saveEntityChanges("competition"); }

    // Offline demo IDs configured in fmlev demo context
    const OFFLINE_DEMO_PLAYER_IDS = new Set([101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 73]);

    function isLivePlayer(player) {
      if (!player || player.id === undefined || player.id === null) return false;
      if (player.is_live === true || player.source === "live") return true;
      return !OFFLINE_DEMO_PLAYER_IDS.has(Number(player.id));
    }

    function updateSyncedAgoDisplay() {
      const el = document.getElementById("player-synced-ago");
      if (!el) return;
      if (!state.lastContextSyncTime) {
        el.style.display = "none";
        return;
      }
      const secs = Math.max(0, Math.floor((Date.now() - state.lastContextSyncTime) / 1000));
      el.style.display = "inline";
      el.innerText = `Synced ${secs}s ago`;
      el.title = `Last polled context from FM26 at ${new Date(state.lastContextSyncTime).toLocaleTimeString()}`;
    }

    const techAttrs = [
      ["marking", "Marking"],
      ["long_throws", "Long Throws"],
      ["corners", "Corners"],
      ["crossing", "Crossing"],
      ["dribbling", "Dribbling"],
      ["finishing", "Finishing"],
      ["first_touch", "First Touch"],
      ["free_kicks", "Free Kicks"],
      ["heading", "Heading"],
      ["long_shots", "Long Shots"],
      ["passing", "Passing"],
      ["penalty_taking", "Penalty Taking"],
      ["tackling", "Tackling"],
      ["technique", "Technique"]
    ];

    const mentalAttrs = [
      ["aggression", "Aggression"],
      ["anticipation", "Anticipation"],
      ["bravery", "Bravery"],
      ["composure", "Composure"],
      ["concentration", "Concentration"],
      ["decisions", "Decisions"],
      ["determination", "Determination"],
      ["flair", "Flair"],
      ["leadership", "Leadership"],
      ["off_the_ball", "Off The Ball"],
      ["positioning", "Positioning"],
      ["teamwork", "Teamwork"],
      ["vision", "Vision"],
      ["work_rate", "Work Rate"]
    ];

    const physAttrs = [
      ["acceleration", "Acceleration"],
      ["agility", "Agility"],
      ["balance", "Balance"],
      ["jumping_reach", "Jumping Reach"],
      ["natural_fitness", "Natural Fitness"],
      ["pace", "Pace"],
      ["stamina", "Stamina"],
      ["strength", "Strength"]
    ];

    const hiddenAttrs = [
      ["adaptability", "Adaptability"],
      ["sportsmanship", "Sportsmanship"],
      ["versatility", "Versatility"],
      ["dirtiness", "Dirtiness"],
      ["controversy", "Controversy"],
      ["consistency", "Consistency"],
      ["important_matches", "Important Matches"],
      ["injury_proneness", "Injury Proneness"],
      ["professionalism", "Professionalism"],
      ["ambition", "Ambition"],
      ["loyalty", "Loyalty"],
      ["pressure", "Pressure"],
      ["temperament", "Temperament"]
    ];

    const gkAttrs = [
      ["punching", "Punching"],
      ["aerial_reach", "Aerial Reach"],
      ["command_of_area", "Command Of Area"],
      ["communication", "Communication"],
      ["eccentricity", "Eccentricity"],
      ["handling", "Handling"],
      ["kicking", "Kicking"],
      ["one_on_ones", "One On Ones"],
      ["reflexes", "Reflexes"],
      ["rushing_out", "Rushing Out"],
      ["throwing", "Throwing"]
    ];

    const positionDefinitions = [
      ["st", "ST", "Striker (Centre)"],
      ["aml", "AML", "Attacking Midfielder (Left)"],
      ["amc", "AMC", "Attacking Midfielder (Centre)"],
      ["amr", "AMR", "Attacking Midfielder (Right)"],
      ["ml", "ML", "Midfielder (Left)"],
      ["mc", "MC", "Midfielder (Centre)"],
      ["mr", "MR", "Midfielder (Right)"],
      ["wbl", "WBL", "Wing Back (Left)"],
      ["dm", "DM", "Defensive Midfielder (Centre)"],
      ["wbr", "WBR", "Wing Back (Right)"],
      ["dl", "DL", "Defender (Left)"],
      ["dc", "DC", "Defender (Centre)"],
      ["dr", "DR", "Defender (Right)"],
      ["gk", "GK", "Goalkeeper"],
    ];

