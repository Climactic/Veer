//! Resolve a serialized props tree against request partial-reload rules.
//!
//! Follows the prop evaluation model of the Inertia v3 protocol; prop paths are
//! dot paths (`posts.data`), as on the wire.

use crate::page::{OnceEntry, ScrollEntry};
use crate::props::prop::{Load, Prop};
use crate::request::RequestInfo;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

/// Output of resolving a props tree.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct ResolvedProps {
    /// The final props JSON sent to the client.
    pub props: Value,
    /// `page.mergeProps`.
    pub merge_props: Vec<String>,
    /// `page.prependProps`.
    pub prepend_props: Vec<String>,
    /// `page.deepMergeProps`.
    pub deep_merge_props: Vec<String>,
    /// `page.matchPropsOn`.
    pub match_props_on: Vec<String>,
    /// Deferred groups → key list (only populated on full visits).
    pub deferred_props: BTreeMap<String, Vec<String>>,
    /// `page.rescuedProps`.
    pub rescued_props: Vec<String>,
    /// `page.scrollProps`.
    pub scroll_props: BTreeMap<String, ScrollEntry>,
    /// `page.onceProps`.
    pub once_props: BTreeMap<String, OnceEntry>,
}

/// A [`Prop::try_new`] closure failed and the prop has no [`Prop::rescue`].
#[derive(Debug, thiserror::Error)]
#[error("prop `{prop}` failed to resolve: {error}")]
pub struct PropError {
    /// The prop key.
    pub prop: String,
    /// The error that the closure returned.
    pub error: super::prop::LoadError,
}

/// Merge labels attached by path through the response builder.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct MergeLabels {
    /// Paths for `mergeProps`.
    pub append: BTreeSet<String>,
    /// Paths for `prependProps`.
    pub prepend: BTreeSet<String>,
    /// Paths for `deepMergeProps`.
    pub deep: BTreeSet<String>,
    /// Entries for `matchPropsOn`.
    pub match_on: BTreeSet<String>,
}

/// Inputs to the resolver.
pub struct ResolveInput<'a> {
    /// Parsed request info for this render.
    pub req: &'a RequestInfo,
    /// Component name (e.g. `"Users/Index"`).
    pub component: &'a str,
    /// Already-serialized base props from the user's struct (via custom serializer that records tags).
    pub base: SerializedBase,
    /// Shared props (serialized via same tag-aware serializer).
    pub shared: Option<SerializedBase>,
    /// Closure-resolved props by top-level key.
    pub props: HashMap<String, Prop>,
    /// Merge labels for values in `base`.
    pub merge: MergeLabels,
}

/// Result of serializing user props through the tag-aware serializer.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct SerializedBase {
    /// The serialized JSON value.
    pub value: Value,
    /// Dot paths (e.g. `"auth.user"`) that came from `Always` wrappers.
    pub always_paths: HashSet<String>,
    /// Same for `Merge` wrappers.
    pub merge_paths: HashSet<String>,
}

/// Serialize a `Serialize` value while collecting the dot paths of any
/// [`crate::props::Always`] and [`crate::props::Merge`] wrappers in the tree.
///
/// `Always<T>` and `Merge<T>` serialize as single-key sentinel objects under
/// standard serde. This function runs `serde_json::to_value`
/// then walks the resulting tree once, recording each sentinel's path and
/// replacing it with its inner value. Because the markers live in the JSON
/// tree itself, this works through any pathway — typed structs, `json!`,
/// hand-built `Value`s, mixed maps, etc.
pub fn serialize_tag_aware<T: Serialize>(value: &T) -> Result<SerializedBase, serde_json::Error> {
    let mut base = SerializedBase {
        value: serde_json::to_value(value)?,
        ..Default::default()
    };
    let mut path = String::new();
    strip_sentinels(
        &mut base.value,
        &mut path,
        &mut base.always_paths,
        &mut base.merge_paths,
    );
    Ok(base)
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
    let sentinels = crate::props::sentinels();

    // Unwrap any chain of sentinels at this position, recording each.
    loop {
        let kind = match value.as_object() {
            Some(map) if map.len() == 1 => {
                if map.contains_key(&sentinels.always) {
                    Some(true)
                } else if map.contains_key(&sentinels.merge) {
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

    let mut descend = |segment: &str, v: &mut Value| {
        let prev_len = path.len();
        if prev_len > 0 {
            path.push('.');
        }
        path.push_str(segment);
        strip_sentinels(v, path, always_paths, merge_paths);
        path.truncate(prev_len);
    };
    match value {
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                descend(k, v);
            }
        }
        Value::Array(arr) => {
            for (i, v) in arr.iter_mut().enumerate() {
                descend(&i.to_string(), v);
            }
        }
        _ => {}
    }
}

/// `true` if `path` is `prefix` or lies below it.
fn is_at_or_below(path: &str, prefix: &str) -> bool {
    path.strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
}

/// Partial-reload filters of one request. An empty header counts as absent.
struct Filter<'a> {
    partial: bool,
    only: Option<&'a HashSet<String>>,
    except: Option<&'a HashSet<String>>,
    reset: &'a HashSet<String>,
}

impl Filter<'_> {
    fn matches_only(&self, path: &str) -> bool {
        self.only
            .is_none_or(|only| only.iter().any(|o| is_at_or_below(path, o)))
    }

    fn matches_except(&self, path: &str) -> bool {
        self.except
            .is_some_and(|except| except.iter().any(|e| is_at_or_below(path, e)))
    }

    /// Whether a value at `path` is part of a partial response. A path that
    /// leads to an `only` entry (`user` for `user.name`) is kept as a container.
    fn includes(&self, path: &str) -> bool {
        let leads_to_only = || {
            self.only
                .is_some_and(|only| only.iter().any(|o| o != path && is_at_or_below(o, path)))
        };
        (self.matches_only(path) || leads_to_only()) && !self.matches_except(path)
    }

    /// Whether merge/once metadata for `path` is part of this response.
    fn emits_metadata(&self, path: &str) -> bool {
        !self.partial || (self.matches_only(path) && !self.matches_except(path))
    }

    /// Whether the client asked to reset `path` or one of its parents.
    fn is_reset(&self, path: &str) -> bool {
        self.reset.iter().any(|r| is_at_or_below(path, r))
    }

    fn prune(&self, map: &mut Map<String, Value>, prefix: &str, always: &HashSet<String>) {
        map.retain(|key, value| {
            let path = join(prefix, key);
            if always.contains(&path) {
                return true;
            }
            // An unselected parent is dropped with its nested `Always` values.
            // A parent with only some of its fields would replace the client's
            // copy, because the client replaces top-level props.
            if !self.includes(&path) {
                return false;
            }
            if let Value::Object(child) = value {
                self.prune(child, &path, always);
            }
            true
        });
    }
}

fn join(prefix: &str, segment: &str) -> String {
    if prefix.is_empty() {
        segment.to_string()
    } else if segment.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}.{segment}")
    }
}

/// Apply the prop evaluation model: filter the tree, run the closures that
/// this response needs, and collect the page-object metadata.
pub async fn resolve(input: ResolveInput<'_>) -> Result<ResolvedProps, PropError> {
    let ResolveInput {
        req,
        component,
        base,
        shared,
        props,
        merge,
    } = input;

    let filter = Filter {
        partial: req.partial_component.as_deref() == Some(component),
        only: Some(&req.partial_only).filter(|s| !s.is_empty()),
        except: Some(&req.partial_except).filter(|s| !s.is_empty()),
        reset: &req.reset,
    };

    // 1) Merge shared under base. `errors` is an always prop by protocol.
    let mut map = match base.value {
        Value::Object(map) => map,
        Value::Null => Map::new(),
        other => {
            tracing::error!(props = %other, "veer: page props must serialize to an object; ignored");
            Map::new()
        }
    };
    // A wrapper label is void when another value replaces the wrapped one: a
    // page prop over a shared prop, or a closure prop over a plain value.
    let prop_keys: Vec<String> = props.keys().cloned().collect();
    let replaced_by_prop = |path: &String| prop_keys.iter().any(|key| is_at_or_below(path, key));
    let mut always = base.always_paths;
    let mut wrapper_merges = base.merge_paths;
    if let Some(Value::Object(mut shared_map)) = shared.as_ref().map(|s| s.value.clone()) {
        let shared = shared.unwrap_or_default();
        let overridden = |path: &String| map.contains_key(path.split('.').next().unwrap_or(""));
        always.extend(shared.always_paths.into_iter().filter(|p| !overridden(p)));
        wrapper_merges.extend(shared.merge_paths.into_iter().filter(|p| !overridden(p)));
        shared_map.append(&mut map);
        map = shared_map;
    }
    always.retain(|p| !replaced_by_prop(p));
    always.insert("errors".to_string());
    let mut labels = merge;
    labels
        .append
        .extend(wrapper_merges.into_iter().filter(|p| !replaced_by_prop(p)));
    let map = &mut map;

    // 2) A closure prop replaces a plain value under the same key.
    for key in props.keys() {
        remove_path(map, key);
    }
    if filter.partial {
        filter.prune(map, "", &always);
    }

    let mut out = ResolvedProps::default();

    // 3) Labels for plain values.
    for (paths, target) in [
        (&labels.append, &mut out.merge_props),
        (&labels.prepend, &mut out.prepend_props),
        (&labels.deep, &mut out.deep_merge_props),
        (&labels.match_on, &mut out.match_props_on),
    ] {
        target.extend(
            paths
                .iter()
                .filter(|p| !filter.is_reset(p) && filter.emits_metadata(p))
                .cloned(),
        );
    }

    // 4) Closure props, in key order so that the metadata is deterministic.
    let mut props: Vec<(String, Prop)> = props.into_iter().collect();
    props.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, prop) in props {
        let Prop {
            loader,
            load,
            once,
            merge,
            scroll_wrapper,
            rescue,
        } = prop;

        let emits = filter.emits_metadata(&key);
        let collect_merge = |out: &mut ResolvedProps| {
            if filter.is_reset(&key) || !emits {
                return;
            }
            let mut merge = merge.clone();
            if let Some(wrapper) = &scroll_wrapper {
                if req.scroll_prepend {
                    merge.prepend.push(wrapper.clone());
                } else {
                    merge.append.push(wrapper.clone());
                }
            }
            if !merge.deep && merge.append.is_empty() && merge.prepend.is_empty() {
                return;
            }
            // A nested path replaces the label of the prop itself.
            if merge
                .append
                .iter()
                .chain(&merge.prepend)
                .any(|p| !p.is_empty())
            {
                merge.append.retain(|p| !p.is_empty());
                merge.prepend.retain(|p| !p.is_empty());
            }
            if merge.deep {
                out.deep_merge_props.push(key.clone());
            } else {
                out.merge_props
                    .extend(merge.append.iter().map(|p| join(&key, p)));
                out.prepend_props
                    .extend(merge.prepend.iter().map(|p| join(&key, p)));
            }
            out.match_props_on
                .extend(merge.match_on.iter().map(|f| join(&key, f)));
        };
        let once_key = once
            .as_ref()
            .map(|o| o.key.clone().unwrap_or_else(|| key.clone()));
        let collect_once = |out: &mut ResolvedProps| {
            if let (Some(once), Some(once_key), true) = (&once, &once_key, emits) {
                out.once_props.insert(
                    once_key.clone(),
                    OnceEntry {
                        prop: key.clone(),
                        expires_at: once.ttl.map(|ttl| {
                            let now = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default();
                            (now + ttl).as_millis() as u64
                        }),
                    },
                );
            }
        };
        // The client lists the once props that it holds; a partial reload ignores the list.
        let client_has_it = req.is_inertia
            && once_key.as_ref().is_some_and(|k| {
                req.except_once_props.contains(k) && !once.as_ref().is_some_and(|o| o.fresh)
            });

        if filter.partial {
            if !filter.includes(&key) || (matches!(load, Load::Optional) && filter.only.is_none()) {
                continue;
            }
        } else {
            match &load {
                Load::Optional => {
                    collect_merge(&mut out);
                    collect_once(&mut out);
                    continue;
                }
                Load::Deferred(group) => {
                    if !client_has_it {
                        out.deferred_props
                            .entry(group.clone())
                            .or_default()
                            .push(key.clone());
                    }
                    collect_merge(&mut out);
                    collect_once(&mut out);
                    continue;
                }
                Load::Eager if client_has_it => {
                    collect_once(&mut out);
                    continue;
                }
                Load::Eager => {}
            }
        }

        match loader().await {
            Ok(loaded) => {
                // A closure can return `Merge` / `Always` wrappers too.
                let mut value = loaded.value;
                let (mut path, mut always, mut merges) =
                    (key.clone(), HashSet::new(), HashSet::new());
                strip_sentinels(&mut value, &mut path, &mut always, &mut merges);
                out.merge_props.extend(
                    merges
                        .into_iter()
                        .filter(|p| !filter.is_reset(p) && filter.emits_metadata(p)),
                );
                insert_path(map, &key, value);
                collect_merge(&mut out);
                if let Some(metadata) = loaded.scroll {
                    out.scroll_props.insert(
                        key.clone(),
                        ScrollEntry {
                            metadata,
                            reset: req.reset.contains(&key),
                        },
                    );
                }
                collect_once(&mut out);
            }
            Err(error) if rescue => {
                tracing::error!(prop = %key, %error, "veer: prop failed to resolve; rescued");
                out.rescued_props.push(key);
            }
            Err(error) => return Err(PropError { prop: key, error }),
        }
    }

    for list in [
        &mut out.merge_props,
        &mut out.prepend_props,
        &mut out.deep_merge_props,
        &mut out.match_props_on,
    ] {
        list.sort();
        list.dedup();
    }
    out.props = Value::Object(std::mem::take(map));
    Ok(out)
}

fn remove_path(map: &mut Map<String, Value>, path: &str) {
    match path.split_once('.') {
        None => {
            map.remove(path);
        }
        Some((head, rest)) => {
            if let Some(Value::Object(child)) = map.get_mut(head) {
                remove_path(child, rest);
            }
        }
    }
}

/// Insert at a dot path. A parent that is not an object is replaced by one.
fn insert_path(map: &mut Map<String, Value>, path: &str, value: Value) {
    match path.split_once('.') {
        None => {
            map.insert(path.to_string(), value);
        }
        Some((head, rest)) => {
            let child = map.entry(head).or_insert_with(|| Value::Object(Map::new()));
            if !child.is_object() {
                *child = Value::Object(Map::new());
            }
            if let Value::Object(child) = child {
                insert_path(child, rest, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props::{Always, Merge, ScrollMetadata};
    use serde_json::json;
    use std::time::Duration;

    fn req_full() -> RequestInfo {
        RequestInfo::from_parts(http::Method::GET, "/".into(), &http::HeaderMap::new())
    }

    fn req_inertia() -> RequestInfo {
        let mut r = req_full();
        r.is_inertia = true;
        r
    }

    fn req_partial(only: &[&str], except: &[&str]) -> RequestInfo {
        let mut r = req_inertia();
        r.partial_component = Some("Page".into());
        r.partial_only = only.iter().map(|s| s.to_string()).collect();
        r.partial_except = except.iter().map(|s| s.to_string()).collect();
        r
    }

    fn value(v: Value) -> Prop {
        Prop::new(move || async move { v })
    }

    async fn run(req: &RequestInfo, base: Value, props: Vec<(&str, Prop)>) -> ResolvedProps {
        resolve(ResolveInput {
            req,
            component: "Page",
            base: serialize_tag_aware(&base).unwrap(),
            shared: None,
            props: props.into_iter().map(|(k, p)| (k.to_string(), p)).collect(),
            merge: MergeLabels::default(),
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn full_visit_skips_optional_and_announces_deferred() {
        let r = run(
            &req_full(),
            json!({"users": [1, 2]}),
            vec![
                ("stats", value(json!(1)).optional()),
                ("comments", value(json!(2)).defer()),
                ("related", value(json!(3)).group("sidebar")),
                ("now", value(json!(4))),
            ],
        )
        .await;
        assert_eq!(r.props, json!({"users": [1, 2], "now": 4}));
        assert_eq!(
            serde_json::to_value(&r.deferred_props).unwrap(),
            json!({"default": ["comments"], "sidebar": ["related"]})
        );
    }

    #[tokio::test]
    async fn partial_reload_resolves_selected_props_only() {
        let r = run(
            &req_partial(&["stats", "comments"], &[]),
            json!({"users": [1, 2], "errors": {}}),
            vec![
                ("stats", value(json!(1)).optional()),
                ("comments", value(json!(2)).defer()),
                ("other", value(json!(3)).optional()),
            ],
        )
        .await;
        // `errors` is an always prop.
        assert_eq!(r.props, json!({"stats": 1, "comments": 2, "errors": {}}));
        assert!(r.deferred_props.is_empty());
    }

    #[tokio::test]
    async fn partial_for_another_component_is_a_full_visit() {
        let mut req = req_partial(&["stats"], &[]);
        req.partial_component = Some("Other".into());
        let r = run(&req, json!({"users": 1}), vec![]).await;
        assert_eq!(r.props, json!({"users": 1}));
    }

    #[tokio::test]
    async fn except_only_partial_keeps_all_other_props() {
        let r = run(
            &req_partial(&[], &["b"]),
            json!({"a": 1, "b": 2}),
            vec![("lazy", value(json!(3)).optional())],
        )
        .await;
        assert_eq!(r.props, json!({"a": 1}));
    }

    #[tokio::test]
    async fn only_and_except_on_same_key_drops_it() {
        let r = run(
            &req_partial(&["a", "b"], &["b"]),
            json!({"a": 1, "b": 2}),
            vec![],
        )
        .await;
        assert_eq!(r.props, json!({"a": 1}));
    }

    #[tokio::test]
    async fn dot_paths_filter_nested_values() {
        let base = json!({"user": {"name": "A", "email": "E", "meta": {"x": 1, "y": 2}}, "z": 1});
        let r = run(
            &req_partial(&["user.name", "user.meta"], &["user.meta.y"]),
            base,
            vec![],
        )
        .await;
        assert_eq!(r.props, json!({"user": {"name": "A", "meta": {"x": 1}}}));
    }

    #[tokio::test]
    async fn always_survives_only_and_except_at_any_depth() {
        let base = json!({"a": 1, "flag": Always(true), "auth": {"user": Always("u"), "x": 1}});
        let r = run(&req_partial(&["a"], &["flag", "auth"]), base, vec![]).await;
        // An unselected parent is not sent: a partial `auth` would replace the
        // client's complete one.
        assert_eq!(r.props, json!({"a": 1, "flag": true}));

        let base = json!({"a": 1, "auth": {"user": Always("u"), "x": 1}});
        let r = run(
            &req_partial(&["auth.x", "auth.user"], &["auth.user"]),
            base,
            vec![],
        )
        .await;
        assert_eq!(r.props, json!({"auth": {"user": "u", "x": 1}}));
    }

    #[tokio::test]
    async fn shared_props_merge_under_base_and_wrappers_are_stripped() {
        let r = resolve(ResolveInput {
            req: &req_partial(&["users"], &[]),
            component: "Page",
            base: serialize_tag_aware(&json!({"users": [1]})).unwrap(),
            shared: Some(
                serialize_tag_aware(&json!({"users": [], "auth": Always("me"), "x": 1})).unwrap(),
            ),
            props: HashMap::new(),
            merge: MergeLabels::default(),
        })
        .await
        .unwrap();
        assert_eq!(r.props, json!({"users": [1], "auth": "me"}));
    }

    #[tokio::test]
    async fn merge_labels_from_wrappers_builder_and_props() {
        let mut merge = MergeLabels::default();
        merge.prepend.insert("notifications".into());
        merge.match_on.insert("notifications.id".into());
        let r = resolve(ResolveInput {
            req: &req_full(),
            component: "Page",
            base: serialize_tag_aware(
                &json!({"posts": Merge(vec![1]), "feed": {"data": Merge(vec![2])}, "notifications": []}),
            )
            .unwrap(),
            shared: None,
            props: HashMap::from([
                ("chat".to_string(), value(json!({})).deep_merge().match_on("data.id")),
                ("log".to_string(), value(json!([])).defer().merge()),
            ]),
            merge,
        })
        .await
        .unwrap();
        assert_eq!(r.props["posts"], json!([1]));
        assert_eq!(r.merge_props, ["feed.data", "log", "posts"]);
        assert_eq!(r.prepend_props, ["notifications"]);
        assert_eq!(r.deep_merge_props, ["chat"]);
        assert_eq!(r.match_props_on, ["chat.data.id", "notifications.id"]);
    }

    #[tokio::test]
    async fn reset_and_partial_filters_remove_merge_labels() {
        let base =
            json!({"posts": Merge(vec![1]), "tags": Merge(vec![2]), "users": Merge(vec![3])});
        let mut req = req_partial(&["posts", "tags"], &[]);
        req.reset.insert("posts".into());
        let r = run(&req, base, vec![]).await;
        assert_eq!(r.props, json!({"posts": [1], "tags": [2]}));
        assert_eq!(r.merge_props, ["tags"]);
    }

    #[tokio::test]
    async fn once_prop_is_skipped_when_client_holds_it() {
        let mut req = req_inertia();
        req.except_once_props.insert("plans".into());
        req.except_once_props.insert("shared-roles".into());
        let props = || {
            vec![
                ("plans", value(json!([1])).once()),
                ("roles", value(json!([2])).once_as("shared-roles")),
                ("rates", value(json!([3])).once().fresh()),
                ("zones", value(json!([4])).until(Duration::from_secs(60))),
            ]
        };
        let r = run(&req, json!({}), props()).await;
        assert_eq!(r.props, json!({"rates": [3], "zones": [4]}));
        assert_eq!(
            r.once_props["shared-roles"],
            OnceEntry {
                prop: "roles".into(),
                expires_at: None
            }
        );
        assert_eq!(r.once_props["plans"].prop, "plans");
        assert!(r.once_props["zones"].expires_at.unwrap() > 1_700_000_000_000);

        // First page load (not an Inertia request): the header is not trusted.
        let mut first = req_full();
        first.except_once_props.insert("plans".into());
        assert_eq!(
            run(&first, json!({}), props()).await.props["plans"],
            json!([1])
        );

        // A partial reload that selects the prop always resolves it.
        let mut partial = req_partial(&["plans"], &[]);
        partial.except_once_props.insert("plans".into());
        let r = run(&partial, json!({}), props()).await;
        assert_eq!(r.props, json!({"plans": [1]}));
        assert_eq!(r.once_props.keys().collect::<Vec<_>>(), ["plans"]);
    }

    #[tokio::test]
    async fn deferred_once_prop_is_not_announced_when_client_holds_it() {
        let mut req = req_inertia();
        req.except_once_props.insert("plans".into());
        let r = run(
            &req,
            json!({}),
            vec![("plans", value(json!(1)).defer().once())],
        )
        .await;
        assert!(r.deferred_props.is_empty());
        assert!(r.once_props.contains_key("plans"));
    }

    #[tokio::test]
    async fn failed_prop_is_rescued() {
        let failing = || {
            Prop::try_new(|| async { Err::<Value, _>("boom") })
                .defer()
                .rescue()
        };
        let r = run(
            &req_partial(&["perms"], &[]),
            json!({"a": 1}),
            vec![("perms", failing())],
        )
        .await;
        assert_eq!(r.props, json!({}));
        assert_eq!(r.rescued_props, ["perms"]);
        let r = run(&req_full(), json!({}), vec![("perms", failing())]).await;
        assert!(r.rescued_props.is_empty());
    }

    #[tokio::test]
    async fn prop_at_a_nested_path() {
        let props = || vec![("auth.perms", value(json!(["edit"])).defer())];
        let base = json!({"auth": {"user": "me", "perms": "stale"}});

        let r = run(&req_full(), base.clone(), props()).await;
        assert_eq!(r.props, json!({"auth": {"user": "me"}}));
        assert_eq!(r.deferred_props["default"], ["auth.perms"]);

        let r = run(&req_partial(&["auth.perms"], &[]), base, props()).await;
        assert_eq!(r.props, json!({"auth": {"perms": ["edit"]}}));
    }

    #[tokio::test]
    async fn fixed_sentinel_names_in_user_data_are_not_unwrapped() {
        let base = json!({"a": {"$$veer_merge$$": [1]}, "b": {"$$veer_always$$": 1}});
        let r = run(&req_full(), base.clone(), vec![]).await;
        assert_eq!(r.props, base);
        assert!(r.merge_props.is_empty());
    }

    #[tokio::test]
    async fn failed_prop_without_rescue_is_an_error() {
        let failing = Prop::try_new(|| async { Err::<Value, _>("boom") });
        let error = resolve(ResolveInput {
            req: &req_full(),
            component: "Page",
            base: SerializedBase::default(),
            shared: None,
            props: HashMap::from([("perms".to_string(), failing)]),
            merge: MergeLabels::default(),
        })
        .await
        .unwrap_err();
        assert_eq!(error.prop, "perms");
    }

    #[tokio::test]
    async fn replaced_values_lose_their_wrapper_labels() {
        let r = resolve(ResolveInput {
            req: &req_partial(&["a"], &[]),
            component: "Page",
            base: serialize_tag_aware(&json!({"notes": [1], "old": Merge(vec![1])})).unwrap(),
            shared: Some(
                serialize_tag_aware(&json!({"notes": Merge(vec![0]), "auth": Always(1)})).unwrap(),
            ),
            props: HashMap::from([("old".to_string(), value(json!([2])))]),
            merge: MergeLabels::default(),
        })
        .await
        .unwrap();
        assert!(r.merge_props.is_empty());
        assert_eq!(r.props, json!({"auth": 1}));
    }

    #[tokio::test]
    async fn non_object_props_keep_shared_props() {
        let r = resolve(ResolveInput {
            req: &req_full(),
            component: "Page",
            base: serialize_tag_aware(&json!([1, 2])).unwrap(),
            shared: Some(serialize_tag_aware(&json!({"errors": {}})).unwrap()),
            props: HashMap::new(),
            merge: MergeLabels::default(),
        })
        .await
        .unwrap();
        assert_eq!(r.props, json!({"errors": {}}));
    }

    #[tokio::test]
    async fn wrappers_in_a_closure_value_are_stripped() {
        let props = vec![(
            "feed",
            value(json!({"data": Merge(vec![1]), "x": Always(2)})),
        )];
        let r = run(&req_full(), json!({}), props).await;
        assert_eq!(r.props, json!({"feed": {"data": [1], "x": 2}}));
        assert_eq!(r.merge_props, ["feed.data"]);
    }

    #[tokio::test]
    async fn nested_merge_path_replaces_the_root_label() {
        let props = vec![
            (
                "a",
                value(json!({}))
                    .merge()
                    .append_at("data")
                    .match_on("data.id"),
            ),
            ("b", value(json!([])).match_on("id")),
        ];
        let r = run(&req_full(), json!({}), props).await;
        assert_eq!(r.merge_props, ["a.data"]);
        assert_eq!(r.match_props_on, ["a.data.id"]);
    }

    fn scroll() -> Prop {
        Prop::scroll(|| async {
            (
                json!({"data": [1, 2]}),
                ScrollMetadata::paged("page", 2, true),
            )
        })
    }

    #[tokio::test]
    async fn scroll_prop_emits_cursor_and_merge_label() {
        let r = run(&req_full(), json!({}), vec![("posts", scroll())]).await;
        assert_eq!(r.props, json!({"posts": {"data": [1, 2]}}));
        assert_eq!(r.merge_props, ["posts.data"]);
        assert_eq!(
            serde_json::to_value(&r.scroll_props).unwrap(),
            json!({"posts": {"pageName": "page", "previousPage": 1, "nextPage": 3, "currentPage": 2, "reset": false}})
        );
    }

    #[tokio::test]
    async fn scroll_prop_honors_prepend_intent_and_reset() {
        let mut req = req_partial(&["posts"], &[]);
        req.scroll_prepend = true;
        let r = run(&req, json!({}), vec![("posts", scroll())]).await;
        assert_eq!(r.prepend_props, ["posts.data"]);
        assert!(r.merge_props.is_empty());

        let mut req = req_partial(&["posts"], &[]);
        req.reset.insert("posts".into());
        let r = run(&req, json!({}), vec![("posts", scroll())]).await;
        assert!(r.merge_props.is_empty());
        assert!(r.scroll_props["posts"].reset);
    }

    #[tokio::test]
    async fn deferred_scroll_prop_has_no_cursor_on_full_visit() {
        let r = run(&req_full(), json!({}), vec![("posts", scroll().defer())]).await;
        assert_eq!(r.props, json!({}));
        assert!(r.scroll_props.is_empty());
        assert_eq!(r.deferred_props["default"], ["posts"]);
        assert_eq!(r.merge_props, ["posts.data"]);
    }
}
