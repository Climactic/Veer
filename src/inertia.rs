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
    pub(crate) previous_url: Option<String>,
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
            previous_url: None,
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
    #[track_caller]
    pub fn render<P: Serialize>(&self, component: impl Into<String>, props: P) -> InertiaResponse {
        InertiaResponse::render(component, props)
    }

    /// Render the page that `props` belongs to. The component name comes from
    /// [`register_page!`](crate::register_page), so it is written one time.
    #[cfg(feature = "ts")]
    #[track_caller]
    pub fn page<P>(&self, props: P) -> InertiaResponse
    where
        P: Serialize + crate::bindings::InertiaPageProps,
    {
        InertiaResponse::render(P::COMPONENT, props)
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
        r.pending_flash.errors.extend(errors.into_all_errors());
        r
    }

    /// External redirect (turns into 409 + `X-Inertia-Location`).
    pub fn location(&self, location: impl Into<String>) -> InertiaResponse {
        let mut r = InertiaResponse::new(String::new(), Value::Null);
        r.redirect = Some(Redirect::External(location.into()));
        r
    }

    /// Redirect to the page that the user came from: the `Referer` header,
    /// then the session's previous URL (see
    /// [`InertiaConfig::store_previous_url`]), then `/`.
    ///
    /// Useful for POST-then-redirect-back flows: submit a form, then call
    /// `inertia.back()` to send the user back to the page they came from.
    pub fn back(&self) -> InertiaResponse {
        let to = self
            .request
            .referer
            .clone()
            .or_else(|| self.previous_url.clone())
            .unwrap_or_else(|| "/".to_string());
        self.redirect(to)
    }
}
