use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::get,
    Router,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tower::ServiceExt;
use veer::{Inertia, InertiaConfig, InertiaLayer, Prop, ScrollMetadata, SharedPropsData};

#[tokio::test]
async fn selected_props_evaluate_only_the_requested_loaders() {
    for (component, only, except, expected) in [
        ("", "", "", [1, 0, 1]),
        ("Page", "picker", "", [0, 1, 0]),
        ("Page", "picker,rows", "", [0, 1, 1]),
        ("Page", "", "rows", [1, 0, 0]),
        ("Page", "picker,rows", "picker", [0, 0, 1]),
        ("Other", "picker", "", [1, 0, 1]),
    ] {
        let counts = Arc::new([
            AtomicUsize::new(0),
            AtomicUsize::new(0),
            AtomicUsize::new(0),
        ]);
        let state = counts.clone();
        let app = Router::new()
            .route(
                "/",
                get(move |inertia: Inertia| {
                    let counts = state.clone();
                    async move {
                        let regular = counts.clone();
                        let optional = counts.clone();
                        inertia
                            .render("Page", json!({}))
                            .try_prop("details", move || async move {
                                regular[0].fetch_add(1, Ordering::SeqCst);
                                Ok::<_, StatusCode>(json!({"name":"Ada"}))
                            })
                            .try_optional("picker", move || async move {
                                optional[1].fetch_add(1, Ordering::SeqCst);
                                Ok::<_, StatusCode>(vec![1])
                            })
                            .prop(
                                "rows",
                                Prop::try_scroll(move || async move {
                                    counts[2].fetch_add(1, Ordering::SeqCst);
                                    Ok::<_, StatusCode>((
                                        json!({"data":[{"id":1}]}),
                                        ScrollMetadata::paged("page", 1, false),
                                    ))
                                })
                                .match_on("data.id"),
                            )
                    }
                }),
            )
            .layer(InertiaLayer::new(InertiaConfig::new()));
        let request = Request::builder()
            .uri("/")
            .header("x-inertia", "true")
            .header("x-inertia-version", "1")
            .header("x-inertia-partial-component", component)
            .header("x-inertia-partial-data", only)
            .header("x-inertia-partial-except", except)
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        for (index, expected) in expected.into_iter().enumerate() {
            assert_eq!(
                counts[index].load(Ordering::SeqCst),
                expected,
                "{component} only={only} except={except}"
            );
        }
        assert_eq!(body.get("scrollProps").is_some(), expected[2] == 1);
    }
}

#[tokio::test]
async fn selected_props_preserve_application_error_responses() {
    let app = Router::new()
        .route(
            "/",
            get(|inertia: Inertia| async move {
                inertia
                    .render("Page", json!({}))
                    .try_optional("picker", || async {
                        Err::<Value, _>((
                            StatusCode::SERVICE_UNAVAILABLE,
                            [("retry-after", "10")],
                            "provider unavailable",
                        ))
                    })
            }),
        )
        .layer(InertiaLayer::new(InertiaConfig::new()));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("x-inertia", "true")
                .header("x-inertia-version", "1")
                .header("x-inertia-partial-component", "Page")
                .header("x-inertia-partial-data", "picker")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.headers()["retry-after"], "10");
    assert_eq!(
        response.into_body().collect().await.unwrap().to_bytes(),
        "provider unavailable"
    );
}

#[tokio::test]
async fn selected_props_page_overrides_shared_loaders() {
    let config = InertiaConfig::new().shared(veer::shared::shared_props_fn(|_| async {
        SharedPropsData::new(json!({}))
            .prop(
                "title",
                Prop::new::<_, _, ()>(|| async { panic!("overridden shared loader") }),
            )
            .prop("picker", Prop::new(|| async { vec![1] }).optional())
    }));
    let app = Router::new()
        .route(
            "/",
            get(|inertia: Inertia| async move {
                inertia
                    .render("Page", json!({}))
                    .prop("title", Prop::new(|| async { "page title" }))
            }),
        )
        .layer(InertiaLayer::new(config));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("x-inertia", "true")
                .header("x-inertia-version", "1")
                .header("x-inertia-partial-component", "Page")
                .header("x-inertia-partial-data", "title,picker")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["props"]["title"], "page title");
    assert_eq!(body["props"]["picker"], json!([1]));
}
