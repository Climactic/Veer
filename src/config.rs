//! Configuration assembled at app startup.

use crate::props::prop::BoxedJsonFuture;
use crate::request::RequestInfo;
use crate::root_view::{MinimalRootView, RootView};
use crate::session::SessionStore;
use crate::shared::SharedProps;
use crate::ssr::SsrClient;
use serde_json::Value;
use std::borrow::Cow;
use std::future::Future;
use std::sync::Arc;

pub(crate) type SharedOnceFn = Arc<dyn Fn(&RequestInfo) -> BoxedJsonFuture + Send + Sync>;
type VersionFn = Arc<dyn Fn() -> Cow<'static, str> + Send + Sync>;

/// Top-level app config. Built once at startup, cloned by Arc into each request.
#[derive(Clone)]
pub struct InertiaConfig {
    pub(crate) version: VersionFn,
    pub(crate) root_view: Arc<dyn RootView>,
    pub(crate) session: Option<Arc<dyn SessionStore>>,
    pub(crate) ssr: Option<Arc<dyn SsrClient>>,
    pub(crate) ssr_required: bool,
    pub(crate) csr_only: bool,
    pub(crate) shared: Option<Arc<dyn SharedProps>>,
    pub(crate) shared_once: Vec<(String, SharedOnceFn)>,
    pub(crate) encrypt_history: bool,
    pub(crate) preserve_big_integers: bool,
    pub(crate) with_all_errors: bool,
    pub(crate) devtools: Option<crate::devtools::DevTools>,
}

impl Default for InertiaConfig {
    fn default() -> Self {
        Self {
            version: Arc::new(|| Cow::Borrowed("1")),
            root_view: Arc::new(MinimalRootView::new()),
            session: None,
            ssr: None,
            ssr_required: false,
            csr_only: false,
            shared: None,
            shared_once: Vec::new(),
            encrypt_history: false,
            preserve_big_integers: false,
            with_all_errors: false,
            devtools: None,
        }
    }
}

impl InertiaConfig {
    /// New config with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set asset version producer.
    pub fn version<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Cow<'static, str> + Send + Sync + 'static,
    {
        self.version = Arc::new(f);
        self
    }

    /// Set the root view used for non-XHR responses.
    pub fn root_view<V: RootView + 'static>(mut self, v: V) -> Self {
        self.root_view = Arc::new(v);
        self
    }

    /// Set the session store used to round-trip flash data.
    pub fn session<S: SessionStore + 'static>(mut self, s: S) -> Self {
        self.session = Some(Arc::new(s));
        self
    }

    /// Set the SSR client.
    pub fn ssr<C: SsrClient + 'static>(mut self, c: C) -> Self {
        self.ssr = Some(Arc::new(c));
        self
    }

    /// If `true`, SSR failures return 500 instead of falling back to client-side render.
    pub fn ssr_required(mut self, required: bool) -> Self {
        self.ssr_required = required;
        self
    }

    /// Enable CSR-only mode: non-XHR GETs return JSON.
    pub fn csr_only(mut self, on: bool) -> Self {
        self.csr_only = on;
        self
    }

    /// Set shared props.
    pub fn shared<P: SharedProps + 'static>(mut self, p: P) -> Self {
        self.shared = Some(Arc::new(p));
        self
    }

    /// Share a once prop with every page. The closure runs only when the
    /// client does not hold the value yet. A handler prop with the same key wins.
    pub fn share_once<F, Fut>(mut self, key: impl Into<String>, f: F) -> Self
    where
        F: Fn(&RequestInfo) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Value> + Send + 'static,
    {
        self.shared_once
            .push((key.into(), Arc::new(move |req| Box::pin(f(req)))));
        self
    }

    /// Set `encryptHistory: true` on every page.
    pub fn encrypt_history(mut self, on: bool) -> Self {
        self.encrypt_history = on;
        self
    }

    /// Record each request for the Inertia DevTools browser extension. The
    /// read API is open unless you set [`crate::DevTools::authorize`], so
    /// enable this in development only:
    ///
    /// ```
    /// # use veer::{DevTools, InertiaConfig};
    /// let mut config = InertiaConfig::new();
    /// if cfg!(debug_assertions) {
    ///     config = config.devtools(DevTools::new());
    /// }
    /// ```
    pub fn devtools(mut self, devtools: crate::devtools::DevTools) -> Self {
        self.devtools = Some(devtools);
        self
    }

    /// Send all validation messages of a field as an array in `props.errors`.
    /// The default sends the first message as a string.
    pub fn with_all_errors(mut self, on: bool) -> Self {
        self.with_all_errors = on;
        self
    }

    /// Send integers outside the JavaScript safe range as `$bigint` markers on
    /// every page. [`crate::InertiaResponse::preserve_big_integers`] overrides
    /// this for one response.
    pub fn preserve_big_integers(mut self, on: bool) -> Self {
        self.preserve_big_integers = on;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_chains() {
        let c = InertiaConfig::new()
            .version(|| "v9".into())
            .csr_only(true)
            .ssr_required(true);
        assert!(c.csr_only);
        assert!(c.ssr_required);
        assert_eq!((c.version)(), "v9");
    }
}
