#![cfg(feature = "csrf")]

use axum::{
    body::Body,
    routing::{get, post},
    Router,
};
use http::Request;
use tower::ServiceExt;
use veer::CsrfLayer;

const SECRET: &[u8] = b"0123456789012345678901234567890123456789";

fn app() -> Router {
    Router::new()
        .route("/", get(|| async { "ok" }))
        .route("/submit", post(|| async { "done" }))
        .layer(CsrfLayer::new(SECRET.to_vec()).secure(false))
}

fn req(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

/// GET issues a JS-readable XSRF-TOKEN cookie, then return its value.
async fn token_from_get() -> String {
    let resp = app().oneshot(req("GET", "/")).await.unwrap();
    let set = resp
        .headers()
        .get(http::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    set.split(';')
        .next()
        .unwrap()
        .split_once('=')
        .unwrap()
        .1
        .to_string()
}

#[tokio::test]
async fn get_issues_readable_xsrf_cookie() {
    let resp = app().oneshot(req("GET", "/")).await.unwrap();
    assert_eq!(resp.status(), 200);
    let set = resp
        .headers()
        .get(http::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(set.starts_with("XSRF-TOKEN="));
    assert!(!set.to_lowercase().contains("httponly"));
}

#[tokio::test]
async fn post_with_matching_token_passes() {
    let token = token_from_get().await;
    let r = Request::builder()
        .method("POST")
        .uri("/submit")
        .header(http::header::COOKIE, format!("XSRF-TOKEN={token}"))
        .header("x-xsrf-token", &token)
        .body(Body::empty())
        .unwrap();
    let resp = app().oneshot(r).await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn post_without_token_is_419() {
    let resp = app().oneshot(req("POST", "/submit")).await.unwrap();
    assert_eq!(resp.status(), 419);
}

#[tokio::test]
async fn post_with_mismatched_token_is_419() {
    let token = token_from_get().await;
    let r = Request::builder()
        .method("POST")
        .uri("/submit")
        .header(http::header::COOKIE, format!("XSRF-TOKEN={token}"))
        .header("x-xsrf-token", "different.value")
        .body(Body::empty())
        .unwrap();
    let resp = app().oneshot(r).await.unwrap();
    assert_eq!(resp.status(), 419);
}

#[tokio::test]
async fn excluded_path_skips_verification() {
    let app = Router::new()
        .route("/webhooks/stripe", post(|| async { "ok" }))
        .layer(
            CsrfLayer::new(SECRET.to_vec())
                .secure(false)
                .exclude("/webhooks"),
        );
    let resp = app.oneshot(req("POST", "/webhooks/stripe")).await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn valid_cookie_not_reissued() {
    let token = token_from_get().await;
    let r = Request::builder()
        .method("GET")
        .uri("/")
        .header(http::header::COOKIE, format!("XSRF-TOKEN={token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app().oneshot(r).await.unwrap();
    assert!(resp.headers().get(http::header::SET_COOKIE).is_none());
}
