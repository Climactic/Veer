//! Axum adapter: extractor, layer, IntoResponse impls.

pub mod extractor;
pub mod form;
pub mod layer;
pub mod response;
pub mod router;

#[cfg(feature = "csrf")]
pub mod csrf;

pub use form::{InertiaForm, InertiaFormRejection};
pub use layer::InertiaLayer;
pub use router::{Method, Router};

#[cfg(feature = "csrf")]
pub use csrf::CsrfLayer;

#[cfg(feature = "multipart")]
pub use form::MultipartStream;
