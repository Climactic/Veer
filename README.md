<div align="center">

<img src=".github/veer.webp" alt="Veer" width="100%" />

# Veer

**The Inertia.js server-side protocol superset, for Rust.**

Build modern single-page apps in React, Vue, or Svelte — without writing a JSON API, a client-side router, or a single `fetch`. Your Rust handlers return typed props, and the frontend hydrates as if the page was server-rendered. Because it was.

[![Discord](https://img.shields.io/badge/Discord-Join%20Us-5865F2?style=for-the-badge&logo=discord&logoColor=white)](http://go.climactic.co/discord)
[![Latest Version on crates.io](https://img.shields.io/crates/v/veer.svg?style=for-the-badge)](https://crates.io/crates/veer)
[![GitHub CI Status](https://img.shields.io/github/actions/workflow/status/climactic/veer/ci.yml?branch=main&label=ci&style=for-the-badge)](https://github.com/climactic/veer/actions?query=workflow%3Aci+branch%3Amain)
[![docs.rs](https://img.shields.io/docsrs/veer?style=for-the-badge)](https://docs.rs/veer)
[![MSRV 1.88](https://img.shields.io/badge/MSRV-1.88-blue?style=for-the-badge)](https://www.rust-lang.org)
[![Sponsor on GitHub](https://img.shields.io/badge/Sponsor-GitHub-ea4aaa?style=for-the-badge&logo=github)](https://github.com/sponsors/climactic)
[![Support on Ko-fi](https://img.shields.io/badge/Support-Ko--fi-FF5E5B?style=for-the-badge&logo=ko-fi&logoColor=white)](https://ko-fi.com/ClimacticCo)

</div>

## 📖 Table of Contents

- ✨ [What is Inertia, and why a Rust adapter](#-what-is-inertia-and-why-a-rust-adapter)
- 📦 [Installation](#-installation)
- 🚀 [Quick Start](#-quick-start)
- 🧭 [A short tour](#-a-short-tour)
- 📚 [Documentation](#-documentation)
- 🎛️ [Feature Flags](#️-feature-flags)
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

`veer` is a clean-room Rust implementation of the server side of the [Inertia v3 protocol](https://inertiajs.com/docs/v3/core-concepts/the-protocol). It targets [axum](https://github.com/tokio-rs/axum) out of the box; the protocol core is framework-agnostic, so adapters for other Rust web frameworks slot in beside it.

```text
   ┌─────────────────────────┐                       ┌─────────────────────────┐
   │  Rust handler           │  ── page object ──▶   │  Inertia client (JS)    │
   │  inertia.render(...)    │       (JSON)          │  React / Vue / Svelte   │
   └─────────────────────────┘                       └─────────────────────────┘
              ▲                                                  │
              └─────────────  navigation XHR  ───────────────────┘
```

**Highlights**

- 🦀 The full Inertia v3 protocol (client 3.8), checked against the Laravel adapter and the real client
- ⚡ First-class [axum](https://github.com/tokio-rs/axum) adapter: one extractor, one tower layer
- 📦 Every prop type: partial reloads, deferred, optional, once, merge, infinite scroll, big integers
- ✅ Forms the Inertia way: validation errors, flash data, error bags, Precognition live validation, file uploads
- 🖥️ SSR through the official Node/Bun renderer, with fallback to client rendering
- ⚙️ Vite dev server + production manifest integration, and embedded assets for a single-binary deploy
- 🪢 End-to-end TypeScript: page props and route helpers generated from your Rust types
- 🔒 CSRF protection, history encryption, and a recorder for the Inertia DevTools extension
- 🧩 Framework-agnostic protocol core with pluggable sessions, root views and SSR clients

## 📦 Installation

```toml
[dependencies]
veer = "0.2"
```

Or with `cargo add`:

```bash
cargo add veer
```

The default feature set includes the axum adapter. See [Feature flags](#️-feature-flags) for everything else. Coming from 0.1? Read the [upgrade guide](docs/upgrading.md).

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

The `Inertia` extractor reads the request. `render(component, props)` returns a builder. The layer handles the rest of the protocol — initial HTML on first load, JSON on XHR navigations, 409 for asset-version mismatches, 303 for redirects, partial reloads. Continue with the [getting started guide](docs/getting-started.md).

## 🧭 A short tour

**Load data only when the page needs it.** Closures run only when the response needs their value, and they return plain Rust types. → [Props](docs/props.md)

```rust,ignore
inertia
    .render("Users/Index", UsersIndexProps { users })
    .lazy("stats", || async { load_stats().await })                    // on request only
    .deferred("activity", "default", || async { load_activity().await }) // after first render
    .once("plans", || async { load_plans().await })                    // one time per client
    .prop("feed", Prop::scroll(move || async move { load_feed(page).await })) // infinite scroll
```

**Handle a form like a classic server app.** Validate, redirect, and the errors and the flash message show on the next page. → [Forms and validation](docs/forms-and-validation.md)

```rust,ignore
async fn users_store(inertia: Inertia, InertiaForm(body): InertiaForm<NewUser>) -> impl IntoResponse {
    if let Err(errors) = body.validate() {
        return inertia.with_errors(errors).redirect("/users/new");
    }
    create_user(body).await;
    inertia.redirect("/users").with_flash("success", json!("User created"))
}
```

**Share data with every page.** → [Shared props](docs/props.md#shared-props)

```rust,ignore
let cfg = InertiaConfig::new().shared(shared_props_fn(|_req| async move {
    json!({ "auth": { "user": current_user().await } })
}));
```

**Get TypeScript types from your Rust structs.** → [TypeScript bindings](docs/typescript.md)

```rust,ignore
#[derive(Serialize, TS)]
#[ts(export)]
pub struct UsersIndexProps { pub users: Vec<User> }
veer::register_page!(UsersIndexProps, "Users/Index");
```

```tsx
import { users, type UsersIndexProps } from "./gen";

<Link href={users.show.url({ id: user.id })}>{user.name}</Link>
```

## 📚 Documentation

| Guide | What it covers |
|---|---|
| [Getting started](docs/getting-started.md) | Install, first page, how a request flows, the frontend entry point |
| [Props](docs/props.md) | Partial reloads, lazy / deferred / once props, merging, infinite scroll, shared props, big integers |
| [Forms and validation](docs/forms-and-validation.md) | `InertiaForm`, validation errors, error bags, flash data, Precognition, file uploads |
| [Redirects and history](docs/redirects-and-history.md) | `redirect`, `back`, external redirects, URL fragments, history encryption |
| [Sessions](docs/sessions.md) | The cookie store, `tower-sessions`, writing your own store |
| [Vite, SSR and assets](docs/vite-ssr-assets.md) | `ViteRootView`, server-side rendering, embedded assets, `<head>` elements |
| [TypeScript bindings](docs/typescript.md) | Typed page props and route helpers generated from Rust |
| [CSRF protection](docs/csrf.md) | `CsrfLayer` and the `XSRF-TOKEN` convention |
| [DevTools](docs/devtools.md) | The recorder for the Inertia DevTools browser extension |
| [Architecture](docs/architecture.md) | Crate layout, protocol coverage, extension points |
| [Upgrading from 0.1](docs/upgrading.md) | Every breaking change in 0.2 and what to do |

API reference: [docs.rs/veer](https://docs.rs/veer). For the client side, use the [Inertia documentation](https://inertiajs.com/docs/v3).

## 🎛️ Feature Flags

| Flag | Default | Effect |
|------|---------|--------|
| `axum` | **on** | Axum extractor + tower layer + `InertiaForm` body extractor |
| `multipart` | off | File upload support (`UploadedFile`, `MultipartStream`) |
| `ssr` | off | HTTP SSR client (`reqwest`) |
| `cookie-session` | off | Signed-cookie session store |
| `tower-sessions` | off | Session store backed by [`tower-sessions`](https://crates.io/crates/tower-sessions) |
| `validator` | off | `IntoErrorBag` impl for `validator::ValidationErrors` |
| `garde` | off | `IntoErrorBag` impl for `garde::Report` |
| `csrf` | off | CSRF protection (`CsrfLayer`) |
| `embed` | off | Embedded-asset serving for single-binary deploys (`EmbeddedAssets`) |
| `devtools` | off | Recorder + read API for the Inertia DevTools browser extension |
| `ts` | off | End-to-end TypeScript bindings codegen (`ts-rs` + `inventory`) |

Disabling a feature drops its transitive deps entirely.

## 🧪 Example App

A complete end-to-end demo lives at [`examples/axum-react-todo/`](examples/axum-react-todo) — axum backend + React/Vite frontend, in CSR and SSR mode. It has a todo list with validation, flash messages and Precognition, and a showcase page for once, deferred, rescued, scroll and big-integer props with generated TypeScript types.

```bash
cd examples/axum-react-todo
just              # CSR mode — open http://localhost:5173
SSR=1 just dev    # SSR mode — open http://localhost:3000
```

## 🗺️ Status & Roadmap

`veer` is pre-1.0. The protocol surface follows Inertia v3 as of client 3.8 / `inertia-laravel` 3.5; the [coverage table](docs/architecture.md#protocol-coverage) has the detail. Planned:

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

Issues and pull requests are welcome.
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
