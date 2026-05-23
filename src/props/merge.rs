//! `Merge<T>` — a prop whose value the client merges into existing state.

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

/// Wraps a value that the client should merge into its existing prop store.
///
/// Detected anywhere in the props tree, through any serialization path
/// (typed `#[derive(Serialize)]` structs, `serde_json::json!`, hand-built
/// `Value`s, etc.). The wrapper serializes as a single-key sentinel object
/// that the Inertia resolver strips before sending to the client. Top-level
/// wrappers are recorded in `page.mergeProps` per the Inertia protocol;
/// deeper-nested wrappers are detected and stripped, but the wire format has
/// no notion of "nested merge prop" so they have no semantic effect today.
///
/// [`crate::response::InertiaResponse::merge`] provides the same effect via
/// builder-style API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Merge<T>(pub T);

impl<T> Merge<T> {
    /// Wrap a value as merge-mode.
    pub fn new(value: T) -> Self {
        Self(value)
    }
}

impl<T: Serialize> Serialize for Merge<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(1))?;
        map.serialize_entry(super::MERGE_SENTINEL, &self.0)?;
        map.end()
    }
}
