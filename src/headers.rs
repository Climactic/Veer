//! Inertia HTTP header names.

use http::HeaderName;

/// Present (`true`) on Inertia XHR requests.
pub const X_INERTIA: HeaderName = HeaderName::from_static("x-inertia");
/// Asset version known to the client. Echoed on a version-mismatch 409.
pub const X_INERTIA_VERSION: HeaderName = HeaderName::from_static("x-inertia-version");
/// Set on response when an external redirect must be performed (paired with 409).
pub const X_INERTIA_LOCATION: HeaderName = HeaderName::from_static("x-inertia-location");
/// Set on response when a redirect target has a URL fragment (paired with 409).
pub const X_INERTIA_REDIRECT: HeaderName = HeaderName::from_static("x-inertia-redirect");
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
/// Error bag that scopes the validation errors of this request.
pub const X_INERTIA_ERROR_BAG: HeaderName = HeaderName::from_static("x-inertia-error-bag");
/// Comma-separated once-prop keys that the client already holds.
pub const X_INERTIA_EXCEPT_ONCE_PROPS: HeaderName =
    HeaderName::from_static("x-inertia-except-once-props");
/// `append` or `prepend`: how the client merges an infinite-scroll page.
pub const X_INERTIA_INFINITE_SCROLL_MERGE_INTENT: HeaderName =
    HeaderName::from_static("x-inertia-infinite-scroll-merge-intent");
/// `prefetch` on prefetch requests.
pub const PURPOSE: HeaderName = HeaderName::from_static("purpose");
/// `true` on Precognition validation requests and responses.
pub const PRECOGNITION: HeaderName = HeaderName::from_static("precognition");
/// Comma-separated field names that a Precognition request validates.
pub const PRECOGNITION_VALIDATE_ONLY: HeaderName =
    HeaderName::from_static("precognition-validate-only");
/// `true` on a Precognition response with no validation errors (paired with 204).
pub const PRECOGNITION_SUCCESS: HeaderName = HeaderName::from_static("precognition-success");
/// Set on responses so caches vary correctly.
pub const VARY: HeaderName = HeaderName::from_static("vary");
