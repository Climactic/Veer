//! Developer-experience behavior: clear failures, `Validated`, shared props
//! with request data, page status.
#![cfg(all(feature = "testing", feature = "validator", feature = "garde"))]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde_json::json;
use std::collections::HashMap;
use tower::ServiceExt;
use veer::testing::{visit, MemorySession, TestPage};
use veer::{GardeValidated, Inertia, InertiaConfig, InertiaLayer, InertiaResponse, Validated};

async fn body_text(response: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn json_post(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::post(uri)
        .header("x-inertia", "true")
        .header("content-type", "application/json")
        .header("referer", "/todos/new")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn missing_layer_names_the_cause() {
    let app = Router::new().route(
        "/",
        get(|i: Inertia| async move { i.render("Home", json!({})) }),
    );
    let response = app.oneshot(visit("GET", "/")).await.unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(body_text(response).await.contains("InertiaLayer"));
}

#[tokio::test]
async fn props_that_do_not_serialize_are_a_500() {
    let app = Router::new()
        .route(
            "/",
            get(|i: Inertia| async move {
                // JSON object keys must be strings.
                i.render("Home", HashMap::from([((1, 2), 3)]))
            }),
        )
        .layer(InertiaLayer::new(InertiaConfig::new()));
    let response = app.oneshot(visit("GET", "/")).await.unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(body_text(response)
        .await
        .contains("`Home` did not serialize"));
}

#[tokio::test]
async fn status_applies_to_a_page_made_without_the_extractor() {
    async fn not_found() -> impl IntoResponse {
        InertiaResponse::render("Error", json!({ "status": 404 })).status(StatusCode::NOT_FOUND)
    }
    let app = Router::new()
        .fallback(not_found)
        .layer(InertiaLayer::new(InertiaConfig::new()));
    let response = app.oneshot(visit("GET", "/nothing")).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let page = TestPage::from_response(response).await;
    assert_eq!(page.component, "Error");
    assert_eq!(page.prop("status"), Some(&json!(404)));
}

#[tokio::test]
async fn shared_props_read_request_extensions() {
    #[derive(Clone)]
    struct User(&'static str);

    let config = InertiaConfig::new().share(|req| {
        let user = req.extension::<User>().map(|u| u.0);
        async move { json!({ "auth": { "user": user } }) }
    });
    let app = Router::new()
        .route(
            "/",
            get(|i: Inertia| async move { i.render("Home", json!({})) }),
        )
        .layer(InertiaLayer::new(config))
        .layer(axum::Extension(User("ada")));
    let page = TestPage::from_response(app.oneshot(visit("GET", "/")).await.unwrap()).await;
    assert_eq!(page.prop("auth.user"), Some(&json!("ada")));
}

#[derive(serde::Deserialize, validator::Validate)]
struct NewTodo {
    #[validate(length(min = 3, message = "too short"))]
    title: String,
}

#[derive(serde::Deserialize, garde::Validate)]
struct GardeTodo {
    #[garde(length(min = 3))]
    title: String,
}

fn todo_app(session: MemorySession) -> Router {
    Router::new()
        .route(
            "/todos",
            post(
                |i: Inertia, Validated(todo): Validated<NewTodo>| async move {
                    i.redirect(format!("/todos/{}", todo.title))
                },
            ),
        )
        .route(
            "/todos/new",
            get(|i: Inertia| async move { i.render("todos/create", json!({})) }),
        )
        .route(
            "/garde",
            post(
                |i: Inertia, GardeValidated(todo): GardeValidated<GardeTodo>| async move {
                    i.redirect(format!("/todos/{}", todo.title))
                },
            ),
        )
        .layer(InertiaLayer::new(InertiaConfig::new().session(session)))
}

#[tokio::test]
async fn validated_runs_the_handler_for_valid_input() {
    let app = todo_app(MemorySession::default());
    let response = app
        .oneshot(json_post("/todos", json!({ "title": "milk" })))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()["location"], "/todos/milk");
}

#[tokio::test]
async fn validated_redirects_back_with_the_errors() {
    for uri in ["/todos", "/garde"] {
        let session = MemorySession::default();
        let response = todo_app(session.clone())
            .oneshot(json_post(uri, json!({ "title": "a" })))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER, "{uri}");
        assert_eq!(response.headers()["location"], "/todos/new", "{uri}");
        assert!(session.flash().errors.contains_key("title"), "{uri}");

        // The page after the redirect has the error.
        let response = todo_app(session)
            .oneshot(visit("GET", "/todos/new"))
            .await
            .unwrap();
        let page = TestPage::from_response(response).await;
        assert!(page.prop("errors.title").is_some(), "{uri}");
    }
}

#[tokio::test]
async fn validated_answers_precognition_and_does_not_run_the_handler() {
    let precognition = |title: &str| {
        let mut request = json_post("/todos", json!({ "title": title }));
        request
            .headers_mut()
            .insert("precognition", "true".parse().unwrap());
        request
    };
    let app = todo_app(MemorySession::default());

    let invalid = app.clone().oneshot(precognition("a")).await.unwrap();
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body_text(invalid).await.contains("too short"));

    // Valid input: `204`, not the handler's redirect.
    let valid = app.oneshot(precognition("milk")).await.unwrap();
    assert_eq!(valid.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn back_does_not_redirect_to_another_site() {
    let app = Router::new()
        .route("/act", post(|i: Inertia| async move { i.back() }))
        .layer(InertiaLayer::new(InertiaConfig::new()));
    let location = |referer: &'static str| {
        let app = app.clone();
        async move {
            let request = Request::post("/act")
                .header("referer", referer)
                .body(Body::empty())
                .unwrap();
            let response = app.oneshot(request).await.unwrap();
            response.headers()["location"].to_str().unwrap().to_owned()
        }
    };
    assert_eq!(location("https://app.test/form?a=1").await, "/form?a=1");
    // The host of the Referer is never used: the target is a path of this site.
    assert_eq!(location("https://evil.test/form").await, "/form");
    assert_eq!(location("//evil.test/form").await, "/");
    assert_eq!(location("https://evil.test//other.test").await, "/");
}

#[tokio::test]
async fn page_url_never_starts_with_two_slashes() {
    let app = Router::new()
        .fallback(|i: Inertia| async move { i.render("Error", json!({})) })
        .layer(InertiaLayer::new(InertiaConfig::new()));
    let response = app.oneshot(visit("GET", "//evil.test/x")).await.unwrap();
    let page = TestPage::from_response(response).await;
    assert_eq!(page.url, "/evil.test/x");
}
