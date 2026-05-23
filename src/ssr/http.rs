//! Default SSR client over HTTP (talks to @inertiajs/server on Node or Bun).

use super::{SsrClient, SsrError, SsrPayload};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

/// HTTP-backed SSR client. Compatible with `@inertiajs/server` (Node or Bun).
#[derive(Debug, Clone)]
pub struct HttpSsrClient {
    client: reqwest::Client,
    url: String,
}

impl HttpSsrClient {
    /// Default URL `http://127.0.0.1:13714/render`.
    pub fn new(url: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("reqwest client");
        Self {
            client,
            url: url.into(),
        }
    }

    /// Construct with a custom `reqwest::Client`.
    pub fn with_client(client: reqwest::Client, url: impl Into<String>) -> Self {
        Self {
            client,
            url: url.into(),
        }
    }
}

#[derive(Deserialize)]
struct WireResponse {
    #[serde(default)]
    head: Vec<String>,
    #[serde(default)]
    body: String,
}

#[async_trait]
impl SsrClient for HttpSsrClient {
    async fn render(&self, page: &Value) -> Result<SsrPayload, SsrError> {
        let resp = self
            .client
            .post(&self.url)
            .json(page)
            .send()
            .await
            .map_err(|e| SsrError::Transport(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(SsrError::Transport(format!("status {}", resp.status())));
        }
        let body: WireResponse = resp
            .json()
            .await
            .map_err(|e| SsrError::Decode(e.to_string()))?;
        Ok(SsrPayload {
            head: body.head,
            body: body.body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::net::SocketAddr;
    use tokio::net::TcpListener;

    async fn spawn_stub() -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;
                let body = br#"{"head":["<title>x</title>"],"body":"<div>hello</div>"}"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                );
                use tokio::io::AsyncWriteExt;
                let _ = stream.write_all(resp.as_bytes()).await;
                let _ = stream.write_all(body).await;
                let _ = stream.shutdown().await;
            }
        });
        (addr, handle)
    }

    #[tokio::test]
    async fn talks_to_inertia_server_style_endpoint() {
        let (addr, _h) = spawn_stub().await;
        let client = HttpSsrClient::new(format!("http://{addr}/render"));
        let payload = client
            .render(&json!({"component": "X", "props": {}}))
            .await
            .unwrap();
        assert_eq!(payload.body, "<div>hello</div>");
        assert_eq!(payload.head, vec!["<title>x</title>".to_string()]);
    }
}
