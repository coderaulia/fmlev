    // Instant Search Implementation
    // GET /api/search?q=&kind=&limit=&offset= answers from the server's search index (or its registries on older servers).
    // GET /api/search/status drives the index chip; POST /api/search/rebuild rebuilds it.
    let searchDebounce = null;
    let searchToken = 0;
    let searchFilter = "all";
    let searchResults = null;
    const SEARCH_PAGE_SIZE = 25;
    const SEARCH_KINDS = ["players", "clubs", "nations", "competitions", "staff"];
    const SEARCH_KIND_LABELS = { players: "Players", staff: "Staff", clubs: "Clubs", nations: "Nations", competitions: "Competitions" };
    const SEARCH_ITEM_KEYS = {
      players: p => `${p.id}`, clubs: c => `${c.club_id}`, nations: n => `${n.id}`, competitions: c => `${c.id}`, staff: s => `${s.index}:${s.uid}`,
    };
    const SEARCH_RECENT_KEY = "fmle.search.recent";
    const SEARCH_RECENT_MAX = 8;
    const SEARCH_RECENT_KINDS = ["player", "staff", "club", "nation", "competition"];
    const searchIndex = { status: null, timer: null, delay: null, stopped: false };

    const formatSearchCount = n => Number(n || 0).toLocaleString("en-US");
    const knownNumber = n => Number.isFinite(Number(n)) && Number(n) > 0;
    const knownText = s => typeof s === "string" && s.trim().length > 0;
    const searchItemKeydown = `onkeydown="if(event.key==='Enter'||event.key===' '){event.preventDefault();this.click()}"`;

    function setSearchFilter(type) {
      if (!["all", "players", "staff", "clubs", "nations", "competitions"].includes(type)) return;
      searchFilter = type;
      for (const chip of document.querySelectorAll?.("[data-search-type]") || []) chip.setAttribute?.("aria-pressed", String(chip.dataset.searchType === type));
      return performSearch(document.getElementById("global-search")?.value || "");
    }
    function handleSearchInput(query) {
      const filters = document.getElementById("search-filters");
      if (filters) filters.hidden = !query.trim();
      const clearBtn = document.getElementById("search-clear-btn");
      if (clearBtn) clearBtn.style.display = query.length > 0 ? "block" : "none";

      clearTimeout(searchDebounce);
      searchDebounce = setTimeout(() => {
        performSearch(query);
      }, 150);
    }

    function searchUrl(query, kind, offset = 0) {
      let url = `/api/search?q=${encodeURIComponent(query)}`;
      if (kind && kind !== "all") url += `&kind=${kind}`;
      if (offset > 0) url += `&limit=${SEARCH_PAGE_SIZE}&offset=${offset}`;
      return url;
    }

    async function performSearch(query) {
      const dropdown = document.getElementById("search-results-dropdown");
      if (!dropdown) return;
      const q = (query || "").trim();
      if (!q) {
        searchToken++;
        searchResults = null;
        renderRecentSelections();
        return;
      }

      const token = ++searchToken;
      try {
        const res = await fetch(searchUrl(q, searchFilter));
        if (token !== searchToken) return;
        if (!res.ok) return;
        const data = await readJson(res);
        if (token !== searchToken) return;
        if (data.index && typeof data.index === "object") applyIndexStatus(data.index);

        const results = buildSearchResults(q, data);
        searchResults = results;
        renderSearchResults();

        // Older servers (or an index that has no staff yet) omit `staff`: fall back to the native staff scan.
        if (results.staffPending) {
          const staff = await searchStaffForDropdown(q);
          if (token !== searchToken || results !== searchResults) return;
          results.staffPending = false;
          results.sections.staff = { items: staff, total: staff.length, pageable: false, loading: false };
          renderSearchResults();
        }
      } catch (e) {
        console.error("Search error:", e);
      }
    }

    function buildSearchResults(query, data) {
      const wants = kind => searchFilter === "all" || searchFilter === kind;
      const totals = data.totals && typeof data.totals === "object" ? data.totals : null;
      const sections = {};
      for (const kind of SEARCH_KINDS) {
        if (!wants(kind)) continue;
        if (kind === "staff" && !Array.isArray(data.staff)) continue;
        const items = Array.isArray(data[kind]) ? data[kind].slice() : [];
        let total = totals && Number.isFinite(Number(totals[kind])) ? Number(totals[kind]) : null;
        if (total === null && kind === "players" && Number.isInteger(data.total_players)) total = data.total_players;
        // Only a server that reports per-kind totals pages with offset; older servers would repeat the first page.
        sections[kind] = { items, total: Math.max(total ?? items.length, items.length), pageable: !!totals, loading: false };
      }
      const staffPending = wants("staff") && !Array.isArray(data.staff) && query.length >= 2;
      return {
        query, sections, staffPending, focus: null,
        index: data.index && typeof data.index === "object" ? data.index : null,
        source: data.source || null,
        discoveryError: data.discovery_error || null,
      };
    }

    function renderPlayerResult(p, pos) {
      const isLive = isLivePlayer(p);
      const caStr = knownNumber(p.current_ability) ? p.current_ability : "—";
      const paStr = knownNumber(p.potential_ability) ? p.potential_ability : "—";
      const natStr = knownText(p.nationality) ? getCountryFlag(p.nationality) : "🌐";
      const meta = [];
      if (knownText(p.position)) meta.push(escapeHtml(p.position));
      if (knownText(p.club_name)) meta.push(escapeHtml(p.club_name));
      const metaHtml = meta.length ? meta.join(" • ") : '<span style="color:var(--text-dim); font-style:italic;">No details yet</span>';
      return `
            <div class="search-item" role="button" tabindex="0" data-kind="players" data-pos="${pos}" ${searchItemKeydown} onclick="selectSearchedPlayer(${Number(p.id)})">
              <div class="search-item-info">
                <div class="search-item-avatar">${natStr}</div>
                <div>
                  <div class="search-item-name">
                    ${escapeHtml(p.name)}
                    ${isLive ? '<span class="live-badge-sm">LIVE</span>' : ''}
                  </div>
                  <div class="search-item-meta">${metaHtml}</div>
                </div>
              </div>
              <span class="search-item-badge">CA ${caStr} / PA ${paStr}</span>
            </div>
            `;
    }

    function renderClubResult(c, pos) {
      return `
            <div class="search-item" role="button" tabindex="0" data-kind="clubs" data-pos="${pos}" ${searchItemKeydown} onclick="selectSearchedClub(${Number(c.club_id)})">
              <div class="search-item-info">
                <div class="search-item-avatar">🏟️</div>
                <div>
                  <div class="search-item-name">${escapeHtml(c.club_name)}</div>
                  <div class="search-item-meta">${knownText(c.league_name) ? escapeHtml(c.league_name) : "Club"}</div>
                </div>
              </div>
              ${knownNumber(c.reputation) ? `<span class="search-item-badge">Rep: ${formatSearchCount(c.reputation)}</span>` : ""}
            </div>
          `;
    }

    function renderNationResult(n, pos) {
      const meta = [];
      if (knownText(n.continent)) meta.push(escapeHtml(n.continent));
      if (knownNumber(n.reputation)) meta.push(`Rep: ${formatSearchCount(n.reputation)}`);
      return `
            <div class="search-item" role="button" tabindex="0" data-kind="nations" data-pos="${pos}" ${searchItemKeydown} onclick="selectSearchedNation(${Number(n.id)})">
              <div class="search-item-info">
                <div class="search-item-avatar">${getCountryFlag(n.name)}</div>
                <div>
                  <div class="search-item-name">${escapeHtml(n.name)}${knownText(n.three_letter_code) ? ` <span style="font-size:10px; color:var(--text-dim);">(${escapeHtml(n.three_letter_code)})</span>` : ""}</div>
                  <div class="search-item-meta">${meta.length ? meta.join(" • ") : "Nation"}</div>
                </div>
              </div>
              ${knownNumber(n.youth_rating) ? `<span class="search-item-badge" style="background: rgba(6, 182, 212, 0.15); color: #22d3ee; border-color: rgba(6, 182, 212, 0.3);">Youth ${Number(n.youth_rating)}/200</span>` : ""}
            </div>
          `;
    }

    function renderCompetitionResult(comp, pos) {
      const meta = [];
      if (knownText(comp.nation_name)) meta.push(escapeHtml(comp.nation_name));
      if (knownNumber(comp.max_substitutes) && knownNumber(comp.max_bench_size)) meta.push(`Subs: ${Number(comp.max_substitutes)}/${Number(comp.max_bench_size)}`);
      return `
            <div class="search-item" role="button" tabindex="0" data-kind="competitions" data-pos="${pos}" ${searchItemKeydown} onclick="selectSearchedCompetition(${Number(comp.id)})">
              <div class="search-item-info">
                <div class="search-item-avatar">🏆</div>
                <div>
                  <div class="search-item-name">${escapeHtml(comp.name)}${knownText(comp.short_name) ? ` <span style="font-size:10px; color:var(--text-dim);">(${escapeHtml(comp.short_name)})</span>` : ""}</div>
                  <div class="search-item-meta">${meta.length ? meta.join(" • ") : "Competition"}</div>
                </div>
              </div>
              ${knownNumber(comp.reputation) ? `<span class="search-item-badge" style="background: rgba(168, 85, 247, 0.15); color: #c084fc; border-color: rgba(168, 85, 247, 0.3);">Rep: ${formatSearchCount(comp.reputation)}</span>` : ""}
            </div>
          `;
    }

    function renderStaffResult(s, pos) {
      return `
            <div class="search-item" role="button" tabindex="0" data-kind="staff" data-pos="${pos}" ${searchItemKeydown} onclick="selectSearchedStaff(${Number(s.index)}, ${Number(s.uid)})">
              <div class="search-item-info">
                <div class="search-item-avatar">ST</div>
                <div>
                  <div class="search-item-name">${escapeHtml(s.name)}</div>
                  <div class="search-item-meta">Staff #${Number(s.index)}</div>
                </div>
              </div>
              <span class="search-item-badge" style="background: rgba(90, 169, 240, 0.15); color: #8cc4f5; border-color: rgba(90, 169, 240, 0.3);">Staff</span>
            </div>
          `;
    }

    const SEARCH_RENDERERS = {
      players: renderPlayerResult, clubs: renderClubResult, nations: renderNationResult, competitions: renderCompetitionResult, staff: renderStaffResult,
    };

    function searchSectionHtml(kind, section) {
      const shown = section.items.length;
      const countLabel = section.total > shown ? `${formatSearchCount(shown)} of ${formatSearchCount(section.total)}` : formatSearchCount(shown);
      let html = `<div class="search-section-title">${SEARCH_KIND_LABELS[kind]} (${countLabel})</div>`;
      html += section.items.map((item, pos) => SEARCH_RENDERERS[kind](item, pos)).join("");
      if (section.pageable && section.total > shown) {
        const label = section.loading ? "Loading…" : `Show more ${SEARCH_KIND_LABELS[kind].toLowerCase()} (${formatSearchCount(section.total - shown)} more)`;
        html += `<div class="search-item search-more" role="button" tabindex="0" data-kind="${kind}" data-more="1" ${searchItemKeydown} onclick="loadMoreSearchResults('${kind}')">${label}</div>`;
      }
      return html;
    }

    function emptySearchMessage(results) {
      const index = results.index || searchIndex.status;
      const state = index && index.state;
      if (state === "building") {
        return { text: `No matches yet: the search index is still building${knownText(index.stage) ? ` (${index.stage})` : ""}. Results appear as each stage finishes.` };
      }
      if (state === "idle") return { text: "No matches. The loaded save has not been indexed yet.", rebuild: true };
      if (state === "error") return { text: `No matches. Indexing failed${knownText(index.error) ? `: ${index.error}` : ""}.`, rebuild: true };
      const what = searchFilter === "all" ? "players, staff, clubs, nations or competitions" : searchFilter;
      if (state === "ready") return { text: `No ${what} match "${results.query}".` };
      return { text: results.discoveryError || `No ${what} found.` };
    }

    function renderSearchResults() {
      const dropdown = document.getElementById("search-results-dropdown");
      const results = searchResults;
      if (!dropdown || !results) return;
      let html = "";
      for (const kind of SEARCH_KINDS) {
        const section = results.sections[kind];
        if (section && section.items.length > 0) html += searchSectionHtml(kind, section);
      }
      if (results.staffPending) html += `<div class="search-section-title">Staff</div><div class="search-empty">Searching staff…</div>`;
      if (!html) {
        const message = emptySearchMessage(results);
        html = `<div class="search-empty">${escapeHtml(message.text)}</div>`;
        if (message.rebuild) html += `<div class="search-item search-more" role="button" tabindex="0" ${searchItemKeydown} onclick="rebuildSearchIndex()">Rebuild index</div>`;
      }
      dropdown.innerHTML = html;
      dropdown.style.display = "block";
      restoreSearchHighlight(dropdown, results.focus);
    }

    // After "Show more" re-renders the list, keep keyboard focus on the first new row (or the Show more row while loading).
    function restoreSearchHighlight(dropdown, focus) {
      if (!focus || !dropdown.querySelectorAll) return;
      const items = Array.from(dropdown.querySelectorAll(".search-item"));
      const target = items.find(item => item.dataset && item.dataset.kind === focus.kind &&
        (focus.more ? item.dataset.more === "1" : item.dataset.pos === String(focus.pos)));
      if (!target) return;
      items.forEach(item => item.classList.toggle("highlighted", item === target));
      if (target.scrollIntoView) target.scrollIntoView({ block: "nearest" });
    }

    // "Show more": same query, one kind, next page; rows are appended (duplicates dropped).
    async function loadMoreSearchResults(kind) {
      const results = searchResults;
      const section = results && results.sections[kind];
      if (!section || section.loading || !section.pageable) return;
      section.loading = true;
      const token = searchToken;
      const offset = section.items.length;
      results.focus = { kind, more: true };
      try {
        const res = await fetch(searchUrl(results.query, kind, offset));
        const data = await readJson(res);
        if (token !== searchToken || results !== searchResults) return;
        if (!res.ok) throw new Error(data.message || `Could not load more ${kind}`);
        if (data.index && typeof data.index === "object") applyIndexStatus(data.index);
        const more = Array.isArray(data[kind]) ? data[kind] : [];
        const keyOf = SEARCH_ITEM_KEYS[kind];
        const seen = new Set(section.items.map(keyOf));
        const fresh = more.filter(item => !seen.has(keyOf(item)));
        section.items.push(...fresh);
        const total = data.totals && Number(data.totals[kind]);
        if (Number.isFinite(total)) section.total = Math.max(total, section.items.length);
        if (!fresh.length) section.total = section.items.length; // nothing new: stop offering more
        results.focus = fresh.length ? { kind, pos: offset } : null;
      } catch (e) {
        showToast(e.message, "error");
      } finally {
        section.loading = false;
        if (results === searchResults && token === searchToken) renderSearchResults();
      }
    }

    // POST /api/staff/search {query}; resolves to [] for short queries or any failure.
    async function searchStaffForDropdown(query) {
      if (query.trim().length < 2) return [];
      try {
        const res = await fetch("/api/staff/search", {
          method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ query: query.trim() })
        });
        if (!res.ok) return [];
        const data = await readJson(res);
        return Array.isArray(data.staff) ? data.staff : [];
      } catch (e) { return []; }
    }

    // ---- Index status chip ----
    function relativeTimeSince(unixSeconds, now = Date.now()) {
      const seconds = Math.max(0, Math.round(now / 1000 - Number(unixSeconds)));
      if (seconds < 45) return "just now";
      const minutes = Math.round(seconds / 60);
      if (minutes < 90) return `${minutes} min ago`;
      const hours = Math.round(minutes / 60);
      if (hours < 36) return `${hours} h ago`;
      return `${Math.round(hours / 24)} d ago`;
    }

    function indexCountsSummary(counts) {
      if (!counts || typeof counts !== "object") return "";
      return SEARCH_KINDS.filter(kind => Number.isFinite(Number(counts[kind])))
        .map(kind => `${formatSearchCount(counts[kind])} ${SEARCH_KIND_LABELS[kind].toLowerCase()}`).join(", ");
    }

    // Returns { state, text, title } for a /api/search/status body, or null when the state is unknown.
    function indexChipView(status, now = Date.now()) {
      if (!status || typeof status !== "object") return null;
      const counts = indexCountsSummary(status.counts);
      if (status.state === "building") {
        return { state: "building", text: `Indexing…${knownText(status.stage) ? ` ${status.stage}` : ""}`, title: counts ? `Indexed so far: ${counts}` : "Indexing the loaded save" };
      }
      if (status.state === "ready") {
        const players = status.counts && Number.isFinite(Number(status.counts.players)) ? Number(status.counts.players) : 0;
        let text = `Index ready · ${formatSearchCount(players)} players`;
        if (status.built_at !== null && status.built_at !== undefined && Number.isFinite(Number(status.built_at))) text += ` · ${relativeTimeSince(status.built_at, now)}`;
        const details = [counts && `Indexed: ${counts}`];
        if (Number.isFinite(Number(status.duration_ms)) && status.duration_ms !== null) details.push(`built in ${(Number(status.duration_ms) / 1000).toFixed(1)} s`);
        if (knownNumber(status.duplicates_dropped)) details.push(`${formatSearchCount(status.duplicates_dropped)} duplicates dropped`);
        return { state: "ready", text, title: details.filter(Boolean).join("; ") };
      }
      if (status.state === "error") return { state: "error", text: "Index error", title: knownText(status.error) ? status.error : "Indexing failed" };
      if (status.state === "idle") return { state: "idle", text: "Not indexed", title: "Use Rebuild index to index the loaded save" };
      return null;
    }

    function renderIndexChip(status) {
      const chip = document.getElementById("search-index-chip");
      if (!chip) return;
      const view = indexChipView(status);
      if (!view) { chip.hidden = true; return; }
      chip.hidden = false;
      chip.textContent = view.text;
      chip.title = view.title;
      if (chip.dataset) chip.dataset.state = view.state;
    }

    function applyIndexStatus(status) {
      const previous = searchIndex.status;
      searchIndex.status = status;
      renderIndexChip(status);
      // A finished build can answer a query that had no hits while indexing: refresh the open results once.
      if (previous && previous.state === "building" && status.state === "ready" && searchResults) {
        const box = document.getElementById("global-search");
        if (box && box.value && box.value.trim() === searchResults.query) performSearch(box.value);
      }
      if (!searchIndex.stopped && searchIndex.delay !== null && searchIndex.delay !== indexPollDelay()) scheduleIndexPoll();
    }

    const indexPollDelay = () => (searchIndex.status && searchIndex.status.state === "building" ? 2000 : 30000);

    function scheduleIndexPoll() {
      clearTimeout(searchIndex.timer);
      if (searchIndex.stopped) return;
      searchIndex.delay = indexPollDelay();
      searchIndex.timer = setTimeout(refreshSearchIndexStatus, searchIndex.delay);
    }

    // Polls every 2 s while building, every 30 s otherwise. Failures are silent; a server without the route stops polling.
    async function refreshSearchIndexStatus() {
      try {
        const res = await fetch("/api/search/status");
        if (res.status === 404) {
          searchIndex.stopped = true;
          clearTimeout(searchIndex.timer);
          renderIndexChip(null);
          return null;
        }
        if (res.ok) {
          const status = await readJson(res);
          if (status && typeof status === "object" && status.state) applyIndexStatus(status);
        }
      } catch (e) { /* offline or restarting: keep the last chip and try again later */ }
      scheduleIndexPoll();
      return searchIndex.status;
    }

    async function rebuildSearchIndex() {
      const button = document.getElementById("rebuild-index-btn");
      if (button) button.disabled = true;
      try {
        const res = await fetch("/api/search/rebuild", { method: "POST" });
        const data = await readJson(res);
        if (res.ok) {
          showToast("Rebuilding the search index…", "info");
          searchIndex.stopped = false;
          applyIndexStatus({ ...(searchIndex.status || {}), state: "building", stage: null, error: null });
          scheduleIndexPoll();
        } else {
          showToast(data.message || `Index rebuild failed (HTTP ${res.status})`, res.status === 409 ? "info" : "error");
        }
      } catch (e) {
        showToast("Index rebuild failed: " + e.message, "error");
      } finally {
        if (button) button.disabled = false;
      }
    }

    if (typeof window !== "undefined" && window.addEventListener) window.addEventListener("DOMContentLoaded", () => { refreshSearchIndexStatus(); });

    // ---- Recent selections (last 8 opened results; storage optional) ----
    let recentSearchMemory = null;

    function validRecent(entry) {
      return entry && SEARCH_RECENT_KINDS.includes(entry.kind) && Number.isFinite(Number(entry.id));
    }

    function readRecentSelections() {
      if (recentSearchMemory) return recentSearchMemory;
      let list = [];
      try {
        const raw = localStorage.getItem(SEARCH_RECENT_KEY);
        const parsed = raw ? JSON.parse(raw) : [];
        if (Array.isArray(parsed)) list = parsed.filter(validRecent).slice(0, SEARCH_RECENT_MAX);
      } catch (e) { /* storage blocked or corrupt: start empty */ }
      recentSearchMemory = list;
      return list;
    }

    function rememberSearchSelection(entry) {
      if (!validRecent(entry)) return;
      const item = {
        kind: entry.kind, id: Number(entry.id),
        uid: entry.uid === undefined || entry.uid === null ? null : Number(entry.uid),
        name: String(entry.name || ""),
      };
      const same = r => r.kind === item.kind && Number(r.id) === item.id && (r.uid ?? null) === item.uid;
      recentSearchMemory = [item, ...readRecentSelections().filter(r => !same(r))].slice(0, SEARCH_RECENT_MAX);
      try { localStorage.setItem(SEARCH_RECENT_KEY, JSON.stringify(recentSearchMemory)); } catch (e) { /* this tab only */ }
    }

    const RECENT_KIND_LABELS = { player: "Player", staff: "Staff", club: "Club", nation: "Nation", competition: "Competition" };

    function renderRecentSelections() {
      const dropdown = document.getElementById("search-results-dropdown");
      if (!dropdown) return false;
      const recents = readRecentSelections();
      if (!recents.length) { dropdown.style.display = "none"; return false; }
      dropdown.innerHTML = `<div class="search-section-title">Recent</div>` + recents.map((r, i) => `
            <div class="search-item search-recent" role="button" tabindex="0" data-kind="recent" data-pos="${i}" ${searchItemKeydown} onclick="openRecentSelection(${i})">
              <div class="search-item-info">
                <div class="search-item-avatar">↺</div>
                <div>
                  <div class="search-item-name">${escapeHtml(r.name || `${RECENT_KIND_LABELS[r.kind]} #${Number(r.id)}`)}</div>
                  <div class="search-item-meta">${RECENT_KIND_LABELS[r.kind]}</div>
                </div>
              </div>
            </div>`).join("");
      dropdown.style.display = "block";
      return true;
    }

    // Focus on an empty search box shows the recent selections.
    function showRecentSelections() {
      const box = document.getElementById("global-search");
      if (box && box.value && box.value.trim()) return false;
      return renderRecentSelections();
    }

    function openRecentSelection(position) {
      const entry = readRecentSelections()[position];
      if (!entry) return;
      if (entry.kind === "player") return selectSearchedPlayer(entry.id);
      if (entry.kind === "club") return selectSearchedClub(entry.id);
      if (entry.kind === "nation") return selectSearchedNation(entry.id);
      if (entry.kind === "competition") return selectSearchedCompetition(entry.id);
      if (entry.kind === "staff") return selectSearchedStaff(entry.id, entry.uid);
    }

    // Keyboard: Escape clears, Arrow keys move through results, Enter opens the highlighted (or first) result.
    function handleSearchKey(event) {
      if (event.key === "Escape") {
        clearSearch();
        return;
      }
      const dropdown = document.getElementById("search-results-dropdown");
      let items = dropdown && dropdown.querySelectorAll ? Array.from(dropdown.querySelectorAll(".search-item")) : [];
      if (dropdown && event.key === "ArrowDown" && (!items.length || dropdown.style.display === "none") && showRecentSelections()) {
        items = dropdown.querySelectorAll ? Array.from(dropdown.querySelectorAll(".search-item")) : [];
      }
      if (!items.length) return;
      const current = items.findIndex(item => item.classList.contains("highlighted"));
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const next = event.key === "ArrowDown" ? (current + 1) % items.length : (current <= 0 ? items.length - 1 : current - 1);
        items.forEach((item, i) => item.classList.toggle("highlighted", i === next));
        if (items[next].scrollIntoView) items[next].scrollIntoView({ block: "nearest" });
      } else if (event.key === "Enter") {
        event.preventDefault();
        items[current >= 0 ? current : 0].click();
      }
    }

    function clearSearch() {
      searchToken++;
      searchResults = null;
      const filters = document.getElementById("search-filters");
      if (filters) filters.hidden = true;
      const input = document.getElementById("global-search");
      if (input) input.value = "";
      const clearBtn = document.getElementById("search-clear-btn");
      if (clearBtn) clearBtn.style.display = "none";
      const dropdown = document.getElementById("search-results-dropdown");
      if (dropdown) dropdown.style.display = "none";
    }

    function canReplaceDraft(kind) {
      const dirty = kind === "player" ? state.playerDirtyFields.size > 0 : state[`${kind}Dirty`];
      return !dirty || typeof confirm !== "function" || confirm(`Discard unsaved ${kind} edits and change selection?`);
    }
    async function selectSearchedPlayer(playerId) {
      if (!canReplaceDraft("player")) return;
      clearSearch();
      showToast("Loading player profile...", "info");
      try {
        const res = await fetch("/api/context/select", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ player_id: playerId })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Selection failed");
        if (data.player || data.active_player) {
          const player = data.player || data.active_player;
          dirtyPlayerFields.clear();
          state.playerDirty = false;
          SaveBar.clear("player");
          displayPlayer(player);
          state.isPinned = true;
          updateSyncUI();
          switchTab("tab-player");
          rememberSearchSelection({ kind: "player", id: playerId, name: player?.name });
          showToast(`Loaded ${player?.name || "Player"}!`, "success");
        }
      } catch (e) {
        showToast("Error selecting player: " + e.message, "error");
      }
    }

    async function selectSearchedClub(clubId) {
      if (!canReplaceDraft("club")) return;
      clearSearch();
      showToast("Loading club...", "info");
      try {
        const res = await fetch("/api/context/select", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ club_id: clubId })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Selection failed");
        if (data.club || data.active_club) {
          const club = data.club || data.active_club;
          state.clubDirty = false;
          SaveBar.clear("club");
          displayClub(club);
          state.isPinned = true;
          updateSyncUI();
          switchTab("tab-club");
          rememberSearchSelection({ kind: "club", id: clubId, name: club?.club_name });
          showToast(`Loaded ${data.club?.club_name || "Club"}!`, "success");
        }
      } catch (e) {
        showToast("Error selecting club: " + e.message, "error");
      }
    }

    async function selectSearchedNation(nationId) {
      if (!canReplaceDraft("nation")) return;
      clearSearch();
      showToast("Loading nation details...", "info");
      try {
        const res = await fetch("/api/context/select", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ nation_id: nationId })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Selection failed");
        if (data.nation || data.active_nation) {
          const nation = data.nation || data.active_nation;
          state.nationDirty = false;
          SaveBar.clear("nation");
          displayNation(nation);
          state.isPinned = true;
          updateSyncUI();
          switchTab("tab-nation");
          rememberSearchSelection({ kind: "nation", id: nationId, name: nation.name });
          showToast(`Loaded ${nation.name}!`, "success");
        }
      } catch (e) {
        showToast("Error selecting nation: " + e.message, "error");
      }
    }

    async function selectSearchedCompetition(compId) {
      if (!canReplaceDraft("competition")) return;
      clearSearch();
      showToast("Loading competition details...", "info");
      try {
        const res = await fetch("/api/context/select", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ competition_id: compId })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Selection failed");
        if (data.competition || data.active_competition) {
          const competition = data.competition || data.active_competition;
          state.competitionDirty = false;
          SaveBar.clear("competition");
          displayCompetition(competition);
          state.isPinned = true;
          updateSyncUI();
          switchTab("tab-competition");
          rememberSearchSelection({ kind: "competition", id: compId, name: competition.name });
          showToast(`Loaded ${competition.name}!`, "success");
        }
      } catch (e) {
        showToast("Error selecting competition: " + e.message, "error");
      }
    }


    // Staff results open the Staff page and load the member by index + uid (staff.js selectStaff).
    async function selectSearchedStaff(staffId, staffUid) {
      clearSearch();
      switchTab("tab-staff");
      await selectStaff(staffId, staffUid);
      const picked = staffState.selected && staffState.selected.staff;
      if (picked && picked.index === staffId) {
        rememberSearchSelection({ kind: "staff", id: staffId, uid: staffUid, name: picked.name });
        Shell.setContext({
          section: "staff", kind: "staff", id: staffId, title: picked.name, subtitle: "Staff",
          open: () => { switchTab("tab-staff"); return selectStaff(staffId, staffUid); }
        });
      }
    }
