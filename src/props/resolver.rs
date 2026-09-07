//! Resolve a serialized props tree against request partial-reload rules.

use crate::props::closure::{DeferredProp, LazyProp, OnceProp};
use crate::request::RequestInfo;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Output of resolving a props tree.
pub struct ResolvedProps {
    /// The final props JSON sent to the client.
    pub props: Value,
    /// Keys to record under `page.mergeProps`.
    pub merge_props: Vec<String>,
    /// Client cache metadata, including already remembered values.
    pub once_props: BTreeMap<String, crate::page::OncePropMetadata>,
    /// Deferred groups → key list (only populated on first/non-partial responses).
    pub deferred_props: BTreeMap<String, Vec<String>>,
}

/// Inputs to the resolver.
pub struct ResolveInput<'a> {
    /// Parsed request info for this render.
    pub req: &'a RequestInfo,
    /// Component name (e.g. `"Users/Index"`).
    pub component: &'a str,
    /// Already-serialized base props from the user's struct (via custom serializer that records tags).
    pub base: SerializedBase,
    /// Ordinary closure props, resolved only when included in this response.
    pub ordinary: HashMap<String, LazyProp>,
    /// Builder-attached lazy/optional props by top-level key (both routes share this map).
    pub lazies: HashMap<String, LazyProp>,
    /// Props remembered by the client.
    pub once: HashMap<String, OnceProp>,
    /// Builder-attached deferred props by top-level key.
    pub deferreds: HashMap<String, DeferredProp>,
    /// Builder-attached merge props by top-level key (already serialized).
    pub merges: HashSet<String>,
    /// Shared props (serialized via same tag-aware serializer).
    pub shared: Option<SerializedBase>,
}

/// Result of serializing user props through the tag-aware serializer.
pub struct SerializedBase {
    /// The serialized JSON value.
    pub value: Value,
    /// JSON-pointer-ish path strings (e.g. `"/notifs"`) that came from `Always` wrappers.
    pub always_paths: HashSet<String>,
    /// Same for `Merge` wrappers.
    pub merge_paths: HashSet<String>,
}

/// Serialize a `Serialize` value while collecting the JSON-pointer paths of any
/// [`crate::props::Always`] and [`crate::props::Merge`] wrappers in the tree.
///
/// `Always<T>` and `Merge<T>` serialize as single-key sentinel objects under
/// standard serde (see [`crate::props::ALWAYS_SENTINEL`] /
/// [`crate::props::MERGE_SENTINEL`]). This function runs `serde_json::to_value`
/// then walks the resulting tree once, recording each sentinel's path and
/// replacing it with its inner value. Because the markers live in the JSON
/// tree itself, this works through any pathway — typed structs, `json!`,
/// hand-built `Value`s, mixed maps, etc.
pub fn serialize_tag_aware<T: Serialize>(value: &T) -> Result<SerializedBase, serde_json::Error> {
    let mut json_value = serde_json::to_value(value)?;
    let mut always_paths = HashSet::new();
    let mut merge_paths = HashSet::new();
    let mut path = String::new();
    strip_sentinels(
        &mut json_value,
        &mut path,
        &mut always_paths,
        &mut merge_paths,
    );
    Ok(SerializedBase {
        value: json_value,
        always_paths,
        merge_paths,
    })
}

/// Walk `value`, recording the path of each [`Always`]/[`Merge`] sentinel and
/// replacing it with its inner value. Recurses into the unwrapped inner value
/// so stacked wrappers (`Always<Merge<T>>`) record both paths.
fn strip_sentinels(
    value: &mut Value,
    path: &mut String,
    always_paths: &mut HashSet<String>,
    merge_paths: &mut HashSet<String>,
) {
    use crate::props::{ALWAYS_SENTINEL, MERGE_SENTINEL};

    // Unwrap any chain of sentinels at this position, recording each.
    loop {
        let kind = match value.as_object() {
            Some(map) if map.len() == 1 => {
                if map.contains_key(ALWAYS_SENTINEL) {
                    Some(true)
                } else if map.contains_key(MERGE_SENTINEL) {
                    Some(false)
                } else {
                    None
                }
            }
            _ => None,
        };
        let Some(is_always) = kind else { break };
        if is_always {
            always_paths.insert(path.clone());
        } else {
            merge_paths.insert(path.clone());
        }
        // Unwrap the single-entry sentinel object in place.
        if let Value::Object(map) = std::mem::take(value) {
            if let Some((_, inner)) = map.into_iter().next() {
                *value = inner;
            }
        }
    }

    match value {
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                let prev_len = path.len();
                path.push('/');
                escape_pointer_segment(path, k);
                strip_sentinels(v, path, always_paths, merge_paths);
                path.truncate(prev_len);
            }
        }
        Value::Array(arr) => {
            for (i, v) in arr.iter_mut().enumerate() {
                let prev_len = path.len();
                use std::fmt::Write;
                let _ = write!(path, "/{i}");
                strip_sentinels(v, path, always_paths, merge_paths);
                path.truncate(prev_len);
            }
        }
        _ => {}
    }
}

/// Append a key to a JSON-pointer path, escaping `~` and `/` per RFC 6901.
/// We don't strictly need RFC compliance internally, but it keeps paths
/// unambiguous when a key contains a `/`.
fn escape_pointer_segment(buf: &mut String, segment: &str) {
    for ch in segment.chars() {
        match ch {
            '~' => buf.push_str("~0"),
            '/' => buf.push_str("~1"),
            c => buf.push(c),
        }
    }
}

/// Apply partial-reload rules to the base value, drop/keep keys, and return.
pub async fn resolve(input: ResolveInput<'_>) -> ResolvedProps {
    let ResolveInput {
        req,
        component,
        base,
        ordinary,
        lazies,
        once,
        deferreds,
        merges,
        shared,
    } = input;

    // 1) merge shared under base
    let mut tree = match shared {
        Some(s) => merge_objects(s.value, base.value),
        None => base.value,
    };

    // 2) Insert builder-attached lazies/optionals/deferreds where appropriate.
    let partial_for_this_component =
        req.is_partial() && req.partial_component.as_deref() == Some(component);

    let only: &HashSet<String> = &req.partial_only;
    let except: &HashSet<String> = &req.partial_except;

    let mut deferred_groups: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (key, prop) in ordinary {
        if req.wants_prop(component, &key) || base.always_paths.contains(&format!("/{key}")) {
            set_top(&mut tree, &key, (prop.closure)().await);
        } else {
            remove_top(&mut tree, &key);
        }
    }

    let mut once_props = BTreeMap::new();
    for (key, prop) in once {
        let explicitly_requested = partial_for_this_component && only.contains(&key);
        let selected = !partial_for_this_component
            || (only.is_empty() || explicitly_requested) && !except.contains(&key);
        if selected {
            once_props.insert(
                prop.key.clone(),
                crate::page::OncePropMetadata { prop: key.clone() },
            );
        }
        let remembered = req.is_inertia && req.except_once_props.contains(&prop.key);
        if selected && (!remembered || explicitly_requested) {
            set_top(&mut tree, &key, (prop.closure)().await);
        } else {
            remove_top(&mut tree, &key);
        }
    }

    for (key, prop) in lazies {
        let include = if partial_for_this_component {
            only.contains(&key)
        } else {
            false // Lazy/Optional default to "not included" outside partials
        };
        if include && !except.contains(&key) {
            let v = (prop.closure)().await;
            set_top(&mut tree, &key, v);
        } else {
            remove_top(&mut tree, &key);
        }
    }

    for (key, prop) in deferreds {
        if partial_for_this_component && only.contains(&key) {
            // Second pass: resolve.
            let v = (prop.closure)().await;
            set_top(&mut tree, &key, v);
        } else if !partial_for_this_component {
            // First pass: advertise.
            deferred_groups
                .entry(prop.group.to_string())
                .or_default()
                .push(key.clone());
            remove_top(&mut tree, &key); // do not send a value
        } else {
            // Partial but not requested: drop.
            remove_top(&mut tree, &key);
        }
    }

    // 3) Apply only/except to non-special keys (only when this is a partial for THIS component).
    if partial_for_this_component {
        let always = &base.always_paths;
        // Keep keys that are either in `only` or marked Always; drop others.
        // Then apply except (Always survives except, by spec).
        if let Value::Object(map) = &mut tree {
            let keys: Vec<String> = map.keys().cloned().collect();
            for k in keys {
                let path = format!("/{k}");
                let is_always = always.contains(&path);
                let allowed = is_always || only.is_empty() || only.contains(&k);
                let excluded = !is_always && except.contains(&k);
                if !allowed || excluded {
                    map.remove(&k);
                }
            }
        }
    }

    // 4) Collect merge keys. Inertia's `mergeProps` is a list of top-level keys
    //    in the protocol wire format, so we only honor paths at depth 1.
    let mut merge_props: Vec<String> = merges.into_iter().collect();
    for path in &base.merge_paths {
        if let Some(k) = path.strip_prefix('/') {
            if !k.contains('/') {
                merge_props.push(k.to_string());
            }
        }
    }
    merge_props.sort();
    merge_props.dedup();

    ResolvedProps {
        props: tree,
        merge_props,
        deferred_props: deferred_groups,
        once_props,
    }
}

fn merge_objects(mut a: Value, b: Value) -> Value {
    match (&mut a, b) {
        (Value::Object(ao), Value::Object(bo)) => {
            for (k, v) in bo {
                ao.insert(k, v);
            }
            a
        }
        (_, b) => b,
    }
}

fn set_top(tree: &mut Value, key: &str, v: Value) {
    if !tree.is_object() {
        *tree = Value::Object(Map::new());
    }
    if let Value::Object(map) = tree {
        map.insert(key.to_string(), v);
    }
}

fn remove_top(tree: &mut Value, key: &str) {
    if let Value::Object(map) = tree {
        map.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props::closure::{DeferredProp, LazyProp};
    use serde_json::json;

    fn empty_base(v: Value) -> SerializedBase {
        SerializedBase {
            value: v,
            always_paths: HashSet::new(),
            merge_paths: HashSet::new(),
        }
    }

    fn req_full() -> RequestInfo {
        RequestInfo::from_parts(http::Method::GET, "/".into(), &http::HeaderMap::new())
    }

    fn req_partial(component: &str, only: &[&str], except: &[&str]) -> RequestInfo {
        let mut r = req_full();
        r.is_inertia = true;
        r.partial_component = Some(component.into());
        r.partial_only = only.iter().map(|s| s.to_string()).collect();
        r.partial_except = except.iter().map(|s| s.to_string()).collect();
        r
    }

    #[tokio::test]
    async fn full_request_excludes_lazy() {
        let req = req_full();
        let mut lazies = HashMap::new();
        lazies.insert(
            "stats".into(),
            LazyProp {
                closure: Box::new(|| Box::pin(async { json!({"hits": 1}) })),
            },
        );
        let resolved = resolve(ResolveInput {
            req: &req,
            component: "Users/Index",
            base: empty_base(json!({"users": [1,2]})),
            ordinary: HashMap::new(),
            lazies,
            once: HashMap::new(),
            deferreds: HashMap::new(),
            merges: HashSet::new(),
            shared: None,
        })
        .await;
        assert_eq!(resolved.props, json!({"users": [1,2]}));
    }

    #[tokio::test]
    async fn partial_request_resolves_only_requested_lazy() {
        let req = req_partial("Users/Index", &["stats"], &[]);
        let mut lazies = HashMap::new();
        lazies.insert(
            "stats".into(),
            LazyProp {
                closure: Box::new(|| Box::pin(async { json!({"hits": 1}) })),
            },
        );
        let resolved = resolve(ResolveInput {
            req: &req,
            component: "Users/Index",
            base: empty_base(json!({"users": [1,2]})),
            ordinary: HashMap::new(),
            lazies,
            once: HashMap::new(),
            deferreds: HashMap::new(),
            merges: HashSet::new(),
            shared: None,
        })
        .await;
        // base "users" key dropped (not in only); lazy stats included.
        assert_eq!(resolved.props, json!({"stats": {"hits": 1}}));
    }

    #[tokio::test]
    async fn deferred_first_pass_advertises_group_drops_value() {
        let req = req_full();
        let mut deferreds = HashMap::new();
        deferreds.insert(
            "expensive".into(),
            DeferredProp {
                group: "dashboard",
                closure: Box::new(|| Box::pin(async { json!("never") })),
            },
        );
        let resolved = resolve(ResolveInput {
            req: &req,
            component: "Users/Index",
            base: empty_base(json!({"users": []})),
            ordinary: HashMap::new(),
            lazies: HashMap::new(),
            once: HashMap::new(),
            deferreds,
            merges: HashSet::new(),
            shared: None,
        })
        .await;
        assert_eq!(resolved.props, json!({"users": []}));
        assert_eq!(
            resolved.deferred_props.get("dashboard"),
            Some(&vec!["expensive".to_string()])
        );
    }

    #[tokio::test]
    async fn deferred_second_pass_resolves() {
        let req = req_partial("Users/Index", &["expensive"], &[]);
        let mut deferreds = HashMap::new();
        deferreds.insert(
            "expensive".into(),
            DeferredProp {
                group: "dashboard",
                closure: Box::new(|| Box::pin(async { json!(42) })),
            },
        );
        let resolved = resolve(ResolveInput {
            req: &req,
            component: "Users/Index",
            base: empty_base(json!({"users": []})),
            ordinary: HashMap::new(),
            lazies: HashMap::new(),
            once: HashMap::new(),
            deferreds,
            merges: HashSet::new(),
            shared: None,
        })
        .await;
        assert_eq!(resolved.props, json!({"expensive": 42}));
        assert!(resolved.deferred_props.is_empty());
    }

    #[tokio::test]
    async fn shared_props_merge_under_base() {
        let req = req_full();
        let shared = empty_base(json!({"app_name": "Acme", "users": "shared-wins-no"}));
        let resolved = resolve(ResolveInput {
            req: &req,
            component: "X",
            base: empty_base(json!({"users": "base-wins"})),
            ordinary: HashMap::new(),
            lazies: HashMap::new(),
            once: HashMap::new(),
            deferreds: HashMap::new(),
            merges: HashSet::new(),
            shared: Some(shared),
        })
        .await;
        assert_eq!(
            resolved.props,
            json!({"app_name": "Acme", "users": "base-wins"})
        );
    }

    #[tokio::test]
    async fn merges_recorded() {
        let req = req_full();
        let mut merges = HashSet::new();
        merges.insert("notifications".to_string());
        let resolved = resolve(ResolveInput {
            req: &req,
            component: "X",
            base: empty_base(json!({"notifications": ["a"]})),
            ordinary: HashMap::new(),
            lazies: HashMap::new(),
            once: HashMap::new(),
            deferreds: HashMap::new(),
            merges,
            shared: None,
        })
        .await;
        assert_eq!(resolved.merge_props, vec!["notifications"]);
    }

    #[tokio::test]
    async fn struct_wrappers_detected_via_serialize_tag_aware() {
        use crate::props::{Always, Merge};
        use serde::Serialize;

        #[derive(Serialize)]
        struct Page {
            users: Vec<&'static str>,
            cached: Always<i64>,
            notifs: Merge<Vec<&'static str>>,
        }

        let p = Page {
            users: vec!["a", "b"],
            cached: Always(42),
            notifs: Merge(vec!["x"]),
        };
        let base = serialize_tag_aware(&p).unwrap();
        assert!(base.always_paths.contains("/cached"));
        assert!(base.merge_paths.contains("/notifs"));

        // Partial reload that asks only for "users" — without Always, "cached" would be dropped.
        let req = req_partial("Users/Index", &["users"], &[]);
        let resolved = resolve(ResolveInput {
            req: &req,
            component: "Users/Index",
            base,
            ordinary: HashMap::new(),
            lazies: HashMap::new(),
            once: HashMap::new(),
            deferreds: HashMap::new(),
            merges: HashSet::new(),
            shared: None,
        })
        .await;

        // `cached` survives because it's tagged Always; `users` is included via `only`;
        // `notifs` is dropped (Merge does not imply Always-survival of partial filters).
        assert_eq!(resolved.props, json!({"users": ["a", "b"], "cached": 42}));
        // The merge path is reported in mergeProps regardless of whether the value
        // was sent this response (clients use it to decide merge semantics if/when
        // the value is sent).
        assert!(resolved.merge_props.contains(&"notifs".to_string()));
    }

    #[tokio::test]
    async fn json_macro_wrappers_are_detected_and_stripped() {
        // The headline fix: wrappers survive a trip through `serde_json::json!`.
        use crate::props::{Always, Merge};

        let value = json!({
            "users": ["a", "b"],
            "cached": Always(42),
            "notifs": Merge(["x"]),
        });
        let base = serialize_tag_aware(&value).unwrap();

        assert!(base.always_paths.contains("/cached"));
        assert!(base.merge_paths.contains("/notifs"));
        // Sentinels are stripped — the JSON sent to the client is clean.
        assert_eq!(
            base.value,
            json!({"users": ["a", "b"], "cached": 42, "notifs": ["x"]})
        );
    }

    #[tokio::test]
    async fn stacked_wrappers_record_both_paths_at_the_same_key() {
        use crate::props::{Always, Merge};

        #[derive(serde::Serialize)]
        struct Page {
            // Top-level key wrapped in both — wire format should treat it as
            // both always-on and merge.
            feed: Always<Merge<Vec<&'static str>>>,
        }

        let p = Page {
            feed: Always(Merge(vec!["a", "b"])),
        };
        let base = serialize_tag_aware(&p).unwrap();
        assert!(base.always_paths.contains("/feed"));
        assert!(base.merge_paths.contains("/feed"));
        assert_eq!(base.value, json!({"feed": ["a", "b"]}));
    }

    #[tokio::test]
    async fn nested_wrappers_are_stripped_but_do_not_affect_wire_format() {
        // Per the Inertia v3 protocol, only top-level keys can be merge/always.
        // Nested wrappers must still be stripped from the JSON so the client
        // never sees our sentinel, but they don't contribute to mergeProps.
        use crate::props::Merge;

        #[derive(serde::Serialize)]
        struct Page {
            user: User,
        }
        #[derive(serde::Serialize)]
        struct User {
            posts: Merge<Vec<&'static str>>,
        }

        let p = Page {
            user: User {
                posts: Merge(vec!["a"]),
            },
        };
        let base = serialize_tag_aware(&p).unwrap();
        // Path is recorded for diagnostic completeness, but the resolver only
        // promotes depth-1 paths into mergeProps.
        assert!(base.merge_paths.contains("/user/posts"));
        // Sentinel is gone from the wire value.
        assert_eq!(base.value, json!({"user": {"posts": ["a"]}}));

        let req = req_full();
        let resolved = resolve(ResolveInput {
            req: &req,
            component: "X",
            base,
            ordinary: HashMap::new(),
            lazies: HashMap::new(),
            once: HashMap::new(),
            deferreds: HashMap::new(),
            merges: HashSet::new(),
            shared: None,
        })
        .await;
        // No top-level merge keys despite the nested wrapper.
        assert!(resolved.merge_props.is_empty());
    }

    #[tokio::test]
    async fn always_survives_partial_except() {
        // Always wrappers must survive `X-Inertia-Except` per spec.
        let req = req_partial("Page", &[], &["cached", "users"]);
        let base = SerializedBase {
            value: json!({"cached": 42, "users": [1,2]}),
            always_paths: ["/cached".to_string()].into_iter().collect(),
            merge_paths: HashSet::new(),
        };
        let resolved = resolve(ResolveInput {
            req: &req,
            component: "Page",
            base,
            ordinary: HashMap::new(),
            lazies: HashMap::new(),
            once: HashMap::new(),
            deferreds: HashMap::new(),
            merges: HashSet::new(),
            shared: None,
        })
        .await;
        // `cached` survives (Always); `users` excluded.
        assert_eq!(resolved.props, json!({"cached": 42}));
    }

    #[tokio::test]
    async fn only_and_except_collision_on_same_key_drops_it() {
        // When a key appears in BOTH `only` and `except`, except wins — the key is dropped.
        // Inertia treats `except` as a strict filter applied after `only`.
        let req = req_partial("Page", &["a", "b"], &["b"]);
        let resolved = resolve(ResolveInput {
            req: &req,
            component: "Page",
            base: empty_base(json!({"a": 1, "b": 2, "c": 3})),
            ordinary: HashMap::new(),
            lazies: HashMap::new(),
            once: HashMap::new(),
            deferreds: HashMap::new(),
            merges: HashSet::new(),
            shared: None,
        })
        .await;
        // a: in `only` → kept. b: in both → dropped (except wins). c: not in `only` → dropped.
        assert_eq!(resolved.props, json!({"a": 1}));
    }
    #[tokio::test]
    async fn once_props_resolve_only_when_the_client_needs_a_value() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        for (cached, requested, cache_key, expected_calls) in [
            (false, None, "org:one", 1),
            (true, None, "org:one", 0),
            (true, Some("organisation"), "org:one", 1),
            (true, Some("other"), "org:one", 0),
            (true, None, "org:two", 1),
        ] {
            let mut req = req_full();
            req.is_inertia = true;
            if cached {
                req.except_once_props.insert("org:one".into());
            }
            if let Some(prop) = requested {
                req.partial_component = Some("Page".into());
                req.partial_only.insert(prop.into());
            }
            let calls = Arc::new(AtomicUsize::new(0));
            let resolver_calls = calls.clone();
            let shared = crate::SharedPropsData::new(json!({})).once_as(
                "organisation",
                cache_key,
                move || async move {
                    resolver_calls.fetch_add(1, Ordering::SeqCst);
                    json!({"name": "Current organisation"})
                },
            );
            let result = resolve(ResolveInput {
                req: &req,
                component: "Page",
                base: empty_base(json!({})),
                once: shared.once,
                ordinary: HashMap::new(),
                lazies: HashMap::new(),
                deferreds: HashMap::new(),
                merges: HashSet::new(),
                shared: None,
            })
            .await;
            assert_eq!(calls.load(Ordering::SeqCst), expected_calls);
            assert_eq!(
                result.props.get("organisation").is_some(),
                expected_calls == 1
            );
            assert_eq!(
                result.once_props.contains_key(cache_key),
                requested != Some("other")
            );
            if let Some(metadata) = result.once_props.get(cache_key) {
                assert_eq!(metadata.prop, "organisation");
            }
        }
    }
}
