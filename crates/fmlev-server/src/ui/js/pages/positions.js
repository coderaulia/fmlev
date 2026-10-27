    // Position & Foot Management
    function getPositionTierClass(val) {
      if (val >= 20) return "pos-natural";
      if (val >= 15) return "pos-accomplished";
      if (val >= 10) return "pos-competent";
      if (val >= 6) return "pos-unconvincing";
      return "pos-ineffectual";
    }

    function getPositionTierLabel(val) {
      if (val >= 20) return "Natural";
      if (val >= 15) return "Accomplished";
      if (val >= 10) return "Competent";
      if (val >= 6) return "Unconvincing";
      return "Ineffectual";
    }

    function getTierColor(val) {
      if (val >= 20) return "#10b981";
      if (val >= 15) return "#34d399";
      if (val >= 10) return "#f59e0b";
      if (val >= 6) return "#ea580c";
      return "#94a3b8";
    }

    function getFootTierDesc(val) {
      if (!val) return "Unknown (not exposed)";
      if (val >= 20) return "Very Strong";
      if (val >= 16) return "Strong";
      if (val >= 12) return "Fairly Strong";
      if (val >= 9) return "Reasonable";
      if (val >= 6) return "Weak";
      return "Very Weak";
    }

    function getFootSummaryString(left, right) {
      if (!left || !right) return "Foot ratings unavailable";
      if (left >= 18 && right >= 18) return "Either Foot";
      if (left >= 15 && right >= 15) return "Both Feet (Strong)";
      if (left >= 15 && right < 10) return "Left Only";
      if (right >= 15 && left < 10) return "Right Only";
      if (left >= right) return `Left Foot (${getFootTierDesc(left)})`;
      return `Right Foot (${getFootTierDesc(right)})`;
    }

    function computePositionString(pos) {
      if (!pos) return "Unknown";
      if (pos.gk >= 15) return "GK";
      const tags = [];
      const dSides = formatSides(pos.dr >= 15, pos.dl >= 15, pos.dc >= 15);
      if (dSides) tags.push(`D (${dSides})`);
      const wbSides = formatSides(pos.wbr >= 15, pos.wbl >= 15, false);
      if (wbSides) tags.push(`WB (${wbSides})`);
      if (pos.dm >= 15) tags.push("DM");
      const mSides = formatSides(pos.mr >= 15, pos.ml >= 15, pos.mc >= 15);
      if (mSides) tags.push(`M (${mSides})`);
      const amSides = formatSides(pos.amr >= 15, pos.aml >= 15, pos.amc >= 15);
      if (amSides) tags.push(`AM (${amSides})`);
      if (pos.st >= 15) tags.push("ST (C)");
      return tags.length > 0 ? tags.join(", ") : "Unknown";
    }

    function formatSides(r, l, c) {
      let s = "";
      if (r) s += "R";
      if (l) s += "L";
      if (c) s += "C";
      return s;
    }

    function renderPositionsTab() {
      if (!state.activePlayer) return;
      const player = state.activePlayer;
      if (!player.positions) {
        player.positions = {
          gk: 0, dl: 0, dc: 0, dr: 0,
          wbl: 0, wbr: 0, dm: 0,
          ml: 0, mc: 0, mr: 0,
          aml: 0, amc: 0, amr: 0, st: 0,
          left_foot: 0, right_foot: 0
        };
      }
      const pos = player.positions;

      // Header labels
      const posPlayerName = document.getElementById("pos-player-name");
      const posClubName = document.getElementById("pos-club-name");
      const posSummaryBadge = document.getElementById("pos-summary-badge");
      const posFootSummary = document.getElementById("pos-foot-summary");

      if (posPlayerName) posPlayerName.innerText = `${player.name || "Unknown Player"} — Positions & Roles`;
      const clubStr = (player.club_name || "").trim();
      if (posClubName) posClubName.innerText = clubStr || "Unknown (live)";

      const posStr = (dirtyPlayerFields.has("positions") || !player.position)
        ? computePositionString(pos)
        : player.position;
      player.position = posStr;
      if (posSummaryBadge) posSummaryBadge.innerText = posStr;
      const playerPosPill = document.getElementById("player-pos");
      if (playerPosPill && !dirtyPlayerFields.has("position")) playerPosPill.innerText = posStr;

      const footStr = getFootSummaryString(pos.left_foot ?? 0, pos.right_foot ?? 0);
      if (posFootSummary) posFootSummary.innerText = footStr;

      // Update Pitch Nodes
      for (const [key, role] of positionDefinitions) {
        const val = pos[key] ?? 0;
        const nodeEl = document.getElementById("pitch-node-" + key);
        const valEl = document.getElementById("pitch-val-" + key);
        if (nodeEl) {
          nodeEl.className = `pitch-node ${getPositionTierClass(val)}`;
          nodeEl.setAttribute?.("aria-disabled", String(val <= 0 || !playerFieldWritable("positions")));
        }
        if (valEl) {
          valEl.innerText = val > 0 ? val : "—";
        }
      }

      // Update Foot Sliders
      const leftVal = pos.left_foot ?? 0;
      const rightVal = pos.right_foot ?? 0;
      const sLeft = document.getElementById("slider-foot-left");
      const sRight = document.getElementById("slider-foot-right");
      const vLeft = document.getElementById("foot-val-left");
      const vRight = document.getElementById("foot-val-right");
      const dLeft = document.getElementById("foot-desc-left");
      const dRight = document.getElementById("foot-desc-right");

      if (sLeft) { sLeft.value = leftVal; sLeft.disabled = leftVal <= 0 || !playerFieldWritable("positions"); }
      if (sRight) { sRight.value = rightVal; sRight.disabled = rightVal <= 0 || !playerFieldWritable("positions"); }
      if (vLeft) {
        vLeft.innerText = leftVal > 0 ? leftVal : "—";
        vLeft.style.color = getTierColor(leftVal);
      }
      if (vRight) {
        vRight.innerText = rightVal > 0 ? rightVal : "—";
        vRight.style.color = getTierColor(rightVal);
      }
      if (dLeft) dLeft.innerText = getFootTierDesc(leftVal);
      if (dRight) dRight.innerText = getFootTierDesc(rightVal);

      // Render 14-position table matrix
      const grid = document.getElementById("pos-grid-container");
      if (grid) {
        grid.innerHTML = positionDefinitions.map(([key, role, fullName]) => {
          const val = pos[key] ?? 0;
          const tierClass = getPositionTierClass(val);
          const tierLabel = val > 0 ? getPositionTierLabel(val) : "Unknown (not exposed)";
          return `
            <div class="pos-item-card">
              <div class="pos-item-left">
                <span class="pos-item-name">${role} <span style="font-size: 10px; font-weight: 500; color: var(--text-muted);">(${key.toUpperCase()})</span></span>
                <span class="pos-item-desc" style="color: ${getTierColor(val)};">${tierLabel}</span>
              </div>
              <div class="pos-item-controls">
                <button class="btn btn-secondary" style="padding: 2px 7px; font-size: 12px; font-weight: 800;" onclick="stepPosition('${key}', -1)" title="Decrease rating">-</button>
                <div class="attr-val-box ${tierClass}" style="cursor: pointer; min-width: 28px; height: 26px; font-size: 12px; line-height: 24px; text-align: center;" onclick="cyclePosition('${key}')" title="Click to cycle tier (20, 15, 10, 1)">
                  ${val > 0 ? val : "—"}
                </div>
                <button class="btn btn-secondary" style="padding: 2px 7px; font-size: 12px; font-weight: 800;" onclick="stepPosition('${key}', 1)" title="Increase rating">+</button>
              </div>
            </div>
          `;
        }).join("");
      }
    }

    function cyclePosition(key) {
      if (!state.activePlayer) return;
      if (!(state.activePlayer.positions?.[key] > 0) || !playerFieldWritable("positions")) return;
      state.playerDirtyFields.add("positions");
      markUnsaved();
      if (!state.activePlayer.positions) {
        state.activePlayer.positions = {
          gk: 0, dl: 0, dc: 0, dr: 0,
          wbl: 0, wbr: 0, dm: 0,
          ml: 0, mc: 0, mr: 0,
          aml: 0, amc: 0, amr: 0, st: 0,
          left_foot: 0, right_foot: 0
        };
      }
      const cur = state.activePlayer.positions[key] || 1;
      let next;
      if (cur >= 20) next = 15;
      else if (cur >= 15) next = 10;
      else if (cur >= 10) next = 1;
      else next = 20;

      state.activePlayer.positions[key] = next;
      renderPositionsTab();
    }

    function stepPosition(key, delta) {
      if (!state.activePlayer) return;
      if (!(state.activePlayer.positions?.[key] > 0) || !playerFieldWritable("positions")) return;
      state.playerDirtyFields.add("positions");
      markUnsaved();
      if (!state.activePlayer.positions) {
        state.activePlayer.positions = {
          gk: 0, dl: 0, dc: 0, dr: 0,
          wbl: 0, wbr: 0, dm: 0,
          ml: 0, mc: 0, mr: 0,
          aml: 0, amc: 0, amr: 0, st: 0,
          left_foot: 0, right_foot: 0
        };
      }
      const cur = state.activePlayer.positions[key] || 1;
      const next = Math.max(1, Math.min(20, cur + delta));
      state.activePlayer.positions[key] = next;
      renderPositionsTab();
    }

    function handleFootSlider(foot, val) {
      if (!state.activePlayer) return;
      if (!(state.activePlayer.positions?.[foot + "_foot"] > 0) || !playerFieldWritable("positions")) return;
      state.playerDirtyFields.add("positions");
      markUnsaved();
      if (!state.activePlayer.positions) {
        state.activePlayer.positions = {
          gk: 0, dl: 0, dc: 0, dr: 0,
          wbl: 0, wbr: 0, dm: 0,
          ml: 0, mc: 0, mr: 0,
          aml: 0, amc: 0, amr: 0, st: 0,
          left_foot: 0, right_foot: 0
        };
      }
      const num = Math.max(1, Math.min(20, parseInt(val) || 1));
      if (foot === "left") {
        state.activePlayer.positions.left_foot = num;
      } else {
        state.activePlayer.positions.right_foot = num;
      }
      renderPositionsTab();
    }

    async function applyPositionPreset(preset) {
      return queuePreset("player", preset);
    }

    async function savePositionsToGame() {
      if (!state.activePlayer) return;
      return applyPlayerChanges();
    }

