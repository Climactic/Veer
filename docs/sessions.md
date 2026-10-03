# Sessions

Veer uses a session store for data that must reach the next request: validation errors, flash data, and the previous URL. Without a store, `with_errors` and `with_flash` on a redirect have no effect.

## Cookie store

For an app with no session crate. Enable the `cookie-session` feature:

```rust,ignore
use veer::session::cookie::CookieSessionStore;

let cfg = InertiaConfig::new()
    .session(CookieSessionStore::new(secret)); // at least 32 bytes
```

The data is in one cookie (`_veer_flash`), signed with HMAC-SHA256 and verified in constant time. The cookie is `HttpOnly`, `Secure` and `SameSite=Lax`, and lives for 60 seconds.

- For local HTTP development, call `.secure(false)`.
- A cookie holds about 4 KB. For large flash data, use `tower-sessions`.

## tower-sessions

If the app already uses [`tower-sessions`](https://crates.io/crates/tower-sessions), enable the `tower-sessions` feature. The data then lives in the backend that you configured (memory, Redis, Postgres, …).

```rust,ignore
use time::Duration;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};
use veer::{session::tower::TowerSessionStore, InertiaConfig, InertiaLayer};

let session_layer = SessionManagerLayer::new(MemoryStore::default())
    .with_expiry(Expiry::OnInactivity(Duration::minutes(30)));

let cfg = InertiaConfig::new().session(TowerSessionStore::new());

let app = axum::Router::new()
    // … routes …
    .layer(InertiaLayer::new(cfg))
    .layer(session_layer); // must wrap outside InertiaLayer
```

The data is under one session key (`_veer_flash`; change it with `TowerSessionStore::new().key("…")`).

## A custom store

Implement `SessionStore` over the session crate that you use:

```rust,ignore
use async_trait::async_trait;
use http::{request::Parts, Extensions, HeaderMap};
use veer::{Flash, SessionStore};

struct MyStore;

#[async_trait]
impl SessionStore for MyStore {
    async fn read_and_clear(&self, req: &Parts) -> Flash {
        // Load the Flash for this request, or Flash::default().
    }

    async fn write(&self, headers: &mut HeaderMap, req_extensions: &Extensions, flash: Flash) {
        // Save the Flash for the next request. An empty Flash means "clear".
    }
}
```

`Flash` implements `Serialize` and `Deserialize`, so you can store it as JSON. It is `#[non_exhaustive]`; start from `Flash::default()`. `write` gets the response headers (for a cookie) and a copy of the request extensions (for a session handle that a middleware put there).

## How the data flows

- A **redirect** writes its errors and flash data, plus the flash data that the request read and did not use. A chain of redirects thus keeps a flash message.
- A **page render** uses the data: errors go to `props.errors`, flash data to the page's `flash`.
- **Other responses** (a JSON endpoint, a `404`, a Precognition response) pass the data on unchanged.
