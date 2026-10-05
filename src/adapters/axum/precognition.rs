//! Precognition: live validation requests from the Inertia form helpers.

use crate::errors::IntoErrorBag;
use crate::headers as veer_headers;
use crate::inertia::Inertia;
use axum::body::Body;
use axum::http::{HeaderValue, Response, StatusCode};
use serde_json::json;
use std::collections::{BTreeMap, HashSet};

/// A Precognition validation request. See [`Inertia::precognition`].
#[derive(Debug, Clone)]
pub struct Precognition {
    validate_only: HashSet<String>,
}

impl Inertia {
    /// `Some` when the request only asks to validate its input
    /// (`Precognition: true`). Answer it with [`Precognition::respond`] and do
    /// not run the action:
    ///
    /// ```ignore
    /// if let Some(precognition) = inertia.precognition() {
    ///     return precognition.respond(form.validate());
    /// }
    /// ```
    pub fn precognition(&self) -> Option<Precognition> {
        self.request.is_precognition.then(|| Precognition {
            validate_only: self.request.validate_only.clone(),
        })
    }
}

impl Precognition {
    /// Build the validation response: `204` when no requested field has an
    /// error, otherwise `422` with the errors. Only the fields named in
    /// `Precognition-Validate-Only` are reported when that header is set.
    pub fn respond<E: IntoErrorBag>(self, result: Result<(), E>) -> Response<Body> {
        let errors: BTreeMap<String, Vec<String>> = result
            .err()
            .map(IntoErrorBag::into_all_errors)
            .unwrap_or_default()
            .into_iter()
            .filter(|(field, _)| {
                self.validate_only.is_empty() || self.validate_only.contains(field)
            })
            .collect();

        let mut response = match errors.values().find_map(|messages| messages.first()) {
            None => {
                let mut r = Response::new(Body::empty());
                *r.status_mut() = StatusCode::NO_CONTENT;
                r.headers_mut().insert(
                    &veer_headers::PRECOGNITION_SUCCESS,
                    HeaderValue::from_static("true"),
                );
                r
            }
            Some(message) => {
                let body = json!({ "message": message, "errors": errors });
                let mut r = Response::new(Body::from(body.to_string()));
                *r.status_mut() = StatusCode::UNPROCESSABLE_ENTITY;
                r.headers_mut().insert(
                    http::header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json"),
                );
                r
            }
        };
        let headers = response.headers_mut();
        headers.insert(
            &veer_headers::PRECOGNITION,
            HeaderValue::from_static("true"),
        );
        headers.append(
            &veer_headers::VARY,
            HeaderValue::from_static("Precognition"),
        );
        response
    }
}
