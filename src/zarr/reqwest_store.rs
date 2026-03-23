//! A read-only [`ObjectStore`] backed by [`reqwest`], for HTTP/HTTPS zarr stores.
//!
//! `object_store`'s built-in HTTP store requires `Content-Length` in every
//! response, but some CDNs (e.g. Cloudflare) omit it when using chunked
//! transfer encoding.  This store avoids that limitation by collecting the
//! full response body and deriving the size from the bytes received.

use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use futures::stream::BoxStream;
use object_store::path::Path;
use object_store::{
    Attributes, GetOptions, GetResult, GetResultPayload, ListResult, MultipartUpload, ObjectMeta,
    ObjectStore, PutMultipartOptions, PutOptions, PutPayload, PutResult, Result,
};
use std::fmt;

#[derive(Debug)]
pub struct ReqwestHttpStore {
    client: reqwest::Client,
    base_url: String,
}

impl ReqwestHttpStore {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Build the full URL for a given object path.
    fn url(&self, location: &Path) -> String {
        let path = location.as_ref();
        if path.is_empty() {
            self.base_url.clone()
        } else {
            format!("{}/{}", self.base_url, path)
        }
    }

    /// GET the full body at `location`, returning the bytes and HTTP status.
    async fn fetch(&self, location: &Path) -> Result<Bytes> {
        let url = self.url(location);
        let response =
            self.client
                .get(&url)
                .send()
                .await
                .map_err(|e| object_store::Error::Generic {
                    store: "ReqwestHTTP",
                    source: Box::new(e),
                })?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(object_store::Error::NotFound {
                path: location.to_string(),
                source: format!("404 Not Found: {url}").into(),
            });
        }
        if !status.is_success() {
            return Err(object_store::Error::Generic {
                store: "ReqwestHTTP",
                source: format!("HTTP {status} for {url}").into(),
            });
        }

        response
            .bytes()
            .await
            .map_err(|e| object_store::Error::Generic {
                store: "ReqwestHTTP",
                source: Box::new(e),
            })
    }
}

impl fmt::Display for ReqwestHttpStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ReqwestHttpStore({})", self.base_url)
    }
}

#[async_trait]
impl ObjectStore for ReqwestHttpStore {
    async fn get_opts(&self, location: &Path, options: GetOptions) -> Result<GetResult> {
        // We only support simple, unconditional GETs.
        // Range requests: fetch the full body, then slice client-side.
        let bytes = self.fetch(location).await?;
        let total_size = bytes.len() as u64;

        let meta = ObjectMeta {
            location: location.clone(),
            last_modified: Utc::now(),
            size: total_size,
            e_tag: None,
            version: None,
        };

        // Apply byte range if requested.
        let (bytes, range) = match options.range {
            Some(get_range) => {
                let r =
                    get_range
                        .as_range(total_size)
                        .map_err(|e| object_store::Error::Generic {
                            store: "ReqwestHTTP",
                            source: Box::new(e),
                        })?;
                let sliced = bytes.slice(r.start as usize..r.end as usize);
                (sliced, r)
            }
            None => (bytes, 0..total_size),
        };

        let stream = futures::stream::once(async move { Ok(bytes) });
        Ok(GetResult {
            payload: GetResultPayload::Stream(Box::pin(stream)),
            meta,
            range,
            attributes: Attributes::default(),
        })
    }

    // ── Write / mutate operations — not supported ─────────────────

    async fn put_opts(
        &self,
        _location: &Path,
        _payload: PutPayload,
        _opts: PutOptions,
    ) -> Result<PutResult> {
        Err(object_store::Error::NotImplemented {
            operation: "put_opts".into(),
            implementer: self.to_string(),
        })
    }

    async fn put_multipart_opts(
        &self,
        _location: &Path,
        _opts: PutMultipartOptions,
    ) -> Result<Box<dyn MultipartUpload>> {
        Err(object_store::Error::NotImplemented {
            operation: "put_multipart_opts".into(),
            implementer: self.to_string(),
        })
    }

    fn delete_stream(
        &self,
        _locations: BoxStream<'static, Result<Path>>,
    ) -> BoxStream<'static, Result<Path>> {
        Box::pin(futures::stream::once(async {
            Err(object_store::Error::NotImplemented {
                operation: "delete_stream".into(),
                implementer: "ReqwestHttpStore".into(),
            })
        }))
    }

    fn list(&self, _prefix: Option<&Path>) -> BoxStream<'static, Result<ObjectMeta>> {
        Box::pin(futures::stream::empty())
    }

    async fn list_with_delimiter(&self, _prefix: Option<&Path>) -> Result<ListResult> {
        Ok(ListResult {
            common_prefixes: vec![],
            objects: vec![],
        })
    }

    async fn copy_opts(
        &self,
        _from: &Path,
        _to: &Path,
        _options: object_store::CopyOptions,
    ) -> Result<()> {
        Err(object_store::Error::NotImplemented {
            operation: "copy_opts".into(),
            implementer: self.to_string(),
        })
    }
}
