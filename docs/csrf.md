# CSRF protection

The Inertia client reads an `XSRF-TOKEN` cookie and sends its value in an `X-XSRF-TOKEN` header on each request that changes data. `CsrfLayer` is the server side of that convention. It needs no session: the token is a signed double-submit token (HMAC-SHA256).

Enable the `csrf` feature and add the layer outside `InertiaLayer`:

```toml
veer = { version = "0.2", features = ["csrf"] }
```

```rust,ignore
use veer::{CsrfLayer, InertiaLayer};

let app = router()
    .with_state(state)
    .layer(InertiaLayer::new(cfg))
    .layer(CsrfLayer::new(secret));   // at least 32 bytes; the outermost layer
```

## Behavior

- `GET`, `HEAD`, `OPTIONS` and `TRACE` are not checked. The layer issues the cookie.
- Other methods need a header that agrees with the cookie. If not, the layer answers `419` and the handler does not run. `419` is the status that the Inertia client knows for an expired token.
- The cookie is readable from JavaScript, because the client must copy it into the header. It is `Secure` and `SameSite=Lax`.

## Options

| Method | Effect |
|---|---|
| `.secure(false)` | No `Secure` flag; for local HTTP development |
| `.same_site(SameSite::Strict)` | Change the `SameSite` attribute |
| `.cookie_name("…")` / `.header_name("…")` | Change the names; set the same names on the client |
| `.exclude("/webhooks")` | No check for this path prefix (for requests that cannot send the header) |

`CsrfTokens` is the core without a framework (`generate`, `verify`), for use outside axum.
