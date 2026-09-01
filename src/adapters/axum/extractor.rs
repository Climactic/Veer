//! `Inertia` request extractor.

use crate::config::InertiaConfig;
use crate::inertia::Inertia;
use crate::request::RequestInfo;
use crate::session::Flash;
use axum::{extract::FromRequestParts, http::request::Parts};
use http::Extensions;
use std::sync::Arc;

/// Stored in request extensions by `InertiaLayer`.
#[derive(Clone)]
pub(crate) struct PerRequest {
    pub config: Arc<InertiaConfig>,
    pub flash: Arc<Flash>,
    /// Snapshot of the incoming request extensions. Kept so the response-side
    /// `SessionStore::write` hook can recover session middleware handles
    /// (`tower-sessions::Session`, etc.) that live in request extensions.
    pub req_extensions: Arc<Extensions>,
}

impl<S> FromRequestParts<S> for Inertia
where
    S: Send + Sync,
{
    type Rejection = axum::http::StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let per = parts
            .extensions
            .get::<PerRequest>()
            .cloned()
            .ok_or_else(|| {
                tracing::error!(
                    "veer: Inertia extractor invoked but `InertiaLayer` is not installed on this Router. Add `.layer(InertiaLayer::new(config))` to your axum app."
                );
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;
        let url = parts
            .uri
            .path_and_query()
            .map(|p| p.as_str().to_string())
            .unwrap_or_else(|| parts.uri.path().to_string());
        let req_info = RequestInfo::from_parts(parts.method.clone(), url, &parts.headers)
            .with_extensions((*per.req_extensions).clone());
        Ok(Inertia::from_parts(
            per.config,
            req_info,
            (*per.flash).clone(),
        ))
    }
}
