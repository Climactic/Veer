#![allow(dead_code)]

use axum::{
    routing::{get, post},
    Router,
};
use http_body_util::BodyExt;
use serde::Deserialize;
use tower::ServiceExt;
use veer::{Inertia, InertiaConfig, InertiaLayer};

#[derive(Deserialize)]
struct Suite {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    request: Req,
    expect: Expect,
}

#[derive(Deserialize)]
struct Req {
    method: String,
    uri: String,
    headers: std::collections::HashMap<String, String>,
}

#[derive(Deserialize)]
struct Expect {
    status: u16,
    #[serde(default)]
    content_type: Option<String>,
    #[serde(default)]
    content_type_starts_with: Option<String>,
    #[serde(default)]
    body_contains: Option<String>,
    /// Substring matches on the body, all must be present.
    #[serde(default)]
    body_contains_all: Vec<String>,
    /// Substrings that must NOT appear in the body.
    #[serde(default)]
    body_excludes: Vec<String>,
    #[serde(default)]
    x_inertia: Option<String>,
    #[serde(default)]
    x_inertia_location: Option<String>,
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    vary: Option<String>,
}

fn app() -> Router {
    let cfg = InertiaConfig::new().version(|| "v1".into());
    Router::new()
        .route(
            "/",
            get(|i: Inertia| async move { i.render("Home", serde_json::json!({})) }),
        )
        .route(
            "/users",
            get(|i: Inertia| async move { i.render("Users/Index", serde_json::json!({})) }),
        )
        .route(
            "/users",
            post(|i: Inertia| async move {
                i.with_errors(vec![("name", "is required")])
                    .redirect("/users/new")
            }),
        )
        .route(
            "/dashboard",
            get(|i: Inertia| async move {
                i.render("Dashboard", serde_json::json!({"summary": "ok"}))
                    .deferred("stats", "metrics", || async {
                        serde_json::json!({"hits": 7})
                    })
            }),
        )
        .route(
            "/oauth",
            get(|i: Inertia| async move { i.location("https://example.com/oauth") }),
        )
        .route(
            "/with-lazy",
            get(|i: Inertia| async move {
                i.render("WithLazy", serde_json::json!({"users": [1, 2]}))
                    .lazy("stats", || async { serde_json::json!({"hits": 99}) })
            }),
        )
        .layer(InertiaLayer::new(cfg))
}

#[tokio::test]
async fn replay_fixture_cases() {
    let raw = std::fs::read_to_string("tests/fixtures/conformance.json").unwrap();
    let suite: Suite = serde_json::from_str(&raw).unwrap();
    for case in suite.cases {
        let mut b = http::Request::builder()
            .method(case.request.method.as_str())
            .uri(case.request.uri.as_str());
        for (k, v) in &case.request.headers {
            b = b.header(k, v);
        }
        let r = b.body(axum::body::Body::empty()).unwrap();
        let resp = app().oneshot(r).await.unwrap();
        assert_eq!(
            resp.status().as_u16(),
            case.expect.status,
            "case {}",
            case.name
        );
        if let Some(ct) = case.expect.content_type {
            assert_eq!(
                resp.headers()
                    .get("content-type")
                    .unwrap()
                    .to_str()
                    .unwrap(),
                ct,
                "case {}",
                case.name
            );
        }
        if let Some(prefix) = case.expect.content_type_starts_with {
            assert!(
                resp.headers()
                    .get("content-type")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with(&prefix),
                "case {}",
                case.name
            );
        }
        if let Some(xi) = case.expect.x_inertia {
            assert_eq!(
                resp.headers().get("x-inertia").unwrap().to_str().unwrap(),
                xi,
                "case {}",
                case.name
            );
        }
        if let Some(loc) = case.expect.x_inertia_location {
            assert_eq!(
                resp.headers()
                    .get("x-inertia-location")
                    .unwrap()
                    .to_str()
                    .unwrap(),
                loc,
                "case {}",
                case.name
            );
        }
        if let Some(loc) = case.expect.location {
            assert_eq!(
                resp.headers().get("location").unwrap().to_str().unwrap(),
                loc,
                "case {}",
                case.name
            );
        }
        if let Some(vary) = case.expect.vary {
            assert_eq!(
                resp.headers().get("vary").unwrap().to_str().unwrap(),
                vary,
                "case {}",
                case.name
            );
        }
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let body_str = std::str::from_utf8(&body).unwrap();
        if let Some(needle) = case.expect.body_contains {
            assert!(
                body_str.contains(&needle),
                "case {}: body does not contain {needle:?}",
                case.name
            );
        }
        for needle in &case.expect.body_contains_all {
            assert!(
                body_str.contains(needle),
                "case {}: body does not contain {needle:?}",
                case.name
            );
        }
        for needle in &case.expect.body_excludes {
            assert!(
                !body_str.contains(needle),
                "case {}: body unexpectedly contains {needle:?}",
                case.name
            );
        }
    }
}
