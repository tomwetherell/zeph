use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::path::Path;

use crossterm::style::{Print, ResetColor, SetForegroundColor};

use super::{CommandResult, CommandAction, Ctx};
use crate::ui::style::Palette;
use zeph::zarr::metadata::{ArrayMeta, StoreMeta};
use zeph::zarr::store::StoreLocation;

pub fn run(ctx: &Ctx) -> CommandResult {
    let mut out = io::stdout();

    let has_dims = ctx.meta.arrays.iter().any(|a| !a.dims.is_empty());
    if has_dims {
        render_xarray_style(&mut out, &ctx.store, &ctx.meta, &ctx.palette);
    } else {
        render_flat(&mut out, &ctx.store, &ctx.meta, &ctx.palette);
    }

    CommandResult {
        action: CommandAction::Continue,
        subtitle: None,
    }
}

fn render_xarray_style(out: &mut impl Write, location: &StoreLocation, store: &StoreMeta, palette: &Palette) {
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
        .filter(|a| a.is_coordinate())
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
        SetForegroundColor(palette.heading),
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
        SetForegroundColor(palette.heading),
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
        SetForegroundColor(palette.heading),
        Print("  Coordinates:\n"),
        ResetColor,
    );
    for arr in &coords {
        print_array_line(out, arr, max_name, max_dims_str, max_dtype, palette);
    }

    // Data variables
    let _ = crossterm::execute!(
        out,
        SetForegroundColor(palette.heading),
        Print("  Data variables:\n"),
        ResetColor,
    );
    for arr in &data_vars {
        print_array_line(out, arr, max_name, max_dims_str, max_dtype, palette);
    }

    // Attributes
    let _ = crossterm::execute!(
        out,
        SetForegroundColor(palette.heading),
        Print("  Attributes:\n"),
        ResetColor,
    );
    if store.root_attrs.is_empty() {
        let _ = crossterm::execute!(
            out,
            SetForegroundColor(palette.dim),
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
                SetForegroundColor(palette.dim),
                Print(format!("{val_str}\n")),
                ResetColor,
            );
        }
    }
    let _ = writeln!(out);
}

fn render_flat(out: &mut impl Write, location: &StoreLocation, store: &StoreMeta, palette: &Palette) {
    let display_path = location.display_path();
    let size_str = store_size_str(location);
    let _ = crossterm::execute!(out, Print("\n"));
    let _ = crossterm::execute!(
        out,
        SetForegroundColor(palette.heading),
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
        SetForegroundColor(palette.heading),
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
            SetForegroundColor(palette.dim),
            Print(format!("{dtype}  {shape_str}\n")),
            ResetColor,
        );
    }
    let _ = writeln!(out);
}

fn print_array_line(out: &mut impl Write, arr: &ArrayMeta, max_name: usize, max_dims: usize, max_dtype: usize, palette: &Palette) {
    let dims_str = format_dims_parens(arr);
    let dtype = friendly_dtype(&arr.dtype);
    let shape_str = format_shape(&arr.shape);
    let name_pad = max_name.saturating_sub(arr.name.len()) + 2;
    let dims_pad = max_dims.saturating_sub(dims_str.len()) + 2;
    let dtype_pad = max_dtype.saturating_sub(dtype.len()) + 2;
    let _ = crossterm::execute!(
        out,
        Print(format!("      {}{}", arr.name, " ".repeat(name_pad))),
        SetForegroundColor(palette.dim),
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

pub(crate) fn friendly_dtype(dtype: &str) -> &str {
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

fn store_size_str(location: &StoreLocation) -> String {
    match location {
        StoreLocation::Local(path) => dir_size_human(path),
        StoreLocation::Cloud { .. } => "remote".to_string(),
    }
}

pub(crate) fn format_bytes(bytes: u64) -> String {
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

fn dir_size_human(path: &Path) -> String {
    format_bytes(dir_size_bytes(path))
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

#[cfg(test)]
mod tests {
    use super::*;
    use zeph::zarr::metadata::ArrayMeta;
    use std::collections::BTreeMap;

    fn make_array(name: &str, dims: &[&str], shape: &[usize], dtype: &str) -> ArrayMeta {
        ArrayMeta {
            name: name.to_string(),
            dims: dims.iter().map(|s| s.to_string()).collect(),
            shape: shape.to_vec(),
            dtype: dtype.to_string(),
            attrs: BTreeMap::new(),
            chunks: Vec::new(),
            compressor: None,
            fill_value: None,
            order: None,
            filters: None,
        }
    }

    // --- friendly_dtype ---

    #[test]
    fn friendly_dtype_float32() {
        assert_eq!(friendly_dtype("<f4"), "float32");
        assert_eq!(friendly_dtype(">f4"), "float32");
    }

    #[test]
    fn friendly_dtype_float64() {
        assert_eq!(friendly_dtype("<f8"), "float64");
        assert_eq!(friendly_dtype(">f8"), "float64");
    }

    #[test]
    fn friendly_dtype_int_types() {
        assert_eq!(friendly_dtype("<i2"), "int16");
        assert_eq!(friendly_dtype(">i2"), "int16");
        assert_eq!(friendly_dtype("<i4"), "int32");
        assert_eq!(friendly_dtype(">i4"), "int32");
        assert_eq!(friendly_dtype("<i8"), "int64");
        assert_eq!(friendly_dtype(">i8"), "int64");
    }

    #[test]
    fn friendly_dtype_uint_types() {
        assert_eq!(friendly_dtype("<u1"), "uint8");
        assert_eq!(friendly_dtype(">u1"), "uint8");
        assert_eq!(friendly_dtype("<u2"), "uint16");
        assert_eq!(friendly_dtype(">u2"), "uint16");
        assert_eq!(friendly_dtype("<u4"), "uint32");
        assert_eq!(friendly_dtype(">u4"), "uint32");
        assert_eq!(friendly_dtype("<u8"), "uint64");
        assert_eq!(friendly_dtype(">u8"), "uint64");
    }

    #[test]
    fn friendly_dtype_bool_and_bytes() {
        assert_eq!(friendly_dtype("|b1"), "bool");
        assert_eq!(friendly_dtype("|S1"), "bytes");
    }

    #[test]
    fn friendly_dtype_unknown_passthrough() {
        assert_eq!(friendly_dtype("<c16"), "<c16");
        assert_eq!(friendly_dtype("object"), "object");
    }

    // --- is_coordinate ---

    #[test]
    fn is_coordinate_true() {
        let arr = make_array("time", &["time"], &[365], "<f8");
        assert!(arr.is_coordinate());
    }

    #[test]
    fn is_coordinate_false_no_dims() {
        let arr = make_array("time", &[], &[365], "<f8");
        assert!(!arr.is_coordinate());
    }

    #[test]
    fn is_coordinate_false_dim_name_mismatch() {
        let arr = make_array("temperature", &["time"], &[365], "<f4");
        assert!(!arr.is_coordinate());
    }

    #[test]
    fn is_coordinate_false_multiple_dims() {
        let arr = make_array("time", &["time", "lat"], &[365, 180], "<f8");
        assert!(!arr.is_coordinate());
    }

    // --- format_dims_parens ---

    #[test]
    fn format_dims_parens_empty() {
        let arr = make_array("data", &[], &[], "<f4");
        assert_eq!(format_dims_parens(&arr), "");
    }

    #[test]
    fn format_dims_parens_single() {
        let arr = make_array("time", &["time"], &[365], "<f8");
        assert_eq!(format_dims_parens(&arr), "(time)");
    }

    #[test]
    fn format_dims_parens_multiple() {
        let arr = make_array("temp", &["time", "lat", "lon"], &[365, 180, 360], "<f4");
        assert_eq!(format_dims_parens(&arr), "(time, lat, lon)");
    }

    // --- format_shape ---

    #[test]
    fn format_shape_empty() {
        assert_eq!(format_shape(&[]), "");
    }

    #[test]
    fn format_shape_single() {
        assert_eq!(format_shape(&[365]), "365");
    }

    #[test]
    fn format_shape_multiple() {
        assert_eq!(format_shape(&[365, 180, 360]), "365 x 180 x 360");
    }

    // --- dir_size_bytes ---

    #[test]
    fn dir_size_bytes_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(dir_size_bytes(dir.path()), 0);
    }

    #[test]
    fn dir_size_bytes_with_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap(); // 5 bytes
        std::fs::write(dir.path().join("b.txt"), "world!").unwrap(); // 6 bytes
        assert_eq!(dir_size_bytes(dir.path()), 11);
    }

    #[test]
    fn dir_size_bytes_nested() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(dir.path().join("top.txt"), "aaa").unwrap(); // 3 bytes
        std::fs::write(sub.join("nested.txt"), "bb").unwrap(); // 2 bytes
        assert_eq!(dir_size_bytes(dir.path()), 5);
    }

    #[test]
    fn dir_size_bytes_nonexistent() {
        assert_eq!(dir_size_bytes(Path::new("/nonexistent/path")), 0);
    }

    // --- dir_size_human ---

    #[test]
    fn dir_size_human_bytes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
        assert_eq!(dir_size_human(dir.path()), "5 B");
    }

    #[test]
    fn dir_size_human_kilobytes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.bin"), vec![0u8; 2048]).unwrap();
        assert_eq!(dir_size_human(dir.path()), "2.0 KB");
    }

    #[test]
    fn dir_size_human_megabytes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.bin"), vec![0u8; 3 * 1024 * 1024]).unwrap();
        assert_eq!(dir_size_human(dir.path()), "3.0 MB");
    }

    // --- store_size_str ---

    #[test]
    fn store_size_str_cloud_is_remote() {
        use std::sync::Arc;
        let store = StoreLocation::Cloud {
            url: "gs://bucket/path".to_string(),
            store: Arc::new(object_store::memory::InMemory::new()),
            base_path: object_store::path::Path::from("path"),
        };
        assert_eq!(store_size_str(&store), "remote");
    }
}
