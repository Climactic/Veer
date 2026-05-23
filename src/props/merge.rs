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

#[cfg(feature = "ts")]
impl<T: ts_rs::TS> ts_rs::TS for Merge<T> {
    type WithoutGenerics = <T as ts_rs::TS>::WithoutGenerics;
    type OptionInnerType = <T as ts_rs::TS>::OptionInnerType;
    fn ident() -> String {
        <T as ts_rs::TS>::ident()
    }
    fn name() -> String {
        <T as ts_rs::TS>::name()
    }
    fn inline() -> String {
        <T as ts_rs::TS>::inline()
    }
    fn inline_flattened() -> String {
        <T as ts_rs::TS>::inline_flattened()
    }
    fn visit_dependencies(v: &mut impl ts_rs::TypeVisitor)
    where
        Self: 'static,
    {
        <T as ts_rs::TS>::visit_dependencies(v);
    }
    fn visit_generics(v: &mut impl ts_rs::TypeVisitor)
    where
        Self: 'static,
    {
        <T as ts_rs::TS>::visit_generics(v);
    }
    fn decl() -> String {
        <T as ts_rs::TS>::decl()
    }
    fn decl_concrete() -> String {
        <T as ts_rs::TS>::decl_concrete()
    }
    fn output_path() -> Option<std::path::PathBuf> {
        <T as ts_rs::TS>::output_path()
    }
}
