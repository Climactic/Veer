//! Inertia HTTP header names.

use http::HeaderName;

/// Present (`true`) on Inertia XHR requests.
pub const X_INERTIA: HeaderName = HeaderName::from_static("x-inertia");
/// Asset version known to the client.
pub const X_INERTIA_VERSION: HeaderName = HeaderName::from_static("x-inertia-version");
/// Set on response when an external redirect must be performed (paired with 409).
pub const X_INERTIA_LOCATION: HeaderName = HeaderName::from_static("x-inertia-location");
/// Component name being partially reloaded.
pub const X_INERTIA_PARTIAL_COMPONENT: HeaderName =
    HeaderName::from_static("x-inertia-partial-component");
/// Comma-separated allowlist of prop keys for a partial reload.
pub const X_INERTIA_PARTIAL_DATA: HeaderName = HeaderName::from_static("x-inertia-partial-data");
/// Comma-separated denylist of prop keys for a partial reload.
pub const X_INERTIA_PARTIAL_EXCEPT: HeaderName =
    HeaderName::from_static("x-inertia-partial-except");
/// Indicates the client wants the server to reset specific merged props.
pub const X_INERTIA_RESET: HeaderName = HeaderName::from_static("x-inertia-reset");
/// Set on JSON responses so caches vary correctly.
pub const VARY: HeaderName = HeaderName::from_static("vary");
