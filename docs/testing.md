# Testing

The `testing` feature has helpers for tests of your handlers. Enable it for tests only:

```toml
[dev-dependencies]
veer = { version = "0.2", features = ["testing"] }
tower = { version = "0.5", features = ["util"] }
```

## A page test

Build your app as in `main`, send a request with `tower::ServiceExt::oneshot`, and read the page:

```rust,ignore
use tower::ServiceExt;
use veer::testing::{visit, TestPage};

#[tokio::test]
async fn users_index_lists_the_users() {
    let response = app().oneshot(visit("GET", "/users")).await.unwrap();

    let page = TestPage::from_response(response).await;
    assert_eq!(page.component, "Users/Index");
    assert_eq!(page.prop("users.0.name"), Some(&json!("Ada")));
    assert_eq!(page.prop("auth.user"), Some(&json!(null)));
}
```

- **`visit(method, uri)`** makes an Inertia request (`X-Inertia: true`) with an empty body. It sends the asset version `"1"`, which is the default. If your test config has a different version, replace the `X-Inertia-Version` header.
- **`TestPage::from_response`** reads the page object from a JSON response or from the HTML of a first page load. It panics with the status and the body if the response is not a page.
- **`page.prop(path)`** reads a prop at a dot path; a number is an array index. `page.props` and `page.raw` are the complete JSON values.

## Forms, errors and flash data

`MemorySession` is a session store in memory. Its clones share the data, so that you can follow a redirect or look at the data that the next request will get:

```rust,ignore
use veer::testing::MemorySession;

#[tokio::test]
async fn store_rejects_a_short_title() {
    let session = MemorySession::default();
    let app = app(InertiaConfig::new().session(session.clone()));

    let request = Request::post("/todos")
        .header("x-inertia", "true")
        .header("content-type", "application/json")
        .header("referer", "/todos/new")
        .body(Body::from(json!({ "title": "a" }).to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()["location"], "/todos/new");
    assert!(session.flash().errors.contains_key("title"));

    // The page after the redirect has the error.
    let page = TestPage::from_response(app.oneshot(visit("GET", "/todos/new")).await.unwrap()).await;
    assert!(page.prop("errors.title").is_some());
}
```
