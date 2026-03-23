use std::io::{self, Write};
use std::time::Duration;

use crossterm::style::{Print, ResetColor, SetForegroundColor};
use serde_json::Value;

use super::summary::{format_bytes, friendly_dtype};
use super::{CommandAction, CommandResult, Ctx};
use zeph::zarr::coord_cache::CoordEntry;
use zeph::zarr::metadata::ArrayMeta;

pub fn run(ctx: &Ctx, array: &ArrayMeta) -> CommandResult {
    let mut out = io::stdout();

    // Header: name (dim1: size1, dim2: size2, ...)
    let dims_str = if array.dims.is_empty() {
        String::new()
    } else {
        let display_dims = array.display_dims();
        let parts: Vec<String> = display_dims
            .iter()
            .zip(array.shape.iter())
            .map(|(d, s)| format!("{d}: {s}"))
            .collect();
        format!("  ({})", parts.join(", "))
    };
    let _ = crossterm::execute!(
        out,
        Print("  "),
        SetForegroundColor(ctx.palette.heading),
        Print(&array.name),
        ResetColor,
        Print(&dims_str),
        Print("\n"),
    );

    let _ = crossterm::execute!(out, Print("\n"));

    // Size
    let byte_size = dtype_byte_size(&array.data_type);
    let total_values: usize = array.shape.iter().product();
    let total_bytes = (total_values * byte_size) as u64;
    let label_width = 13;
    let _ = crossterm::execute!(
        out,
        SetForegroundColor(ctx.palette.heading),
        Print(format!("  {:<label_width$}", "Size:")),
        ResetColor,
        Print(format!(
            "{}  ({} values)\n",
            format_bytes(total_bytes),
            format_with_commas(total_values),
        )),
    );

    // Dtype
    let _ = crossterm::execute!(
        out,
        SetForegroundColor(ctx.palette.heading),
        Print(format!("  {:<label_width$}", "Dtype:")),
        ResetColor,
        Print(format!("{}\n", friendly_dtype(&array.data_type))),
    );

    // Fill value
    let _ = crossterm::execute!(
        out,
        SetForegroundColor(ctx.palette.heading),
        Print(format!("  {:<label_width$}", "Fill value:")),
        ResetColor,
        Print(format!("{}\n", format_fill_value(&array.fill_value))),
    );

    // Storage: v3 shows codecs, v2 shows order + compressor
    let sharding_config = array.codecs.as_ref().and_then(find_sharding_config);

    if let Some(ref codecs) = array.codecs {
        let display_codecs = sharding_config
            .and_then(|cfg| cfg.get("codecs"))
            .unwrap_or(codecs);
        let _ = crossterm::execute!(
            out,
            SetForegroundColor(ctx.palette.heading),
            Print(format!("  {:<label_width$}", "Codecs:")),
            ResetColor,
            Print(format!("{}\n", format_codecs(display_codecs))),
        );
    } else {
        if let Some(ref order) = array.order {
            let _ = crossterm::execute!(
                out,
                SetForegroundColor(ctx.palette.heading),
                Print(format!("  {:<label_width$}", "Order:")),
                ResetColor,
                Print(format!("{order}\n")),
            );
        }
        let _ = crossterm::execute!(
            out,
            SetForegroundColor(ctx.palette.heading),
            Print(format!("  {:<label_width$}", "Compressor:")),
            ResetColor,
            Print(format!("{}\n", format_compressor(&array.compressor))),
        );
    }

    // Shards + Chunks (sharded) or just Chunks (non-sharded)
    if !array.chunks.is_empty() {
        if let Some(cfg) = sharding_config {
            // --- Sharded: array.chunks are shard shapes ---
            let _ = crossterm::execute!(out, Print("\n"));

            let shard_tuple: Vec<String> = array.chunks.iter().map(|c| c.to_string()).collect();
            let _ = crossterm::execute!(
                out,
                SetForegroundColor(ctx.palette.heading),
                Print(format!("  {:<label_width$}", "Shards:")),
                ResetColor,
                Print(format!("({})\n", shard_tuple.join(", "))),
            );

            let shard_counts: Vec<usize> = array
                .shape
                .iter()
                .zip(array.chunks.iter())
                .map(|(&s, &c)| if c == 0 { 0 } else { s.div_ceil(c) })
                .collect();
            let total_shards: usize = shard_counts.iter().product();

            let shard_label: Vec<String> = if !array.dims.is_empty() {
                let display_dims = array.display_dims();
                display_dims
                    .iter()
                    .zip(shard_counts.iter())
                    .map(|(d, c)| format!("{d}: {c}"))
                    .collect()
            } else {
                shard_counts.iter().map(|c| c.to_string()).collect()
            };

            let shard_word = if total_shards == 1 { "shard" } else { "shards" };
            let _ = crossterm::execute!(
                out,
                Print(format!(
                    "  {:<label_width$}{total_shards} {shard_word}  [{}]\n",
                    "",
                    shard_label.join(", "),
                )),
            );

            let shard_values: usize = array.chunks.iter().product();
            let shard_bytes = (shard_values * byte_size) as u64;
            let _ = crossterm::execute!(
                out,
                Print(format!(
                    "  {:<label_width$}{} per shard\n",
                    "",
                    format_bytes(shard_bytes),
                )),
            );

            // Inner chunks from sharding config
            let inner_chunks: Vec<usize> = cfg
                .get("chunk_shape")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_u64().map(|n| n as usize))
                        .collect()
                })
                .unwrap_or_default();

            if !inner_chunks.is_empty() {
                let _ = crossterm::execute!(out, Print("\n"));

                let chunk_tuple: Vec<String> = inner_chunks.iter().map(|c| c.to_string()).collect();
                let _ = crossterm::execute!(
                    out,
                    SetForegroundColor(ctx.palette.heading),
                    Print(format!("  {:<label_width$}", "Chunks:")),
                    ResetColor,
                    Print(format!("({})\n", chunk_tuple.join(", "))),
                );

                let chunks_per_shard: Vec<usize> = array
                    .chunks
                    .iter()
                    .zip(inner_chunks.iter())
                    .map(|(&s, &c)| if c == 0 { 0 } else { s / c })
                    .collect();
                let total_per_shard: usize = chunks_per_shard.iter().product();

                let per_shard_label: Vec<String> = if !array.dims.is_empty() {
                    let display_dims = array.display_dims();
                    display_dims
                        .iter()
                        .zip(chunks_per_shard.iter())
                        .map(|(d, c)| format!("{d}: {c}"))
                        .collect()
                } else {
                    chunks_per_shard.iter().map(|c| c.to_string()).collect()
                };

                let chunk_word = if total_per_shard == 1 {
                    "chunk"
                } else {
                    "chunks"
                };
                let _ = crossterm::execute!(
                    out,
                    Print(format!(
                        "  {:<label_width$}{total_per_shard} {chunk_word} per shard  [{}]\n",
                        "",
                        per_shard_label.join(", "),
                    )),
                );

                let chunk_values: usize = inner_chunks.iter().product();
                let chunk_bytes = (chunk_values * byte_size) as u64;
                let _ = crossterm::execute!(
                    out,
                    Print(format!(
                        "  {:<label_width$}{} per chunk\n",
                        "",
                        format_bytes(chunk_bytes),
                    )),
                );
            }
        } else {
            // --- Non-sharded: original display ---
            let _ = crossterm::execute!(out, Print("\n"));

            let chunk_tuple: Vec<String> = array.chunks.iter().map(|c| c.to_string()).collect();
            let _ = crossterm::execute!(
                out,
                SetForegroundColor(ctx.palette.heading),
                Print(format!("  {:<label_width$}", "Chunks:")),
                ResetColor,
                Print(format!("({})\n", chunk_tuple.join(", "))),
            );

            let chunk_counts: Vec<usize> = array
                .shape
                .iter()
                .zip(array.chunks.iter())
                .map(|(&s, &c)| if c == 0 { 0 } else { s.div_ceil(c) })
                .collect();
            let total_chunks: usize = chunk_counts.iter().product();

            let chunk_label: Vec<String> = if !array.dims.is_empty() {
                let display_dims = array.display_dims();
                display_dims
                    .iter()
                    .zip(chunk_counts.iter())
                    .map(|(d, c)| format!("{d}: {c}"))
                    .collect()
            } else {
                chunk_counts.iter().map(|c| c.to_string()).collect()
            };

            let chunk_word = if total_chunks == 1 { "chunk" } else { "chunks" };
            let _ = crossterm::execute!(
                out,
                Print(format!(
                    "  {:<label_width$}{total_chunks} {chunk_word}  [{}]\n",
                    "",
                    chunk_label.join(", "),
                )),
            );

            let chunk_values: usize = array.chunks.iter().product();
            let chunk_bytes = (chunk_values * byte_size) as u64;
            let _ = crossterm::execute!(
                out,
                Print(format!(
                    "  {:<label_width$}{} per chunk\n",
                    "",
                    format_bytes(chunk_bytes),
                )),
            );
        }
    }

    // Coordinates — show values for dimensions that have coordinate arrays
    let coord_entries: Vec<(&ArrayMeta, Option<CoordEntry>)> = array
        .dims
        .iter()
        .filter_map(|dim_name| {
            ctx.meta
                .arrays
                .iter()
                .find(|a| a.is_coordinate() && a.name == *dim_name)
        })
        .map(|coord_arr| {
            let entry = ctx
                .coord_cache
                .get_or_wait(&coord_arr.name, Duration::from_millis(200));
            (coord_arr, entry)
        })
        .collect();

    if !coord_entries.is_empty() {
        let _ = crossterm::execute!(out, Print("\n"));
        let _ = crossterm::execute!(
            out,
            SetForegroundColor(ctx.palette.heading),
            Print("  Coordinates:\n"),
            ResetColor,
        );

        // Pre-compute display data for column alignment
        let rows: Vec<_> = coord_entries
            .iter()
            .map(|(coord_arr, entry)| {
                let size = coord_arr.shape.first().copied().unwrap_or(0);
                let size_str = format!("({size})");
                let dtype: &str = match entry {
                    Some(CoordEntry::Ready(vals)) if vals.is_datetime() => "datetime64",
                    _ => friendly_dtype(&coord_arr.data_type),
                };
                let values_str = match entry {
                    Some(CoordEntry::Ready(vals)) => vals.format_summary(3, 3),
                    Some(CoordEntry::Pending) => "loading...".to_string(),
                    Some(CoordEntry::Failed(_)) | None => String::new(),
                };
                (coord_arr, size_str, dtype, values_str)
            })
            .collect();

        let max_name = rows
            .iter()
            .map(|(a, _, _, _)| a.name.len())
            .max()
            .unwrap_or(0);
        let max_size = rows.iter().map(|(_, s, _, _)| s.len()).max().unwrap_or(0);
        let max_dtype = rows.iter().map(|(_, _, d, _)| d.len()).max().unwrap_or(0);

        // Column where values start: "      " + max_name + 2 + max_size + "  " + max_dtype + "   "
        let values_col = 6 + max_name + 2 + max_size + 2 + max_dtype + 3;
        let term_width = crossterm::terminal::size()
            .map(|(w, _)| w as usize)
            .unwrap_or(80);

        for (coord_arr, size_str, dtype, values_str) in &rows {
            let name_pad = max_name.saturating_sub(coord_arr.name.len()) + 2;
            let size_pad = max_size.saturating_sub(size_str.len());
            let dtype_pad = max_dtype.saturating_sub(dtype.len());

            let _ = crossterm::execute!(
                out,
                Print(format!("      {}{}", coord_arr.name, " ".repeat(name_pad))),
                SetForegroundColor(ctx.palette.dim),
                Print(format!("{size_str}{}  {dtype}", " ".repeat(size_pad))),
                ResetColor,
            );
            if !values_str.is_empty() {
                let wrapped = wrap_at_commas(
                    values_str,
                    term_width.saturating_sub(values_col),
                    values_col,
                );
                let _ = crossterm::execute!(
                    out,
                    Print(format!("{}   {wrapped}", " ".repeat(dtype_pad))),
                );
            }
            let _ = crossterm::execute!(out, Print("\n"));
        }
    }

    // Attributes
    let _ = crossterm::execute!(out, Print("\n"));
    let _ = crossterm::execute!(
        out,
        SetForegroundColor(ctx.palette.heading),
        Print("  Attributes:\n"),
        ResetColor,
    );

    if array.attrs.is_empty() {
        let _ = crossterm::execute!(
            out,
            SetForegroundColor(ctx.palette.dim),
            Print("      (none)\n"),
            ResetColor,
        );
    } else {
        let max_key = array.attrs.keys().map(|k| k.len()).max().unwrap_or(0);
        for (k, v) in &array.attrs {
            let val_str = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            let pad = max_key.saturating_sub(k.len()) + 2;
            let _ = crossterm::execute!(
                out,
                SetForegroundColor(ctx.palette.dim),
                Print(format!("      {k}:{}", " ".repeat(pad))),
                ResetColor,
                Print(format!("{val_str}\n")),
            );
        }
    }

    let _ = writeln!(out);

    CommandResult {
        action: CommandAction::Continue,
        subtitle: None,
    }
}

fn dtype_byte_size(data_type: &str) -> usize {
    match data_type {
        // v3-style (canonical)
        "float32" | "int32" | "uint32" => 4,
        "float64" | "int64" | "uint64" => 8,
        "float16" | "int16" | "uint16" => 2,
        "uint8" | "bool" | "string" => 1,
        // v2 legacy fallback
        "<f4" | ">f4" | "<i4" | ">i4" | "<u4" | ">u4" => 4,
        "<f8" | ">f8" | "<i8" | ">i8" | "<u8" | ">u8" => 8,
        "<f2" | ">f2" | "<i2" | ">i2" | "<u2" | ">u2" => 2,
        "<u1" | ">u1" | "|b1" | "|S1" => 1,
        _ => 4, // default assumption
    }
}

fn format_compressor(compressor: &Option<Value>) -> String {
    match compressor {
        None => "none".to_string(),
        Some(val) => {
            let id = val.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
            match id {
                "blosc" => {
                    let cname = val.get("cname").and_then(|v| v.as_str()).unwrap_or("?");
                    let clevel = val.get("clevel").and_then(|v| v.as_u64()).unwrap_or(0);
                    let shuffle = val.get("shuffle").and_then(|v| v.as_u64()).unwrap_or(0);
                    let shuffle_str = match shuffle {
                        0 => "noshuffle",
                        1 => "shuffle",
                        2 => "bitshuffle",
                        _ => "?",
                    };
                    format!("blosc ({cname}, level {clevel}, {shuffle_str})")
                }
                "zstd" => {
                    let level = val.get("level").and_then(|v| v.as_i64()).unwrap_or(0);
                    format!("zstd  (level {level})")
                }
                "zlib" => {
                    let level = val.get("level").and_then(|v| v.as_i64()).unwrap_or(0);
                    format!("zlib  (level {level})")
                }
                other => format!("{other}  {}", val),
            }
        }
    }
}

/// If the codec pipeline includes sharding, return its configuration object.
fn find_sharding_config(codecs: &Value) -> Option<&serde_json::Map<String, Value>> {
    codecs
        .as_array()?
        .iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some("sharding_indexed"))
        .and_then(|c| c.get("configuration"))
        .and_then(|v| v.as_object())
}

/// Format a v3 codec pipeline for display, e.g. "bytes (little-endian) → zstd (level 3)".
fn format_codecs(codecs: &Value) -> String {
    let arr = match codecs.as_array() {
        Some(a) if !a.is_empty() => a,
        _ => return "none".to_string(),
    };
    arr.iter()
        .map(|c| {
            let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
            let config = c.get("configuration").and_then(|v| v.as_object());
            match (name, config) {
                ("bytes", Some(cfg)) => {
                    let endian = cfg.get("endian").and_then(|v| v.as_str()).unwrap_or("?");
                    format!("bytes ({endian}-endian)")
                }
                ("transpose", Some(cfg)) => {
                    let order = cfg.get("order").and_then(|v| v.as_str()).unwrap_or("?");
                    format!("transpose ({order})")
                }
                ("blosc", Some(cfg)) => {
                    let cname = cfg.get("cname").and_then(|v| v.as_str()).unwrap_or("?");
                    let clevel = cfg.get("clevel").and_then(|v| v.as_u64()).unwrap_or(0);
                    format!("blosc ({cname}, level {clevel})")
                }
                ("zstd", Some(cfg)) => {
                    let level = cfg.get("level").and_then(|v| v.as_i64()).unwrap_or(0);
                    format!("zstd (level {level})")
                }
                ("gzip", Some(cfg)) => {
                    let level = cfg.get("level").and_then(|v| v.as_i64()).unwrap_or(0);
                    format!("gzip (level {level})")
                }
                (n, _) => n.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(" → ")
}

fn format_fill_value(fill_value: &Option<Value>) -> String {
    match fill_value {
        None => "null".to_string(),
        Some(Value::Null) => "null".to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(other) => other.to_string(),
    }
}

/// Wrap a comma-separated string so each line fits within `width`.
/// Continuation lines are indented by `indent` spaces.
fn wrap_at_commas(s: &str, width: usize, indent: usize) -> String {
    if width == 0 || s.len() <= width {
        return s.to_string();
    }

    let mut result = String::with_capacity(s.len() + indent);
    let mut line_len = 0;

    for (i, item) in s.split(", ").enumerate() {
        let chunk = if i == 0 {
            item.to_string()
        } else {
            format!(", {item}")
        };

        if line_len + chunk.len() > width && line_len > 0 {
            result.push('\n');
            result.push_str(&" ".repeat(indent));
            // Start new line without the leading ", "
            result.push_str(item);
            line_len = item.len();
        } else {
            result.push_str(&chunk);
            line_len += chunk.len();
        }
    }

    result
}

fn format_with_commas(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(c);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- dtype_byte_size ---

    #[test]
    fn dtype_byte_size_float32() {
        assert_eq!(dtype_byte_size("<f4"), 4);
        assert_eq!(dtype_byte_size(">f4"), 4);
    }

    #[test]
    fn dtype_byte_size_float64() {
        assert_eq!(dtype_byte_size("<f8"), 8);
        assert_eq!(dtype_byte_size(">f8"), 8);
    }

    #[test]
    fn dtype_byte_size_int_types() {
        assert_eq!(dtype_byte_size("<i2"), 2);
        assert_eq!(dtype_byte_size("<i4"), 4);
        assert_eq!(dtype_byte_size("<i8"), 8);
    }

    #[test]
    fn dtype_byte_size_uint_types() {
        assert_eq!(dtype_byte_size("<u1"), 1);
        assert_eq!(dtype_byte_size("<u2"), 2);
        assert_eq!(dtype_byte_size("<u4"), 4);
        assert_eq!(dtype_byte_size("<u8"), 8);
    }

    #[test]
    fn dtype_byte_size_bool_and_bytes() {
        assert_eq!(dtype_byte_size("|b1"), 1);
        assert_eq!(dtype_byte_size("|S1"), 1);
    }

    #[test]
    fn dtype_byte_size_v3_names() {
        assert_eq!(dtype_byte_size("float32"), 4);
        assert_eq!(dtype_byte_size("float64"), 8);
        assert_eq!(dtype_byte_size("int32"), 4);
        assert_eq!(dtype_byte_size("int64"), 8);
        assert_eq!(dtype_byte_size("uint8"), 1);
        assert_eq!(dtype_byte_size("bool"), 1);
    }

    #[test]
    fn dtype_byte_size_unknown_defaults_to_4() {
        assert_eq!(dtype_byte_size("<c16"), 4);
        assert_eq!(dtype_byte_size("object"), 4);
    }

    // --- format_compressor ---

    #[test]
    fn format_compressor_blosc() {
        let val = serde_json::json!({
            "id": "blosc",
            "cname": "lz4",
            "clevel": 5,
            "shuffle": 1,
            "blocksize": 0
        });
        assert_eq!(
            format_compressor(&Some(val)),
            "blosc (lz4, level 5, shuffle)"
        );
    }

    #[test]
    fn format_compressor_zstd() {
        let val = serde_json::json!({ "id": "zstd", "level": 3 });
        assert_eq!(format_compressor(&Some(val)), "zstd  (level 3)");
    }

    #[test]
    fn format_compressor_zlib() {
        let val = serde_json::json!({ "id": "zlib", "level": 6 });
        assert_eq!(format_compressor(&Some(val)), "zlib  (level 6)");
    }

    #[test]
    fn format_compressor_none() {
        assert_eq!(format_compressor(&None), "none");
    }

    // --- format_codecs ---

    #[test]
    fn format_codecs_bytes_and_zstd() {
        let codecs = serde_json::json!([
            {"name": "bytes", "configuration": {"endian": "little"}},
            {"name": "zstd", "configuration": {"level": 3, "checksum": false}}
        ]);
        assert_eq!(
            format_codecs(&codecs),
            "bytes (little-endian) → zstd (level 3)"
        );
    }

    #[test]
    fn format_codecs_blosc() {
        let codecs = serde_json::json!([
            {"name": "bytes", "configuration": {"endian": "little"}},
            {"name": "blosc", "configuration": {"cname": "lz4", "clevel": 5}}
        ]);
        assert_eq!(
            format_codecs(&codecs),
            "bytes (little-endian) → blosc (lz4, level 5)"
        );
    }

    #[test]
    fn format_codecs_empty() {
        assert_eq!(format_codecs(&serde_json::json!([])), "none");
    }

    #[test]
    fn format_codecs_single() {
        let codecs = serde_json::json!([
            {"name": "bytes", "configuration": {"endian": "big"}}
        ]);
        assert_eq!(format_codecs(&codecs), "bytes (big-endian)");
    }

    // --- find_sharding_config ---

    #[test]
    fn find_sharding_config_present() {
        let codecs = serde_json::json!([{
            "name": "sharding_indexed",
            "configuration": {
                "chunk_shape": [1440, 32, 32],
                "codecs": [
                    {"name": "bytes", "configuration": {"endian": "little"}},
                    {"name": "blosc", "configuration": {"cname": "zstd", "clevel": 3}}
                ],
                "index_codecs": [{"name": "bytes", "configuration": {"endian": "little"}}],
                "index_location": "end"
            }
        }]);
        let cfg = find_sharding_config(&codecs).unwrap();
        let chunk_shape = cfg["chunk_shape"].as_array().unwrap();
        assert_eq!(chunk_shape.len(), 3);
        assert_eq!(chunk_shape[0], 1440);
    }

    #[test]
    fn find_sharding_config_absent() {
        let codecs = serde_json::json!([
            {"name": "bytes", "configuration": {"endian": "little"}},
            {"name": "zstd", "configuration": {"level": 3}}
        ]);
        assert!(find_sharding_config(&codecs).is_none());
    }

    #[test]
    fn find_sharding_config_empty() {
        assert!(find_sharding_config(&serde_json::json!([])).is_none());
    }

    // --- format_codecs with sharding inner codecs ---

    #[test]
    fn format_codecs_sharding_inner_pipeline() {
        // When we extract the inner codecs from a sharding config, format_codecs
        // should format them normally
        let inner_codecs = serde_json::json!([
            {"name": "bytes", "configuration": {"endian": "little"}},
            {"name": "blosc", "configuration": {"cname": "zstd", "clevel": 3}}
        ]);
        assert_eq!(
            format_codecs(&inner_codecs),
            "bytes (little-endian) → blosc (zstd, level 3)"
        );
    }

    // --- format_fill_value ---

    #[test]
    fn format_fill_value_nan_string() {
        let val = Some(serde_json::json!("NaN"));
        assert_eq!(format_fill_value(&val), "NaN");
    }

    #[test]
    fn format_fill_value_null() {
        assert_eq!(format_fill_value(&None), "null");
        assert_eq!(format_fill_value(&Some(Value::Null)), "null");
    }

    #[test]
    fn format_fill_value_numeric() {
        let val = Some(serde_json::json!(0));
        assert_eq!(format_fill_value(&val), "0");

        let val = Some(serde_json::json!(0.0));
        assert_eq!(format_fill_value(&val), "0.0");
    }

    // --- format_with_commas ---

    #[test]
    fn format_with_commas_small() {
        assert_eq!(format_with_commas(0), "0");
        assert_eq!(format_with_commas(999), "999");
    }

    #[test]
    fn format_with_commas_thousands() {
        assert_eq!(format_with_commas(1_000), "1,000");
        assert_eq!(format_with_commas(12_345), "12,345");
    }

    #[test]
    fn format_with_commas_millions() {
        assert_eq!(format_with_commas(1_000_000), "1,000,000");
        assert_eq!(format_with_commas(745_472), "745,472");
    }

    // --- wrap_at_commas ---

    #[test]
    fn wrap_at_commas_fits_on_one_line() {
        assert_eq!(wrap_at_commas("1, 2, 3", 20, 10), "1, 2, 3");
    }

    #[test]
    fn wrap_at_commas_wraps_with_indent() {
        let result = wrap_at_commas("aaa, bbb, ccc, ddd", 10, 4);
        assert_eq!(result, "aaa, bbb\n    ccc, ddd");
    }

    #[test]
    fn wrap_at_commas_zero_width_returns_unchanged() {
        assert_eq!(wrap_at_commas("1, 2, 3", 0, 10), "1, 2, 3");
    }

    // --- chunk count computation ---

    #[test]
    fn chunk_count_exact_division() {
        // shape [100, 13, 64, 32], chunks [100, 13, 64, 32] -> all 1
        let shape = vec![100, 13, 64, 32];
        let chunks = vec![100, 13, 64, 32];
        let counts: Vec<usize> = shape
            .iter()
            .zip(chunks.iter())
            .map(|(&s, &c)| (s + c - 1) / c)
            .collect();
        assert_eq!(counts, vec![1, 1, 1, 1]);
    }

    #[test]
    fn chunk_count_with_remainder() {
        // shape [28, 13, 64, 32], chunks [100, 13, 64, 32] -> [1, 1, 1, 1]
        // because 28/100 rounds up to 1
        let shape = vec![28, 13, 64, 32];
        let chunks = vec![100, 13, 64, 32];
        let counts: Vec<usize> = shape
            .iter()
            .zip(chunks.iter())
            .map(|(&s, &c)| (s + c - 1) / c)
            .collect();
        assert_eq!(counts, vec![1, 1, 1, 1]);
    }

    #[test]
    fn chunk_count_multiple_chunks() {
        // shape [365, 180, 360], chunks [100, 90, 90]
        let shape = vec![365, 180, 360];
        let chunks = vec![100, 90, 90];
        let counts: Vec<usize> = shape
            .iter()
            .zip(chunks.iter())
            .map(|(&s, &c)| (s + c - 1) / c)
            .collect();
        assert_eq!(counts, vec![4, 2, 4]);
        assert_eq!(counts.iter().product::<usize>(), 32);
    }
}
