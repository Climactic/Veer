mod common;

use axum::{
    routing::{get, post},
    Extension, Router,
};
use common::*;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;
#[cfg(feature = "tower-sessions")]
use tower_sessions::{MemoryStore, Session, SessionManagerLayer};
use veer::{shared::shared_props_fn, Inertia, InertiaConfig, InertiaLayer, MinimalRootView};

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

#[tokio::test]
async fn shared_props_can_read_request_extensions() {
    #[derive(Clone)]
    struct SessionUser(&'static str);

    let cfg = InertiaConfig::new()
        .version(|| "v1".into())
        .shared(shared_props_fn(|request| {
            let user = request.extension::<SessionUser>().map(|user| user.0);
            async move { json!({"auth": {"user": user}}) }
        }));
    let app = app(cfg).layer(Extension(SessionUser("Ada")));

    let response = app.oneshot(req_inertia("GET", "/", "v1")).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let page: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(page["props"]["auth"]["user"], "Ada");
}

#[cfg(feature = "tower-sessions")]
#[tokio::test]
async fn shared_props_can_read_tower_session() {
    let cfg = InertiaConfig::new()
        .version(|| "v1".into())
        .shared(shared_props_fn(|request| {
            let session = request.extension::<Session>().cloned();
            async move {
                let user = match session {
                    Some(session) => session.get::<String>("user").await.unwrap(),
                    None => None,
                };
                json!({"auth": {"user": user}})
            }
        }));
    let app = Router::new()
        .route(
            "/",
            get(|inertia: Inertia, session: Session| async move {
                session.insert("user", "Ada").await.unwrap();
                inertia.render("Home", json!({"msg": "hello"}))
            }),
        )
        .layer(InertiaLayer::new(cfg))
        .layer(SessionManagerLayer::new(MemoryStore::default()).with_secure(false));

    let response = app.oneshot(req_inertia("GET", "/", "v1")).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let page: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(page["props"]["auth"]["user"], "Ada");
}

#[tokio::test]
async fn shared_lazy_props_execute_only_when_requested_and_page_props_win() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let shared_calls = calls.clone();
    let cfg = InertiaConfig::new()
        .version(|| "v1".into())
        .shared(shared_props_fn(move |_| {
            let calls = shared_calls.clone();
            async move {
                veer::SharedPropsData::new(json!({"count": 3}))
                    .lazy("notifications", move || async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        json!(["notice"])
                    })
                    .lazy("msg", || async { panic!("page values must win") })
                    .lazy("stats", || async { panic!("page lazy values must win") })
            }
        }));
    let app = app(cfg);
    for (path, component, only, expected_calls) in [
        ("/", "Home", "", 0),
        ("/", "Home", "count", 0),
        ("/", "WrongComponent", "notifications", 0),
        ("/", "Home", "notifications", 1),
        ("/", "Home", "msg", 1),
        ("/with-lazy", "WithLazy", "stats", 1),
    ] {
        let mut request = req_inertia("GET", path, "v1");
        if !only.is_empty() {
            request
                .headers_mut()
                .insert("x-inertia-partial-component", component.parse().unwrap());
            request
                .headers_mut()
                .insert("x-inertia-partial-data", only.parse().unwrap());
        }
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), 200);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let page: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), expected_calls);
        if component == "Home" && only == "notifications" {
            assert_eq!(page["props"]["notifications"], json!(["notice"]));
        }
    }
}

#[tokio::test]
async fn non_page_responses_skip_display_resolvers_and_preserve_flash() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let session = MockSession::default();
    let cfg = InertiaConfig::new()
        .version(|| "v2".into())
        .session(session.clone())
        .shared(shared_props_fn(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            async { json!({}) }
        }));
    let app = Router::new()
        .route(
            "/redirect",
            post(|inertia: Inertia| async move {
                inertia
                    .redirect("/destination")
                    .with_flash("saved", json!(true))
            }),
        )
        .route(
            "/external",
            get(|inertia: Inertia| async move { inertia.location("https://example.com") }),
        )
        .route(
            "/page",
            get(|inertia: Inertia| async move {
                inertia
                    .render("Page", json!({}))
                    .prop("expensive", || async {
                        panic!("stale version must not load page display data");
                    })
                    .try_prop("fallible", || async {
                        Err::<serde_json::Value, _>(axum::http::StatusCode::IM_A_TEAPOT)
                    })
            }),
        )
        .layer(InertiaLayer::new(cfg));
    let response = app.clone().oneshot(req("POST", "/redirect")).await.unwrap();
    assert_eq!(response.status(), 303);
    assert_eq!(response.headers()["location"], "/destination");
    assert_eq!(
        session.store.lock().await.bags.get("saved"),
        Some(&json!(true))
    );
    let response = app.clone().oneshot(req("GET", "/external")).await.unwrap();
    assert_eq!(response.status(), 409);
    assert_eq!(
        response.headers()["x-inertia-location"],
        "https://example.com"
    );
    let response = app
        .oneshot(req_inertia("GET", "/page", "v1"))
        .await
        .unwrap();
    assert_eq!(response.status(), 409);
    assert_eq!(response.headers()["x-inertia-location"], "/page");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn ordinary_closures_run_only_when_needed_without_caching_across_visits() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let page_loads = Arc::new(AtomicUsize::new(0));
    let shared_loads = Arc::new(AtomicUsize::new(0));
    let shared_counter = shared_loads.clone();
    let cfg = InertiaConfig::new()
        .version(|| "v1".into())
        .shared(shared_props_fn(move |_| {
            let counter = shared_counter.clone();
            async move {
                veer::SharedPropsData::new(json!({})).prop("sidebar", move || async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    json!("tree")
                })
            }
        }));
    let page_counter = page_loads.clone();
    let app = Router::new()
        .route(
            "/page",
            get(move |inertia: Inertia| {
                let counter = page_counter.clone();
                async move {
                    inertia
                        .render("Page", json!({}))
                        .prop("detail", move || async move {
                            counter.fetch_add(1, Ordering::SeqCst);
                            json!("body")
                        })
                }
            }),
        )
        .layer(InertiaLayer::new(cfg));
    for (component, only, except, expected_page, expected_shared) in [
        ("", "", "", 1, 1),
        ("", "", "", 1, 1),
        ("Page", "detail", "", 1, 0),
        ("Page", "sidebar", "", 0, 1),
        ("Page", "", "detail", 0, 1),
        ("Page", "detail", "detail", 0, 0),
        ("OtherPage", "detail", "", 1, 1),
    ] {
        page_loads.store(0, Ordering::SeqCst);
        shared_loads.store(0, Ordering::SeqCst);
        let mut request = req_inertia("GET", "/page", "v1");
        for (key, value) in [
            ("x-inertia-partial-component", component),
            ("x-inertia-partial-data", only),
            ("x-inertia-partial-except", except),
        ] {
            if !value.is_empty() {
                request.headers_mut().insert(key, value.parse().unwrap());
            }
        }
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), 200);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let page: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(page["props"].get("detail").is_some(), expected_page == 1);
        assert_eq!(page["props"].get("sidebar").is_some(), expected_shared == 1);
        assert!(page.get("onceProps").is_none());
        assert_eq!(page_loads.load(Ordering::SeqCst), expected_page);
        assert_eq!(shared_loads.load(Ordering::SeqCst), expected_shared);
    }
}

#[tokio::test]
async fn fallible_closures_preserve_errors_and_scroll_closures_produce_merge_metadata() {
    let cfg = InertiaConfig::new()
        .version(|| "v1".into())
        .shared(shared_props_fn(|_| async {
            veer::SharedPropsData::new(json!({})).prop("items", || async { panic!("page wins") })
        }));
    let app = Router::new()
        .route(
            "/",
            get(|inertia: Inertia| async move {
                inertia
                    .render("Page", json!({}))
                    .try_prop("failure", || async {
                        Err::<serde_json::Value, _>((axum::http::StatusCode::NOT_FOUND, "missing"))
                    })
                    .try_scroll_prop("items", || async {
                        Ok::<_, axum::http::StatusCode>((
                            json!({"data": [{"id": 1}]}),
                            veer::ScrollMetadata::new(
                                "cursor",
                                "current".to_string(),
                                None,
                                Some("next".to_string()),
                            )
                            .match_on("id"),
                        ))
                    })
            }),
        )
        .layer(InertiaLayer::new(cfg));
    let mut request = req_inertia("GET", "/", "v1");
    request
        .headers_mut()
        .insert("x-inertia-partial-component", "Page".parse().unwrap());
    request
        .headers_mut()
        .insert("x-inertia-partial-data", "items".parse().unwrap());
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), 200);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let page: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(page["props"]["items"]["data"][0]["id"], 1);
    assert_eq!(page["scrollProps"]["items"]["nextPage"], "next");
    assert_eq!(page["mergeProps"], json!(["items.data"]));
    let response = app.oneshot(req_inertia("GET", "/", "v1")).await.unwrap();
    assert_eq!(response.status(), 404);
    assert_eq!(
        response.into_body().collect().await.unwrap().to_bytes(),
        "missing"
    );
}
