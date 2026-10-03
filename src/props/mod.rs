//! Prop wrappers and resolution machinery.

pub mod always;
pub mod merge;
pub mod prop;
pub mod resolver;

pub use always::Always;
pub use merge::Merge;
pub use prop::{Prop, ScrollMetadata};

/// Sentinel object keys that mark a value as wrapped in [`Always`] / [`Merge`].
///
/// A wrapper serializes as `{<sentinel>: <inner>}`, and the resolver strips the
/// sentinel back out after recording the path. The keys have a random suffix
/// for each process, so that user data cannot contain them.
pub(crate) struct Sentinels {
    pub always: String,
    pub merge: String,
}

pub(crate) fn sentinels() -> &'static Sentinels {
    use std::hash::{BuildHasher, Hasher};
    static SENTINELS: std::sync::OnceLock<Sentinels> = std::sync::OnceLock::new();
    SENTINELS.get_or_init(|| {
        // `RandomState` is seeded from the OS for each process.
        let nonce = std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish();
        Sentinels {
            always: format!("$$veer_always_{nonce:016x}$$"),
            merge: format!("$$veer_merge_{nonce:016x}$$"),
        }
    })
}

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
                "cached": {sentinels().always.as_str(): 42},
                "notifs": {sentinels().merge.as_str(): ["x"]},
            })
        );
    }
}
