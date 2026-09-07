//! Per-request data parsed from Inertia headers.

use http::{Extensions, HeaderMap, Method};
use std::collections::HashSet;

/// Request information needed to drive the Inertia protocol.
#[derive(Debug, Clone)]
pub struct RequestInfo {
    /// HTTP method.
    pub method: Method,
    /// Full URL the client is currently at (path + query).
    pub url: String,
    /// Value of the `Referer` header, if present.
    ///
    /// Used by [`crate::inertia::Inertia::back()`] to redirect the client to the
    /// previous page. Falls back to `"/"` when absent.
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
    /// True when InfiniteScroll is loading an earlier page.
    pub prepend_scroll: bool,
    /// Once-prop cache keys already available in the client.
    pub except_once_props: HashSet<String>,
    extensions: Extensions,
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
        let is_inertia = headers
            .get(&crate::headers::X_INERTIA)
            .and_then(|v| v.to_str().ok())
            == Some("true");
        let client_version = headers
            .get(&crate::headers::X_INERTIA_VERSION)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let partial_component = headers
            .get(&crate::headers::X_INERTIA_PARTIAL_COMPONENT)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let referer = headers
            .get(http::header::REFERER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
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
            prepend_scroll: headers
                .get(&crate::headers::X_INERTIA_INFINITE_SCROLL_MERGE_INTENT)
                .and_then(|value| value.to_str().ok())
                == Some("prepend"),
            except_once_props: split_csv(headers, &crate::headers::X_INERTIA_EXCEPT_ONCE_PROPS),
            extensions: Extensions::new(),
        }
    }

    /// Attach extensions installed by outer request middleware.
    ///
    /// Adapters use this to make request-scoped values such as sessions
    /// available to shared prop resolvers.
    pub fn with_extensions(mut self, extensions: Extensions) -> Self {
        self.extensions = extensions;
        self
    }

    /// Return the request extensions installed by outer middleware.
    pub fn extensions(&self) -> &Extensions {
        &self.extensions
    }

    /// Return a request extension by type.
    pub fn extension<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.extensions.get::<T>()
    }

    /// Returns `true` if the request is a partial reload (component header set + only/except non-empty).
    pub fn is_partial(&self) -> bool {
        self.partial_component.is_some()
            && (!self.partial_only.is_empty() || !self.partial_except.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderValue;

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
        assert_eq!(
            info.referer.as_deref(),
            Some("https://example.com/previous")
        );
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

    #[test]
    fn request_extensions_are_available() {
        let mut extensions = Extensions::new();
        extensions.insert(String::from("session-user"));

        let info = RequestInfo::from_parts(Method::GET, "/".into(), &HeaderMap::new())
            .with_extensions(extensions);

        assert_eq!(
            info.extension::<String>().map(String::as_str),
            Some("session-user")
        );
    }
}
