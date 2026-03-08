use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::path::Path;

use crossterm::style::{Print, ResetColor, SetForegroundColor};

use super::{CommandResult, CommandAction, Ctx};
use crate::ui::style;
use crate::zarr::metadata::{self, ArrayMeta, StoreMeta};
use crate::zarr::store::StoreLocation;

pub fn run(ctx: &Ctx) -> CommandResult {
    let mut out = io::stdout();

    match metadata::parse_store(&ctx.store, &ctx.runtime) {
        Ok(store) => {
            let has_dims = store.arrays.iter().any(|a| !a.dims.is_empty());
            if has_dims {
                render_xarray_style(&mut out, &ctx.store, &store);
            } else {
                render_flat(&mut out, &ctx.store, &store);
            }
        }
        Err(e) => {
            let _ = crossterm::execute!(
                out,
                Print("\n"),
                SetForegroundColor(style::DIM),
                Print(format!("  Error: {e}\n\n")),
                ResetColor,
            );
        }
    }

    CommandResult {
        action: CommandAction::Continue,
        subtitle: None,
    }
}

fn render_xarray_style(out: &mut impl Write, location: &StoreLocation, store: &StoreMeta) {
    // Build dimension sizes from all arrays
    let mut dim_sizes: BTreeMap<String, usize> = BTreeMap::new();
    for arr in &store.arrays {
        for (i, dim) in arr.dims.iter().enumerate() {
            if let Some(&size) = arr.shape.get(i) {
                dim_sizes.entry(dim.clone()).or_insert(size);
            }
        }
    }

    // Classify arrays
    let coord_set: BTreeSet<String> = store
        .arrays
        .iter()
        .filter(|a| is_coordinate(a))
        .map(|a| a.name.clone())
        .collect();

    let mut coords: Vec<&ArrayMeta> = store
        .arrays
        .iter()
        .filter(|a| coord_set.contains(&a.name))
        .collect();
    let mut data_vars: Vec<&ArrayMeta> = store
        .arrays
        .iter()
        .filter(|a| !coord_set.contains(&a.name))
        .collect();

    // Sort coords by dimension order, data vars alphabetically
    let dim_order: Vec<&str> = dim_sizes.keys().map(|s| s.as_str()).collect();
    coords.sort_by_key(|a| {
        dim_order
            .iter()
            .position(|d| *d == a.name)
            .unwrap_or(usize::MAX)
    });
    data_vars.sort_by(|a, b| a.name.cmp(&b.name));

    // Compute column widths
    let all_arrays: Vec<&ArrayMeta> = coords.iter().chain(data_vars.iter()).copied().collect();
    let max_name = all_arrays.iter().map(|a| a.name.len()).max().unwrap_or(0);
    let max_dims_str = all_arrays
        .iter()
        .map(|a| format_dims_parens(a).len())
        .max()
        .unwrap_or(0);
    let max_dtype = all_arrays
        .iter()
        .map(|a| friendly_dtype(&a.dtype).len())
        .max()
        .unwrap_or(0);

    // Store header
    let display_path = location.display_path();
    let size_str = store_size_str(location);
    let _ = crossterm::execute!(out, Print("\n"));
    let _ = crossterm::execute!(
        out,
        SetForegroundColor(style::HEADING),
        Print("  Store: "),
        ResetColor,
        Print(format!(
            "{}  (zarr v{}, {})\n",
            display_path, store.zarr_format, size_str
        )),
    );

    // Dimensions
    let _ = crossterm::execute!(out, Print("\n"));
    let _ = crossterm::execute!(
        out,
        SetForegroundColor(style::HEADING),
        Print("  Dimensions:  "),
        ResetColor,
    );
    let dims_str: Vec<String> = dim_sizes
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect();
    let _ = write!(out, "{}\n", dims_str.join(", "));

    // Coordinates
    let _ = crossterm::execute!(
        out,
        SetForegroundColor(style::HEADING),
        Print("  Coordinates:\n"),
        ResetColor,
    );
    for arr in &coords {
        print_array_line(out, arr, max_name, max_dims_str, max_dtype);
    }

    // Data variables
    let _ = crossterm::execute!(
        out,
        SetForegroundColor(style::HEADING),
        Print("  Data variables:\n"),
        ResetColor,
    );
    for arr in &data_vars {
        print_array_line(out, arr, max_name, max_dims_str, max_dtype);
    }

    // Attributes
    let _ = crossterm::execute!(
        out,
        SetForegroundColor(style::HEADING),
        Print("  Attributes:\n"),
        ResetColor,
    );
    if store.root_attrs.is_empty() {
        let _ = crossterm::execute!(
            out,
            SetForegroundColor(style::DIM),
            Print("      (none)\n"),
            ResetColor,
        );
    } else {
        for (k, v) in &store.root_attrs {
            let val_str = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            let _ = crossterm::execute!(
                out,
                Print(format!("      {k}: ")),
                SetForegroundColor(style::DIM),
                Print(format!("{val_str}\n")),
                ResetColor,
            );
        }
    }
    let _ = writeln!(out);
}

fn render_flat(out: &mut impl Write, location: &StoreLocation, store: &StoreMeta) {
    let display_path = location.display_path();
    let size_str = store_size_str(location);
    let _ = crossterm::execute!(out, Print("\n"));
    let _ = crossterm::execute!(
        out,
        SetForegroundColor(style::HEADING),
        Print("  Store: "),
        ResetColor,
        Print(format!(
            "{}  (zarr v{}, {})\n\n",
            display_path, store.zarr_format, size_str
        )),
    );

    let max_name = store.arrays.iter().map(|a| a.name.len()).max().unwrap_or(0);
    let _ = crossterm::execute!(
        out,
        SetForegroundColor(style::HEADING),
        Print("  Arrays:\n"),
        ResetColor,
    );
    for arr in &store.arrays {
        let shape_str = format_shape(&arr.shape);
        let dtype = friendly_dtype(&arr.dtype);
        let pad = max_name.saturating_sub(arr.name.len()) + 2;
        let _ = crossterm::execute!(
            out,
            Print(format!("      {}{}", arr.name, " ".repeat(pad))),
            SetForegroundColor(style::DIM),
            Print(format!("{dtype}  {shape_str}\n")),
            ResetColor,
        );
    }
    let _ = writeln!(out);
}

fn print_array_line(out: &mut impl Write, arr: &ArrayMeta, max_name: usize, max_dims: usize, max_dtype: usize) {
    let dims_str = format_dims_parens(arr);
    let dtype = friendly_dtype(&arr.dtype);
    let shape_str = format_shape(&arr.shape);
    let name_pad = max_name.saturating_sub(arr.name.len()) + 2;
    let dims_pad = max_dims.saturating_sub(dims_str.len()) + 2;
    let dtype_pad = max_dtype.saturating_sub(dtype.len()) + 2;
    let _ = crossterm::execute!(
        out,
        Print(format!("      {}{}", arr.name, " ".repeat(name_pad))),
        SetForegroundColor(style::DIM),
        Print(format!(
            "{}{}{dtype}{}{shape_str}\n",
            dims_str,
            " ".repeat(dims_pad),
            " ".repeat(dtype_pad)
        )),
        ResetColor,
    );
}

fn format_dims_parens(arr: &ArrayMeta) -> String {
    if arr.dims.is_empty() {
        String::new()
    } else {
        format!("({})", arr.dims.join(", "))
    }
}

fn format_shape(shape: &[usize]) -> String {
    shape
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
        .join(" x ")
}

fn friendly_dtype(dtype: &str) -> &str {
    match dtype {
        "<f4" | ">f4" => "float32",
        "<f8" | ">f8" => "float64",
        "<i4" | ">i4" => "int32",
        "<i8" | ">i8" => "int64",
        "<i2" | ">i2" => "int16",
        "<u1" | ">u1" => "uint8",
        "<u2" | ">u2" => "uint16",
        "<u4" | ">u4" => "uint32",
        "<u8" | ">u8" => "uint64",
        "|b1" => "bool",
        "|S1" => "bytes",
        other => other,
    }
}

fn is_coordinate(arr: &ArrayMeta) -> bool {
    arr.dims.len() == 1 && arr.dims[0] == arr.name
}

fn store_size_str(location: &StoreLocation) -> String {
    match location {
        StoreLocation::Local(path) => dir_size_human(path),
        StoreLocation::Cloud { .. } => "remote".to_string(),
    }
}

fn dir_size_human(path: &Path) -> String {
    let bytes = dir_size_bytes(path);
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn dir_size_bytes(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if ft.is_file() {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            } else if ft.is_dir() {
                total += dir_size_bytes(&entry.path());
            }
        }
    }
    total
}
