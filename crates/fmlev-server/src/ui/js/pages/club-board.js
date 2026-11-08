    // ---- Board objectives and club status (opt-in writes) ----
    // Read: /api/club/board and /api/club/status. Write: one explicit button per row, only with the
    // "Enable experimental board editing" box ticked. Nothing here is staged through the save bar.
    const boardRows = [];
    const boardView = { clubId: null, objectives: [], status: null };

    function boardExperimentalOn() { return !!document.getElementById("board-experimental")?.checked; }
    function boardClubId() { return state.activeClub?.club_id ?? null; }

    function isoToYearDay(iso) {
      const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(iso || "").trim());
      if (!m) return null;
      const [year, month, dayOfMonth] = [Number(m[1]), Number(m[2]), Number(m[3])];
      const date = new Date(Date.UTC(year, month - 1, dayOfMonth));
      if (date.getUTCFullYear() !== year || date.getUTCMonth() !== month - 1 || date.getUTCDate() !== dayOfMonth) return null;
      return { year, day: Math.round((date.getTime() - Date.UTC(year, 0, 1)) / 86400000) + 1 };
    }

    // Builds the update body from what the user typed; only fields that differ from the loaded objective are sent.
    function boardUpdateBody(clubId, objective, typed) {
      const body = { club_id: clubId, objective_index: objective.index, type_code: objective.type_code, experimental: true };
      let changed = 0;
      if (typed.importance !== "" && Number(typed.importance) !== objective.importance) {
        const n = Number(typed.importance);
        if (!Number.isInteger(n) || n < 1 || n > 10) throw new Error("Importance must be a whole number from 1 to 10");
        body.importance = n; changed++;
      }
      if (typed.value !== "" && objective.value !== null && Number(typed.value) !== objective.value) {
        const n = Number(typed.value);
        if (!Number.isInteger(n) || n < 0 || n > 2147483647) throw new Error("Target value must be a whole number, 0 or more");
        body.value = n; changed++;
      }
      if (typed.deadline !== "" && objective.deadline && typed.deadline !== objective.deadline.iso) {
        const parsed = isoToYearDay(typed.deadline);
        if (!parsed) throw new Error("Deadline must be a real date written YYYY-MM-DD");
        body.deadline_year = parsed.year; body.deadline_day = parsed.day; changed++;
      }
      if (typed.status !== "" && Number(typed.status) !== objective.status_code) {
        body.status_code = Number(typed.status); changed++;
      }
      if (!changed) throw new Error("Nothing changed on this objective");
      return body;
    }

    function updateBoardButtons() {
      const on = boardExperimentalOn() && !!state.connected;
      boardRows.forEach(row => { row.button.disabled = !on; });
      const status = document.getElementById("board-status-apply");
      if (status) status.disabled = !on || !(boardView.status && boardView.status.code);
    }

    function boardCell(text, title) {
      const td = document.createElement("td");
      td.textContent = text;
      if (title) td.title = title;
      return td;
    }

    function boardInput(value, disabled, width) {
      const input = document.createElement("input");
      input.className = "form-control";
      input.style.width = width;
      input.style.padding = "2px 6px";
      input.value = value === null || value === undefined ? "" : String(value);
      input.disabled = disabled;
      return input;
    }

    function renderClubBoard(data) {
      const body = document.getElementById("board-table-body");
      boardRows.length = 0;
      if (!body) return;
      body.innerHTML = "";
      boardView.objectives = data.board.objectives;
      for (const objective of data.board.objectives) {
        const tr = document.createElement("tr");
        tr.appendChild(boardCell(String(objective.index)));
        tr.appendChild(boardCell(objective.class === "competition_vision" ? "Competition" : "Club vision"));
        tr.appendChild(boardCell(
          `${objective.type_label || "Unknown type"} (${objective.type_code})`,
          "Label from a community catalog, not compared with the game's board screen"
        ));
        const target = objective.target_id === null ? "—" : `id ${objective.target_id}${objective.target_kind_code === null ? "" : ` (kind ${objective.target_kind_code})`}`;
        tr.appendChild(boardCell(target));
        const row = {
          importance: boardInput(objective.importance, objective.importance === null, "56px"),
          value: boardInput(objective.value, objective.value === null, "86px"),
          deadline: boardInput(objective.deadline ? objective.deadline.iso : "", !objective.deadline, "110px"),
          status: boardInput(objective.status_code, ![1, 2].includes(objective.status_code), "48px"),
          button: document.createElement("button"),
        };
        row.importance.title = "Importance 1-10";
        row.value.title = objective.value === null ? "This objective has no target value" : "Target value (places, age, years ...)";
        row.deadline.title = objective.deadline ? "Deadline YYYY-MM-DD" : "No deadline set; one cannot be added";
        row.status.title = "Status code 1 or 2 (meaning unmapped)";
        for (const key of ["importance", "value", "deadline", "status"]) {
          const td = document.createElement("td");
          td.appendChild(row[key]);
          tr.appendChild(td);
        }
        row.button.className = "btn btn-sm";
        row.button.textContent = "Apply";
        row.button.disabled = true;
        row.button.addEventListener("click", () => applyBoardObjective(objective.index));
        const action = document.createElement("td");
        action.appendChild(row.button);
        tr.appendChild(action);
        boardRows[objective.index] = row;
        body.appendChild(tr);
      }
      updateBoardButtons();
    }

    function setBoardNote(text) {
      const note = document.getElementById("board-note");
      if (note) note.innerText = text;
    }

    async function postBoard(url, payload) {
      const res = await fetch(url, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(payload) });
      const data = await readJson(res);
      if (!res.ok) throw new Error(data.message || "Board request failed");
      return data;
    }

    async function loadClubBoard() {
      const clubId = boardClubId();
      if (clubId === null || !state.connected) { setBoardNote("Attach to FM26 and select a club first."); return false; }
      try {
        const data = await postBoard("/api/club/board", { club_id: clubId });
        if (boardClubId() !== clubId) return false;
        boardView.clubId = clubId;
        renderClubBoard(data);
        setBoardNote(`${data.board.objectives.length} objectives read (${data.board.chain.human_manager_details ? "your club" : "AI club"}). Labels and status codes are unverified.`);
        return true;
      } catch (error) {
        const body = document.getElementById("board-table-body");
        if (body) body.innerHTML = "";
        boardRows.length = 0;
        boardView.objectives = [];
        setBoardNote(`Unavailable: ${error.message}`);
        return false;
      }
    }

    async function applyBoardObjective(index) {
      const clubId = boardClubId();
      const objective = boardView.objectives[index];
      const row = boardRows[index];
      if (!objective || !row || clubId === null || boardView.clubId !== clubId) { showToast("Read the board first", "error"); return false; }
      if (!boardExperimentalOn()) { showToast("Enable experimental board editing first", "error"); return false; }
      try {
        const body = boardUpdateBody(clubId, objective, {
          importance: row.importance.value.trim(), value: row.value.value.trim(),
          deadline: row.deadline.value.trim(), status: row.status.value.trim(),
        });
        if (typeof confirm === "function" && !confirm(`Experimental: write objective ${index} (${objective.type_label || objective.type_code}) in game memory? Use a throwaway save.`)) return false;
        await postBoard("/api/club/board/update", body);
        showToast("Objective written and read back. Reopen the board screen in FM26.", "success");
        await loadClubBoard();
        return true;
      } catch (error) {
        showToast(error.message, "error");
        return false;
      }
    }

    async function loadClubStatusInfo() {
      const clubId = boardClubId();
      const label = document.getElementById("board-status-current");
      const select = document.getElementById("board-status-select");
      if (clubId === null || !state.connected) { if (label) label.innerText = "Attach to FM26 and select a club first."; return false; }
      try {
        const data = await postBoard("/api/club/status", { club_id: clubId });
        boardView.status = data.club_status;
        const s = data.club_status;
        if (label) label.innerText = s.code ? `${s.label} (code ${s.code}, label inferred)` : `Unavailable: ${s.reason}`;
        if (select && s.code) select.value = String(s.code);
        updateBoardButtons();
        return true;
      } catch (error) {
        boardView.status = null;
        if (label) label.innerText = `Unavailable: ${error.message}`;
        updateBoardButtons();
        return false;
      }
    }

    async function applyClubStatus() {
      const clubId = boardClubId();
      const status = Number(document.getElementById("board-status-select")?.value);
      if (clubId === null || !boardView.status || !boardView.status.code) { showToast("Read the club status first", "error"); return false; }
      if (!boardExperimentalOn()) { showToast("Enable experimental board editing first", "error"); return false; }
      if (![1, 2, 3].includes(status)) { showToast("Pick professional, semi-professional or amateur", "error"); return false; }
      if (status === boardView.status.code) { showToast("The club already has this status", "info"); return false; }
      if (typeof confirm === "function" && !confirm("Experimental: write the club status byte in game memory? Use a throwaway save.")) return false;
      try {
        await postBoard("/api/club/status/update", { club_id: clubId, status, experimental: true });
        showToast("Club status written and read back. Reopen the club screen in FM26.", "success");
        await loadClubStatusInfo();
        return true;
      } catch (error) {
        showToast(error.message, "error");
        return false;
      }
    }
