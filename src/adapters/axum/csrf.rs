//! Standalone CSRF tower layer — stateless signed double-submit, axios/Inertia
//! compatible. Compose next to [`InertiaLayer`](super::InertiaLayer):
//!
//! ```ignore
//! .layer(InertiaLayer::new(cfg)).layer(CsrfLayer::new(secret))
//! ```
//!
//! On mutating methods it verifies the `X-XSRF-TOKEN` header against the
//! `XSRF-TOKEN` cookie (returning 419 on mismatch); on every response lacking a
//! valid token cookie it issues a fresh one (JS-readable, so axios can echo it).

use crate::csrf::CsrfTokens;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, Response, StatusCode};
use cookie::Cookie;
use http::header;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::{Layer, Service};

#[derive(Clone)]
struct CsrfConfig {
    tokens: CsrfTokens,
    secure: bool,
    same_site: cookie::SameSite,
    cookie_name: String,
    header_name: String,
    excludes: Vec<String>,
}

/// Tower layer adding double-submit CSRF protection.
#[derive(Clone)]
pub struct CsrfLayer {
    config: CsrfConfig,
}

impl CsrfLayer {
    /// New layer. `secret` must be >= 32 bytes. Defaults: cookie `XSRF-TOKEN`,
    /// header `x-xsrf-token`, `Secure` on, `SameSite=Lax`, no exclusions.
    pub fn new(secret: impl Into<Vec<u8>>) -> Self {
        Self {
            config: CsrfConfig {
                tokens: CsrfTokens::new(secret),
                secure: true,
                same_site: cookie::SameSite::Lax,
                cookie_name: "XSRF-TOKEN".into(),
                header_name: "x-xsrf-token".into(),
                excludes: Vec::new(),
            },
        }
    }

    /// Toggle the cookie `Secure` flag (default `true`). Disable for local HTTP.
    pub fn secure(mut self, yes: bool) -> Self {
        self.config.secure = yes;
        self
    }

    /// Set the cookie `SameSite` attribute (default `Lax`).
    pub fn same_site(mut self, s: cookie::SameSite) -> Self {
        self.config.same_site = s;
        self
    }

    /// Override the cookie name (default `XSRF-TOKEN`).
    pub fn cookie_name(mut self, name: impl Into<String>) -> Self {
        self.config.cookie_name = name.into();
        self
    }

    /// Override the verified header name (default `x-xsrf-token`).
    pub fn header_name(mut self, name: impl Into<String>) -> Self {
        self.config.header_name = name.into();
        self
    }

    /// Skip verification for a path prefix (matches the path exactly or as a
    /// `/`-bounded prefix). Repeatable. Use for webhook endpoints.
    pub fn exclude(mut self, path: impl Into<String>) -> Self {
        self.config.excludes.push(path.into());
        self
    }
}

impl<S> Layer<S> for CsrfLayer {
    type Service = CsrfMiddleware<S>;
    fn layer(&self, inner: S) -> Self::Service {
        CsrfMiddleware {
            inner,
            config: Arc::new(self.config.clone()),
        }
    }
}

#[doc(hidden)]
#[derive(Clone)]
pub struct CsrfMiddleware<S> {
    inner: S,
    config: Arc<CsrfConfig>,
}

impl<S> Service<Request<Body>> for CsrfMiddleware<S>
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
            let cookie_val = read_cookie(req.headers(), &cfg.cookie_name);
            let cookie_is_valid = cookie_val
                .as_deref()
                .map(|c| cfg.tokens.is_valid(c))
                .unwrap_or(false);

            let is_mutating = matches!(
                *req.method(),
                Method::POST | Method::PUT | Method::PATCH | Method::DELETE
            );
            let excluded = path_excluded(req.uri().path(), &cfg.excludes);

            if is_mutating && !excluded {
                let header_val = req
                    .headers()
                    .get(cfg.header_name.as_str())
                    .and_then(|v| v.to_str().ok());
                let ok = match (cookie_val.as_deref(), header_val) {
                    (Some(c), Some(h)) => cfg.tokens.verify(c, h),
                    _ => false,
                };
                if !ok {
                    let mut resp = Response::new(Body::from("CSRF token mismatch"));
                    *resp.status_mut() = StatusCode::from_u16(419).unwrap();
                    // Seed a token only if the client doesn't already hold a
                    // valid one — don't rotate a good cookie out from under a
                    // request whose header was merely missing/stale.
                    if !cookie_is_valid {
                        set_token_cookie(resp.headers_mut(), &cfg);
                    }
                    return Ok(resp);
                }
            }

            let mut resp = inner.call(req).await?;
            if !cookie_is_valid {
                set_token_cookie(resp.headers_mut(), &cfg);
            }
            Ok(resp)
        })
    }
}

fn path_excluded(path: &str, excludes: &[String]) -> bool {
    excludes.iter().any(|p| {
        let p = p.trim_end_matches('/');
        path == p || path.starts_with(&format!("{p}/"))
    })
}

fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|hv| hv.to_str().ok())
        .flat_map(|s| s.split(';'))
        .filter_map(|s| Cookie::parse(s.trim().to_owned()).ok())
        .find(|c| c.name() == name)
        .map(|c| c.value().to_string())
}

fn set_token_cookie(headers: &mut HeaderMap, cfg: &CsrfConfig) {
    let mut c = Cookie::new(cfg.cookie_name.clone(), cfg.tokens.generate());
    c.set_path("/");
    c.set_secure(cfg.secure);
    c.set_same_site(cfg.same_site);
    // Deliberately NOT http_only: axios reads the value from document.cookie.
    if let Ok(hv) = http::HeaderValue::from_str(&c.to_string()) {
        headers.append(header::SET_COOKIE, hv);
    }
}
