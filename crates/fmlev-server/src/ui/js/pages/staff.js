    // ---- Staff (read-only native reads) ----
    const staffState = { results: [], selected: null, editing: false };
    const staffGroups = [["coaching", "Coaching"], ["mental", "Mental"], ["knowledge", "Knowledge"], ["scouting", "Scouting"], ["medical", "Medical"]];
    function staffFieldLabel(field) { return field.split("_").map(w => w[0].toUpperCase() + w.slice(1)).join(" "); }
    async function searchStaff() {
      const query = (document.getElementById("staff-query")?.value || "").trim();
      if (query.length < 2) { showToast("Type at least 2 letters.", "warning"); return; }
      const box = document.getElementById("staff-results");
      if (box) box.innerHTML = "Searching game memory…";
      try {
        const res = await fetch("/api/staff/search", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ query }) });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Staff search failed");
        staffState.results = data.staff;
        box.innerHTML = data.staff.length
          ? data.staff.map(s => `<div class="search-item" style="cursor:pointer;padding:4px 0;" onclick="selectStaff(${Number(s.index)}, ${Number(s.uid)})">${escapeHtml(s.name)} <span style="color:var(--text-dim);">#${Number(s.index)}</span></div>`).join("")
          : "No staff found.";
      } catch (error) { if (box) box.innerHTML = ""; showToast(error.message, "error"); }
    }
    function renderStaffDetail(data) {
      const detail = document.getElementById("staff-detail");
      if (!detail) return;
      const editing = !!staffState.editing;
      const cell = a => a.value === null
        ? `<span style="color:var(--accent-gold);" title="${escapeHtml(a.reason || "")}">Unavailable</span>`
        : editing
          ? `<input id="staff-attr-${escapeHtml(a.field)}" type="number" min="1" max="20" value="${a.value}" class="form-control" style="width:64px;padding:2px 6px;">`
          : `<b>${a.value}</b>`;
      detail.innerHTML = `<div style="font-weight:800;margin-bottom:6px;">${escapeHtml(data.staff.name)} <span style="color:var(--text-dim);font-weight:400;">#${Number(data.staff.index)}</span></div>` +
        `<div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:12px;">` +
        staffGroups.map(([key, title]) => {
          const rows = data.attributes.filter(a => a.group === key);
          return rows.length ? `<div><div style="font-size:11px;font-weight:800;text-transform:uppercase;color:var(--text-muted);margin-bottom:4px;">${title}</div>` +
            rows.map(a => `<div style="display:flex;justify-content:space-between;align-items:center;font-size:12px;padding:2px 0;"><span>${escapeHtml(staffFieldLabel(a.field))}</span>${cell(a)}</div>`).join("") + `</div>` : "";
        }).join("") + `</div>` +
        (editing ? `<button class="btn" style="margin-top:10px;" onclick="saveStaffAttributes()">Save staff attributes (experimental)</button>` : "") +
        `<div style="margin-top:8px;font-size:11px;color:var(--text-dim);">${escapeHtml(data.verification || "")}</div>`;
    }
    function toggleStaffEditing(enabled) {
      staffState.editing = !!enabled;
      if (staffState.selected) renderStaffDetail(staffState.selected);
    }
    async function saveStaffAttributes() {
      const data = staffState.selected;
      if (!data || !staffState.editing) return false;
      const attributes = {};
      for (const a of data.attributes) {
        const el = document.getElementById(`staff-attr-${a.field}`);
        if (!el || a.value === null) continue;
        const v = Number.parseInt(el.value, 10);
        if (!Number.isInteger(v) || v < 1 || v > 20) { showToast(`${staffFieldLabel(a.field)} must be 1–20.`, "warning"); return false; }
        if (v !== a.value) attributes[a.field] = v;
      }
      if (!Object.keys(attributes).length) { showToast("Nothing changed.", "info"); return false; }
      try {
        const res = await fetch("/api/staff/update", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ staff_id: data.staff.index, staff_uid: data.staff.uid, attributes, experimental: true })
        });
        const reply = await readJson(res);
        if (!res.ok) throw new Error(reply.message || "Staff save failed");
        if (reply.attributes) data.attributes = reply.attributes;
        renderStaffDetail(data);
        showToast(reply.message, "success");
        return true;
      } catch (error) { showToast(error.message, "error"); return false; }
    }
    async function selectStaff(id, uid) {
      const detail = document.getElementById("staff-detail");
      try {
        const res = await fetch("/api/staff/native", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ staff_id: id, staff_uid: uid }) });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Staff read failed");
        staffState.selected = data;
        renderStaffDetail(data);
      } catch (error) { staffState.selected = null; if (detail) detail.innerHTML = ""; showToast(error.message, "error"); }
    }

