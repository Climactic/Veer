//! Big-integer transport: integers outside the JavaScript safe range travel as
//! `{"$bigint": "<digits>"}` markers, which the client revives as `BigInt`.

use serde_json::{json, Value};

const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// Replace each integer outside the JavaScript safe range with a `$bigint` marker.
pub fn encode_big_integers(value: &mut Value) {
    match value {
        Value::Number(n) => {
            let unsafe_int = match (n.as_u64(), n.as_i64()) {
                (Some(u), _) => u > MAX_SAFE_INTEGER,
                (None, Some(i)) => i.unsigned_abs() > MAX_SAFE_INTEGER,
                (None, None) => false,
            };
            if unsafe_int {
                *value = json!({ "$bigint": n.to_string() });
            }
        }
        Value::Array(items) => items.iter_mut().for_each(encode_big_integers),
        Value::Object(map) => map.values_mut().for_each(encode_big_integers),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_unsafe_integers_become_markers() {
        let mut v = json!({
            "safe": 9_007_199_254_740_991u64,
            "big": 900_719_925_474_099_988u64,
            "max": u64::MAX,
            "neg": [-9_007_199_254_740_992i64],
            "float": 1e300,
        });
        encode_big_integers(&mut v);
        assert_eq!(
            v,
            json!({
                "safe": 9_007_199_254_740_991u64,
                "big": {"$bigint": "900719925474099988"},
                "max": {"$bigint": "18446744073709551615"},
                "neg": [{"$bigint": "-9007199254740992"}],
                "float": 1e300,
            })
        );
    }
}
