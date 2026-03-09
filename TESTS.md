# Testing Strategy

## 1. Unit Tests

Pure functions that can be tested with inline `#[cfg(test)]` modules — no I/O, no mocking.

### `src/commands/summary.rs`

| Function              | What to test                                                        |
|-----------------------|---------------------------------------------------------------------|
| `friendly_dtype()`    | Maps NumPy dtype codes to friendly names (`<f4` → `float32`, etc.) |
| `is_coordinate()`     | True when `dims.len() == 1 && dims[0] == name`                     |
| `format_dims_parens()`| Formats dims as `(time, lat, lon)`                                  |
| `format_shape()`      | Formats shape as `100 x 200 x 300`                                 |
| `dir_size_bytes()`    | Recursive directory size calculation (use a temp dir)               |

### `src/zarr/store.rs`

| Function                | What to test                                                    |
|-------------------------|-----------------------------------------------------------------|
| `StoreLocation::parse()`| Classifies `s3://`, `gs://`, `az://`, HTTPS, and local paths   |
| `display_path()`        | Abbreviates local paths with `~`, returns cloud URLs as-is     |

### `src/zarr/metadata.rs`

| Function       | What to test                                                             |
|----------------|--------------------------------------------------------------------------|
| `parse_store()`| Given `.zmetadata` JSON, produces correct `StoreMeta` (arrays, dims, attrs) |

For `parse_store()`, construct `StoreLocation::Local` pointing at a fixture directory containing a `.zmetadata` file (see section 2).

## 2. Integration Tests — Fixture-Based

Test the full parsing pipeline using real `.zmetadata` files as fixtures.

### Setup

Create a `tests/fixtures/` directory with `.zmetadata` files extracted from real zarr stores:

```
tests/
  fixtures/
    local_era5/
      .zmetadata          ← from data/weatherbench2/era5_64x32_1week/
    gcs_cmip6/
      .zmetadata          ← from gs://cmip6/CMIP6/CMIP/MPI-M/...
    s3_mur_sst/
      .zmetadata          ← from s3://mur-sst/zarr/
```

### What to assert

Test at the `parse_store()` level — the meaningful contract is the `StoreMeta` struct, not the rendered terminal output:

- Number of arrays
- Specific array names, shapes, dtypes
- Dimension names and sizes
- Root attributes
- Coordinate vs data variable classification (via `is_coordinate()`)

This avoids fragile assertions on ANSI escape codes or column alignment.

### Example

```rust
#[test]
fn parse_local_era5_fixture() {
    let location = StoreLocation::Local(PathBuf::from("tests/fixtures/local_era5"));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let meta = parse_store(&location, &runtime).unwrap();

    assert_eq!(meta.zarr_format, 2);
    assert!(meta.arrays.iter().any(|a| a.name == "temperature"));
    // ... assert shapes, dtypes, dims
}
```

## 3. Cloud Regression Tests

Test against real public cloud zarr stores. These are slow and require network access, so they should be marked `#[ignore]` and run explicitly:

```
cargo test -- --ignored
```

### Datasets

| Provider | Dataset | Path |
|----------|---------|------|
| GCS      | CMIP6   | `gs://cmip6/CMIP6/CMIP/MPI-M/MPI-ESM1-2-LR/historical/r10i1p1f1/day/pr/gn/v20190710` |
| S3       | MUR SST | `s3://mur-sst/zarr/` |

### What to assert

Keep assertions stable — only check properties unlikely to change:

- `parse_store()` succeeds without error
- Expected array names are present
- Array count is within a reasonable range
- Zarr format version

### Example

```rust
#[test]
#[ignore] // requires network
fn cloud_gcs_cmip6() {
    let location = StoreLocation::parse("gs://cmip6/CMIP6/CMIP/MPI-M/...").unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let meta = parse_store(&location, &runtime).unwrap();

    assert_eq!(meta.zarr_format, 2);
    assert!(meta.arrays.iter().any(|a| a.name == "pr"));
}
```

## 4. What We're Not Testing (Yet)

- **Rendered terminal output**: The summary rendering is still evolving. Snapshot tests (e.g. with `insta`) would add maintenance burden for cosmetic changes. Revisit once the output format stabilises.
- **REPL / interactive input**: Raw mode input handling and autocomplete are tightly coupled to the terminal. These are best tested manually for now.
- **Authenticated cloud stores**: No test coverage for stores requiring credentials until we have a way to manage test credentials.

## Running Tests

```bash
cargo test                   # unit + fixture integration tests
cargo test -- --ignored      # also run cloud regression tests
```
