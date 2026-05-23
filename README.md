<div align="center">

<img src=".github/veer.webp" alt="Veer" width="100%" />

# Veer

**The Inertia.js server-side protocol superset, for Rust.**

Build modern single-page apps in React, Vue, or Svelte — without writing a JSON API, a client-side router, or a single `fetch`. Your Rust handlers return typed props, and the frontend hydrates as if the page was server-rendered. Because it was.

[![Discord](https://img.shields.io/badge/Discord-Join%20Us-5865F2?style=for-the-badge&logo=discord&logoColor=white)](http://go.climactic.co/discord)
[![Latest Version on crates.io](https://img.shields.io/crates/v/veer.svg?style=for-the-badge)](https://crates.io/crates/veer)
[![GitHub CI Status](https://img.shields.io/github/actions/workflow/status/climactic/veer/ci.yml?branch=main&label=ci&style=for-the-badge)](https://github.com/climactic/veer/actions?query=workflow%3Aci+branch%3Amain)
[![docs.rs](https://img.shields.io/docsrs/veer?style=for-the-badge)](https://docs.rs/veer)
[![MSRV 1.75](https://img.shields.io/badge/MSRV-1.75-blue?style=for-the-badge)](https://www.rust-lang.org)
[![Sponsor on GitHub](https://img.shields.io/badge/Sponsor-GitHub-ea4aaa?style=for-the-badge&logo=github)](https://github.com/sponsors/climactic)
[![Support on Ko-fi](https://img.shields.io/badge/Support-Ko--fi-FF5E5B?style=for-the-badge&logo=ko-fi&logoColor=white)](https://ko-fi.com/ClimacticCo)

</div>

## 📖 Table of Contents

- ✨ [What is Inertia, and why a Rust adapter](#-what-is-inertia-and-why-a-rust-adapter)
- 📦 [Installation](#-installation)
- 🚀 [Quick Start](#-quick-start)
- 🍳 [Cookbook](#-cookbook)
- 🎛️ [Feature Flags](#️-feature-flags)
- 🏗️ [Architecture](#️-architecture)
- ⚠️ [Caveats](#️-caveats)
- 🧪 [Example App](#-example-app)
- 🗺️ [Status & Roadmap](#️-status--roadmap)
- 🙌 [Acknowledgements](#-acknowledgements)
- 🧪 [Testing](#-testing)
- 📋 [Changelog](#-changelog)
- 🤝 [Contributing](#-contributing)
- 🔒 [Security Vulnerabilities](#-security-vulnerabilities)
- 💖 [Support This Project](#-support-this-project)
- ⭐ [Star History](#-star-history)
- 📄 [License](#-license)

## ✨ What is Inertia, and why a Rust adapter

[Inertia.js](https://inertiajs.com) is a glue layer that lets a classic server-rendered backend drive a modern SPA frontend. The server returns a page object (component name + props); the official Inertia client adapter for React/Vue/Svelte takes care of mounting the component, hydrating props, intercepting links, and making subsequent navigations into JSON XHRs.

`veer` is a clean-room Rust implementation of the server side of the [Inertia v3 protocol](https://inertiajs.com/the-protocol). It targets [axum](https://github.com/tokio-rs/axum) out of the box; the protocol core is framework-agnostic, so adapters for other Rust web frameworks slot in beside it.

```text
   ┌─────────────────────────┐                       ┌─────────────────────────┐
   │  Rust handler           │  ── page object ──▶   │  Inertia client (JS)    │
   │  inertia.render(...)    │       (JSON)          │  React / Vue / Svelte   │
   └─────────────────────────┘                       └─────────────────────────┘
              ▲                                                  │
              └─────────────  navigation XHR  ───────────────────┘
```

**Highlights**

- 🦀 Pure Rust server-side implementation of Inertia v3
- ⚡ First-class [axum](https://github.com/tokio-rs/axum) adapter (extractor + tower layer)
- 🧩 Framework-agnostic protocol core — drop new adapters in beside the axum one
- 📦 Partial reloads, deferred props, merge props, encrypted/clear history
- 🖥️ SSR via `@inertiajs/server` (Node or Bun) with graceful fallback
- ⚙️ Vite dev + production manifest integration that mirrors Laravel's `@vite`
- 📤 File uploads via typed `InertiaForm` + a streaming `MultipartStream` extractor
- ✅ Validation flash for `validator` and `garde`, plus a cookie-based session store
- 🪢 End-to-end TypeScript bindings: typed page props + Ziggy-style route helpers, generated from Rust

## 📦 Installation

Add `veer` to your `Cargo.toml`:

```toml
[dependencies]
veer = "0.1"
```

Or with `cargo add`:

```bash
cargo add veer
```

The default feature set includes the axum adapter. See [Feature flags](#️-feature-flags) for everything else.

## 🚀 Quick Start

```rust,no_run
use axum::{routing::get, Router};
use veer::{Inertia, InertiaConfig, InertiaLayer, MinimalRootView};

#[tokio::main]
async fn main() {
    let cfg = InertiaConfig::new()
        .version(|| "1".into())
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

The `Inertia` extractor reads the request. `render(component, props)` returns a builder. The layer handles the rest of the protocol — initial HTML on first load, JSON on XHR navigations, 409 for asset-version mismatches, 303 for POST-redirects, version checks, partial reloads.

## 🍳 Cookbook

<details>
<summary><b>Validation + flash, Laravel-style</b></summary>

```rust,ignore
async fn users_create(
    inertia: Inertia,
    InertiaForm(body): InertiaForm<NewUser>,
) -> impl IntoResponse {
    if let Err(errors) = body.validate() {
        return inertia.with_errors(errors).redirect("/users/new");
    }
    create_user(body).await;
    inertia
        .redirect("/users")
        .with_flash("success", serde_json::json!("User created"))
}
```

`InertiaForm` accepts `application/json`, `application/x-www-form-urlencoded`, and (with the `multipart` feature) `multipart/form-data`. The same handler works regardless of how the Inertia client serialized the request.

On the frontend, `usePage().props.errors` and `usePage().props.flash` are auto-populated whenever a `SessionStore` is configured — no extra wiring.

</details>

<details>
<summary><b>File uploads</b></summary>

Enable the `multipart` feature and add `UploadedFile` fields to your typed struct:

```rust,ignore
use veer::{InertiaForm, UploadedFile};

#[derive(serde::Deserialize)]
struct CreateAvatar {
    user_id: String,
    avatar: UploadedFile,
}

async fn upload_avatar(
    inertia: Inertia,
    InertiaForm(form): InertiaForm<CreateAvatar>,
) -> impl IntoResponse {
    save_avatar(&form.user_id, &form.avatar.bytes, form.avatar.filename.as_deref()).await;
    inertia.redirect("/profile").with_flash("success", json!("Avatar updated"))
}
```

Inertia's `useForm` automatically switches to `multipart/form-data` when any field is a `File` or `Blob`. The same handler accepts JSON when no file is attached and multipart when one is — no separate route required.

For large or unbounded uploads where in-memory buffering isn't acceptable, use `MultipartStream` instead:

```rust,ignore
use veer::MultipartStream;

async fn huge_upload(MultipartStream(mut m): MultipartStream) -> impl IntoResponse {
    while let Some(field) = m.next_field().await.unwrap() {
        // pipe field.chunk().await into S3, disk, wherever — no buffering
    }
    // ...
}
```

</details>

<details>
<summary><b>External redirect (OAuth, billing, etc.)</b></summary>

```rust,ignore
async fn oauth_start(inertia: Inertia) -> impl IntoResponse {
    inertia.location("https://accounts.google.com/o/oauth2/v2/auth?...")
}
```

Returns 409 + `X-Inertia-Location`. The Inertia client honors this with a hard navigation.

</details>

<details>
<summary><b>Partial reloads, lazy & deferred props</b></summary>

```rust,ignore
inertia
    .render("Users/Index", serde_json::json!({ "users": users }))
    .lazy("stats", || async { json!({ "hits": load_stats().await }) })
    .deferred("expensive", "dashboard", || async { json!(load_expensive().await) })
    .merge("notifications")
```

| Method | Behavior |
|---|---|
| `lazy(key, closure)` (alias `optional`) | Closure only runs when the client requests this key via a partial reload |
| `deferred(key, group, closure)` | First response advertises the key under `deferredProps[group]`; client then issues a follow-up reload that resolves it |
| `merge(key)` | Marks the key so the client merges the value into existing state instead of replacing |
| `encrypt_history()` / `clear_history()` | Set the Inertia v2+ history-state primitives |
| `no_ssr()` | Skip SSR for this response only |

</details>

<details>
<summary><b>Shared props (auth, app name, feature flags)</b></summary>

```rust,ignore
use veer::shared::shared_props_fn;

let cfg = InertiaConfig::new()
    .shared(shared_props_fn(|_req| async move {
        serde_json::json!({
            "auth": { "user": current_user().await },
            "app": { "name": "Acme" },
        })
    }));
```

Shared props merge under per-response props (handler props win on key collision).

</details>

<details>
<summary><b>SSR via @inertiajs/server (Node or Bun)</b></summary>

Enable the `ssr` feature and point at the SSR service:

```rust,ignore
use veer::ssr::http::HttpSsrClient;

let cfg = InertiaConfig::new()
    .ssr(HttpSsrClient::new("http://127.0.0.1:13714/render"));
```

SSR failures fall back to client-side rendering by default. Set `ssr_required(true)` for hard-fail behavior.

For end-to-end SSR you'll usually combine this with `ViteRootView` (next entry) — `ViteRootView` inlines the SSR body verbatim and emits the `<script data-page>` mount the Inertia v3 client expects.

</details>

<details>
<summary><b>Vite integration (dev + production)</b></summary>

`ViteRootView` is a drop-in `RootView` that mirrors what Laravel's `@vite` + `@viteReactRefresh` Blade directives do. Two modes:

**Dev** — cross-origin script tags pointing at the Vite dev server, plus (opt-in) the React refresh preamble required when the HTML shell is served off-origin:

```rust,ignore
use veer::ViteRootView;

let cfg = InertiaConfig::new()
    .root_view(
        ViteRootView::dev()
            .title("Acme")
            .entry("frontend/app.tsx")
            .dev_server("http://localhost:5173")
            .react_refresh(true), // needed for @vitejs/plugin-react cross-origin
    );
```

**Production** — `vite build` writes `dist/.vite/manifest.json`. `ViteRootView::production` walks it from the entry, emitting the entry script, its CSS, and `<link rel="modulepreload">` hints for transitively imported chunks:

```rust,ignore
use veer::{ViteManifest, ViteRootView};

let manifest = ViteManifest::load("dist/.vite/manifest.json")?;
let version  = manifest.hash(); // changes whenever any chunk hash changes

let cfg = InertiaConfig::new()
    .version(move || version.clone().into()) // bumps client asset cache on rebuild
    .root_view(
        ViteRootView::production()
            .title("Acme")
            .entry("frontend/app.tsx")
            .manifest(manifest)
            .asset_base("/build"),
    );

// Serve the built bundle next to the routes:
// .nest_service("/build", tower_http::services::ServeDir::new("dist"))
```

Wiring `version` to `manifest.hash()` means any rebuild auto-invalidates clients via the Inertia 409 + force-reload protocol — no manual version bumps.

For SSR in production, build the sidecar with `vite build --ssr frontend/ssr.tsx` and run the resulting bundle (`bun dist/ssr/ssr.js` or `node …`). Same `:13714/render` interface as dev.

</details>

<details>
<summary><b>End-to-end TypeScript bindings (Wayfinder-style)</b></summary>

Enable the `ts` feature and the frontend gets an auto-generated `gen/` directory containing every page's props type, a discriminated `Pages` union, and one TypeScript module per controller with method-aware URL builders.

```toml
[dependencies]
veer = { version = "0.1", features = ["ts"] }
ts-rs = "10"
```

Annotate each prop struct and register it as a page:

```rust,ignore
use serde::Serialize;
use ts_rs::TS;

#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UsersIndexProps {
    pub users: Vec<User>,
}
veer::register_page!(UsersIndexProps, "Users/Index");
```

Then build your router using `veer::Router` — same fluent API as `axum::Router`, but every route gets a name and method so the codegen knows about it:

```rust,ignore
use veer::Method::*;

pub fn router() -> veer::Router<AppState> {
    veer::Router::new()
        .named_route(GET,    "users.index",   "/users",      users_index)
        .named_route(GET,    "users.show",    "/users/:id",  users_show)
        .named_route(GET,    "users.create",  "/users/new",  users_create)
        .named_route(POST,   "users.store",   "/users",      users_store)
        .named_route(PATCH,  "users.update",  "/users/:id",  users_update)
        .named_route(DELETE, "users.destroy", "/users/:id",  users_destroy)
}

// In main():
let app = router().build().with_state(state).layer(InertiaLayer::new(cfg));
```

`build()` returns a regular `axum::Router`, so `.with_state` / `.layer` / `.merge` / anything else just works. Same-path multi-method calls (GET + POST on `/users`) are merged into a single `MethodRouter` automatically — no panic on duplicate paths.

`ts-rs` mirrors `#[serde(rename_all)]` into the generated TypeScript, so the same struct drives both wire format and type. Route names follow the Laravel resource convention (`index`/`show`/`create`/`store`/`update`/`destroy`) so they don't collide with JavaScript reserved words.

Add a tiny binary that builds the router (so the runtime route registry populates) and then emits the bundle. Drop this in `src/bin/gen-bindings.rs` inside your app crate — `cargo` auto-discovers it:

```rust,ignore
// src/bin/gen-bindings.rs
fn main() {
    let _ = my_app::router().build();   // populate registry
    veer::bindings::generate_split("./frontend/gen").unwrap();
}
```

Generate with:

```bash
cargo run --bin gen-bindings
```

The codegen has to run inside *your* app binary (not a standalone CLI), because the route registry is populated at runtime by `Router::build()` and the page registrations are link-time `inventory` submissions — both only exist in your compiled crate.

On the frontend, import types and the per-controller action module:

```tsx,ignore
import { usePage, router, Link } from "@inertiajs/react";
import { users, type UsersIndexProps } from "./gen";

export default function Index() {
  const { props } = usePage<UsersIndexProps>();
  return (
    <>
      <Link href={users.create.url()}>New user</Link>
      {props.users.map((u) => (
        <Link key={u.id} href={users.show.url({ id: u.id })}>{u.name}</Link>
      ))}
    </>
  );
}
```

Each action is a callable with `.url` and `.form` helpers:

| Call | Returns |
|---|---|
| `users.show({id: 1})` | `{ url: "/users/1", method: "get" } as const` (visit definition) |
| `users.show.url({id: 1})` | `"/users/1"` (just the path string) |
| `users.show.form({id: 1})` | `{ action: "/users/1", method: "get" } as const` (props for `<form>`) |

Generated layout:

```text
frontend/gen/
  index.ts              protocol types, Pages, prop types, action namespace re-exports
  actions/
    users.ts            export const index, show, create, store, update, destroy
    posts.ts            …
    _root.ts            routes without a dotted prefix
```

`Always<T>` and `Merge<T>` collapse to `T` on the TypeScript side — the protocol's `mergeProps` / `deferredProps` arrays in the envelope carry the marker information instead.

**Customize the output layout** with the `Split` builder — rename the `actions/` subdirectory, add a filename prefix/suffix, or flatten everything into the root:

```rust,ignore
veer::bindings::Split::new("./frontend/gen")
    .actions_dir("controllers")    // -> frontend/gen/controllers/
    .file_suffix("-controller")    // -> users-controller.ts, posts-controller.ts
    .generate()?;
```

`actions_dir("")` writes the controller files directly alongside `index.ts`; `file_prefix` is similarly available.

**Need a single bundled file** instead of the split tree? Use [`bindings::generate("./frontend/inertia.gen.ts")`](https://docs.rs/veer/latest/veer/bindings/fn.generate.html) — same content, one file, with the route tree exposed as a nested `routes.users.show(...)` namespace.

**Auto-regenerate on commit** with [lefthook](https://github.com/evilmartians/lefthook). Add this to `lefthook.yml` at the repo root:

```yaml
pre-commit:
  commands:
    veer-bindings:
      glob: "*.rs"
      run: cargo run -q --bin gen-bindings
      stage_fixed: true
```

`stage_fixed: true` re-adds the regenerated TS files to the commit so they never drift from the Rust source. Run `lefthook install` once per dev machine. Pair with a CI job that runs `cargo run --bin gen-bindings && git diff --exit-code` on PRs to fail-fast if someone bypasses the hook.

</details>

<details>
<summary><b>Cookie-based flash session</b></summary>

For apps without an existing session crate:

```rust,ignore
use veer::session::cookie::CookieSessionStore;

let cfg = InertiaConfig::new()
    .session(CookieSessionStore::new(env_secret()).secure(true));
```

HMAC-SHA256-signed, constant-time verification, one-shot semantics. For anything more elaborate, implement the `SessionStore` trait over your existing session crate (`axum-login`, redis, …) in ~30 lines.

</details>

<details>
<summary><b>tower-sessions integration</b></summary>

If the app already runs `tower-sessions`, enable the `tower-sessions` feature and plug `TowerSessionStore` into the config. Flash data round-trips through whatever backend (Redis, Postgres, in-memory, …) tower-sessions is configured with.

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
    .layer(session_layer); // tower-sessions must wrap *outside* InertiaLayer
```

Flash is stored under a single key (`_veer_flash` by default; override with `TowerSessionStore::new().key("...")`).

</details>

## 🎛️ Feature Flags

| Flag | Default | Effect |
|------|---------|--------|
| `axum` | **on** | Axum extractor + tower layer + `InertiaForm` body extractor |
| `multipart` | off | File upload support (`UploadedFile`, `MultipartStream`) |
| `ssr` | off | HTTP SSR client (`reqwest`) |
| `cookie-session` | off | Signed-cookie one-shot flash store |
| `tower-sessions` | off | Flash store backed by [`tower-sessions`](https://crates.io/crates/tower-sessions) |
| `validator` | off | `IntoErrorBag` impl for `validator::ValidationErrors` |
| `garde` | off | `IntoErrorBag` impl for `garde::Report` |
| `ts` | off | End-to-end TypeScript bindings codegen (`ts-rs` + `inventory`) |

Disabling a feature drops its transitive deps entirely.

## 🏗️ Architecture

```text
┌────────────────────────────────────────────────────────┐
│  axum Router                                           │
│  ┌──────────────────────────────────────────────────┐  │
│  │ InertiaLayer                                     │  │
│  │   ├─ reads flash from SessionStore               │  │
│  │   ├─ injects config into request extensions      │  │
│  │   └─ on response: finalizes InertiaResponse,     │  │
│  │      writes flash, rewrites 302→303 on non-GET   │  │
│  └──────────────────────────────────────────────────┘  │
│                       │                                │
│                       ▼                                │
│        ┌───────────────────────────┐                   │
│        │  Inertia (extractor)      │                   │
│        │  inertia.render(...) →    │                   │
│        │  InertiaResponse builder  │                   │
│        └───────────────────────────┘                   │
└────────────────────────────────────────────────────────┘
              │
              ▼
   ┌──────────────────────────────────────┐
   │  Protocol core (framework-agnostic)  │
   │  • RequestInfo / PageObject          │
   │  • decide() state machine            │
   │  • prop resolver (partial reloads,   │
   │    deferred groups, merge keys)      │
   │  • Pluggable: RootView, SessionStore,│
   │    SsrClient, SharedProps            │
   └──────────────────────────────────────┘
```

The protocol core has zero I/O and zero framework deps — pure data structures and decision functions, exhaustively unit-tested. Adapters for other Rust frameworks (actix, rocket, salvo) can sit beside the axum one without touching it.

## ⚠️ Caveats

> `Always<T>` and `Merge<T>` are detected at any depth and through any serialization path — typed `#[derive(Serialize)]` structs, `serde_json::json!`, hand-built `Value`s, mixed maps. Only top-level matches affect the Inertia wire format though, because the protocol has no notion of a "nested merge prop". Wrappers placed deeper are still stripped from the JSON sent to the client; they just don't appear in `mergeProps`.

## 🧪 Example App

A complete end-to-end demo lives at [`examples/axum-react-todo/`](examples/axum-react-todo) — axum backend + React/Vite frontend, with validation, flash messages, and an in-memory todo store.

```bash
# Terminal 1 — Rust backend
cargo run -p axum-react-todo

# Terminal 2 — Vite dev server
cd examples/axum-react-todo
bun install
bun dev
```

## 🗺️ Status & Roadmap

`veer` is pre-1.0. The v0.1 protocol surface is complete (all Inertia v3 features: partial reloads, deferred props, merge props, encrypted/clear history, SSR, asset versioning, validation flash). Planned for follow-ups:

- Adapters for `actix-web` and `rocket`
- Typed route-param inference (today: `string | number`; goal: read each handler's `Path` extractor and emit the matching TS type)

Contributions, bug reports, and protocol-conformance fixtures welcome.

## 🙌 Acknowledgements

The protocol is [Inertia.js](https://inertiajs.com) by Jonathan Reinink and contributors. `veer` is an independent server-side implementation for Rust, modeled on the Laravel adapter's behavior.

## 🧪 Testing

```bash
cargo test --all-features
```

## 📋 Changelog

Please see [CHANGELOG](CHANGELOG.md) for more information on what has changed recently.

## 🤝 Contributing

Please see [CONTRIBUTING](CONTRIBUTING.md) for details.
You can also join our Discord server to discuss ideas and get help: [Discord Invite](http://go.climactic.co/discord).

## 🔒 Security Vulnerabilities

Please report security vulnerabilities to [security@climactic.co](mailto:security@climactic.co).

## 💖 Support This Project

Veer is free and open source, built and maintained with care. If this crate has saved you development time or helped power your application, please consider supporting its continued development.

<a href="https://github.com/sponsors/climactic">
    <img src="https://img.shields.io/badge/Sponsor%20on-GitHub-ea4aaa?style=for-the-badge&logo=github" alt="Sponsor on GitHub" />
</a>
&nbsp;
<a href="https://ko-fi.com/ClimacticCo">
    <img src="https://img.shields.io/badge/Support%20on-Ko--fi-FF5E5B?style=for-the-badge&logo=ko-fi&logoColor=white" alt="Support on Ko-fi" />
</a>

### 🌟 Sponsors

<!-- sponsors -->
*Your logo here* — Become a sponsor and get your logo featured in this README and on our website.
<!-- sponsors -->

**Interested in title sponsorship?** Contact us at [sponsors@climactic.co](mailto:sponsors@climactic.co) for premium placement and recognition.

## ⭐ Star History

<a href="https://star-history.com/#climactic/veer&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=climactic/veer&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=climactic/veer&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=climactic/veer&type=Date" />
 </picture>
</a>

## 📄 License

Dual-licensed under **MIT** or **Apache 2.0** at your option. Please see [`MIT`](LICENSE-MIT) and [`APACHE`](LICENSE-APACHE) for more information.
