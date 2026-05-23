//! Shared props: per-config props merged into every response.

use crate::request::RequestInfo;
use async_trait::async_trait;
use serde_json::Value;
use std::future::Future;
use std::sync::Arc;

/// Resolves shared props on each request.
#[async_trait]
pub trait SharedProps: Send + Sync {
    /// Produce the shared props for this request.
    async fn shared(&self, req: &RequestInfo) -> Value;
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

/// Helper to wrap a closure as a boxed `SharedProps` trait object.
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
