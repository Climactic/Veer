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

#[cfg(feature = "ts")]
impl<T: ts_rs::TS> ts_rs::TS for Always<T> {
    type WithoutGenerics = <T as ts_rs::TS>::WithoutGenerics;
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
    fn output_path() -> Option<&'static std::path::Path> {
        <T as ts_rs::TS>::output_path()
    }
}
