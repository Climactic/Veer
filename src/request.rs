//! Per-request data parsed from Inertia headers.

use http::{Extensions, HeaderMap, Method};
use std::collections::HashSet;
use std::sync::Arc;

/// Request information needed to drive the Inertia protocol.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RequestInfo {
    /// HTTP method.
    pub method: Method,
    /// Full URL the client is currently at (path + query).
    pub url: String,
    /// The path and query of the `Referer` header. The scheme and the host are
    /// dropped, so that [`crate::inertia::Inertia::back()`] cannot redirect to
    /// another site.
    pub referer: Option<String>,
    /// `true` iff `X-Inertia: true` was set.
    pub is_inertia: bool,
    /// Client-reported asset version, if any.
    pub client_version: Option<String>,
    /// Component being partially reloaded, if any.
    pub partial_component: Option<String>,
    /// Allowlist of prop keys for a partial reload.
    pub partial_only: HashSet<String>,
    /// Denylist of prop keys for a partial reload.
    pub partial_except: HashSet<String>,
    /// Keys the client wants reset (clear merge state for these).
    pub reset: HashSet<String>,
    /// Error bag name from `X-Inertia-Error-Bag`, if any.
    pub error_bag: Option<String>,
    /// Once-prop keys the client already holds (`X-Inertia-Except-Once-Props`).
    pub except_once_props: HashSet<String>,
    /// `true` iff `X-Inertia-Infinite-Scroll-Merge-Intent: prepend` was set.
    pub scroll_prepend: bool,
    /// `true` iff `Purpose: prefetch` was set.
    pub is_prefetch: bool,
    /// `true` iff `Precognition: true` was set.
    pub is_precognition: bool,
    /// Fields from `Precognition-Validate-Only`. Empty means all fields.
    pub validate_only: HashSet<String>,
    /// The request extensions, as they were when the request reached the
    /// adapter. Read one with [`RequestInfo::extension`].
    pub extensions: Arc<Extensions>,
}

impl RequestInfo {
    /// Parse headers + method + url into a [`RequestInfo`].
    pub fn from_parts(method: Method, url: String, headers: &HeaderMap) -> Self {
        fn split_csv(headers: &HeaderMap, name: &http::HeaderName) -> HashSet<String> {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(|s| {
                    s.split(',')
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty())
                        .collect()
                })
                .unwrap_or_default()
        }
        let text = |name: &http::HeaderName| headers.get(name).and_then(|v| v.to_str().ok());
        let is_inertia = text(&crate::headers::X_INERTIA) == Some("true");
        let client_version = text(&crate::headers::X_INERTIA_VERSION).map(str::to_owned);
        let partial_component =
            text(&crate::headers::X_INERTIA_PARTIAL_COMPONENT).map(str::to_owned);
        let referer = text(&http::header::REFERER).and_then(local_path);
        Self {
            method,
            url,
            referer,
            is_inertia,
            client_version,
            partial_component,
            partial_only: split_csv(headers, &crate::headers::X_INERTIA_PARTIAL_DATA),
            partial_except: split_csv(headers, &crate::headers::X_INERTIA_PARTIAL_EXCEPT),
            reset: split_csv(headers, &crate::headers::X_INERTIA_RESET),
            error_bag: text(&crate::headers::X_INERTIA_ERROR_BAG)
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            except_once_props: split_csv(headers, &crate::headers::X_INERTIA_EXCEPT_ONCE_PROPS),
            scroll_prepend: text(&crate::headers::X_INERTIA_INFINITE_SCROLL_MERGE_INTENT)
                == Some("prepend"),
            is_prefetch: text(&crate::headers::PURPOSE) == Some("prefetch"),
            is_precognition: text(&crate::headers::PRECOGNITION) == Some("true"),
            validate_only: split_csv(headers, &crate::headers::PRECOGNITION_VALIDATE_ONLY),
            extensions: Arc::default(),
        }
    }

    /// The URL of a request: path and query. Leading slashes are collapsed,
    /// because a browser reads `//host/path` as a URL of another site.
    pub fn url_of(uri: &http::Uri) -> String {
        let url = uri.path_and_query().map_or(uri.path(), |p| p.as_str());
        format!("/{}", url.trim_start_matches(['/', '\\']))
    }

    /// Attach the request extensions; used by adapters.
    pub fn with_extensions(mut self, extensions: Arc<Extensions>) -> Self {
        self.extensions = extensions;
        self
    }

    /// A value that a middleware put in the request extensions (the signed-in
    /// user, a session handle, a locale). The middleware must run before the
    /// Inertia layer: add its layer after `InertiaLayer`.
    pub fn extension<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.extensions.get()
    }

    /// Returns `true` if the request is a partial reload (component header set + only/except non-empty).
    pub fn is_partial(&self) -> bool {
        self.partial_component.is_some()
            && (!self.partial_only.is_empty() || !self.partial_except.is_empty())
    }
}

/// The path and query of `referer`. Only the path is kept, so the result is
/// always a URL of this site. The host is not compared with the request: it
/// is not reliable behind a proxy or on HTTP/2, and the path alone cannot
/// point at another site.
fn local_path(referer: &str) -> Option<String> {
    let path = if referer.starts_with('/') {
        referer.to_owned()
    } else {
        let uri: http::Uri = referer.parse().ok()?;
        // An absolute URL has a scheme; anything else is not a Referer.
        uri.scheme()?;
        uri.path_and_query().map_or("/", |p| p.as_str()).to_owned()
    };
    // `//host` and `/\host` are URLs of another site for a browser.
    let is_local = path.starts_with('/') && !path[1..].starts_with(['/', '\\']);
    is_local.then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderValue;

    #[test]
    fn referer_keeps_only_a_local_path() {
        let referer = |value: &str| {
            let mut h = HeaderMap::new();
            h.insert("referer", hv(value));
            RequestInfo::from_parts(Method::POST, "/".into(), &h).referer
        };
        assert_eq!(referer("/form?a=1").as_deref(), Some("/form?a=1"));
        assert_eq!(
            referer("https://app.test:3000/form?a=1").as_deref(),
            Some("/form?a=1")
        );
        assert_eq!(referer("http://app.test").as_deref(), Some("/"));
        // The host of another site is dropped; the path stays on this site.
        assert_eq!(referer("https://evil.test/form").as_deref(), Some("/form"));
        for other in [
            "//evil.test/form",
            "/\\evil.test",
            "http://app.test//evil.test",
            "javascript:alert(1)",
            "not a url",
        ] {
            assert_eq!(referer(other), None, "{other}");
        }
    }

    #[test]
    fn url_of_collapses_leading_slashes() {
        let url = |s: &str| RequestInfo::url_of(&s.parse().unwrap());
        assert_eq!(url("/users?page=2"), "/users?page=2");
        assert_eq!(url("//evil.test/x"), "/evil.test/x");
        assert_eq!(url("/"), "/");
    }

    fn hv(s: &str) -> HeaderValue {
        HeaderValue::from_str(s).unwrap()
    }

    #[test]
    fn plain_request_is_not_inertia() {
        let info = RequestInfo::from_parts(Method::GET, "/".into(), &HeaderMap::new());
        assert!(!info.is_inertia);
        assert!(info.client_version.is_none());
        assert!(info.partial_only.is_empty());
        assert!(!info.is_partial());
        assert!(info.referer.is_none());
    }

    #[test]
    fn referer_parsed_from_header() {
        let mut h = HeaderMap::new();
        h.insert(http::header::REFERER, hv("https://example.com/previous"));
        let info = RequestInfo::from_parts(Method::POST, "/submit".into(), &h);
        assert_eq!(info.referer.as_deref(), Some("/previous"));
    }

    #[test]
    fn referer_absent_when_header_missing() {
        let info = RequestInfo::from_parts(Method::GET, "/page".into(), &HeaderMap::new());
        assert!(info.referer.is_none());
    }

    #[test]
    fn inertia_xhr_request_parsed() {
        let mut h = HeaderMap::new();
        h.insert(&crate::headers::X_INERTIA, hv("true"));
        h.insert(&crate::headers::X_INERTIA_VERSION, hv("abc123"));
        let info = RequestInfo::from_parts(Method::GET, "/users".into(), &h);
        assert!(info.is_inertia);
        assert_eq!(info.client_version.as_deref(), Some("abc123"));
    }

    #[test]
    fn v3_headers_parsed() {
        let mut h = HeaderMap::new();
        h.insert(&crate::headers::X_INERTIA_ERROR_BAG, hv("login"));
        h.insert(
            &crate::headers::X_INERTIA_EXCEPT_ONCE_PROPS,
            hv("plans,roles"),
        );
        h.insert(
            &crate::headers::X_INERTIA_INFINITE_SCROLL_MERGE_INTENT,
            hv("prepend"),
        );
        h.insert(&crate::headers::PURPOSE, hv("prefetch"));
        h.insert(&crate::headers::PRECOGNITION, hv("true"));
        h.insert(&crate::headers::PRECOGNITION_VALIDATE_ONLY, hv("email"));
        let info = RequestInfo::from_parts(Method::POST, "/".into(), &h);
        assert_eq!(info.error_bag.as_deref(), Some("login"));
        assert!(info.except_once_props.contains("roles"));
        assert!(info.scroll_prepend);
        assert!(info.is_prefetch);
        assert!(info.is_precognition);
        assert!(info.validate_only.contains("email"));
    }

    #[test]
    fn partial_reload_parses_only_and_except() {
        let mut h = HeaderMap::new();
        h.insert(&crate::headers::X_INERTIA, hv("true"));
        h.insert(
            &crate::headers::X_INERTIA_PARTIAL_COMPONENT,
            hv("Users/Index"),
        );
        h.insert(&crate::headers::X_INERTIA_PARTIAL_DATA, hv("users, stats"));
        h.insert(&crate::headers::X_INERTIA_PARTIAL_EXCEPT, hv("auth"));
        let info = RequestInfo::from_parts(Method::GET, "/users".into(), &h);
        assert_eq!(info.partial_component.as_deref(), Some("Users/Index"));
        assert!(info.partial_only.contains("users"));
        assert!(info.partial_only.contains("stats"));
        assert!(info.partial_except.contains("auth"));
        assert!(info.is_partial());
    }
}
