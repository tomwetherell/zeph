use std::collections::BTreeMap;

use anyhow::{bail, Context};
use object_store::ObjectStoreExt;
use serde_json::Value;

use super::store::StoreLocation;

#[derive(Debug)]
#[allow(dead_code)]
pub struct ArrayMeta {
    pub name: String,
    pub shape: Vec<usize>,
    pub dtype: String,
    pub dims: Vec<String>,
    pub attrs: BTreeMap<String, Value>,
}

#[derive(Debug)]
pub struct StoreMeta {
    pub zarr_format: u32,
    pub root_attrs: BTreeMap<String, Value>,
    pub arrays: Vec<ArrayMeta>,
}

pub fn parse_store(
    location: &StoreLocation,
    runtime: &tokio::runtime::Runtime,
) -> anyhow::Result<StoreMeta> {
    let raw = match location {
        StoreLocation::Local(store_path) => {
            let zmetadata_path = store_path.join(".zmetadata");
            std::fs::read_to_string(&zmetadata_path)
                .with_context(|| format!("Could not read {}", zmetadata_path.display()))?
        }
        StoreLocation::Cloud {
            store, base_path, ..
        } => {
            let meta_path = base_path.child(".zmetadata");
            let result = runtime
                .block_on(store.get(&meta_path))
                .with_context(|| format!("Could not read .zmetadata from remote store (path: {meta_path})"))?;
            let bytes = runtime
                .block_on(result.bytes())
                .context("Could not read bytes from remote .zmetadata")?;
            String::from_utf8(bytes.to_vec())
                .context("Remote .zmetadata is not valid UTF-8")?
        }
    };
    let top: Value = serde_json::from_str(&raw).context("Invalid JSON in .zmetadata")?;
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

        let dtype = zarray_val
            .get("dtype")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

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
            dtype,
            dims,
            attrs,
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
        assert_eq!(meta.arrays[0].dtype, "<f4");
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
        assert_eq!(meta.arrays[0].dtype, "");
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
}
