use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Context};
use serde_json::Value;

#[allow(dead_code)]
pub struct ArrayMeta {
    pub name: String,
    pub shape: Vec<usize>,
    pub dtype: String,
    pub dims: Vec<String>,
    pub attrs: BTreeMap<String, Value>,
}

pub struct StoreMeta {
    pub zarr_format: u32,
    pub root_attrs: BTreeMap<String, Value>,
    pub arrays: Vec<ArrayMeta>,
}

pub fn parse_store(store_path: &Path) -> anyhow::Result<StoreMeta> {
    let zmetadata_path = store_path.join(".zmetadata");
    let raw = std::fs::read_to_string(&zmetadata_path)
        .with_context(|| format!("Could not read {}", zmetadata_path.display()))?;
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
