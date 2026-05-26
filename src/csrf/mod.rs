//! Stateless, signed double-submit CSRF tokens (framework-agnostic).
//!
//! A token is `"{rand_b64}.{hmac(rand_b64)}"` using a URL-safe base64 alphabet,
//! so the value is safe to place in a cookie verbatim — axios reads it back and
//! echoes it in the `X-XSRF-TOKEN` header without `decodeURIComponent` mangling.
//!
//! Validation is double-submit: the request header must equal the cookie
//! (constant-time) AND the cookie must carry a valid server-issued signature.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// HMAC-SHA256 token minter/verifier. Clone-cheap (just holds the key).
#[derive(Clone)]
pub struct CsrfTokens {
    key: Vec<u8>,
}

impl CsrfTokens {
    /// Create a minter. `key` must be at least 32 bytes (asserts, matching
    /// [`crate::session::cookie::CookieSessionStore::new`]).
    pub fn new(key: impl Into<Vec<u8>>) -> Self {
        let key = key.into();
        assert!(key.len() >= 32, "veer csrf secret must be >= 32 bytes");
        Self { key }
    }

    fn sign(&self, payload: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("hmac key");
        mac.update(payload);
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    }

    /// Mint a fresh token: 32 random bytes, base64'd, with its signature.
    pub fn generate(&self) -> String {
        let mut rand = [0u8; 32];
        getrandom::fill(&mut rand).expect("getrandom");
        let rand_b64 = URL_SAFE_NO_PAD.encode(rand);
        let sig = self.sign(rand_b64.as_bytes());
        format!("{rand_b64}.{sig}")
    }

    /// True iff `token` is well-formed and carries a valid signature we issued.
    ///
    /// Checks only the signature, **not** the double-submit cookie↔header
    /// binding, so it is not a CSRF check on its own — that is what
    /// [`Self::verify`] is for. Kept crate-internal so it can't be mistaken for
    /// a validation entry point; the layer uses it to decide whether to
    /// re-issue a cookie.
    pub(crate) fn is_valid(&self, token: &str) -> bool {
        let Some((rand_b64, sig_b64)) = token.split_once('.') else {
            return false;
        };
        let Ok(sig_bytes) = URL_SAFE_NO_PAD.decode(sig_b64) else {
            return false;
        };
        let Ok(mut mac) = HmacSha256::new_from_slice(&self.key) else {
            return false;
        };
        mac.update(rand_b64.as_bytes());
        mac.verify_slice(&sig_bytes).is_ok()
    }

    /// Double-submit check: header equals cookie (constant-time) AND the cookie
    /// is a validly-signed token.
    pub fn verify(&self, cookie_value: &str, header_value: &str) -> bool {
        ct_eq(cookie_value.as_bytes(), header_value.as_bytes()) && self.is_valid(cookie_value)
    }
}

/// Constant-time byte-slice equality, backed by `subtle`. Unequal lengths are
/// not constant-time (the length itself is not secret here), but content
/// comparison is.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"0123456789012345678901234567890123456789";

    #[test]
    fn generate_then_verify_roundtrips() {
        let t = CsrfTokens::new(KEY.to_vec());
        let token = t.generate();
        assert!(t.verify(&token, &token));
        assert!(t.is_valid(&token));
    }

    #[test]
    fn tampered_signature_fails() {
        let t = CsrfTokens::new(KEY.to_vec());
        let token = t.generate();

        // Length-changing tamper: extra char.
        let longer = format!("{token}x");
        assert!(!t.is_valid(&longer));
        assert!(!t.verify(&longer, &longer));

        // Same-length tamper: flip one base64 char inside the signature.
        let (rand_b64, sig_b64) = token.split_once('.').unwrap();
        let mut sig: Vec<u8> = sig_b64.bytes().collect();
        let mid = sig.len() / 2;
        sig[mid] = if sig[mid] == b'A' { b'B' } else { b'A' };
        let flipped = format!("{rand_b64}.{}", String::from_utf8(sig).unwrap());
        assert_eq!(flipped.len(), token.len());
        assert!(!t.is_valid(&flipped));
        assert!(!t.verify(&flipped, &flipped));
    }

    #[test]
    fn token_from_other_key_fails() {
        let a = CsrfTokens::new(KEY.to_vec());
        let b = CsrfTokens::new(b"abcdefghabcdefghabcdefghabcdefgh".to_vec());
        let token = a.generate();
        assert!(!b.verify(&token, &token));
    }

    #[test]
    fn cookie_header_mismatch_fails_even_when_both_valid() {
        let t = CsrfTokens::new(KEY.to_vec());
        let c = t.generate();
        let h = t.generate();
        assert!(t.is_valid(&c) && t.is_valid(&h));
        assert!(!t.verify(&c, &h));
    }

    #[test]
    fn malformed_token_fails() {
        let t = CsrfTokens::new(KEY.to_vec());
        assert!(!t.is_valid("no-dot"));
        assert!(!t.verify("no-dot", "no-dot"));
    }
}
