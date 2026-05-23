//! Configuration assembled at app startup.

use crate::root_view::{MinimalRootView, RootView};
use crate::session::SessionStore;
use crate::shared::SharedProps;
use crate::ssr::SsrClient;
use std::borrow::Cow;
use std::sync::Arc;

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
