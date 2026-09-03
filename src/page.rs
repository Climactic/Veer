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
