use std::collections::BTreeMap;
use std::fmt;

use anyhow::{bail, Context};
use object_store::ObjectStoreExt;
use serde_json::Value;

use super::store::StoreLocation;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ArrayMeta {
    pub name: String,
    pub shape: Vec<usize>,
    /// Data type using v3-style clean names (e.g. "float32", "int64").
    /// Normalised from numpy-style dtype strings for v2 stores.
    pub data_type: String,
    pub dims: Vec<String>,
    pub attrs: BTreeMap<String, Value>,
    pub chunks: Vec<usize>,
    pub fill_value: Option<Value>,

    // ── v3-only fields ──────────────────────────────────────────
    /// Ordered codec pipeline (v3). `None` for v2 stores.
    pub codecs: Option<Value>,

    // ── v2-only fields ──────────────────────────────────────────
    /// Compressor object (v2). `None` for v3 stores.
    pub compressor: Option<Value>,
    /// Memory layout order, "C" or "F" (v2). `None` for v3 stores.
    pub order: Option<String>,
    /// Filter pipeline (v2). `None` for v3 stores.
    pub filters: Option<Value>,
}

impl ArrayMeta {
    /// A coordinate array is one-dimensional and its single dimension
    /// name matches the array name (xarray convention).
    ///
    /// NOTE: For stores with nested groups (e.g. `group1/time`), `name`
    /// is the full path while `dims[0]` is the leaf name (`"time"`), so
    /// this check will not recognise nested coordinate arrays. Downstream
    /// lookups in info.rs and summary.rs also compare `a.name` against
    /// dimension names and would need similar treatment. Flat stores
    /// (the common case) are unaffected.
    pub fn is_coordinate(&self) -> bool {
        self.dims.len() == 1 && self.dims[0] == self.name
    }

    /// Return display-friendly dimension labels.
    /// Named dimensions keep their name; unnamed (empty-string) dimensions
    /// become `"dim_0"`, `"dim_1"`, etc.
    pub fn display_dims(&self) -> Vec<String> {
        self.dims
            .iter()
            .enumerate()
            .map(|(i, d)| {
                if d.is_empty() {
                    format!("dim_{i}")
                } else {
                    d.clone()
                }
            })
            .collect()
    }
}

#[derive(Debug)]
pub struct StoreMeta {
    pub zarr_format: u32,
    pub root_attrs: BTreeMap<String, Value>,
    pub arrays: Vec<ArrayMeta>,
}

/// Describes why fetching store metadata failed.
pub enum FetchError {
    /// The store or path was not found (404).
    NotFound(String),
    /// Credentials are missing or invalid (401).
    Unauthenticated(String),
    /// Credentials lack permission (403).
    PermissionDenied(String),
    /// Local path exists but has no .zmetadata file.
    NoConsolidatedMetadata(String),
    /// Any other error (network, DNS, parse, etc.).
    Other(anyhow::Error),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::NotFound(msg) => write!(f, "{msg}"),
            FetchError::Unauthenticated(msg) => write!(f, "{msg}"),
            FetchError::PermissionDenied(msg) => write!(f, "{msg}"),
            FetchError::NoConsolidatedMetadata(msg) => write!(f, "{msg}"),
            FetchError::Other(err) => write!(f, "{err:#}"),
        }
    }
}

impl fmt::Debug for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Fetch and parse consolidated metadata from a store.
///
/// Tries v3 (`zarr.json` with `consolidated_metadata`) first, then falls
/// back to v2 (`.zmetadata`).  Returns a typed `FetchError` so callers can
/// display provider-specific guidance for auth failures, missing stores, etc.
pub fn fetch_store_meta(
    location: &StoreLocation,
    runtime: &tokio::runtime::Runtime,
) -> Result<StoreMeta, FetchError> {
    // Try zarr.json first (v3)
    match fetch_raw_file(location, runtime, "zarr.json") {
        Ok(raw) => {
            match parse_zarr_json(&raw) {
                Ok(meta) => return Ok(meta),
                Err(e) => {
                    // If the zarr.json declares zarr_format >= 3, this is a v3
                    // store — don't silently fall through to .zmetadata.
                    if is_v3_json(&raw) {
                        return Err(FetchError::NoConsolidatedMetadata(format!(
                            "Found zarr.json (v3) in {} but failed to read consolidated metadata:\n\
                             {e:#}\n\n\
                             Zeph requires consolidated metadata.\n\
                             See https://zarr.readthedocs.io/en/latest/user-guide/consolidated_metadata/",
                            location.display_path(),
                        )));
                    }
                    // Not a v3 store (zarr_format < 3 or unparseable) — fall through to .zmetadata
                }
            }
        }
        Err(FetchError::NotFound(_) | FetchError::NoConsolidatedMetadata(_)) => {
            // zarr.json not found — try .zmetadata
        }
        Err(e) => {
            // Auth/permission errors apply to the whole store — propagate
            return Err(e);
        }
    }

    // Try .zmetadata (v2)
    match fetch_raw_file(location, runtime, ".zmetadata") {
        Ok(raw) => parse_zmetadata(&raw).map_err(FetchError::Other),
        Err(FetchError::NotFound(_) | FetchError::NoConsolidatedMetadata(_)) => {
            // Distinguish "v2 store without consolidated metadata" from "not a zarr store".
            // A v2 store root always has a .zgroup file.
            let has_zgroup = fetch_raw_file(location, runtime, ".zgroup").is_ok();
            if has_zgroup {
                Err(FetchError::NoConsolidatedMetadata(format!(
                    "Found a v2 zarr store at {} but no consolidated metadata (.zmetadata).\n\
                     Zeph requires consolidated metadata.\n\
                     See https://zarr.readthedocs.io/en/latest/user-guide/consolidated_metadata/",
                    location.display_path(),
                )))
            } else {
                Err(FetchError::NotFound(format!(
                    "No zarr store found at {}",
                    location.display_path(),
                )))
            }
        }
        Err(e) => Err(e),
    }
}

/// Fetch a single file from the store by name.
fn fetch_raw_file(
    location: &StoreLocation,
    runtime: &tokio::runtime::Runtime,
    filename: &str,
) -> Result<String, FetchError> {
    match location {
        StoreLocation::Local(store_path) => {
            let file_path = store_path.join(filename);
            std::fs::read_to_string(&file_path).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    FetchError::NotFound(format!(
                        "{} not found in {}",
                        filename,
                        store_path.display(),
                    ))
                } else {
                    FetchError::Other(
                        anyhow::Error::from(e)
                            .context(format!("Could not read {}", file_path.display())),
                    )
                }
            })
        }
        StoreLocation::Cloud {
            url,
            store,
            base_path,
        } => {
            let meta_path = base_path.child(filename);
            match runtime.block_on(store.get(&meta_path)) {
                Ok(result) => {
                    let bytes = runtime
                        .block_on(result.bytes())
                        .map_err(|e| FetchError::Other(anyhow::Error::from(e)))?;
                    String::from_utf8(bytes.to_vec()).map_err(|e| {
                        FetchError::Other(
                            anyhow::Error::from(e)
                                .context(format!("Remote {filename} is not valid UTF-8")),
                        )
                    })
                }
                Err(e) => Err(classify_cloud_error(e, url)),
            }
        }
    }
}

/// Map an object_store error to a FetchError with provider-specific guidance.
fn classify_cloud_error(err: object_store::Error, url: &str) -> FetchError {
    match err {
        object_store::Error::NotFound { .. } => {
            FetchError::NotFound(format!(
                "Store not found at {url}\n\
                 Check the URL is correct, or the store may not have consolidated metadata (.zmetadata)."
            ))
        }
        object_store::Error::Unauthenticated { .. } => {
            let guidance = auth_guidance(url);
            FetchError::Unauthenticated(format!(
                "Authentication required for {url}\n{guidance}"
            ))
        }
        object_store::Error::PermissionDenied { .. } => {
            FetchError::PermissionDenied(format!(
                "Permission denied for {url}\n\
                 Your credentials may not have access to this store."
            ))
        }
        other => FetchError::Other(anyhow::Error::from(other)),
    }
}

/// Return provider-specific credential guidance based on the URL scheme.
fn auth_guidance(url: &str) -> String {
    if url.starts_with("s3://") {
        "Set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY, or configure AWS_PROFILE.".to_string()
    } else if url.starts_with("gs://") {
        "Set GOOGLE_APPLICATION_CREDENTIALS, or run: gcloud auth application-default login"
            .to_string()
    } else if url.starts_with("az://") || url.contains(".blob.core.windows.net") {
        "Set AZURE_STORAGE_ACCOUNT_NAME and AZURE_STORAGE_ACCOUNT_KEY.".to_string()
    } else {
        "Check that your credentials are configured correctly.".to_string()
    }
}

/// Kept for backwards compatibility with existing call sites.
pub fn parse_store(
    location: &StoreLocation,
    runtime: &tokio::runtime::Runtime,
) -> anyhow::Result<StoreMeta> {
    fetch_store_meta(location, runtime).map_err(|e| anyhow::anyhow!("{e}"))
}

/// Quick check: does the raw zarr.json text declare zarr_format >= 3?
fn is_v3_json(raw: &str) -> bool {
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|v| v.get("zarr_format")?.as_u64())
        .is_some_and(|f| f >= 3)
}

/// Parse a v3 root zarr.json with inline consolidated metadata into a StoreMeta.
///
/// Returns `Ok` only when the JSON is a valid v3 group with a
/// `consolidated_metadata` section; otherwise returns an error.
fn parse_zarr_json(raw: &str) -> anyhow::Result<StoreMeta> {
    let top: Value = serde_json::from_str(raw).context("Invalid JSON in zarr.json")?;

    let zarr_format = top
        .get("zarr_format")
        .and_then(|v| v.as_u64())
        .context("Missing 'zarr_format' in zarr.json")? as u32;

    if zarr_format < 3 {
        bail!("zarr.json has zarr_format {zarr_format}, expected 3");
    }

    let node_type = top
        .get("node_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if node_type != "group" {
        bail!("Root zarr.json node_type is '{node_type}', expected 'group'");
    }

    // Root-level attributes
    let root_attrs: BTreeMap<String, Value> = top
        .get("attributes")
        .and_then(|v| v.as_object())
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    // Consolidated metadata
    let consolidated = top
        .get("consolidated_metadata")
        .context("No 'consolidated_metadata' in zarr.json")?;
    let entries = consolidated
        .get("metadata")
        .and_then(|v| v.as_object())
        .context("Missing 'metadata' in consolidated_metadata")?;

    let mut arrays = Vec::new();
    for (name, node) in entries {
        let nt = node.get("node_type").and_then(|v| v.as_str()).unwrap_or("");
        if nt != "array" {
            continue; // skip groups
        }

        let shape: Vec<usize> = node
            .get("shape")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_u64().map(|n| n as usize))
                    .collect()
            })
            .unwrap_or_default();

        let data_type = match node.get("data_type") {
            Some(v) if v.is_string() => v.as_str().unwrap_or("").to_string(),
            Some(v) if v.is_object() => v.get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string(),
            _ => String::new(),
        };

        // v3 uses dimension_names directly (no _ARRAY_DIMENSIONS indirection).
        // Nulls are valid (unnamed dimensions) — map them to "" to keep
        // dims.len() == shape.len().
        let dims: Vec<String> = node
            .get("dimension_names")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|d| d.as_str().unwrap_or("").to_string())
                    .collect()
            })
            .unwrap_or_default();

        // Extract chunk shape from chunk_grid (only "regular" grids)
        let chunks: Vec<usize> = node
            .get("chunk_grid")
            .and_then(|cg| {
                let grid_name = cg.get("name")?.as_str()?;
                if grid_name == "regular" {
                    cg.get("configuration")?
                        .get("chunk_shape")?
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_u64().map(|n| n as usize))
                                .collect()
                        })
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let codecs = node.get("codecs").cloned();
        let fill_value = node.get("fill_value").cloned();

        // v3 attributes are inline
        let attrs: BTreeMap<String, Value> = node
            .get("attributes")
            .and_then(|v| v.as_object())
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();

        // `name` is the full consolidated-metadata key, which includes
        // the group path for nested arrays (e.g. "group1/temperature").
        // See is_coordinate() for a known limitation this causes.
        arrays.push(ArrayMeta {
            name: name.clone(),
            shape,
            data_type,
            dims,
            attrs,
            chunks,
            fill_value,
            codecs,
            compressor: None,
            order: None,
            filters: None,
        });
    }

    if arrays.is_empty() {
        bail!("No arrays found in store");
    }

    Ok(StoreMeta {
        zarr_format,
        root_attrs,
        arrays,
    })
}

/// Parse a raw .zmetadata JSON string into a StoreMeta.
fn parse_zmetadata(raw: &str) -> anyhow::Result<StoreMeta> {
    let top: Value = serde_json::from_str(raw).context("Invalid JSON in .zmetadata")?;
    let metadata = top
        .get("metadata")
        .and_then(|v| v.as_object())
        .context("Missing 'metadata' key in .zmetadata")?;

    // Parse zarr_format from .zgroup
    let zarr_format = metadata
        .get(".zgroup")
        .and_then(|v| v.get("zarr_format"))
        .and_then(|v| v.as_u64())
        .unwrap_or(2) as u32;

    // Parse root attrs
    let root_attrs: BTreeMap<String, Value> = metadata
        .get(".zattrs")
        .and_then(|v| v.as_object())
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    // Group entries by array name
    let mut zarray_map: BTreeMap<String, &Value> = BTreeMap::new();
    let mut zattrs_map: BTreeMap<String, &Value> = BTreeMap::new();

    for (key, val) in metadata {
        if let Some(name) = key.strip_suffix("/.zarray") {
            zarray_map.insert(name.to_string(), val);
        } else if let Some(name) = key.strip_suffix("/.zattrs") {
            if name != "" {
                zattrs_map.insert(name.to_string(), val);
            }
        }
    }

    let mut arrays = Vec::new();
    for (name, zarray_val) in &zarray_map {
        let shape: Vec<usize> = zarray_val
            .get("shape")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_u64().map(|n| n as usize))
                    .collect()
            })
            .unwrap_or_default();

        let data_type = zarray_val
            .get("dtype")
            .and_then(|v| v.as_str())
            .map(normalize_v2_dtype)
            .unwrap_or_default();

        let chunks: Vec<usize> = zarray_val
            .get("chunks")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_u64().map(|n| n as usize))
                    .collect()
            })
            .unwrap_or_default();

        let compressor = zarray_val
            .get("compressor")
            .filter(|v| !v.is_null())
            .cloned();

        let fill_value = zarray_val.get("fill_value").cloned();

        let order = zarray_val
            .get("order")
            .and_then(|v| v.as_str())
            .map(String::from);

        let filters = zarray_val
            .get("filters")
            .filter(|v| !v.is_null())
            .cloned();

        let mut attrs: BTreeMap<String, Value> = BTreeMap::new();
        let mut dims = Vec::new();

        if let Some(zattrs_val) = zattrs_map.get(name) {
            if let Some(obj) = zattrs_val.as_object() {
                for (k, v) in obj {
                    if k == "_ARRAY_DIMENSIONS" {
                        if let Some(arr) = v.as_array() {
                            dims = arr
                                .iter()
                                .filter_map(|d| d.as_str().map(String::from))
                                .collect();
                        }
                    } else {
                        attrs.insert(k.clone(), v.clone());
                    }
                }
            }
        }

        arrays.push(ArrayMeta {
            name: name.clone(),
            shape,
            data_type,
            dims,
            attrs,
            chunks,
            fill_value,
            codecs: None,
            compressor,
            order,
            filters,
        });
    }

    if arrays.is_empty() {
        bail!("No arrays found in store");
    }

    Ok(StoreMeta {
        zarr_format,
        root_attrs,
        arrays,
    })
}

/// Convert a v2 numpy-style dtype string to a v3-style clean name.
fn normalize_v2_dtype(dtype: &str) -> String {
    match dtype {
        "<f2" | ">f2" => "float16".to_string(),
        "<f4" | ">f4" => "float32".to_string(),
        "<f8" | ">f8" => "float64".to_string(),
        "<i2" | ">i2" => "int16".to_string(),
        "<i4" | ">i4" => "int32".to_string(),
        "<i8" | ">i8" => "int64".to_string(),
        "|i1" => "int8".to_string(),
        "<u1" | ">u1" | "|u1" => "uint8".to_string(),
        "<u2" | ">u2" => "uint16".to_string(),
        "<u4" | ">u4" => "uint32".to_string(),
        "<u8" | ">u8" => "uint64".to_string(),
        "|b1" => "bool".to_string(),
        "|S1" => "string".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write a .zmetadata JSON file into a temp dir and parse it.
    fn parse_json(json: &str) -> anyhow::Result<StoreMeta> {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".zmetadata"), json).unwrap();
        let location = StoreLocation::Local(dir.path().to_path_buf());
        let runtime = tokio::runtime::Runtime::new().unwrap();
        parse_store(&location, &runtime)
    }

    fn minimal_zmetadata(arrays_json: &str) -> String {
        format!(
            r#"{{
                "zarr_format": 2,
                "metadata": {{
                    ".zgroup": {{ "zarr_format": 2 }},
                    ".zattrs": {{}},
                    {}
                }}
            }}"#,
            arrays_json
        )
    }

    // --- zarr_format ---

    #[test]
    fn parse_zarr_format() {
        let json = minimal_zmetadata(
            r#""temperature/.zarray": { "shape": [365], "dtype": "<f4" }"#,
        );
        let meta = parse_json(&json).unwrap();
        assert_eq!(meta.zarr_format, 2);
    }

    #[test]
    fn parse_zarr_format_v3() {
        let json = r#"{
            "zarr_format": 3,
            "metadata": {
                ".zgroup": { "zarr_format": 3 },
                ".zattrs": {},
                "data/.zarray": { "shape": [10], "dtype": "<f4" }
            }
        }"#;
        let meta = parse_json(json).unwrap();
        assert_eq!(meta.zarr_format, 3);
    }

    #[test]
    fn parse_zarr_format_defaults_to_2() {
        let json = r#"{
            "zarr_format": 2,
            "metadata": {
                ".zattrs": {},
                "data/.zarray": { "shape": [10], "dtype": "<f4" }
            }
        }"#;
        let meta = parse_json(json).unwrap();
        assert_eq!(meta.zarr_format, 2);
    }

    // --- root_attrs ---

    #[test]
    fn parse_root_attrs_empty() {
        let json = minimal_zmetadata(
            r#""x/.zarray": { "shape": [5], "dtype": "<f4" }"#,
        );
        let meta = parse_json(&json).unwrap();
        assert!(meta.root_attrs.is_empty());
    }

    #[test]
    fn parse_root_attrs_populated() {
        let json = r#"{
            "zarr_format": 2,
            "metadata": {
                ".zgroup": { "zarr_format": 2 },
                ".zattrs": { "title": "Test Dataset", "version": 1 },
                "x/.zarray": { "shape": [5], "dtype": "<f4" }
            }
        }"#;
        let meta = parse_json(json).unwrap();
        assert_eq!(meta.root_attrs.len(), 2);
        assert_eq!(meta.root_attrs["title"], serde_json::json!("Test Dataset"));
        assert_eq!(meta.root_attrs["version"], serde_json::json!(1));
    }

    // --- array parsing ---

    #[test]
    fn parse_single_array() {
        let json = minimal_zmetadata(
            r#""temperature/.zarray": { "shape": [365, 180, 360], "dtype": "<f4" }"#,
        );
        let meta = parse_json(&json).unwrap();
        assert_eq!(meta.arrays.len(), 1);
        assert_eq!(meta.arrays[0].name, "temperature");
        assert_eq!(meta.arrays[0].shape, vec![365, 180, 360]);
        assert_eq!(meta.arrays[0].data_type, "float32");
    }

    #[test]
    fn parse_multiple_arrays() {
        let json = minimal_zmetadata(
            r#"
            "temperature/.zarray": { "shape": [365, 180, 360], "dtype": "<f4" },
            "pressure/.zarray": { "shape": [365, 180, 360], "dtype": "<f8" },
            "time/.zarray": { "shape": [365], "dtype": "<i8" }
            "#,
        );
        let meta = parse_json(&json).unwrap();
        assert_eq!(meta.arrays.len(), 3);
        let names: Vec<&str> = meta.arrays.iter().map(|a| a.name.as_str()).collect();
        assert!(names.contains(&"temperature"));
        assert!(names.contains(&"pressure"));
        assert!(names.contains(&"time"));
    }

    #[test]
    fn parse_no_arrays_errors() {
        let json = r#"{
            "zarr_format": 2,
            "metadata": {
                ".zgroup": { "zarr_format": 2 },
                ".zattrs": {}
            }
        }"#;
        let result = parse_json(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No arrays found"));
    }

    // --- dimensions from _ARRAY_DIMENSIONS ---

    #[test]
    fn parse_dimensions() {
        let json = minimal_zmetadata(
            r#"
            "temperature/.zarray": { "shape": [365, 180, 360], "dtype": "<f4" },
            "temperature/.zattrs": { "_ARRAY_DIMENSIONS": ["time", "lat", "lon"] }
            "#,
        );
        let meta = parse_json(&json).unwrap();
        assert_eq!(meta.arrays[0].dims, vec!["time", "lat", "lon"]);
    }

    #[test]
    fn parse_no_dimensions() {
        let json = minimal_zmetadata(
            r#""data/.zarray": { "shape": [100], "dtype": "<f4" }"#,
        );
        let meta = parse_json(&json).unwrap();
        assert!(meta.arrays[0].dims.is_empty());
    }

    // --- array attrs (non-dimension) ---

    #[test]
    fn parse_array_attrs_excludes_array_dimensions() {
        let json = minimal_zmetadata(
            r#"
            "temperature/.zarray": { "shape": [365], "dtype": "<f4" },
            "temperature/.zattrs": {
                "_ARRAY_DIMENSIONS": ["time"],
                "units": "K",
                "long_name": "Temperature"
            }
            "#,
        );
        let meta = parse_json(&json).unwrap();
        // _ARRAY_DIMENSIONS should not appear in attrs
        assert!(!meta.arrays[0].attrs.contains_key("_ARRAY_DIMENSIONS"));
        assert_eq!(meta.arrays[0].attrs["units"], serde_json::json!("K"));
        assert_eq!(
            meta.arrays[0].attrs["long_name"],
            serde_json::json!("Temperature")
        );
    }

    // --- storage fields (chunks, compressor, fill_value, order, filters) ---

    #[test]
    fn parse_chunks() {
        let json = minimal_zmetadata(
            r#""data/.zarray": { "shape": [100, 50], "dtype": "<f4", "chunks": [10, 25] }"#,
        );
        let meta = parse_json(&json).unwrap();
        assert_eq!(meta.arrays[0].chunks, vec![10, 25]);
    }

    #[test]
    fn parse_missing_chunks_defaults_empty() {
        let json = minimal_zmetadata(
            r#""data/.zarray": { "shape": [100], "dtype": "<f4" }"#,
        );
        let meta = parse_json(&json).unwrap();
        assert!(meta.arrays[0].chunks.is_empty());
    }

    #[test]
    fn parse_compressor_blosc() {
        let json = minimal_zmetadata(
            r#""data/.zarray": { "shape": [10], "dtype": "<f4", "compressor": { "id": "blosc", "cname": "lz4", "clevel": 5 } }"#,
        );
        let meta = parse_json(&json).unwrap();
        let comp = meta.arrays[0].compressor.as_ref().unwrap();
        assert_eq!(comp["id"], serde_json::json!("blosc"));
        assert_eq!(comp["cname"], serde_json::json!("lz4"));
    }

    #[test]
    fn parse_null_compressor() {
        let json = minimal_zmetadata(
            r#""data/.zarray": { "shape": [10], "dtype": "<f4", "compressor": null }"#,
        );
        let meta = parse_json(&json).unwrap();
        assert!(meta.arrays[0].compressor.is_none());
    }

    #[test]
    fn parse_missing_compressor() {
        let json = minimal_zmetadata(
            r#""data/.zarray": { "shape": [10], "dtype": "<f4" }"#,
        );
        let meta = parse_json(&json).unwrap();
        assert!(meta.arrays[0].compressor.is_none());
    }

    #[test]
    fn parse_fill_value_string() {
        let json = minimal_zmetadata(
            r#""data/.zarray": { "shape": [10], "dtype": "<f4", "fill_value": "NaN" }"#,
        );
        let meta = parse_json(&json).unwrap();
        assert_eq!(meta.arrays[0].fill_value, Some(serde_json::json!("NaN")));
    }

    #[test]
    fn parse_fill_value_null() {
        let json = minimal_zmetadata(
            r#""data/.zarray": { "shape": [10], "dtype": "<f4", "fill_value": null }"#,
        );
        let meta = parse_json(&json).unwrap();
        assert_eq!(meta.arrays[0].fill_value, Some(serde_json::Value::Null));
    }

    #[test]
    fn parse_fill_value_numeric() {
        let json = minimal_zmetadata(
            r#""data/.zarray": { "shape": [10], "dtype": "<f4", "fill_value": 0 }"#,
        );
        let meta = parse_json(&json).unwrap();
        assert_eq!(meta.arrays[0].fill_value, Some(serde_json::json!(0)));
    }

    #[test]
    fn parse_missing_fill_value() {
        let json = minimal_zmetadata(
            r#""data/.zarray": { "shape": [10], "dtype": "<f4" }"#,
        );
        let meta = parse_json(&json).unwrap();
        assert!(meta.arrays[0].fill_value.is_none());
    }

    #[test]
    fn parse_order() {
        let json = minimal_zmetadata(
            r#""data/.zarray": { "shape": [10], "dtype": "<f4", "order": "F" }"#,
        );
        let meta = parse_json(&json).unwrap();
        assert_eq!(meta.arrays[0].order, Some("F".to_string()));
    }

    #[test]
    fn parse_missing_order() {
        let json = minimal_zmetadata(
            r#""data/.zarray": { "shape": [10], "dtype": "<f4" }"#,
        );
        let meta = parse_json(&json).unwrap();
        assert!(meta.arrays[0].order.is_none());
    }

    #[test]
    fn parse_null_filters() {
        let json = minimal_zmetadata(
            r#""data/.zarray": { "shape": [10], "dtype": "<f4", "filters": null }"#,
        );
        let meta = parse_json(&json).unwrap();
        assert!(meta.arrays[0].filters.is_none());
    }

    #[test]
    fn parse_missing_filters() {
        let json = minimal_zmetadata(
            r#""data/.zarray": { "shape": [10], "dtype": "<f4" }"#,
        );
        let meta = parse_json(&json).unwrap();
        assert!(meta.arrays[0].filters.is_none());
    }

    // --- missing/empty shape and dtype ---

    #[test]
    fn parse_missing_shape_defaults_empty() {
        let json = minimal_zmetadata(
            r#""data/.zarray": { "dtype": "<f4" }"#,
        );
        let meta = parse_json(&json).unwrap();
        assert!(meta.arrays[0].shape.is_empty());
    }

    #[test]
    fn parse_missing_dtype_defaults_empty() {
        let json = minimal_zmetadata(
            r#""data/.zarray": { "shape": [10] }"#,
        );
        let meta = parse_json(&json).unwrap();
        assert_eq!(meta.arrays[0].data_type, "");
    }

    // --- invalid JSON ---

    #[test]
    fn parse_invalid_json_errors() {
        let result = parse_json("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn parse_missing_metadata_key_errors() {
        let result = parse_json(r#"{ "zarr_format": 2 }"#);
        assert!(result.is_err());
    }

    // --- v3 zarr.json parsing ---

    fn minimal_v3(arrays_json: &str) -> String {
        format!(
            r#"{{
                "zarr_format": 3,
                "node_type": "group",
                "attributes": {{}},
                "consolidated_metadata": {{
                    "kind": "inline",
                    "must_understand": false,
                    "metadata": {{
                        {}
                    }}
                }}
            }}"#,
            arrays_json
        )
    }

    #[test]
    fn parse_v3_single_array() {
        let json = minimal_v3(r#"
            "temperature": {
                "zarr_format": 3,
                "node_type": "array",
                "shape": [365, 180, 360],
                "data_type": "float32",
                "chunk_grid": {"name": "regular", "configuration": {"chunk_shape": [100, 90, 90]}},
                "codecs": [{"name": "bytes", "configuration": {"endian": "little"}}],
                "fill_value": "NaN",
                "dimension_names": ["time", "lat", "lon"],
                "attributes": {"units": "K"}
            }
        "#);
        let meta = parse_zarr_json(&json).unwrap();
        assert_eq!(meta.zarr_format, 3);
        assert_eq!(meta.arrays.len(), 1);

        let arr = &meta.arrays[0];
        assert_eq!(arr.name, "temperature");
        assert_eq!(arr.shape, vec![365, 180, 360]);
        assert_eq!(arr.data_type, "float32");
        assert_eq!(arr.dims, vec!["time", "lat", "lon"]);
        assert_eq!(arr.chunks, vec![100, 90, 90]);
        assert!(arr.codecs.is_some());
        assert_eq!(arr.fill_value, Some(serde_json::json!("NaN")));
        assert_eq!(arr.attrs["units"], serde_json::json!("K"));
        // v2-only fields are None
        assert!(arr.compressor.is_none());
        assert!(arr.order.is_none());
        assert!(arr.filters.is_none());
    }

    #[test]
    fn parse_v3_multiple_arrays() {
        let json = minimal_v3(r#"
            "temperature": {
                "zarr_format": 3, "node_type": "array",
                "shape": [365, 180], "data_type": "float32",
                "chunk_grid": {"name": "regular", "configuration": {"chunk_shape": [100, 90]}},
                "codecs": [], "fill_value": 0, "attributes": {}
            },
            "time": {
                "zarr_format": 3, "node_type": "array",
                "shape": [365], "data_type": "int64",
                "chunk_grid": {"name": "regular", "configuration": {"chunk_shape": [365]}},
                "codecs": [], "fill_value": 0, "dimension_names": ["time"],
                "attributes": {}
            }
        "#);
        let meta = parse_zarr_json(&json).unwrap();
        assert_eq!(meta.arrays.len(), 2);
        let names: Vec<&str> = meta.arrays.iter().map(|a| a.name.as_str()).collect();
        assert!(names.contains(&"temperature"));
        assert!(names.contains(&"time"));
    }

    #[test]
    fn parse_v3_skips_groups() {
        let json = minimal_v3(r#"
            "data": {
                "zarr_format": 3, "node_type": "array",
                "shape": [10], "data_type": "float32",
                "chunk_grid": {"name": "regular", "configuration": {"chunk_shape": [10]}},
                "codecs": [], "fill_value": 0, "attributes": {}
            },
            "subgroup": {
                "zarr_format": 3, "node_type": "group",
                "attributes": {},
                "consolidated_metadata": {"kind": "inline", "must_understand": false, "metadata": {}}
            }
        "#);
        let meta = parse_zarr_json(&json).unwrap();
        assert_eq!(meta.arrays.len(), 1);
        assert_eq!(meta.arrays[0].name, "data");
    }

    #[test]
    fn parse_v3_dimension_names() {
        let json = minimal_v3(r#"
            "temp": {
                "zarr_format": 3, "node_type": "array",
                "shape": [10, 20], "data_type": "float64",
                "chunk_grid": {"name": "regular", "configuration": {"chunk_shape": [10, 20]}},
                "codecs": [], "fill_value": 0,
                "dimension_names": ["x", "y"],
                "attributes": {}
            }
        "#);
        let meta = parse_zarr_json(&json).unwrap();
        assert_eq!(meta.arrays[0].dims, vec!["x", "y"]);
    }

    #[test]
    fn parse_v3_null_dimension_names() {
        let json = minimal_v3(r#"
            "temp": {
                "zarr_format": 3, "node_type": "array",
                "shape": [10, 20, 30], "data_type": "float64",
                "chunk_grid": {"name": "regular", "configuration": {"chunk_shape": [10, 20, 30]}},
                "codecs": [], "fill_value": 0,
                "dimension_names": ["time", null, "lon"],
                "attributes": {}
            }
        "#);
        let meta = parse_zarr_json(&json).unwrap();
        // Null entries become "" to keep dims.len() == shape.len()
        assert_eq!(meta.arrays[0].dims, vec!["time", "", "lon"]);
        assert_eq!(meta.arrays[0].dims.len(), meta.arrays[0].shape.len());
        // display_dims replaces "" with positional labels
        assert_eq!(meta.arrays[0].display_dims(), vec!["time", "dim_1", "lon"]);
    }

    #[test]
    fn parse_v3_no_dimension_names() {
        let json = minimal_v3(r#"
            "data": {
                "zarr_format": 3, "node_type": "array",
                "shape": [10], "data_type": "float32",
                "chunk_grid": {"name": "regular", "configuration": {"chunk_shape": [10]}},
                "codecs": [], "fill_value": 0, "attributes": {}
            }
        "#);
        let meta = parse_zarr_json(&json).unwrap();
        assert!(meta.arrays[0].dims.is_empty());
    }

    #[test]
    fn parse_v3_root_attrs() {
        let json = r#"{
            "zarr_format": 3,
            "node_type": "group",
            "attributes": {"title": "Test Dataset", "version": 2},
            "consolidated_metadata": {
                "kind": "inline", "must_understand": false,
                "metadata": {
                    "x": {
                        "zarr_format": 3, "node_type": "array",
                        "shape": [5], "data_type": "float32",
                        "chunk_grid": {"name": "regular", "configuration": {"chunk_shape": [5]}},
                        "codecs": [], "fill_value": 0, "attributes": {}
                    }
                }
            }
        }"#;
        let meta = parse_zarr_json(json).unwrap();
        assert_eq!(meta.root_attrs.len(), 2);
        assert_eq!(meta.root_attrs["title"], serde_json::json!("Test Dataset"));
        assert_eq!(meta.root_attrs["version"], serde_json::json!(2));
    }

    #[test]
    fn parse_v3_codecs_preserved() {
        let json = minimal_v3(r#"
            "data": {
                "zarr_format": 3, "node_type": "array",
                "shape": [100], "data_type": "float32",
                "chunk_grid": {"name": "regular", "configuration": {"chunk_shape": [100]}},
                "codecs": [
                    {"name": "bytes", "configuration": {"endian": "little"}},
                    {"name": "zstd", "configuration": {"level": 3, "checksum": false}}
                ],
                "fill_value": 0, "attributes": {}
            }
        "#);
        let meta = parse_zarr_json(&json).unwrap();
        let codecs = meta.arrays[0].codecs.as_ref().unwrap();
        let arr = codecs.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["name"], "bytes");
        assert_eq!(arr[1]["name"], "zstd");
        assert_eq!(arr[1]["configuration"]["level"], 3);
    }

    #[test]
    fn parse_v3_no_arrays_errors() {
        let json = minimal_v3(r#"
            "subgroup": {
                "zarr_format": 3, "node_type": "group",
                "attributes": {},
                "consolidated_metadata": {"kind": "inline", "must_understand": false, "metadata": {}}
            }
        "#);
        let result = parse_zarr_json(&json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No arrays found"));
    }

    #[test]
    fn parse_v3_missing_consolidated_metadata_errors() {
        let json = r#"{
            "zarr_format": 3,
            "node_type": "group",
            "attributes": {}
        }"#;
        let result = parse_zarr_json(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("consolidated_metadata"));
    }

    #[test]
    fn parse_v3_non_regular_chunk_grid() {
        let json = minimal_v3(r#"
            "data": {
                "zarr_format": 3, "node_type": "array",
                "shape": [100], "data_type": "float32",
                "chunk_grid": {"name": "rectangular", "configuration": {}},
                "codecs": [], "fill_value": 0, "attributes": {}
            }
        "#);
        let meta = parse_zarr_json(&json).unwrap();
        assert!(meta.arrays[0].chunks.is_empty());
    }

    #[test]
    fn parse_v3_object_data_type() {
        let json = minimal_v3(r#"
            "timestamps": {
                "zarr_format": 3, "node_type": "array",
                "shape": [100], "data_type": {"name": "numpy.datetime64", "configuration": {"unit": "ns", "scale_factor": 1}},
                "chunk_grid": {"name": "regular", "configuration": {"chunk_shape": [100]}},
                "codecs": [], "fill_value": 0, "attributes": {}
            }
        "#);
        let meta = parse_zarr_json(&json).unwrap();
        assert_eq!(meta.arrays[0].data_type, "numpy.datetime64");
    }

    // --- fetch_store_meta error classification ---

    #[test]
    fn fetch_empty_dir_returns_not_found() {
        let dir = tempfile::tempdir().unwrap();
        // Directory exists but has no zarr files at all
        let location = StoreLocation::Local(dir.path().to_path_buf());
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let result = fetch_store_meta(&location, &runtime);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, FetchError::NotFound(_)),
            "Expected NotFound, got: {err}"
        );
        let msg = err.to_string();
        assert!(msg.contains("No zarr store found"), "Message: {msg}");
    }

    #[test]
    fn fetch_v2_store_without_consolidated_metadata() {
        let dir = tempfile::tempdir().unwrap();
        // v2 store root has .zgroup but no .zmetadata
        std::fs::write(
            dir.path().join(".zgroup"),
            r#"{"zarr_format": 2}"#,
        )
        .unwrap();
        let location = StoreLocation::Local(dir.path().to_path_buf());
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let result = fetch_store_meta(&location, &runtime);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, FetchError::NoConsolidatedMetadata(_)),
            "Expected NoConsolidatedMetadata, got: {err}"
        );
        let msg = err.to_string();
        assert!(msg.contains("no consolidated metadata"), "Message: {msg}");
    }

    #[test]
    fn fetch_valid_store_returns_meta() {
        let dir = tempfile::tempdir().unwrap();
        let json = minimal_zmetadata(
            r#""data/.zarray": { "shape": [10], "dtype": "<f4" }"#,
        );
        std::fs::write(dir.path().join(".zmetadata"), json).unwrap();
        let location = StoreLocation::Local(dir.path().to_path_buf());
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let meta = fetch_store_meta(&location, &runtime).unwrap();
        assert_eq!(meta.arrays.len(), 1);
        assert_eq!(meta.arrays[0].name, "data");
    }

    #[test]
    fn fetch_v3_store_returns_meta() {
        let dir = tempfile::tempdir().unwrap();
        let json = minimal_v3(r#"
            "data": {
                "zarr_format": 3, "node_type": "array",
                "shape": [10], "data_type": "float32",
                "chunk_grid": {"name": "regular", "configuration": {"chunk_shape": [10]}},
                "codecs": [], "fill_value": 0, "attributes": {}
            }
        "#);
        std::fs::write(dir.path().join("zarr.json"), json).unwrap();
        let location = StoreLocation::Local(dir.path().to_path_buf());
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let meta = fetch_store_meta(&location, &runtime).unwrap();
        assert_eq!(meta.zarr_format, 3);
        assert_eq!(meta.arrays.len(), 1);
        assert_eq!(meta.arrays[0].name, "data");
    }

    #[test]
    fn fetch_prefers_v3_over_v2() {
        let dir = tempfile::tempdir().unwrap();
        // Write both zarr.json (v3) and .zmetadata (v2)
        let v3_json = minimal_v3(r#"
            "v3data": {
                "zarr_format": 3, "node_type": "array",
                "shape": [10], "data_type": "float32",
                "chunk_grid": {"name": "regular", "configuration": {"chunk_shape": [10]}},
                "codecs": [], "fill_value": 0, "attributes": {}
            }
        "#);
        let v2_json = minimal_zmetadata(
            r#""v2data/.zarray": { "shape": [10], "dtype": "<f4" }"#,
        );
        std::fs::write(dir.path().join("zarr.json"), v3_json).unwrap();
        std::fs::write(dir.path().join(".zmetadata"), v2_json).unwrap();
        let location = StoreLocation::Local(dir.path().to_path_buf());
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let meta = fetch_store_meta(&location, &runtime).unwrap();
        assert_eq!(meta.zarr_format, 3);
        assert_eq!(meta.arrays[0].name, "v3data");
    }

    // --- auth_guidance ---

    #[test]
    fn auth_guidance_s3() {
        let guidance = auth_guidance("s3://bucket/path");
        assert!(guidance.contains("AWS_ACCESS_KEY_ID"));
        assert!(guidance.contains("AWS_PROFILE"));
    }

    #[test]
    fn auth_guidance_gcs() {
        let guidance = auth_guidance("gs://bucket/path");
        assert!(guidance.contains("GOOGLE_APPLICATION_CREDENTIALS"));
    }

    #[test]
    fn auth_guidance_azure() {
        let guidance = auth_guidance("az://container/path");
        assert!(guidance.contains("AZURE_STORAGE_ACCOUNT_NAME"));
    }

    #[test]
    fn auth_guidance_azure_https() {
        let guidance = auth_guidance("https://account.blob.core.windows.net/container/path");
        assert!(guidance.contains("AZURE_STORAGE_ACCOUNT_NAME"));
    }

    #[test]
    fn auth_guidance_unknown() {
        let guidance = auth_guidance("https://example.com/data");
        assert!(guidance.contains("credentials"));
    }
}
