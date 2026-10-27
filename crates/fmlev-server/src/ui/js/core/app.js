    // Initialization
    window.addEventListener("DOMContentLoaded", () => {
      // Check query params for compact overlay mode
      const params = new URLSearchParams(window.location.search);
      if (params.get("mode") === "overlay" || params.get("mode") === "compact") {
        setOverlayMode(true);
      }

      Shell.init();
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
        setText("overlay-mode-btn", "Full Editor");
        showToast("Switched to Compact In-Game Overlay HUD", "info");
      } else {
        document.body.classList.remove("mode-compact-overlay");
        setText("overlay-mode-btn", "Compact Overlay");
        showToast("Switched to Full Live Editor", "info");
      }
      Shell.render();
    }

    function setText(id, text) {
      const node = document.getElementById(id);
      if (node) node.innerText = text;
    }

    // Tab navigation lives in js/core/shell.js (switchTab / Shell.navigate).

    const allAttrKeys = [
      ...techAttrs,
      ...mentalAttrs,
      ...physAttrs,
      ...hiddenAttrs,
      ...gkAttrs
    ].map(([k]) => k);

