//! Inertia DevTools protocol: records one entry per request for the Inertia
//! DevTools browser extension.
//!
//! Spec: <https://inertiajs.com/docs/v3/advanced/devtools-protocol>. Enable it
//! with [`crate::InertiaConfig::devtools`] in development only: the read API
//! (`GET /_inertia/devtools/entries[/{id}]`) is open unless you set
//! [`DevTools::authorize`].

use crate::request::RequestInfo;
use http::{request::Parts, HeaderMap, HeaderName, Method, StatusCode};
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Set on every response: the id of the recorded entry.
pub const X_INERTIA_DEVTOOLS_ID: HeaderName = HeaderName::from_static("x-inertia-devtools-id");
/// Set on every response: the id of the entry that started the batch.
pub const X_INERTIA_DEVTOOLS_PARENT_OUT: HeaderName =
    HeaderName::from_static("x-inertia-devtools-parent-out");

/// Path prefix of the read API.
pub const ENTRIES_PATH: &str = "/_inertia/devtools/entries";

/// JSON request bodies up to this size are recorded.
pub const BODY_LIMIT: usize = 256_000;

const REDACTED: &str = "[REDACTED]";
const REDACT_HEADERS: [&str; 6] = [
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
    "x-csrf-token",
    "x-xsrf-token",
];
const REDACT_KEYS: [&str; 10] = [
    "password",
    "password_confirmation",
    "current_password",
    "token",
    "_token",
    "access_token",
    "refresh_token",
    "secret",
    "client_secret",
    "api_key",
];

type Authorize = Arc<dyn Fn(&Parts) -> bool + Send + Sync>;

/// In-memory store of recorded entries.
#[derive(Clone)]
pub struct DevTools {
    entries: Arc<Mutex<VecDeque<Value>>>,
    limit: usize,
    authorize: Option<Authorize>,
}

impl Default for DevTools {
    fn default() -> Self {
        Self::new()
    }
}

impl DevTools {
    /// A store that keeps the 100 newest entries of each browser tab.
    pub fn new() -> Self {
        Self {
            entries: Default::default(),
            limit: 100,
            authorize: None,
        }
    }

    /// Guard the read API: it answers `403` when `f` returns `false`. Use it if
    /// the recorder runs where other people can reach the server.
    pub fn authorize<F>(mut self, f: F) -> Self
    where
        F: Fn(&Parts) -> bool + Send + Sync + 'static,
    {
        self.authorize = Some(Arc::new(f));
        self
    }

    /// Set how many entries are kept for each browser tab.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit.max(1);
        self
    }

    /// The entry with this id.
    pub fn entry(&self, id: &str) -> Option<Value> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.iter().find(|e| e["__meta"]["id"] == id).cloned()
    }

    /// All stored entries, newest first.
    pub fn entries(&self) -> Vec<Value> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.iter().rev().cloned().collect()
    }

    /// Answer a read API request: `(status, JSON body)`. `None` if the request
    /// is not for the read API.
    pub fn read_api(&self, request: &Parts) -> Option<(StatusCode, String)> {
        let rest = request.uri.path().strip_prefix(ENTRIES_PATH)?;
        if request.method != Method::GET {
            return None;
        }
        if self
            .authorize
            .as_ref()
            .is_some_and(|allowed| !allowed(request))
        {
            return Some((StatusCode::FORBIDDEN, "null".to_string()));
        }
        Some(match rest.strip_prefix('/').filter(|id| !id.is_empty()) {
            None if rest.is_empty() || rest == "/" => {
                (StatusCode::OK, Value::Array(self.entries()).to_string())
            }
            None => return None,
            Some(id) => match self.entry(id) {
                Some(entry) => (StatusCode::OK, entry.to_string()),
                None => (StatusCode::NOT_FOUND, "null".to_string()),
            },
        })
    }

    fn push(&self, entry: Value) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let tab = &entry["__meta"]["tabUuid"];
        let same_tab = |e: &Value| &e["__meta"]["tabUuid"] == tab;
        if entries.iter().filter(|e| same_tab(e)).count() >= self.limit {
            if let Some(oldest) = entries.iter().position(same_tab) {
                entries.remove(oldest);
            }
        }
        entries.push_back(entry);
        // Tab ids come from the client, so the total is capped too.
        while entries.len() > self.limit.saturating_mul(10) {
            entries.pop_front();
        }
    }

    /// Start the recording of one request.
    pub fn start(&self, headers: &HeaderMap) -> Recording {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let id = format!(
            "{:012x}{:08x}{:06x}",
            at.as_millis(),
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed) & 0xff_ffff
        );
        let batch_id = text(headers, "x-inertia-devtools-parent");
        Recording {
            store: self.clone(),
            started: Instant::now(),
            at,
            batch_id,
            id,
        }
    }
}

/// One request that is being recorded.
pub struct Recording {
    store: DevTools,
    started: Instant,
    at: Duration,
    batch_id: Option<String>,
    /// The entry id (`X-Inertia-Devtools-Id`).
    pub id: String,
}

/// What the adapter knows when the response is ready.
pub struct Finished<'a> {
    /// Parsed request info.
    pub req: &'a RequestInfo,
    /// Request headers.
    pub request_headers: &'a HeaderMap,
    /// Request body capture (see [`body_present`], [`body_empty`], [`body_omitted`]).
    pub request_body: Value,
    /// Response status.
    pub status: StatusCode,
    /// Response headers.
    pub response_headers: &'a HeaderMap,
    /// The rendered page object, if the response is an Inertia page.
    pub page: Option<&'a Value>,
    /// `true` if the response has no body.
    pub response_is_empty: bool,
    /// The matched route pattern, if known.
    pub route: Option<&'a str>,
    /// Where the page render was called, if known.
    pub render_source: Option<&'static std::panic::Location<'static>>,
}

impl Recording {
    /// The batch root: the client sends it back as `X-Inertia-Devtools-Parent`.
    pub fn parent_out(&self, req: &RequestInfo) -> &str {
        match &self.batch_id {
            // A prefetch is speculative and must not move the batch cursor.
            Some(batch) if !req.is_prefetch => batch,
            _ => &self.id,
        }
    }

    /// Build the entry and store it.
    pub fn finish(self, f: Finished<'_>) {
        let component = f.page.and_then(|p| p["component"].as_str());
        let request_type = if f.req.is_precognition {
            "precognition"
        } else if !f.req.is_inertia {
            if component.is_some() {
                "initial"
            } else {
                "http"
            }
        } else if f
            .request_headers
            .contains_key("x-inertia-devtools-deferred")
        {
            "deferred"
        } else if f.request_headers.contains_key("x-inertia-devtools-poll") {
            "poll"
        } else if f.req.partial_component.is_some() {
            "partial"
        } else if f.req.is_prefetch {
            "prefetch"
        } else {
            "navigate"
        };

        let scheme = text(f.request_headers, "x-forwarded-proto").unwrap_or("http".into());
        let host = text(f.request_headers, "host").unwrap_or("localhost".into());
        let redirect_location = text(f.response_headers, "x-inertia-location")
            .or_else(|| text(f.response_headers, "x-inertia-redirect"))
            .or_else(|| text(f.response_headers, "location").filter(|_| f.status.is_redirection()))
            .map(|url| redact_url(&url));
        let response_body = match f.page {
            Some(page) => body_present(page.clone()),
            None if f.response_is_empty => body_empty(),
            None => body_omitted("non-inertia-response"),
        };
        let mut prop_values = f.page.map_or(json!({}), |p| p["props"].clone());
        redact(&mut prop_values);

        self.store.push(json!({
            "__meta": {
                "id": self.id,
                "tabUuid": text(f.request_headers, "x-inertia-devtools-tab"),
                "batchId": self.batch_id,
                "timestamp": iso_8601(self.at),
                "utime": self.at.as_secs_f64(),
                "method": f.req.method.as_str(),
                "url": format!("{scheme}://{host}{}", redact_url(&f.req.url)),
                "component": component,
                "requestType": request_type,
                "status": f.status.as_u16(),
                "redirectLocation": redirect_location,
                "serverTimingMs": self.started.elapsed().as_secs_f64() * 1000.0,
                "visitId": text(f.request_headers, "x-inertia-devtools-visit"),
            },
            "http": {
                "requestHeaders": headers_json(f.request_headers),
                "responseHeaders": headers_json(f.response_headers),
                "requestBody": f.request_body,
                "responseBody": response_body,
            },
            "props": f.page.map_or(json!({}), prop_meta),
            "propValues": prop_values,
            "route": { "name": null, "uri": f.route.unwrap_or(""), "action": null },
            "renderSource": f.render_source.map(|l| json!({ "file": l.file(), "line": l.line() })),
            "componentPath": null,
        }));
    }
}

/// Body capture: no body.
pub fn body_empty() -> Value {
    json!({ "status": "empty" })
}

/// Body capture: a body that is not recorded, with the reason.
pub fn body_omitted(reason: &str) -> Value {
    json!({ "status": "omitted", "reason": reason })
}

/// Body capture: a JSON value, with sensitive keys redacted.
pub fn body_present(mut value: Value) -> Value {
    redact(&mut value);
    json!({ "status": "present", "value": value })
}

fn text(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get(name)?.to_str().ok().map(str::to_owned)
}

fn headers_json(headers: &HeaderMap) -> Value {
    let mut map = Map::new();
    for (name, value) in headers {
        let value = if REDACT_HEADERS.contains(&name.as_str()) {
            REDACTED.to_string()
        } else {
            redact_url(value.to_str().unwrap_or(""))
        };
        map.insert(name.to_string(), value.into());
    }
    map.into()
}

fn is_sensitive(key: &str) -> bool {
    REDACT_KEYS.contains(&key.to_ascii_lowercase().as_str())
}

/// Redact the values of sensitive query parameters (`?token=…`).
fn redact_url(url: &str) -> String {
    let Some((before, query)) = url.split_once('?') else {
        return url.to_string();
    };
    let (query, fragment) = query.split_once('#').unwrap_or((query, ""));
    let query: Vec<String> = query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((key, _)) if is_sensitive(key) => format!("{key}={REDACTED}"),
            _ => pair.to_string(),
        })
        .collect();
    let fragment = if fragment.is_empty() {
        String::new()
    } else {
        format!("#{fragment}")
    };
    format!("{before}?{}{fragment}", query.join("&"))
}

fn redact(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if key == "url" && value.is_string() {
                    *value = redact_url(value.as_str().unwrap_or_default()).into();
                } else if is_sensitive(key) {
                    *value = REDACTED.into();
                } else {
                    redact(value);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact),
        _ => {}
    }
}

/// Per-prop metadata, derived from the page object's own metadata fields.
fn prop_meta(page: &Value) -> Value {
    let mut props: Map<String, Value> = Map::new();
    let mut set = |path: &str, key: &str, value: Value| {
        if let Value::Object(meta) = props.entry(path).or_insert_with(|| json!({})) {
            meta.insert(key.to_string(), value);
        }
    };
    let strings = |field: &str| -> Vec<String> {
        let list = page[field].as_array().into_iter().flatten();
        list.filter_map(|v| v.as_str().map(str::to_owned)).collect()
    };

    let shared = strings("sharedProps");
    for key in page["props"].as_object().into_iter().flat_map(Map::keys) {
        set(key, "shared", shared.contains(key).into());
    }
    set("errors", "inertiaType", "always".into());
    for (key, entry) in page["onceProps"].as_object().into_iter().flatten() {
        let path = entry["prop"].as_str().unwrap_or(key);
        set(path, "inertiaType", "once".into());
        set(path, "once", true.into());
    }
    for (field, direction, deep) in [
        ("mergeProps", "append", false),
        ("prependProps", "prepend", false),
        ("deepMergeProps", "append", true),
    ] {
        for path in strings(field) {
            set(&path, "inertiaType", "merge".into());
            set(&path, "mergeDirection", direction.into());
            if deep {
                set(&path, "deepMerge", true.into());
            }
        }
    }
    for (path, entry) in page["scrollProps"].as_object().into_iter().flatten() {
        set(path, "inertiaType", "scroll".into());
        set(path, "reset", entry["reset"].clone());
    }
    for (group, paths) in page["deferredProps"].as_object().into_iter().flatten() {
        for path in paths
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            set(path, "inertiaType", "defer".into());
            set(path, "deferGroup", group.as_str().into());
        }
    }
    for path in strings("rescuedProps") {
        set(&path, "inertiaType", "defer".into());
        set(&path, "rescued", true.into());
    }
    props.into()
}

/// UTC timestamp with milliseconds, e.g. `2026-07-09T10:00:00.000Z`.
fn iso_8601(since_epoch: Duration) -> String {
    let secs = since_epoch.as_secs();
    // Civil date from days since the epoch (Howard Hinnant's algorithm).
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        secs % 86_400 / 3_600,
        secs % 3_600 / 60,
        secs % 60,
        since_epoch.subsec_millis()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_8601_matches_known_instants() {
        assert_eq!(iso_8601(Duration::ZERO), "1970-01-01T00:00:00.000Z");
        // The instant of the example entry in the protocol document.
        assert_eq!(
            iso_8601(Duration::from_millis(1_783_591_200_123)),
            "2026-07-09T10:00:00.123Z"
        );
        assert_eq!(
            iso_8601(Duration::from_secs(1_709_164_800)),
            "2024-02-29T00:00:00.000Z"
        );
    }

    #[test]
    fn prop_meta_is_derived_from_the_page_object() {
        let page = json!({
            "props": {"errors": {}, "auth": 1, "posts": {"data": []}},
            "sharedProps": ["auth", "errors"],
            "mergeProps": ["posts.data"],
            "scrollProps": {"posts": {"reset": false}},
            "deferredProps": {"side": ["stats"]},
            "onceProps": {"plans-key": {"prop": "plans", "expiresAt": null}},
            "rescuedProps": ["perms"],
        });
        assert_eq!(
            prop_meta(&page),
            json!({
                "errors": {"shared": true, "inertiaType": "always"},
                "auth": {"shared": true},
                "posts": {"shared": false, "inertiaType": "scroll", "reset": false},
                "posts.data": {"inertiaType": "merge", "mergeDirection": "append"},
                "stats": {"inertiaType": "defer", "deferGroup": "side"},
                "plans": {"inertiaType": "once", "once": true},
                "perms": {"inertiaType": "defer", "rescued": true},
            })
        );
    }

    #[test]
    fn store_keeps_the_newest_entries_of_each_tab() {
        let store = DevTools::new().limit(2);
        for (id, tab) in [("a", "t1"), ("b", "t1"), ("c", "t2"), ("d", "t1")] {
            store.push(json!({"__meta": {"id": id, "tabUuid": tab}}));
        }
        let ids: Vec<_> = store
            .entries()
            .iter()
            .map(|e| e["__meta"]["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(ids, ["d", "c", "b"]);
        let parts = |method: &str, path: &str| {
            let request = http::Request::builder().method(method).uri(path);
            request.body(()).unwrap().into_parts().0
        };
        let status = |store: &DevTools, path| store.read_api(&parts("GET", path)).map(|r| r.0);
        let entry = "/_inertia/devtools/entries/c";
        assert_eq!(
            status(&store, "/_inertia/devtools/entries/a"),
            Some(StatusCode::NOT_FOUND)
        );
        assert_eq!(status(&store, entry), Some(StatusCode::OK));
        assert_eq!(status(&store, "/_inertia/devtools/other"), None);
        assert!(store.read_api(&parts("POST", entry)).is_none());

        let guarded = store.clone().authorize(|r| r.headers.contains_key("x-dev"));
        assert_eq!(status(&guarded, entry), Some(StatusCode::FORBIDDEN));
    }

    #[test]
    fn sensitive_query_parameters_are_redacted() {
        assert_eq!(
            redact_url("/reset?token=abc&page=2#top"),
            "/reset?token=[REDACTED]&page=2#top"
        );
        assert_eq!(redact_url("/plain"), "/plain");
    }

    #[test]
    fn sensitive_values_are_redacted() {
        let body = body_present(json!({"user": {"Password": "x", "name": "a"}, "token": 1}));
        assert_eq!(
            body["value"],
            json!({"user": {"Password": REDACTED, "name": "a"}, "token": REDACTED})
        );
    }
}
