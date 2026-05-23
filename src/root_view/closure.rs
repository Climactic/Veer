//! Adapter from a closure to a `RootView`.

use super::{RootView, RootViewContext};

/// Wrap any `Fn(RootViewContext<'_>) -> Result<String, String>` as a `RootView`.
pub struct ClosureRootView<F>(pub F);

impl<F> RootView for ClosureRootView<F>
where
    F: Fn(RootViewContext<'_>) -> Result<String, String> + Send + Sync,
{
    fn render(&self, ctx: RootViewContext<'_>) -> Result<String, String> {
        (self.0)(ctx)
    }
}
