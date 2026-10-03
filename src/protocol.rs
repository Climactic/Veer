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
    /// 302 redirect (external redirect on a non-Inertia request).
    Found {
        /// Redirect destination URL.
        location: String,
    },
    /// 409 with `X-Inertia-Location` (external redirect).
    InertiaLocation {
        /// URL sent back in the `X-Inertia-Location` header.
        location: String,
    },
    /// 409 with `X-Inertia-Location` and `X-Inertia-Version` (asset version mismatch).
    VersionMismatch {
        /// URL sent back in the `X-Inertia-Location` header.
        location: String,
    },
    /// 409 with `X-Inertia-Redirect` (redirect whose target has a URL fragment).
    InertiaRedirect {
        /// URL sent back in the `X-Inertia-Redirect` header.
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

/// `true` if an Inertia `GET` carries an asset version that is not the server's.
pub fn is_version_mismatch(req: &RequestInfo, server_version: &str) -> bool {
    req.is_inertia
        && req.method == Method::GET
        && req.client_version.as_deref().unwrap_or("") != server_version
}

/// Pure decision function. No I/O. No serialization. Just rules.
pub fn decide(input: DecisionInputs<'_>) -> ResponseShape {
    if let Some(r) = input.redirect {
        return match r {
            // Inertia headers do not survive a hop to another origin, so an XHR
            // visit gets a 409; a plain browser request follows a normal redirect.
            Redirect::External(location) if input.req.is_inertia => {
                ResponseShape::InertiaLocation { location }
            }
            Redirect::External(location) => ResponseShape::Found { location },
            // XHR drops the fragment of a followed redirect, so the client must
            // make the visit itself.
            Redirect::Internal(location)
                if input.req.is_inertia && !input.req.is_prefetch && location.contains('#') =>
            {
                ResponseShape::InertiaRedirect { location }
            }
            // 303 makes the browser follow with a GET after any method.
            Redirect::Internal(location) => ResponseShape::SeeOther { location },
        };
    }

    if input.req.is_inertia {
        // XHR: version mismatch on a GET → 409 reload at same URL.
        if is_version_mismatch(input.req, input.server_version) {
            return ResponseShape::VersionMismatch {
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
            ResponseShape::VersionMismatch {
                location: "/users".into()
            }
        );
    }

    #[test]
    fn external_redirect_on_plain_request_is_a_302() {
        let r = req(Method::GET, "/oauth", false, None);
        let d = decide(DecisionInputs {
            req: &r,
            server_version: "v1",
            redirect: Some(Redirect::External("https://example.com/".into())),
            csr_only: false,
        });
        assert_eq!(
            d,
            ResponseShape::Found {
                location: "https://example.com/".into()
            }
        );
    }

    #[test]
    fn fragment_redirect_is_an_inertia_redirect_unless_prefetch_or_plain() {
        let decide_for = |r: &RequestInfo| {
            decide(DecisionInputs {
                req: r,
                server_version: "v1",
                redirect: Some(Redirect::Internal("/docs#install".into())),
                csr_only: false,
            })
        };
        let see_other = ResponseShape::SeeOther {
            location: "/docs#install".into(),
        };
        let mut r = req(Method::POST, "/docs", true, Some("v1"));
        assert_eq!(
            decide_for(&r),
            ResponseShape::InertiaRedirect {
                location: "/docs#install".into()
            }
        );
        r.is_prefetch = true;
        assert_eq!(decide_for(&r), see_other);
        assert_eq!(
            decide_for(&req(Method::POST, "/docs", false, None)),
            see_other
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
