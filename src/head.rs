//! Server-provided `<head>` elements for the client's `serverHead` option.
//!
//! The client reads an array of raw HTML strings from a prop (`head` by
//! default). [`Head`] builds that array and escapes each value.

use serde::Serialize;

/// A list of `<head>` elements. Pass it as the `head` prop.
///
/// ```
/// use veer::Head;
/// let head = Head::new().title("Users").meta("description", "All <users>");
/// assert_eq!(
///     serde_json::to_value(&head).unwrap(),
///     serde_json::json!([
///         "<title data-inertia=\"title\">Users</title>",
///         "<meta data-inertia=\"description\" name=\"description\" content=\"All &lt;users&gt;\">",
///     ])
/// );
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Head(Vec<String>);

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

impl Head {
    /// An empty list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add `<title>`.
    pub fn title(mut self, title: &str) -> Self {
        self.0.push(format!(
            r#"<title data-inertia="title">{}</title>"#,
            escape(title)
        ));
        self
    }

    /// Add `<meta name="…" content="…">`. The name is also the `data-inertia`
    /// key, so that a later page replaces the element.
    pub fn meta(mut self, name: &str, content: &str) -> Self {
        let (name, content) = (escape(name), escape(content));
        self.0.push(format!(
            r#"<meta data-inertia="{name}" name="{name}" content="{content}">"#
        ));
        self
    }

    /// Add an element as raw HTML. The caller must escape untrusted values.
    pub fn raw(mut self, html: impl Into<String>) -> Self {
        self.0.push(html.into());
        self
    }
}
