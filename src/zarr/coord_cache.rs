use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
                Ok(values) => cache.set(&meta.name, CoordEntry::Ready(values)),
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
        }
    }

    pub fn len(&self) -> usize {
        match self {
            CoordValues::Float32(v) => v.len(),
            CoordValues::Float64(v) => v.len(),
            CoordValues::Int32(v) => v.len(),
            CoordValues::Int64(v) => v.len(),
        }
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
}
