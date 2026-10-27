    // ---- Settings page: display currency / measurement / wage period and value-band colours ----
    // Formatter functions (formatMoney, setMoneyInput, ...) live in js/core/format.js.
    // The page has one Load and one Save (SaveBar). Nothing here writes game memory.
    const BAND_SCHEME_KEY = "fmle.bandScheme";
    let savedBandScheme = "fm26";

    function readStoredBandScheme() {
      try { return localStorage.getItem(BAND_SCHEME_KEY) === "legacy" ? "legacy" : "fm26"; }
      catch (error) { return "fm26"; }
    }
    function writeStoredBandScheme(name) {
      try { localStorage.setItem(BAND_SCHEME_KEY, name); } catch (error) { /* private mode: the choice lasts for this tab only */ }
    }

    function applySettingsToControls() {
      const set = (id, value) => { const el = document.getElementById(id); if (el) el.value = value; };
      const select = document.getElementById("settings-currency");
      if (select && select.options && select.options.length !== currencyTable.length) {
        select.innerHTML = currencyTable.map(c => `<option value="${c.code}">${c.code} (${c.symbol.trim()})</option>`).join("");
      }
      set("settings-currency", displaySettings.currency);
      set("settings-measurement", displaySettings.measurement);
      set("settings-wage-period", displaySettings.wage_period);
      set("settings-band-scheme", savedBandScheme);
      const cur = currentCurrency();
      const rate = document.getElementById("settings-rate");
      if (rate) rate.innerText = `Fixed rate: 1 GBP = ${cur.rate_per_gbp} ${cur.code} (table dated ${ratesDate}, approximate)`;
      const preview = document.getElementById("settings-preview");
      if (preview) preview.innerText = `Preview: ${formatMoney(50000000, { compact: true })} budget, ${formatMoney(400000, { kind: "wage" })} wage, 185 cm = ${metersToHeight(185)}, 80 kg = ${formatWeight(80)}`;
      refreshMoneyLabels();
    }
    function applySettingsPayload(data) {
      displaySettings = data.settings;
      if (Array.isArray(data.currencies) && data.currencies.length) currencyTable = data.currencies;
      ratesDate = data.rates_date || "";
      applySettingsToControls();
    }

    // Load: read the saved settings from the server (and the stored colour choice). Returns true on success.
    async function loadDisplaySettings() {
      savedBandScheme = readStoredBandScheme();
      applyBandScheme(savedBandScheme);
      try {
        const res = await fetch("/api/settings");
        const data = await res.json();
        if (!res.ok) return false;
        applySettingsPayload(data);
        rebaseMoneyInputs();
        return true;
      } catch (error) { /* keep GBP/metric defaults when the server cannot be reached */ return false; }
    }

    function settingsControlValues() {
      const read = id => { const el = document.getElementById(id); return el ? el.value : undefined; };
      return {
        currency: read("settings-currency"), measurement: read("settings-measurement"),
        wage_period: read("settings-wage-period"), band_scheme: read("settings-band-scheme"),
      };
    }

    // Pending edits for the Save bar: controls that differ from what is currently saved.
    function settingsChanges() {
      const now = settingsControlValues();
      const saved = { currency: displaySettings.currency, measurement: displaySettings.measurement,
        wage_period: displaySettings.wage_period, band_scheme: savedBandScheme };
      const labels = { currency: "Currency", measurement: "Measurement", wage_period: "Contract wage period", band_scheme: "Value colours" };
      return Object.keys(labels)
        .filter(key => now[key] !== undefined && now[key] !== "" && now[key] !== saved[key])
        .map(key => ({ label: labels[key], from: saved[key], to: now[key] }));
    }

    // Edits only mark the page dirty (and preview the colours); nothing is saved until Save.
    function markSettingsDirty() {
      const scheme = settingsControlValues().band_scheme;
      if (scheme) applyBandScheme(scheme);
      SaveBar.markDirty("settings");
    }

    // Save: persist display settings (server) and the colour choice (this browser). Returns true on success.
    async function saveDisplaySettings() {
      const values = settingsControlValues();
      const body = { currency: values.currency, measurement: values.measurement, wage_period: values.wage_period };
      // Capture what each input means in raw units under the OLD settings before they change.
      const raws = new Map();
      document.querySelectorAll("input[data-money]").forEach(el => raws.set(el, readMoneyInput(el, null)));
      try {
        const res = await fetch("/api/settings", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
        const data = await res.json();
        if (!res.ok) throw new Error(data.message || "Settings were rejected");
        applySettingsPayload(data);
        rebaseMoneyInputs(el => raws.get(el));
        if (values.band_scheme) { savedBandScheme = applyBandScheme(values.band_scheme); writeStoredBandScheme(savedBandScheme); }
        applySettingsToControls();
        showToast("Display settings saved.", "success");
        return true;
      } catch (error) {
        applySettingsToControls();
        showToast(error.message, "error");
        return false;
      }
    }

    // Discard: put the controls (and colour preview) back to what is saved.
    function discardSettingsEdits() {
      applyBandScheme(savedBandScheme);
      applySettingsToControls();
    }

    SaveBar.register("settings", {
      load: loadDisplaySettings, save: saveDisplaySettings, discard: discardSettingsEdits, changes: settingsChanges,
    });
