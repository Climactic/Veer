//! The response value handlers return.

use crate::props::resolver::MergeLabels;
use crate::props::Prop;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;

/// The mutable response builder returned by `Inertia::render`.
pub struct InertiaResponse {
    pub(crate) component: String,
    pub(crate) base_props: Value,
    pub(crate) props: HashMap<String, Prop>,
    pub(crate) merge: MergeLabels,
    pub(crate) encrypt_history: bool,
    pub(crate) clear_history: bool,
    pub(crate) preserve_fragment: bool,
    pub(crate) preserve_big_integers: Option<bool>,
    pub(crate) skip_ssr: bool,
    pub(crate) redirect: Option<crate::protocol::Redirect>,
    pub(crate) pending_flash: crate::session::Flash,
    pub(crate) status: Option<http::StatusCode>,
    /// The props did not serialize; the response is a 500.
    pub(crate) props_error: Option<String>,
    /// Where `Inertia::render` was called (for DevTools).
    #[cfg_attr(not(feature = "devtools"), allow(dead_code))]
    pub(crate) render_source: Option<&'static std::panic::Location<'static>>,
}

impl InertiaResponse {
    pub(crate) fn new(component: impl Into<String>, base_props: Value) -> Self {
        Self {
            component: component.into(),
            base_props,
            props: HashMap::new(),
            merge: MergeLabels::default(),
            encrypt_history: false,
            clear_history: false,
            preserve_fragment: false,
            preserve_big_integers: None,
            skip_ssr: false,
            redirect: None,
            pending_flash: Default::default(),
            status: None,
            props_error: None,
            render_source: None,
        }
    }

    /// Render a component with props. The same as [`crate::Inertia::render`],
    /// for code that has no `Inertia` handle, such as the `IntoResponse` impl
    /// of an application error type.
    #[track_caller]
    pub fn render<P: serde::Serialize>(component: impl Into<String>, props: P) -> Self {
        let (value, error) = match serde_json::to_value(&props) {
            Ok(v) => (v, None),
            Err(e) => (Value::Null, Some(e.to_string())),
        };
        let mut response = Self::new(component, value);
        response.props_error = error;
        response.render_source = Some(std::panic::Location::caller());
        response
    }

    /// Set the HTTP status of a page response (for example `404` for an error
    /// page). The default is `200`.
    pub fn status(mut self, status: http::StatusCode) -> Self {
        self.status = Some(status);
        self
    }

    /// Attach a closure-resolved [`Prop`] under a key. A dot path (`auth.perms`)
    /// puts it inside a nested object. It replaces a plain value at the same path.
    pub fn prop(mut self, key: impl Into<String>, prop: Prop) -> Self {
        self.props.insert(key.into(), prop);
        self
    }

    /// Attach an ordinary fallible prop, preserving application HTTP errors.
    #[cfg(feature = "axum")]
    pub fn try_prop<F, Fut, T, E>(self, key: impl Into<String>, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        T: serde::Serialize,
        E: axum::response::IntoResponse,
    {
        self.prop(key, Prop::try_response(f))
    }

    /// Attach an optional fallible prop, preserving application HTTP errors.
    #[cfg(feature = "axum")]
    pub fn try_optional<F, Fut, T, E>(self, key: impl Into<String>, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        T: serde::Serialize,
        E: axum::response::IntoResponse,
    {
        self.prop(key, Prop::try_response(f).optional())
    }

    /// Load a page and its scroll metadata only when selected.
    #[cfg(feature = "axum")]
    pub fn try_scroll<F, Fut, T, E>(self, key: impl Into<String>, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<(T, crate::ScrollMetadata), E>> + Send + 'static,
        T: serde::Serialize,
        E: axum::response::IntoResponse,
    {
        self.prop(key, Prop::try_scroll(f))
    }

    /// Attach a lazy prop (default-excluded; included only on partial reload that names it).
    ///
    /// Inertia v3 calls this concept "optional". `lazy()` is the preferred method name in
    /// this crate; `optional()` is provided as a direct alias for ergonomics.
    pub fn lazy<F, Fut, T>(self, key: impl Into<String>, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
        T: serde::Serialize,
    {
        self.prop(key, Prop::new(f).optional())
    }

    /// Attach an optional prop (default-excluded; included only on partial reload that names it).
    ///
    /// Inertia v3 calls this "optional". This method is an alias for [`Self::lazy`]; both
    /// route through the same internal map and behave identically.
    pub fn optional<F, Fut, T>(self, key: impl Into<String>, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
        T: serde::Serialize,
    {
        self.lazy(key, f)
    }

    /// Attach a deferred prop.
    pub fn deferred<F, Fut, T>(self, key: impl Into<String>, group: impl Into<String>, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
        T: serde::Serialize,
    {
        self.prop(key, Prop::new(f).group(group))
    }

    /// Attach a once prop: resolved one time, then remembered by the client
    /// across pages. Use [`Self::prop`] for a custom key or an expiry.
    pub fn once<F, Fut, T>(self, key: impl Into<String>, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
        T: serde::Serialize,
    {
        self.prop(key, Prop::new(f).once())
    }

    /// Mark a prop path as merge-mode: the client appends to its existing state.
    /// A dot path merges a nested array (`posts.data`).
    pub fn merge(mut self, path: impl Into<String>) -> Self {
        self.merge.append.insert(path.into());
        self
    }

    /// Mark a prop path as prepend-mode: the client prepends to its existing state.
    pub fn prepend(mut self, path: impl Into<String>) -> Self {
        self.merge.prepend.insert(path.into());
        self
    }

    /// Mark a prop path as deep-merge: the client merges objects recursively.
    pub fn deep_merge(mut self, path: impl Into<String>) -> Self {
        self.merge.deep.insert(path.into());
        self
    }

    /// Identify items during a merge: `<propPath>.<keyField>`, e.g. `posts.id`.
    pub fn match_on(mut self, path: impl Into<String>) -> Self {
        self.merge.match_on.insert(path.into());
        self
    }

    /// Set `encryptHistory: true` in the page object (v2+ client primitive).
    pub fn encrypt_history(mut self) -> Self {
        self.encrypt_history = true;
        self
    }

    /// Set `clearHistory: true`. On a redirect, the flag travels with the flash
    /// data to the page that the redirect lands on.
    pub fn clear_history(mut self) -> Self {
        self.clear_history = true;
        self
    }

    /// Keep the URL fragment of the original request. On a redirect, the flag
    /// travels with the flash data to the page that the redirect lands on.
    pub fn preserve_fragment(mut self) -> Self {
        self.preserve_fragment = true;
        self
    }

    /// Send integers outside the JavaScript safe range as `$bigint` markers,
    /// which the client revives as `BigInt`. Overrides the config default.
    pub fn preserve_big_integers(mut self, preserve: bool) -> Self {
        self.preserve_big_integers = Some(preserve);
        self
    }

    /// Skip SSR for this response only.
    pub fn no_ssr(mut self) -> Self {
        self.skip_ssr = true;
        self
    }

    /// Attach validation errors to be flashed for the next request.
    pub fn with_errors<E: crate::errors::IntoErrorBag>(mut self, errors: E) -> Self {
        self.pending_flash.errors.extend(errors.into_all_errors());
        self
    }

    /// Attach flash data. On a redirect it goes to the next page; on a render it
    /// goes into the `flash` of this page.
    pub fn with_flash(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.pending_flash.bags.insert(key.into(), value.into());
        self
    }

    /// Set an internal redirect destination (303 See Other). A target with a
    /// URL fragment becomes 409 + `X-Inertia-Redirect` for Inertia requests.
    pub fn redirect(mut self, location: impl Into<String>) -> Self {
        self.redirect = Some(crate::protocol::Redirect::Internal(location.into()));
        self
    }

    /// Set an external redirect destination (409 + `X-Inertia-Location` for
    /// Inertia requests, a plain 302 otherwise).
    pub fn location(mut self, location: impl Into<String>) -> Self {
        self.redirect = Some(crate::protocol::Redirect::External(location.into()));
        self
    }
}
