    // Instant Search Implementation
    let searchDebounce = null;
    function handleSearchInput(query) {
      const clearBtn = document.getElementById("search-clear-btn");
      clearBtn.style.display = query.length > 0 ? "block" : "none";

      clearTimeout(searchDebounce);
      searchDebounce = setTimeout(() => {
        performSearch(query);
      }, 150);
    }

    async function performSearch(query) {
      const dropdown = document.getElementById("search-results-dropdown");
      if (!query || query.trim().length === 0) {
        dropdown.style.display = "none";
        return;
      }

      try {
        const res = await fetch(`/api/search?q=${encodeURIComponent(query)}`);
        if (!res.ok) return;
        const data = await readJson(res);

        const players = data.players || [];
        const clubs = data.clubs || [];
        const nations = data.nations || [];
        const competitions = data.competitions || [];

        if (players.length === 0 && clubs.length === 0 && nations.length === 0 && competitions.length === 0) {
          dropdown.textContent = data.discovery_error || "No players, clubs, nations, or competitions found. Open a player's Attributes screen and use Scan Loaded Players to retry indexing.";
          dropdown.style.display = "block";
          return;
        }

        let html = "";
        if (players.length > 0) {
          const totalPlayers = Number.isInteger(data.total_players) ? data.total_players : players.length;
          const countLabel = totalPlayers > players.length ? `${players.length} shown of ${totalPlayers}; refine your search` : String(totalPlayers);
          html += `<div class="search-section-title">⚽ Players (${countLabel})</div>`;
          html += players.map(p => {
            const isLive = isLivePlayer(p);
            const caStr = (p.current_ability || 0) > 0 ? p.current_ability : "—";
            const paStr = (p.potential_ability || 0) > 0 ? p.potential_ability : "—";
            const clubStr = p.club_name && p.club_name.trim() ? escapeHtml(p.club_name) : '<span style="color:var(--text-dim); font-style:italic;">Unknown (live)</span>';
            const natStr = p.nationality && p.nationality.trim() ? getCountryFlag(p.nationality) : '🌐';
            return `
            <div class="search-item" onclick="selectSearchedPlayer(${p.id})">
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
          html += `<div class="search-section-title">🏟️ Clubs (${clubs.length})</div>`;
          html += clubs.slice(0, 4).map(c => `
            <div class="search-item" onclick="selectSearchedClub(${c.club_id})">
              <div class="search-item-info">
                <div class="search-item-avatar">🏟️</div>
                <div>
                  <div class="search-item-name">${escapeHtml(c.club_name)}</div>
                  <div class="search-item-meta">${escapeHtml(c.league_name || "League")}</div>
                </div>
              </div>
              <span class="search-item-badge">Rep: ${(c.reputation || 8000).toLocaleString()}</span>
            </div>
          `).join("");
        }

        if (nations.length > 0) {
          html += `<div class="search-section-title">🌍 Nations (${nations.length})</div>`;
          html += nations.slice(0, 4).map(n => `
            <div class="search-item" onclick="selectSearchedNation(${n.id})">
              <div class="search-item-info">
                <div class="search-item-avatar">${getCountryFlag(n.name)}</div>
                <div>
                  <div class="search-item-name">${escapeHtml(n.name)} <span style="font-size:10px; color:var(--text-dim);">(${escapeHtml(n.three_letter_code || "")})</span></div>
                  <div class="search-item-meta">${escapeHtml(n.continent || "Continent")} • Rep: ${(n.reputation || 7000).toLocaleString()}</div>
                </div>
              </div>
              <span class="search-item-badge" style="background: rgba(6, 182, 212, 0.15); color: #22d3ee; border-color: rgba(6, 182, 212, 0.3);">Youth ${n.youth_rating || 100}/200</span>
            </div>
          `).join("");
        }

        if (competitions.length > 0) {
          html += `<div class="search-section-title">🏆 Competitions (${competitions.length})</div>`;
          html += competitions.slice(0, 4).map(comp => `
            <div class="search-item" onclick="selectSearchedCompetition(${comp.id})">
              <div class="search-item-info">
                <div class="search-item-avatar">🏆</div>
                <div>
                  <div class="search-item-name">${escapeHtml(comp.name)} <span style="font-size:10px; color:var(--text-dim);">(${escapeHtml(comp.short_name || "")})</span></div>
                  <div class="search-item-meta">${escapeHtml(comp.nation_name || "International")} • Subs: ${comp.max_substitutes || 5}/${comp.max_bench_size || 9}</div>
                </div>
              </div>
              <span class="search-item-badge" style="background: rgba(168, 85, 247, 0.15); color: #c084fc; border-color: rgba(168, 85, 247, 0.3);">Rep: ${(comp.reputation || 8000).toLocaleString()}</span>
            </div>
          `).join("");
        }

        dropdown.innerHTML = html;
        dropdown.style.display = "block";
      } catch (e) {
        console.error("Search error:", e);
      }
    }

    function handleSearchKey(event) {
      if (event.key === "Escape") {
        clearSearch();
      }
    }

    function clearSearch() {
      const input = document.getElementById("global-search");
      input.value = "";
      document.getElementById("search-clear-btn").style.display = "none";
      document.getElementById("search-results-dropdown").style.display = "none";
    }

    async function selectSearchedPlayer(playerId) {
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

