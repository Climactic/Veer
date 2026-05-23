//! Session/flash store contract for cross-redirect prop data (errors, success messages).

#[cfg(feature = "cookie-session")]
pub mod cookie;

#[cfg(feature = "tower-sessions")]
pub mod tower;

use async_trait::async_trait;
use http::{request::Parts as RequestParts, Extensions, HeaderMap};
use serde_json::Value;
use std::collections::HashMap;

/// One-shot flash data carried between two requests via the session store.
#[derive(Debug, Default, Clone)]
pub struct Flash {
    /// Validation errors keyed by field name.
    pub errors: HashMap<String, String>,
    /// Other named flash bags (`success`, `info`, etc.).
    pub bags: HashMap<String, Value>,
}

impl Flash {
    /// `true` if there's nothing to write.
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty() && self.bags.is_empty()
    }
}

/// Contract for reading + writing one-shot flash data.
///
/// Implementations get the request side at read time and the response headers
/// plus a snapshot of the request extensions at write time. Cookie-backed
/// stores write to `headers`; stores that piggyback on session middleware
/// (`tower-sessions`, `axum-login`, …) read their session handle out of
/// `req_extensions` and mutate it directly.
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Read (and clear) the flash bag from incoming request parts.
    async fn read_and_clear(&self, req: &RequestParts) -> Flash;
    /// Persist the flash bag for the next request.
    ///
    /// `headers` is the outgoing response's header map. `req_extensions` is a
    /// clone of the incoming request's extensions, captured by `InertiaLayer`
    /// so session middlewares' per-request handles remain reachable.
    async fn write(&self, headers: &mut HeaderMap, req_extensions: &Extensions, flash: Flash);
}
