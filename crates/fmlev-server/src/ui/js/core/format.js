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
    // Re-show every money input in the new units without changing the raw value it represents.
    function rebaseMoneyInputs(previous) {
      document.querySelectorAll("input[data-money]").forEach(el => {
        const raw = previous ? previous(el) : (el.dataset.raw !== undefined ? Number(el.dataset.raw) : null);
        if (raw !== null && Number.isFinite(raw)) setMoneyInput(el, raw);
      });
    }

    // ---- FM 1-20 value bands ----
    // valueBand(v) returns the CSS class for a rating: band-1-5, band-6-10, band-11-15, band-16-20, or band-none.
    // Pass `scale` for other ranges (valueBand(150, 200) maps onto 1-20).
    function valueBand(value, scale = 20) {
      if (value === null || value === undefined || value === "") return "band-none";
      const n = Number(value);
      if (!Number.isFinite(n) || n < 1 || n > scale) return "band-none";
      const v = scale === 20 ? n : (n * 20) / scale;
      return v <= 5 ? "band-1-5" : v <= 10 ? "band-6-10" : v <= 15 ? "band-11-15" : "band-16-20";
    }
    // "fm26" (default) or "legacy" (old red/grey/amber/green four tiers). Only the colours change.
    let bandScheme = "fm26";
    function applyBandScheme(name) {
      bandScheme = name === "legacy" ? "legacy" : "fm26";
      const body = typeof document !== "undefined" ? document.body : null;
      if (body) {
        if (body.dataset) body.dataset.bandScheme = bandScheme;
        else if (body.setAttribute) body.setAttribute("data-band-scheme", bandScheme);
      }
      return bandScheme;
    }
