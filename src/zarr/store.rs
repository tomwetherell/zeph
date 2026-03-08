use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Context};
use object_store::path::Path as ObjectPath;
use object_store::ObjectStore;

pub enum StoreLocation {
    Local(PathBuf),
    Cloud {
        url: String,
        store: Arc<dyn ObjectStore>,
        base_path: ObjectPath,
    },
}

impl StoreLocation {
    pub fn parse(input: &str) -> anyhow::Result<Self> {
        if input.starts_with("s3://") {
            parse_s3(input)
        } else if input.starts_with("gs://") {
            parse_gcs(input)
        } else if input.starts_with("az://") {
            parse_azure(input)
        } else if input.starts_with("http://") || input.starts_with("https://") {
            parse_http(input)
        } else {
            let path = PathBuf::from(input);
            if !path.exists() {
                bail!("Path does not exist: {input}");
            }
            Ok(StoreLocation::Local(path))
        }
    }

    pub fn display_path(&self) -> String {
        match self {
            StoreLocation::Local(path) => abbreviate_path(path),
            StoreLocation::Cloud { url, .. } => url.clone(),
        }
    }
}

fn parse_gcs(input: &str) -> anyhow::Result<StoreLocation> {
    let url = url::Url::parse(input)
        .with_context(|| format!("Invalid URL: {input}"))?;
    let bucket = url.host_str().context("Missing bucket name in GCS URL")?;
    let path = url.path().trim_start_matches('/');

    let has_creds = std::env::var("GOOGLE_SERVICE_ACCOUNT").is_ok()
        || std::env::var("GOOGLE_SERVICE_ACCOUNT_PATH").is_ok()
        || std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok()
        || default_gcp_creds_exist();

    let mut builder = object_store::gcp::GoogleCloudStorageBuilder::from_env()
        .with_bucket_name(bucket);
    if !has_creds {
        builder = builder.with_skip_signature(true);
    }
    let store = builder.build()
        .with_context(|| format!("Could not create GCS store for {input}"))?;

    Ok(StoreLocation::Cloud {
        url: input.to_string(),
        store: Arc::new(store),
        base_path: ObjectPath::from(path),
    })
}

fn parse_s3(input: &str) -> anyhow::Result<StoreLocation> {
    let url = url::Url::parse(input)
        .with_context(|| format!("Invalid URL: {input}"))?;
    let bucket = url.host_str().context("Missing bucket name in S3 URL")?;
    let path = url.path().trim_start_matches('/');

    let has_creds = std::env::var("AWS_ACCESS_KEY_ID").is_ok()
        || std::env::var("AWS_PROFILE").is_ok()
        || std::env::var("AWS_WEB_IDENTITY_TOKEN_FILE").is_ok();

    let region = std::env::var("AWS_DEFAULT_REGION")
        .or_else(|_| std::env::var("AWS_REGION"))
        .unwrap_or_else(|_| detect_s3_region(bucket).unwrap_or_else(|| "us-east-1".to_string()));

    let mut builder = object_store::aws::AmazonS3Builder::from_env()
        .with_bucket_name(bucket)
        .with_region(region);
    if !has_creds {
        builder = builder.with_skip_signature(true);
    }
    let store = builder.build()
        .with_context(|| format!("Could not create S3 store for {input}"))?;

    Ok(StoreLocation::Cloud {
        url: input.to_string(),
        store: Arc::new(store),
        base_path: ObjectPath::from(path),
    })
}

/// Detect the region of an S3 bucket via a HEAD request to the global endpoint.
fn detect_s3_region(bucket: &str) -> Option<String> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpStream;

    let host = format!("{bucket}.s3.amazonaws.com");
    let mut stream = TcpStream::connect((&*host, 443_u16)).ok()?;

    // We need TLS — use rustls via the already-present rustls-tls stack.
    // Fall back to a simpler approach: parse the redirect from a plain HTTP request
    // to the non-TLS endpoint. S3 actually responds to HTTP on port 80.
    drop(stream);

    stream = TcpStream::connect((&*host, 80_u16)).ok()?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).ok()?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(5))).ok()?;

    let request = format!(
        "HEAD / HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).ok()?;

    let reader = BufReader::new(stream);
    for line in reader.lines().take(20) {
        let line = line.ok()?;
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("x-amz-bucket-region: ") {
            return Some(value.trim().to_string());
        }
    }
    None
}

fn parse_azure(input: &str) -> anyhow::Result<StoreLocation> {
    let url = url::Url::parse(input)
        .with_context(|| format!("Invalid URL: {input}"))?;
    let (store, base_path) = object_store::parse_url(&url)
        .with_context(|| format!("Could not create Azure store for {input}"))?;
    Ok(StoreLocation::Cloud {
        url: input.to_string(),
        store: Arc::new(store),
        base_path,
    })
}

fn parse_http(input: &str) -> anyhow::Result<StoreLocation> {
    let url = url::Url::parse(input)
        .with_context(|| format!("Invalid URL: {input}"))?;
    let (store, base_path) = object_store::parse_url(&url)
        .with_context(|| format!("Could not create HTTP store for {input}"))?;
    Ok(StoreLocation::Cloud {
        url: input.to_string(),
        store: Arc::new(store),
        base_path,
    })
}

fn default_gcp_creds_exist() -> bool {
    if let Some(config_dir) = std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".config/gcloud/application_default_credentials.json"))
    {
        config_dir.exists()
    } else {
        false
    }
}

fn abbreviate_path(path: &std::path::Path) -> String {
    if let Ok(home) = std::env::var("HOME") {
        let home_path = std::path::Path::new(&home);
        if let Ok(rel) = path.strip_prefix(home_path) {
            return format!("~/{}", rel.display());
        }
    }
    path.display().to_string()
}
