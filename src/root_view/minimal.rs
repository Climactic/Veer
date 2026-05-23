//! A built-in zero-dep root view.

use super::{RootView, RootViewContext};

/// Builder-style minimal HTML shell — title, Vite entry, optional head extras.
#[derive(Debug, Clone, Default)]
pub struct MinimalRootView {
    title: String,
    vite_entry: Option<String>,
    head_extras: String,
}

impl MinimalRootView {
    /// New instance with empty fields.
    pub fn new() -> Self {
        Self::default()
    }
    /// Set `<title>`.
    pub fn title(mut self, t: impl Into<String>) -> Self {
        self.title = t.into();
        self
    }
    /// Add `<script type="module" src="...">` for the given Vite entry path.
    pub fn vite_entry(mut self, p: impl Into<String>) -> Self {
        self.vite_entry = Some(p.into());
        self
    }
    /// Raw HTML appended to `<head>`.
    pub fn head_extras(mut self, s: impl Into<String>) -> Self {
        self.head_extras = s.into();
        self
    }
}

impl RootView for MinimalRootView {
    fn render(&self, ctx: RootViewContext<'_>) -> Result<String, String> {
        let mut head = String::new();
        for h in ctx.ssr.iter().flat_map(|s| s.head.iter()) {
            head.push_str(h);
        }
        head.push_str(&self.head_extras);
        let vite = self
            .vite_entry
            .as_deref()
            .map(|p| format!(r#"<script type="module" src="{p}"></script>"#))
            .unwrap_or_default();
        // SSR sidecars (`@inertiajs/<framework>/server`) return a body that
        // already contains the `<script data-page="app">` page payload and a
        // `<div id="app">` mount node. When present, inline it verbatim. When
        // absent, emit the same shape ourselves so the client always reads
        // the initial page from the script tag.
        let body_block = ctx
            .ssr
            .map(|s| s.body.clone())
            .unwrap_or_else(|| {
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
{vite}
</body>
</html>"#,
            title = html_escape(&self.title),
            head = head,
            body = body_block,
            vite = vite,
        ))
    }
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

    #[test]
    fn renders_with_page_data_and_title() {
        let v = MinimalRootView::new()
            .title("Acme")
            .vite_entry("/src/main.tsx");
        let html = v
            .render(RootViewContext {
                page_json: "{&quot;component&quot;:&quot;Home&quot;}",
                page_json_script: r#"{"component":"Home"}"#,
                asset_version: "1",
                ssr: None,
            })
            .unwrap();
        assert!(html.contains("<title>Acme</title>"));
        assert!(html.contains(
            r#"<script data-page="app" type="application/json">{"component":"Home"}</script>"#
        ));
        assert!(html.contains(r#"<div id="app"></div>"#));
        assert!(html.contains(r#"<script type="module" src="/src/main.tsx"></script>"#));
    }

    #[test]
    fn html_escape_neutralizes_xss_payload_chars() {
        let escaped = html_escape(r#"<script>alert("xss")</script>&'"#);
        // Critical: no raw `<`, `>`, `"`, `'`, or unescaped `&` survives.
        assert!(!escaped.contains('<'));
        assert!(!escaped.contains('>'));
        assert!(!escaped.contains('"'));
        assert!(!escaped.contains('\''));
        assert!(escaped.contains("&lt;script&gt;"));
        assert!(escaped.contains("&quot;xss&quot;"));
        assert!(escaped.contains("&#39;"));
        // Ampersand must be escaped FIRST so we don't double-escape.
        assert!(escaped.ends_with("&amp;&#39;"));
    }

    #[test]
    fn title_with_html_chars_is_escaped_in_output() {
        let v = MinimalRootView::new().title("<bad>");
        let html = v
            .render(RootViewContext {
                page_json: "{}",
                page_json_script: "{}",
                asset_version: "1",
                ssr: None,
            })
            .unwrap();
        assert!(html.contains("<title>&lt;bad&gt;</title>"));
        assert!(!html.contains("<title><bad>"));
    }

    #[test]
    fn ssr_body_replaces_placeholder_no_double_app_div() {
        let v = MinimalRootView::new().title("Acme");
        let ssr = crate::ssr::SsrPayload {
            head: vec![],
            body: r#"<script data-page="app" type="application/json">{"component":"X"}</script><div data-server-rendered="true" id="app"><h1>SSR</h1></div>"#.into(),
        };
        let html = v
            .render(RootViewContext {
                page_json: "should-not-appear",
                page_json_script: "should-not-appear",
                asset_version: "1",
                ssr: Some(&ssr),
            })
            .unwrap();
        // SSR body inlined verbatim; no second `<div id="app">`.
        assert!(html.contains(r#"data-server-rendered="true""#));
        assert!(html.contains("<h1>SSR</h1>"));
        assert_eq!(html.matches(r#"id="app""#).count(), 1);
        // Page JSON comes from the SSR body, not from our context fields.
        assert!(!html.contains("should-not-appear"));
    }
}
