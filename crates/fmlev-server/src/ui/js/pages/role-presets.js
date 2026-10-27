    // ---- Role presets (proposal from the server; staged into the normal player Save) ----
    const roleState = { roles: [], proposal: null };
    async function loadRoles() {
      try {
        const res = await fetch("/api/roles");
        const data = await res.json();
        if (!res.ok) return;
        roleState.roles = data.roles;
        const badge = document.getElementById("role-data-badge");
        if (badge) badge.innerText = data.role_data_verified ? "Role data: verified" : "Role data: unverified";
        const select = document.getElementById("role-select");
        if (select) {
          const groups = [...new Set(data.roles.map(r => r.group))];
          select.innerHTML = groups.map(group =>
            `<optgroup label="${escapeHtml(group)}">${data.roles.filter(r => r.group === group)
              .map(r => `<option value="${escapeHtml(r.id)}">${escapeHtml(r.name)}</option>`).join("")}</optgroup>`).join("");
        }
      } catch (error) { /* roles stay unavailable until the server answers */ }
    }
    function clearRoleProposal() {
      roleState.proposal = null;
      const diff = document.getElementById("role-diff");
      if (diff) diff.innerHTML = "";
      const btn = document.getElementById("role-stage-btn");
      if (btn) btn.style.display = "none";
    }
    function roleFieldLabel(field) {
      return field.replace("attributes.", "").replace("positions.", "pos ").replace(/_/g, " ");
    }
    async function previewRolePreset(level) {
      if (!state.activePlayer || !state.activePlayer.id) { showToast("Open a player first.", "warning"); return; }
      const roleId = document.getElementById("role-select")?.value;
      if (!roleId) { showToast("Role list is not loaded.", "warning"); return; }
      const caMode = document.getElementById("role-ca-mode")?.value || undefined;
      try {
        const res = await fetch("/api/player/role-preset", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ player_id: state.activePlayer.id, role_id: roleId, level, ca_mode: caMode })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Role preset request failed");
        roleState.proposal = { player_id: data.player_id, ...data.proposal };
        const p = roleState.proposal;
        const diff = document.getElementById("role-diff");
        const rows = p.changes.map(c => `<div>${escapeHtml(roleFieldLabel(c.field))}: ${c.old} → <b>${c.new}</b>${c.field.endsWith("_ability") ? " (estimate)" : ""}</div>`).join("");
        diff.innerHTML = `<div style="font-weight:700;margin-bottom:4px;">${escapeHtml(p.role_name)} · Level ${p.level} · CA ${escapeHtml(p.ca_mode.replace("_", " "))}</div>` +
          (rows || "<div>No changes needed: the player already meets this level.</div>");
        const btn = document.getElementById("role-stage-btn");
        if (btn) btn.style.display = p.changes.length ? "" : "none";
      } catch (error) { clearRoleProposal(); showToast(error.message, "error"); }
    }
    function stageRoleProposal() {
      const p = roleState.proposal;
      if (!p || !state.activePlayer || state.activePlayer.id !== p.player_id) {
        showToast("The previewed player is no longer open. Preview again.", "warning");
        clearRoleProposal();
        return false;
      }
      if (!state.activePlayer.positions) state.activePlayer.positions = {};
      let positionsChanged = false;
      for (const c of p.changes) {
        if (c.field.startsWith("attributes.")) {
          handleAttrInput(c.field.slice("attributes.".length), String(c.new), true);
        } else if (c.field.startsWith("positions.")) {
          state.activePlayer.positions[c.field.slice("positions.".length)] = c.new;
          positionsChanged = true;
        } else if (c.field === "current_ability") {
          handleCAInput(String(c.new));
          const el = document.getElementById("input-ca"); if (el) el.value = String(c.new);
        } else if (c.field === "potential_ability") {
          handlePAInput(String(c.new));
          const el = document.getElementById("input-pa"); if (el) el.value = String(c.new);
        }
      }
      if (positionsChanged) {
        state.playerDirtyFields.add("positions");
        renderPositionsTab();
      }
      markUnsaved();
      showToast(`${p.role_name} level ${p.level} staged (${p.changes.length} changes). Press Save to write it.`, "info");
      clearRoleProposal();
      return true;
    }

