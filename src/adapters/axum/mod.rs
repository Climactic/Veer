//! Axum adapter: extractor, layer, IntoResponse impls.

pub mod extractor;
pub mod form;
pub mod layer;
pub mod response;
pub mod router;

pub use form::{InertiaForm, InertiaFormRejection};
pub use layer::InertiaLayer;
pub use router::{Method, Router};

#[cfg(feature = "multipart")]
pub use form::MultipartStream;
