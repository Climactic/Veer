//! Tower layer that wires `InertiaConfig` + flash into request extensions, then
//! finalizes any `InertiaResponseMarker` produced by handlers.

use super::extractor::PerRequest;
use super::response::{finalize, InertiaResponseMarker};
use crate::config::InertiaConfig;
use crate::request::RequestInfo;
use crate::session::Flash;
use axum::body::Body;
use axum::http::{Method, Request, Response, StatusCode};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::{Layer, Service};

/// Tower layer that enables Inertia handling on a router.
#[derive(Clone)]
pub struct InertiaLayer {
    config: Arc<InertiaConfig>,
}

impl InertiaLayer {
    /// Wrap a router in an Inertia layer.
    pub fn new(config: InertiaConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }
}

impl<S> Layer<S> for InertiaLayer {
    type Service = InertiaMiddleware<S>;
    fn layer(&self, inner: S) -> Self::Service {
        InertiaMiddleware {
            inner,
            config: self.config.clone(),
        }
    }
}

#[doc(hidden)]
#[derive(Clone)]
pub struct InertiaMiddleware<S> {
    inner: S,
    config: Arc<InertiaConfig>,
}

impl<S> Service<Request<Body>> for InertiaMiddleware<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let cfg = self.config.clone();
        let mut inner = self.inner.clone();
        Box::pin(async move {
            // Read flash from session (if configured).
            let (mut parts, body) = req.into_parts();
            let flash = if let Some(s) = &cfg.session {
                s.read_and_clear(&parts).await
            } else {
                Flash::default()
            };
            // Capture data we'll need to rebuild RequestInfo for finalize.
            let method_for_post = parts.method.clone();
            let url_for_finalize = parts
                .uri
                .path_and_query()
                .map(|p| p.as_str().to_string())
                .unwrap_or_else(|| parts.uri.path().to_string());
            let headers_clone = parts.headers.clone();
            // Snapshot extensions so session stores that piggyback on
            // middleware-installed handles (e.g. tower-sessions::Session) can
            // still reach them on the response side.
            let extensions_snapshot = Arc::new(parts.extensions.clone());

            let per_request = PerRequest {
                config: cfg.clone(),
                flash: Arc::new(flash),
                req_extensions: extensions_snapshot.clone(),
            };
            parts.extensions.insert(per_request.clone());

            let request = Request::from_parts(parts, body);
            let mut resp = inner.call(request).await?;

            // If handler returned an Inertia marker, finalize.
            if let Some(marker) = resp.extensions_mut().remove::<InertiaResponseMarker>() {
                let req_info = RequestInfo::from_parts(
                    method_for_post.clone(),
                    url_for_finalize,
                    &headers_clone,
                );
                // Marker wraps Arc<Mutex<Option<InertiaResponse>>>. Take the inner value.
                let inner_response = marker
                    .0
                    .lock()
                    .expect("InertiaResponse mutex poisoned")
                    .take();
                if let Some(ir) = inner_response {
                    resp = finalize(ir, &per_request, &req_info).await;
                }
            } else {
                // If the handler returned a plain 302 from a non-GET, rewrite to 303 per Inertia spec.
                let status = resp.status();
                if status == StatusCode::FOUND
                    && matches!(
                        method_for_post,
                        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
                    )
                {
                    *resp.status_mut() = StatusCode::SEE_OTHER;
                }
            }

            Ok(resp)
        })
    }
}
