//! Prop wrappers and resolution machinery.

pub mod always;
pub mod closure;
pub mod merge;
pub mod resolver;

pub use always::Always;
pub use merge::Merge;

/// Sentinel object key that marks a value as wrapped in [`Always`].
///
/// The wrapper serializes as `{ALWAYS_SENTINEL: <inner>}`, and the resolver
/// strips the sentinel back out after recording the path. The key is namespaced
/// so user data is implausibly unlikely to collide.
pub(crate) const ALWAYS_SENTINEL: &str = "$$veer_always$$";
/// Sentinel object key that marks a value as wrapped in [`Merge`]. See
/// [`ALWAYS_SENTINEL`].
pub(crate) const MERGE_SENTINEL: &str = "$$veer_merge$$";

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    use serde_json::json;

    #[derive(Serialize)]
    struct Page {
        users: Vec<&'static str>,
        cached: Always<i64>,
        notifs: Merge<Vec<&'static str>>,
    }

    #[test]
    fn wrappers_serialize_as_sentinels_under_plain_serde() {
        // Standard serde produces sentinel objects. The resolver strips them
        // out via `serialize_tag_aware`. This guarantees wrappers survive any
        // serialization path (including `serde_json::json!`).
        let p = Page {
            users: vec!["a", "b"],
            cached: Always(42),
            notifs: Merge(vec!["x"]),
        };
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            json!({
                "users": ["a", "b"],
                "cached": {ALWAYS_SENTINEL: 42},
                "notifs": {MERGE_SENTINEL: ["x"]},
            })
        );
    }
}
