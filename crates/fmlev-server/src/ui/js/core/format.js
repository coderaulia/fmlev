    // ---- Display settings: currency, measurement, wage period ----
    // Native values are raw GBP (wages per week) and metric. Everything shown or typed goes through here.
    let displaySettings = { currency: "GBP", measurement: "metric", wage_period: "weekly" };
    let currencyTable = [{ code: "GBP", symbol: "£", decimals: 0, rate_per_gbp: 1 }];
    let ratesDate = "";
    function currentCurrency() {
      return currencyTable.find(c => c.code === displaySettings.currency) || currencyTable[0];
    }
    function periodFactor(period) { return period === "yearly" ? 52 : period === "monthly" ? 52 / 12 : 1; }
    function periodLabel(period) { return period === "yearly" ? "year" : period === "monthly" ? "month" : "week"; }
    // kind "wage" follows the wage period; kind "money" is currency only.
    function moneyFactor(kind) {
      return currentCurrency().rate_per_gbp * (kind === "wage" ? periodFactor(displaySettings.wage_period) : 1);
    }
    function rawToDisplay(raw, kind) {
      const scale = 10 ** currentCurrency().decimals;
      return Math.round(raw * moneyFactor(kind) * scale) / scale;
    }
    function displayToRaw(shown, kind) { return Math.round(shown / moneyFactor(kind)); }
    function formatMoney(raw, opts = {}) {
      if (raw === null || raw === undefined || !Number.isFinite(Number(raw))) return "Unavailable";
      const cur = currentCurrency();
      const value = Number(raw) * moneyFactor(opts.kind);
      const sign = value < 0 ? "-" : "";
      const abs = Math.abs(value);
      let text;
      if (opts.compact) {
        const units = [[1e12, "T"], [1e9, "B"], [1e6, "M"], [1e3, "k"]];
        const unit = units.find(([size]) => abs >= size);
        text = unit ? `${+(abs / unit[0]).toFixed(2)}${unit[1]}` : String(+abs.toFixed(cur.decimals));
      } else {
        text = abs.toLocaleString(undefined, { minimumFractionDigits: cur.decimals, maximumFractionDigits: cur.decimals });
      }
      const suffix = opts.kind === "wage" && opts.period !== false ? ` / ${periodLabel(displaySettings.wage_period)}` : "";
      return `${sign}${cur.symbol}${text}${suffix}`;
    }
    function parseMoney(text, kind) {
      const cleaned = String(text ?? "").replace(/[^0-9.\-]/g, "");
      const n = Number(cleaned);
      return cleaned === "" || !Number.isFinite(n) ? null : displayToRaw(n, kind);
    }
    // Money inputs (data-money="wage|money") show display units but remember the raw native baseline,
    // so an untouched control returns the exact native value even when display rounding differs.
    function setMoneyInput(el, raw) {
      if (!el) return;
      if (raw === null || raw === undefined) { el.value = ""; delete el.dataset.raw; delete el.dataset.shown; return; }
      const shown = String(rawToDisplay(Number(raw), el.dataset.money));
      el.dataset.raw = String(raw);
      el.dataset.shown = shown;
      el.value = shown;
    }
    function readMoneyInput(el, fallback = 0) {
      if (!el || el.value === "") return fallback;
      const data = el.dataset || {};
      if (data.raw !== undefined && el.value === data.shown) return Number(data.raw);
      const n = Number(el.value);
      return Number.isFinite(n) ? displayToRaw(n, data.money) : fallback;
    }
    function metersToHeight(cm) {
      if (displaySettings.measurement !== "imperial") return `${cm} cm`;
      const totalIn = Math.round(cm / 2.54);
      return `${Math.floor(totalIn / 12)}'${totalIn % 12}"`;
    }
    function formatWeight(kg) {
      return displaySettings.measurement === "imperial" ? `${Math.round(kg * 2.20462)} lb` : `${kg} kg`;
    }
    function formatDistance(km) {
      return displaySettings.measurement === "imperial" ? `${+(km * 0.621371).toFixed(1)} mi` : `${km} km`;
    }
    function adjustMoneyInput(id, deltaRaw, minRaw = 0) {
      const el = document.getElementById(id);
      if (!el || el.disabled) return;
      setMoneyInput(el, Math.max(minRaw, readMoneyInput(el, 0) + deltaRaw));
    }
    function refreshMoneyLabels() {
      const cur = currentCurrency();
      document.querySelectorAll("[data-money-label]").forEach(el => {
        const kind = el.dataset.moneyKind;
        const per = kind === "wage" ? ` / ${periodLabel(displaySettings.wage_period)}` : kind === "rawweek" ? " / week, raw period unverified" : "";
        el.innerText = `${el.dataset.moneyLabel} (${cur.symbol.trim()}${per})`;
      });
      document.querySelectorAll("[data-step-raw]").forEach(el => {
        const delta = Number(el.dataset.stepRaw);
        el.innerText = `${delta < 0 ? "-" : "+"}${formatMoney(Math.abs(delta), { compact: true, kind: el.dataset.stepKind, period: false })}`;
      });
      document.querySelectorAll("[data-money-tpl]").forEach(el => {
        if (el.dataset.tplSource === undefined) el.dataset.tplSource = el.innerText;
        el.innerText = el.dataset.tplSource.replace(/\{m:(\d+)\}/g, (_, n) => formatMoney(Number(n), { compact: true }));
      });
      document.querySelectorAll("[data-money-placeholder]").forEach(el => {
        el.placeholder = `${el.dataset.moneyPlaceholder} (${cur.symbol.trim()})`;
      });
      const sym = document.getElementById("currency-symbol-label");
      if (sym) sym.innerText = `${cur.code} (${cur.symbol.trim()}) / ${periodLabel(displaySettings.wage_period)}`;
    }
    function applySettingsToControls() {
      const set = (id, value) => { const el = document.getElementById(id); if (el) el.value = value; };
      const select = document.getElementById("settings-currency");
      if (select && select.options.length !== currencyTable.length) {
        select.innerHTML = currencyTable.map(c => `<option value="${c.code}">${c.code} (${c.symbol.trim()})</option>`).join("");
      }
      set("settings-currency", displaySettings.currency);
      set("settings-measurement", displaySettings.measurement);
      set("settings-wage-period", displaySettings.wage_period);
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
    async function loadDisplaySettings() {
      try {
        const res = await fetch("/api/settings");
        const data = await res.json();
        if (res.ok) { applySettingsPayload(data); rebaseMoneyInputs(); }
      } catch (error) { /* keep GBP/metric defaults when the server cannot be reached */ }
    }
    // Re-show every money input in the new units without changing the raw value it represents.
    function rebaseMoneyInputs(previous) {
      document.querySelectorAll("input[data-money]").forEach(el => {
        const raw = previous ? previous(el) : (el.dataset.raw !== undefined ? Number(el.dataset.raw) : null);
        if (raw !== null && Number.isFinite(raw)) setMoneyInput(el, raw);
      });
    }
    async function saveDisplaySettings() {
      const read = id => document.getElementById(id).value;
      const body = { currency: read("settings-currency"), measurement: read("settings-measurement"), wage_period: read("settings-wage-period") };
      // Capture what each input means in raw units under the OLD settings before they change.
      const raws = new Map();
      document.querySelectorAll("input[data-money]").forEach(el => raws.set(el, readMoneyInput(el, null)));
      try {
        const res = await fetch("/api/settings", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
        const data = await res.json();
        if (!res.ok) throw new Error(data.message || "Settings were rejected");
        applySettingsPayload(data);
        rebaseMoneyInputs(el => raws.get(el));
        showToast("Display settings saved.", "success");
      } catch (error) {
        applySettingsToControls();
        showToast(error.message, "error");
      }
    }

