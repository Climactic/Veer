//! `IntoResponse` for `InertiaResponse` — runs the protocol decision and serializes.

use crate::adapters::axum::extractor::PerRequest;
use crate::headers as veer_headers;
use crate::page::PageObject;
use crate::props::resolver::{resolve, serialize_tag_aware, ResolveInput, SerializedBase};
use crate::protocol::{decide, DecisionInputs, ResponseShape};
use crate::request::RequestInfo;
use crate::response::InertiaResponse;
use crate::root_view::RootViewContext;
use axum::body::Body;
use axum::http::{HeaderValue, Response, StatusCode};
use axum::response::IntoResponse;
use serde_json::Value;
use std::sync::{Arc, Mutex};

impl IntoResponse for InertiaResponse {
    fn into_response(self) -> Response<Body> {
        // Inertia handle context is recovered via request extensions on the response path
        // by carrying it inside the response builder. For simplicity in v0.1, we require
        // the layer to wrap the response after the handler runs. This impl produces a
        // placeholder; the layer (Task 16) finishes the work using `finalize`.
        let marker = InertiaResponseMarker(Arc::new(Mutex::new(Some(self))));
        let mut resp = Response::new(Body::empty());
        resp.extensions_mut().insert(marker);
        resp
    }
}

/// Marker the layer inspects after a handler returns. Public to the crate only.
///
/// Wrapped in `Arc<Mutex<Option<...>>>` so that it satisfies the `Clone + Send + Sync`
/// bounds required by `http::Extensions::insert`, despite `InertiaResponse` containing
/// `FnOnce` closures (which are `Send` but not `Sync` or `Clone`).
#[derive(Clone)]
pub(crate) struct InertiaResponseMarker(pub Arc<Mutex<Option<InertiaResponse>>>);

/// Finish the response. Called by the layer with access to per-request state.
pub(crate) async fn finalize(
    mut builder: InertiaResponse,
    per: &PerRequest,
    req_info: &RequestInfo,
) -> Response<Body> {
    let cfg = per.config.clone();
    let version_owned = (cfg.version)().into_owned();

    let decision = decide(DecisionInputs {
        req: req_info,
        server_version: &version_owned,
        redirect: builder.redirect.clone(),
        csr_only: cfg.csr_only,
    });

    // Outgoing flash (write later in this function via finish_with_flash).
    let pending_flash = builder.pending_flash.clone();

    // Resolve shared + base props.
    let shared = match &cfg.shared {
        Some(s) => {
            let shared = s.shared(req_info).await;
            for (key, prop) in shared.once {
                if builder.base_props.get(&key).is_none()
                    && !builder.deferreds.contains_key(&key)
                    && !builder.lazies.contains_key(&key)
                {
                    builder.once.entry(key).or_insert(prop);
                }
            }
            for (key, lazy) in shared.lazies {
                if builder.base_props.get(&key).is_none()
                    && !builder.deferreds.contains_key(&key)
                    && !builder.once.contains_key(&key)
                {
                    builder.lazies.entry(key).or_insert(lazy);
                }
            }
            Some(shared.value)
        }
        None => None,
    };

    let base_serialized = serialize_tag_aware(&builder.base_props).unwrap_or_else(|e| {
        tracing::error!(error = %e, "veer: failed to serialize base props; using null");
        SerializedBase {
            value: Value::Null,
            always_paths: Default::default(),
            merge_paths: Default::default(),
        }
    });

    // Auto-inject `errors` and `flash` shared props (always present).
    let mut shared_value = shared.unwrap_or_else(|| Value::Object(Default::default()));
    if let Value::Object(map) = &mut shared_value {
        let errors_value = serde_json::to_value(&per.flash.errors).unwrap_or_else(|e| {
            tracing::error!(error = %e, "veer: failed to serialize flash errors");
            Value::Null
        });
        map.insert("errors".into(), errors_value);
        let bags_value = serde_json::to_value(&per.flash.bags).unwrap_or_else(|e| {
            tracing::error!(error = %e, "veer: failed to serialize flash bags");
            Value::Null
        });
        map.insert("flash".into(), bags_value);
    }

    let resolved = resolve(ResolveInput {
        req: req_info,
        component: &builder.component,
        base: base_serialized,
        lazies: builder.lazies,
        deferreds: builder.deferreds,
        once: builder.once,
        merges: builder.merges,
        shared: Some(SerializedBase {
            value: shared_value,
            always_paths: Default::default(),
            merge_paths: Default::default(),
        }),
    })
    .await;

    let mut page = PageObject::new(
        &builder.component,
        resolved.props,
        &req_info.url,
        &version_owned,
    );
    page.encrypt_history = builder.encrypt_history;
    page.clear_history = builder.clear_history;
    page.merge_props = resolved.merge_props;
    page.deferred_props = resolved.deferred_props;
    page.once_props = resolved.once_props;
    // Combine builder-attached reset keys with any the client signaled via
    // `X-Inertia-Reset`, deduped.
    let mut reset = builder.reset_merge_props.clone();
    for k in &req_info.reset {
        if !reset.contains(k) {
            reset.push(k.clone());
        }
    }
    reset.sort();
    page.reset_merge_props = reset;

    // Build response based on decision.
    let response = match decision {
        ResponseShape::Json => {
            let body = serde_json::to_vec(&page).unwrap_or_else(|e| {
                tracing::error!(error = %e, "veer: failed to serialize PageObject as JSON");
                Vec::new()
            });
            let mut r = Response::new(Body::from(body));
            *r.status_mut() = StatusCode::OK;
            r.headers_mut().insert(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            r.headers_mut()
                .insert(&veer_headers::X_INERTIA, HeaderValue::from_static("true"));
            r.headers_mut()
                .insert(&veer_headers::VARY, HeaderValue::from_static("X-Inertia"));
            r
        }
        ResponseShape::Html => {
            let page_json = serde_json::to_string(&page).unwrap_or_else(|e| {
                tracing::error!(error = %e, "veer: failed to serialize PageObject for HTML embed");
                String::new()
            });
            let escaped = html_attr_escape(&page_json);
            let script_escaped = script_tag_escape(&page_json);

            // Optional SSR.
            let ssr_payload = if !builder.skip_ssr {
                if let Some(client) = &cfg.ssr {
                    let page_value = serde_json::to_value(&page).unwrap_or_else(|e| {
                        tracing::error!(error = %e, "veer: failed to serialize PageObject for SSR");
                        Value::Null
                    });
                    match client.render(&page_value).await {
                        Ok(p) => Some(p),
                        Err(e) => {
                            if cfg.ssr_required {
                                let mut r = Response::new(Body::from(format!("ssr failed: {e}")));
                                *r.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                                return finish_with_flash(r, pending_flash, per).await;
                            }
                            tracing::warn!(error = ?e, "SSR failed; falling back to client render");
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let html = cfg
                .root_view
                .render(RootViewContext {
                    page_json: &escaped,
                    page_json_script: &script_escaped,
                    asset_version: &version_owned,
                    ssr: ssr_payload.as_ref(),
                })
                .unwrap_or_else(|e| format!("root view error: {e}"));
            let mut r = Response::new(Body::from(html));
            r.headers_mut().insert(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            );
            r
        }
        ResponseShape::SeeOther { location } => {
            let mut r = Response::new(Body::empty());
            *r.status_mut() = StatusCode::SEE_OTHER;
            r.headers_mut().insert(
                http::header::LOCATION,
                HeaderValue::from_str(&location).unwrap_or(HeaderValue::from_static("/")),
            );
            r
        }
        ResponseShape::InertiaLocation { location } => {
            let mut r = Response::new(Body::empty());
            *r.status_mut() = StatusCode::CONFLICT;
            r.headers_mut().insert(
                &veer_headers::X_INERTIA_LOCATION,
                HeaderValue::from_str(&location).unwrap_or(HeaderValue::from_static("/")),
            );
            r
        }
    };

    finish_with_flash(response, pending_flash, per).await
}

async fn finish_with_flash(
    mut response: Response<Body>,
    pending: crate::session::Flash,
    per: &PerRequest,
) -> Response<Body> {
    if let Some(session) = &per.config.session {
        session
            .write(response.headers_mut(), &per.req_extensions, pending)
            .await;
    }
    response
}

fn html_attr_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Make a JSON string safe to embed inside `<script>...</script>`. The HTML
/// spec's only forbidden subsequences in script content are `</`, `<!--`, and
/// `]]>` (the last only matters in XHTML, included for safety). Replacing the
/// `<` and `]` with their `\uXXXX` JSON escapes leaves the parsed JSON value
/// identical while preventing premature script termination or comment-state
/// confusion.
fn script_tag_escape(json: &str) -> String {
    json.replace("</", "<\\/")
        .replace("<!--", "\\u003c!--")
        .replace("]]>", "]]\\u003e")
}
