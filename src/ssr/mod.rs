//! Server-side rendering integration.

#[cfg(feature = "ssr")]
pub mod http;

use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;

/// SSR-rendered HTML chunks returned by the SSR service.
#[derive(Debug, Clone, Default)]
pub struct SsrPayload {
    /// HTML strings to inject into `<head>`.
    pub head: Vec<String>,
    /// HTML to put inside the mount element.
    pub body: String,
}

/// Errors from the SSR client.
#[derive(Debug, Error)]
pub enum SsrError {
    /// Transport-level failure (network, timeout, status).
    #[error("ssr transport: {0}")]
    Transport(String),
    /// Response body could not be parsed as expected.
    #[error("ssr decode: {0}")]
    Decode(String),
}

/// A pluggable SSR renderer.
#[async_trait]
pub trait SsrClient: Send + Sync {
    /// Render `page` to head + body HTML chunks.
    async fn render(&self, page: &Value) -> Result<SsrPayload, SsrError>;
}
