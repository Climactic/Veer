//! Inertia v3 protocol behavior on the wire. Spec:
//! <https://inertiajs.com/docs/v3/core-concepts/the-protocol>

mod common;

use axum::response::Redirect;
use axum::routing::{get, post};
use axum::Router;
use common::{req, req_inertia, MockSession};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;
use veer::{Inertia, InertiaConfig, InertiaLayer, Prop, ScrollMetadata};

async fn body_string(resp: axum::response::Response) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

async fn page(resp: axum::response::Response) -> Value {
    assert_eq!(resp.status(), 200);
    serde_json::from_str(&body_string(resp).await).unwrap()
}

fn with_headers(
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
) -> http::Request<axum::body::Body> {
    let mut b = http::Request::builder()
        .method(method)
        .uri(uri)
        .header("x-inertia", "true")
        .header("x-inertia-version", "v1");
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    b.body(axum::body::Body::empty()).unwrap()
}

fn config() -> InertiaConfig {
    InertiaConfig::new().version(|| "v1".into())
}

fn home(cfg: InertiaConfig) -> Router {
    Router::new()
        .route(
            "/",
            get(|i: Inertia| async move { i.render("Home", json!({})) }),
        )
        .layer(InertiaLayer::new(cfg))
}

#[tokio::test]
async fn version_mismatch_409_echoes_the_version_and_keeps_the_flash() {
    let session = MockSession::default();
    session
        .store
        .lock()
        .await
        .bags
        .insert("success".into(), json!("Saved"));
    session
        .store
        .lock()
        .await
        .errors
        .insert("name".into(), vec!["bad".into()]);
    let app = home(config().session(session.clone()));

    let resp = app
        .oneshot(req_inertia("GET", "/?a=1", "stale"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    assert_eq!(resp.headers().get("x-inertia-location").unwrap(), "/?a=1");
    assert_eq!(resp.headers().get("x-inertia-version").unwrap(), "v1");
    assert!(resp.headers().get("x-inertia").is_none());
    let flash = session.store.lock().await;
    assert_eq!(flash.bags["success"], "Saved");
    assert_eq!(flash.errors["name"], ["bad"]);
}

#[tokio::test]
async fn plain_handler_responses_follow_the_protocol_too() {
    let session = MockSession::default();
    session
        .store
        .lock()
        .await
        .bags
        .insert("success".into(), json!("Saved"));
    let app = Router::new()
        .route("/text", get(|| async { "plain" }))
        .route("/empty", post(|| async {}))
        .route("/go", post(|| async { Redirect::to("/done") }))
        .route("/anchor", post(|| async { Redirect::to("/docs#install") }))
        .layer(InertiaLayer::new(config().session(session.clone())));

    let resp = app
        .clone()
        .oneshot(req_inertia("GET", "/text", "stale"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    assert_eq!(resp.headers().get("x-inertia-version").unwrap(), "v1");

    // A redirect does not use up the flash data.
    let resp = app
        .clone()
        .oneshot(req_inertia("POST", "/go", "v1"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 303);
    assert_eq!(session.store.lock().await.bags["success"], "Saved");

    let resp = app
        .clone()
        .oneshot(req_inertia("POST", "/anchor", "v1"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    assert_eq!(
        resp.headers().get("x-inertia-redirect").unwrap(),
        "/docs#install"
    );

    // An empty 200 is not a page: the client goes back.
    let r = with_headers("POST", "/empty", &[("referer", "/form")]);
    let resp = app.clone().oneshot(r).await.unwrap();
    assert_eq!(resp.status(), 303);
    assert_eq!(resp.headers().get("location").unwrap(), "/form");
    assert_eq!(
        app.clone()
            .oneshot(req("POST", "/empty"))
            .await
            .unwrap()
            .status(),
        200
    );

    // A browser keeps the fragment of a normal redirect.
    let resp = app.oneshot(req("POST", "/anchor")).await.unwrap();
    assert_eq!(resp.status(), 303);
}

#[tokio::test]
async fn fragment_redirect_returns_409_with_inertia_redirect() {
    let app = Router::new()
        .route(
            "/go",
            post(|i: Inertia| async move { i.redirect("/docs#install") }),
        )
        .layer(InertiaLayer::new(config()));

    let resp = app
        .clone()
        .oneshot(req_inertia("POST", "/go", "v1"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    assert_eq!(
        resp.headers().get("x-inertia-redirect").unwrap(),
        "/docs#install"
    );

    let prefetch = with_headers("POST", "/go", &[("purpose", "prefetch")]);
    assert_eq!(app.oneshot(prefetch).await.unwrap().status(), 303);
}

#[tokio::test]
async fn html_response_varies_on_x_inertia_and_escapes_the_page_json() {
    let app = Router::new()
        .route(
            "/",
            get(|i: Inertia| async move {
                i.render(
                    "Home",
                    json!({"html": "<!--<script></script>", "path": "/a"}),
                )
            }),
        )
        .layer(InertiaLayer::new(config()));
    let resp = app.oneshot(req("GET", "/")).await.unwrap();
    assert_eq!(resp.headers().get("vary").unwrap(), "X-Inertia");
    let html = body_string(resp).await;
    let start = html.find(r#"type="application/json">"#).unwrap() + 24;
    let json_text = &html[start..start + html[start..].find("</script>").unwrap()];
    assert!(!json_text.contains('<'), "{json_text}");
    let page: Value = serde_json::from_str(json_text).unwrap();
    assert_eq!(page["props"]["html"], "<!--<script></script>");
    assert_eq!(page["props"]["path"], "/a");
}

#[tokio::test]
async fn errors_default_to_empty_and_nest_under_the_error_bag() {
    let session = MockSession::default();
    let app = home(config().session(session.clone()));

    let p = page(
        app.clone()
            .oneshot(req_inertia("GET", "/", "v1"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(p["props"]["errors"], json!({}));

    session
        .store
        .lock()
        .await
        .errors
        .insert("email".into(), vec!["taken".into()]);
    let r = with_headers("GET", "/", &[("x-inertia-error-bag", "signup")]);
    let p = page(app.oneshot(r).await.unwrap()).await;
    assert_eq!(p["props"]["errors"], json!({"signup": {"email": "taken"}}));
}

#[tokio::test]
async fn with_all_errors_sends_each_message() {
    let session = MockSession::default();
    let messages = vec!["too short".to_string(), "no digit".to_string()];
    session
        .store
        .lock()
        .await
        .errors
        .insert("password".into(), messages.clone());
    let app = home(config().session(session.clone()));
    let p = page(app.oneshot(req_inertia("GET", "/", "v1")).await.unwrap()).await;
    assert_eq!(p["props"]["errors"], json!({"password": "too short"}));

    session
        .store
        .lock()
        .await
        .errors
        .insert("password".into(), messages);
    let app = home(config().session(session.clone()).with_all_errors(true));
    let p = page(app.oneshot(req_inertia("GET", "/", "v1")).await.unwrap()).await;
    assert_eq!(
        p["props"]["errors"],
        json!({"password": ["too short", "no digit"]})
    );
}

#[tokio::test]
async fn shared_props_are_listed_and_share_once_is_skipped_when_loaded() {
    let cfg = config()
        .shared(veer::shared::FnSharedProps(|_: &veer::RequestInfo| async {
            json!({"auth": {"user": "me"}})
        }))
        .share_once("countries", |_| async { json!(["NL"]) });
    let app = home(cfg);

    let p = page(
        app.clone()
            .oneshot(req_inertia("GET", "/", "v1"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(p["sharedProps"], json!(["auth", "countries", "errors"]));
    assert_eq!(p["props"]["countries"], json!(["NL"]));
    assert_eq!(
        p["onceProps"],
        json!({"countries": {"prop": "countries", "expiresAt": null}})
    );

    let r = with_headers("GET", "/", &[("x-inertia-except-once-props", "countries")]);
    let p = page(app.oneshot(r).await.unwrap()).await;
    assert!(p["props"].get("countries").is_none());
    assert!(p["onceProps"].get("countries").is_some());
}

#[tokio::test]
async fn flash_and_history_flags_travel_across_redirects() {
    let session = MockSession::default();
    let app = Router::new()
        .route(
            "/save",
            post(|i: Inertia| async move {
                i.redirect("/hop")
                    .with_flash("success", json!("Saved"))
                    .clear_history()
                    .preserve_fragment()
            }),
        )
        .route("/hop", get(|i: Inertia| async move { i.redirect("/") }))
        .route(
            "/",
            get(
                |i: Inertia| async move { i.render("Home", json!({})).with_flash("now", json!(1)) },
            ),
        )
        .layer(InertiaLayer::new(config().session(session.clone())));

    assert_eq!(
        app.clone()
            .oneshot(req_inertia("POST", "/save", "v1"))
            .await
            .unwrap()
            .status(),
        303
    );
    assert_eq!(
        app.clone()
            .oneshot(req_inertia("GET", "/hop", "v1"))
            .await
            .unwrap()
            .status(),
        303
    );
    let p = page(
        app.clone()
            .oneshot(req_inertia("GET", "/", "v1"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(p["flash"], json!({"success": "Saved", "now": 1}));
    assert_eq!(p["clearHistory"], true);
    assert_eq!(p["preserveFragment"], true);

    // One-shot: the next page has only the data that its handler flashes.
    let p = page(app.oneshot(req_inertia("GET", "/", "v1")).await.unwrap()).await;
    assert_eq!(p["flash"], json!({"now": 1}));
    assert!(p.get("clearHistory").is_none());
}

#[tokio::test]
async fn big_integers_become_markers_when_enabled() {
    let app = Router::new()
        .route(
            "/",
            get(|i: Inertia| async move { i.render("Home", json!({"id": u64::MAX, "n": 1})) }),
        )
        .route(
            "/off",
            get(|i: Inertia| async move {
                i.render("Home", json!({"id": u64::MAX}))
                    .preserve_big_integers(false)
            }),
        )
        .layer(InertiaLayer::new(config().preserve_big_integers(true)));

    let p = page(
        app.clone()
            .oneshot(req_inertia("GET", "/", "v1"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(p["preserveBigIntegers"], true);
    assert_eq!(p["props"]["id"], json!({"$bigint": "18446744073709551615"}));
    assert_eq!(p["props"]["n"], 1);

    let p = page(app.oneshot(req_inertia("GET", "/off", "v1")).await.unwrap()).await;
    assert!(p.get("preserveBigIntegers").is_none());
    assert_eq!(p["props"]["id"], json!(u64::MAX));
}

#[tokio::test]
async fn infinite_scroll_and_rescued_props_on_the_wire() {
    let app = Router::new()
        .route(
            "/posts",
            get(|i: Inertia| async move {
                i.render("Posts", json!({}))
                    .prop(
                        "posts",
                        Prop::scroll(|| async {
                            (json!({"data": [1]}), ScrollMetadata::paged("page", 1, true))
                        }),
                    )
                    .prop(
                        "perms",
                        Prop::try_new(|| async { Err::<Value, _>("db down") })
                            .defer()
                            .rescue(),
                    )
            }),
        )
        .layer(InertiaLayer::new(config().encrypt_history(true)));

    let p = page(
        app.clone()
            .oneshot(req_inertia("GET", "/posts", "v1"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(p["encryptHistory"], true);
    assert_eq!(p["mergeProps"], json!(["posts.data"]));
    assert_eq!(p["scrollProps"]["posts"]["nextPage"], 2);
    assert_eq!(p["deferredProps"], json!({"default": ["perms"]}));

    let r = with_headers(
        "GET",
        "/posts",
        &[
            ("x-inertia-partial-component", "Posts"),
            ("x-inertia-partial-data", "perms,posts"),
            ("x-inertia-infinite-scroll-merge-intent", "prepend"),
        ],
    );
    let p = page(app.oneshot(r).await.unwrap()).await;
    assert_eq!(p["rescuedProps"], json!(["perms"]));
    assert!(p["props"].get("perms").is_none());
    assert_eq!(p["prependProps"], json!(["posts.data"]));
}

#[tokio::test]
async fn failed_prop_without_rescue_is_a_500() {
    let app = Router::new()
        .route(
            "/",
            get(|i: Inertia| async move {
                i.render("Home", json!({}))
                    .prop("x", Prop::try_new(|| async { Err::<Value, _>("db down") }))
            }),
        )
        .layer(InertiaLayer::new(config()));
    let resp = app.oneshot(req_inertia("GET", "/", "v1")).await.unwrap();
    assert_eq!(resp.status(), 500);
    assert!(!body_string(resp).await.contains("db down"));
}

#[tokio::test]
async fn errors_on_a_render_show_on_that_page_and_flash_survives_plain_responses() {
    let session = MockSession::default();
    let app = Router::new()
        .route(
            "/form",
            post(|i: Inertia| async move {
                i.render("Form", json!({}))
                    .with_errors(vec![("name", "is required")])
            }),
        )
        .route("/api", get(|| async { "plain" }))
        .route(
            "/",
            get(|i: Inertia| async move { i.render("Home", json!({})) }),
        )
        .layer(InertiaLayer::new(config().session(session.clone())));

    let p = page(
        app.clone()
            .oneshot(req_inertia("POST", "/form", "v1"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(p["props"]["errors"], json!({"name": "is required"}));
    assert!(session.store.lock().await.is_empty());

    // A plain response between a redirect and its page does not use up the flash.
    session
        .store
        .lock()
        .await
        .bags
        .insert("success".into(), json!("Saved"));
    app.clone()
        .oneshot(req_inertia("GET", "/api", "v1"))
        .await
        .unwrap();
    let p = page(app.oneshot(req_inertia("GET", "/", "v1")).await.unwrap()).await;
    assert_eq!(p["flash"], json!({"success": "Saved"}));
}

#[tokio::test]
async fn precognition_answers_204_or_422() {
    let app = Router::new()
        .route(
            "/users",
            post(|i: Inertia| async move {
                let p = i.precognition().expect("precognition request");
                p.respond(Err(vec![("email", "is taken"), ("name", "is required")]))
            }),
        )
        .layer(InertiaLayer::new(config()));

    let r = with_headers(
        "POST",
        "/users",
        &[
            ("precognition", "true"),
            ("precognition-validate-only", "email"),
        ],
    );
    let resp = app.clone().oneshot(r).await.unwrap();
    assert_eq!(resp.status(), 422);
    assert_eq!(resp.headers().get("precognition").unwrap(), "true");
    let body: Value = serde_json::from_str(&body_string(resp).await).unwrap();
    assert_eq!(
        body,
        json!({"message": "is taken", "errors": {"email": ["is taken"]}})
    );

    let r = with_headers(
        "POST",
        "/users",
        &[
            ("precognition", "true"),
            ("precognition-validate-only", "phone"),
        ],
    );
    let resp = app.oneshot(r).await.unwrap();
    assert_eq!(resp.status(), 204);
    assert_eq!(resp.headers().get("precognition-success").unwrap(), "true");
}

#[tokio::test]
async fn back_uses_the_stored_previous_url_when_there_is_no_referer() {
    #[derive(serde::Serialize)]
    struct Plan {
        name: &'static str,
    }
    let session = MockSession::default();
    let app = Router::new()
        .route(
            "/users",
            get(|i: Inertia| async move {
                // A closure prop returns a typed value; no `json!` is necessary.
                i.render("Users", json!({}))
                    .once("plans", || async { vec![Plan { name: "Pro" }] })
            })
            .post(|i: Inertia| async move { i.back() }),
        )
        .layer(InertiaLayer::new(
            config().session(session.clone()).store_previous_url(true),
        ));

    let p = page(
        app.clone()
            .oneshot(req_inertia("GET", "/users?page=2", "v1"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(p["props"]["plans"], json!([{"name": "Pro"}]));

    // A partial reload and a prefetch do not change the stored URL.
    let partial = with_headers(
        "GET",
        "/users?page=3",
        &[("x-inertia-partial-component", "Users")],
    );
    app.clone().oneshot(partial).await.unwrap();
    let prefetch = with_headers("GET", "/users?page=4", &[("purpose", "prefetch")]);
    app.clone().oneshot(prefetch).await.unwrap();

    let resp = app
        .clone()
        .oneshot(req_inertia("POST", "/users", "v1"))
        .await
        .unwrap();
    assert_eq!(resp.headers().get("location").unwrap(), "/users?page=2");
    // The URL is kept for later requests.
    let resp = app
        .oneshot(req_inertia("POST", "/users", "v1"))
        .await
        .unwrap();
    assert_eq!(resp.headers().get("location").unwrap(), "/users?page=2");
}

#[tokio::test]
async fn precognition_request_with_a_rejected_body_gets_a_precognition_response() {
    #[derive(serde::Deserialize)]
    struct NewUser {
        #[allow(dead_code)]
        name: String,
    }
    let app = Router::new()
        .route(
            "/users",
            post(|_: Inertia, _: veer::InertiaForm<NewUser>| async { "created" }),
        )
        .layer(InertiaLayer::new(config()));
    let r = http::Request::builder()
        .method("POST")
        .uri("/users")
        .header("precognition", "true")
        .header("content-type", "application/json")
        .body(axum::body::Body::from("{}"))
        .unwrap();
    let resp = app.oneshot(r).await.unwrap();
    assert_eq!(resp.status(), 422);
    assert_eq!(resp.headers().get("precognition").unwrap(), "true");
    let body: Value = serde_json::from_str(&body_string(resp).await).unwrap();
    assert_eq!(body["errors"], json!({}));
}

#[cfg(feature = "devtools")]
#[tokio::test]
async fn devtools_records_entries_and_serves_the_read_api() {
    let app = Router::new()
        .route(
            "/users/{id}",
            get(|i: Inertia| async move {
                i.render("Users/Show", json!({"name": "A", "token": "t"}))
                    .deferred("stats", "side", || async { json!(1) })
            })
            .post(|i: Inertia| async move { i.redirect("/users/1") }),
        )
        .layer(InertiaLayer::new(config().devtools(veer::DevTools::new())));

    // First page load: the id is in a header and in a script tag.
    let resp = app.clone().oneshot(req("GET", "/users/1")).await.unwrap();
    let id = resp.headers()["x-inertia-devtools-id"]
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(resp.headers()["x-inertia-devtools-parent-out"], id.as_str());
    let html = body_string(resp).await;
    assert!(html.contains(&format!(
        r#"<script data-inertia-devtools-id type="application/json">"{id}"</script></body>"#
    )));

    let read = |path: String| {
        let app = app.clone();
        async move {
            let resp = app.oneshot(req("GET", &path)).await.unwrap();
            let status = resp.status();
            let body: Value = serde_json::from_str(&body_string(resp).await).unwrap();
            (status, body)
        }
    };
    let (status, entry) = read(format!("/_inertia/devtools/entries/{id}")).await;
    assert_eq!(status, 200);
    let meta = &entry["__meta"];
    assert_eq!(meta["requestType"], "initial");
    assert_eq!(meta["component"], "Users/Show");
    assert_eq!(meta["url"], "http://localhost/users/1");
    assert_eq!(entry["route"]["uri"], "/users/{id}");
    assert!(entry["renderSource"]["file"]
        .as_str()
        .unwrap()
        .ends_with("v3_protocol.rs"));
    assert_eq!(entry["propValues"]["token"], "[REDACTED]");
    assert_eq!(entry["props"]["stats"]["deferGroup"], "side");
    assert_eq!(
        entry["http"]["responseBody"]["value"]["component"],
        "Users/Show"
    );

    // An Inertia POST: the JSON body reaches the handler and is recorded.
    let r = http::Request::builder()
        .method("POST")
        .uri("/users/1")
        .header("x-inertia", "true")
        .header("content-type", "application/json")
        .header("content-length", "29")
        .header("x-inertia-devtools-parent", "root")
        .header("x-inertia-devtools-tab", "tab-1")
        .body(axum::body::Body::from(r#"{"name":"B","password":"pw1"}"#))
        .unwrap();
    let resp = app.clone().oneshot(r).await.unwrap();
    assert_eq!(resp.status(), 303);
    assert_eq!(resp.headers()["x-inertia-devtools-parent-out"], "root");

    let (_, entries) = read("/_inertia/devtools/entries".into()).await;
    assert_eq!(entries.as_array().unwrap().len(), 2);
    let post = &entries[0];
    assert_eq!(post["__meta"]["requestType"], "navigate");
    assert_eq!(post["__meta"]["redirectLocation"], "/users/1");
    assert_eq!(post["__meta"]["tabUuid"], "tab-1");
    assert_eq!(
        post["http"]["requestBody"],
        json!({"status": "present", "value": {"name": "B", "password": "[REDACTED]"}})
    );
    assert_eq!(read("/_inertia/devtools/entries/nope".into()).await.0, 404);

    // Without the recorder there are no DevTools headers and no read API.
    let resp = home(config()).oneshot(req("GET", "/")).await.unwrap();
    assert!(resp.headers().get("x-inertia-devtools-id").is_none());
}
