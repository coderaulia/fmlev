    // ---- Registration doctor (read-only) ----
    const eligibilityState = { report: null };
    async function checkEligibility() {
      if (!state.activePlayer || !state.activePlayer.id) { showToast("Open a player first.", "warning"); return; }
      const compIndex = Number.parseInt(document.getElementById("elig-comp-index")?.value, 10);
      const compUid = Number.parseInt(document.getElementById("elig-comp-uid")?.value, 10);
      if (!Number.isInteger(compIndex) || !Number.isInteger(compUid)) { showToast("Enter the competition index and uid.", "warning"); return; }
      try {
        const res = await fetch("/api/player/eligibility", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ player_id: state.activePlayer.id, comp_index: compIndex, comp_uid: compUid, game_message: document.getElementById("elig-message")?.value || "" })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Eligibility check failed");
        eligibilityState.report = data.support_report;
        const icon = { ok: "✅", blocked: "⛔", unknown: "❔" };
        document.getElementById("elig-results").innerHTML = data.results.map(r =>
          `<div style="margin-bottom:6px;">${icon[r.status] || "❔"} <b>${escapeHtml(r.title)}</b> · ${escapeHtml(r.status)}<div style="color:var(--text-dim);">${escapeHtml(r.evidence)}${r.fix ? ` <i>Fix: ${escapeHtml(r.fix)}</i>` : ""}</div></div>`).join("") +
          `<div style="color:var(--text-dim);">${escapeHtml(data.player_note)}</div>`;
      } catch (error) { eligibilityState.report = null; showToast(error.message, "error"); }
    }
    async function copySupportReport() {
      if (!eligibilityState.report) { showToast("Run Check gates first.", "warning"); return; }
      const report = { ...eligibilityState.report, game_message: document.getElementById("elig-message")?.value || "" };
      try { await navigator.clipboard.writeText(JSON.stringify(report, null, 2)); showToast("Support report copied.", "success"); }
      catch (error) { showToast("Could not copy: " + error.message, "error"); }
    }

