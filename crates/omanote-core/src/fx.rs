//! Daily exchange rates for the calculator, from the European Central Bank
//! (frankfurter.app publishes them, no key, no account). The table is fetched at
//! most once a day and cached next to the database, so the calculator keeps
//! converting offline with the last known rates. This is the only request
//! Omanote makes to anything other than the user's own Joplin Server, and it
//! carries no note content.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The cached table lives next to the database.
pub const CACHE_FILE: &str = "fx-rates.json";
/// The ECB reference rates, published on working days around 16:00 CET.
/// (`api.frankfurter.app` redirects here.)
pub const SOURCE: &str = "https://api.frankfurter.dev/v1/latest?from=EUR";
/// A table older than this is fetched again.
pub const MAX_AGE_SECS: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FxRates {
    /// Publication date on the ECB side (`YYYY-MM-DD`).
    pub date: String,
    /// When this table was downloaded (unix seconds).
    pub fetched_at: i64,
    /// The currency the rates are quoted against. Always `EUR`.
    pub base: String,
    /// `code → rate`: one unit of `base` is worth this many.
    pub rates: BTreeMap<String, f64>,
}

#[derive(Deserialize)]
struct ApiResponse {
    date: String,
    base: String,
    rates: BTreeMap<String, f64>,
}

/// The ECB table as a JSON body (`fetched_at` is ours, not the server's).
pub fn parse(json: &str, fetched_at: i64) -> Result<FxRates> {
    let api: ApiResponse =
        serde_json::from_str(json).map_err(|e| Error::Format(format!("fx rates: {e}")))?;
    let base = api.base.trim().to_uppercase();
    if base != "EUR" {
        return Err(Error::Format(format!("fx rates: unexpected base {base}")));
    }
    let rates: BTreeMap<String, f64> = api
        .rates
        .into_iter()
        .filter(|(code, rate)| code.len() == 3 && *rate > 0.0)
        .map(|(code, rate)| (code.trim().to_uppercase(), rate))
        .collect();
    if rates.is_empty() {
        return Err(Error::Format("fx rates: empty table".into()));
    }
    Ok(FxRates { date: api.date, fetched_at, base, rates })
}

pub fn cache_path(dir: &Path) -> PathBuf {
    dir.join(CACHE_FILE)
}

/// The cached table, when the file is there and readable.
pub fn load_cached(dir: &Path) -> Option<FxRates> {
    let raw = std::fs::read_to_string(cache_path(dir)).ok()?;
    let rates: FxRates = serde_json::from_str(&raw).ok()?;
    (rates.base == "EUR" && !rates.rates.is_empty()).then_some(rates)
}

/// Best effort: a cache that cannot be written is not an error worth reporting.
pub fn save(dir: &Path, rates: &FxRates) -> bool {
    let Ok(body) = serde_json::to_string(rates) else { return false };
    let path = cache_path(dir);
    let tmp = path.with_extension("json.tmp");
    // Write then rename, so a half-written table is never read.
    std::fs::write(&tmp, body).is_ok() && std::fs::rename(&tmp, &path).is_ok()
}

pub fn is_fresh(rates: &FxRates, now_secs: i64) -> bool {
    now_secs - rates.fetched_at < MAX_AGE_SECS
}

/// A day is enough for the calculator; `fetched_at` is when we downloaded it.
pub async fn fetch() -> Result<FxRates> {
    let http = reqwest::Client::builder()
        .user_agent(concat!("Omanote/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let body = http.get(SOURCE).send().await?.error_for_status()?.text().await?;
    parse(&body, crate::item::now_ms() / 1000)
}

/// The table to use: the fresh cache, or a fresh download. When the download
/// fails the last cached table is returned with the reason it is stale.
pub async fn rates(dir: &Path, now_secs: i64) -> (Option<FxRates>, Option<String>) {
    let cached = load_cached(dir);
    if let Some(ref table) = cached {
        if is_fresh(table, now_secs) {
            return (cached, None);
        }
    }
    match fetch().await {
        Ok(fresh) => {
            save(dir, &fresh);
            (Some(fresh), None)
        }
        Err(e) => (cached, Some(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"amount":1.0,"base":"EUR","date":"2026-09-25",
        "rates":{"USD":1.08,"GBP":0.85,"JPY":170.2,"usd":1.09,"XX":9.9}}"#;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("omanote-fx-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parses_the_ecb_table() {
        let table = parse(SAMPLE, 1_700_000_000).unwrap();
        assert_eq!(table.base, "EUR");
        assert_eq!(table.date, "2026-09-25");
        assert_eq!(table.rates.get("USD"), Some(&1.09)); // "usd" folds onto USD
        assert_eq!(table.rates.get("JPY"), Some(&170.2));
        assert_eq!(table.rates.get("XX"), None); // not a currency code
        assert_eq!(table.rates.len(), 3);
    }

    #[test]
    fn rejects_what_it_cannot_use() {
        assert!(parse("not json", 0).is_err());
        assert!(parse(r#"{"base":"USD","date":"x","rates":{"EUR":1}}"#, 0).is_err());
        assert!(parse(r#"{"base":"EUR","date":"x","rates":{}}"#, 0).is_err());
        assert!(parse(r#"{"base":"EUR","date":"x","rates":{"USD":0}}"#, 0).is_err());
    }

    #[test]
    fn cache_round_trip_and_freshness() {
        let dir = scratch("cache");
        assert!(load_cached(&dir).is_none());
        let table = parse(SAMPLE, 1_000_000).unwrap();
        assert!(save(&dir, &table));
        assert_eq!(load_cached(&dir).as_ref(), Some(&table));
        assert!(is_fresh(&table, 1_000_000 + MAX_AGE_SECS - 1));
        assert!(!is_fresh(&table, 1_000_000 + MAX_AGE_SECS));
        // A broken file is ignored, not fatal.
        std::fs::write(cache_path(&dir), "{ truncated").unwrap();
        assert!(load_cached(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_cache_is_not_used() {
        let dir = scratch("empty");
        let mut empty = parse(SAMPLE, 1_000_000).unwrap();
        empty.rates.clear();
        assert!(save(&dir, &empty));
        assert!(load_cached(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
