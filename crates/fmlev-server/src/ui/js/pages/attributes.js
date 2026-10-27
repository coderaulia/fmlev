    // Render Attribute Tables
    function buildAttributeLists() {
      renderAttrGroup("list-technical", techAttrs);
      renderAttrGroup("list-mental", mentalAttrs);
      renderAttrGroup("list-physical", physAttrs);
      renderAttrGroup("list-hidden", hiddenAttrs);
      renderAttrGroup("list-goalkeeping", gkAttrs);
    }

    function renderAttrGroup(containerId, list) {
      const container = document.getElementById(containerId);
      if (!container) return;

      container.innerHTML = list.map(([key, name]) => `
        <div class="attr-row" data-key="${key}">
          <span class="attr-name">${name}</span>
          <div class="attr-controls">
            <button class="attr-stepper" onclick="stepAttr('${key}', -1)">−</button>
            <input id="attr-val-${key}" type="text" inputmode="numeric" class="attr-val-box tier-unexposed" value="" placeholder="—" title="Not exposed by game binding yet — open this player's Attributes screen in FM26" oninput="handleAttrInput('${key}', this.value, false)" onchange="handleAttrInput('${key}', this.value, true)" onblur="handleAttrInput('${key}', this.value, true)">
            <button class="attr-stepper" onclick="stepAttr('${key}', 1)">+</button>
          </div>
        </div>
      `).join("");
    }

    function getTierClass(val) {
      const num = parseInt(val, 10) || 0;
      if (num <= 0) return "tier-unexposed";
      if (num >= 16) return "tier-elite";
      if (num >= 11) return "tier-good";
      if (num >= 6) return "tier-avg";
      return "tier-low";
    }

    function stepAttr(key, delta) {
      if (!state.activePlayer) return;
      if (!state.activePlayer.attributes) state.activePlayer.attributes = {};
      dirtyPlayerFields.add("attr_" + key);
      markUnsaved();
      const current = parseInt(state.activePlayer.attributes[key], 10) || 0;
      let next;
      if (current <= 0) {
        next = delta > 0 ? 1 : 0;
      } else {
        next = Math.max(0, Math.min(20, current + delta));
      }
      state.activePlayer.attributes[key] = next;

      updateAttrDisplay(key, next, true);

      updateRecommendedCA();

    }

    function handleAttrInput(key, rawVal, commit) {
      if (!state.activePlayer) return;
      if (!state.activePlayer.attributes) state.activePlayer.attributes = {};
      const trimmed = String(rawVal || "").trim();
      const parsed = parseInt(trimmed, 10);
      let num = 0;
      if (trimmed !== "" && trimmed !== "—" && !isNaN(parsed) && parsed > 0) {
        num = Math.max(1, Math.min(20, parsed));
      }

      const prev = parseInt(state.activePlayer.attributes[key], 10) || 0;
      if (!commit || num !== prev || dirtyPlayerFields.has("attr_" + key)) {
        dirtyPlayerFields.add("attr_" + key);
        markUnsaved();
      }

      state.activePlayer.attributes[key] = num;
      updateAttrDisplay(key, num, commit);

      updateRecommendedCA();
    }

    function promptEditAttr(key, name) {
      const el = document.getElementById("attr-val-" + key);
      if (el) {
        el.focus();
        el.select();
      }
    }

    function updateAttrDisplay(key, val, forceValue = true) {
      const el = document.getElementById("attr-val-" + key);
      if (!el) return;
      const num = parseInt(val, 10) || 0;
      if (num <= 0) {
        if (forceValue || document.activeElement !== el) {
          el.value = "";
        }
        el.placeholder = "—";
        el.className = "attr-val-box tier-unexposed";
        el.title = "Not exposed by game binding yet — open this player's Attributes screen in FM26";
      } else {
        if (forceValue || document.activeElement !== el) {
          el.value = String(num);
        }
        el.placeholder = "—";
        el.className = `attr-val-box ${getTierClass(num)}`;
        el.title = "Type a value (1-20) or use - / + steppers";
      }
    }

    // Toggle Hidden Attributes section
    function toggleHiddenAttributes() {
      const p = document.getElementById("hidden-attributes-panel");
      const arrow = document.getElementById("hidden-toggle-arrow");
      if (p.style.display === "none") {
        p.style.display = "block";
        arrow.innerText = "▲";
      } else {
        p.style.display = "none";
        arrow.innerText = "▼";
      }
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

