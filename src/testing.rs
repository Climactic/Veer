//! Helpers for tests of your handlers (`testing` feature).
//!
//! ```ignore
//! use tower::ServiceExt;
//! use veer::testing::{visit, MemorySession, TestPage};
//!
//! let app = app(InertiaConfig::new().session(MemorySession::default()));
//! let response = app.oneshot(visit("GET", "/users")).await.unwrap();
//!
//! let page = TestPage::from_response(response).await;
//! assert_eq!(page.component, "Users/Index");
//! assert_eq!(page.prop("users.0.name"), Some(&json!("Ada")));
//! ```

use crate::session::{Flash, SessionStore};
use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, Response};
use http::{Extensions, HeaderMap};
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// An Inertia visit: a request with `X-Inertia: true` and an empty body.
///
/// It sends the asset version `"1"`, the default of [`crate::InertiaConfig`].
/// If your config has another version, replace the `X-Inertia-Version`
/// header; a `GET` with a different version gets a `409`. Use
/// [`axum::http::Request::builder`] for a request with a body.
pub fn visit(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(&crate::headers::X_INERTIA, "true")
        .header(&crate::headers::X_INERTIA_VERSION, "1")
        .body(Body::empty())
        .expect("veer::testing::visit: invalid method or URI")
}

/// A session store that keeps its data in memory. Clones share the data, so
/// one store carries flash data from a request to the request that follows.
#[derive(Clone, Default)]
pub struct MemorySession {
    flash: Arc<Mutex<Flash>>,
    previous_url: Arc<Mutex<Option<String>>>,
}

impl MemorySession {
    /// The flash data that the next request will read.
    pub fn flash(&self) -> Flash {
        self.flash.lock().unwrap().clone()
    }
}

#[async_trait]
impl SessionStore for MemorySession {
    async fn read_and_clear(&self, _req: &http::request::Parts) -> Flash {
        std::mem::take(&mut *self.flash.lock().unwrap())
    }

    async fn write(&self, _headers: &mut HeaderMap, _ext: &Extensions, flash: Flash) {
        *self.flash.lock().unwrap() = flash;
    }

    async fn previous_url(&self, _req: &http::request::Parts) -> Option<String> {
        self.previous_url.lock().unwrap().clone()
    }

    async fn store_previous_url(&self, _headers: &mut HeaderMap, _ext: &Extensions, url: &str) {
        *self.previous_url.lock().unwrap() = Some(url.to_owned());
    }
}

/// The page object of a response.
#[derive(Debug, Clone, PartialEq)]
pub struct TestPage {
    /// The component name.
    pub component: String,
    /// The props, with `errors` and the shared props.
    pub props: Value,
    /// The page URL.
    pub url: String,
    /// The complete page object.
    pub raw: Value,
}

impl TestPage {
    /// Read the page object from an Inertia JSON response or from the
    /// `<script data-page>` tag of an HTML response.
    ///
    /// # Panics
    ///
    /// Panics, with the status and the body, if the response has no page.
    pub async fn from_response(response: Response<Body>) -> Self {
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("veer::testing: could not read the response body");
        let body = String::from_utf8_lossy(&bytes);
        let raw: Value = serde_json::from_str(&body)
            .ok()
            .or_else(|| serde_json::from_str(script_tag(&body)?).ok())
            .filter(|page: &Value| page.get("component").is_some())
            .unwrap_or_else(|| panic!("the response has no Inertia page ({status}): {body}"));
        let text = |key: &str| raw[key].as_str().unwrap_or_default().to_owned();
        Self {
            component: text("component"),
            url: text("url"),
            props: raw["props"].clone(),
            raw,
        }
    }

    /// The prop at a dot path. A number is an array index: `users.0.name`.
    pub fn prop(&self, path: &str) -> Option<&Value> {
        path.split('.')
            .try_fold(&self.props, |value, key| match key.parse::<usize>() {
                Ok(index) if value.is_array() => value.get(index),
                _ => value.get(key),
            })
    }
}

/// The content of the `<script data-page=...>` tag.
fn script_tag(html: &str) -> Option<&str> {
    let tag = html.find("<script data-page")?;
    let start = tag + html[tag..].find('>')? + 1;
    let end = start + html[start..].find("</script>")?;
    Some(&html[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Inertia, InertiaConfig, InertiaLayer};
    use axum::routing::get;
    use serde_json::json;
    use tower::ServiceExt;

    fn app() -> axum::Router {
        axum::Router::new()
            .route(
                "/users",
                get(|inertia: Inertia| async move {
                    inertia.render("Users/Index", json!({ "users": [{ "name": "</script>" }] }))
                }),
            )
            .layer(InertiaLayer::new(InertiaConfig::new()))
    }

    #[tokio::test]
    async fn reads_the_page_from_json_and_from_html() {
        let json_response = app().oneshot(visit("GET", "/users")).await.unwrap();
        let from_json = TestPage::from_response(json_response).await;

        let html_request = Request::get("/users").body(Body::empty()).unwrap();
        let html_response = app().oneshot(html_request).await.unwrap();
        let from_html = TestPage::from_response(html_response).await;

        assert_eq!(from_json, from_html);
        assert_eq!(from_json.component, "Users/Index");
        assert_eq!(from_json.url, "/users");
        assert_eq!(from_json.prop("users.0.name"), Some(&json!("</script>")));
        assert_eq!(from_json.prop("users.1"), None);
    }
}
