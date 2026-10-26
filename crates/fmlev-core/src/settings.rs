//! Display settings: currency, measurement system and wage period.
//!
//! Settings only change how the UI shows and parses values. Native storage stays raw GBP
//! (weekly for wages) and native units, so nothing here ever writes game memory.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::{CoreError, Result};

/// Date the shipped rate table was set. Rates are fixed, approximate and never fetched.
pub const RATES_DATE: &str = "2026-10-26";

/// Weeks per year used for weekly -> yearly conversion (FM convention).
pub const WEEKS_PER_YEAR: f64 = 52.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Currency {
    pub code: &'static str,
    pub symbol: &'static str,
    pub decimals: u8,
    /// Units of this currency per 1 GBP.
    pub rate_per_gbp: f64,
}

pub const CURRENCIES: &[Currency] = &[
    Currency { code: "GBP", symbol: "£", decimals: 0, rate_per_gbp: 1.0 },
    Currency { code: "EUR", symbol: "€", decimals: 0, rate_per_gbp: 1.17 },
    Currency { code: "USD", symbol: "$", decimals: 0, rate_per_gbp: 1.3508 },
    Currency { code: "AUD", symbol: "A$", decimals: 0, rate_per_gbp: 2.05 },
    Currency { code: "CAD", symbol: "C$", decimals: 0, rate_per_gbp: 1.88 },
    Currency { code: "CHF", symbol: "CHF ", decimals: 0, rate_per_gbp: 1.07 },
    Currency { code: "BRL", symbol: "R$", decimals: 0, rate_per_gbp: 7.3 },
    Currency { code: "JPY", symbol: "¥", decimals: 0, rate_per_gbp: 202.0 },
    Currency { code: "IDR", symbol: "Rp ", decimals: 0, rate_per_gbp: 22400.0 },
];

pub fn currency(code: &str) -> Option<&'static Currency> {
    CURRENCIES.iter().find(|c| c.code == code)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Measurement {
    Metric,
    Imperial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WagePeriod {
    Weekly,
    Monthly,
    Yearly,
}

impl WagePeriod {
    /// Display amount per period = weekly amount x this factor.
    pub fn factor(self) -> f64 {
        match self {
            WagePeriod::Weekly => 1.0,
            WagePeriod::Yearly => WEEKS_PER_YEAR,
            WagePeriod::Monthly => WEEKS_PER_YEAR / 12.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplaySettings {
    pub currency: String,
    pub measurement: Measurement,
    pub wage_period: WagePeriod,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            currency: "GBP".into(),
            measurement: Measurement::Metric,
            wage_period: WagePeriod::Weekly,
        }
    }
}

impl DisplaySettings {
    pub fn validate(&self) -> Result<()> {
        if currency(&self.currency).is_none() {
            return Err(CoreError::InvalidValue(format!("Unknown currency '{}'", self.currency)));
        }
        Ok(())
    }

    /// Reads settings from `path`; a missing file gives defaults, a corrupt or invalid one is an error.
    pub fn load_from(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let settings: Self = serde_json::from_str(&text)
                    .map_err(|e| CoreError::InvalidValue(format!("Invalid settings file: {e}")))?;
                settings.validate()?;
                Ok(settings)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(CoreError::InvalidValue(format!("Cannot read settings: {e}"))),
        }
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        self.validate()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| CoreError::InvalidValue(format!("Cannot create settings dir: {e}")))?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| CoreError::InvalidValue(format!("Cannot encode settings: {e}")))?;
        std::fs::write(path, text).map_err(|e| CoreError::InvalidValue(format!("Cannot write settings: {e}")))
    }
}

/// `${XDG_CONFIG_HOME:-~/.config}/fmlev/settings.json`.
pub fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("fmlev").join("settings.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fmlev-settings-{}-{name}", std::process::id())).join("settings.json")
    }

    #[test]
    fn missing_file_gives_defaults() {
        let loaded = DisplaySettings::load_from(&temp_path("missing")).unwrap();
        assert_eq!(loaded, DisplaySettings::default());
        assert_eq!(loaded.currency, "GBP");
    }

    #[test]
    fn save_and_load_round_trip() {
        let path = temp_path("roundtrip");
        let settings = DisplaySettings {
            currency: "IDR".into(),
            measurement: Measurement::Imperial,
            wage_period: WagePeriod::Monthly,
        };
        settings.save_to(&path).unwrap();
        assert_eq!(DisplaySettings::load_from(&path).unwrap(), settings);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn rejects_unknown_currency_and_fields() {
        let bad = DisplaySettings { currency: "XXX".into(), ..Default::default() };
        assert!(bad.validate().is_err());
        assert!(bad.save_to(&temp_path("bad")).is_err());
        assert!(serde_json::from_str::<DisplaySettings>(
            r#"{"currency":"GBP","measurement":"metric","wage_period":"weekly","rate":2}"#
        )
        .is_err());
        assert!(serde_json::from_str::<DisplaySettings>(
            r#"{"currency":"GBP","measurement":"metric","wage_period":"daily"}"#
        )
        .is_err());
    }

    #[test]
    fn corrupt_file_is_an_error_not_defaults() {
        let path = temp_path("corrupt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{nope").unwrap();
        assert!(DisplaySettings::load_from(&path).is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn every_rate_is_positive_and_codes_unique() {
        for (i, c) in CURRENCIES.iter().enumerate() {
            assert!(c.rate_per_gbp > 0.0, "{}", c.code);
            assert!(CURRENCIES[i + 1..].iter().all(|o| o.code != c.code));
        }
        assert!((WagePeriod::Monthly.factor() * 12.0 - 52.0).abs() < 1e-9);
    }
}
