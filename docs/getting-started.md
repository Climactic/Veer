# Getting started

## Install

```toml
[dependencies]
veer = "0.3"
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

The default feature set has the axum adapter. Other features are opt-in; see the [feature flag table](architecture.md#feature-flags).

Minimum supported Rust version: **1.88**.

## A first page

```rust,no_run
use axum::{routing::get, Router};
use veer::{Inertia, InertiaConfig, InertiaLayer, MinimalRootView};

#[tokio::main]
async fn main() {
    let cfg = InertiaConfig::new()
        .root_view(
            MinimalRootView::new()
                .title("Acme")
                .vite_entry("/src/main.tsx"),
        );

    let app = Router::new()
        .route("/", get(|inertia: Inertia| async move {
            inertia.render("Home", serde_json::json!({ "msg": "hello" }))
        }))
        .layer(InertiaLayer::new(cfg));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

There are three parts:

- **`InertiaConfig`** is built once at startup. It holds the root view (the HTML shell), and optional parts: a session store, an SSR client, shared props.
- **`Inertia`** is an axum extractor. `inertia.render(component, props)` returns an `InertiaResponse` builder. `props` is any `Serialize` value: a struct or `serde_json::json!`.
- **`InertiaLayer`** does the protocol work after your handler returns.

## What the layer does

| Request | Response |
|---|---|
| First page load (no `X-Inertia` header) | The HTML shell, with the page object in a `<script data-page="app" type="application/json">` tag |
| Inertia visit (`X-Inertia: true`) | The page object as JSON, with `X-Inertia: true` |
| Inertia `GET` with an old asset version | `409` + `X-Inertia-Location` + `X-Inertia-Version`; the client reloads |
| `inertia.redirect(...)` | `303 See Other` |
| A plain `302` from a `POST` / `PUT` / `PATCH` / `DELETE` handler | Changed to `303` |

Every response gets `Vary: X-Inertia`, because HTML and JSON share a URL.

## Typed props

Use a struct in place of `json!`:

```rust,ignore
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UsersIndexProps {
    users: Vec<User>,
    can_create: bool,
}

async fn users_index(inertia: Inertia) -> impl IntoResponse {
    inertia.render("Users/Index", UsersIndexProps { users: load_users().await, can_create: true })
}
```

The same struct can generate the TypeScript type of the page; see [TypeScript bindings](typescript.md).

## The frontend entry point

Veer needs nothing special on the client. Use the official adapter:

```tsx
// src/main.tsx
import { createInertiaApp } from "@inertiajs/react";
import { createRoot } from "react-dom/client";

createInertiaApp({
  resolve: (name) => {
    const pages = import.meta.glob("./pages/**/*.tsx", { eager: true });
    return pages[`./pages/${name}.tsx`];
  },
  setup({ el, App, props }) {
    createRoot(el).render(<App {...props} />);
  },
});
```

`MinimalRootView` is sufficient to start. For a real app, use `ViteRootView`, which handles the Vite dev server and the production manifest; see [Vite, SSR and assets](vite-ssr-assets.md).

## Asset versioning

The asset version is a string that changes when your assets change. When a client sends an old version, veer answers `409` and the client does a full reload.

With `ViteRootView` in production mode, the version is the hash of the Vite manifest; you set nothing. Without it, the version is `"1"`. Set your own with `InertiaConfig::version_str("…")`, or with `InertiaConfig::version(|| …)` for a value that changes while the server runs.

## When something is wrong

Veer tells you about a wrong setup. In a debug build, the cause is in the response body; it is always in the log (`tracing`).

| Symptom | Cause |
|---|---|
| `500`: "not inside `InertiaLayer`" | A handler uses the `Inertia` extractor, but its route has no `InertiaLayer` |
| `500`: "the props of … did not serialize" | The props value is not valid JSON, for example a map with keys that are not strings |
| Warning: "there is no session store" | A redirect has errors or flash data, but the config has no [session store](sessions.md) |

## Next steps

- [Props](props.md): load data only when the page needs it.
- [Forms and validation](forms-and-validation.md): handle a form submit with errors and a flash message.
- [Error pages](error-pages.md) and [Testing](testing.md).
- [`examples/axum-react-todo`](../examples/axum-react-todo): a complete app.
