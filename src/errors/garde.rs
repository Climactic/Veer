//! `IntoErrorBag` impl for the `garde` crate.

use super::IntoErrorBag;
use std::collections::HashMap;

impl IntoErrorBag for garde::Report {
    fn into_error_bag(self) -> HashMap<String, String> {
        self.iter()
            .map(|(path, error)| (path.to_string(), error.message().to_string()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garde::Validate;

    #[derive(Validate)]
    struct NewUser {
        #[garde(length(min = 1))]
        title: String,
        #[garde(email)]
        email: String,
    }

    #[test]
    fn garde_report_flattens_to_inertia_bag() {
        let bad = NewUser {
            title: String::new(),
            email: "not-an-email".into(),
        };
        let errors = bad.validate().unwrap_err();
        let bag = errors.into_error_bag();
        assert!(bag.contains_key("title"));
        assert!(bag.contains_key("email"));
        // Both messages are non-empty.
        assert!(!bag.get("title").unwrap().is_empty());
        assert!(!bag.get("email").unwrap().is_empty());
    }
}
