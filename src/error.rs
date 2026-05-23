//! Crate error type.

use thiserror::Error;

/// Errors returned by `veer` APIs.
#[derive(Debug, Error)]
pub enum VeerError {
    /// Serialization of the page object or props failed.
    #[error("serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),

    /// The configured root view failed to render.
    #[error("root view render failed: {0}")]
    RootView(String),

    /// The SSR client returned an error and SSR was required.
    #[error("ssr render failed: {0}")]
    Ssr(String),

    /// The session store returned an error.
    #[error("session store error: {0}")]
    Session(String),

    /// A header was present but malformed.
    #[error("invalid header `{name}`: {reason}")]
    BadHeader {
        /// The header name.
        name: &'static str,
        /// The reason the header was rejected.
        reason: String,
    },
}
