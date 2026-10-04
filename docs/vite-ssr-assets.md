# Vite, SSR and assets

The **root view** is the HTML shell of the first page load. Veer has three:

| Root view | Use |
|---|---|
| `MinimalRootView` | A shell with no dependencies; good for a first test |
| `ViteRootView` | Vite dev server and production manifest |
| `ClosureRootView`, or your own `RootView` | Full control over the HTML (for example a template engine) |

## Vite

`ViteRootView` does what Laravel's `@vite` and `@viteReactRefresh` directives do.

**One setup for both modes.** `ViteRootView::auto` gives dev mode in a debug build and production mode in a release build:

```rust,ignore
use veer::ViteRootView;

let cfg = InertiaConfig::new().root_view(
    ViteRootView::auto("dist/.vite/manifest.json")?
        .title("Acme")
        .entry("frontend/app.tsx")
        .react_refresh(true)   // dev only; necessary for @vitejs/plugin-react
        .asset_base("/build"), // production only
);

// Serve the built files in production:
// .nest_service("/build", tower_http::services::ServeDir::new("dist"))
```

The manifest is read in a release build only. A setter of one mode has no effect in the other mode. For explicit control, use the two constructors:

**Development.** Script tags point at the Vite dev server:

```rust,ignore
ViteRootView::dev()
    .title("Acme")
    .entry("frontend/app.tsx")
    .dev_server("http://localhost:5173")
    .react_refresh(true)
```

**Production.** `vite build` writes `dist/.vite/manifest.json`. `ViteRootView::production` uses it to emit the entry script, its CSS, and `modulepreload` links for imported chunks:

```rust,ignore
use veer::{ViteManifest, ViteRootView};

ViteRootView::production()
    .title("Acme")
    .entry("frontend/app.tsx")
    .manifest(ViteManifest::load("dist/.vite/manifest.json")?)
    .asset_base("/build")
```

**Asset version.** In production mode, the hash of the manifest is the asset version, so each new build makes old clients reload. You do not set a version by hand. `InertiaConfig::version` or `version_str` overrides it.

## Server-side rendering

SSR needs a small Node or Bun process that renders the page; the official client packages provide it (`@inertiajs/react/server` and the equivalents). Enable the `ssr` feature and give veer its URL:

```rust,ignore
use veer::ssr::http::HttpSsrClient;

let cfg = InertiaConfig::new()
    .ssr(HttpSsrClient::new("http://127.0.0.1:13714/render"));
```

For each first page load, veer posts the page object to the SSR server and puts the returned `head` and `body` into the shell.

| Setting | Effect |
|---|---|
| default | If SSR fails, veer logs the error (with the detail from the SSR server) and renders on the client |
| `InertiaConfig::ssr_required(true)` | If SSR fails, answer `500` |
| `inertia.render(…).no_ssr()` | No SSR for this response |
| `HttpSsrClient::timeout(duration)` | Request timeout (default 5 seconds) |
| `client.health().await` | `true` if the SSR server answers `GET /health`; use it to wait at startup |

The SSR entry file on the frontend:

```tsx
// frontend/ssr.tsx
import createServer from "@inertiajs/react/server";
import { createInertiaApp } from "@inertiajs/react";
import ReactDOMServer from "react-dom/server";

const pages = import.meta.glob("./pages/**/*.tsx", { eager: true });

createServer((page) =>
  createInertiaApp({
    page,
    render: ReactDOMServer.renderToString,
    resolve: (name) => pages[`./pages/${name}.tsx`],
    setup: ({ App, props }) => <App {...props} />,
  }),
);
```

Build it with `vite build --ssr frontend/ssr.tsx` and run the bundle with `node` or `bun`. `import.meta.glob` is a Vite feature: if you run the source file directly with Bun in development, import the pages into a map by hand, as the [example app](../examples/axum-react-todo/frontend/ssr.tsx) does.

## Embedded assets

For one self-contained binary, embed the built frontend. Enable the `embed` feature:

```rust,ignore
use rust_embed::RustEmbed;
use veer::{EmbeddedAssets, ViteManifest, ViteRootView};

#[derive(RustEmbed)]
#[folder = "dist/"]
struct Assets;

let manifest: ViteManifest = include_str!("../dist/.vite/manifest.json").parse()?;

let cfg = InertiaConfig::new()
    .root_view(ViteRootView::production().entry("frontend/app.tsx").manifest(manifest));

let app = router()
    .layer(InertiaLayer::new(cfg))
    .nest_service("/build", EmbeddedAssets::new(|p| Assets::get(p).map(|f| f.data)));
```

`EmbeddedAssets` takes any `Fn(&str) -> Option<Cow<'static, [u8]>>`, so it works with `rust-embed`, `include_dir`, or a map. It sets `Content-Type` from the file extension and a long `Cache-Control`, which is correct for file names that contain a content hash.

## Head elements from the server

The client has a `serverHead` option that reads `<head>` elements from a prop (`head` by default). `veer::Head` builds that prop and escapes its values:

```rust,ignore
use veer::Head;

inertia.render("Users/Show", UsersShowProps {
    head: Head::new().title(&user.name).meta("description", &user.bio),
    user,
})
```

`Head::raw(html)` adds an element as raw HTML; escape untrusted values yourself.

## A custom root view

```rust,ignore
use veer::{RootView, RootViewContext};

struct MyView;

impl RootView for MyView {
    fn render(&self, ctx: RootViewContext<'_>) -> Result<String, String> {
        let body = match ctx.ssr {
            Some(ssr) => ssr.body.clone(),
            None => format!(
                r#"<script data-page="app" type="application/json">{}</script><div id="app"></div>"#,
                ctx.page_json_script
            ),
        };
        Ok(format!("<!doctype html><html><head>…</head><body>{body}</body></html>"))
    }
}
```

Use `ctx.page_json_script` inside the `<script>` tag: it is the page JSON with `<`, `>` and `/` escaped, so that prop data cannot close the tag. Do not HTML-escape it again.
