    // ---- Staff: search, profile header, ability tabs. Nothing is written until the save bar's Save. ----
    const staffState = { results: [], selected: null, editing: false, tab: "head_coach", edits: {}, extra: {} };
    // Ability tabs: which attribute groups (and sub-groups) each one shows, in order.
    const STAFF_TABS = [
      { id: "head_coach", label: "Head Coach", groups: [{ group: "coaching", title: "Coaching" }, { group: "mental", title: "Mental" }, { group: "knowledge", title: "Knowledge" }] },
      { id: "coaching", label: "Coaching", groups: [{ group: "coaching", title: "Coaching" }, { group: "mental", title: "Mental" }] },
      { id: "scouting", label: "Scouting", groups: [{ group: "knowledge", title: "Knowledge" }, { group: "mental", title: "Mental" }, { group: "scouting", title: "Analyzing Data" }] },
      { id: "medical", label: "Medical", groups: [{ group: "knowledge", title: "Knowledge" }, { group: "mental", title: "Mental" },
        { group: "medical", title: "Physiotherapy", match: /physio/i }, { group: "medical", title: "Sports Science", match: /^(?!.*physio)/i }] },
      // Read-only views backed by their own routes, fetched when first opened. Unverified fields stay "Unavailable".
      { id: "information", label: "Information", kind: "information" },
      { id: "contract", label: "Contract", kind: "contract" },
    ];
    // FM26 ability labels measured in game (10 seen, 11 inferred).
    function staffAbilityLabel(value) {
      const v = Number(value);
      if (!Number.isInteger(v) || v < 1 || v > 20) return "";
      if (v <= 3) return "Unsuited";
      if (v <= 6) return "Reasonable";
      if (v <= 9) return "Competent";
      if (v <= 11) return "Average";
      if (v <= 14) return "Good";
      if (v <= 17) return "Very Good";
      if (v <= 19) return "Outstanding";
      return "Elite";
    }
    function staffFieldLabel(field) { return field.split("_").map(w => w[0].toUpperCase() + w.slice(1)).join(" "); }
    function staffByField(field) { return staffState.selected ? staffState.selected.attributes.find(a => a.field === field) : null; }
    function staffEl(id) { return typeof document !== "undefined" && document.getElementById ? document.getElementById(id) : null; }
    function staffRequest(url, body) {
      return fetch(url, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
    }

    async function searchStaff() {
      const query = (staffEl("staff-query")?.value || "").trim();
      if (query.length < 2) { showToast("Type at least 2 letters.", "warning"); return; }
      const box = staffEl("staff-results");
      if (box) box.innerHTML = "Searching game memory...";
      try {
        const res = await staffRequest("/api/staff/search", { query });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Staff search failed");
        staffState.results = data.staff;
        if (box) box.innerHTML = data.staff.length
          ? `<table class="fm-table"><thead><tr><th>Name</th><th class="num">ID</th></tr></thead><tbody>` +
            data.staff.map(s => `<tr onclick="selectStaff(${Number(s.index)}, ${Number(s.uid)})"><td>${escapeHtml(s.name)}</td><td class="num">#${Number(s.index)}</td></tr>`).join("") + `</tbody></table>`
          : "No staff found.";
      } catch (error) { if (box) box.innerHTML = ""; showToast(error.message, "error"); }
    }

    function staffAttrLine(a) {
      if (a.value === null) {
        return `<div class="attr-line unavailable"><span class="attr-line-name">${escapeHtml(staffFieldLabel(a.field))}</span>` +
          `<span class="attr-line-val">Unavailable <span class="unverified-note">${escapeHtml(a.reason || "not read from the game")}</span></span></div>`;
      }
      const key = escapeHtml(a.field);
      const edited = Object.prototype.hasOwnProperty.call(staffState.edits, a.field);
      const shown = edited ? staffState.edits[a.field] : a.value;
      const num = Number(shown);
      const value = staffState.editing
        ? `<span class="stepper"><button type="button" onclick="staffStep('${key}', -1)">-</button>` +
          `<input id="staff-attr-${key}" type="number" min="1" max="20" value="${escapeHtml(String(shown))}" class="${edited ? "changed" : ""} ${valueBand(num)}" oninput="staffSetEdit('${key}', this.value)">` +
          `<button type="button" onclick="staffStep('${key}', 1)">+</button></span>`
        : `<b class="attr-line-val ${valueBand(num)}">${a.value}</b>`;
      return `<div class="attr-line"><span class="attr-line-name">${escapeHtml(staffFieldLabel(a.field))}</span>${value}` +
        `<span id="staff-lbl-${key}" class="attr-line-label ${valueBand(num)}">${escapeHtml(staffAbilityLabel(num))}</span></div>`;
    }

    function renderStaffDetail(data) {
      const detail = staffEl("staff-detail");
      const profile = staffEl("staff-profile");
      if (!detail) return;
      if (!data) { detail.innerHTML = ""; if (profile) profile.hidden = true; return; }
      if (profile) profile.hidden = false;
      const header = staffEl("staff-header");
      if (header) header.innerHTML = `<span class="staff-name">${escapeHtml(data.staff.name)}</span><span class="staff-id">#${Number(data.staff.index)}</span>` +
        (staffState.editing ? `<span class="pill pill-warn">Experimental editing</span>` : "");
      const tab = STAFF_TABS.find(t => t.id === staffState.tab) || STAFF_TABS[0];
      if (tab.kind) { detail.innerHTML = renderStaffExtra(tab.kind, data); return; }
      const cols = tab.groups.map(g => {
        const rows = data.attributes.filter(a => a.group === g.group && (!g.match || g.match.test(a.field)));
        return rows.length ? `<div class="attr-col"><div class="attr-col-head">${g.title}</div>${rows.map(staffAttrLine).join("")}</div>` : "";
      }).join("");
      detail.innerHTML = `<div class="attr-table">${cols}</div>` +
        `<div class="staff-note">${escapeHtml(data.verification || "")}</div>`;
    }

    // Information / Contract: read-only rows. Only fields the server marks "verified" show a value; the rest say why not.
    function staffExtraRow(f) {
      const label = escapeHtml(f.label || staffFieldLabel(f.field));
      if (f.status === "verified" && f.value !== null && f.value !== undefined) {
        return `<div class="attr-line"><span class="attr-line-name">${label}</span><b class="attr-line-val">${escapeHtml(String(f.value))}</b></div>`;
      }
      return `<div class="attr-line unavailable"><span class="attr-line-name">${label}</span>` +
        `<span class="attr-line-val">Unavailable <span class="unverified-note">${escapeHtml(f.reason || "not verified")}</span></span></div>`;
    }
    function renderStaffExtra(kind, data) {
      const entry = staffState.extra[kind];
      const title = kind === "contract" ? "Contract" : "Information";
      if (!entry) return `<div class="staff-note">Loading ${title.toLowerCase()}...</div>`;
      if (entry.error) return `<div class="attr-line unavailable"><span class="attr-line-val">Unavailable <span class="unverified-note">${escapeHtml(entry.error)}</span></span></div>`;
      const s = data.staff;
      let rows;
      if (kind === "information") {
        const identity = [
          { label: "Name", status: "verified", value: s.name },
          { label: "ID", status: "verified", value: s.index },
          { label: "Unique ID", status: "verified", value: s.uid },
        ];
        rows = `<div class="attr-col"><div class="attr-col-head">Identity (verified)</div>${identity.map(staffExtraRow).join("")}</div>` +
          `<div class="attr-col"><div class="attr-col-head">Personal details</div>${entry.data.fields.map(staffExtraRow).join("")}</div>`;
      } else {
        const c = entry.data.contract;
        rows = `<div class="attr-col"><div class="attr-col-head">Contract <span class="pill pill-warn">Read-only</span></div>${c.fields.map(staffExtraRow).join("")}</div>`;
      }
      const note = kind === "contract" ? entry.data.contract.reason : entry.data.verification;
      return `<div class="attr-table">${rows}</div><div class="staff-note">${escapeHtml(note || "")} Editing is unavailable for this view.</div>`;
    }
    async function loadStaffExtra(kind) {
      const current = staffState.selected;
      if (!current || staffState.extra[kind]) return;
      try {
        const res = await staffRequest(`/api/staff/${kind}`, { staff_id: current.staff.index, staff_uid: current.staff.uid });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || `Staff ${kind} read failed`);
        if (staffState.selected !== current) return;
        staffState.extra[kind] = { data };
      } catch (error) {
        if (staffState.selected !== current) return;
        staffState.extra[kind] = { error: error.message };
      }
      if (staffState.selected === current) renderStaffDetail(current);
    }

    function setStaffTab(id) {
      const tab = STAFF_TABS.find(t => t.id === id);
      if (!tab) return;
      staffState.tab = id;
      if (typeof Shell !== "undefined") Shell.setActiveSubTab(id, "staff");
      renderStaffDetail(staffState.selected);
      return tab.kind && staffState.selected ? loadStaffExtra(tab.kind) : undefined;
    }

    function toggleStaffEditing(enabled) {
      staffState.editing = !!enabled;
      if (!enabled) staffState.edits = {};
      renderStaffDetail(staffState.selected);
    }

    // Records an edit only; nothing is sent until Save in the save bar.
    function staffSetEdit(field, raw) {
      const a = staffByField(field);
      if (!a || a.value === null || !staffState.editing) return;
      const text = String(raw).trim();
      const n = Number(text);
      if (text !== "" && Number.isInteger(n) && n === a.value) delete staffState.edits[field];
      else staffState.edits[field] = text !== "" && Number.isInteger(n) ? n : text;
      const input = staffEl(`staff-attr-${field}`);
      if (input && input.classList) { if (field in staffState.edits) input.classList.add("changed"); else input.classList.remove("changed"); }
      const label = staffEl(`staff-lbl-${field}`);
      if (label) { label.textContent = staffAbilityLabel(n); label.className = `attr-line-label ${valueBand(n)}`; }
      if (typeof SaveBar !== "undefined") SaveBar.markDirty("staff");
    }
    function staffStep(field, delta) {
      const a = staffByField(field);
      if (!a || a.value === null) return;
      const current = Object.prototype.hasOwnProperty.call(staffState.edits, field) ? Number(staffState.edits[field]) : a.value;
      const next = Math.max(1, Math.min(20, (Number.isInteger(current) ? current : a.value) + delta));
      const input = staffEl(`staff-attr-${field}`);
      if (input) input.value = String(next);
      staffSetEdit(field, next);
    }

    function staffChanges() {
      return Object.keys(staffState.edits).map(field => {
        const a = staffByField(field);
        return { label: staffFieldLabel(field), from: a ? a.value : undefined, to: staffState.edits[field] };
      });
    }

    async function loadStaff(id, uid) {
      const res = await staffRequest("/api/staff/native", { staff_id: id, staff_uid: uid });
      const data = await readJson(res);
      if (!res.ok) throw new Error(data.message || "Staff read failed");
      staffState.selected = data;
      staffState.edits = {};
      staffState.extra = {};
      renderStaffDetail(data);
      const open = STAFF_TABS.find(t => t.id === staffState.tab);
      if (open && open.kind) loadStaffExtra(open.kind);
      if (typeof SaveBar !== "undefined") SaveBar.clear("staff");
      return data;
    }

    async function selectStaff(id, uid) {
      try {
        const data = await loadStaff(id, uid);
        if (typeof Shell !== "undefined") {
          Shell.setContext({ section: "staff", kind: "staff", id: data.staff.index, title: data.staff.name, subtitle: "Staff",
            open: () => { switchTab("tab-staff"); return selectStaff(id, uid); } });
        }
      } catch (error) { staffState.selected = null; staffState.edits = {}; renderStaffDetail(null); showToast(error.message, "error"); }
    }

    // Save bar "Load": re-read the selected member from the game, dropping edits.
    async function loadStaffFromGame() {
      const current = staffState.selected;
      if (!current) { showToast("Select a staff member first.", "warning"); return false; }
      try { await loadStaff(current.staff.index, current.staff.uid); return true; }
      catch (error) { showToast(error.message, "error"); return false; }
    }
    function discardStaffEdits() { staffState.edits = {}; renderStaffDetail(staffState.selected); if (typeof SaveBar !== "undefined") SaveBar.clear("staff"); return true; }

    // Save bar "Save": only changed fields, with the experimental opt-in and staff identity.
    async function saveStaffAttributes() {
      const data = staffState.selected;
      if (!data || !staffState.editing) return false;
      const attributes = {};
      for (const [field, raw] of Object.entries(staffState.edits)) {
        const a = staffByField(field);
        if (!a || a.value === null) continue;
        const v = typeof raw === "number" ? raw : Number.parseInt(raw, 10);
        if (!Number.isInteger(v) || v < 1 || v > 20) { showToast(`${staffFieldLabel(field)} must be 1-20.`, "warning"); return false; }
        if (v !== a.value) attributes[field] = v;
      }
      if (!Object.keys(attributes).length) { showToast("Nothing changed.", "info"); return false; }
      try {
        const submittedEdits = { ...staffState.edits };
        const res = await staffRequest("/api/staff/update", { staff_id: data.staff.index, staff_uid: data.staff.uid, attributes, experimental: true });
        const reply = await readJson(res);
        if (!res.ok) throw new Error(reply.message || "Staff save failed");
        if (staffState.selected !== data) return false;
        showToast(reply.message, "success");
        if (reply.attributes) data.attributes = reply.attributes;
        else for (const a of data.attributes) if (attributes[a.field] !== undefined) a.value = attributes[a.field];
        for (const [field, value] of Object.entries(submittedEdits)) if (staffState.edits[field] === value) delete staffState.edits[field];
        renderStaffDetail(data);
        return Object.keys(staffState.edits).length === 0;
      } catch (error) { showToast(error.message, "error"); return false; }
    }

    SaveBar.register("staff", { load: loadStaffFromGame, save: saveStaffAttributes, discard: discardStaffEdits, changes: staffChanges });
    try { Shell.setSubTabs(STAFF_TABS.map(t => ({ id: t.id, label: t.label })), staffState.tab, { section: "staff", onSelect: setStaffTab }); } catch (error) { /* shell not ready in a bare sandbox */ }
