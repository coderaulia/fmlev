    // Instant Search Implementation
    let searchDebounce = null;
    let searchToken = 0;
    let searchFilter = "all";
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

    async function performSearch(query) {
      const dropdown = document.getElementById("search-results-dropdown");
      if (!dropdown) return;
      if (!query || query.trim().length === 0) {
        dropdown.style.display = "none";
        return;
      }

      const token = ++searchToken;
      try {
        // Staff live in a separate native index; a staff failure (not attached, no save) never hides the other results.
        const [res, staffResults] = await Promise.all([fetch(`/api/search?q=${encodeURIComponent(query)}`), searchStaffForDropdown(query)]);
        if (token !== searchToken) return;
        if (!res.ok) return;
        const data = await readJson(res);

        const players = searchFilter === "all" || searchFilter === "players" ? data.players || [] : [];
        const staff = searchFilter === "all" || searchFilter === "staff" ? staffResults : [];
        const clubs = searchFilter === "all" || searchFilter === "clubs" ? data.clubs || [] : [];
        const nations = searchFilter === "all" || searchFilter === "nations" ? data.nations || [] : [];
        const competitions = searchFilter === "all" || searchFilter === "competitions" ? data.competitions || [] : [];

        if (players.length === 0 && clubs.length === 0 && nations.length === 0 && competitions.length === 0 && staff.length === 0) {
          dropdown.textContent = data.discovery_error || "No players, clubs, nations, or competitions found. Open a player's Attributes screen and use Scan Loaded Players to retry indexing.";
          dropdown.style.display = "block";
          return;
        }

        let html = "";
        if (players.length > 0) {
          const totalPlayers = Number.isInteger(data.total_players) ? data.total_players : players.length;
          const countLabel = totalPlayers > players.length ? `${players.length} shown of ${totalPlayers}; refine your search` : String(totalPlayers);
          html += `<div class="search-section-title">Players (${countLabel})</div>`;
          html += players.map(p => {
            const isLive = isLivePlayer(p);
            const caStr = (p.current_ability || 0) > 0 ? p.current_ability : "—";
            const paStr = (p.potential_ability || 0) > 0 ? p.potential_ability : "—";
            const clubStr = p.club_name && p.club_name.trim() ? escapeHtml(p.club_name) : '<span style="color:var(--text-dim); font-style:italic;">Unknown (live)</span>';
            const natStr = p.nationality && p.nationality.trim() ? getCountryFlag(p.nationality) : '🌐';
            return `
            <div class="search-item" role="button" tabindex="0" onkeydown="if(event.key==='Enter'||event.key===' '){event.preventDefault();this.click()}" onclick="selectSearchedPlayer(${p.id})">
              <div class="search-item-info">
                <div class="search-item-avatar">${natStr}</div>
                <div>
                  <div class="search-item-name">
                    ${escapeHtml(p.name)}
                    ${isLive ? '<span class="live-badge-sm">LIVE</span>' : ''}
                  </div>
                  <div class="search-item-meta">${escapeHtml(p.position || "Unknown position")} • ${clubStr}</div>
                </div>
              </div>
              <span class="search-item-badge">CA ${caStr} / PA ${paStr}</span>
            </div>
            `;
          }).join("");
        }

        if (clubs.length > 0) {
          html += `<div class="search-section-title">Clubs (${clubs.length})</div>`;
          html += clubs.slice(0, 4).map(c => `
            <div class="search-item" role="button" tabindex="0" onkeydown="if(event.key==='Enter'||event.key===' '){event.preventDefault();this.click()}" onclick="selectSearchedClub(${c.club_id})">
              <div class="search-item-info">
                <div class="search-item-avatar">🏟️</div>
                <div>
                  <div class="search-item-name">${escapeHtml(c.club_name)}</div>
                  <div class="search-item-meta">${escapeHtml(c.league_name || "League")}</div>
                </div>
              </div>
              <span class="search-item-badge">Rep: ${(c.reputation > 0 ? c.reputation.toLocaleString() : "Unavailable")}</span>
            </div>
          `).join("");
        }

        if (nations.length > 0) {
          html += `<div class="search-section-title">Nations (${nations.length})</div>`;
          html += nations.slice(0, 4).map(n => `
            <div class="search-item" role="button" tabindex="0" onkeydown="if(event.key==='Enter'||event.key===' '){event.preventDefault();this.click()}" onclick="selectSearchedNation(${n.id})">
              <div class="search-item-info">
                <div class="search-item-avatar">${getCountryFlag(n.name)}</div>
                <div>
                  <div class="search-item-name">${escapeHtml(n.name)} <span style="font-size:10px; color:var(--text-dim);">(${escapeHtml(n.three_letter_code || "")})</span></div>
                  <div class="search-item-meta">${escapeHtml(n.continent || "Continent")} • Rep: ${(n.reputation > 0 ? n.reputation.toLocaleString() : "Unavailable")}</div>
                </div>
              </div>
              <span class="search-item-badge" style="background: rgba(6, 182, 212, 0.15); color: #22d3ee; border-color: rgba(6, 182, 212, 0.3);">Youth ${n.youth_rating || "—"}/200</span>
            </div>
          `).join("");
        }

        if (competitions.length > 0) {
          html += `<div class="search-section-title">Competitions (${competitions.length})</div>`;
          html += competitions.slice(0, 4).map(comp => `
            <div class="search-item" role="button" tabindex="0" onkeydown="if(event.key==='Enter'||event.key===' '){event.preventDefault();this.click()}" onclick="selectSearchedCompetition(${comp.id})">
              <div class="search-item-info">
                <div class="search-item-avatar">🏆</div>
                <div>
                  <div class="search-item-name">${escapeHtml(comp.name)} <span style="font-size:10px; color:var(--text-dim);">(${escapeHtml(comp.short_name || "")})</span></div>
                  <div class="search-item-meta">${escapeHtml(comp.nation_name || "International")} • Subs: ${comp.max_substitutes ?? "—"}/${comp.max_bench_size ?? "—"}</div>
                </div>
              </div>
              <span class="search-item-badge" style="background: rgba(168, 85, 247, 0.15); color: #c084fc; border-color: rgba(168, 85, 247, 0.3);">Rep: ${(comp.reputation > 0 ? comp.reputation.toLocaleString() : "Unavailable")}</span>
            </div>
          `).join("");
        }

        if (staff.length > 0) {
          html += `<div class="search-section-title">Staff (${staff.length})</div>`;
          html += staff.slice(0, 6).map(s => `
            <div class="search-item" role="button" tabindex="0" onkeydown="if(event.key==='Enter'||event.key===' '){event.preventDefault();this.click()}" onclick="selectSearchedStaff(${Number(s.index)}, ${Number(s.uid)})">
              <div class="search-item-info">
                <div class="search-item-avatar">ST</div>
                <div>
                  <div class="search-item-name">${escapeHtml(s.name)}</div>
                  <div class="search-item-meta">Staff #${Number(s.index)}</div>
                </div>
              </div>
              <span class="search-item-badge" style="background: rgba(90, 169, 240, 0.15); color: #8cc4f5; border-color: rgba(90, 169, 240, 0.3);">Staff</span>
            </div>
          `).join("");
        }

        dropdown.innerHTML = html;
        dropdown.style.display = "block";
      } catch (e) {
        console.error("Search error:", e);
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

    // Keyboard: Escape clears, Arrow keys move through results, Enter opens the highlighted (or first) result.
    function handleSearchKey(event) {
      if (event.key === "Escape") {
        clearSearch();
        return;
      }
      const dropdown = document.getElementById("search-results-dropdown");
      const items = dropdown && dropdown.querySelectorAll ? Array.from(dropdown.querySelectorAll(".search-item")) : [];
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
          dirtyPlayerFields.clear();
          state.playerDirty = false;
          SaveBar.clear("player");
          displayPlayer(data.player || data.active_player);
          state.isPinned = true;
          updateSyncUI();
          switchTab("tab-player");
          showToast(`Loaded ${(data.player || data.active_player)?.name || "Player"}!`, "success");
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
          state.clubDirty = false;
          SaveBar.clear("club");
          displayClub(data.club || data.active_club);
          state.isPinned = true;
          updateSyncUI();
          switchTab("tab-club");
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
          state.nationDirty = false;
          SaveBar.clear("nation");
          displayNation(data.nation || data.active_nation);
          state.isPinned = true;
          updateSyncUI();
          switchTab("tab-nation");
          showToast(`Loaded ${(data.nation || data.active_nation).name}!`, "success");
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
          state.competitionDirty = false;
          SaveBar.clear("competition");
          displayCompetition(data.competition || data.active_competition);
          state.isPinned = true;
          updateSyncUI();
          switchTab("tab-competition");
          showToast(`Loaded ${(data.competition || data.active_competition).name}!`, "success");
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
        Shell.setContext({
          section: "staff", kind: "staff", id: staffId, title: picked.name, subtitle: "Staff",
          open: () => { switchTab("tab-staff"); return selectStaff(staffId, staffUid); }
        });
      }
    }
