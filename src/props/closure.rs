//! Internal types for closure-resolved props (`Lazy`, `Deferred`).
//!
//! These are not used directly in user props structs — they're built via the
//! response builder (`InertiaResponse::lazy`/`optional`/`deferred`).

use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

/// A boxed future returning a JSON value.
pub type BoxedJsonFuture = Pin<Box<dyn Future<Output = Value> + Send>>;

/// A closure that produces a prop value lazily.
pub type LazyClosure = Box<dyn FnOnce() -> BoxedJsonFuture + Send>;

/// Lazily-resolved prop attached via the response builder.
///
/// Backs both `InertiaResponse::lazy` and `InertiaResponse::optional` (the latter is an alias).
pub struct LazyProp {
    /// The closure that resolves the prop value.
    pub closure: LazyClosure,
}

/// Deferred prop — first response advertises group; second response (when client asks) resolves.
pub struct DeferredProp {
    /// Group key under which this deferred prop is advertised in the page object.
    pub group: &'static str,
    /// The closure that resolves the prop value when requested.
    pub closure: LazyClosure,
}
