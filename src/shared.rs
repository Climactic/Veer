//! Shared props: per-config props merged into every response.

use crate::props::closure::{LazyProp, OnceProp};
use crate::request::RequestInfo;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

/// Shared values and on-demand resolvers, merged underneath page props.
pub struct SharedPropsData {
    pub(crate) value: Value,
    pub(crate) props: HashMap<String, LazyProp>,
    pub(crate) once: HashMap<String, OnceProp>,
    pub(crate) lazies: HashMap<String, LazyProp>,
}

impl SharedPropsData {
    /// Start with eagerly resolved shared values.
    pub fn new(value: Value) -> Self {
        Self {
            value,
            props: HashMap::new(),
            once: HashMap::new(),
            lazies: HashMap::new(),
        }
    }

    /// Resolve an ordinary prop on full visits and partial reloads that include it.
    /// Unlike `lazy`/`optional`, this runs on the first visit. Unlike `once`, its
    /// value is not cached across visits. Unrequested closures are never invoked.
    pub fn prop<F, Fut>(mut self, key: impl Into<String>, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Value> + Send + 'static,
    {
        self.props.insert(
            key.into(),
            LazyProp {
                closure: Box::new(|| Box::pin(f())),
            },
        );
        self
    }

    /// Remember a prop across visits until the client explicitly requests it again.
    pub fn once<F, Fut>(self, key: impl Into<String>, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Value> + Send + 'static,
    {
        let key = key.into();
        self.once_as(key.clone(), key, f)
    }

    /// Remember a prop under a custom key, for example one scoped to an organisation.
    pub fn once_as<F, Fut>(
        mut self,
        prop: impl Into<String>,
        cache_key: impl Into<String>,
        f: F,
    ) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Value> + Send + 'static,
    {
        self.once.insert(
            prop.into(),
            OnceProp {
                key: cache_key.into(),
                closure: Box::new(|| Box::pin(f())),
            },
        );
        self
    }

    /// Include this value only when explicitly requested by a partial reload.
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
}

impl From<Value> for SharedPropsData {
    fn from(value: Value) -> Self {
        Self::new(value)
    }
}

/// Resolves shared props on each request.
///
/// Middleware-installed request values, such as a `tower_sessions::Session`,
/// can be read through [`RequestInfo::extension`].
#[async_trait]
pub trait SharedProps: Send + Sync {
    /// Produce the shared props for this request.
    async fn shared(&self, req: &RequestInfo) -> SharedPropsData;
}

#[async_trait]
impl<P> SharedProps for Arc<P>
where
    P: SharedProps + ?Sized,
{
    async fn shared(&self, req: &RequestInfo) -> SharedPropsData {
        self.as_ref().shared(req).await
    }
}

/// Adapter for a resolver returning JSON values or [`SharedPropsData`].
pub struct FnSharedProps<F>(pub F);

#[async_trait]
impl<F, Fut, R> SharedProps for FnSharedProps<F>
where
    F: Fn(&RequestInfo) -> Fut + Send + Sync,
    Fut: Future<Output = R> + Send,
    R: Into<SharedPropsData> + Send,
{
    async fn shared(&self, req: &RequestInfo) -> SharedPropsData {
        (self.0)(req).await.into()
    }
}

/// Helper to wrap a closure as a boxed [`SharedProps`] resolver.
pub fn shared_props_fn<F, Fut, R>(f: F) -> Arc<dyn SharedProps>
where
    F: Fn(&RequestInfo) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = R> + Send + 'static,
    R: Into<SharedPropsData> + Send,
{
    Arc::new(FnSharedProps(f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderMap;
    use serde_json::json;

    #[tokio::test]
    async fn fn_shared_props_works() {
        let s = shared_props_fn(|_r| async { json!({"x": 1}) });
        let r = RequestInfo::from_parts(http::Method::GET, "/".into(), &HeaderMap::new());
        assert_eq!(s.shared(&r).await.value, json!({"x": 1}));
    }
}
