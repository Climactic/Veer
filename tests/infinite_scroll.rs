use axum::{body::Body, routing::get, Router};
use http::{HeaderValue, Request};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;
use veer::{Inertia, InertiaConfig, InertiaLayer, ScrollMetadata};

async fn response(headers: &[(&str, &str)]) -> Value {
    let app = Router::new()
        .route(
            "/",
            get(|inertia: Inertia| async move {
                inertia
                    .render(
                        "Users",
                        json!({"users": {"data": [{"id": 3}], "total": 5}, "title": "Users"}),
                    )
                    .scroll(
                        "users",
                        ScrollMetadata::new("page", 2_u32, Some(1), Some(3)),
                    )
            }),
        )
        .layer(InertiaLayer::new(
            InertiaConfig::new().version(|| "v1".into()),
        ));
    let mut request = Request::builder()
        .uri("/")
        .header("x-inertia", "true")
        .header("x-inertia-version", "v1")
        .body(Body::empty())
        .unwrap();
    for (name, value) in headers {
        request.headers_mut().insert(
            http::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
    }
    let response = app.oneshot(request).await.unwrap();
    assert!(response.status().is_success());
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
async fn scroll_metadata_and_append_path_are_emitted() {
    let page = response(&[]).await;
    assert_eq!(page["mergeProps"], json!(["users.data"]));
    assert_eq!(
        page["scrollProps"]["users"],
        json!({
            "pageName": "page", "currentPage": 2, "previousPage": 1, "nextPage": 3, "reset": false
        })
    );
    assert_eq!(page["props"]["users"]["data"], json!([{"id": 3}]));
}

#[tokio::test]
async fn scroll_previous_pages_prepend() {
    let page = response(&[
        ("x-inertia-partial-component", "Users"),
        ("x-inertia-partial-data", "users"),
        ("x-inertia-infinite-scroll-merge-intent", "prepend"),
    ])
    .await;
    assert_eq!(page["prependProps"], json!(["users.data"]));
    assert!(page.get("mergeProps").is_none());
}

#[tokio::test]
async fn resetting_scroll_replaces_data_instead_of_merging() {
    let page = response(&[
        ("x-inertia-partial-component", "Users"),
        ("x-inertia-partial-data", "users"),
        ("x-inertia-reset", "users"),
        ("x-inertia-infinite-scroll-merge-intent", "prepend"),
    ])
    .await;
    assert_eq!(page["scrollProps"]["users"]["reset"], true);
    assert!(page.get("mergeProps").is_none());
    assert!(page.get("prependProps").is_none());
}

#[tokio::test]
async fn excluded_scroll_props_have_no_metadata_or_merge_path() {
    let page = response(&[
        ("x-inertia-partial-component", "Users"),
        ("x-inertia-partial-data", "title"),
    ])
    .await;
    assert!(page["props"].get("users").is_none());
    assert!(page.get("scrollProps").is_none());
    assert!(page.get("mergeProps").is_none());
}

#[test]
fn cursor_boundaries_serialize_as_strings_or_null() {
    let metadata = ScrollMetadata::new(
        "cursor",
        "current".to_string(),
        None,
        Some("next".to_string()),
    );
    let value = serde_json::to_value(metadata).unwrap();
    assert_eq!(value["currentPage"], "current");
    assert_eq!(value["nextPage"], "next");
    assert_eq!(value["previousPage"], Value::Null);
}
