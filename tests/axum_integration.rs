mod common;

use axum::{
    routing::{get, post},
    Router,
};
use common::*;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;
use veer::{Inertia, InertiaConfig, InertiaLayer, MinimalRootView};

fn app(config: InertiaConfig) -> Router {
    Router::new()
        .route(
            "/",
            get(|inertia: Inertia| async move { inertia.render("Home", json!({"msg": "hello"})) }),
        )
        .route(
            "/with-lazy",
            get(|inertia: Inertia| async move {
                inertia
                    .render("WithLazy", json!({"users": [1,2]}))
                    .lazy("stats", || async { json!({"hits": 99}) })
            }),
        )
        .route(
            "/users",
            post(|inertia: Inertia| async move {
                inertia
                    .with_errors(vec![("name", "is required")])
                    .redirect("/users/new")
            }),
        )
        .route(
            "/users/new",
            get(|inertia: Inertia| async move { inertia.render("Users/New", json!({})) }),
        )
        .layer(InertiaLayer::new(config))
}

#[tokio::test]
async fn initial_get_returns_html_with_data_page() {
    let cfg = InertiaConfig::new()
        .version(|| "v1".into())
        .root_view(MinimalRootView::new().title("T"));
    let resp = app(cfg).oneshot(req("GET", "/")).await.unwrap();
    assert_eq!(resp.status(), 200);
    assert!(resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("text/html"));
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let html = std::str::from_utf8(&body).unwrap();
    assert!(html.contains("data-page="));
    assert!(html.contains("Home"));
}

#[tokio::test]
async fn xhr_get_returns_json_when_version_matches() {
    let cfg = InertiaConfig::new().version(|| "v1".into());
    let resp = app(cfg)
        .oneshot(req_inertia("GET", "/", "v1"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("x-inertia").unwrap(), "true");
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/json"
    );
}

#[tokio::test]
async fn xhr_get_with_stale_version_returns_409() {
    let cfg = InertiaConfig::new().version(|| "v2".into());
    let resp = app(cfg)
        .oneshot(req_inertia("GET", "/", "v1"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    assert_eq!(resp.headers().get("x-inertia-location").unwrap(), "/");
}

#[tokio::test]
async fn post_redirect_is_303_and_flashes_errors() {
    let session = MockSession::default();
    let cfg = InertiaConfig::new()
        .version(|| "v1".into())
        .session(session.clone());
    let app = app(cfg);

    let r1 = app.clone().oneshot(req("POST", "/users")).await.unwrap();
    assert_eq!(r1.status(), 303);
    assert_eq!(r1.headers().get("location").unwrap(), "/users/new");

    // session now holds the flash
    let g = session.store.lock().await;
    assert_eq!(g.errors.get("name").unwrap(), "is required");
}

#[tokio::test]
async fn partial_reload_returns_only_requested_lazy() {
    let cfg = InertiaConfig::new().version(|| "v1".into());
    let req = http::Request::builder()
        .method("GET")
        .uri("/with-lazy")
        .header("x-inertia", "true")
        .header("x-inertia-version", "v1")
        .header("x-inertia-partial-component", "WithLazy")
        .header("x-inertia-partial-data", "stats")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app(cfg).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let page: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // Partial reload requested only "stats" → base props ("users") + auto-shared
    // (errors/flash) are stripped by the partial filter; only "stats" remains.
    assert_eq!(page["props"], json!({"stats": {"hits": 99}}));
}

#[tokio::test]
async fn csr_only_returns_json_for_plain_get() {
    let cfg = InertiaConfig::new().version(|| "v1".into()).csr_only(true);
    let resp = app(cfg).oneshot(req("GET", "/")).await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/json"
    );
}
