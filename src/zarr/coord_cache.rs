use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{NaiveDate, NaiveDateTime};
use serde_json::Value;
use zarrs::array::{Array, ArraySubset};
use zarrs_storage::AsyncReadableStorageTraits;

use super::metadata::ArrayMeta;

/// Decoded values of a single coordinate array.
#[derive(Clone, Debug)]
pub enum CoordValues {
    Float32(Vec<f32>),
    Float64(Vec<f64>),
    Int32(Vec<i32>),
    Int64(Vec<i64>),
    /// CF-decoded datetime strings (e.g. "2020-01-01", "2020-01-01T06:00:00").
    Datetime(Vec<String>),
}

/// Status of a single coordinate fetch.
#[derive(Clone, Debug)]
pub enum CoordEntry {
    Pending,
    Ready(CoordValues),
    Failed(String),
}

/// Thread-safe coordinate value cache, shared between the background
/// prefetch task and the command handlers.
pub struct CoordCache {
    inner: Mutex<HashMap<String, CoordEntry>>,
}

impl CoordCache {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    fn set(&self, name: &str, entry: CoordEntry) {
        self.inner.lock().unwrap().insert(name.to_string(), entry);
    }

    /// Retrieve a coordinate entry, waiting briefly if it is still pending.
    pub fn get_or_wait(&self, name: &str, timeout: Duration) -> Option<CoordEntry> {
        let start = Instant::now();
        loop {
            let guard = self.inner.lock().unwrap();
            match guard.get(name) {
                Some(CoordEntry::Pending) => {
                    drop(guard);
                    if start.elapsed() >= timeout {
                        return Some(CoordEntry::Pending);
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Some(entry) => return Some(entry.clone()),
                None => return None,
            }
        }
    }
}

/// Prefetch all coordinate arrays in the background.
///
/// `store` must be rooted at the zarr store root so that
/// `Array::async_open(store, "/time")` finds `time/.zarray`.
pub async fn prefetch_coordinates(
    cache: Arc<CoordCache>,
    coord_metas: Vec<ArrayMeta>,
    store: Arc<dyn AsyncReadableStorageTraits>,
) {
    // Mark all as pending first
    for meta in &coord_metas {
        cache.set(&meta.name, CoordEntry::Pending);
    }

    // Fetch all in parallel
    let mut handles = Vec::new();
    for meta in coord_metas {
        let cache = cache.clone();
        let store = store.clone();
        handles.push(tokio::spawn(async move {
            let path = format!("/{}", meta.name);
            match fetch_one(&store, &path, &meta.dtype).await {
                Ok(values) => {
                    let values = try_decode_cf_time(values, &meta.attrs);
                    cache.set(&meta.name, CoordEntry::Ready(values));
                }
                Err(e) => cache.set(&meta.name, CoordEntry::Failed(format!("{e:#}"))),
            }
        }));
    }

    for h in handles {
        let _ = h.await;
    }
}

async fn fetch_one(
    store: &Arc<dyn AsyncReadableStorageTraits>,
    path: &str,
    dtype: &str,
) -> anyhow::Result<CoordValues> {
    let array = Array::async_open(store.clone(), path).await?;
    let shape = array.shape();
    let len = shape[0];
    let subset = ArraySubset::new_with_ranges(&[0..len]);

    match dtype {
        "<f4" | ">f4" => {
            let data: Vec<f32> = array
                .async_retrieve_array_subset::<Vec<_>>(&subset)
                .await?;
            Ok(CoordValues::Float32(data))
        }
        "<f8" | ">f8" => {
            let data: Vec<f64> = array
                .async_retrieve_array_subset::<Vec<_>>(&subset)
                .await?;
            Ok(CoordValues::Float64(data))
        }
        "<i4" | ">i4" => {
            let data: Vec<i32> = array
                .async_retrieve_array_subset::<Vec<_>>(&subset)
                .await?;
            Ok(CoordValues::Int32(data))
        }
        "<i8" | ">i8" => {
            let data: Vec<i64> = array
                .async_retrieve_array_subset::<Vec<_>>(&subset)
                .await?;
            Ok(CoordValues::Int64(data))
        }
        other => anyhow::bail!("Unsupported coordinate dtype: {other}"),
    }
}

// ── Display helpers ────────────────────────────────────────────────

impl CoordValues {
    /// Format as "first, first, first, ..., last, last, last".
    pub fn format_summary(&self, head: usize, tail: usize) -> String {
        match self {
            CoordValues::Float32(v) => fmt_slice(v, head, tail),
            CoordValues::Float64(v) => fmt_slice(v, head, tail),
            CoordValues::Int32(v) => fmt_slice(v, head, tail),
            CoordValues::Int64(v) => fmt_slice(v, head, tail),
            CoordValues::Datetime(v) => fmt_slice(v, head, tail),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            CoordValues::Float32(v) => v.len(),
            CoordValues::Float64(v) => v.len(),
            CoordValues::Int32(v) => v.len(),
            CoordValues::Int64(v) => v.len(),
            CoordValues::Datetime(v) => v.len(),
        }
    }

    /// Returns true if the values were decoded from CF time conventions.
    pub fn is_datetime(&self) -> bool {
        matches!(self, CoordValues::Datetime(_))
    }
}

fn fmt_slice<T: std::fmt::Display>(values: &[T], head: usize, tail: usize) -> String {
    let len = values.len();
    if len <= head + tail {
        values
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        let head_vals: Vec<String> = values[..head].iter().map(|v| v.to_string()).collect();
        let tail_vals: Vec<String> = values[len - tail..].iter().map(|v| v.to_string()).collect();
        format!("{}, ..., {}", head_vals.join(", "), tail_vals.join(", "))
    }
}

// ── CF time decoding ─────────────────────────────────────────────

/// If the array has CF-convention time attributes (e.g. `units: "hours since 1900-01-01"`),
/// decode raw numeric values into formatted datetime strings.
fn try_decode_cf_time(values: CoordValues, attrs: &BTreeMap<String, Value>) -> CoordValues {
    let units = match attrs.get("units").and_then(|v| v.as_str()) {
        Some(u) => u,
        None => return values,
    };

    let parts: Vec<&str> = units.splitn(2, " since ").collect();
    if parts.len() != 2 {
        return values;
    }

    let multiplier = parts[0].trim();
    let epoch_str = parts[1].trim();

    // Nanoseconds per unit — use i64 for exact integer arithmetic
    let unit_nanos: i64 = match multiplier {
        "nanoseconds" | "nanosecond" => 1,
        "microseconds" | "microsecond" => 1_000,
        "milliseconds" | "millisecond" => 1_000_000,
        "seconds" | "second" => 1_000_000_000,
        "minutes" | "minute" => 60_000_000_000,
        "hours" | "hour" => 3_600_000_000_000,
        "days" | "day" => 86_400_000_000_000,
        _ => return values,
    };

    let epoch = match parse_cf_epoch(epoch_str) {
        Some(e) => e,
        None => return values,
    };

    match &values {
        CoordValues::Int64(vals) => {
            let datetimes: Vec<String> = vals
                .iter()
                .map(|&v| {
                    let delta = chrono::TimeDelta::nanoseconds(v.saturating_mul(unit_nanos));
                    format_datetime(epoch + delta)
                })
                .collect();
            CoordValues::Datetime(datetimes)
        }
        CoordValues::Float64(vals) => {
            let unit_secs = unit_nanos as f64 / 1e9;
            let datetimes: Vec<String> = vals
                .iter()
                .map(|&v| {
                    let total_secs = v * unit_secs;
                    let secs = total_secs.floor() as i64;
                    let nanos = ((total_secs - secs as f64) * 1e9).round() as u32;
                    let delta = chrono::TimeDelta::new(secs, nanos)
                        .unwrap_or(chrono::TimeDelta::seconds(secs));
                    format_datetime(epoch + delta)
                })
                .collect();
            CoordValues::Datetime(datetimes)
        }
        _ => values,
    }
}

/// Parse a CF epoch string like "1900-01-01" or "1970-01-01 00:00:00".
fn parse_cf_epoch(s: &str) -> Option<NaiveDateTime> {
    // Try date+time formats first, then date-only
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .ok()
        .or_else(|| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
        })
}

/// Format a datetime: date-only if midnight, otherwise include time.
fn format_datetime(dt: NaiveDateTime) -> String {
    if dt.time() == chrono::NaiveTime::MIN {
        dt.format("%Y-%m-%d").to_string()
    } else {
        dt.format("%Y-%m-%dT%H:%M:%S").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_summary_short_shows_all() {
        let vals = CoordValues::Int32(vec![1, 2, 3, 4]);
        assert_eq!(vals.format_summary(3, 3), "1, 2, 3, 4");
    }

    #[test]
    fn format_summary_long_shows_head_tail() {
        let vals = CoordValues::Float64(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]);
        assert_eq!(vals.format_summary(3, 3), "1, 2, 3, ..., 8, 9, 10");
    }

    #[test]
    fn format_summary_exact_boundary() {
        let vals = CoordValues::Int64(vec![10, 20, 30, 40, 50, 60]);
        // head + tail = 6, len = 6 → show all
        assert_eq!(vals.format_summary(3, 3), "10, 20, 30, 40, 50, 60");
    }

    #[test]
    fn coord_values_len() {
        let vals = CoordValues::Float32(vec![1.0, 2.0, 3.0]);
        assert_eq!(vals.len(), 3);
    }

    #[test]
    fn cache_set_and_get() {
        let cache = CoordCache::new();
        cache.set("time", CoordEntry::Ready(CoordValues::Int64(vec![1, 2, 3])));
        let entry = cache.get_or_wait("time", Duration::from_millis(10));
        assert!(matches!(entry, Some(CoordEntry::Ready(_))));
    }

    #[test]
    fn cache_missing_returns_none() {
        let cache = CoordCache::new();
        let entry = cache.get_or_wait("missing", Duration::from_millis(10));
        assert!(entry.is_none());
    }

    #[test]
    fn cache_pending_times_out() {
        let cache = CoordCache::new();
        cache.set("time", CoordEntry::Pending);
        let start = Instant::now();
        let entry = cache.get_or_wait("time", Duration::from_millis(50));
        assert!(matches!(entry, Some(CoordEntry::Pending)));
        assert!(start.elapsed() >= Duration::from_millis(50));
    }

    // --- CF time decoding ---

    fn make_attrs(units: &str) -> BTreeMap<String, Value> {
        let mut attrs = BTreeMap::new();
        attrs.insert("units".to_string(), Value::String(units.to_string()));
        attrs
    }

    #[test]
    fn cf_time_hours_since_1900() {
        // WeatherBench2 style: hours since 1900-01-01
        let values = CoordValues::Int64(vec![0, 6, 12]);
        let attrs = make_attrs("hours since 1900-01-01");
        let decoded = try_decode_cf_time(values, &attrs);
        match decoded {
            CoordValues::Datetime(dt) => {
                assert_eq!(dt, vec!["1900-01-01", "1900-01-01T06:00:00", "1900-01-01T12:00:00"]);
            }
            other => panic!("Expected Datetime, got {:?}", other),
        }
    }

    #[test]
    fn cf_time_days_since_epoch() {
        let values = CoordValues::Int64(vec![0, 1, 365]);
        let attrs = make_attrs("days since 1970-01-01");
        let decoded = try_decode_cf_time(values, &attrs);
        match decoded {
            CoordValues::Datetime(dt) => {
                assert_eq!(dt, vec!["1970-01-01", "1970-01-02", "1971-01-01"]);
            }
            other => panic!("Expected Datetime, got {:?}", other),
        }
    }

    #[test]
    fn cf_time_seconds_with_time_component() {
        let values = CoordValues::Int64(vec![0, 3600, 7200]);
        let attrs = make_attrs("seconds since 2020-01-01 00:00:00");
        let decoded = try_decode_cf_time(values, &attrs);
        match decoded {
            CoordValues::Datetime(dt) => {
                assert_eq!(dt, vec!["2020-01-01", "2020-01-01T01:00:00", "2020-01-01T02:00:00"]);
            }
            other => panic!("Expected Datetime, got {:?}", other),
        }
    }

    #[test]
    fn cf_time_float64_values() {
        let values = CoordValues::Float64(vec![0.0, 0.5, 1.0]);
        let attrs = make_attrs("days since 2020-01-01");
        let decoded = try_decode_cf_time(values, &attrs);
        match decoded {
            CoordValues::Datetime(dt) => {
                assert_eq!(dt, vec!["2020-01-01", "2020-01-01T12:00:00", "2020-01-02"]);
            }
            other => panic!("Expected Datetime, got {:?}", other),
        }
    }

    #[test]
    fn cf_time_no_units_attr_passthrough() {
        let values = CoordValues::Int64(vec![1, 2, 3]);
        let attrs = BTreeMap::new();
        let decoded = try_decode_cf_time(values, &attrs);
        assert!(matches!(decoded, CoordValues::Int64(_)));
    }

    #[test]
    fn cf_time_non_time_units_passthrough() {
        let values = CoordValues::Float64(vec![1.0, 2.0]);
        let attrs = make_attrs("meters");
        let decoded = try_decode_cf_time(values, &attrs);
        assert!(matches!(decoded, CoordValues::Float64(_)));
    }

    #[test]
    fn cf_time_int32_passthrough() {
        // Int32 time coords are uncommon; should pass through unchanged
        let values = CoordValues::Int32(vec![0, 1, 2]);
        let attrs = make_attrs("hours since 1900-01-01");
        let decoded = try_decode_cf_time(values, &attrs);
        assert!(matches!(decoded, CoordValues::Int32(_)));
    }

    #[test]
    fn cf_time_large_offset() {
        // 1,051,896 hours since 1900-01-01 = 2020-01-01 (43829 days × 24)
        let values = CoordValues::Int64(vec![1_051_896]);
        let attrs = make_attrs("hours since 1900-01-01");
        let decoded = try_decode_cf_time(values, &attrs);
        match decoded {
            CoordValues::Datetime(dt) => {
                assert_eq!(dt, vec!["2020-01-01"]);
            }
            other => panic!("Expected Datetime, got {:?}", other),
        }
    }

    #[test]
    fn cf_time_format_summary() {
        let vals = CoordValues::Datetime(vec![
            "2020-01-01".to_string(),
            "2020-01-02".to_string(),
            "2020-01-03".to_string(),
            "2020-01-04".to_string(),
            "2020-01-05".to_string(),
            "2020-01-06".to_string(),
            "2020-01-07".to_string(),
        ]);
        assert_eq!(
            vals.format_summary(2, 2),
            "2020-01-01, 2020-01-02, ..., 2020-01-06, 2020-01-07"
        );
    }

    #[test]
    fn parse_cf_epoch_date_only() {
        let dt = parse_cf_epoch("1900-01-01").unwrap();
        assert_eq!(dt.to_string(), "1900-01-01 00:00:00");
    }

    #[test]
    fn parse_cf_epoch_datetime() {
        let dt = parse_cf_epoch("1970-01-01 00:00:00").unwrap();
        assert_eq!(dt.to_string(), "1970-01-01 00:00:00");
    }

    #[test]
    fn parse_cf_epoch_fractional_seconds() {
        let dt = parse_cf_epoch("1900-01-01 00:00:0.0").unwrap();
        assert_eq!(dt.to_string(), "1900-01-01 00:00:00");
    }
}
