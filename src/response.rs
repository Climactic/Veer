//! The response value handlers return.

use crate::props::closure::{DeferredProp, LazyProp};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::future::Future;

/// The mutable response builder returned by `Inertia::render`.
pub struct InertiaResponse {
    pub(crate) component: String,
    pub(crate) base_props: Value,
    pub(crate) lazies: HashMap<String, LazyProp>,
    pub(crate) deferreds: HashMap<String, DeferredProp>,
    pub(crate) merges: HashSet<String>,
    pub(crate) encrypt_history: bool,
    pub(crate) clear_history: bool,
    pub(crate) reset_merge_props: Vec<String>,
    pub(crate) skip_ssr: bool,
    pub(crate) redirect: Option<crate::protocol::Redirect>,
    pub(crate) pending_flash: crate::session::Flash,
}

impl InertiaResponse {
    pub(crate) fn new(component: impl Into<String>, base_props: Value) -> Self {
        Self {
            component: component.into(),
            base_props,
            lazies: HashMap::new(),
            deferreds: HashMap::new(),
            merges: HashSet::new(),
            encrypt_history: false,
            clear_history: false,
            reset_merge_props: Vec::new(),
            skip_ssr: false,
            redirect: None,
            pending_flash: Default::default(),
        }
    }

    /// Attach a lazy prop (default-excluded; included only on partial reload that names it).
    ///
    /// Inertia v3 calls this concept "optional". `lazy()` is the preferred method name in
    /// this crate; `optional()` is provided as a direct alias for ergonomics.
    pub fn lazy<F, Fut>(mut self, key: impl Into<String>, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Value> + Send + 'static,
    {
        self.lazies.insert(
            key.into(),
            LazyProp {
                closure: Box::new(|| Box::pin(f())),
            },
        );
        self
    }

    /// Attach an optional prop (default-excluded; included only on partial reload that names it).
    ///
    /// Inertia v3 calls this "optional". This method is an alias for [`Self::lazy`]; both
    /// route through the same internal map and behave identically.
    pub fn optional<F, Fut>(self, key: impl Into<String>, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Value> + Send + 'static,
    {
        self.lazy(key, f)
    }

    /// Attach a deferred prop.
    pub fn deferred<F, Fut>(mut self, key: impl Into<String>, group: &'static str, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Value> + Send + 'static,
    {
        self.deferreds.insert(
            key.into(),
            DeferredProp {
                group,
                closure: Box::new(|| Box::pin(f())),
            },
        );
        self
    }

    /// Mark a top-level key as merge-mode (client merges into existing state).
    pub fn merge(mut self, key: impl Into<String>) -> Self {
        self.merges.insert(key.into());
        self
    }

    /// Set `encryptHistory: true` in the page object (v2+ client primitive).
    pub fn encrypt_history(mut self) -> Self {
        self.encrypt_history = true;
        self
    }

    /// Set `clearHistory: true`.
    pub fn clear_history(mut self) -> Self {
        self.clear_history = true;
        self
    }

    /// Reset merge state for specific keys.
    pub fn reset_merge(mut self, keys: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.reset_merge_props
            .extend(keys.into_iter().map(Into::into));
        self
    }

    /// Skip SSR for this response only.
    pub fn no_ssr(mut self) -> Self {
        self.skip_ssr = true;
        self
    }

    /// Attach validation errors to be flashed for the next request.
    pub fn with_errors<E: crate::errors::IntoErrorBag>(mut self, errors: E) -> Self {
        self.pending_flash.errors.extend(errors.into_error_bag());
        self
    }

    /// Attach a named flash bag entry.
    pub fn with_flash(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.pending_flash.bags.insert(key.into(), value.into());
        self
    }

    /// Set an internal redirect destination (303 SeeOther on POST/PUT/PATCH/DELETE).
    pub fn redirect(mut self, location: impl Into<String>) -> Self {
        self.redirect = Some(crate::protocol::Redirect::Internal(location.into()));
        self
    }

    /// Set an external redirect destination (409 + `X-Inertia-Location`).
    pub fn location(mut self, location: impl Into<String>) -> Self {
        self.redirect = Some(crate::protocol::Redirect::External(location.into()));
        self
    }
}
