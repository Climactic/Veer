//! `Always<T>` — a prop that's always serialized, even against `X-Inertia-Except`.

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

/// Wraps a value that must always be sent, regardless of `X-Inertia-Except`.
///
/// Detected anywhere in the props tree, through any serialization path
/// (typed `#[derive(Serialize)]` structs, `serde_json::json!`, hand-built
/// `Value`s, etc.). The wrapper serializes as a single-key sentinel object
/// that the Inertia resolver strips before sending to the client. The Inertia
/// protocol only honors the rule at the top level of a render's props —
/// wrappers at deeper paths are still detected and stripped but have no
/// wire-format meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Always<T>(pub T);

impl<T> Always<T> {
    /// Wrap a value as always-on.
    pub fn new(value: T) -> Self {
        Self(value)
    }
}

impl<T: Serialize> Serialize for Always<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(1))?;
        map.serialize_entry(super::ALWAYS_SENTINEL, &self.0)?;
        map.end()
    }
}
