//! Configuration assembled at app startup.

use crate::props::prop::BoxedJsonFuture;
use crate::request::RequestInfo;
use crate::root_view::{MinimalRootView, RootView};
use crate::session::SessionStore;
use crate::shared::SharedProps;
use crate::ssr::SsrClient;
use std::borrow::Cow;
use std::future::Future;
use std::sync::Arc;

pub(crate) type SharedOnceFn = Arc<dyn Fn(&RequestInfo) -> BoxedJsonFuture + Send + Sync>;
type VersionFn = Arc<dyn Fn() -> Cow<'static, str> + Send + Sync>;

/// Top-level app config. Built once at startup, cloned by Arc into each request.
#[derive(Clone)]
pub struct InertiaConfig {
    pub(crate) version: Option<VersionFn>,
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
    pub(crate) store_previous_url: bool,
    #[cfg(feature = "devtools")]
    pub(crate) devtools: Option<crate::devtools::DevTools>,
}

impl Default for InertiaConfig {
    fn default() -> Self {
        Self {
            version: None,
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
            store_previous_url: false,
            #[cfg(feature = "devtools")]
            devtools: None,
        }
    }
}

impl InertiaConfig {
    /// New config with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set asset version producer. Without one, the version comes from the
    /// root view ([`RootView::version`]; the manifest hash for a production
    /// [`crate::ViteRootView`]), and is `"1"` if the root view has none.
    pub fn version<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Cow<'static, str> + Send + Sync + 'static,
    {
        self.version = Some(Arc::new(f));
        self
    }

    /// Set a constant asset version.
    pub fn version_str(self, version: impl Into<Cow<'static, str>>) -> Self {
        let version = version.into();
        self.version(move || version.clone())
    }

    /// The asset version of this moment.
    pub(crate) fn current_version(&self) -> Cow<'static, str> {
        match &self.version {
            Some(f) => f(),
            None => self
                .root_view
                .version()
                .map_or(Cow::Borrowed("1"), Cow::Owned),
        }
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

    /// Set shared props from a [`SharedProps`] implementation. For a closure,
    /// use [`Self::share`].
    pub fn shared<P: SharedProps + 'static>(mut self, p: P) -> Self {
        self.shared = Some(Arc::new(p));
        self
    }

    /// Share props with every page. The closure runs on each page render and
    /// returns any `Serialize` value that serializes to an object:
    ///
    /// ```
    /// # use veer::InertiaConfig;
    /// # #[derive(Clone, serde::Serialize)] struct User { name: String }
    /// let config = InertiaConfig::new().share(|req| {
    ///     // Put there by your auth middleware.
    ///     let user = req.extension::<User>().cloned();
    ///     async move { serde_json::json!({ "auth": { "user": user } }) }
    /// });
    /// ```
    pub fn share<F, Fut, T>(self, f: F) -> Self
    where
        F: Fn(&RequestInfo) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = T> + Send + 'static,
        T: serde::Serialize,
    {
        self.shared(crate::shared::FnSharedProps(move |req: &RequestInfo| {
            let value = f(req);
            async { crate::props::prop::to_json(value.await) }
        }))
    }

    /// Share a once prop with every page. The closure runs only when the
    /// client does not hold the value yet. A handler prop with the same key wins.
    pub fn share_once<F, Fut, T>(mut self, key: impl Into<String>, f: F) -> Self
    where
        F: Fn(&RequestInfo) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = T> + Send + 'static,
        T: serde::Serialize,
    {
        self.shared_once.push((
            key.into(),
            Arc::new(move |req| {
                let value = f(req);
                Box::pin(async { crate::props::prop::to_json(value.await) })
            }),
        ));
        self
    }

    /// Store the URL of each page visit in the session, so that
    /// [`crate::Inertia::back`] has a target when the request has no `Referer`
    /// header. Needs a session store. Partial reloads and prefetches are not
    /// stored.
    pub fn store_previous_url(mut self, on: bool) -> Self {
        self.store_previous_url = on;
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
    #[cfg(feature = "devtools")]
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
        assert_eq!(c.current_version(), "v9");
    }

    #[test]
    fn version_defaults_to_the_root_view_then_to_1() {
        struct Versioned;
        impl RootView for Versioned {
            fn render(&self, _: crate::RootViewContext<'_>) -> Result<String, String> {
                Ok(String::new())
            }
            fn version(&self) -> Option<String> {
                Some("abc".into())
            }
        }
        assert_eq!(InertiaConfig::new().current_version(), "1");
        let c = InertiaConfig::new().root_view(Versioned);
        assert_eq!(c.current_version(), "abc");
        assert_eq!(c.version_str("v2").current_version(), "v2");
    }

    #[tokio::test]
    async fn share_reads_request_extensions() {
        let c = InertiaConfig::new().share(|req| {
            let user = req.extension::<&'static str>().copied();
            async move { serde_json::json!({ "user": user }) }
        });
        let mut extensions = http::Extensions::new();
        extensions.insert("ada");
        let req = RequestInfo::from_parts(http::Method::GET, "/".into(), &http::HeaderMap::new())
            .with_extensions(Arc::new(extensions));
        let shared = c.shared.unwrap().shared(&req).await;
        assert_eq!(shared.value, serde_json::json!({ "user": "ada" }));
    }
}
