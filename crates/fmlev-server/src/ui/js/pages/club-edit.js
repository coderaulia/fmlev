    // Club & Squad Management
    function markClubUnsaved() {
      state.clubDirty = true;
    }

    function autoSaveClub() {
      markClubUnsaved();
    }

    function promptEditClubFac(sliderId, title) {
      const slider = document.getElementById(sliderId);
      if (!slider || slider.disabled) return;
      const current = slider.value || 10;
      const input = prompt(`Enter ${title} rating (1 - 20):`, current);
      if (input !== null) {
        const val = Math.max(1, Math.min(20, parseInt(input) || current));
        slider.value = val;
        updateFacVal(`${sliderId}-disp`, val);
        markClubUnsaved();
      }
    }

    async function writeClubChanges() {
      if (!state.activeClub) return false;
      state.clubDirty = true;

      if (!state.connected) {
        if (document.getElementById("club-balance")) state.activeClub.balance = readMoneyInput(document.getElementById("club-balance"), 0);
        if (document.getElementById("club-transfer-budget")) state.activeClub.transfer_budget = readMoneyInput(document.getElementById("club-transfer-budget"), 0);
        if (document.getElementById("club-wage-budget")) state.activeClub.wage_budget = readMoneyInput(document.getElementById("club-wage-budget"), 0);
        if (document.getElementById("club-max-wage")) state.activeClub.max_wage_allowance = readMoneyInput(document.getElementById("club-max-wage"), 600000);
        if (document.getElementById("club-stadium-name")) state.activeClub.stadium_name = (document.getElementById("club-stadium-name").value || "").trim();
        if (document.getElementById("club-stadium-cap")) state.activeClub.stadium_capacity = numberInput(document.getElementById("club-stadium-cap").value, 45000);
        if (document.getElementById("club-stadium-exp")) state.activeClub.stadium_expansion_cap = numberInput(document.getElementById("club-stadium-exp").value, 60000);
        if (document.getElementById("club-board-patience")) state.activeClub.board_patience = numberInput(document.getElementById("club-board-patience").value, 15);
        if (document.getElementById("club-fan-confidence")) state.activeClub.fan_confidence = numberInput(document.getElementById("club-fan-confidence").value, 85);
        if (document.getElementById("club-board-confidence")) state.activeClub.board_confidence = numberInput(document.getElementById("club-board-confidence").value, 88);
        if (document.getElementById("club-transfer-retention")) state.activeClub.transfer_retention_pct = numberInput(document.getElementById("club-transfer-retention").value, 100);
      }

      // 6 verified native facilities + sugar daddy
      if (document.getElementById("fac-training")) state.activeClub.training_facilities = numberInput(document.getElementById("fac-training").value, 20);
      if (document.getElementById("fac-youth")) state.activeClub.youth_facilities = numberInput(document.getElementById("fac-youth").value, 20);
      if (document.getElementById("fac-recruitment")) state.activeClub.youth_recruitment = numberInput(document.getElementById("fac-recruitment").value, 20);
      if (document.getElementById("fac-coaching")) state.activeClub.junior_coaching = numberInput(document.getElementById("fac-coaching").value, 20);
      if (document.getElementById("fac-data-analysis")) state.activeClub.data_analysis_facilities = numberInput(document.getElementById("fac-data-analysis").value, 18);
      if (document.getElementById("fac-corporate")) state.activeClub.corporate_facilities = numberInput(document.getElementById("fac-corporate").value, 19);
      if (document.getElementById("club-sugar-daddy")) state.activeClub.sugar_daddy_type = document.getElementById("club-sugar-daddy").value || "None";

      const submittedEntity = JSON.stringify(state.activeClub);
      try {
        const res = await fetch("/api/club/update", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: submittedEntity
        });
        const data = await readJson(res);
        if (res.ok) {
          if (JSON.stringify(state.activeClub) === submittedEntity) {
            state.clubDirty = false;
          }
          nativeClubSyncedId = null;
          showToast(`Saved ${state.activeClub.club_name}: changed facility ratings, balance and transfer budget are written natively; other fields are not live-writable.`, "success");
          return true;
        } else {
          showToast(data.message || "Failed to save club", "error");
        }
      } catch (e) {
        showToast("Club save error: " + e.message, "error");
      }
      return false;
    }

    function stepClubFac(sliderId, delta) {
      const slider = document.getElementById(sliderId);
      if (!slider) return;
      const current = parseInt(slider.value) || 10;
      const next = Math.max(1, Math.min(20, current + delta));
      slider.value = next;
      updateFacVal(`${sliderId}-disp`, next);
      autoSaveClub();
    }

    function stepPatience(delta) {
      const slider = document.getElementById("club-board-patience");
      if (!slider) return;
      const current = parseInt(slider.value) || 10;
      const next = Math.max(1, Math.min(20, current + delta));
      slider.value = next;
      updateDisp("disp-patience", next);
      autoSaveClub();
    }

    function handleSugarDaddySelect(val) {
      updateSugarDaddyBadge(val);
      if (state.activeClub) {
        state.activeClub.sugar_daddy_type = val;
        if (val === "Foreground") {
          const balEl = document.getElementById("club-balance");
          const tbEl = document.getElementById("club-transfer-budget");
          if (balEl) setMoneyInput(balEl, Math.max(500000000, readMoneyInput(balEl, 0)));
          if (tbEl) setMoneyInput(tbEl, Math.max(250000000, readMoneyInput(tbEl, 0)));
        }
      }
      autoSaveClub();
    }

    function updateSugarDaddyBadge(val) {
      const badge = document.getElementById("sugar-daddy-badge");
      const desc = document.getElementById("sugar-daddy-desc");
      if (!badge || !desc) return;
      badge.innerText = val;
      if (val === "Foreground") {
        badge.className = "pill-badge badge-pos tier-wonderkid";
        desc.innerText = "Foreground Benefactor: Continuous, limitless wealth injections to dominate world football.";
      } else if (val === "Underwriter") {
        badge.className = "pill-badge badge-club";
        desc.innerText = "Underwriter: Subsidizes operating deficits and guarantees debt coverage unconditionally.";
      } else if (val === "FrontEnd") {
        badge.className = "pill-badge badge-morale";
        desc.innerText = "Front-End Investor: Provides substantial initial cash injections to elevate team standing.";
      } else {
        badge.className = "pill-badge badge-nat";
        desc.innerText = "Standard club economic operation without benefactor financing.";
      }
    }

    function updateDisp(id, val) {
      const el = document.getElementById(id);
      if (el) {
        el.innerText = val;
        el.className = `facility-val ${getTierClass(parseInt(val) || 10)}`;
      }
    }

    function updateDispPercent(id, val) {
      const el = document.getElementById(id);
      if (el) el.innerText = `${val}%`;
    }

    function renderAffiliatesList(affiliates) {
      const container = document.getElementById("affiliates-list");
      const countBadge = document.getElementById("affiliates-count-badge");
      if (!container) return;
      if (countBadge) countBadge.innerText = `${affiliates.length} Active`;

      if (affiliates.length === 0) {
        container.innerHTML = `<div style="color: var(--text-muted); font-size: 12px; padding: 14px; text-align: center;">No affiliated clubs linked. Add one below!</div>`;
        return;
      }

      container.innerHTML = affiliates.map(a => `
        <div class="affiliate-card">
          <div class="affiliate-card-header">
            <div class="affiliate-card-title">
              <span>🤝</span>
              <span>${escapeHtml(a.affiliate_name || "Affiliate")}</span>
              <span style="font-size: 10px; color: var(--text-dim);">#${a.affiliate_club_id}</span>
            </div>
            <button class="btn-remove-affiliate" onclick="removeAffiliate(${a.affiliate_club_id})" title="Terminate affiliate link">
              ✕
            </button>
          </div>
          <div class="affiliate-badges">
            ${a.first_option_to_buy ? '<span class="affiliate-badge active">🏷️ First Option</span>' : ''}
            ${a.send_players_on_loan ? '<span class="affiliate-badge active">✈️ Loans</span>' : ''}
            ${a.shared_scouting ? '<span class="affiliate-badge active">🔍 Shared Scouting</span>' : ''}
            ${a.work_permit_haven ? '<span class="affiliate-badge haven">🛡️ Work Permit Haven</span>' : ''}
            <span class="affiliate-badge fee">💵 ${formatMoney(a.annual_fee || 0)}/yr</span>
          </div>
        </div>
      `).join("");
    }

    async function submitNewAffiliate() {
      if (!state.activeClub) {
        showToast("No active club loaded", "warning");
        return;
      }
      const id = numberInput(document.getElementById("new-aff-id").value, 0);
      const name = (document.getElementById("new-aff-name").value || "").trim();
      const fee = readMoneyInput(document.getElementById("new-aff-fee"), 0);
      if (!name) {
        showToast("Please enter an affiliate club name", "warning");
        return;
      }
      if (!Number.isInteger(id) || id <= 0) {
        showToast("Enter the affiliate club's numeric club ID (greater than 0)", "warning");
        return;
      }

      const affiliateObj = {
        affiliate_club_id: id,
        affiliate_name: name,
        is_parent: false,
        first_option_to_buy: document.getElementById("new-aff-option").checked,
        send_players_on_loan: document.getElementById("new-aff-loans").checked,
        shared_scouting: document.getElementById("new-aff-scout").checked,
        annual_fee: fee,
        work_permit_haven: document.getElementById("new-aff-haven").checked
      };

      return queuePendingAction("club", "/api/club/affiliate/add", {
        club_id: state.activeClub.club_id, affiliate: affiliateObj
      }, `Add affiliate ${name}`);
    }

    async function removeAffiliate(affiliateId) {
      if (!state.activeClub) return false;
      return queuePendingAction("club", "/api/club/affiliate/remove", {
        club_id: state.activeClub.club_id, affiliate_id: affiliateId
      }, `Remove affiliate ${affiliateId}`);
    }

    async function applyClubPreset(preset) {
      return queuePreset("club", preset);
    }

