//! Spec-coverage tests for the Inertia v3 protocol.
//!
//! Each test pins one behavior described in <https://inertiajs.com/the-protocol>.
//! Add tests here when you discover a behavior the existing suite doesn't pin.

mod common;

use axum::{
    response::Json,
    routing::{get, post},
    Router,
};
use common::*;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;
use veer::{Inertia, InertiaConfig, InertiaLayer, MinimalRootView};

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn body_string(resp: axum::response::Response) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

// =============================================================================
// PAGE OBJECT WIRE FORMAT
// =============================================================================

#[tokio::test]
async fn json_response_sets_vary_x_inertia_header() {
    let cfg = InertiaConfig::new().version(|| "v1".into());
    let app = Router::new()
        .route(
            "/",
            get(|i: Inertia| async move { i.render("Home", json!({})) }),
        )
        .layer(InertiaLayer::new(cfg));
    let resp = app.oneshot(req_inertia("GET", "/", "v1")).await.unwrap();
    assert_eq!(resp.headers().get("vary").unwrap(), "X-Inertia");
}

#[tokio::test]
async fn encrypt_and_clear_history_flags_serialize_on_wire() {
    let cfg = InertiaConfig::new().version(|| "v1".into());
    let app = Router::new()
        .route(
            "/",
            get(|i: Inertia| async move {
                i.render("Home", json!({}))
                    .encrypt_history()
                    .clear_history()
            }),
        )
        .layer(InertiaLayer::new(cfg));
    let resp = app.oneshot(req_inertia("GET", "/", "v1")).await.unwrap();
    let page = body_json(resp).await;
    assert_eq!(page["encryptHistory"], true);
    assert_eq!(page["clearHistory"], true);
}

#[tokio::test]
async fn reset_merge_props_from_builder_serialize_on_wire() {
    let cfg = InertiaConfig::new().version(|| "v1".into());
    let app = Router::new()
        .route(
            "/",
            get(|i: Inertia| async move { i.render("Home", json!({})).reset_merge(["notifs"]) }),
        )
        .layer(InertiaLayer::new(cfg));
    let resp = app.oneshot(req_inertia("GET", "/", "v1")).await.unwrap();
    let page = body_json(resp).await;
    assert_eq!(page["resetMergeProps"], json!(["notifs"]));
}

#[tokio::test]
async fn x_inertia_reset_request_header_propagates_to_reset_merge_props() {
    let cfg = InertiaConfig::new().version(|| "v1".into());
    let app = Router::new()
        .route(
            "/",
            get(|i: Inertia| async move { i.render("Home", json!({})) }),
        )
        .layer(InertiaLayer::new(cfg));
    let r = http::Request::builder()
        .method("GET")
        .uri("/")
        .header("x-inertia", "true")
        .header("x-inertia-version", "v1")
        .header("x-inertia-reset", "alpha,beta")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(r).await.unwrap();
    let page = body_json(resp).await;
    // Sorted by finalize().
    assert_eq!(page["resetMergeProps"], json!(["alpha", "beta"]));
}

// =============================================================================
// REDIRECTS
// =============================================================================

#[tokio::test]
async fn external_redirect_via_location_returns_409_with_inertia_location_header() {
    let cfg = InertiaConfig::new().version(|| "v1".into());
    let app = Router::new()
        .route(
            "/oauth",
            get(|i: Inertia| async move { i.location("https://accounts.example.com/auth") }),
        )
        .layer(InertiaLayer::new(cfg));
    let resp = app.oneshot(req("GET", "/oauth")).await.unwrap();
    assert_eq!(resp.status(), 409);
    assert_eq!(
        resp.headers().get("x-inertia-location").unwrap(),
        "https://accounts.example.com/auth"
    );
}

#[tokio::test]
async fn external_redirect_works_from_post_too() {
    let cfg = InertiaConfig::new().version(|| "v1".into());
    let app = Router::new()
        .route(
            "/checkout",
            post(|i: Inertia| async move { i.location("https://stripe.example.com/session") }),
        )
        .layer(InertiaLayer::new(cfg));
    let resp = app.oneshot(req("POST", "/checkout")).await.unwrap();
    assert_eq!(resp.status(), 409);
    assert!(resp
        .headers()
        .get("x-inertia-location")
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("https://stripe.example.com"));
}

#[tokio::test]
async fn back_redirects_to_referer_header() {
    let cfg = InertiaConfig::new().version(|| "v1".into());
    let app = Router::new()
        .route("/submit", post(|i: Inertia| async move { i.back() }))
        .layer(InertiaLayer::new(cfg));
    let r = http::Request::builder()
        .method("POST")
        .uri("/submit")
        .header("referer", "/form")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(r).await.unwrap();
    assert_eq!(resp.status(), 303);
    assert_eq!(resp.headers().get("location").unwrap(), "/form");
}

#[tokio::test]
async fn back_falls_back_to_root_when_no_referer() {
    let cfg = InertiaConfig::new().version(|| "v1".into());
    let app = Router::new()
        .route("/submit", post(|i: Inertia| async move { i.back() }))
        .layer(InertiaLayer::new(cfg));
    let resp = app.oneshot(req("POST", "/submit")).await.unwrap();
    assert_eq!(resp.status(), 303);
    assert_eq!(resp.headers().get("location").unwrap(), "/");
}

// =============================================================================
// PARTIAL RELOAD EDGE CASES
// =============================================================================

#[tokio::test]
async fn partial_reload_for_different_component_does_not_filter() {
    let cfg = InertiaConfig::new().version(|| "v1".into());
    let app = Router::new()
        .route(
            "/",
            get(|i: Inertia| async move { i.render("Home", json!({"a": 1, "b": 2})) }),
        )
        .layer(InertiaLayer::new(cfg));
    // Partial-component header names a DIFFERENT component than the one we render.
    // Spec: partial-reload filters only apply when the component matches.
    let r = http::Request::builder()
        .method("GET")
        .uri("/")
        .header("x-inertia", "true")
        .header("x-inertia-version", "v1")
        .header("x-inertia-partial-component", "OtherComponent")
        .header("x-inertia-partial-data", "a")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(r).await.unwrap();
    let page = body_json(resp).await;
    // Both keys present — filter doesn't apply for non-matching component.
    assert_eq!(page["props"]["a"], 1);
    assert_eq!(page["props"]["b"], 2);
}

// =============================================================================
// SSR
// =============================================================================

#[tokio::test]
async fn ssr_payload_injected_into_root_view_html() {
    let cfg = InertiaConfig::new()
        .version(|| "v1".into())
        .root_view(MinimalRootView::new().title("T"))
        .ssr(StubSsr);
    let app = Router::new()
        .route(
            "/",
            get(|i: Inertia| async move { i.render("Home", json!({})) }),
        )
        .layer(InertiaLayer::new(cfg));
    let resp = app.oneshot(req("GET", "/")).await.unwrap();
    assert_eq!(resp.status(), 200);
    let html = body_string(resp).await;
    assert!(
        html.contains("<div>ssr-body</div>"),
        "SSR body not injected: {html}"
    );
    assert!(
        html.contains("<meta name=\"stub\" content=\"1\">"),
        "SSR head not injected"
    );
}

#[tokio::test]
async fn ssr_failure_with_ssr_required_returns_500() {
    let cfg = InertiaConfig::new()
        .version(|| "v1".into())
        .ssr(FailingSsr)
        .ssr_required(true);
    let app = Router::new()
        .route(
            "/",
            get(|i: Inertia| async move { i.render("Home", json!({})) }),
        )
        .layer(InertiaLayer::new(cfg));
    let resp = app.oneshot(req("GET", "/")).await.unwrap();
    assert_eq!(resp.status(), 500);
}

#[tokio::test]
async fn ssr_failure_without_ssr_required_falls_back_to_client_render() {
    let cfg = InertiaConfig::new()
        .version(|| "v1".into())
        .root_view(MinimalRootView::new().title("T"))
        .ssr(FailingSsr); // ssr_required defaults to false
    let app = Router::new()
        .route(
            "/",
            get(|i: Inertia| async move { i.render("Home", json!({})) }),
        )
        .layer(InertiaLayer::new(cfg));
    let resp = app.oneshot(req("GET", "/")).await.unwrap();
    assert_eq!(resp.status(), 200);
    let html = body_string(resp).await;
    // No SSR body; client-render shell with data-page is enough.
    assert!(html.contains("data-page="));
}

#[tokio::test]
async fn no_ssr_per_response_skips_ssr_call() {
    let cfg = InertiaConfig::new()
        .version(|| "v1".into())
        .root_view(MinimalRootView::new().title("T"))
        .ssr(StubSsr);
    let app = Router::new()
        .route(
            "/",
            get(|i: Inertia| async move { i.render("Home", json!({})).no_ssr() }),
        )
        .layer(InertiaLayer::new(cfg));
    let resp = app.oneshot(req("GET", "/")).await.unwrap();
    let html = body_string(resp).await;
    assert!(
        !html.contains("ssr-body"),
        "SSR body shouldn't appear when no_ssr() set"
    );
}

// =============================================================================
// SESSION / FLASH
// =============================================================================

#[tokio::test]
async fn flashed_errors_appear_as_shared_prop_on_next_get() {
    let session = MockSession::default();
    // Pre-seed the mock session as if a previous request flashed errors.
    {
        let mut g = session.store.lock().await;
        g.errors.insert("name".into(), "is required".into());
    }
    let cfg = InertiaConfig::new()
        .version(|| "v1".into())
        .session(session.clone());
    let app = Router::new()
        .route(
            "/users/new",
            get(|i: Inertia| async move { i.render("Users/New", json!({})) }),
        )
        .layer(InertiaLayer::new(cfg));
    let resp = app
        .oneshot(req_inertia("GET", "/users/new", "v1"))
        .await
        .unwrap();
    let page = body_json(resp).await;
    assert_eq!(page["props"]["errors"]["name"], "is required");
}

#[tokio::test]
async fn flashed_messages_appear_under_flash_prop() {
    let session = MockSession::default();
    {
        let mut g = session.store.lock().await;
        g.bags.insert("success".into(), json!("Created"));
    }
    let cfg = InertiaConfig::new()
        .version(|| "v1".into())
        .session(session.clone());
    let app = Router::new()
        .route(
            "/users",
            get(|i: Inertia| async move { i.render("Users/Index", json!({})) }),
        )
        .layer(InertiaLayer::new(cfg));
    let resp = app
        .oneshot(req_inertia("GET", "/users", "v1"))
        .await
        .unwrap();
    let page = body_json(resp).await;
    assert_eq!(page["props"]["flash"]["success"], "Created");
}

#[cfg(feature = "cookie-session")]
#[tokio::test]
async fn cookie_session_round_trips_flash_through_layer() {
    use veer::session::cookie::CookieSessionStore;
    let store = CookieSessionStore::new(vec![0u8; 32]).secure(false);
    let cfg = InertiaConfig::new().version(|| "v1".into()).session(store);
    let app = Router::new()
        .route(
            "/users",
            post(|i: Inertia| async move {
                i.with_errors(vec![("name", "is required")])
                    .redirect("/users/new")
            }),
        )
        .route(
            "/users/new",
            get(|i: Inertia| async move { i.render("Users/New", json!({})) }),
        )
        .layer(InertiaLayer::new(cfg));

    // POST → 303 with Set-Cookie carrying the flash.
    let r1 = app.clone().oneshot(req("POST", "/users")).await.unwrap();
    assert_eq!(r1.status(), 303);
    let set_cookie = r1
        .headers()
        .get(http::header::SET_COOKIE)
        .expect("flash cookie set")
        .to_str()
        .unwrap()
        .to_string();

    // GET → reuse the cookie; errors prop populated from it.
    let r2 = http::Request::builder()
        .method("GET")
        .uri("/users/new")
        .header("x-inertia", "true")
        .header("x-inertia-version", "v1")
        .header(
            http::header::COOKIE,
            set_cookie.split(';').next().unwrap().to_string(),
        )
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(r2).await.unwrap();
    assert_eq!(resp.status(), 200);
    let page = body_json(resp).await;
    assert_eq!(page["props"]["errors"]["name"], "is required");
}

// =============================================================================
// NON-INERTIA PASSTHROUGH
// =============================================================================

#[tokio::test]
async fn non_inertia_handler_response_passes_through_untouched() {
    let cfg = InertiaConfig::new().version(|| "v1".into());
    let app = Router::new()
        .route("/api/raw", get(|| async { Json(json!({"raw": true})) }))
        .layer(InertiaLayer::new(cfg));
    let resp = app.oneshot(req("GET", "/api/raw")).await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/json"
    );
    // No X-Inertia header on non-Inertia responses.
    assert!(resp.headers().get("x-inertia").is_none());
    let v = body_json(resp).await;
    assert_eq!(v, json!({"raw": true}));
}

// =============================================================================
// LAZY ALIAS
// =============================================================================

#[tokio::test]
async fn optional_is_alias_for_lazy_and_routes_through_same_map() {
    let cfg = InertiaConfig::new().version(|| "v1".into());
    let app = Router::new()
        .route(
            "/page",
            get(|i: Inertia| async move {
                i.render("Page", json!({"a": 1}))
                    .optional("opt", || async { json!("resolved") })
            }),
        )
        .layer(InertiaLayer::new(cfg));

    // Full request: optional is excluded.
    let resp = app
        .clone()
        .oneshot(req_inertia("GET", "/page", "v1"))
        .await
        .unwrap();
    let page = body_json(resp).await;
    assert!(page["props"].get("opt").is_none());

    // Partial requesting "opt": it's resolved.
    let r = http::Request::builder()
        .method("GET")
        .uri("/page")
        .header("x-inertia", "true")
        .header("x-inertia-version", "v1")
        .header("x-inertia-partial-component", "Page")
        .header("x-inertia-partial-data", "opt")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(r).await.unwrap();
    let page = body_json(resp).await;
    assert_eq!(page["props"]["opt"], "resolved");
}

// =============================================================================
// INERTIAFORM EXTRACTOR
// =============================================================================

mod inertia_form_tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use veer::InertiaForm;

    #[derive(Deserialize, Serialize, Debug, PartialEq)]
    struct NewItem {
        title: String,
        qty: u32,
    }

    fn echo_app() -> Router {
        let cfg = InertiaConfig::new().version(|| "v1".into());
        Router::new()
            .route(
                "/items",
                post(
                    |i: Inertia, InertiaForm(body): InertiaForm<NewItem>| async move {
                        i.render(
                            "items/created",
                            json!({"got_title": body.title, "got_qty": body.qty}),
                        )
                    },
                ),
            )
            .layer(InertiaLayer::new(cfg))
    }

    #[tokio::test]
    async fn accepts_application_json_body() {
        let r = http::Request::builder()
            .method("POST")
            .uri("/items")
            .header("x-inertia", "true")
            .header("x-inertia-version", "v1")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"title":"buy milk","qty":2}"#))
            .unwrap();
        let resp = echo_app().oneshot(r).await.unwrap();
        assert_eq!(resp.status(), 200);
        let page = body_json(resp).await;
        assert_eq!(page["props"]["got_title"], "buy milk");
        assert_eq!(page["props"]["got_qty"], 2);
    }

    #[tokio::test]
    async fn accepts_form_urlencoded_body() {
        let r = http::Request::builder()
            .method("POST")
            .uri("/items")
            .header("x-inertia", "true")
            .header("x-inertia-version", "v1")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(axum::body::Body::from("title=buy+milk&qty=2"))
            .unwrap();
        let resp = echo_app().oneshot(r).await.unwrap();
        assert_eq!(resp.status(), 200);
        let page = body_json(resp).await;
        assert_eq!(page["props"]["got_title"], "buy milk");
        assert_eq!(page["props"]["got_qty"], 2);
    }

    #[tokio::test]
    async fn accepts_json_with_charset_parameter() {
        // Some clients send `application/json; charset=utf-8`. starts_with handles it.
        let r = http::Request::builder()
            .method("POST")
            .uri("/items")
            .header("x-inertia", "true")
            .header("x-inertia-version", "v1")
            .header("content-type", "application/json; charset=utf-8")
            .body(axum::body::Body::from(r#"{"title":"x","qty":1}"#))
            .unwrap();
        let resp = echo_app().oneshot(r).await.unwrap();
        assert_eq!(resp.status(), 200);
    }

    #[tokio::test]
    async fn unsupported_content_type_returns_415() {
        let r = http::Request::builder()
            .method("POST")
            .uri("/items")
            .header("content-type", "application/xml")
            .body(axum::body::Body::from("<x/>"))
            .unwrap();
        let resp = echo_app().oneshot(r).await.unwrap();
        assert_eq!(resp.status(), 415);
        let body = body_string(resp).await;
        assert!(body.contains("application/json"));
        assert!(body.contains("application/x-www-form-urlencoded"));
    }
}

// =============================================================================
// MULTIPART / FILE UPLOADS
// =============================================================================

#[cfg(feature = "multipart")]
mod multipart_tests {
    use super::*;
    use serde::Deserialize;
    use veer::{InertiaForm, UploadedFile};

    #[derive(Deserialize)]
    struct CreateAvatar {
        user_id: String,
        avatar: UploadedFile,
    }

    /// Build a minimal `multipart/form-data` body. Real clients use a random
    /// boundary; for tests a fixed one is fine.
    fn multipart_body(boundary: &str, parts: &[Part<'_>]) -> Vec<u8> {
        let mut body = Vec::new();
        for p in parts {
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            match p {
                Part::Text { name, value } => {
                    body.extend_from_slice(
                        format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n")
                            .as_bytes(),
                    );
                    body.extend_from_slice(value.as_bytes());
                    body.extend_from_slice(b"\r\n");
                }
                Part::File {
                    name,
                    filename,
                    content_type,
                    bytes,
                } => {
                    body.extend_from_slice(
                        format!(
                            "Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n\
                             Content-Type: {content_type}\r\n\r\n"
                        )
                        .as_bytes(),
                    );
                    body.extend_from_slice(bytes);
                    body.extend_from_slice(b"\r\n");
                }
            }
        }
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        body
    }

    enum Part<'a> {
        Text {
            name: &'a str,
            value: &'a str,
        },
        File {
            name: &'a str,
            filename: &'a str,
            content_type: &'a str,
            bytes: &'a [u8],
        },
    }

    fn upload_app() -> Router {
        let cfg = InertiaConfig::new().version(|| "v1".into());
        Router::new()
            .route(
                "/avatar",
                post(
                    |i: Inertia, InertiaForm(form): InertiaForm<CreateAvatar>| async move {
                        i.render(
                            "avatar/created",
                            json!({
                                "user_id": form.user_id,
                                "uploaded_filename": form.avatar.filename,
                                "uploaded_content_type": form.avatar.content_type,
                                "uploaded_size": form.avatar.bytes.len(),
                            }),
                        )
                    },
                ),
            )
            .layer(InertiaLayer::new(cfg))
    }

    #[tokio::test]
    async fn multipart_with_file_deserializes_into_struct() {
        let boundary = "veer-test-1";
        let body = multipart_body(
            boundary,
            &[
                Part::Text {
                    name: "user_id",
                    value: "42",
                },
                Part::File {
                    name: "avatar",
                    filename: "me.png",
                    content_type: "image/png",
                    bytes: b"\x89PNGfakebytes",
                },
            ],
        );
        let r = http::Request::builder()
            .method("POST")
            .uri("/avatar")
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .header("x-inertia", "true")
            .header("x-inertia-version", "v1")
            .body(axum::body::Body::from(body))
            .unwrap();
        let resp = upload_app().oneshot(r).await.unwrap();
        assert_eq!(resp.status(), 200);
        let page = body_json(resp).await;
        assert_eq!(page["props"]["user_id"], "42");
        assert_eq!(page["props"]["uploaded_filename"], "me.png");
        assert_eq!(page["props"]["uploaded_content_type"], "image/png");
        assert_eq!(page["props"]["uploaded_size"], 13);
    }

    #[tokio::test]
    async fn multipart_with_indexed_files_deserializes_into_vec() {
        #[derive(Deserialize)]
        struct UploadDocuments {
            files: Vec<UploadedFile>,
        }

        let cfg = InertiaConfig::new().version(|| "v1".into());
        let app = Router::new()
            .route(
                "/documents",
                post(
                    |i: Inertia, InertiaForm(form): InertiaForm<UploadDocuments>| async move {
                        i.render(
                            "documents/created",
                            json!({
                                "filenames": form.files.into_iter().map(|file| file.filename).collect::<Vec<_>>(),
                            }),
                        )
                    },
                ),
            )
            .layer(InertiaLayer::new(cfg));

        let boundary = "veer-test-indexed-files";
        let body = multipart_body(
            boundary,
            &[
                Part::File {
                    name: "files[0]",
                    filename: "first.pdf",
                    content_type: "application/pdf",
                    bytes: b"first",
                },
                Part::File {
                    name: "files[1]",
                    filename: "second.pdf",
                    content_type: "application/pdf",
                    bytes: b"second",
                },
            ],
        );
        let request = http::Request::builder()
            .method("POST")
            .uri("/documents")
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .header("x-inertia", "true")
            .header("x-inertia-version", "v1")
            .body(axum::body::Body::from(body))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), 200);
        let page = body_json(response).await;
        assert_eq!(
            page["props"]["filenames"],
            json!(["first.pdf", "second.pdf"])
        );
    }

    #[tokio::test]
    async fn multipart_text_only_still_works() {
        // A multipart submission with no file fields should still deserialize
        // text fields correctly.
        #[derive(Deserialize)]
        struct OnlyText {
            name: String,
        }

        let cfg = InertiaConfig::new().version(|| "v1".into());
        let app = Router::new()
            .route(
                "/echo",
                post(
                    |i: Inertia, InertiaForm(body): InertiaForm<OnlyText>| async move {
                        i.render("echo", json!({"got": body.name}))
                    },
                ),
            )
            .layer(InertiaLayer::new(cfg));

        let boundary = "veer-test-2";
        let body = multipart_body(
            boundary,
            &[Part::Text {
                name: "name",
                value: "claude",
            }],
        );
        let r = http::Request::builder()
            .method("POST")
            .uri("/echo")
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .header("x-inertia", "true")
            .header("x-inertia-version", "v1")
            .body(axum::body::Body::from(body))
            .unwrap();
        let resp = app.oneshot(r).await.unwrap();
        assert_eq!(resp.status(), 200);
        let page = body_json(resp).await;
        assert_eq!(page["props"]["got"], "claude");
    }

    #[tokio::test]
    async fn missing_required_field_returns_422() {
        // No `user_id` part — serde_json::from_value should error, surfaced as 422.
        let boundary = "veer-test-3";
        let body = multipart_body(
            boundary,
            &[Part::File {
                name: "avatar",
                filename: "x.png",
                content_type: "image/png",
                bytes: b"x",
            }],
        );
        let r = http::Request::builder()
            .method("POST")
            .uri("/avatar")
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(axum::body::Body::from(body))
            .unwrap();
        let resp = upload_app().oneshot(r).await.unwrap();
        assert_eq!(resp.status(), 422);
    }

    #[tokio::test]
    async fn multipart_stream_extractor_yields_parts() {
        use veer::MultipartStream;

        let cfg = InertiaConfig::new().version(|| "v1".into());
        let app = Router::new()
            .route(
                "/stream",
                post(|MultipartStream(mut m): MultipartStream| async move {
                    let mut total = 0usize;
                    while let Some(field) = m.next_field().await.unwrap() {
                        let bytes = field.bytes().await.unwrap();
                        total += bytes.len();
                    }
                    axum::response::Json(serde_json::json!({"bytes_seen": total}))
                }),
            )
            .layer(InertiaLayer::new(cfg));

        let boundary = "veer-stream-1";
        let body = multipart_body(
            boundary,
            &[
                Part::File {
                    name: "a",
                    filename: "a.bin",
                    content_type: "application/octet-stream",
                    bytes: b"hello",
                },
                Part::File {
                    name: "b",
                    filename: "b.bin",
                    content_type: "application/octet-stream",
                    bytes: b"world!",
                },
            ],
        );
        let r = http::Request::builder()
            .method("POST")
            .uri("/stream")
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(axum::body::Body::from(body))
            .unwrap();
        let resp = app.oneshot(r).await.unwrap();
        assert_eq!(resp.status(), 200);
        let v = body_json(resp).await;
        assert_eq!(v["bytes_seen"], 11); // "hello" (5) + "world!" (6)
    }
}
