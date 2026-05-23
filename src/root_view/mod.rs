//! Root HTML document rendering.

pub mod closure;
pub mod minimal;
pub mod vite;

pub use closure::ClosureRootView;
pub use minimal::MinimalRootView;
pub use vite::{ViteManifest, ViteRootView};

use crate::ssr::SsrPayload;

/// Context passed to a `RootView` implementation.
pub struct RootViewContext<'a> {
    /// HTML-attribute-escaped page object JSON (safe to interpolate inside
    /// `data-page=""`). Retained for views that emit the legacy div-attribute
    /// form; the Inertia v3 client reads page data from a `<script>` tag, so
    /// most views should prefer [`Self::page_json_script`].
    pub page_json: &'a str,
    /// Script-tag-safe page object JSON (safe to interpolate inside
    /// `<script ...>...</script>`). The only sequences mutated relative to
    /// raw JSON are `</`, `<!--`, and `]]>` — the JSON value is otherwise
    /// byte-for-byte preserved, so the client can `JSON.parse` it directly.
    pub page_json_script: &'a str,
    /// Current asset version.
    pub asset_version: &'a str,
    /// Optional pre-rendered SSR chunks.
    pub ssr: Option<&'a SsrPayload>,
}

/// Renders the initial HTML shell for a non-XHR response.
pub trait RootView: Send + Sync {
    /// Produce the HTML body for the response.
    fn render(&self, ctx: RootViewContext<'_>) -> Result<String, String>;
}
