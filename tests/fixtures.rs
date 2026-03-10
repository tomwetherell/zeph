use std::path::PathBuf;

use zeph::zarr::metadata::parse_store;
use zeph::zarr::store::StoreLocation;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn parse_wb2_era5_fixture() {
    let location = StoreLocation::Local(fixture_path("wb2_era5"));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let meta = parse_store(&location, &runtime).unwrap();

    // Top-level metadata
    assert_eq!(meta.zarr_format, 2);
    assert!(meta.root_attrs.is_empty());
    assert_eq!(meta.arrays.len(), 66);

    // --- Coordinate variables (1D, dimension name == array name) ---

    let time = meta.arrays.iter().find(|a| a.name == "time").unwrap();
    assert_eq!(time.shape, vec![28]);
    assert_eq!(time.dtype, "<i8");
    assert_eq!(time.dims, vec!["time"]);
    assert_eq!(time.attrs["units"], serde_json::json!("hours since 1959-01-01"));
    assert_eq!(time.attrs["calendar"], serde_json::json!("proleptic_gregorian"));

    let latitude = meta.arrays.iter().find(|a| a.name == "latitude").unwrap();
    assert_eq!(latitude.shape, vec![32]);
    assert_eq!(latitude.dtype, "<f8");
    assert_eq!(latitude.dims, vec!["latitude"]);

    let longitude = meta.arrays.iter().find(|a| a.name == "longitude").unwrap();
    assert_eq!(longitude.shape, vec![64]);
    assert_eq!(longitude.dtype, "<f8");
    assert_eq!(longitude.dims, vec!["longitude"]);

    let level = meta.arrays.iter().find(|a| a.name == "level").unwrap();
    assert_eq!(level.shape, vec![13]);
    assert_eq!(level.dtype, "<i8");
    assert_eq!(level.dims, vec!["level"]);

    // --- 4D pressure-level variable ---

    let temp = meta.arrays.iter().find(|a| a.name == "temperature").unwrap();
    assert_eq!(temp.shape, vec![28, 13, 64, 32]);
    assert_eq!(temp.dtype, "<f4");
    assert_eq!(temp.dims, vec!["time", "level", "longitude", "latitude"]);
    assert_eq!(temp.attrs["units"], serde_json::json!("K"));
    assert_eq!(temp.attrs["long_name"], serde_json::json!("Temperature"));
    assert_eq!(temp.attrs["standard_name"], serde_json::json!("air_temperature"));

    // --- 3D surface variable ---

    let t2m = meta.arrays.iter().find(|a| a.name == "2m_temperature").unwrap();
    assert_eq!(t2m.shape, vec![28, 64, 32]);
    assert_eq!(t2m.dtype, "<f4");
    assert_eq!(t2m.dims, vec!["time", "longitude", "latitude"]);
    assert_eq!(t2m.attrs["units"], serde_json::json!("K"));
    assert_eq!(t2m.attrs["short_name"], serde_json::json!("t2m"));

    // --- 2D static field ---

    let lsm = meta.arrays.iter().find(|a| a.name == "land_sea_mask").unwrap();
    assert_eq!(lsm.shape, vec![64, 32]);
    assert_eq!(lsm.dtype, "<f4");
    assert_eq!(lsm.dims, vec!["longitude", "latitude"]);

    // --- New storage fields ---

    assert_eq!(temp.chunks, vec![100, 13, 64, 32]);
    assert!(temp.compressor.is_some());
    let comp = temp.compressor.as_ref().unwrap();
    assert_eq!(comp["id"], serde_json::json!("blosc"));
    assert_eq!(comp["cname"], serde_json::json!("lz4"));
    assert_eq!(comp["clevel"], serde_json::json!(5));
    assert_eq!(temp.fill_value, Some(serde_json::json!("NaN")));
    assert_eq!(temp.order, Some("C".to_string()));
    assert!(temp.filters.is_none());

    // Coordinate also has storage fields
    assert_eq!(time.chunks, vec![23386]);
    assert!(time.compressor.is_some());
    assert_eq!(time.fill_value, Some(serde_json::Value::Null));
    assert_eq!(time.order, Some("C".to_string()));

    // --- _ARRAY_DIMENSIONS should not leak into attrs ---

    for arr in &meta.arrays {
        assert!(
            !arr.attrs.contains_key("_ARRAY_DIMENSIONS"),
            "{} should not have _ARRAY_DIMENSIONS in attrs",
            arr.name
        );
    }
}
