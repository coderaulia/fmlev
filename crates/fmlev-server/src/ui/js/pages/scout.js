    // Scout page: GET /api/scout filters the search index; a name opens the Player or Staff tab.
    const SCOUT_PAGE_SIZE = 50;
    const scoutState = { offset: 0, total: 0, kind: "players", token: 0 };
    const SCOUT_FIELDS = ["q", "club", "nation", "generated", "position", "pos-min", "ca-min", "ca-max", "pa-min", "pa-max", "sort"];

    const scoutValue = id => (document.getElementById(`scout-${id}`)?.value || "").trim();

    function scoutParams(offset) {
      const kind = scoutValue("kind") === "staff" ? "staff" : "players";
      const params = new URLSearchParams({ kind, limit: String(SCOUT_PAGE_SIZE), offset: String(offset) });
      const add = (key, id) => { const value = scoutValue(id); if (value) params.set(key, value); };
      add("q", "q");
      add("club", "club");
      if (kind === "players") {
        add("nation", "nation");
        add("generated", "generated");
        add("position", "position");
        if (scoutValue("position")) add("pos_min", "pos-min");
        add("ca_min", "ca-min"); add("ca_max", "ca-max"); add("pa_min", "pa-min"); add("pa_max", "pa-max");
        if (scoutValue("sort") !== "name") add("sort", "sort");
      }
      return { kind, params };
    }

    function scoutKindChanged() {
      const staff = scoutValue("kind") === "staff";
      for (const node of document.querySelectorAll?.(".scout-player-only") || []) node.hidden = staff;
      scoutState.offset = 0;
      return runScout();
    }

    function resetScout() {
      for (const id of SCOUT_FIELDS) {
        const node = document.getElementById(`scout-${id}`);
        if (node) node.value = id === "pos-min" ? "15" : id === "sort" ? "name" : "";
      }
      scoutState.offset = 0;
      return runScout();
    }

    function scoutPage(direction) {
      const next = scoutState.offset + direction * SCOUT_PAGE_SIZE;
      if (next < 0 || next >= scoutState.total) return;
      return runScout(next);
    }

    function scoutPositions(item) {
      const list = Array.isArray(item.positions) ? item.positions : [];
      return list.length ? list.map(p => `${escapeHtml(String(p.position).toUpperCase())} ${Number(p.rating)}`).join(", ") : "—";
    }

    function renderScoutResults(data) {
      const box = document.getElementById("scout-results");
      if (!box) return;
      const items = Array.isArray(data.items) ? data.items : [];
      if (!items.length) { box.innerHTML = '<div class="scout-empty">No matches.</div>'; return; }
      const staff = data.kind === "staff";
      const head = staff
        ? "<tr><th>Name</th><th>Club</th></tr>"
        : "<tr><th>Name</th><th>Club</th><th>Nationality</th><th>CA</th><th>PA</th><th>Positions</th></tr>";
      const rows = items.map(item => {
        const club = item.club_name ? escapeHtml(item.club_name) : "—";
        if (staff) {
          return `<tr><td><button class="scout-link" onclick="openScoutStaff(${Number(item.index)}, ${Number(item.uid)})">${escapeHtml(item.name)}</button></td><td>${club}</td></tr>`;
        }
        const num = value => (Number.isFinite(Number(value)) && value !== null ? Number(value) : "—");
        return `<tr><td><button class="scout-link" onclick="openScoutPlayer(${Number(item.id)})">${escapeHtml(item.name)}</button></td><td>${club}</td>` +
          `<td>${item.nation ? escapeHtml(item.nation) : "—"}${item.generated ? ' <span class="scout-tag" title="Unique id 2,000,000,000 or above (heuristic)">gen</span>' : ""}</td>` +
          `<td>${num(item.ca)}</td><td>${num(item.pa)}</td><td>${scoutPositions(item)}</td></tr>`;
      }).join("");
      box.innerHTML = `<table class="fm-table scout-table"><thead>${head}</thead><tbody>${rows}</tbody></table>`;
    }

    async function runScout(offset = 0) {
      const { kind, params } = scoutParams(offset);
      const token = ++scoutState.token;
      const summary = document.getElementById("scout-summary");
      try {
        const res = await fetch(`/api/scout?${params}`);
        const data = await readJson(res);
        if (token !== scoutState.token) return null;
        if (!res.ok) throw new Error(data.message || "Scout search failed");
        scoutState.offset = Number(data.offset) || 0;
        scoutState.total = Number(data.total) || 0;
        scoutState.kind = kind;
        renderScoutResults(data);
        const shown = Array.isArray(data.items) ? data.items.length : 0;
        if (summary) summary.textContent = scoutState.total
          ? `${scoutState.offset + 1}-${scoutState.offset + shown} of ${formatSearchCount(scoutState.total)}`
          : "0 results";
        const prev = document.getElementById("scout-prev");
        const next = document.getElementById("scout-next");
        if (prev) prev.disabled = scoutState.offset <= 0;
        if (next) next.disabled = scoutState.offset + shown >= scoutState.total;
        return data;
      } catch (error) {
        if (token !== scoutState.token) return null;
        const box = document.getElementById("scout-results");
        if (box) box.innerHTML = `<div class="scout-empty">${escapeHtml(error.message)}</div>`;
        if (summary) summary.textContent = "";
        return null;
      }
    }

    function openScoutPlayer(id) { return selectSearchedPlayer(id); }
    function openScoutStaff(index, uid) { return selectSearchedStaff(index, uid); }
