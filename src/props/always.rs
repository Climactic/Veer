//! `Always<T>` — a prop that's always serialized, even against `X-Inertia-Except`.

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

/// Wraps a value that must always be sent, regardless of `X-Inertia-Except`.
///
/// Detected anywhere in the props tree, through any serialization path
/// (typed `#[derive(Serialize)]` structs, `serde_json::json!`, hand-built
/// `Value`s, etc.). The wrapper serializes as a single-key sentinel object
/// that the Inertia resolver strips before sending to the client. A nested
/// wrapper keeps its value when the partial reload selects its parent. When the
/// parent is not selected, the parent is not sent: the client replaces
/// top-level props as a whole, so a partial parent would lose its other fields.
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
        map.serialize_entry(&super::sentinels().always, &self.0)?;
        map.end()
    }
}

#[cfg(feature = "ts")]
impl<T: ts_rs::TS> ts_rs::TS for Always<T> {
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
