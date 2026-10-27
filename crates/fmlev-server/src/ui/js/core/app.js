    // Initialization
    window.addEventListener("DOMContentLoaded", () => {
      // Check query params for compact overlay mode
      const params = new URLSearchParams(window.location.search);
      if (params.get("mode") === "overlay" || params.get("mode") === "compact") {
        setOverlayMode(true);
      }

      buildAttributeLists();
      loadDisplaySettings();
      loadRoles();
      fetchContext();

      // Poll every 1 second when unpinned; displayPlayer protects focused inputs and dirtyPlayerFields
      setInterval(() => {
        updateSyncedAgoDisplay();
        if (!state.isPinned) {
          fetchContext();
        }
      }, 1000);
    });

    function isInputFocused() {
      const active = document.activeElement;
      return active && (active.tagName === "INPUT" || active.tagName === "SELECT" || active.tagName === "TEXTAREA");
    }

    // Toggle Overlay Mode
    function toggleOverlayMode() {
      setOverlayMode(!state.isOverlayMode);
    }

    function setOverlayMode(enable) {
      state.isOverlayMode = enable;
      if (enable) {
        document.body.classList.add("mode-compact-overlay");
        document.getElementById("overlay-mode-btn").innerText = "🖥️ Full Editor";
        showToast("Switched to Compact In-Game Overlay HUD", "info");
      } else {
        document.body.classList.remove("mode-compact-overlay");
        document.getElementById("overlay-mode-btn").innerText = "📱 Compact Overlay";
        showToast("Switched to Full Live Editor", "info");
      }
    }

    // Tab Navigation
    function switchTab(tabId) {
      document.querySelectorAll(".tab-view").forEach(el => el.classList.remove("active"));
      document.querySelectorAll(".tab-btn").forEach(el => el.classList.remove("active"));

      const target = document.getElementById(tabId);
      if (target) target.classList.add("active");

      const buttons = Array.from(document.querySelectorAll(".tab-btn"));
      const btn = buttons.find(b => b.getAttribute("onclick")?.includes(tabId));
      if (btn) btn.classList.add("active");
    }

    const allAttrKeys = [
      ...techAttrs,
      ...mentalAttrs,
      ...physAttrs,
      ...hiddenAttrs,
      ...gkAttrs
    ].map(([k]) => k);

