//! Per-request data parsed from Inertia headers.

use http::{HeaderMap, Method};
use std::collections::HashSet;

/// Request information needed to drive the Inertia protocol.
#[derive(Debug, Clone)]
#[non_exhaustive]
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
        let referer = text(&http::header::REFERER).map(str::to_owned);
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
        }
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
