//! Flash store backed by `tower-sessions`.
//!
//! Plug a `TowerSessionStore` into [`InertiaConfig::session`] when the app
//! already runs `tower_sessions::SessionManagerLayer` — flash data then
//! round-trips through whatever backend (Redis, Postgres, in-memory, …)
//! tower-sessions is configured with.
//!
//! **Layer order**: `SessionManagerLayer` must wrap **outside** [`InertiaLayer`]
//! so the per-request `Session` handle is present when this store reads and
//! writes flash.
//!
//! ```ignore
//! use axum::Router;
//! use time::Duration;
//! use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};
//! use veer::{session::tower::TowerSessionStore, InertiaConfig, InertiaLayer};
//!
//! let session_layer = SessionManagerLayer::new(MemoryStore::default())
//!     .with_expiry(Expiry::OnInactivity(Duration::minutes(30)));
//!
//! let cfg = InertiaConfig::new().session(TowerSessionStore::new());
//!
//! let app: Router = Router::new()
//!     .layer(InertiaLayer::new(cfg))
//!     .layer(session_layer);
//! ```
//!
//! [`InertiaConfig::session`]: crate::config::InertiaConfig::session
//! [`InertiaLayer`]: crate::adapters::axum::layer::InertiaLayer

use super::{Flash, SessionStore};
use async_trait::async_trait;
use http::{request::Parts as RequestParts, Extensions, HeaderMap};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tower_sessions::Session;

const DEFAULT_KEY: &str = "_veer_flash";

#[derive(Default, Serialize, Deserialize)]
struct StoredFlash {
    #[serde(default)]
    errors: HashMap<String, String>,
    #[serde(default)]
    bags: HashMap<String, serde_json::Value>,
}

impl From<StoredFlash> for Flash {
    fn from(s: StoredFlash) -> Self {
        Flash {
            errors: s.errors,
            bags: s.bags,
        }
    }
}

impl From<&Flash> for StoredFlash {
    fn from(f: &Flash) -> Self {
        StoredFlash {
            errors: f.errors.clone(),
            bags: f.bags.clone(),
        }
    }
}

/// One-shot flash store backed by `tower-sessions`.
#[derive(Clone, Debug)]
pub struct TowerSessionStore {
    key: String,
}

impl TowerSessionStore {
    /// Use the default session key (`_veer_flash`).
    pub fn new() -> Self {
        Self {
            key: DEFAULT_KEY.into(),
        }
    }

    /// Override the session key used to hold flash data.
    pub fn key(mut self, k: impl Into<String>) -> Self {
        self.key = k.into();
        self
    }
}

impl Default for TowerSessionStore {
    fn default() -> Self {
        Self::new()
    }
}

fn missing_session_warning() {
    tracing::error!(
        "veer: TowerSessionStore configured but tower-sessions Session not present in request extensions. \
         Make sure SessionManagerLayer is applied outside InertiaLayer."
    );
}

#[async_trait]
impl SessionStore for TowerSessionStore {
    async fn read_and_clear(&self, req: &RequestParts) -> Flash {
        let Some(session) = req.extensions.get::<Session>() else {
            missing_session_warning();
            return Flash::default();
        };
        match session.remove::<StoredFlash>(&self.key).await {
            Ok(Some(stored)) => stored.into(),
            Ok(None) => Flash::default(),
            Err(e) => {
                tracing::error!(error = %e, "veer: failed to read flash from tower-sessions");
                Flash::default()
            }
        }
    }

    async fn write(&self, _headers: &mut HeaderMap, req_extensions: &Extensions, flash: Flash) {
        if flash.is_empty() {
            // `read_and_clear` already removed the key. Nothing to write.
            return;
        }
        let Some(session) = req_extensions.get::<Session>() else {
            missing_session_warning();
            return;
        };
        let stored = StoredFlash::from(&flash);
        if let Err(e) = session.insert(&self.key, stored).await {
            tracing::error!(error = %e, "veer: failed to write flash to tower-sessions");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::Request;
    use tower_sessions::{MemoryStore, Session};

    fn make_session() -> Session {
        // Fresh anonymous session backed by an in-memory store.
        Session::new(None, std::sync::Arc::new(MemoryStore::default()), None)
    }

    fn parts_with_session(session: Session) -> RequestParts {
        let mut req = Request::builder().method("GET").uri("/").body(()).unwrap();
        req.extensions_mut().insert(session);
        req.into_parts().0
    }

    #[tokio::test]
    async fn write_then_read_roundtrips_flash() {
        let session = make_session();
        let store = TowerSessionStore::new();

        let mut flash = Flash::default();
        flash.errors.insert("email".into(), "invalid".into());
        flash
            .bags
            .insert("success".into(), serde_json::json!("ok"));

        let mut headers = HeaderMap::new();
        let mut exts = Extensions::new();
        exts.insert(session.clone());
        store.write(&mut headers, &exts, flash.clone()).await;

        // Same session handle on the next request — see what comes back.
        let parts = parts_with_session(session);
        let read = store.read_and_clear(&parts).await;
        assert_eq!(read.errors.get("email").map(String::as_str), Some("invalid"));
        assert_eq!(
            read.bags.get("success"),
            Some(&serde_json::json!("ok"))
        );

        // One-shot: a second read returns nothing.
        let read_again = store.read_and_clear(&parts).await;
        assert!(read_again.is_empty());
    }

    #[tokio::test]
    async fn missing_session_yields_empty_flash() {
        let store = TowerSessionStore::new();
        let req = Request::builder()
            .method("GET")
            .uri("/")
            .body(())
            .unwrap();
        let parts = req.into_parts().0;
        assert!(store.read_and_clear(&parts).await.is_empty());
    }

    #[tokio::test]
    async fn empty_flash_does_not_touch_session() {
        let store = TowerSessionStore::new();
        let session = make_session();
        let mut exts = Extensions::new();
        exts.insert(session.clone());
        let mut headers = HeaderMap::new();
        store.write(&mut headers, &exts, Flash::default()).await;
        // No-op: nothing written, session unchanged.
        let parts = parts_with_session(session);
        assert!(store.read_and_clear(&parts).await.is_empty());
    }

}
