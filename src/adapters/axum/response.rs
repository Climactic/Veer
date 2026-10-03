//! `IntoResponse` for `InertiaResponse` — runs the protocol decision and serializes.

use crate::adapters::axum::extractor::PerRequest;
use crate::bigint::encode_big_integers;
use crate::headers as veer_headers;
use crate::page::PageObject;
use crate::props::resolver::{resolve, serialize_tag_aware, ResolveInput};
use crate::props::Prop;
use crate::protocol::{decide, DecisionInputs, ResponseShape};
use crate::request::RequestInfo;
use crate::response::InertiaResponse;
use crate::root_view::RootViewContext;
use axum::body::Body;
use axum::http::{HeaderName, HeaderValue, Response, StatusCode};
use axum::response::IntoResponse;
use serde_json::{json, Map, Value};
use std::sync::{Arc, Mutex};

impl IntoResponse for InertiaResponse {
    fn into_response(self) -> Response<Body> {
        // Inertia handle context is recovered via request extensions on the response path
        // by carrying it inside the response builder. The layer wraps the response
        // after the handler runs: this impl produces a placeholder, and the layer
        // finishes the work using `finalize`.
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
    let cfg = &per.config;
    let incoming = &*per.flash;
    let version = (cfg.version)().into_owned();

    let decision = decide(DecisionInputs {
        req: req_info,
        server_version: &version,
        redirect: builder.redirect.take(),
        csr_only: cfg.csr_only,
    });

    let mut pending = std::mem::take(&mut builder.pending_flash);

    // Control responses carry no page. The flash data that this request read
    // goes on to the request that follows.
    let control = match &decision {
        ResponseShape::SeeOther { location } => Some(redirect(
            StatusCode::SEE_OTHER,
            http::header::LOCATION,
            location,
        )),
        ResponseShape::Found { location } => Some(redirect(
            StatusCode::FOUND,
            http::header::LOCATION,
            location,
        )),
        ResponseShape::InertiaLocation { location } => Some(redirect(
            StatusCode::CONFLICT,
            veer_headers::X_INERTIA_LOCATION,
            location,
        )),
        ResponseShape::InertiaRedirect { location } => Some(redirect(
            StatusCode::CONFLICT,
            veer_headers::X_INERTIA_REDIRECT,
            location,
        )),
        ResponseShape::VersionMismatch { location } => {
            pending.errors = incoming.errors.clone();
            Some(version_mismatch(location, &version))
        }
        ResponseShape::Html | ResponseShape::Json => None,
    };
    if let Some(response) = control {
        for (k, v) in &incoming.bags {
            pending.bags.entry(k.clone()).or_insert_with(|| v.clone());
        }
        pending.clear_history |= builder.clear_history || incoming.clear_history;
        pending.preserve_fragment |= builder.preserve_fragment || incoming.preserve_fragment;
        return finish_with_flash(response, pending, per).await;
    }

    // Shared props, plus the always-present `errors`.
    let mut shared = match &cfg.shared {
        Some(s) => serialize_tag_aware(&s.shared(req_info).await).unwrap_or_default(),
        None => Default::default(),
    };
    if !shared.value.is_object() {
        shared.value = Value::Object(Map::new());
    }
    // Errors from the previous request, then errors set on this render.
    let mut all_errors = incoming.errors.clone();
    all_errors.extend(std::mem::take(&mut pending.errors));
    let mut errors: Value = all_errors
        .iter()
        .map(|(field, messages)| {
            let value = if cfg.with_all_errors {
                json!(messages)
            } else {
                json!(messages.first())
            };
            (field.clone(), value)
        })
        .collect::<Map<_, _>>()
        .into();
    if let (Some(bag), false) = (&req_info.error_bag, all_errors.is_empty()) {
        errors = Value::Object(Map::from_iter([(bag.clone(), errors)]));
    }
    let mut shared_keys = Vec::new();
    if let Value::Object(map) = &mut shared.value {
        map.insert("errors".into(), errors);
        shared_keys.extend(map.keys().cloned());
    }

    let base = serialize_tag_aware(&builder.base_props).unwrap_or_else(|e| {
        tracing::error!(error = %e, "veer: failed to serialize base props; using null");
        Default::default()
    });

    // Shared once props. A handler prop with the same key wins.
    for (key, f) in &cfg.shared_once {
        shared_keys.push(key.clone());
        if !builder.props.contains_key(key) && base.value.get(key).is_none() {
            let value = f(req_info);
            builder
                .props
                .insert(key.clone(), Prop::new(move || value).once());
        }
    }
    shared_keys.sort();
    shared_keys.dedup();

    let resolved = resolve(ResolveInput {
        req: req_info,
        component: &builder.component,
        base,
        shared: Some(shared),
        props: builder.props,
        merge: builder.merge,
    })
    .await;
    let resolved = match resolved {
        Ok(resolved) => resolved,
        Err(error) => {
            tracing::error!(%error, "veer: prop failed to resolve");
            let mut r = Response::new(Body::from("Internal Server Error"));
            *r.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            return finish_with_flash(r, pending, per).await;
        }
    };

    let mut page = PageObject::new(&builder.component, resolved.props, &req_info.url, &version);
    page.encrypt_history = builder.encrypt_history || cfg.encrypt_history;
    page.clear_history = builder.clear_history || incoming.clear_history;
    page.preserve_fragment = builder.preserve_fragment || incoming.preserve_fragment;
    page.shared_props = shared_keys;
    page.merge_props = resolved.merge_props;
    page.prepend_props = resolved.prepend_props;
    page.deep_merge_props = resolved.deep_merge_props;
    page.match_props_on = resolved.match_props_on;
    page.deferred_props = resolved.deferred_props;
    page.rescued_props = resolved.rescued_props;
    page.scroll_props = resolved.scroll_props;
    page.once_props = resolved.once_props;
    // Flash data from the previous request, then data flashed by this handler.
    page.flash = incoming.bags.clone().into_iter().collect();
    page.flash.extend(std::mem::take(&mut pending.bags));
    if builder
        .preserve_big_integers
        .unwrap_or(cfg.preserve_big_integers)
    {
        page.preserve_big_integers = true;
        encode_big_integers(&mut page.props);
        page.flash.values_mut().for_each(encode_big_integers);
    }

    // The DevTools recorder reads the page from the response extensions.
    let recorded = per
        .devtools_id
        .as_ref()
        .map(|_| RecordedPage(Arc::new(serde_json::to_value(&page).unwrap_or_default())));

    let mut response = if decision == ResponseShape::Json {
        let body = serde_json::to_vec(&page).unwrap_or_else(|e| {
            tracing::error!(error = %e, "veer: failed to serialize PageObject as JSON");
            Vec::new()
        });
        let mut r = Response::new(Body::from(body));
        r.headers_mut().insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        r.headers_mut()
            .insert(&veer_headers::X_INERTIA, HeaderValue::from_static("true"));
        r
    } else {
        let page_json = serde_json::to_string(&page).unwrap_or_else(|e| {
            tracing::error!(error = %e, "veer: failed to serialize PageObject for HTML embed");
            String::new()
        });
        let escaped = html_attr_escape(&page_json);
        let script_escaped = script_tag_escape(&page_json);

        // Optional SSR.
        let mut ssr_payload = None;
        if let (false, Some(client)) = (builder.skip_ssr, &cfg.ssr) {
            let page_value = serde_json::to_value(&page).unwrap_or_else(|e| {
                tracing::error!(error = %e, "veer: failed to serialize PageObject for SSR");
                Value::Null
            });
            match client.render(&page_value).await {
                Ok(p) => ssr_payload = Some(p),
                Err(e) if cfg.ssr_required => {
                    let mut r = Response::new(Body::from(format!("ssr failed: {e}")));
                    *r.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                    return finish_with_flash(r, pending, per).await;
                }
                Err(e) => tracing::warn!(error = ?e, "SSR failed; falling back to client render"),
            }
        }

        let mut html = cfg
            .root_view
            .render(RootViewContext {
                page_json: &escaped,
                page_json_script: &script_escaped,
                asset_version: &version,
                ssr: ssr_payload.as_ref(),
            })
            .unwrap_or_else(|e| format!("root view error: {e}"));
        // The extension sees the first page load through the DOM only.
        if let (Some(id), Some(at)) = (&per.devtools_id, html.rfind("</body>")) {
            html.insert_str(at, &crate::devtools::Recording::script_tag(id));
        }
        let mut r = Response::new(Body::from(html));
        r.headers_mut().insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
        r
    };

    if let Some(recorded) = recorded {
        response.extensions_mut().insert(recorded);
    }
    finish_with_flash(response, pending, per).await
}

/// The rendered page object, for the DevTools recorder.
#[derive(Clone)]
pub(crate) struct RecordedPage(pub Arc<Value>);

/// An empty response that points the client at `location` through `header`.
pub(crate) fn redirect(status: StatusCode, header: HeaderName, location: &str) -> Response<Body> {
    let mut r = Response::new(Body::empty());
    *r.status_mut() = status;
    r.headers_mut().insert(
        header,
        HeaderValue::from_str(location).unwrap_or(HeaderValue::from_static("/")),
    );
    r
}

/// 409 for an asset version mismatch. `X-Inertia-Version` tells the client
/// that this is a version change and not an external redirect.
pub(crate) fn version_mismatch(location: &str, version: &str) -> Response<Body> {
    let mut r = redirect(
        StatusCode::CONFLICT,
        veer_headers::X_INERTIA_LOCATION,
        location,
    );
    if let Ok(v) = HeaderValue::from_str(version) {
        r.headers_mut().insert(&veer_headers::X_INERTIA_VERSION, v);
    }
    r
}

pub(crate) async fn finish_with_flash(
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

/// Make a JSON string safe to embed inside `<script>...</script>`. `<`, `>`
/// and `/` occur only inside JSON strings, so their JSON escapes leave the
/// parsed value identical. Without a literal `<` there is no `</script>` and
/// no `<!--` (which would put the HTML parser in a state where the page stays
/// blank). The protocol also requires the `\/` escape.
fn script_tag_escape(json: &str) -> String {
    json.replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('/', "\\/")
}

#[cfg(test)]
mod tests {
    use super::script_tag_escape;

    #[test]
    fn script_escape_keeps_the_json_value() {
        let value = serde_json::json!({"a": "</script><!--<script>", "url": "/x"});
        let escaped = script_tag_escape(&value.to_string());
        assert!(!escaped.contains('<') && !escaped.contains('>'));
        assert!(escaped.contains(r#""\/x""#));
        let parsed: serde_json::Value = serde_json::from_str(&escaped).unwrap();
        assert_eq!(parsed, value);
    }
}
