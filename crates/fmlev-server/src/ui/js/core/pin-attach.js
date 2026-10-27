    // Toggle Pin / Unpin
    async function togglePinPlayer() {
      const willPin = !state.isPinned;
      try {
        const res = await fetch("/api/context/select", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            player_id: willPin && state.activePlayer ? state.activePlayer.id : null,
            unpin: !willPin
          })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Selection failed");
        state.isPinned = data.is_pinned;
        updateSyncUI();
        showToast(state.isPinned ? "📌 Player pinned (game screen auto-follow paused)" : "👁️ Auto-following FM26 game screen", "info");
      } catch (e) {
        showToast("Error updating pin state: " + e.message, "error");
      }
    }

    // Auto Attach
    async function toggleAutoAttach() {
      showToast("Scanning for active Football Manager 26 process...", "info");
      try {
        const res = await fetch("/api/attach", { method: "POST" });
        const data = await readJson(res);
        if (res.ok && data.status === "attached") {
          showToast(`Attached to FM26 (PID: ${data.pid})!`, "success");
          fetchContext();
        } else {
          showToast(data.message || "Game not detected. Start FM26 on Linux/Proton.", "warning");
        }
      } catch (e) {
        showToast("Error: " + e.message, "error");
      }
    }

