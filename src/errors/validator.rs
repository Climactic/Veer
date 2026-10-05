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

    fn into_all_errors(self) -> HashMap<String, Vec<String>> {
        self.field_errors()
            .into_iter()
            .map(|(field, errs)| {
                let messages = errs
                    .iter()
                    .map(|e| {
                        e.message
                            .as_ref()
                            .map_or(e.code.to_string(), |m| m.to_string())
                    })
                    .collect();
                (field.to_string(), messages)
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
        #[validate(
            length(min = 1, message = "title is required"),
            email(message = "title is not an email")
        )]
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
        let mut all = bad.validate().unwrap_err().into_all_errors();
        all.get_mut("title").unwrap().sort();
        assert_eq!(all["title"], ["title is not an email", "title is required"]);

        let errors = bad.validate().unwrap_err();
        let bag = errors.into_error_bag();
        assert!(bag.get("title").unwrap().starts_with("title is"));
        // `email` has no custom message; we fall back to the code.
        assert_eq!(bag.get("email").unwrap(), "email");
    }
}
