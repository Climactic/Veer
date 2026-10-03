//! `Merge<T>` — a prop whose value the client merges into existing state.

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

/// Wraps a value that the client should merge into its existing prop store.
///
/// Detected anywhere in the props tree, through any serialization path
/// (typed `#[derive(Serialize)]` structs, `serde_json::json!`, hand-built
/// `Value`s, etc.). The wrapper serializes as a single-key sentinel object
/// that the Inertia resolver strips before sending to the client. The dot
/// path of each wrapper is recorded in `page.mergeProps`, so a nested wrapper
/// (`posts.data`) merges at that path.
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
    fn ident(cfg: &ts_rs::Config) -> String {
        <T as ts_rs::TS>::ident(cfg)
    }
    fn name(cfg: &ts_rs::Config) -> String {
        <T as ts_rs::TS>::name(cfg)
    }
    fn inline(cfg: &ts_rs::Config) -> String {
        <T as ts_rs::TS>::inline(cfg)
    }
    fn inline_flattened(cfg: &ts_rs::Config) -> String {
        <T as ts_rs::TS>::inline_flattened(cfg)
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
    fn decl(cfg: &ts_rs::Config) -> String {
        <T as ts_rs::TS>::decl(cfg)
    }
    fn decl_concrete(cfg: &ts_rs::Config) -> String {
        <T as ts_rs::TS>::decl_concrete(cfg)
    }
    fn output_path() -> Option<std::path::PathBuf> {
        <T as ts_rs::TS>::output_path()
    }
}
