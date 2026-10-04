//! `Inertia` request extractor.

use super::response::server_error;
use crate::config::InertiaConfig;
use crate::inertia::Inertia;
use crate::request::RequestInfo;
use crate::session::Flash;
use axum::response::{IntoResponse, Response};
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
    /// Id of the DevTools entry of this request, when the recorder is on.
    pub devtools_id: Option<String>,
    /// URL of the last page visit, when `store_previous_url` is on.
    pub previous_url: Option<String>,
}

/// Rejection of the [`Inertia`] extractor: the route is not inside an
/// [`InertiaLayer`](super::InertiaLayer). It is a `500`; a debug build puts
/// the cause in the response body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MissingInertiaLayer;

impl std::fmt::Display for MissingInertiaLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "veer: the `Inertia` extractor ran on a route that is not inside `InertiaLayer`. \
             Add `.layer(InertiaLayer::new(config))` to your axum Router, after the routes.",
        )
    }
}

impl IntoResponse for MissingInertiaLayer {
    fn into_response(self) -> Response {
        server_error(&self.to_string())
    }
}

impl<S> FromRequestParts<S> for Inertia
where
    S: Send + Sync,
{
    type Rejection = MissingInertiaLayer;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let per = parts
            .extensions
            .get::<PerRequest>()
            .cloned()
            .ok_or(MissingInertiaLayer)?;
        let url = RequestInfo::url_of(&parts.uri);
        let req_info = RequestInfo::from_parts(parts.method.clone(), url, &parts.headers)
            .with_extensions(per.req_extensions.clone());
        let previous_url = per.previous_url.clone();
        let mut inertia = Inertia::from_parts(per.config, req_info, (*per.flash).clone());
        inertia.previous_url = previous_url;
        Ok(inertia)
    }
}
