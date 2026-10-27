    // ---- Application shell: sidebar navigation, context header with history, sub-tab row, global keys ----
    // Public API (see docs/UI_ARCHITECTURE.md):
    //   switchTab(tabId) / Shell.navigate(tabId)     show a page (<section id="tab-...">)
    //   Shell.setContext({title, subtitle, section?, kind?, id?, open?})   entity name in the header
    //   Shell.setSubTabs([{id,label,onSelect?}], activeId, {section?, onSelect?})   sub-tab row
    //   Shell.back() / Shell.forward()               entity history
    const Shell = (() => {
      const ICONS = {
        player: '<circle cx="12" cy="8" r="3.6"/><path d="M5 20c0-3.9 3.1-6.5 7-6.5s7 2.6 7 6.5"/>',
        staff: '<path d="M3 9l9-4 9 4-9 4-9-4z"/><path d="M7 11.5V16c0 1.2 2.2 2.5 5 2.5s5-1.3 5-2.5v-4.5"/>',
        club: '<path d="M12 3l7 3v5c0 5-3 8.3-7 10-4-1.7-7-5-7-10V6l7-3z"/>',
        nation: '<circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17M12 3.5c2.6 2.5 2.6 14.5 0 17M12 3.5c-2.6 2.5-2.6 14.5 0 17"/>',
        competition: '<path d="M8 4h8v5a4 4 0 01-8 0V4z"/><path d="M8 6H4.5c0 3 1.5 4.5 3.7 4.8M16 6h3.5c0 3-1.5 4.5-3.7 4.8M12 13v4M8.5 20h7M10 17h4"/>',
        cheats: '<path d="M13 3L5 13.5h6L10 21l8-10.5h-6L13 3z"/>',
        settings: '<circle cx="12" cy="12" r="3"/><path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M18.4 5.6l-2.1 2.1M7.7 16.3l-2.1 2.1"/>',
        diagnostics: '<path d="M3 12h4l2.5-6 4 12 2.5-6H21"/>',
      };
      const PLAYER_SUBTABS = [
        { id: "tab-player", label: "Attributes" },
        { id: "tab-positions", label: "Positions & Roles" },
        { id: "tab-contract", label: "Contract & Transfer" },
      ];
      // Sidebar sections. `id` is also the SaveBar page id for every tab in `tabs`.
      const NAV = [
        { id: "player", label: "Player", group: "Editors", tab: "tab-player", tabs: ["tab-player", "tab-positions", "tab-contract", "tab-registration"], subTabs: PLAYER_SUBTABS },
        { id: "staff", label: "Staff", group: "Editors", tab: "tab-staff", tabs: ["tab-staff"] },
        { id: "club", label: "Club", group: "Editors", tab: "tab-club", tabs: ["tab-club"] },
        { id: "nation", label: "Nation", group: "Editors", tab: "tab-nation", tabs: ["tab-nation"] },
        { id: "competition", label: "Competition", group: "Editors", tab: "tab-competition", tabs: ["tab-competition"] },
        { id: "cheats", label: "Cheats", group: "Tools", tab: "tab-godmode", tabs: ["tab-godmode"] },
        { id: "settings", label: "Settings", group: "Tools", tab: "tab-settings", tabs: ["tab-settings"] },
        { id: "diagnostics", label: "Diagnostics", group: "Tools", tab: "tab-status", tabs: ["tab-status"] },
      ];
      // How a history entry re-opens its entity when no `open` callback was given.
      const KIND_OPENERS = {
        player: id => typeof selectSearchedPlayer === "function" && selectSearchedPlayer(id),
        club: id => typeof selectSearchedClub === "function" && selectSearchedClub(id),
        nation: id => typeof selectSearchedNation === "function" && selectSearchedNation(id),
        competition: id => typeof selectSearchedCompetition === "function" && selectSearchedCompetition(id),
      };
      const HISTORY_LIMIT = 60;

      const st = {
        tabId: "tab-player", section: "player",
        contexts: {},                 // section -> {title, subtitle}
        subTabs: {},                  // section -> {items, active, onSelect}
        history: [], index: -1,       // entity history
        listeners: [],
      };
      for (const item of NAV) if (item.subTabs) st.subTabs[item.id] = { items: item.subTabs, active: item.tab, onSelect: null };

      const el = id => (typeof document !== "undefined" && document.getElementById ? document.getElementById(id) : null);
      const qsa = selector => (typeof document !== "undefined" && document.querySelectorAll ? Array.from(document.querySelectorAll(selector)) : []);
      const sectionOf = tabId => NAV.find(item => item.tabs.includes(tabId)) || null;
      const sectionById = id => NAV.find(item => item.id === id) || null;

      function renderNav() {
        const nav = el("sidebar-nav");
        if (!nav) return;
        let group = "", html = "";
        for (const item of NAV) {
          if (item.group !== group) { group = item.group; html += `<div class="nav-group-label">${group}</div>`; }
          html += `<button class="nav-item${item.id === st.section ? " active" : ""}" data-section="${item.id}" aria-label="${item.label}" title="${item.label}"${item.id === st.section ? ' aria-current="page"' : ""} onclick="switchTab('${item.tab}')">` +
            `<svg viewBox="0 0 24 24" aria-hidden="true">${ICONS[item.id] || ""}</svg><span>${item.label}</span></button>`;
        }
        nav.innerHTML = html;
        if (state.isOverlayMode) nav.querySelector?.('[aria-current="page"]')?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
      }

      function renderContext() {
        const section = sectionById(st.section);
        const ctx = st.contexts[st.section] || {};
        const title = el("context-title"), subtitle = el("context-subtitle");
        if (title) title.textContent = ctx.title || (section ? section.label : "FMLE");
        if (subtitle) subtitle.textContent = ctx.subtitle || "";
        const back = el("context-back"), forward = el("context-forward");
        if (back) back.disabled = st.index <= 0;
        if (forward) forward.disabled = st.index >= st.history.length - 1;
      }

      function renderSubTabs() {
        const bar = el("subtabs");
        if (!bar) return;
        const entry = st.subTabs[st.section];
        if (!entry || !entry.items || !entry.items.length) { bar.hidden = true; bar.innerHTML = ""; return; }
        bar.hidden = false;
        bar.innerHTML = entry.items.map(item =>
          `<button class="subtab${item.id === entry.active ? " active" : ""}" role="tab" aria-selected="${item.id === entry.active}" onclick="Shell.selectSubTab('${String(item.id).replace(/'/g, "")}')">${escapeHtml(item.label)}</button>`).join("");
        if (state.isOverlayMode) bar.querySelector?.('[aria-selected="true"]')?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
      }

      function render() { renderNav(); renderContext(); renderSubTabs(); }

      function navigate(tabId) {
        for (const view of qsa(".tab-view")) view.classList.remove("active");
        const target = el(tabId);
        if (target) target.classList.add("active");
        const section = sectionOf(tabId);
        st.tabId = tabId;
        if (section) st.section = section.id;
        const subs = st.subTabs[st.section];
        if (subs && subs.items.some(item => item.id === tabId)) subs.active = tabId;
        render();
        if (typeof SaveBar !== "undefined") SaveBar.setActivePage(st.section);
        for (const listener of st.listeners) { try { listener(tabId, st.section); } catch (error) { console.warn("Shell listener failed", error); } }
        return st.section;
      }

      function setSubTabs(items, activeId, opts = {}) {
        const section = opts.section || st.section;
        const list = Array.isArray(items) ? items : [];
        st.subTabs[section] = {
          items: list,
          active: list.some(item => item.id === activeId) ? activeId : (list[0] ? list[0].id : null),
          onSelect: typeof opts.onSelect === "function" ? opts.onSelect : null,
        };
        if (section === st.section) renderSubTabs();
      }

      function selectSubTab(id) {
        const entry = st.subTabs[st.section];
        if (!entry) return;
        const item = entry.items.find(candidate => candidate.id === id);
        if (!item) return;
        entry.active = id;
        if (typeof item.onSelect === "function") item.onSelect(id);
        else if (entry.onSelect) entry.onSelect(id);
        else if (String(id).startsWith("tab-") && el(id)) { navigate(id); return; }
        renderSubTabs();
      }

      function setActiveSubTab(id, section = st.section) {
        const entry = st.subTabs[section];
        if (entry && entry.items.some(item => item.id === id)) { entry.active = id; if (section === st.section) renderSubTabs(); }
      }

      function setContext(ctx = {}) {
        const section = ctx.section || st.section;
        const title = ctx.title === undefined || ctx.title === null ? "" : String(ctx.title);
        const subtitle = ctx.subtitle === undefined || ctx.subtitle === null ? "" : String(ctx.subtitle);
        const previous = st.contexts[section];
        const isNew = !previous || previous.title !== title;
        st.contexts[section] = { title, subtitle, kind: ctx.kind, id: ctx.id, open: ctx.open };
        // One history entry per distinct entity per section; polling the same entity never adds entries.
        if (title && isNew) {
          st.history = st.history.slice(0, st.index + 1);
          const home = sectionById(section);
          st.history.push({ section, title, subtitle, kind: ctx.kind, id: ctx.id, open: ctx.open, tab: section === st.section ? st.tabId : (home ? home.tab : st.tabId) });
          if (st.history.length > HISTORY_LIMIT) st.history.shift();
          st.index = st.history.length - 1;
        }
        if (section === st.section) renderContext();
      }

      function openHistory(index) {
        const entry = st.history[index];
        if (!entry) return false;
        st.index = index;
        st.contexts[entry.section] = { title: entry.title, subtitle: entry.subtitle, kind: entry.kind, id: entry.id, open: entry.open };
        navigate(entry.tab);
        if (typeof entry.open === "function") entry.open();
        else if (entry.kind && KIND_OPENERS[entry.kind] && entry.id !== undefined && entry.id !== null) KIND_OPENERS[entry.kind](entry.id);
        return true;
      }
      const back = () => (st.index > 0 ? openHistory(st.index - 1) : false);
      const forward = () => (st.index < st.history.length - 1 ? openHistory(st.index + 1) : false);

      // Derive the header text from the entities the context poll returned (pages may call setContext themselves).
      function syncEntities(source) {
        const s = source || state;
        const join = (...parts) => parts.filter(part => part !== undefined && part !== null && String(part).trim() !== "").join(" · ");
        if (s.activePlayer && s.activePlayer.name) {
          const p = s.activePlayer;
          setContext({ section: "player", kind: "player", id: p.id, title: p.name, subtitle: join(p.position, p.club_name) });
        }
        if (s.activeClub && s.activeClub.club_name) {
          const c = s.activeClub;
          setContext({ section: "club", kind: "club", id: c.club_id, title: c.club_name, subtitle: join(c.league_name) });
        }
        if (s.activeNation && s.activeNation.name) {
          const n = s.activeNation;
          setContext({ section: "nation", kind: "nation", id: n.id, title: n.name, subtitle: join(n.continent) });
        }
        if (s.activeCompetition && s.activeCompetition.name) {
          const c = s.activeCompetition;
          setContext({ section: "competition", kind: "competition", id: c.id, title: c.name, subtitle: join(c.nation_name) });
        }
      }

      function isTyping(target) {
        if (!target) return false;
        const tag = String(target.tagName || "").toUpperCase();
        return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target.isContentEditable === true;
      }

      // One global keydown handler: "/" focuses search, Ctrl/Cmd+S saves the active page, Escape closes popovers.
      function handleKey(event) {
        const key = event.key;
        if ((event.ctrlKey || event.metaKey) && !event.altKey && (key === "s" || key === "S")) {
          event.preventDefault();
          if (typeof SaveBar !== "undefined") SaveBar.save();
          return;
        }
        if (key === "/" && !event.ctrlKey && !event.metaKey && !event.altKey && !isTyping(event.target)) {
          const box = el("global-search");
          if (box) { event.preventDefault(); box.focus(); if (box.select) box.select(); }
          return;
        }
        if (key === "Escape" && typeof SaveBar !== "undefined") SaveBar.closeReview();
      }

      function init() {
        render();
        if (typeof document !== "undefined" && document.addEventListener) document.addEventListener("keydown", handleKey);
        if (typeof SaveBar !== "undefined") SaveBar.init();
        navigate(st.tabId);
      }

      return {
        NAV, state: st, init, render, navigate, setContext, setSubTabs, setActiveSubTab, selectSubTab,
        back, forward, syncEntities, handleKey,
        onNavigate: fn => st.listeners.push(fn),
        current: () => ({ tabId: st.tabId, section: st.section }),
      };
    })();

    // Navigation API used by inline handlers and page code.
    function switchTab(tabId) { return Shell.navigate(tabId); }
