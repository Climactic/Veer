//! File upload support for `multipart/form-data` bodies.
//!
//! Two surfaces:
//!
//! - **`UploadedFile`** — a buffered file value that can appear as a field in a
//!   typed struct deserialized via `InertiaForm<T>`. The parser fills in the
//!   file bytes from the multipart body; the user just reads `file.bytes`.
//!   Good for the common case (avatars, small attachments) where size is bounded.
//!
//! - **`MultipartStream`** — a separate extractor (in the axum adapter) that
//!   exposes the raw `axum::extract::Multipart` for streaming uploads to disk
//!   or S3 without buffering. Use this when files are large or the count is
//!   unbounded.

use serde::{de::Error as _, Deserialize, Deserializer, Serialize};

// The multipart-to-Value bridge in `adapters::axum::form` emits a file field as
// `{"__veer_uploaded_file__": true, "filename": ..., "content_type": ..., "bytes_b64": ...}`.
// `UploadedFile::Wire` below has to use the same literal key in its `#[serde(rename = ...)]`
// because serde rename attributes can't reference constants. If you change the marker
// string, change it in both places.

/// A file uploaded via `multipart/form-data` and buffered in memory.
///
/// Use as a field type on a struct passed to `InertiaForm<T>`:
///
/// ```ignore
/// #[derive(Deserialize)]
/// struct CreateAvatar {
///     user_id: String,
///     avatar: UploadedFile,
/// }
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct UploadedFile {
    /// Original filename reported by the client, if any.
    pub filename: Option<String>,
    /// `Content-Type` of the part, if reported.
    pub content_type: Option<String>,
    /// Raw bytes of the file.
    pub bytes: Vec<u8>,
}

impl UploadedFile {
    /// File size in bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// `true` if the file has zero bytes.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl<'de> Deserialize<'de> for UploadedFile {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "__veer_uploaded_file__")]
            marker: bool,
            filename: Option<String>,
            content_type: Option<String>,
            bytes_b64: String,
        }
        let w = Wire::deserialize(d)?;
        if !w.marker {
            return Err(D::Error::custom(
                "UploadedFile: missing __veer_uploaded_file__ marker — \
                 this type can only be deserialized from a multipart/form-data \
                 field, not from JSON or form-urlencoded input",
            ));
        }
        let bytes = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(w.bytes_b64.as_bytes())
                .map_err(D::Error::custom)?
        };
        Ok(UploadedFile {
            filename: w.filename,
            content_type: w.content_type,
            bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use serde_json::json;

    #[test]
    fn deserializes_from_marker_shape() {
        let bytes = b"hello".to_vec();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let v = json!({
            "__veer_uploaded_file__": true,
            "filename": "greeting.txt",
            "content_type": "text/plain",
            "bytes_b64": b64,
        });
        let file: UploadedFile = serde_json::from_value(v).unwrap();
        assert_eq!(file.bytes, bytes);
        assert_eq!(file.filename.as_deref(), Some("greeting.txt"));
        assert_eq!(file.content_type.as_deref(), Some("text/plain"));
    }

    #[test]
    fn rejects_plain_string_value() {
        let v = json!("just a string");
        let r: Result<UploadedFile, _> = serde_json::from_value(v);
        assert!(r.is_err());
    }
}
