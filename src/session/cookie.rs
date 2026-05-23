//! Signed-cookie one-shot flash store.

use super::{Flash, SessionStore};
use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use cookie::Cookie;
use hmac::{Hmac, KeyInit, Mac};
use http::{header, request::Parts as RequestParts, Extensions, HeaderMap};
use sha2::Sha256;
use std::time::Duration;

const COOKIE_NAME: &str = "_veer_flash";

type HmacSha256 = Hmac<Sha256>;

/// HMAC-SHA256-signed cookie flash store.
#[derive(Clone)]
pub struct CookieSessionStore {
    key: Vec<u8>,
    secure: bool,
    same_site: cookie::SameSite,
    max_age: Duration,
}

impl CookieSessionStore {
    /// Create a new store. `key` must be at least 32 bytes.
    pub fn new(key: impl Into<Vec<u8>>) -> Self {
        let key = key.into();
        assert!(
            key.len() >= 32,
            "veer cookie session key must be >= 32 bytes"
        );
        Self {
            key,
            secure: true,
            same_site: cookie::SameSite::Lax,
            max_age: Duration::from_secs(60),
        }
    }

    /// Toggle the `Secure` flag (default `true`). Disable only for local HTTP dev.
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    /// Set the cookie's `SameSite` attribute.
    pub fn same_site(mut self, s: cookie::SameSite) -> Self {
        self.same_site = s;
        self
    }

    fn sign(&self, payload: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("hmac key");
        mac.update(payload);
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    }

    /// Verify a signature in constant time using HMAC's built-in `verify_slice`.
    fn verify(&self, payload: &[u8], sig_b64: &str) -> bool {
        let Ok(sig_bytes) = URL_SAFE_NO_PAD.decode(sig_b64) else {
            return false;
        };
        let Ok(mut mac) = HmacSha256::new_from_slice(&self.key) else {
            return false;
        };
        mac.update(payload);
        mac.verify_slice(&sig_bytes).is_ok()
    }

    fn encode(&self, flash: &Flash) -> String {
        let payload = serde_json::to_vec(&serde_json::json!({
            "errors": flash.errors,
            "bags": flash.bags,
        }))
        .unwrap();
        let b64 = URL_SAFE_NO_PAD.encode(&payload);
        let sig = self.sign(b64.as_bytes());
        format!("{b64}.{sig}")
    }

    fn decode(&self, raw: &str) -> Option<Flash> {
        let (b64, sig) = raw.split_once('.')?;
        if !self.verify(b64.as_bytes(), sig) {
            return None;
        }
        let bytes = URL_SAFE_NO_PAD.decode(b64).ok()?;
        let parsed: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        Some(Flash {
            errors: parsed
                .get("errors")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            bags: parsed
                .get("bags")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
        })
    }

    fn clear_cookie(&self) -> Cookie<'static> {
        let mut c = Cookie::new(COOKIE_NAME, "");
        c.set_path("/");
        c.set_secure(self.secure);
        c.set_http_only(true);
        c.set_same_site(self.same_site);
        c.set_max_age(cookie::time::Duration::ZERO);
        c
    }
}

#[async_trait]
impl SessionStore for CookieSessionStore {
    async fn read_and_clear(&self, req: &RequestParts) -> Flash {
        let raw = req
            .headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|hv| hv.to_str().ok())
            .flat_map(|s| s.split(';'))
            .filter_map(|s| Cookie::parse(s.trim().to_owned()).ok())
            .find(|c| c.name() == COOKIE_NAME)
            .map(|c| c.value().to_string());
        raw.and_then(|r| self.decode(&r)).unwrap_or_default()
    }

    async fn write(&self, headers: &mut HeaderMap, _req_extensions: &Extensions, flash: Flash) {
        if flash.is_empty() {
            let c = self.clear_cookie();
            if let Ok(hv) = http::HeaderValue::from_str(&c.to_string()) {
                headers.append(header::SET_COOKIE, hv);
            }
            return;
        }
        let value = self.encode(&flash);
        let mut c = Cookie::new(COOKIE_NAME, value);
        c.set_path("/");
        c.set_secure(self.secure);
        c.set_http_only(true);
        c.set_same_site(self.same_site);
        c.set_max_age(cookie::time::Duration::seconds(
            self.max_age.as_secs() as i64
        ));
        if let Ok(hv) = http::HeaderValue::from_str(&c.to_string()) {
            headers.append(header::SET_COOKIE, hv);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::Request;

    fn parts(cookie_value: Option<&str>) -> RequestParts {
        let mut b = Request::builder().method("GET").uri("/");
        if let Some(v) = cookie_value {
            b = b.header(header::COOKIE, format!("{COOKIE_NAME}={v}"));
        }
        b.body(()).unwrap().into_parts().0
    }

    #[tokio::test]
    async fn roundtrip_encode_decode_via_cookie() {
        let store = CookieSessionStore::new(vec![0u8; 32]).secure(false);
        let mut flash = Flash::default();
        flash.errors.insert("name".into(), "required".into());

        let mut headers = HeaderMap::new();
        let exts = Extensions::new();
        store.write(&mut headers, &exts, flash.clone()).await;
        let set = headers
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        // Extract value after `_veer_flash=` up to first `;`
        let v = set.split_once('=').unwrap().1.split(';').next().unwrap();

        let req = parts(Some(v));
        let read = store.read_and_clear(&req).await;
        assert_eq!(read.errors.get("name").unwrap(), "required");
    }

    #[tokio::test]
    async fn missing_cookie_yields_empty_flash() {
        let store = CookieSessionStore::new(vec![0u8; 32]);
        let req = parts(None);
        assert!(store.read_and_clear(&req).await.is_empty());
    }

    #[tokio::test]
    async fn bad_signature_yields_empty_flash() {
        let store = CookieSessionStore::new(vec![0u8; 32]);
        let req = parts(Some("tampered.bad"));
        assert!(store.read_and_clear(&req).await.is_empty());
    }
}
