    // One editor for the active club and the selected player's native transfer deals.
    const transferDealLists = {};
    const transferDealBusy = new Set();

    function transferDealTarget(scope) {
      return scope === "player" ? state.activePlayer?.id : state.activeClub?.club_id;
    }

    // One-line clause summary; the clause kinds are only matched for the headline deal, so thresholds stay generic.
    function dealClauseSummary(clauses) {
      if (!clauses) return "Unavailable";
      if (clauses.unavailable_reason) return `Unavailable: ${clauses.unavailable_reason}`;
      if (!clauses.present) return "None";
      const parts = [];
      for (const i of clauses.instalments || []) {
        parts.push(`${formatMoney(i.amount_gbp, { compact: true })} over ${i.count ?? "?"} instalment${i.count === 1 ? "" : "s"} (${i.months} months)`);
      }
      for (const p of clauses.performance || []) parts.push(`${formatMoney(p.amount_gbp, { compact: true })} after ${p.threshold}`);
      return parts.length ? parts.join("; ") : "None";
    }

    function clubDealStatusText(deal) {
      return deal.status_label || `${deal.status_text} (unknown)`;
    }

    function renderClubDeals(scope = "club") {
      const body = document.getElementById(`${scope}-deals-rows`);
      if (!body) return;
      const list = transferDealLists[scope];
      const deals = state.connected && list?.id === transferDealTarget(scope) ? list.rows : [];
      const editing = !!document.getElementById(`${scope}-deals-experimental`)?.checked;
      if (!deals.length) { body.innerHTML = `<tr><td colspan="7" class="unavailable">No deals loaded</td></tr>`; return; }
      body.innerHTML = deals.map((row, i) => {
        const deal = row.deal;
        const player = deal.player;
        const canEdit = editing && !transferDealBusy.has(scope) && player.kind === "player" && player.id > 0 && player.uid > 0;
        const team = t => (t ? escapeHtml(t.club_name || t.kind) : "—");
        const join = deal.join_date;
        const editAttrs = canEdit ? "" : ` disabled title="${escapeHtml(player.kind === "player" ? "Enable experimental editing first" : (player.unavailable_reason || "Not editable"))}"`;
        return `<tr data-deal-index="${i}">
          <td>${escapeHtml(player.name || "Unknown")}<div class="unverified-note">${player.id !== null ? `id ${player.id}` : escapeHtml(player.kind)}</div></td>
          <td>${escapeHtml(row.direction)}</td>
          <td>${team(deal.selling_team)} → ${team(deal.buying_team)}<div class="unverified-note">${escapeHtml(deal.deal_type_label || `type ${deal.deal_type}`)}</div></td>
          <td>${escapeHtml(clubDealStatusText(deal))}</td>
          <td class="num">${escapeHtml(formatMoney(deal.fee_gbp))}<div class="unverified-note">${escapeHtml(dealClauseSummary(deal.clauses))}</div></td>
          <td>${join ? escapeHtml(join.iso) : "Unavailable"}<div class="unverified-note">Scheduling edit unavailable</div></td>
          <td>
            <input id="${scope}-deal-fee-${i}" type="number" min="0" max="2147483647" step="1" class="form-control" style="width: 120px;" value="${deal.fee_gbp}" aria-label="Deal fee, raw GBP"${editAttrs}>
            <button class="btn btn-sm btn-primary" onclick="applyClubDeal(${i}, '${scope}')"${editAttrs}>Apply</button>
          </td>
        </tr>`;
      }).join("");
    }

    function updateClubDealControls(scope = "club") { renderClubDeals(scope); }

    function clearPlayerDeals() {
      delete transferDealLists.player;
      renderClubDeals("player");
      const status = document.getElementById("player-deals-status");
      if (status) status.innerText = "Read the selected player's deals.";
    }

    async function loadClubDeals(scope = "club") {
      const status = document.getElementById(`${scope}-deals-status`);
      const setStatus = text => { if (status) status.innerText = text; };
      const id = transferDealTarget(scope);
      if (!state.connected) { setStatus("Attach to FM26 first."); return null; }
      if (!id) { setStatus(`Select a ${scope} first.`); return null; }
      const list = { id, rows: [] };
      transferDealLists[scope] = list;
      renderClubDeals(scope);
      const current = () => state.connected && transferDealTarget(scope) === id && transferDealLists[scope] === list;
      setStatus("Reading transfer deals...");
      try {
        const res = await fetch(`/api/deals/${scope}`, {
          method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ [`${scope}_id`]: id })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Deal read failed");
        if (!current()) return null;
        list.rows = scope === "player" ? data.deals.map(deal => ({ deal, direction: "—" })) : data.deals;
        setStatus(`${list.rows.length} deal${list.rows.length === 1 ? "" : "s"} for this ${scope}.`);
        renderClubDeals(scope);
        return data;
      } catch (error) {
        if (current()) setStatus(`Unavailable: ${error.message}`);
        return null;
      }
    }

    async function applyClubDeal(index, scope = "club") {
      const list = transferDealLists[scope];
      if (!state.connected || !list || list.id !== transferDealTarget(scope) || transferDealBusy.has(scope)) return false;
      const deal = list.rows[index]?.deal;
      if (!deal) return false;
      if (!document.getElementById(`${scope}-deals-experimental`)?.checked) {
        showToast("Enable experimental fee editing first.", "warning");
        return false;
      }
      if (deal.player.kind !== "player" || !(deal.player.id > 0 && deal.player.uid > 0)) {
        showToast(deal.player.unavailable_reason || "Player identity is unavailable.", "warning");
        return false;
      }
      const text = String(document.getElementById(`${scope}-deal-fee-${index}`)?.value ?? "").trim();
      const fee = Number(text);
      if (!text || fee === deal.fee_gbp) { showToast("Nothing changed in this deal.", "info"); return false; }
      if (!Number.isInteger(fee) || fee < 0 || fee > 2147483647) {
        showToast("The fee must be a whole raw-GBP amount from 0 to 2,147,483,647.", "warning");
        return false;
      }
      if (typeof confirm === "function" && !confirm(`Change ${deal.player.name || "this player's"} headline fee to ${fee.toLocaleString()} GBP? Use a throwaway save.`)) return false;
      transferDealBusy.add(scope);
      renderClubDeals(scope);
      try {
        const res = await fetch("/api/deals/update", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ deal_address: deal.address, player_id: deal.player.id, player_uid: deal.player.uid,
            expected_fee_gbp: deal.fee_gbp, fee_gbp: fee, experimental: true })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Deal update failed");
        showToast(data.message || "Headline fee saved.", "success");
        if (transferDealLists[scope] === list && transferDealTarget(scope) === list.id) await loadClubDeals(scope);
        return true;
      } catch (error) {
        showToast(error.message, "error");
        return false;
      } finally {
        transferDealBusy.delete(scope);
        renderClubDeals(scope);
      }
    }
