//! The per-request `Inertia` facade.

use crate::config::InertiaConfig;
use crate::protocol::Redirect;
use crate::request::RequestInfo;
use crate::response::InertiaResponse;
use crate::session::Flash;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

/// Request-scoped handle for Inertia operations.
#[derive(Clone)]
pub struct Inertia {
    #[allow(dead_code)]
    pub(crate) config: Arc<InertiaConfig>,
    pub(crate) request: Arc<RequestInfo>,
    pub(crate) incoming_flash: Arc<Flash>,
}

impl Inertia {
    /// Construct from parts; used by adapters.
    pub fn from_parts(
        config: Arc<InertiaConfig>,
        request: RequestInfo,
        incoming_flash: Flash,
    ) -> Self {
        Self {
            config,
            request: Arc::new(request),
            incoming_flash: Arc::new(incoming_flash),
        }
    }

    /// The parsed request info.
    pub fn request(&self) -> &RequestInfo {
        &self.request
    }

    /// The incoming flash bag (errors + named flash bags from the previous request).
    pub fn incoming_flash(&self) -> &Flash {
        &self.incoming_flash
    }

    /// Render a component with strongly-typed props.
    pub fn render<P: Serialize>(&self, component: impl Into<String>, props: P) -> InertiaResponse {
        let value = match serde_json::to_value(&props) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, "veer: failed to serialize props for render; using null");
                Value::Null
            }
        };
        InertiaResponse::new(component, value)
    }

    /// Internal redirect (303 on POST/PUT/PATCH/DELETE; 302-equivalent SeeOther on GET).
    pub fn redirect(&self, location: impl Into<String>) -> InertiaResponse {
        let mut r = InertiaResponse::new(String::new(), Value::Null);
        r.redirect = Some(Redirect::Internal(location.into()));
        r
    }

    /// Convenience: start an `InertiaResponse` with validation errors pre-attached.
    ///
    /// Typical usage: `inertia.with_errors(errors).redirect("/form")`.
    pub fn with_errors<E: crate::errors::IntoErrorBag>(&self, errors: E) -> InertiaResponse {
        let mut r = InertiaResponse::new(String::new(), serde_json::Value::Null);
        r.pending_flash.errors.extend(errors.into_error_bag());
        r
    }

    /// External redirect (turns into 409 + `X-Inertia-Location`).
    pub fn location(&self, location: impl Into<String>) -> InertiaResponse {
        let mut r = InertiaResponse::new(String::new(), Value::Null);
        r.redirect = Some(Redirect::External(location.into()));
        r
    }

    /// Redirect to the `Referer` header value, or `/` if absent.
    ///
    /// Useful for POST-then-redirect-back flows: submit a form, then call
    /// `inertia.back()` to send the user back to the page they came from.
    /// Without a `Referer` header (e.g. direct navigation) the redirect falls
    /// back to `/`.
    pub fn back(&self) -> InertiaResponse {
        let to = self
            .request
            .referer
            .clone()
            .unwrap_or_else(|| "/".to_string());
        self.redirect(to)
    }
}
