use axum_react_todo::{router, todos::TodoStore};
use std::net::SocketAddr;
use veer::{
    session::cookie::CookieSessionStore, ssr::http::HttpSsrClient, CsrfLayer, InertiaConfig,
    InertiaLayer, ViteRootView,
};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let store = TodoStore::default();

    // Two dev workflows in one binary, toggled by `SSR=1`:
    //
    // - Default (CSR): Vite owns the browser at :5173, the Rust backend is a
    //   JSON-only API. `csr_only(true)` makes plain GETs return JSON so the
    //   client can bootstrap purely from `fetch`.
    //
    // - SSR (`SSR=1 just dev`): Rust at :3000 is the HTML origin. It POSTs
    //   each page object to a Bun sidecar at :13714 (run from
    //   `frontend/ssr.tsx`), inlines the returned head + body into a
    //   `ViteRootView::dev` shell, then loads `frontend/app.tsx` cross-origin
    //   from the Vite dev server. `react_refresh(true)` emits the preamble
    //   `@vitejs/plugin-react` needs when the shell is served off-origin.
    //   See README for the production swap (`ViteRootView::production` +
    //   `ViteManifest::load`).
    let ssr_mode = std::env::var("SSR").is_ok();

    let mut cfg = InertiaConfig::new().version(|| "dev".into()).session(
        CookieSessionStore::new(b"01234567890123456789012345678901".to_vec()).secure(false),
    );
    if ssr_mode {
        cfg = cfg
            .root_view(
                ViteRootView::dev()
                    .title("veer todo")
                    .entry("frontend/app.tsx")
                    .dev_server("http://localhost:5173")
                    .react_refresh(true),
            )
            .ssr(HttpSsrClient::new("http://127.0.0.1:13714/render"))
            .ssr_required(true);
    } else {
        cfg = cfg.csr_only(true);
    }

    // CSRF protection (demo secret — load from config/env in production).
    // Stacked outside InertiaLayer so it verifies before the handler runs and
    // issues the XSRF-TOKEN cookie the Inertia/axios client echoes back.
    let app = router()
        .build()
        .with_state(store)
        .layer(InertiaLayer::new(cfg))
        .layer(CsrfLayer::new(b"01234567890123456789012345678901".to_vec()).secure(false));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!(
        "listening on http://{addr} ({} mode)",
        if ssr_mode { "SSR" } else { "CSR" }
    );
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
