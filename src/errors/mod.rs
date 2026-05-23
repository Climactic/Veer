//! Bridges between Rust validation crates and Inertia's flat `{field: message}` error shape.

use std::collections::HashMap;

#[cfg(feature = "garde")]
pub mod garde;
#[cfg(feature = "validator")]
pub mod validator;

/// Convert a validation result type into the flat error-bag shape Inertia clients expect.
pub trait IntoErrorBag {
    /// Flatten into `{field: first_message}`.
    fn into_error_bag(self) -> HashMap<String, String>;
}

impl IntoErrorBag for HashMap<String, String> {
    fn into_error_bag(self) -> HashMap<String, String> {
        self
    }
}

impl IntoErrorBag for Vec<(String, String)> {
    fn into_error_bag(self) -> HashMap<String, String> {
        self.into_iter().collect()
    }
}

impl IntoErrorBag for Vec<(&'static str, &'static str)> {
    fn into_error_bag(self) -> HashMap<String, String> {
        self.into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn from_pairs_works() {
        let bag: Vec<(&str, &str)> = vec![("name", "is required")];
        let bag = bag.into_error_bag();
        assert_eq!(bag.get("name").unwrap(), "is required");
    }
}
