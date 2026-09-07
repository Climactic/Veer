//! The Inertia "page object" — the JSON payload that drives the client adapter.

use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// The shape the Inertia JS client expects.
///
/// Field order matches the protocol; serialized as snake/camelCase as required.
#[derive(Debug, Clone, Serialize)]
pub struct PageObject {
    /// Component name (e.g. `"Users/Index"`).
    pub component: String,
    /// Resolved props for this render.
    pub props: Value,
    /// Current URL.
    pub url: String,
    /// Asset version this response was generated against.
    pub version: String,
    /// Encrypted history flag (Inertia v2+).
    #[serde(rename = "encryptHistory", skip_serializing_if = "is_false")]
    pub encrypt_history: bool,
    /// Clear history flag (Inertia v2+).
    #[serde(rename = "clearHistory", skip_serializing_if = "is_false")]
    pub clear_history: bool,
    /// Keys whose values should merge into the client's existing prop state.
    #[serde(rename = "mergeProps", skip_serializing_if = "Vec::is_empty")]
    pub merge_props: Vec<String>,
    /// Array paths to prepend when loading an earlier scroll page.
    #[serde(rename = "prependProps", skip_serializing_if = "Vec::is_empty")]
    pub prepend_props: Vec<String>,
    /// Item identity paths used to update matching items instead of duplicating them.
    #[serde(rename = "matchPropsOn", skip_serializing_if = "Vec::is_empty")]
    pub match_props_on: Vec<String>,
    /// Pagination metadata consumed by Inertia's InfiniteScroll component.
    #[serde(rename = "scrollProps", skip_serializing_if = "BTreeMap::is_empty")]
    pub scroll_props: BTreeMap<String, ScrollMetadata>,
    /// Keys whose merge state the server is asking the client to reset.
    #[serde(rename = "resetMergeProps", skip_serializing_if = "Vec::is_empty")]
    pub reset_merge_props: Vec<String>,
    /// Client cache key to prop mapping for remembered values.
    #[serde(rename = "onceProps", skip_serializing_if = "BTreeMap::is_empty")]
    pub once_props: BTreeMap<String, OncePropMetadata>,
    /// Deferred props grouped by group name (Inertia v2+).
    #[serde(rename = "deferredProps", skip_serializing_if = "BTreeMap::is_empty")]
    pub deferred_props: BTreeMap<String, Vec<String>>,
}

/// Metadata identifying a value remembered by the Inertia client.
#[derive(Debug, Clone, Serialize)]
pub struct OncePropMetadata {
    /// Name of the prop containing the remembered value.
    pub prop: String,
}

/// A numbered page or an opaque cursor for an infinite-scroll prop.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ScrollPage {
    /// A one-based page number.
    Number(u32),
    /// An opaque database cursor.
    Cursor(String),
}

impl From<u32> for ScrollPage {
    fn from(value: u32) -> Self {
        Self::Number(value)
    }
}

impl From<String> for ScrollPage {
    fn from(value: String) -> Self {
        Self::Cursor(value)
    }
}

/// Page boundaries supplied by the application's database paginator.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrollMetadata {
    /// Query-string parameter used to request a page.
    pub page_name: String,
    /// Page represented by this response.
    pub current_page: ScrollPage,
    /// Previous page, or `None` at the start.
    pub previous_page: Option<ScrollPage>,
    /// Next page, or `None` at the end.
    pub next_page: Option<ScrollPage>,
    pub(crate) reset: bool,
    #[serde(skip)]
    pub(crate) match_on: Option<String>,
}

impl ScrollMetadata {
    /// Create metadata using either numbered pages or string cursors.
    pub fn new<P: Into<ScrollPage>>(
        page_name: impl Into<String>,
        current_page: P,
        previous_page: Option<P>,
        next_page: Option<P>,
    ) -> Self {
        Self {
            page_name: page_name.into(),
            current_page: current_page.into(),
            previous_page: previous_page.map(Into::into),
            next_page: next_page.map(Into::into),
            reset: false,
            match_on: None,
        }
    }

    /// Match existing items by this field when merging refreshed scroll data.
    pub fn match_on(mut self, field: impl Into<String>) -> Self {
        self.match_on = Some(field.into());
        self
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl PageObject {
    /// Construct a new page object with required fields; other fields default-empty.
    pub fn new(
        component: impl Into<String>,
        props: Value,
        url: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            component: component.into(),
            props,
            url: url.into(),
            version: version.into(),
            encrypt_history: false,
            clear_history: false,
            merge_props: Vec::new(),
            prepend_props: Vec::new(),
            match_props_on: Vec::new(),
            scroll_props: BTreeMap::new(),
            reset_merge_props: Vec::new(),
            deferred_props: BTreeMap::new(),
            once_props: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn minimal_page_serializes_with_only_required_fields() {
        let p = PageObject::new("Home", json!({"msg": "hi"}), "/", "v1");
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(
            v,
            json!({
                "component": "Home",
                "props": {"msg": "hi"},
                "url": "/",
                "version": "v1"
            })
        );
    }

    #[test]
    fn flags_and_lists_serialize_when_non_default() {
        let mut p = PageObject::new("Home", json!({}), "/", "v1");
        p.encrypt_history = true;
        p.merge_props = vec!["notifications".into()];
        p.deferred_props
            .insert("dashboard".into(), vec!["stats".into()]);
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["encryptHistory"], true);
        assert_eq!(v["mergeProps"], json!(["notifications"]));
        assert_eq!(v["deferredProps"], json!({"dashboard": ["stats"]}));
    }
}
