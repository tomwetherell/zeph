use zeph::zarr::metadata::parse_store;
use zeph::zarr::store::StoreLocation;

const CMIP6_GCS: &str =
    "gs://cmip6/CMIP6/CMIP/MPI-M/MPI-ESM1-2-LR/historical/r10i1p1f1/day/pr/gn/v20190710";

const CMIP6_HTTPS: &str =
    "https://storage.googleapis.com/cmip6/CMIP6/CMIP/MPI-M/MPI-ESM1-2-LR/historical/r10i1p1f1/day/pr/gn/v20190710";

const MUR_SST_S3: &str = "s3://mur-sst/zarr/";

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().unwrap()
}

// --- GCS ---

#[test]
#[ignore] // requires network
fn cloud_gcs_cmip6() {
    let location = StoreLocation::parse(CMIP6_GCS).unwrap();
    let runtime = runtime();
    let meta = parse_store(&location, &runtime).unwrap();

    assert_eq!(meta.zarr_format, 2);
    assert_eq!(meta.arrays.len(), 7);
    assert!(meta.arrays.iter().any(|a| a.name == "pr"));
    assert!(meta.arrays.iter().any(|a| a.name == "time"));
    assert!(meta.arrays.iter().any(|a| a.name == "lat"));
    assert!(meta.arrays.iter().any(|a| a.name == "lon"));
}

// --- S3 (also exercises auto-region detection) ---

#[test]
#[ignore] // requires network
fn cloud_s3_mur_sst() {
    let location = StoreLocation::parse(MUR_SST_S3).unwrap();
    let runtime = runtime();
    let meta = parse_store(&location, &runtime).unwrap();

    assert_eq!(meta.zarr_format, 2);
    assert_eq!(meta.arrays.len(), 7);
    assert!(meta.arrays.iter().any(|a| a.name == "analysed_sst"));
    assert!(meta.arrays.iter().any(|a| a.name == "time"));
    assert!(meta.arrays.iter().any(|a| a.name == "lat"));
    assert!(meta.arrays.iter().any(|a| a.name == "lon"));
}

// --- HTTPS (same dataset as GCS, different code path) ---

#[test]
#[ignore] // requires network
fn cloud_https_cmip6() {
    let location = StoreLocation::parse(CMIP6_HTTPS).unwrap();
    let runtime = runtime();
    let meta = parse_store(&location, &runtime).unwrap();

    assert_eq!(meta.zarr_format, 2);
    assert_eq!(meta.arrays.len(), 7);
    assert!(meta.arrays.iter().any(|a| a.name == "pr"));
    assert!(meta.arrays.iter().any(|a| a.name == "time"));
    assert!(meta.arrays.iter().any(|a| a.name == "lat"));
    assert!(meta.arrays.iter().any(|a| a.name == "lon"));
}

/// GCS and HTTPS point at the same dataset — verify they produce identical results.
#[test]
#[ignore] // requires network
fn cloud_gcs_and_https_match() {
    let runtime = runtime();

    let gcs_loc = StoreLocation::parse(CMIP6_GCS).unwrap();
    let gcs_meta = parse_store(&gcs_loc, &runtime).unwrap();

    let https_loc = StoreLocation::parse(CMIP6_HTTPS).unwrap();
    let https_meta = parse_store(&https_loc, &runtime).unwrap();

    assert_eq!(gcs_meta.zarr_format, https_meta.zarr_format);
    assert_eq!(gcs_meta.arrays.len(), https_meta.arrays.len());

    let gcs_names: Vec<&str> = gcs_meta.arrays.iter().map(|a| a.name.as_str()).collect();
    let https_names: Vec<&str> = https_meta.arrays.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(gcs_names, https_names);
}
