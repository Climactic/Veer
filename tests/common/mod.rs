use async_trait::async_trait;
use http::{Extensions, HeaderMap, Request};
use std::sync::Arc;
use tokio::sync::Mutex;
use veer::session::{Flash, SessionStore};
use veer::ssr::{SsrClient, SsrError, SsrPayload};

#[derive(Clone, Default)]
pub struct MockSession {
    pub store: Arc<Mutex<Flash>>,
}

#[async_trait]
impl SessionStore for MockSession {
    async fn read_and_clear(&self, _req: &http::request::Parts) -> Flash {
        let mut g = self.store.lock().await;
        std::mem::take(&mut *g)
    }
    async fn write(&self, _h: &mut HeaderMap, _ext: &Extensions, flash: Flash) {
        let mut g = self.store.lock().await;
        *g = flash;
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct StubSsr;

#[async_trait]
impl SsrClient for StubSsr {
    async fn render(&self, _page: &serde_json::Value) -> Result<SsrPayload, SsrError> {
        Ok(SsrPayload {
            head: vec!["<meta name=\"stub\" content=\"1\">".into()],
            body: "<div>ssr-body</div>".into(),
        })
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct FailingSsr;

#[async_trait]
impl SsrClient for FailingSsr {
    async fn render(&self, _page: &serde_json::Value) -> Result<SsrPayload, SsrError> {
        Err(SsrError::Transport("simulated failure".into()))
    }
}

pub fn req(method: &str, uri: &str) -> Request<axum::body::Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(axum::body::Body::empty())
        .unwrap()
}

pub fn req_inertia(method: &str, uri: &str, version: &str) -> Request<axum::body::Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("x-inertia", "true")
        .header("x-inertia-version", version)
        .body(axum::body::Body::empty())
        .unwrap()
}
