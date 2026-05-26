//! Serve build assets embedded in the binary (rust-embed / `include_dir` /
//! a map) — the single-binary deploy counterpart to `ServeDir`.
//!
//! `EmbeddedAssets` is an infallible tower [`Service`] usable directly with
//! [`axum::Router::nest_service`]. It owns the boring parts (path stripping,
//! content-type, immutable cache headers, 404/405) and delegates byte lookup to
//! a resolver closure, so Veer depends on no embedding crate:
//!
//! ```ignore
//! #[derive(rust_embed::RustEmbed)]
//! #[folder = "dist/"]
//! struct Assets;
//!
//! router.nest_service("/build", EmbeddedAssets::new(|p| Assets::get(p).map(|f| f.data)))
//! ```

use axum::body::Body;
use axum::http::{header, HeaderValue, Method, Request, Response, StatusCode};
use bytes::Bytes;
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::Infallible;
use std::future::{ready, Ready};
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::Service;

type Resolver = dyn Fn(&str) -> Option<Cow<'static, [u8]>> + Send + Sync;

/// Tower service that serves embedded assets via a byte-resolver closure.
///
/// Responses carry `Cache-Control: public, max-age=31536000, immutable`, which
/// assumes Vite-style content-hashed filenames. Don't route non-fingerprinted
/// files (e.g. `robots.txt`, `manifest.webmanifest`) through this service —
/// they would be cached for a year with no revalidation path.
#[derive(Clone)]
pub struct EmbeddedAssets {
    resolver: Arc<Resolver>,
    mimes: Arc<HashMap<String, String>>,
}

impl EmbeddedAssets {
    /// Construct from a resolver. The closure receives the request path with
    /// the `nest_service` prefix already stripped and a leading `/` trimmed
    /// (e.g. `assets/app-AAA.js`) and returns the bytes, or `None` for 404.
    pub fn new<F>(resolver: F) -> Self
    where
        F: Fn(&str) -> Option<Cow<'static, [u8]>> + Send + Sync + 'static,
    {
        Self {
            resolver: Arc::new(resolver),
            mimes: Arc::new(HashMap::new()),
        }
    }

    /// Add or override an extension → content-type mapping (extension without
    /// the dot, e.g. `.mime("vtt", "text/vtt")`).
    pub fn mime(mut self, ext: impl Into<String>, content_type: impl Into<String>) -> Self {
        Arc::make_mut(&mut self.mimes).insert(ext.into(), content_type.into());
        self
    }

    fn content_type(&self, path: &str) -> String {
        let ext = path.rsplit('.').next().unwrap_or("");
        if let Some(ct) = self.mimes.get(ext) {
            return ct.clone();
        }
        builtin_mime(ext).to_string()
    }

    fn serve(&self, req: Request<Body>) -> Response<Body> {
        if !matches!(*req.method(), Method::GET | Method::HEAD) {
            return Response::builder()
                .status(StatusCode::METHOD_NOT_ALLOWED)
                .header(header::ALLOW, "GET, HEAD")
                .body(Body::empty())
                .unwrap();
        }
        let path = req.uri().path().trim_start_matches('/');
        match (self.resolver)(path) {
            Some(bytes) => {
                // A bad `.mime()` override must not panic the builder; fall
                // back to octet-stream if it isn't a valid header value.
                let ct = HeaderValue::try_from(self.content_type(path))
                    .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
                let len = bytes.len();
                let body = if *req.method() == Method::HEAD {
                    Body::empty()
                } else {
                    // Avoid copying for the common rust-embed case where the
                    // bytes are 'static borrowed; only Owned needs a move.
                    match bytes {
                        Cow::Borrowed(b) => Body::from(Bytes::from_static(b)),
                        Cow::Owned(v) => Body::from(v),
                    }
                };
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, ct)
                    .header(header::CONTENT_LENGTH, len)
                    .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
                    .body(body)
                    .unwrap()
            }
            None => status(StatusCode::NOT_FOUND),
        }
    }
}

impl Service<Request<Body>> for EmbeddedAssets {
    type Response = Response<Body>;
    type Error = Infallible;
    type Future = Ready<Result<Response<Body>, Infallible>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        ready(Ok(self.serve(req)))
    }
}

fn status(code: StatusCode) -> Response<Body> {
    Response::builder()
        .status(code)
        .body(Body::empty())
        .unwrap()
}

fn builtin_mime(ext: &str) -> &'static str {
    match ext {
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "html" | "htm" => "text/html; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "wasm" => "application/wasm",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
