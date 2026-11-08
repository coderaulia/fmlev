    // ---- Homegrown ("Trained at club / nation") for the open player ----
    // Reading is safe. Writing is EXPERIMENTAL: when the player's relationship vector is full the server allocates
    // a new array inside the game process, so the buttons stay behind an unchecked checkbox and a confirm dialog.
    const homegrownState = { loadedFor: null, data: null, busy: false };
    function homegrownEditEnabled() { return !!document.getElementById("homegrown-experimental")?.checked; }
    function homegrownTargetText(list, empty) {
      if (!Array.isArray(list) || !list.length) return empty;
      return list.map(t => `${t.name || "Unnamed"}${t.id ? ` (#${t.id})` : ""}`).join(", ");
    }
    function renderHomegrown() {
      const data = homegrownState.data;
      const set = (id, text) => { const el = document.getElementById(id); if (el) el.innerText = text; };
      if (!data) { set("homegrown-clubs", "Unavailable"); set("homegrown-nations", "Unavailable"); }
      else {
        set("homegrown-clubs", homegrownTargetText(data.clubs, "None recorded"));
        set("homegrown-nations", homegrownTargetText(data.nations, "None recorded"));
        const extra = data.unresolved_targets ? ` ${data.unresolved_targets} record(s) point at an unrecognised object and are not shown.` : "";
        set("homegrown-note", `Current club: ${data.current_club?.name || "unavailable"}. Club nation: ${data.club_nation?.name || "unavailable"}. Free relationship slots: ${data.spare_slots}.${extra}`);
      }
      updateHomegrownButtons();
    }
    function updateHomegrownButtons() {
      const editingAvailable = homegrownState.data?.editing_available === true;
      const checkbox = document.getElementById("homegrown-experimental");
      if (checkbox) checkbox.disabled = !editingAvailable;
      const ready = editingAvailable && !!state.connected && !!state.activePlayer?.id && homegrownEditEnabled() && !homegrownState.busy;
      const why = !editingAvailable ? "Homegrown edits are unverified in FM26" : !state.connected ? "Attach to a game first" : !state.activePlayer?.id ? "Open a player first" : !homegrownEditEnabled() ? "Tick the experimental checkbox first" : "Busy";
      const data = homegrownState.data;
      for (const [id, key, label] of [["btn-homegrown-club", "current_club", "club"], ["btn-homegrown-nation", "club_nation", "club nation"]]) {
        const el = document.getElementById(id);
        if (!el) continue;
        const resolvable = key === "club_nation" ? data?.[key]?.id > 0 : !!data?.[key];
        el.disabled = !ready || !resolvable;
        el.title = !ready ? `Unavailable: ${why}` : !resolvable ? `Unavailable: the player's ${label} could not be resolved` : "Asks for confirmation";
      }
    }
    async function refreshHomegrown() {
      const id = state.activePlayer?.id;
      if (!state.connected || !id) { homegrownState.data = null; homegrownState.loadedFor = null; renderHomegrown(); return null; }
      homegrownState.loadedFor = id; // one attempt per player: a failed read is not retried on every poll
      homegrownState.data = null;
      renderHomegrown();
      try {
        const res = await fetch("/api/player/homegrown", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ player_id: id }) });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Homegrown read failed");
        if (!state.connected || state.activePlayer?.id !== id) return null;
        homegrownState.data = data;
      } catch (error) {
        if (!state.connected || state.activePlayer?.id !== id) return null;
      }
      renderHomegrown();
      return homegrownState.data;
    }
    async function setHomegrown(which) {
      const player = state.activePlayer;
      if (!state.connected || !player?.id) { showToast("Attach to a game and open a player first", "warning"); return false; }
      if (homegrownState.data?.editing_available !== true) { showToast("Homegrown editing is unavailable", "warning"); return false; }
      if (!homegrownEditEnabled()) { showToast("Tick the experimental checkbox first", "warning"); return false; }
      const club = which === "club";
      const target = club ? homegrownState.data?.current_club : homegrownState.data?.club_nation;
      if (!target || (!club && !(target.id > 0))) { showToast(`The player's ${club ? "current club" : "club nation"} could not be resolved`, "warning"); return false; }
      const text = `Mark ${player.name || "this player"} as trained at ${club ? "club" : "nation"} ${target.name}? ` +
        "If the player's relationship list is full this ALLOCATES MEMORY inside the game and swaps the list pointers; the game may crash later. Throwaway save only, keep a backup.";
      if (typeof confirm === "function" && !confirm(text)) return false;
      homegrownState.busy = true; updateHomegrownButtons();
      try {
        const body = { player_id: player.id, experimental: true };
        if (club) body.club_current = true; else body.nation_id = target.id;
        const res = await fetch("/api/player/homegrown/set", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Homegrown edit failed");
        if (state.activePlayer?.id === player.id && state.connected) {
          homegrownState.data = data;
          homegrownState.loadedFor = player.id;
        }
        showToast(data.message || "Homegrown status written.", "success");
        return true;
      } catch (error) {
        showToast(`Homegrown edit failed: ${error.message}`, "error");
        return false;
      } finally { homegrownState.busy = false; renderHomegrown(); }
    }
    function trainedAtClub() { return setHomegrown("club"); }
    function trainedAtNation() { return setHomegrown("nation"); }
