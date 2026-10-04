//! Axum adapter: extractor, layer, IntoResponse impls.

pub mod extractor;
pub mod form;
pub mod layer;
pub mod precognition;
pub mod response;
pub mod router;

#[cfg(any(feature = "validator", feature = "garde"))]
pub mod validated;

#[cfg(feature = "csrf")]
pub mod csrf;

#[cfg(feature = "embed")]
pub mod embed;

pub use extractor::MissingInertiaLayer;
pub use form::{InertiaForm, InertiaFormRejection};
pub use layer::InertiaLayer;
pub use precognition::Precognition;
pub use router::{Method, Router};

#[cfg(feature = "garde")]
pub use validated::GardeValidated;
#[cfg(feature = "validator")]
pub use validated::Validated;

#[cfg(feature = "csrf")]
pub use csrf::CsrfLayer;

#[cfg(feature = "embed")]
pub use embed::EmbeddedAssets;

#[cfg(feature = "multipart")]
pub use form::MultipartStream;
