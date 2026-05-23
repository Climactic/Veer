//! A Vite-aware root view — handles dev (cross-origin Vite dev server,
//! React refresh preamble) and production (manifest-driven script/link tags
//! with transitive `modulepreload` for code-split chunks).
//!
//! Mirrors what Laravel's `@vite` + `@viteReactRefresh` Blade directives do.

use super::{RootView, RootViewContext};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::str::FromStr;

/// Parsed `dist/.vite/manifest.json` produced by `vite build`.
///
/// Use [`ViteManifest::load`] to read one from disk, then hand it to
/// [`ViteRootView::manifest`]. The [`ViteManifest::hash`] helper produces a
/// stable identifier suitable for wiring into
/// [`InertiaConfig::version`](crate::InertiaConfig::version) — any frontend
/// rebuild changes the manifest and bumps the asset version.
#[derive(Debug, Clone)]
pub struct ViteManifest {
    entries: BTreeMap<String, ViteChunk>,
    raw_hash: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct ViteChunk {
    file: String,
    #[serde(default)]
    css: Vec<String>,
    #[serde(default)]
    imports: Vec<String>,
}

impl FromStr for ViteManifest {
    type Err = String;

    /// Parse a manifest from a JSON string. Useful in tests or when the file
    /// has already been read (e.g. embedded via `include_str!`).
    fn from_str(json: &str) -> Result<Self, Self::Err> {
        let entries: BTreeMap<String, ViteChunk> =
            serde_json::from_str(json).map_err(|e| format!("vite manifest parse: {e}"))?;
        let mut hasher = DefaultHasher::new();
        // Hash the canonicalized form (BTreeMap iteration is stable on key
        // order) so two equivalent manifests with different key ordering
        // produce the same hash.
        for (k, v) in &entries {
            k.hash(&mut hasher);
            v.file.hash(&mut hasher);
            for c in &v.css {
                c.hash(&mut hasher);
            }
            for i in &v.imports {
                i.hash(&mut hasher);
            }
        }
        Ok(Self {
            entries,
            raw_hash: hasher.finish(),
        })
    }
}

impl ViteManifest {
    /// Read and parse the manifest from `path`.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let bytes =
            std::fs::read_to_string(path.as_ref()).map_err(|e| format!("vite manifest read: {e}"))?;
        bytes.parse()
    }

    /// Stable, opaque identifier for this manifest. Two equal manifests
    /// produce the same value across processes; any change to file paths,
    /// CSS, or imports changes it.
    pub fn hash(&self) -> String {
        format!("{:016x}", self.raw_hash)
    }
}

/// Vite-aware root view.
///
/// Two construction modes:
/// - [`ViteRootView::dev`] for `vite dev` — emits cross-origin script tags
///   pointing at the dev server plus the Vite HMR client, and (opt-in) the
///   React refresh preamble required by `@vitejs/plugin-react`.
/// - [`ViteRootView::production`] — walks a parsed [`ViteManifest`] from the
///   chosen entry, emitting the entry script, its CSS, and `modulepreload`
///   hints for transitively imported chunks.
#[derive(Debug, Clone)]
pub struct ViteRootView {
    title: String,
    entry: String,
    head_extras: String,
    mode: Mode,
}

#[derive(Debug, Clone)]
enum Mode {
    Dev {
        dev_server: String,
        react_refresh: bool,
    },
    Production {
        manifest: ViteManifest,
        asset_base: String,
    },
}

impl ViteRootView {
    /// Start a dev-mode builder. Defaults: dev server at
    /// `http://localhost:5173`, React refresh preamble off.
    pub fn dev() -> Self {
        Self {
            title: String::new(),
            entry: String::new(),
            head_extras: String::new(),
            mode: Mode::Dev {
                dev_server: "http://localhost:5173".into(),
                react_refresh: false,
            },
        }
    }

    /// Start a production-mode builder. Manifest defaults to an empty one —
    /// callers MUST set it via [`Self::manifest`] (a missing entry produces
    /// an error at render time). `asset_base` defaults to `/build`.
    pub fn production() -> Self {
        Self {
            title: String::new(),
            entry: String::new(),
            head_extras: String::new(),
            mode: Mode::Production {
                manifest: ViteManifest {
                    entries: BTreeMap::new(),
                    raw_hash: 0,
                },
                asset_base: "/build".into(),
            },
        }
    }

    /// Set `<title>`.
    pub fn title(mut self, t: impl Into<String>) -> Self {
        self.title = t.into();
        self
    }

    /// Set the entry name. Dev: a path under your project root the Vite dev
    /// server can resolve (e.g. `"frontend/app.tsx"` — emitted as
    /// `{dev_server}/frontend/app.tsx`). Prod: a key into the Vite manifest
    /// (same string — Vite uses the source path as the key for entries).
    pub fn entry(mut self, e: impl Into<String>) -> Self {
        self.entry = e.into();
        self
    }

    /// Raw HTML appended to `<head>` (after Vite-emitted tags, before SSR head).
    pub fn head_extras(mut self, s: impl Into<String>) -> Self {
        self.head_extras = s.into();
        self
    }

    /// Dev only: change the Vite dev server origin. Panics if called on a
    /// production-mode builder.
    pub fn dev_server(mut self, url: impl Into<String>) -> Self {
        match &mut self.mode {
            Mode::Dev { dev_server, .. } => *dev_server = url.into(),
            Mode::Production { .. } => panic!("dev_server() called on production ViteRootView"),
        }
        self
    }

    /// Dev only: emit `@vitejs/plugin-react`'s HMR preamble. Required when
    /// the HTML shell is served from an origin other than the Vite dev
    /// server (e.g. SSR from your Rust app on `:3000` while Vite runs on
    /// `:5173`) — without it, React plugin throws "can't detect preamble".
    pub fn react_refresh(mut self, on: bool) -> Self {
        match &mut self.mode {
            Mode::Dev { react_refresh, .. } => *react_refresh = on,
            Mode::Production { .. } => panic!("react_refresh() called on production ViteRootView"),
        }
        self
    }

    /// Production only: set the parsed manifest. Panics if called on a
    /// dev-mode builder.
    pub fn manifest(mut self, m: ViteManifest) -> Self {
        match &mut self.mode {
            Mode::Production { manifest, .. } => *manifest = m,
            Mode::Dev { .. } => panic!("manifest() called on dev ViteRootView"),
        }
        self
    }

    /// Production only: URL prefix where built assets are served (e.g.
    /// `/build`, or an absolute CDN URL). Joined with each manifest `file`
    /// path with a single `/`.
    pub fn asset_base(mut self, base: impl Into<String>) -> Self {
        match &mut self.mode {
            Mode::Production { asset_base, .. } => *asset_base = base.into(),
            Mode::Dev { .. } => panic!("asset_base() called on dev ViteRootView"),
        }
        self
    }
}

impl RootView for ViteRootView {
    fn render(&self, ctx: RootViewContext<'_>) -> Result<String, String> {
        let mut head = String::new();
        let vite_tags = match &self.mode {
            Mode::Dev {
                dev_server,
                react_refresh,
            } => render_dev(dev_server, &self.entry, *react_refresh),
            Mode::Production {
                manifest,
                asset_base,
            } => render_production(manifest, asset_base, &self.entry)?,
        };
        head.push_str(&vite_tags);
        // SSR-injected head fragments come after Vite's tags so SSR <title>
        // (if any) wins over a static title. We still emit the static title
        // first as a fallback.
        for h in ctx.ssr.iter().flat_map(|s| s.head.iter()) {
            head.push_str(h);
        }
        head.push_str(&self.head_extras);

        // SSR sidecars return a body containing the `<script data-page="app">`
        // page payload and the `<div id="app">` mount node — inline verbatim.
        // No SSR → emit the same shape ourselves so the client always reads
        // the initial page from the script tag.
        let body_block = ctx.ssr.map(|s| s.body.clone()).unwrap_or_else(|| {
            format!(
                r#"<script data-page="app" type="application/json">{page}</script><div id="app"></div>"#,
                page = ctx.page_json_script,
            )
        });
        Ok(format!(
            r#"<!doctype html>
<html>
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>{title}</title>
{head}
</head>
<body>
{body}
</body>
</html>"#,
            title = html_escape(&self.title),
            head = head,
            body = body_block,
        ))
    }
}

fn render_dev(server: &str, entry: &str, react_refresh: bool) -> String {
    let server = server.trim_end_matches('/');
    let mut out = String::new();
    if react_refresh {
        // Verbatim from @vitejs/plugin-react's docs — must run before any
        // React module loads. Origin is the Vite dev server so the
        // `/@react-refresh` virtual module resolves.
        out.push_str(&format!(
            r#"<script type="module">
import RefreshRuntime from "{server}/@react-refresh"
RefreshRuntime.injectIntoGlobalHook(window)
window.$RefreshReg$ = () => {{}}
window.$RefreshSig$ = () => (type) => type
window.__vite_plugin_react_preamble_installed__ = true
</script>
"#
        ));
    }
    out.push_str(&format!(
        r#"<script type="module" src="{server}/@vite/client"></script>
<script type="module" src="{server}/{entry}"></script>
"#
    ));
    out
}

fn render_production(
    manifest: &ViteManifest,
    asset_base: &str,
    entry: &str,
) -> Result<String, String> {
    let chunk = manifest
        .entries
        .get(entry)
        .ok_or_else(|| format!("vite manifest: entry {entry:?} not found"))?;
    let base = asset_base.trim_end_matches('/');

    let mut scripts = String::new();
    let mut styles = String::new();
    let mut preload_js: BTreeSet<String> = BTreeSet::new();
    let mut preload_css: BTreeSet<String> = BTreeSet::new();

    // Entry script + entry CSS.
    scripts.push_str(&format!(
        r#"<script type="module" src="{base}/{file}"></script>
"#,
        file = chunk.file
    ));
    for css in &chunk.css {
        styles.push_str(&format!(
            r#"<link rel="stylesheet" href="{base}/{css}" />
"#
        ));
    }

    // Walk transitively imported chunks; each becomes a `modulepreload`,
    // and its CSS is treated like the entry's own CSS (the browser needs it
    // for the initial paint regardless of which chunk pulled it in).
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut stack: Vec<String> = chunk.imports.clone();
    while let Some(key) = stack.pop() {
        if !seen.insert(key.clone()) {
            continue;
        }
        let Some(c) = manifest.entries.get(&key) else {
            continue;
        };
        preload_js.insert(c.file.clone());
        for css in &c.css {
            preload_css.insert(css.clone());
        }
        for next in &c.imports {
            stack.push(next.clone());
        }
    }
    for file in &preload_js {
        scripts.push_str(&format!(
            r#"<link rel="modulepreload" href="{base}/{file}" />
"#
        ));
    }
    for css in &preload_css {
        styles.push_str(&format!(
            r#"<link rel="stylesheet" href="{base}/{css}" />
"#
        ));
    }

    Ok(format!("{styles}{scripts}"))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssr::SsrPayload;

    fn ctx<'a>(page: &'a str, ssr: Option<&'a SsrPayload>) -> RootViewContext<'a> {
        RootViewContext {
            page_json: page,
            page_json_script: page,
            asset_version: "1",
            ssr,
        }
    }

    #[test]
    fn dev_emits_vite_client_and_entry_tags() {
        let v = ViteRootView::dev()
            .title("Acme")
            .entry("frontend/app.tsx")
            .dev_server("http://localhost:5173");
        let html = v.render(ctx("{}", None)).unwrap();
        assert!(html.contains(r#"<script type="module" src="http://localhost:5173/@vite/client"></script>"#));
        assert!(html.contains(
            r#"<script type="module" src="http://localhost:5173/frontend/app.tsx"></script>"#
        ));
        // No preamble unless opted in.
        assert!(!html.contains("__vite_plugin_react_preamble_installed__"));
    }

    #[test]
    fn dev_emits_react_refresh_preamble_when_enabled() {
        let v = ViteRootView::dev()
            .title("Acme")
            .entry("frontend/app.tsx")
            .dev_server("http://localhost:5173")
            .react_refresh(true);
        let html = v.render(ctx("{}", None)).unwrap();
        assert!(html.contains("__vite_plugin_react_preamble_installed__"));
        assert!(html.contains("http://localhost:5173/@react-refresh"));
        // Preamble must come before the entry script (it has to run first).
        let preamble_pos = html.find("__vite_plugin_react_preamble_installed__").unwrap();
        let entry_pos = html.find("frontend/app.tsx").unwrap();
        assert!(preamble_pos < entry_pos);
    }

    #[test]
    fn dev_trims_trailing_slash_on_dev_server() {
        let v = ViteRootView::dev()
            .entry("frontend/app.tsx")
            .dev_server("http://localhost:5173/");
        let html = v.render(ctx("{}", None)).unwrap();
        assert!(!html.contains("//frontend"));
        assert!(html.contains("http://localhost:5173/frontend/app.tsx"));
    }

    #[test]
    fn production_entry_script_and_css() {
        let manifest = ViteManifest::from_str(
            r#"{
                "frontend/app.tsx": {
                    "file": "assets/app-AAA.js",
                    "css": ["assets/app-BBB.css"]
                }
            }"#,
        )
        .unwrap();
        let v = ViteRootView::production()
            .title("Acme")
            .entry("frontend/app.tsx")
            .manifest(manifest)
            .asset_base("/build");
        let html = v.render(ctx("{}", None)).unwrap();
        assert!(html.contains(r#"<script type="module" src="/build/assets/app-AAA.js"></script>"#));
        assert!(html.contains(r#"<link rel="stylesheet" href="/build/assets/app-BBB.css" />"#));
    }

    #[test]
    fn production_walks_transitive_imports_for_preload() {
        let manifest = ViteManifest::from_str(
            r#"{
                "frontend/app.tsx": {
                    "file": "assets/app-AAA.js",
                    "imports": ["_vendor.js"]
                },
                "_vendor.js": {
                    "file": "assets/vendor-CCC.js",
                    "css": ["assets/vendor-DDD.css"],
                    "imports": ["_nested.js"]
                },
                "_nested.js": {
                    "file": "assets/nested-EEE.js"
                }
            }"#,
        )
        .unwrap();
        let v = ViteRootView::production()
            .entry("frontend/app.tsx")
            .manifest(manifest);
        let html = v.render(ctx("{}", None)).unwrap();
        assert!(html.contains(r#"<link rel="modulepreload" href="/build/assets/vendor-CCC.js" />"#));
        assert!(html.contains(r#"<link rel="modulepreload" href="/build/assets/nested-EEE.js" />"#));
        // Transitively imported CSS is treated like entry CSS so it loads
        // synchronously alongside the entry's own styles.
        assert!(html.contains(r#"<link rel="stylesheet" href="/build/assets/vendor-DDD.css" />"#));
    }

    #[test]
    fn production_missing_entry_errors() {
        let v = ViteRootView::production()
            .entry("missing.tsx")
            .manifest(ViteManifest::from_str("{}").unwrap());
        let err = v.render(ctx("{}", None)).unwrap_err();
        assert!(err.contains("missing.tsx"));
    }

    #[test]
    fn ssr_body_and_head_are_inlined() {
        let v = ViteRootView::dev()
            .entry("frontend/app.tsx")
            .dev_server("http://localhost:5173");
        let ssr = SsrPayload {
            head: vec!["<meta name=\"x\" />".into()],
            body: r#"<script data-page="app" type="application/json">{"c":"X"}</script><div id="app">SSR'd</div>"#.into(),
        };
        let html = v.render(ctx("{}", Some(&ssr))).unwrap();
        // SSR body inlined verbatim — no second `<div id="app">`.
        assert!(html.contains(r#"<div id="app">SSR'd</div>"#));
        assert_eq!(html.matches(r#"id="app""#).count(), 1);
        assert!(html.contains(r#"<meta name="x" />"#));
    }

    #[test]
    fn no_ssr_emits_script_tag_and_empty_mount_div() {
        let v = ViteRootView::dev()
            .entry("frontend/app.tsx")
            .dev_server("http://localhost:5173");
        let html = v.render(ctx(r#"{"component":"home"}"#, None)).unwrap();
        assert!(html.contains(
            r#"<script data-page="app" type="application/json">{"component":"home"}</script>"#
        ));
        assert!(html.contains(r#"<div id="app"></div>"#));
    }

    #[test]
    fn manifest_hash_is_stable_and_change_sensitive() {
        let a = ViteManifest::from_str(
            r#"{"frontend/app.tsx": {"file": "assets/app-1.js"}}"#,
        )
        .unwrap();
        let b = ViteManifest::from_str(
            r#"{"frontend/app.tsx": {"file": "assets/app-1.js"}}"#,
        )
        .unwrap();
        let c = ViteManifest::from_str(
            r#"{"frontend/app.tsx": {"file": "assets/app-2.js"}}"#,
        )
        .unwrap();
        assert_eq!(a.hash(), b.hash());
        assert_ne!(a.hash(), c.hash());
    }

    #[test]
    fn title_is_html_escaped() {
        let v = ViteRootView::dev()
            .title("<bad>")
            .entry("frontend/app.tsx");
        let html = v.render(ctx("{}", None)).unwrap();
        assert!(html.contains("<title>&lt;bad&gt;</title>"));
    }
}
