//! veer — Inertia.js v3 server-side protocol for Rust.
//!
//! Build classic server-rendered apps that drive React, Vue, or Svelte frontends
//! through the official Inertia.js client adapters — no separate JSON API needed.
//!
//! See <https://inertiajs.com/the-protocol> for the protocol spec.
//!
//! # Quick start (axum)
//!
//! ```no_run
//! use veer::{Inertia, InertiaConfig, InertiaLayer, MinimalRootView};
//! use axum::{Router, routing::get};
//!
//! # async fn run() {
//! let config = InertiaConfig::new()
//!     .version(|| "1".into())
//!     .root_view(MinimalRootView::new().title("Acme").vite_entry("/src/main.tsx"));
//!
//! let app: Router = Router::new()
//!     .route("/", get(|inertia: Inertia| async move {
//!         inertia.render("Home", serde_json::json!({ "msg": "hello" }))
//!     }))
//!     .layer(InertiaLayer::new(config));
//! # }
//! ```
//!
//! # Feature flags
//!
//! | Flag | Default | Purpose |
//! |------|---------|---------|
//! | `axum` | on | Axum extractor + tower layer |
//! | `ssr` | off | HTTP SSR client backed by `reqwest` |
//! | `cookie-session` | off | Signed-cookie session store |
//! | `validator` | off | `IntoErrorBag` impl for `validator::ValidationErrors` |
//! | `garde` | off | `IntoErrorBag` impl for `garde::Report` |
//! | `csrf` | off | Inertia/axios-compatible CSRF layer (`CsrfLayer`) |
//! | `embed` | off | Embedded-asset serving service (`EmbeddedAssets`) |
//!
//! # Architecture
//!
//! - Protocol core: pure data + decision logic, no I/O. Lives in
//!   [`protocol`], [`page`], [`props`], [`request`].
//! - Pluggable traits: [`RootView`], [`SessionStore`], [`SsrClient`],
//!   [`SharedProps`] — implement your own or use the built-ins.
//! - Adapters: [`adapters::axum`] under the `axum` feature.
//!
//! # Caveats
//!
//! - `Always<T>` and `Merge<T>` wrappers are detected at any depth inside a
//!   typed `Serialize` value, but only top-level matches affect the Inertia
//!   wire format (the protocol has no notion of "nested merge prop").
//! - When building props with the `serde_json::json!` macro, wrappers are
//!   collapsed into raw values before they reach `Inertia::render` — they
//!   only survive via typed `#[derive(Serialize)]` structs. Use
//!   [`InertiaResponse::merge`] to mark top-level keys built via `json!`.
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod config;
pub mod error;
pub mod errors;
pub mod headers;
pub mod inertia;
pub mod page;
pub mod props;
pub mod protocol;
pub mod request;
pub mod response;
pub mod root_view;
pub mod session;
pub mod shared;
pub mod ssr;

#[cfg(feature = "csrf")]
pub mod csrf;

#[cfg(feature = "multipart")]
pub mod multipart;

#[cfg(feature = "axum")]
pub mod adapters;

#[cfg(feature = "ts")]
pub mod bindings;

#[cfg(feature = "ts")]
#[doc(hidden)]
pub mod __private {
    //! Re-exports used by `register_page!` / `inertia_route!`. Not stable.
    pub use inventory;
    pub use ts_rs;
}

pub use config::InertiaConfig;
pub use error::VeerError;
pub use inertia::Inertia;
pub use page::PageObject;
pub use props::{Always, Merge};
pub use request::RequestInfo;
pub use response::InertiaResponse;
pub use root_view::{MinimalRootView, RootView, RootViewContext, ViteManifest, ViteRootView};
pub use session::{Flash, SessionStore};
pub use shared::SharedProps;
#[cfg(feature = "csrf")]
pub use csrf::CsrfTokens;
pub use ssr::{SsrClient, SsrPayload};

#[cfg(feature = "axum")]
pub use adapters::axum::{InertiaForm, InertiaFormRejection, InertiaLayer, Method, Router};

#[cfg(feature = "csrf")]
pub use adapters::axum::CsrfLayer;

#[cfg(feature = "embed")]
pub use adapters::axum::EmbeddedAssets;

#[cfg(feature = "multipart")]
pub use multipart::UploadedFile;

#[cfg(all(feature = "axum", feature = "multipart"))]
pub use adapters::axum::MultipartStream;
