    // ---- Save bar: one sticky bar, bound to the active page. Nothing is ever saved without a Save click / Ctrl+S. ----
    //   SaveBar.register(pageId, { load, save, discard, changes })
    //     load()    read current values from the game into the page
    //     save()    write the page's edits; return false (or throw) to report failure
    //     discard() put edits back to the last loaded values (defaults to load())
    //     changes() -> [{label, from, to}] pending edits (drives the count and the Review list)
    //   SaveBar.markDirty(pageId) / SaveBar.clear(pageId) / SaveBar.save() / SaveBar.load() / SaveBar.discard()
    // pageId is the Shell section id: player, staff, club, nation, competition, cheats, settings, diagnostics.
    const SaveBar = (() => {
      const pages = {};
      const flagged = {};      // pageId -> true after markDirty until save/clear
      const editSeq = {};      // pageId -> bumps on markDirty, so edits made during a save survive it
      const busy = {};
      const errors = {};
      let active = null;
      let reviewOpen = false;
      let timer = null;

      const el = id => (typeof document !== "undefined" && document.getElementById ? document.getElementById(id) : null);
      const pageOf = id => pages[id] || null;

      function changesOf(id) {
        const page = pageOf(id);
        if (!page || typeof page.changes !== "function") return [];
        try { const list = page.changes(); return Array.isArray(list) ? list : []; }
        catch (error) { return []; }
      }
      function countOf(id) { return Math.max(changesOf(id).length, flagged[id] ? 1 : 0); }
      function isDirty(id = active) { return !!id && countOf(id) > 0; }
      function anyDirty() { return pendingActions.size > 0 || Object.keys(pages).some(id => isDirty(id)); }

      function setDisabled(id, value) { const node = el(id); if (node) node.disabled = !!value; }
      function setHidden(id, value) { const node = el(id); if (node) node.hidden = !!value; }

      function renderReview(list) {
        const pop = el("savebar-popover");
        if (!pop) return;
        pop.hidden = !(reviewOpen && list.length > 0);
        if (pop.hidden) return;
        const show = value => (value === undefined || value === null || value === "" ? "empty" : String(value));
        pop.innerHTML = list.map(change => {
          const hasValues = change.from !== undefined || change.to !== undefined;
          return `<div class="review-row"><span class="review-label">${escapeHtml(change.label)}</span>` +
            (hasValues ? `<span class="review-from">${escapeHtml(show(change.from))}</span><span class="review-arrow">&rarr;</span><span class="review-to">${escapeHtml(show(change.to))}</span>` : "") + `</div>`;
        }).join("");
      }

      function render() {
        const bar = el("savebar");
        const page = pageOf(active);
        if (bar) bar.hidden = !page;
        if (!page) return;
        const list = changesOf(active);
        const count = Math.max(list.length, flagged[active] ? 1 : 0);
        const dirty = count > 0;
        const saving = !!busy[active];
        const status = el("savebar-status");
        if (status) status.textContent = saving ? "Working..." : dirty ? `${count} unsaved change${count === 1 ? "" : "s"}` : "No unsaved changes";
        if (bar && bar.classList) { if (dirty) bar.classList.add("dirty"); else bar.classList.remove("dirty"); }
        setHidden("savebar-review", !(dirty && list.length > 0));
        setHidden("savebar-load", typeof page.load !== "function");
        setDisabled("savebar-load", saving);
        setDisabled("savebar-save", !dirty || saving || typeof page.save !== "function");
        setDisabled("savebar-discard", !dirty || saving);
        const err = el("savebar-error");
        if (err) { err.textContent = errors[active] || ""; err.hidden = !errors[active]; }
        if (reviewOpen && !dirty) reviewOpen = false;
        renderReview(list);
      }

      function register(id, handlers) {
        pages[id] = handlers || {};
        if (id === active) render();
      }
      function setActivePage(id) {
        active = id;
        reviewOpen = false;
        render();
      }
      function markDirty(id = active) {
        if (!id) return;
        flagged[id] = true;
        editSeq[id] = (editSeq[id] || 0) + 1;
        if (id === active) render();
      }
      function clear(id = active) {
        if (!id) return;
        flagged[id] = false;
        errors[id] = "";
        if (id === active) render();
      }

      async function save(id = active) {
        const page = pageOf(id);
        if (!page || typeof page.save !== "function" || busy[id]) return false;
        if (!isDirty(id)) return false;
        const seq = editSeq[id] || 0;
        busy[id] = true;
        errors[id] = "";
        render();
        let ok = false;
        try {
          ok = (await page.save()) !== false;
          if (!ok) errors[id] = "Save did not complete. Your changes are kept.";
        } catch (error) {
          errors[id] = (error && error.message) || String(error);
          if (typeof showToast === "function") showToast(`Save failed: ${errors[id]}`, "error");
        } finally {
          busy[id] = false;
        }
        if (ok && (editSeq[id] || 0) === seq) flagged[id] = false;
        else if (!ok) flagged[id] = true;
        render();
        return ok;
      }

      async function load(id = active) {
        const page = pageOf(id);
        if (!page || typeof page.load !== "function" || busy[id]) return false;
        if (isDirty(id) && typeof confirm === "function" && !confirm("Loading from the game replaces your unsaved changes. Continue?")) return false;
        busy[id] = true;
        errors[id] = "";
        render();
        let ok = false;
        try {
          ok = (await page.load()) !== false;
          if (!ok) errors[id] = "Load did not complete.";
        } catch (error) {
          errors[id] = (error && error.message) || String(error);
          if (typeof showToast === "function") showToast(`Load failed: ${errors[id]}`, "error");
        } finally {
          busy[id] = false;
        }
        if (ok) flagged[id] = false;
        render();
        return ok;
      }

      async function discard(id = active) {
        const page = pageOf(id);
        if (!page || busy[id]) return false;
        const revert = typeof page.discard === "function" ? page.discard : page.load;
        if (typeof revert !== "function") return false;
        busy[id] = true;
        errors[id] = "";
        let ok = true;
        try { ok = (await revert.call(page)) !== false; }
        catch (error) {
          ok = false;
          errors[id] = (error && error.message) || String(error);
          if (typeof showToast === "function") showToast(`Discard failed: ${errors[id]}`, "error");
        } finally { busy[id] = false; }
        if (ok) flagged[id] = false;
        reviewOpen = false;
        render();
        return ok;
      }

      function toggleReview() { reviewOpen = !reviewOpen; render(); }
      function closeReview() { if (reviewOpen) { reviewOpen = false; render(); } }

      // Adapters for pages that still use the older per-entity dirty flags (state.playerDirtyFields, state.clubDirty, ...).
      // A page that registers itself replaces the adapter.
      async function reloadEntity(kind) {
        const control = entityControls[kind];
        const entity = state[control.active];
        if (!entity) throw new Error(`Select a ${kind} first`);
        const res = await fetch("/api/context/select", {
          method: "POST", headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ [`${kind}_id`]: entity[control.id] })
        });
        const data = await readJson(res);
        if (!res.ok) throw new Error(data.message || "Load from game failed");
        if (state[control.active]?.[control.id] !== entity[control.id]) throw new Error("Selection changed while loading. Retry for the current selection.");
        const fresh = data[kind] || data[`active_${kind}`];
        if (!fresh || fresh[control.id] !== entity[control.id]) throw new Error("Load returned no matching entity");
        pendingActions.delete(`${kind}:${entity[control.id]}`);
        if (kind === "player") { state.playerDirtyFields.clear(); state.playerDirty = false; }
        else state[`${kind}Dirty`] = false;
        if (fresh) control.display(fresh);
        return true;
      }
      function legacyChanges(kind) {
        const control = entityControls[kind];
        const entity = state[control.active];
        const list = [];
        if (kind === "player") for (const field of state.playerDirtyFields) list.push({ label: String(field).replace(/^attr_/, "").replace(/_/g, " ") });
        else if (control.dirty()) list.push({ label: "Edited fields" });
        for (const action of (entity ? pendingActions.get(`${kind}:${entity[control.id]}`) : null) || []) list.push({ label: action.label });
        return list;
      }
      function registerLegacyPages() {
        const defs = {
          player: () => applyPlayerChanges(), club: () => saveClubToGame(),
          nation: () => saveNationToGame(), competition: () => saveCompetitionToGame(),
        };
        for (const kind of Object.keys(defs)) {
          if (pages[kind]) continue;
          register(kind, { load: () => reloadEntity(kind), save: defs[kind], discard: () => reloadEntity(kind), changes: () => legacyChanges(kind) });
        }
      }

      function init() {
        registerLegacyPages();
        if (typeof document !== "undefined" && document.addEventListener) {
          document.addEventListener("click", event => {
            const target = event && event.target;
            if (reviewOpen && target && target.closest && !target.closest("#savebar")) closeReview();
          });
        }
        if (typeof window !== "undefined" && window.addEventListener) {
          window.addEventListener("beforeunload", event => { if (anyDirty()) { event.preventDefault(); event.returnValue = ""; } });
        }
        // Legacy pages flip state flags directly, so refresh the bar a few times a second.
        if (!timer && typeof setInterval === "function") timer = setInterval(render, 500);
        render();
      }

      return {
        register, setActivePage, markDirty, clear, save, load, discard, init, render,
        toggleReview, closeReview, isDirty, anyDirty,
        changes: id => changesOf(id === undefined ? active : id),
        activePage: () => active,
      };
    })();
