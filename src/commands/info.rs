use std::io::{self, Write};
use std::time::Duration;

use crossterm::style::{Print, ResetColor, SetForegroundColor};
use serde_json::Value;

use super::{CommandAction, CommandResult, Ctx};
use super::summary::{format_bytes, friendly_dtype};
use zeph::zarr::coord_cache::CoordEntry;
use zeph::zarr::metadata::ArrayMeta;

pub fn run(ctx: &Ctx, array: &ArrayMeta) -> CommandResult {
    let mut out = io::stdout();

    // Header: name (dim1: size1, dim2: size2, ...)
    let dims_str = if array.dims.is_empty() {
        String::new()
    } else {
        let parts: Vec<String> = array
            .dims
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
    let byte_size = dtype_byte_size(&array.dtype);
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
        Print(format!("{}\n", friendly_dtype(&array.dtype))),
    );

    // Fill value
    let _ = crossterm::execute!(
        out,
        SetForegroundColor(ctx.palette.heading),
        Print(format!("  {:<label_width$}", "Fill value:")),
        ResetColor,
        Print(format!("{}\n", format_fill_value(&array.fill_value))),
    );

    // Order
    if let Some(ref order) = array.order {
        let _ = crossterm::execute!(
            out,
            SetForegroundColor(ctx.palette.heading),
            Print(format!("  {:<label_width$}", "Order:")),
            ResetColor,
            Print(format!("{order}\n")),
        );
    }

    // Compressor
    let _ = crossterm::execute!(
        out,
        SetForegroundColor(ctx.palette.heading),
        Print(format!("  {:<label_width$}", "Compressor:")),
        ResetColor,
        Print(format!("{}\n", format_compressor(&array.compressor))),
    );

    // Chunks
    if !array.chunks.is_empty() {
        let _ = crossterm::execute!(out, Print("\n"));

        let chunk_tuple: Vec<String> = array.chunks.iter().map(|c| c.to_string()).collect();
        let _ = crossterm::execute!(
            out,
            SetForegroundColor(ctx.palette.heading),
            Print(format!("  {:<label_width$}", "Chunks:")),
            ResetColor,
            Print(format!("({})\n", chunk_tuple.join(", "))),
        );

        // Per-dim chunk count
        let chunk_counts: Vec<usize> = array
            .shape
            .iter()
            .zip(array.chunks.iter())
            .map(|(&s, &c)| if c == 0 { 0 } else { (s + c - 1) / c })
            .collect();
        let total_chunks: usize = chunk_counts.iter().product();

        let chunk_label: Vec<String> = if !array.dims.is_empty() {
            array
                .dims
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

        let max_name = coord_entries.iter().map(|(a, _)| a.name.len()).max().unwrap_or(0);
        for (coord_arr, entry) in &coord_entries {
            let name_pad = max_name.saturating_sub(coord_arr.name.len()) + 2;
            let size = coord_arr.shape.first().copied().unwrap_or(0);
            let dtype = friendly_dtype(&coord_arr.dtype);

            let values_str = match entry {
                Some(CoordEntry::Ready(vals)) => vals.format_summary(3, 3),
                Some(CoordEntry::Pending) => "loading...".to_string(),
                Some(CoordEntry::Failed(_)) | None => String::new(),
            };

            let _ = crossterm::execute!(
                out,
                Print(format!("    * {}{}", coord_arr.name, " ".repeat(name_pad))),
                SetForegroundColor(ctx.palette.dim),
                Print(format!("({size})  {dtype}")),
                ResetColor,
            );
            if !values_str.is_empty() {
                let _ = crossterm::execute!(
                    out,
                    Print(format!("   {values_str}")),
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

fn dtype_byte_size(dtype: &str) -> usize {
    match dtype {
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
            let id = val
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
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
                    format!("blosc / {cname}  (level {clevel}, {shuffle_str})")
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

fn format_with_commas(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
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
            "blosc / lz4  (level 5, shuffle)"
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
