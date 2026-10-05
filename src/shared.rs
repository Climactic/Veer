//! Shared props: per-config props merged into every response.

use crate::request::RequestInfo;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

/// Shared values and on-demand resolvers, merged underneath page props.
pub struct SharedPropsData {
    pub(crate) value: Value,
    pub(crate) props: HashMap<String, crate::Prop>,
}

impl SharedPropsData {
    /// Start with eagerly resolved shared values.
    pub fn new(value: Value) -> Self {
        Self {
            value,
            props: HashMap::new(),
        }
    }
    /// Attach a closure prop using the same semantics as page props.
    pub fn prop(mut self, key: impl Into<String>, prop: crate::Prop) -> Self {
        self.props.insert(key.into(), prop);
        self
    }
    /// Remember a shared prop across visits.
    pub fn once<F, Fut>(self, key: impl Into<String>, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Value> + Send + 'static,
    {
        self.prop(key, crate::Prop::new(f).once())
    }
    /// Remember a shared prop under a scoped cache key.
    pub fn once_as<F, Fut>(self, key: impl Into<String>, cache_key: impl Into<String>, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Value> + Send + 'static,
    {
        self.prop(key, crate::Prop::new(f).once_as(cache_key))
    }
    /// Include a shared prop only when explicitly requested.
    pub fn lazy<F, Fut>(self, key: impl Into<String>, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Value> + Send + 'static,
    {
        self.prop(key, crate::Prop::new(f).optional())
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
