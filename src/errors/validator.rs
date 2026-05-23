//! `IntoErrorBag` impl for the `validator` crate.

use super::IntoErrorBag;
use std::collections::HashMap;

impl IntoErrorBag for validator::ValidationErrors {
    fn into_error_bag(self) -> HashMap<String, String> {
        self.field_errors()
            .into_iter()
            .filter_map(|(field, errs)| {
                errs.first().and_then(|e| {
                    e.message
                        .as_ref()
                        .map(|m| (field.to_string(), m.to_string()))
                        .or_else(|| Some((field.to_string(), e.code.to_string())))
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    #[derive(Validate)]
    struct NewUser {
        #[validate(length(min = 1, message = "title is required"))]
        title: String,
        #[validate(email)]
        email: String,
    }

    #[test]
    fn validation_errors_flatten_to_inertia_bag() {
        let bad = NewUser {
            title: String::new(),
            email: "not-an-email".into(),
        };
        let errors = bad.validate().unwrap_err();
        let bag = errors.into_error_bag();
        assert_eq!(bag.get("title").unwrap(), "title is required");
        // `email` has no custom message; we fall back to the code.
        assert_eq!(bag.get("email").unwrap(), "email");
    }
}
