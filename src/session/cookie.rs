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
const PREVIOUS_URL_COOKIE: &str = "_veer_previous_url";
const PREVIOUS_URL_MAX_AGE: Duration = Duration::from_secs(2 * 60 * 60);

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

    /// `name` is the cookie name. It is part of the signed data, so that the
    /// value of one cookie is not valid as another cookie.
    fn sign(&self, name: &str, payload: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("hmac key");
        mac.update(name.as_bytes());
        mac.update(b"=");
        mac.update(payload);
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    }

    /// Verify a signature in constant time using HMAC's built-in `verify_slice`.
    fn verify(&self, name: &str, payload: &[u8], sig_b64: &str) -> bool {
        let Ok(sig_bytes) = URL_SAFE_NO_PAD.decode(sig_b64) else {
            return false;
        };
        let Ok(mut mac) = HmacSha256::new_from_slice(&self.key) else {
            return false;
        };
        mac.update(name.as_bytes());
        mac.update(b"=");
        mac.update(payload);
        mac.verify_slice(&sig_bytes).is_ok()
    }

    fn encode(&self, flash: &Flash) -> String {
        let payload = serde_json::to_vec(flash).unwrap();
        let b64 = URL_SAFE_NO_PAD.encode(&payload);
        let sig = self.sign(COOKIE_NAME, b64.as_bytes());
        format!("{b64}.{sig}")
    }

    fn decode(&self, raw: &str) -> Option<Flash> {
        let (b64, sig) = raw.split_once('.')?;
        if !self.verify(COOKIE_NAME, b64.as_bytes(), sig) {
            return None;
        }
        let bytes = URL_SAFE_NO_PAD.decode(b64).ok()?;
        serde_json::from_slice(&bytes).ok()
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

fn read_cookie(req: &RequestParts, name: &str) -> Option<String> {
    req.headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|hv| hv.to_str().ok())
        .flat_map(|s| s.split(';'))
        .filter_map(|s| Cookie::parse(s.trim().to_owned()).ok())
        .find(|c| c.name() == name)
        .map(|c| c.value().to_string())
}

impl CookieSessionStore {
    fn set_cookie(
        &self,
        headers: &mut HeaderMap,
        name: &'static str,
        value: String,
        max_age: Duration,
    ) {
        let mut c = Cookie::new(name, value);
        c.set_path("/");
        c.set_secure(self.secure);
        c.set_http_only(true);
        c.set_same_site(self.same_site);
        c.set_max_age(cookie::time::Duration::seconds(max_age.as_secs() as i64));
        if let Ok(hv) = http::HeaderValue::from_str(&c.to_string()) {
            headers.append(header::SET_COOKIE, hv);
        }
    }
}

#[async_trait]
impl SessionStore for CookieSessionStore {
    async fn read_and_clear(&self, req: &RequestParts) -> Flash {
        read_cookie(req, COOKIE_NAME)
            .and_then(|r| self.decode(&r))
            .unwrap_or_default()
    }

    async fn write(&self, headers: &mut HeaderMap, _req_extensions: &Extensions, flash: Flash) {
        if flash.is_empty() {
            let c = self.clear_cookie();
            if let Ok(hv) = http::HeaderValue::from_str(&c.to_string()) {
                headers.append(header::SET_COOKIE, hv);
            }
            return;
        }
        self.set_cookie(headers, COOKIE_NAME, self.encode(&flash), self.max_age);
    }

    async fn previous_url(&self, req: &RequestParts) -> Option<String> {
        let raw = read_cookie(req, PREVIOUS_URL_COOKIE)?;
        let (b64, sig) = raw.split_once('.')?;
        if !self.verify(PREVIOUS_URL_COOKIE, b64.as_bytes(), sig) {
            return None;
        }
        String::from_utf8(URL_SAFE_NO_PAD.decode(b64).ok()?).ok()
    }

    async fn store_previous_url(
        &self,
        headers: &mut HeaderMap,
        _req_extensions: &Extensions,
        url: &str,
    ) {
        let b64 = URL_SAFE_NO_PAD.encode(url);
        let value = format!("{b64}.{}", self.sign(PREVIOUS_URL_COOKIE, b64.as_bytes()));
        self.set_cookie(headers, PREVIOUS_URL_COOKIE, value, PREVIOUS_URL_MAX_AGE);
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
        flash.errors.insert("name".into(), vec!["required".into()]);

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
        assert_eq!(read.errors["name"], ["required"]);
    }

    #[tokio::test]
    async fn previous_url_has_its_own_signed_cookie() {
        let store = CookieSessionStore::new(vec![0u8; 32]);
        let mut headers = HeaderMap::new();
        store
            .store_previous_url(&mut headers, &Extensions::new(), "/users?page=2")
            .await;
        let set = headers[header::SET_COOKIE].to_str().unwrap();
        assert!(set.starts_with("_veer_previous_url=") && set.contains("Max-Age=7200"));
        let value = set.split_once('=').unwrap().1.split(';').next().unwrap();

        let request = |value: &str| {
            let b =
                Request::builder().header(header::COOKIE, format!("_veer_previous_url={value}"));
            b.body(()).unwrap().into_parts().0
        };
        assert_eq!(
            store.previous_url(&request(value)).await.as_deref(),
            Some("/users?page=2")
        );
        assert_eq!(store.previous_url(&request("L2V2aWw.bad")).await, None);
        // A signed value of one cookie is not valid as the other cookie.
        let as_flash = Request::builder().header(header::COOKIE, format!("{COOKIE_NAME}={value}"));
        let as_flash = as_flash.body(()).unwrap().into_parts().0;
        assert!(store.read_and_clear(&as_flash).await.is_empty());
        let flash_value = store.encode(&Flash::default());
        assert_eq!(store.previous_url(&request(&flash_value)).await, None);
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
