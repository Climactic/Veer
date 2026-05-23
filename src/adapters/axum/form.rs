//! Content-type-flexible body extractor for Inertia POST/PUT/PATCH handlers.
//!
//! Inertia's JS client sends `application/json` by default (via axios), switches
//! to `application/x-www-form-urlencoded` under some configurations, and uses
//! `multipart/form-data` whenever a `File` is in the form data. `InertiaForm<T>`
//! handles all three and deserializes into the same `T`.
//!
//! For multipart, use [`crate::multipart::UploadedFile`] as field types in `T`.
//! For streaming uploads (huge files, no in-memory buffering), use
//! [`MultipartStream`] instead.

use axum::{
    body::Body,
    extract::{rejection::FormRejection, rejection::JsonRejection, Form, FromRequest, Json},
    http::{header, Request, StatusCode},
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;

/// A request body that may be JSON or form-urlencoded, decoded into `T`.
///
/// Use this in handlers that should accept whatever the Inertia client sends,
/// regardless of how the frontend was wired:
///
/// ```ignore
/// async fn create(InertiaForm(body): InertiaForm<NewUser>) -> impl IntoResponse {
///     // body deserialized from application/json or application/x-www-form-urlencoded
/// }
/// ```
pub struct InertiaForm<T>(pub T);

impl<S, T> FromRequest<S> for InertiaForm<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = InertiaFormRejection;

    async fn from_request(req: Request<Body>, state: &S) -> Result<Self, Self::Rejection> {
        let ct = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        if ct.starts_with("application/json") {
            let Json(value) = Json::<T>::from_request(req, state).await?;
            Ok(InertiaForm(value))
        } else if ct.starts_with("application/x-www-form-urlencoded") || ct.is_empty() {
            // Empty content-type happens on GET (Form reads from query string) and on
            // some clients that omit the header for trivial bodies.
            let Form(value) = Form::<T>::from_request(req, state).await?;
            Ok(InertiaForm(value))
        } else {
            #[cfg(feature = "multipart")]
            if ct.starts_with("multipart/form-data") {
                let value = multipart_to_value(req, state).await?;
                let parsed: T =
                    serde_json::from_value(value).map_err(InertiaFormRejection::MultipartDecode)?;
                return Ok(InertiaForm(parsed));
            }
            Err(InertiaFormRejection::UnsupportedMediaType(ct))
        }
    }
}

#[cfg(feature = "multipart")]
async fn multipart_to_value<S>(
    req: Request<Body>,
    state: &S,
) -> Result<serde_json::Value, InertiaFormRejection>
where
    S: Send + Sync,
{
    use axum::extract::Multipart;
    use base64::Engine;

    let mut multipart = Multipart::from_request(req, state)
        .await
        .map_err(|e| InertiaFormRejection::MultipartTransport(e.to_string()))?;
    let mut map = serde_json::Map::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| InertiaFormRejection::MultipartTransport(e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        let filename = field.file_name().map(str::to_owned);
        let content_type = field.content_type().map(str::to_owned);
        // Heuristic: presence of `filename=` indicates a file part. Pure text
        // fields are stored as strings; file fields as the marker shape that
        // `UploadedFile`'s Deserialize impl recognises.
        if filename.is_some() {
            let bytes = field
                .bytes()
                .await
                .map_err(|e| InertiaFormRejection::MultipartTransport(e.to_string()))?;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            map.insert(
                name,
                serde_json::json!({
                    "__veer_uploaded_file__": true,
                    "filename": filename,
                    "content_type": content_type,
                    "bytes_b64": b64,
                }),
            );
        } else {
            let text = field
                .text()
                .await
                .map_err(|e| InertiaFormRejection::MultipartTransport(e.to_string()))?;
            map.insert(name, serde_json::Value::String(text));
        }
    }

    Ok(serde_json::Value::Object(map))
}

/// Rejection from [`InertiaForm`]. Renders as a `4xx` HTTP response.
#[derive(Debug)]
pub enum InertiaFormRejection {
    /// Inner JSON decoder failed.
    Json(JsonRejection),
    /// Inner form decoder failed.
    Form(FormRejection),
    /// Multipart transport / parsing failure.
    #[cfg(feature = "multipart")]
    MultipartTransport(String),
    /// Multipart bytes parsed OK but deserializing into `T` failed.
    #[cfg(feature = "multipart")]
    MultipartDecode(serde_json::Error),
    /// Content-Type was none of the supported kinds.
    UnsupportedMediaType(String),
}

impl From<JsonRejection> for InertiaFormRejection {
    fn from(e: JsonRejection) -> Self {
        InertiaFormRejection::Json(e)
    }
}

impl From<FormRejection> for InertiaFormRejection {
    fn from(e: FormRejection) -> Self {
        InertiaFormRejection::Form(e)
    }
}

impl IntoResponse for InertiaFormRejection {
    fn into_response(self) -> Response {
        match self {
            InertiaFormRejection::Json(e) => e.into_response(),
            InertiaFormRejection::Form(e) => e.into_response(),
            #[cfg(feature = "multipart")]
            InertiaFormRejection::MultipartTransport(msg) => (
                StatusCode::BAD_REQUEST,
                format!("InertiaForm: multipart parse failed: {msg}"),
            )
                .into_response(),
            #[cfg(feature = "multipart")]
            InertiaFormRejection::MultipartDecode(e) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("InertiaForm: could not decode multipart fields into target type: {e}"),
            )
                .into_response(),
            InertiaFormRejection::UnsupportedMediaType(ct) => {
                let accepted = if cfg!(feature = "multipart") {
                    "application/json, application/x-www-form-urlencoded, or multipart/form-data"
                } else {
                    "application/json or application/x-www-form-urlencoded \
                     (enable the `multipart` feature for file uploads)"
                };
                (
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    format!("InertiaForm: unsupported Content-Type {ct:?}; expected {accepted}"),
                )
                    .into_response()
            }
        }
    }
}

/// Streaming multipart extractor — newtype around [`axum::extract::Multipart`].
///
/// Use for large or unbounded uploads where buffering each file in memory is
/// not acceptable. Iterate parts with [`axum::extract::Multipart::next_field`]
/// and write each one to disk / S3 / wherever as it streams in.
///
/// For small file uploads where you'd rather just deserialize into a typed
/// struct, use [`InertiaForm`] with [`crate::multipart::UploadedFile`] fields.
#[cfg(feature = "multipart")]
pub struct MultipartStream(pub axum::extract::Multipart);

#[cfg(feature = "multipart")]
impl<S> FromRequest<S> for MultipartStream
where
    S: Send + Sync,
{
    type Rejection = Response;
    async fn from_request(req: Request<Body>, state: &S) -> Result<Self, Self::Rejection> {
        let m = axum::extract::Multipart::from_request(req, state)
            .await
            .map_err(IntoResponse::into_response)?;
        Ok(MultipartStream(m))
    }
}
