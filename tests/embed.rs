#![cfg(feature = "embed")]

use axum::{body::Body, Router};
use http::Request;
use http_body_util::BodyExt;
use std::borrow::Cow;
use tower::ServiceExt;
use veer::EmbeddedAssets;

fn assets() -> EmbeddedAssets {
    EmbeddedAssets::new(|p: &str| match p {
        "assets/app-AAA.js" => Some(Cow::Borrowed(b"console.log(1)" as &[u8])),
        _ => None,
    })
}

fn app() -> Router {
    Router::new().nest_service("/build", assets())
}

#[tokio::test]
async fn serves_known_asset_with_type_and_cache() {
    let resp = app()
        .oneshot(
            Request::builder()
                .uri("/build/assets/app-AAA.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-type").unwrap(), "text/javascript");
    assert_eq!(
        resp.headers().get("cache-control").unwrap(),
        "public, max-age=31536000, immutable"
    );
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"console.log(1)");
}

#[tokio::test]
async fn unknown_asset_is_404() {
    let resp = app()
        .oneshot(
            Request::builder()
                .uri("/build/nope.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn head_has_length_but_empty_body() {
    let resp = app()
        .oneshot(
            Request::builder()
                .method("HEAD")
                .uri("/build/assets/app-AAA.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-length").unwrap(), "14");
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert!(body.is_empty());
}

#[tokio::test]
async fn mime_override_takes_effect() {
    let a = EmbeddedAssets::new(|p: &str| {
        (p == "x.custom").then_some(Cow::Borrowed(b"hi" as &[u8]))
    })
    .mime("custom", "application/x-custom");
    let app = Router::new().nest_service("/build", a);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/build/x.custom")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/x-custom"
    );
}

#[tokio::test]
async fn non_get_head_method_is_405_with_allow_header() {
    let resp = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/build/assets/app-AAA.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 405);
    assert_eq!(resp.headers().get("allow").unwrap(), "GET, HEAD");
}
