    // Keep manual edits through context polling after an input loses focus.
    for (const eventName of ["input", "change"]) {
      document.addEventListener(eventName, event => {
        // Money, prize, affiliate and search inputs have their own requests and are not part of the entity form.
        if (event.target.closest("#club-money-balance, #club-money-transfer, [id^='prize-'], #prize-list, [id^='new-aff-'], .search-wrapper")) return;
        if (event.target.closest("#tab-club")) state.clubDirty = true;
        if (event.target.closest("#tab-nation")) state.nationDirty = true;
        if (event.target.closest("#tab-competition")) state.competitionDirty = true;
      });
    }

    // Close search dropdown on click outside
    document.addEventListener("click", (e) => {
      const searchBox = document.querySelector(".search-wrapper");
      if (searchBox && !searchBox.contains(e.target)) {
        document.getElementById("search-results-dropdown").style.display = "none";
      }
    });

