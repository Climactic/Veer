//! Body extractors that validate before the handler runs.

use crate::adapters::axum::InertiaForm;
use crate::errors::IntoErrorBag;
use crate::inertia::Inertia;
use axum::body::Body;
use axum::extract::{FromRequest, FromRequestParts};
use axum::http::Request;
use axum::response::{IntoResponse, Response};
use serde::de::DeserializeOwned;

/// Decode the body as [`InertiaForm`] does, then apply `validate`.
///
/// A Precognition request gets its answer here. Invalid input goes back to
/// the page that the user came from, with the errors.
async fn extract<S, T, E>(
    req: Request<Body>,
    state: &S,
    validate: impl FnOnce(&T) -> Result<(), E>,
) -> Result<T, Response>
where
    S: Send + Sync,
    T: DeserializeOwned,
    E: IntoErrorBag,
{
    let (mut parts, body) = req.into_parts();
    let inertia = Inertia::from_request_parts(&mut parts, state)
        .await
        .map_err(IntoResponse::into_response)?;
    let InertiaForm(value) =
        InertiaForm::<T>::from_request(Request::from_parts(parts, body), state)
            .await
            .map_err(IntoResponse::into_response)?;

    let result = validate(&value);
    if let Some(precognition) = inertia.precognition() {
        return Err(precognition.respond(result));
    }
    match result {
        Ok(()) => Ok(value),
        Err(errors) => Err(inertia.back().with_errors(errors).into_response()),
    }
}

/// An [`InertiaForm`] body that passed `validator::Validate`.
///
/// The handler runs only for valid input that is not a Precognition request:
///
/// ```ignore
/// async fn store(inertia: Inertia, Validated(body): Validated<NewUser>) -> impl IntoResponse {
///     create_user(body).await;
///     inertia.redirect("/users").with_flash("success", "User created")
/// }
/// ```
///
/// Invalid input redirects back ([`Inertia::back`]) with the errors, which
/// needs a session store. A Precognition request gets `204` or `422`.
#[cfg(feature = "validator")]
pub struct Validated<T>(pub T);

#[cfg(feature = "validator")]
impl<S, T> FromRequest<S> for Validated<T>
where
    S: Send + Sync,
    T: DeserializeOwned + validator::Validate,
{
    type Rejection = Response;

    async fn from_request(req: Request<Body>, state: &S) -> Result<Self, Self::Rejection> {
        extract(req, state, T::validate).await.map(Self)
    }
}

/// An [`InertiaForm`] body that passed `garde::Validate`. It behaves as
/// `Validated` does for the `validator` crate.
#[cfg(feature = "garde")]
pub struct GardeValidated<T>(pub T);

#[cfg(feature = "garde")]
impl<S, T> FromRequest<S> for GardeValidated<T>
where
    S: Send + Sync,
    T: DeserializeOwned + garde::Validate,
    T::Context: Default,
{
    type Rejection = Response;

    async fn from_request(req: Request<Body>, state: &S) -> Result<Self, Self::Rejection> {
        extract(req, state, T::validate).await.map(Self)
    }
}
