    // ---- Club > Transfer activity (FULL_TRANSFER_OFFER deal objects). Reads are native and read-only; the fee and
    // join-date edits are EXPERIMENTAL and unverified in game, behind an unchecked checkbox. Nothing here is autosaved:
    // every edit has its own Apply button and a confirmation.

    // "YYYY-MM-DD" -> { year, day-of-year }, or null for anything that is not a real calendar date.
    function dealDateParts(iso) {
      const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(iso || ""));
      if (!match) return null;
      const [year, month, day] = [Number(match[1]), Number(match[2]), Number(match[3])];
      const date = new Date(Date.UTC(year, month - 1, day));
      if (date.getUTCFullYear() !== year || date.getUTCMonth() !== month - 1 || date.getUTCDate() !== day) return null;
      return { year, day: Math.round((date - new Date(Date.UTC(year, 0, 1))) / 86400000) + 1 };
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

    function renderClubDeals() {
      const body = document.getElementById("club-deals-rows");
      if (!body) return;
      const deals = state.clubDeals || [];
      const editing = !!document.getElementById("club-deals-experimental")?.checked;
      if (!deals.length) { body.innerHTML = `<tr><td colspan="7" class="unavailable">No deals loaded</td></tr>`; return; }
      body.innerHTML = deals.map((row, i) => {
        const deal = row.deal;
        const player = deal.player;
        const canEdit = editing && player.kind === "player" && player.id !== null;
        const team = t => (t ? escapeHtml(t.club_name || t.kind) : "—");
        const join = deal.join_date;
        const dateAttrs = canEdit && join ? "" : ` disabled title="${escapeHtml(join ? "Enable experimental editing first" : "This deal has no join date set; it is not overwritten")}"`;
        const editAttrs = canEdit ? "" : ` disabled title="${escapeHtml(player.kind === "player" ? "Enable experimental editing first" : (player.unavailable_reason || "Not editable"))}"`;
        return `<tr data-deal-index="${i}">
          <td>${escapeHtml(player.name || "Unknown")}<div class="unverified-note">${player.id !== null ? `id ${player.id}` : escapeHtml(player.kind)}</div></td>
          <td>${escapeHtml(row.direction)}</td>
          <td>${team(deal.selling_team)} → ${team(deal.buying_team)}<div class="unverified-note">${escapeHtml(deal.deal_type_label || `type ${deal.deal_type}`)}</div></td>
          <td>${escapeHtml(clubDealStatusText(deal))}</td>
          <td class="num">${escapeHtml(formatMoney(deal.fee_gbp))}<div class="unverified-note">${escapeHtml(dealClauseSummary(deal.clauses))}</div></td>
          <td>${join ? escapeHtml(join.iso) : "Unavailable"}</td>
          <td>
            <input id="club-deal-fee-${i}" type="number" min="0" max="2147483647" step="1" class="form-control" style="width: 120px;" value="${deal.fee_gbp}" aria-label="Deal fee, raw GBP"${editAttrs}>
            <input id="club-deal-date-${i}" type="date" class="form-control" style="width: 140px;" value="${join ? escapeHtml(join.iso) : ""}" aria-label="Deal join date"${dateAttrs}>
            <button class="btn btn-sm btn-primary" onclick="applyClubDeal(${i})"${editAttrs}>Apply</button>
          </td>
        </tr>`;
      }).join("");
    }

    function updateClubDealControls() { renderClubDeals(); }

    async function loadClubDeals() {
      const status = document.getElementById("club-deals-status");
      const setStatus = text => { if (status) status.innerText = text; };
      const clubId = state.activeClub?.club_id;
      if (!state.connected) { setStatus("Attach to FM26 first."); return null; }
      if (!clubId) { setStatus("Select a club first."); return null; }
      setStatus("Reading deals from the game (about a second)...");
      try {
        const res = await fetch("/api/deals/club", {
          method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ club_id: clubId })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Deal read failed");
        if (state.activeClub?.club_id !== clubId) return null;
        state.clubDeals = data.deals || [];
        setStatus(`${state.clubDeals.length} deal${state.clubDeals.length === 1 ? "" : "s"} for this club (read ${data.scanned?.candidates ?? "?"} deal objects).`);
        renderClubDeals();
        return data;
      } catch (error) {
        state.clubDeals = [];
        renderClubDeals();
        setStatus(`Unavailable: ${error.message}`);
        return null;
      }
    }

    // Sends only the fields that differ from what was loaded. Fee is raw GBP; the date becomes year + day of year.
    async function applyClubDeal(index) {
      const row = (state.clubDeals || [])[index];
      if (!row) return false;
      if (!document.getElementById("club-deals-experimental")?.checked) { showToast("Enable experimental deal editing first.", "warning"); return false; }
      const deal = row.deal;
      if (deal.player.id === null) { showToast(deal.player.unavailable_reason || "This deal has no player id.", "warning"); return false; }
      const body = { deal_address: deal.address, player_id: deal.player.id, experimental: true };
      const changes = [];
      const feeText = String(document.getElementById(`club-deal-fee-${index}`)?.value ?? "").trim();
      if (feeText !== "" && feeText !== String(deal.fee_gbp)) {
        const fee = Number(feeText);
        if (!Number.isInteger(fee) || fee < 0 || fee > 2147483647) { showToast("The fee must be a whole raw-GBP amount from 0 to 2,147,483,647.", "warning"); return false; }
        body.fee_gbp = fee;
        changes.push(`fee ${deal.fee_gbp.toLocaleString()} -> ${fee.toLocaleString()}`);
      }
      const dateText = String(document.getElementById(`club-deal-date-${index}`)?.value ?? "");
      if (deal.join_date && dateText !== "" && dateText !== deal.join_date.iso) {
        const parts = dealDateParts(dateText);
        if (!parts) { showToast("Enter a valid join date.", "warning"); return false; }
        body.join_date = parts;
        changes.push(`join date ${deal.join_date.iso} -> ${dateText}`);
      }
      if (!changes.length) { showToast("Nothing changed in this deal.", "info"); return false; }
      const question = `Change the deal for ${deal.player.name || "this player"}: ${changes.join(", ")}? Experimental and unverified in game; the game decides what happens next. Keep a backup save.`;
      if (typeof confirm === "function" && !confirm(question)) return false;
      try {
        const res = await fetch("/api/deals/update", {
          method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body)
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Deal update failed");
        showToast(data.message || "Deal updated.", "success");
        await loadClubDeals();
        return true;
      } catch (error) {
        showToast(error.message, "error");
        return false;
      }
    }
