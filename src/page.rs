//! The Inertia "page object" — the JSON payload that drives the client adapter.

use crate::props::ScrollMetadata;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// One entry of `onceProps`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OnceEntry {
    /// The prop path that holds the value.
    pub prop: String,
    /// Expiry as a Unix timestamp in milliseconds; `null` means no expiry.
    #[serde(rename = "expiresAt")]
    pub expires_at: Option<u64>,
}

/// One entry of `scrollProps`.
#[derive(Debug, Clone, Serialize)]
pub struct ScrollEntry {
    /// Cursor data for the page that this response carries.
    #[serde(flatten)]
    pub metadata: ScrollMetadata,
    /// `true` when the client asked to reset this prop.
    pub reset: bool,
}

/// The shape the Inertia JS client expects.
///
/// Field order matches the protocol; serialized as snake/camelCase as required.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
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
    /// Top-level keys of the shared props.
    #[serde(rename = "sharedProps", skip_serializing_if = "Vec::is_empty")]
    pub shared_props: Vec<String>,
    /// Prop paths that the client appends to its existing state.
    #[serde(rename = "mergeProps", skip_serializing_if = "Vec::is_empty")]
    pub merge_props: Vec<String>,
    /// Prop paths that the client prepends to its existing state.
    #[serde(rename = "prependProps", skip_serializing_if = "Vec::is_empty")]
    pub prepend_props: Vec<String>,
    /// Prop paths that the client deep-merges into its existing state.
    #[serde(rename = "deepMergeProps", skip_serializing_if = "Vec::is_empty")]
    pub deep_merge_props: Vec<String>,
    /// `<propPath>.<keyField>` entries that identify items during a merge.
    #[serde(rename = "matchPropsOn", skip_serializing_if = "Vec::is_empty")]
    pub match_props_on: Vec<String>,
    /// Deferred props grouped by group name (Inertia v2+).
    #[serde(rename = "deferredProps", skip_serializing_if = "BTreeMap::is_empty")]
    pub deferred_props: BTreeMap<String, Vec<String>>,
    /// Deferred props that failed to resolve and were rescued.
    #[serde(rename = "rescuedProps", skip_serializing_if = "Vec::is_empty")]
    pub rescued_props: Vec<String>,
    /// Infinite-scroll cursors by prop path.
    #[serde(rename = "scrollProps", skip_serializing_if = "BTreeMap::is_empty")]
    pub scroll_props: BTreeMap<String, ScrollEntry>,
    /// Once props by once key.
    #[serde(rename = "onceProps", skip_serializing_if = "BTreeMap::is_empty")]
    pub once_props: BTreeMap<String, OnceEntry>,
    /// `true` when integers outside the JavaScript safe range are sent as `$bigint` markers.
    #[serde(rename = "preserveBigIntegers", skip_serializing_if = "is_false")]
    pub preserve_big_integers: bool,
    /// Flash data for this request. Not kept in the browser history state.
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub flash: Map<String, Value>,
    /// Keep the URL fragment of the original request across a redirect.
    #[serde(rename = "preserveFragment", skip_serializing_if = "is_false")]
    pub preserve_fragment: bool,
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
            shared_props: Vec::new(),
            merge_props: Vec::new(),
            prepend_props: Vec::new(),
            deep_merge_props: Vec::new(),
            match_props_on: Vec::new(),
            deferred_props: BTreeMap::new(),
            rescued_props: Vec::new(),
            scroll_props: BTreeMap::new(),
            once_props: BTreeMap::new(),
            preserve_big_integers: false,
            flash: Map::new(),
            preserve_fragment: false,
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

    #[test]
    fn once_and_scroll_entries_match_the_wire_format() {
        let mut p = PageObject::new("Home", json!({}), "/", "v1");
        p.once_props.insert(
            "plans".into(),
            OnceEntry {
                prop: "plans".into(),
                expires_at: None,
            },
        );
        p.scroll_props.insert(
            "posts".into(),
            ScrollEntry {
                metadata: ScrollMetadata::paged("page", 1, true),
                reset: false,
            },
        );
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(
            v["onceProps"],
            json!({"plans": {"prop": "plans", "expiresAt": null}})
        );
        assert_eq!(
            v["scrollProps"],
            json!({"posts": {
                "pageName": "page",
                "previousPage": null,
                "nextPage": 2,
                "currentPage": 1,
                "reset": false
            }})
        );
    }
}
