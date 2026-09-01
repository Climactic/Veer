//! Shared props: per-config props merged into every response.

use crate::request::RequestInfo;
use async_trait::async_trait;
use serde_json::Value;
use std::future::Future;
use std::sync::Arc;

/// Resolves shared props on each request.
///
/// Middleware-installed request values, such as a `tower_sessions::Session`,
/// can be read through [`RequestInfo::extension`].
#[async_trait]
pub trait SharedProps: Send + Sync {
    /// Produce the shared props for this request.
    async fn shared(&self, req: &RequestInfo) -> Value;
}

#[async_trait]
impl<P> SharedProps for Arc<P>
where
    P: SharedProps + ?Sized,
{
    async fn shared(&self, req: &RequestInfo) -> Value {
        self.as_ref().shared(req).await
    }
}

/// Adapter for `Fn(&RequestInfo) -> impl Future<Output = Value>`.
pub struct FnSharedProps<F>(pub F);

#[async_trait]
impl<F, Fut> SharedProps for FnSharedProps<F>
where
    F: Fn(&RequestInfo) -> Fut + Send + Sync,
    Fut: Future<Output = Value> + Send,
{
    async fn shared(&self, req: &RequestInfo) -> Value {
        (self.0)(req).await
    }
}

/// Helper to wrap a closure as a boxed [`SharedProps`] resolver.
pub fn shared_props_fn<F, Fut>(f: F) -> Arc<dyn SharedProps>
where
    F: Fn(&RequestInfo) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Value> + Send + 'static,
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
        assert_eq!(s.shared(&r).await, json!({"x": 1}));
    }
}
