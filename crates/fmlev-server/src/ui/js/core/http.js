    // Error bodies are not always JSON (proxy or crash); keep the HTTP status instead of a parser error.
    async function readJson(res) {
      try { return await res.json(); }
      catch (e) { return { message: res.ok ? "" : `HTTP ${res.status} ${res.statusText}`.trim() }; }
    }
    function showToast(msg, type = "info") {
      const container = document.getElementById("toast-container");
      if (!container) return;
      const el = document.createElement("div");
      el.className = `toast ${type}`;
      el.innerHTML = `<div>${escapeHtml(msg)}</div>`;
      container.appendChild(el);
      setTimeout(() => {
        el.style.opacity = "0";
        el.style.transform = "translateX(100%)";
        setTimeout(() => el.remove(), 250);
      }, 3200);
    }

    function escapeHtml(str) {
      if (!str) return "";
      return String(str).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    }
