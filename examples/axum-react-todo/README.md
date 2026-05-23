# axum-react-todo

End-to-end demo of `veer` driving a React + Vite frontend. Runs in two modes
from the same binary, toggled by an env flag.

## Run

From this directory:

```bash
just              # CSR mode  — open http://localhost:5173
SSR=1 just dev    # SSR mode  — open http://localhost:3000
```

`just` installs JS deps on first run and starts everything in parallel.
Ctrl-C kills the whole stack.

If you don't have `just`: `cargo install just` (or `brew install just`).

## CSR mode (default)

Pure client-side bootstrap — Vite owns the browser, Rust owns the data.

1. Browser loads `http://localhost:5173/` → Vite serves `index.html`.
2. `frontend/app.tsx` runs, does a single `fetch` with `X-Inertia: true` for the current URL.
3. Vite's proxy forwards it to the Rust backend on `:3000`.
4. Rust returns the page object as JSON (made unambiguous by `csr_only(true)`).
5. `createInertiaApp` mounts with `createRoot`. Subsequent navigations are XHRs through the same proxy.

Refreshing on `/todos` or any sub-route works — Vite's proxy `bypass` returns `index.html` for browser navigations, so the SPA re-boots and re-fetches.

## SSR mode (`SSR=1`)

Server-rendered first paint — Rust is the HTML origin.

1. Browser loads `http://localhost:3000/` → Rust handler builds the Inertia page object.
2. Rust POSTs the page object to the Bun SSR sidecar on `:13714` (`frontend/ssr.tsx`).
3. Sidecar runs the same React tree through `renderToString`, returns `{head, body}`.
4. `ViteRootView::dev` inlines the SSR body + a script tag with the page payload, plus cross-origin `<script>` tags pointing at the Vite dev server (`:5173`).
5. Browser parses, loads `app.tsx` from Vite, calls `hydrateRoot`. No bootstrap fetch — the page is already in the script tag.
6. Subsequent navigations: regular Inertia XHRs to `:3000`.

The React refresh preamble is emitted in the shell because `@vitejs/plugin-react` can't auto-inject it when the HTML comes from off-origin. `ssr_required(true)` means a dead sidecar fails loudly with a 500.

## Run pieces individually

```bash
# Terminal 1 — Vite
bun install && bun dev

# Terminal 2 — Rust (add SSR=1 for SSR mode)
cargo run

# Terminal 3 (SSR only) — Bun SSR sidecar
bun frontend/ssr.tsx
```

## Production build

Build the client bundle and the SSR sidecar bundle:

```bash
just build        # → dist/  (+ dist/.vite/manifest.json)
just build-ssr    # → dist/ssr/ssr.js
```

For a real deploy, the Rust app would:

- Load the manifest at startup: `let manifest = ViteManifest::load("dist/.vite/manifest.json")?;`
- Use `ViteRootView::production().entry("frontend/app.tsx").manifest(manifest).asset_base("/build")` as the root view
- Wire `InertiaConfig::version` to `manifest.hash()` so frontend rebuilds force-reload stale clients via the Inertia 409 protocol
- Mount `tower_http::services::ServeDir::new("dist")` at `/build` to serve hashed assets
- Run `bun dist/ssr/ssr.js` (or `node`) as a supervised long-lived process for SSR

See the top-level `README.md` ("Vite integration (dev + production)") for the full snippet.
