//! Protocol state machine: decide the response shape for a given request + page.

use crate::request::RequestInfo;
use http::Method;

/// What kind of response should be produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseShape {
    /// Render the root view; embed the page object in the HTML.
    Html,
    /// Return the page object as JSON with `X-Inertia: true`.
    Json,
    /// 303 redirect (internal navigation following a POST/PUT/PATCH/DELETE).
    SeeOther {
        /// Redirect destination URL.
        location: String,
    },
    /// 409 with `X-Inertia-Location` (asset version mismatch or external redirect).
    InertiaLocation {
        /// URL sent back in the `X-Inertia-Location` header.
        location: String,
    },
}

/// Decision inputs.
#[derive(Debug, Clone)]
pub struct DecisionInputs<'a> {
    /// Parsed request info for this decision.
    pub req: &'a RequestInfo,
    /// Current asset version (server-side).
    pub server_version: &'a str,
    /// User-explicit redirect target, if any.
    pub redirect: Option<Redirect>,
    /// When `true`, plain GETs return JSON instead of HTML.
    pub csr_only: bool,
}

/// User-issued redirect.
#[derive(Debug, Clone)]
pub enum Redirect {
    /// Same-app redirect; renders as 303.
    Internal(String),
    /// Off-app redirect; renders as 409 + X-Inertia-Location.
    External(String),
}

/// Pure decision function. No I/O. No serialization. Just rules.
pub fn decide(input: DecisionInputs<'_>) -> ResponseShape {
    if let Some(r) = input.redirect {
        return match r {
            Redirect::External(loc) => ResponseShape::InertiaLocation { location: loc },
            Redirect::Internal(loc) => {
                if matches!(
                    input.req.method,
                    Method::POST | Method::PUT | Method::PATCH | Method::DELETE
                ) {
                    ResponseShape::SeeOther { location: loc }
                } else {
                    // GET redirect (e.g. signed-route auth flow). Use 302 semantics via SeeOther.
                    ResponseShape::SeeOther { location: loc }
                }
            }
        };
    }

    if input.req.is_inertia {
        // XHR: version mismatch on a GET → 409 reload at same URL.
        if input.req.method == Method::GET
            && input.req.client_version.as_deref() != Some(input.server_version)
        {
            return ResponseShape::InertiaLocation {
                location: input.req.url.clone(),
            };
        }
        return ResponseShape::Json;
    }

    // Non-XHR
    if input.csr_only {
        ResponseShape::Json
    } else {
        ResponseShape::Html
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderMap;

    fn req(method: Method, url: &str, is_inertia: bool, version: Option<&str>) -> RequestInfo {
        let mut info = RequestInfo::from_parts(method, url.to_string(), &HeaderMap::new());
        info.is_inertia = is_inertia;
        info.client_version = version.map(str::to_owned);
        info
    }

    #[test]
    fn plain_get_returns_html() {
        let r = req(Method::GET, "/", false, None);
        let d = decide(DecisionInputs {
            req: &r,
            server_version: "v1",
            redirect: None,
            csr_only: false,
        });
        assert_eq!(d, ResponseShape::Html);
    }

    #[test]
    fn csr_only_returns_json_for_plain_get() {
        let r = req(Method::GET, "/", false, None);
        let d = decide(DecisionInputs {
            req: &r,
            server_version: "v1",
            redirect: None,
            csr_only: true,
        });
        assert_eq!(d, ResponseShape::Json);
    }

    #[test]
    fn xhr_with_matching_version_returns_json() {
        let r = req(Method::GET, "/", true, Some("v1"));
        let d = decide(DecisionInputs {
            req: &r,
            server_version: "v1",
            redirect: None,
            csr_only: false,
        });
        assert_eq!(d, ResponseShape::Json);
    }

    #[test]
    fn xhr_get_with_stale_version_returns_409_at_same_url() {
        let r = req(Method::GET, "/users", true, Some("old"));
        let d = decide(DecisionInputs {
            req: &r,
            server_version: "new",
            redirect: None,
            csr_only: false,
        });
        assert_eq!(
            d,
            ResponseShape::InertiaLocation {
                location: "/users".into()
            }
        );
    }

    #[test]
    fn xhr_post_with_stale_version_still_returns_json() {
        // POSTs are not subject to the version check — only GETs would force a reload.
        let r = req(Method::POST, "/users", true, Some("old"));
        let d = decide(DecisionInputs {
            req: &r,
            server_version: "new",
            redirect: None,
            csr_only: false,
        });
        assert_eq!(d, ResponseShape::Json);
    }

    #[test]
    fn internal_redirect_from_post_returns_303() {
        let r = req(Method::POST, "/users", true, Some("v1"));
        let d = decide(DecisionInputs {
            req: &r,
            server_version: "v1",
            redirect: Some(Redirect::Internal("/users/42".into())),
            csr_only: false,
        });
        assert_eq!(
            d,
            ResponseShape::SeeOther {
                location: "/users/42".into()
            }
        );
    }

    #[test]
    fn external_redirect_returns_inertia_location() {
        let r = req(Method::GET, "/oauth", true, Some("v1"));
        let d = decide(DecisionInputs {
            req: &r,
            server_version: "v1",
            redirect: Some(Redirect::External("https://example.com/oauth".into())),
            csr_only: false,
        });
        assert_eq!(
            d,
            ResponseShape::InertiaLocation {
                location: "https://example.com/oauth".into()
            }
        );
    }
}
