//! Tower layer that wires `InertiaConfig` + flash into request extensions, then
//! finalizes any `InertiaResponseMarker` produced by handlers.

use super::extractor::PerRequest;
#[cfg(feature = "devtools")]
use super::response::RecordedPage;
use super::response::{
    finalize, finish_with_flash, redirect, version_mismatch, InertiaResponseMarker,
};
use crate::config::InertiaConfig;
#[cfg(feature = "devtools")]
use crate::devtools::{self, Finished};
use crate::headers as veer_headers;
use crate::protocol::is_version_mismatch;
use crate::request::RequestInfo;
use crate::session::Flash;
use axum::body::Body;
use axum::http::{HeaderValue, Method, Request, Response, StatusCode};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::{Layer, Service};

/// Tower layer that enables Inertia handling on a router.
#[derive(Clone)]
pub struct InertiaLayer {
    config: Arc<InertiaConfig>,
}

impl InertiaLayer {
    /// Wrap a router in an Inertia layer.
    pub fn new(config: InertiaConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }
}

impl<S> Layer<S> for InertiaLayer {
    type Service = InertiaMiddleware<S>;
    fn layer(&self, inner: S) -> Self::Service {
        InertiaMiddleware {
            inner,
            config: self.config.clone(),
        }
    }
}

#[doc(hidden)]
#[derive(Clone)]
pub struct InertiaMiddleware<S> {
    inner: S,
    config: Arc<InertiaConfig>,
}

impl<S> Service<Request<Body>> for InertiaMiddleware<S>
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
        // Tower contract: drive the instance that poll_ready readied, leaving a
        // fresh clone behind for the next call.
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        Box::pin(async move {
            #[cfg_attr(not(feature = "devtools"), allow(unused_mut))]
            let (mut parts, mut body) = req.into_parts();

            // DevTools: answer the read API, or start a recording.
            #[cfg(feature = "devtools")]
            let mut recording = None;
            #[cfg(feature = "devtools")]
            let mut request_body = devtools::body_empty();
            #[cfg(feature = "devtools")]
            if let Some(devtools) = &cfg.devtools {
                if let Some((status, json)) = devtools.read_api(&parts) {
                    let mut resp = Response::new(Body::from(json));
                    *resp.status_mut() = status;
                    resp.headers_mut().insert(
                        http::header::CONTENT_TYPE,
                        HeaderValue::from_static("application/json"),
                    );
                    return Ok(resp);
                }
                recording = Some(devtools.start(&parts.headers));
                (body, request_body) = capture_request_body(&parts, body).await;
            }

            // Read flash from session (if configured).
            let flash = if let Some(s) = &cfg.session {
                s.read_and_clear(&parts).await
            } else {
                Flash::default()
            };
            // Capture data we'll need to rebuild RequestInfo for finalize.
            let method_for_post = parts.method.clone();
            let url_for_finalize = RequestInfo::url_of(&parts.uri);
            let headers_clone = parts.headers.clone();
            // Snapshot extensions so session stores that piggyback on
            // middleware-installed handles (e.g. tower-sessions::Session) can
            // still reach them on the response side.
            let extensions_snapshot = Arc::new(parts.extensions.clone());

            let previous_url = match &cfg.session {
                Some(s) if cfg.store_previous_url => s.previous_url(&parts).await,
                _ => None,
            };

            let per_request = PerRequest {
                previous_url,
                config: cfg.clone(),
                flash: Arc::new(flash),
                req_extensions: extensions_snapshot.clone(),
                #[cfg(feature = "devtools")]
                devtools_id: recording.as_ref().map(|r| r.id.clone()),
                #[cfg(not(feature = "devtools"))]
                devtools_id: None,
            };
            parts.extensions.insert(per_request.clone());

            let request = Request::from_parts(parts, body);
            let mut resp = inner.call(request).await?;

            let req_info =
                RequestInfo::from_parts(method_for_post.clone(), url_for_finalize, &headers_clone)
                    .with_extensions(extensions_snapshot.clone());

            // If handler returned an Inertia marker, finalize.
            if let Some(marker) = resp.extensions_mut().remove::<InertiaResponseMarker>() {
                // Marker wraps Arc<Mutex<Option<InertiaResponse>>>. Take the inner value.
                let inner_response = marker
                    .0
                    .lock()
                    .expect("InertiaResponse mutex poisoned")
                    .take();
                if let Some(ir) = inner_response {
                    resp = finalize(ir, &per_request, &req_info).await;
                }
            } else {
                resp = plain_response(resp, &per_request, &req_info).await;
            }

            // HTML and JSON share a URL, so caches must key on the header.
            if !resp
                .headers()
                .get_all(&veer_headers::VARY)
                .iter()
                .any(|v| v.as_bytes().eq_ignore_ascii_case(b"x-inertia"))
            {
                resp.headers_mut()
                    .append(&veer_headers::VARY, HeaderValue::from_static("X-Inertia"));
            }

            #[cfg(feature = "devtools")]
            if let Some(recording) = recording {
                let page = resp.extensions_mut().remove::<RecordedPage>();
                for (name, value) in [
                    (devtools::X_INERTIA_DEVTOOLS_ID, recording.id.as_str()),
                    (
                        devtools::X_INERTIA_DEVTOOLS_PARENT_OUT,
                        recording.parent_out(&req_info),
                    ),
                ] {
                    if let Ok(value) = HeaderValue::from_str(value) {
                        resp.headers_mut().insert(name, value);
                    }
                }
                recording.finish(Finished {
                    req: &req_info,
                    request_headers: &headers_clone,
                    request_body,
                    status: resp.status(),
                    response_headers: resp.headers(),
                    page: page.as_ref().map(|p| &*p.page),
                    render_source: page.as_ref().and_then(|p| p.source),
                    response_is_empty: http_body::Body::size_hint(resp.body()).exact() == Some(0),
                    route: extensions_snapshot
                        .get::<axum::extract::MatchedPath>()
                        .map(|p| p.as_str()),
                });
            }

            Ok(resp)
        })
    }
}

#[cfg(feature = "devtools")]
/// Capture the request body for a DevTools entry. Only a JSON body of a known,
/// small size is read; it is then handed on unchanged.
async fn capture_request_body(
    parts: &http::request::Parts,
    body: Body,
) -> (Body, serde_json::Value) {
    let header = |name| parts.headers.get(name).and_then(|v| v.to_str().ok());
    let length = header(http::header::CONTENT_LENGTH).and_then(|v| v.parse::<usize>().ok());
    let is_json = header(http::header::CONTENT_TYPE).is_some_and(|t| t.contains("json"));
    let is_inertia = header(veer_headers::X_INERTIA) == Some("true");
    let capture = match length {
        None | Some(0) if matches!(parts.method, Method::GET | Method::HEAD) => {
            devtools::body_empty()
        }
        Some(0) => devtools::body_empty(),
        _ if !is_inertia => devtools::body_omitted("non-inertia-request"),
        None => devtools::body_omitted("streamed"),
        Some(n) if n > devtools::BODY_LIMIT => devtools::body_omitted("too-large"),
        Some(_) if !is_json => devtools::body_omitted("non-textual"),
        Some(n) => {
            return match axum::body::to_bytes(body, n).await {
                Ok(bytes) => {
                    let capture = match serde_json::from_slice(&bytes) {
                        Ok(value) => devtools::body_present(value),
                        Err(_) => devtools::body_omitted("unserializable"),
                    };
                    (Body::from(bytes), capture)
                }
                Err(_) => (Body::empty(), devtools::body_omitted("streamed")),
            };
        }
    };
    (body, capture)
}

/// Apply the protocol rules to a response that did not come from
/// `Inertia::render` / `Inertia::redirect` (for example `axum::response::Redirect`).
async fn plain_response(
    mut resp: Response<Body>,
    per: &PerRequest,
    req: &RequestInfo,
) -> Response<Body> {
    let version = per.config.current_version();
    let is_redirect = resp.status().is_redirection();
    let fragment_target = resp
        .headers()
        .get(http::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .filter(|l| is_redirect && l.contains('#'))
        .map(str::to_owned);

    if is_version_mismatch(req, &version) {
        resp = version_mismatch(&req.url, &version);
    } else if let (true, false, Some(location)) = (req.is_inertia, req.is_prefetch, fragment_target)
    {
        resp = redirect(
            StatusCode::CONFLICT,
            veer_headers::X_INERTIA_REDIRECT,
            &location,
        );
    } else if resp.status() == StatusCode::FOUND
        && matches!(
            req.method,
            Method::POST | Method::PUT | Method::PATCH | Method::DELETE
        )
    {
        // A plain 302 from a non-GET becomes a 303 per Inertia spec.
        *resp.status_mut() = StatusCode::SEE_OTHER;
    } else if req.is_inertia
        && resp.status() == StatusCode::OK
        && http_body::Body::size_hint(resp.body()).exact() == Some(0)
    {
        // An empty 200 is not a page. Go back, as the Laravel adapter does.
        resp = redirect(
            StatusCode::SEE_OTHER,
            http::header::LOCATION,
            req.referer
                .as_deref()
                .or(per.previous_url.as_deref())
                .unwrap_or("/"),
        );
    }

    // Only a page render uses the flash data. From any other response it goes
    // on to the request that follows.
    if per.flash.is_empty() {
        return resp;
    }
    finish_with_flash(resp, (*per.flash).clone(), per).await
}
