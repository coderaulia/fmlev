    // ---- Player attributes: FM-style three-column table with value bands and click-to-edit steppers ----
    // Goalkeepers see Goalkeeping in place of Technical; the other group moves into the collapsible block.
    let attrLayoutGk = null;
    const ATTR_UNREAD_TITLE = "Not read from the game yet. Open this player's Attributes screen in FM26.";

    function playerIsGoalkeeper(player = state.activePlayer) {
      if (!player) return false;
      if (player.positions && Number(player.positions.gk) >= 15) return true;
      return /^GK\b/i.test(String(player.position || ""));
    }

    // Sub-tabs of the Player section (Profile, Positions & Roles, Contract & Transfer, Registration).
    function setupPlayerSubTabs() {
      if (typeof Shell === "undefined" || !Shell.setSubTabs) return;
      Shell.setSubTabs([
        { id: "tab-player", label: "Profile" },
        { id: "tab-positions", label: "Positions & Roles" },
        { id: "tab-contract", label: "Contract & Transfer" },
        { id: "tab-registration", label: "Registration" },
      ], "tab-player", { section: "player" });
    }

    function buildAttributeLists(gk = playerIsGoalkeeper()) {
      setupPlayerSubTabs();
      attrLayoutGk = gk;
      const text = (id, value) => { const el = document.getElementById(id); if (el) el.textContent = value; };
      text("attr-head-main", gk ? "Goalkeeping" : "Technical");
      text("attr-alt-title", gk ? "Technical" : "Goalkeeping");
      text("attr-alt-head", gk ? "Technical" : "Goalkeeping (outfield players do not use these)");
      renderAttrGroup("list-technical", gk ? gkAttrs : techAttrs);
      renderAttrGroup("list-mental", mentalAttrs);
      renderAttrGroup("list-physical", physAttrs);
      renderAttrGroup("list-hidden", hiddenAttrs);
      renderAttrGroup("list-alt", gk ? techAttrs : gkAttrs);
      applyRoleHighlight();
    }

    function renderAttrGroup(containerId, list) {
      const container = document.getElementById(containerId);
      if (!container) return;
      container.innerHTML = list.map(([key, name]) => `
        <div class="attr-line unavailable" id="attr-line-${key}" data-key="${key}">
          <span class="attr-line-name">${name}</span>
          <span class="attr-edit">
            <button type="button" class="attr-step" onclick="stepAttr('${key}', -1)" aria-label="Decrease ${name}">&minus;</button>
            <input id="attr-val-${key}" type="text" inputmode="numeric" class="attr-line-val band-none" value="" placeholder="—" title="${ATTR_UNREAD_TITLE}" aria-label="${name}" disabled
                   onfocus="beginAttrEdit('${key}')" onkeydown="handleAttrKey(event, '${key}')" oninput="handleAttrInput('${key}', this.value, false)" onchange="handleAttrInput('${key}', this.value, true)" onblur="handleAttrInput('${key}', this.value, true)">
            <button type="button" class="attr-step" onclick="stepAttr('${key}', 1)" aria-label="Increase ${name}">+</button>
          </span>
        </div>`).join("");
    }

    // Repaint every attribute from the open player (used after the layout changes).
    function refreshAllAttrDisplays() {
      const attrs = state.activePlayer?.attributes || {};
      for (const key of allAttrKeys) updateAttrDisplay(key, attrs[key], true);
    }

    function getTierClass(val) {
      const num = parseInt(val, 10) || 0;
      if (num <= 0) return "tier-unexposed";
      if (num >= 16) return "tier-elite";
      if (num >= 11) return "tier-good";
      if (num >= 6) return "tier-avg";
      return "tier-low";
    }

    // Whether the live server has a verified write path for this attribute (null = unknown/offline, allow).
    function attrWritable(key) {
      const fields = state.playerWritableFields;
      return !state.connected || Array.isArray(fields) && fields.includes(`attributes.${key}`);
    }

    function baselineAttr(key) {
      const base = state.caEstimateBaseline;
      if (!base || !state.activePlayer || base.id !== state.activePlayer.id) return undefined;
      return base.attributes ? base.attributes[key] : undefined;
    }

    // Track an attribute edit: dirty only while it differs from the last value read from the game.
    function trackAttrEdit(key, value) {
      const base = baselineAttr(key);
      if (base !== undefined && base > 0 && value === base) dirtyPlayerFields.delete("attr_" + key);
      else dirtyPlayerFields.add("attr_" + key);
      markUnsaved();
    }

    function stepAttr(key, delta) {
      if (!state.activePlayer) return;
      if (!state.activePlayer.attributes) state.activePlayer.attributes = {};
      const current = parseInt(state.activePlayer.attributes[key], 10) || 0;
      if (current <= 0) {
        showToast("This attribute has not been read from the game, so it cannot be edited.", "warning");
        return;
      }
      if (!attrWritable(key)) {
        showToast("This attribute has no verified write path.", "warning");
        return;
      }
      const next = Math.max(1, Math.min(20, current + delta));
      state.activePlayer.attributes[key] = next;
      trackAttrEdit(key, next);
      updateAttrDisplay(key, next, true);
      updateRecommendedCA();
    }

    function handleAttrInput(key, rawVal, commit) {
      if (!state.activePlayer) return;
      if (!state.activePlayer.attributes) state.activePlayer.attributes = {};
      const trimmed = String(rawVal ?? "").trim();
      const parsed = parseInt(trimmed, 10);
      const prev = parseInt(state.activePlayer.attributes[key], 10) || 0;
      if (prev <= 0 || !attrWritable(key)) return;
      // Typing a partial or invalid value never changes the player; a blank commit restores the shown value.
      if (trimmed === "" || trimmed === "—" || isNaN(parsed) || parsed < 1) {
        if (commit) updateAttrDisplay(key, prev, true);
        return;
      }
      const num = Math.max(1, Math.min(20, parsed));
      state.activePlayer.attributes[key] = num;
      if (num !== prev || dirtyPlayerFields.has("attr_" + key)) trackAttrEdit(key, num);
      updateAttrDisplay(key, num, commit);
      updateRecommendedCA();
    }

    const attrEditStarts = new Map();
    function beginAttrEdit(key) {
      attrEditStarts.set(key, { id: state.activePlayer?.id, value: state.activePlayer?.attributes?.[key], dirty: dirtyPlayerFields.has("attr_" + key) });
    }
    function handleAttrKey(event, key) {
      const input = document.getElementById("attr-val-" + key);
      if (event.key === "Escape") {
        const start = attrEditStarts.get(key);
        if (!start || start.id !== state.activePlayer?.id) return;
        event.preventDefault(); event.stopPropagation?.();
        state.activePlayer.attributes[key] = start.value;
        if (start.dirty) dirtyPlayerFields.add("attr_" + key); else dirtyPlayerFields.delete("attr_" + key);
        updateAttrDisplay(key, start.value, true); markUnsaved(); updateRecommendedCA(); input?.blur?.();
      } else if (event.key === "Enter") {
        event.preventDefault(); handleAttrInput(key, input?.value, true); input?.blur?.();
      } else if (event.key === "ArrowUp" || event.key === "ArrowDown") {
        const inputs = Array.from(document.querySelectorAll?.(".attr-line-val") || [])
          .filter(node => !node.disabled && (!node.getClientRects || node.getClientRects().length));
        const index = inputs.indexOf(input), next = index + (event.key === "ArrowDown" ? 1 : -1);
        if (index >= 0 && inputs[next]) { event.preventDefault(); inputs[next].focus(); inputs[next].select?.(); }
      }
    }

    function updateAttrDisplay(key, val, forceValue = true) {
      const el = document.getElementById("attr-val-" + key);
      if (!el) return;
      const num = parseInt(val, 10) || 0;
      const line = document.getElementById("attr-line-" + key);
      const writable = num > 0 && attrWritable(key);
      if (num <= 0) {
        if (forceValue || document.activeElement !== el) el.value = "";
        el.title = ATTR_UNREAD_TITLE;
      } else {
        if (forceValue || document.activeElement !== el) el.value = String(num);
        el.title = writable ? "Type a value (1-20) or use the - / + buttons. Written when you click Save." : "This attribute has no verified write path.";
      }
      el.className = `attr-line-val ${valueBand(num)}`;
      el.disabled = !writable;
      if (line && line.classList) {
        line.classList.toggle("unavailable", num <= 0);
        line.classList.toggle("readonly", num > 0 && !writable);
      }
    }

    // Highlight the key and secondary attributes of the role chosen under Positions & Roles.
    function applyRoleHighlight() {
      const role = typeof selectedRole === "function" ? selectedRole() : null;
      const key = new Set(role ? role.key : []);
      const secondary = new Set(role ? role.secondary : []);
      for (const attr of allAttrKeys) {
        const line = document.getElementById("attr-line-" + attr);
        if (!line || !line.classList) continue;
        line.classList.toggle("key-attr", key.has(attr));
        line.classList.toggle("sec-attr", secondary.has(attr) && !key.has(attr));
      }
      const note = document.getElementById("role-highlight-note");
      if (note) note.textContent = role
        ? `${role.name}: ${role.key.length} key attributes highlighted, ${role.secondary.length} secondary${roleState.dataVerified ? "" : " (role data unverified)"}.`
        : "Choose a role under Positions & Roles to highlight its key attributes.";
      const clear = document.getElementById("role-highlight-clear");
      if (clear) clear.style.display = role ? "" : "none";
    }

    async function discoverLoadedPlayers() {
      const button = document.getElementById("discover-players-btn");
      if (button) { button.disabled = true; button.innerText = "Scanning players…"; }
      try {
        const res = await fetch("/api/players/discover", { method: "POST" });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Player scan failed");
        showToast(data.message || "Done", "success");
      } catch (error) { showToast(error.message, "error"); }
      finally { if (button) { button.disabled = false; button.innerText = "Scan Loaded Players"; } }
    }
