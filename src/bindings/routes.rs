//! Named-route registry and Wayfinder-style TS codegen.
//!
//! Apps register named routes via [`register_route!`](crate::register_route)
//! or [`register_routes!`](crate::register_routes). Each entry carries an
//! HTTP method, a dotted name, and an axum path pattern. The bindings
//! generator turns the registry into a per-controller TS module (or a
//! single-bundle namespace tree, depending on which generate function is
//! used).
//!
//! Output shape per route — each "leaf" is a callable enriched (via
//! `Object.assign`) with helper properties:
//!
//! ```ts
//! show(params)        // → { url: "/users/1", method: "get" } (definition)
//! show.url(params)    // → "/users/1"                          (string)
//! show.form(params)   // → { action: "/users/1", method: "get" } (form props)
//! ```

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// A named route submitted into the inventory.
///
/// Built by the [`register_route!`](crate::register_route) /
/// [`register_routes!`](crate::register_routes) macros; rarely constructed by hand.
pub struct RouteEntry {
    /// Dotted name (e.g. `"users.show"`). The first segment becomes the
    /// controller (module / namespace); the remainder is the action.
    pub name: &'static str,
    /// Axum path pattern (e.g. `"/users/:id"` or `"/files/*rest"`).
    pub path: &'static str,
    /// HTTP method as a lowercase string (`"get"`, `"post"`, …).
    pub method: &'static str,
}

inventory::collect!(RouteEntry);

#[doc(hidden)]
#[macro_export]
macro_rules! __method_str {
    (GET)     => { "get" };
    (POST)    => { "post" };
    (PUT)     => { "put" };
    (PATCH)   => { "patch" };
    (DELETE)  => { "delete" };
    (HEAD)    => { "head" };
    (OPTIONS) => { "options" };
}

/// Register a single named route at module scope.
///
/// The macro expands to an `inventory::submit!` block (item-level) so it
/// must appear at module scope, not inside a function. Mount the actual
/// axum handler the usual way (`Router::route(path, get(handler))`); this
/// registration only feeds the TS bindings generator.
///
/// # Examples
/// ```ignore
/// // Wayfinder-style: HTTP method is part of the definition.
/// veer::register_route!(GET    "users.show" => "/users/:id");
/// veer::register_route!(POST   "users.store" => "/users");
/// veer::register_route!(DELETE "users.destroy" => "/users/:id");
///
/// // Back-compat: no method = GET.
/// veer::register_route!("users.index", "/users");
/// ```
#[macro_export]
macro_rules! register_route {
    ($method:ident $name:literal => $path:literal) => {
        $crate::__private::inventory::submit! {
            $crate::bindings::RouteEntry {
                name: $name,
                path: $path,
                method: $crate::__method_str!($method),
            }
        }
    };
    ($name:literal, $path:literal) => {
        $crate::register_route!(GET $name => $path);
    };
}

/// Register many named routes in a single block at module scope.
///
/// # Example
/// ```ignore
/// veer::register_routes! {
///     GET    "users.index"   => "/users",
///     GET    "users.show"    => "/users/:id",
///     POST   "users.store"   => "/users",
///     PATCH  "users.update"  => "/users/:id",
///     DELETE "users.destroy" => "/users/:id",
/// }
/// ```
///
/// The bare `"name" => "/path"` form (no method keyword) still works and
/// defaults to GET, for migration from earlier versions.
#[macro_export]
macro_rules! register_routes {
    // Wayfinder-style: each entry is `METHOD "name" => "/path"`.
    ( $( $method:ident $name:literal => $path:literal ),* $(,)? ) => {
        $( $crate::register_route!($method $name => $path); )*
    };
    // Back-compat: each entry is `"name" => "/path"` (defaults to GET).
    ( $( $name:literal => $path:literal ),* $(,)? ) => {
        $( $crate::register_route!(GET $name => $path); )*
    };
}

/// One action in the routes tree.
struct Action {
    path: &'static str,
    method: &'static str,
}

/// Recursive node: either a sub-namespace (children) or a leaf action.
#[derive(Default)]
struct Node {
    children: BTreeMap<String, Node>,
    leaf: Option<Action>,
}

fn build_tree() -> Node {
    let mut root = Node::default();
    for entry in inventory::iter::<RouteEntry>() {
        let mut node = &mut root;
        let segments: Vec<&str> = entry.name.split('.').collect();
        for (i, seg) in segments.iter().enumerate() {
            node = node.children.entry((*seg).to_string()).or_default();
            if i == segments.len() - 1 {
                node.leaf = Some(Action {
                    path: entry.path,
                    method: entry.method,
                });
            }
        }
    }
    root
}

/// Render a single bundled `export const routes = {...}` object (for the
/// non-split [`super::generate`] mode).
pub(super) fn render_routes() -> String {
    let root = build_tree();
    let mut s = String::new();
    s.push_str("export const routes = ");
    write_node(&mut s, &root, 0);
    s.push_str(";\n");
    s
}

/// Group routes by their first dot-segment (the "controller") and render
/// each group's TS module body. Returns a stable, alphabetized map of
/// `controller_name → rendered_module_source`.
///
/// Routes without a dot land under the special `_root` controller.
pub(super) fn render_per_controller() -> BTreeMap<String, String> {
    let root = build_tree();
    let mut groups: BTreeMap<String, Node> = BTreeMap::new();
    for (top, sub) in root.children {
        if sub.leaf.is_some() && sub.children.is_empty() {
            groups
                .entry("_root".to_string())
                .or_default()
                .children
                .insert(top, sub);
        } else {
            groups.insert(top, sub);
        }
    }

    groups
        .into_iter()
        .map(|(name, node)| {
            let mut s = String::new();
            for (action_name, child) in &node.children {
                let _ = write!(s, "export const {} = ", js_ident(action_name));
                write_node(&mut s, child, 0);
                s.push_str(";\n");
            }
            (name, s)
        })
        .collect()
}

fn write_node(s: &mut String, node: &Node, depth: usize) {
    if let Some(action) = &node.leaf {
        write_leaf(s, action, depth);
        return;
    }

    let pad = "  ".repeat(depth);
    let inner_pad = "  ".repeat(depth + 1);
    s.push_str("{\n");
    let len = node.children.len();
    for (i, (name, child)) in node.children.iter().enumerate() {
        let _ = write!(s, "{}{}: ", inner_pad, js_key(name));
        write_node(s, child, depth + 1);
        if i + 1 < len {
            s.push(',');
        }
        s.push('\n');
    }
    let _ = write!(s, "{}}}", pad);
}

/// Emit one leaf as `Object.assign(callable, { url, form })`.
fn write_leaf(s: &mut String, action: &Action, depth: usize) {
    let pad = "  ".repeat(depth);
    let inner_pad = "  ".repeat(depth + 1);
    let params = parse_params(action.path);

    s.push_str("Object.assign(\n");

    // Main callable: (params) => ({ url, method } as const)
    let _ = write!(s, "{}", inner_pad);
    write_arrow(s, &params, |s| {
        s.push_str("({ url: ");
        write_url_template(s, action.path, &params);
        let _ = write!(s, ", method: {:?} }} as const)", action.method);
    });
    s.push_str(",\n");

    // Helper bag
    let _ = write!(s, "{}{{\n", inner_pad);

    // .url(params) → string
    let helper_pad = "  ".repeat(depth + 2);
    let _ = write!(s, "{}url: ", helper_pad);
    write_arrow(s, &params, |s| {
        write_url_template(s, action.path, &params);
    });
    s.push_str(",\n");

    // .form(params) → { action, method } as const
    let _ = write!(s, "{}form: ", helper_pad);
    write_arrow(s, &params, |s| {
        s.push_str("({ action: ");
        write_url_template(s, action.path, &params);
        let _ = write!(s, ", method: {:?} }} as const)", action.method);
    });
    s.push_str(",\n");

    let _ = write!(s, "{}}},\n", inner_pad);
    let _ = write!(s, "{})", pad);
}

fn write_arrow<F: FnOnce(&mut String)>(s: &mut String, params: &[&str], body: F) {
    if params.is_empty() {
        s.push_str("() => ");
    } else {
        s.push_str("(params: { ");
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                s.push_str("; ");
            }
            let _ = write!(s, "{}: string | number", p);
        }
        s.push_str(" }) => ");
    }
    body(s);
}

fn write_url_template(s: &mut String, path: &str, params: &[&str]) {
    if params.is_empty() {
        let _ = write!(s, "{:?}", path);
        return;
    }
    s.push('`');
    for seg in path.split('/') {
        if seg.is_empty() {
            continue;
        }
        s.push('/');
        if let Some(name) = seg.strip_prefix(':') {
            let _ = write!(s, "${{params.{}}}", name);
        } else if let Some(name) = seg.strip_prefix('*') {
            let _ = write!(s, "${{params.{}}}", name);
        } else {
            s.push_str(seg);
        }
    }
    s.push('`');
}

fn parse_params(path: &str) -> Vec<&str> {
    path.split('/')
        .filter_map(|seg| seg.strip_prefix(':').or_else(|| seg.strip_prefix('*')))
        .collect()
}

fn is_ident(name: &str) -> bool {
    !name.is_empty()
        && name.chars().next().unwrap().is_ascii_alphabetic()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// JavaScript reserved words that can't be used as a bare `export const`
/// identifier. Action names that hit this list are suffixed with `_` in
/// the generated TS — prefer Laravel-idiomatic names (`destroy`, `create`,
/// `store`, `update`, `edit`, `show`, `index`) to avoid the rename.
const JS_RESERVED: &[&str] = &[
    "break", "case", "catch", "class", "const", "continue", "debugger",
    "default", "delete", "do", "else", "enum", "export", "extends", "false",
    "finally", "for", "function", "if", "import", "in", "instanceof", "new",
    "null", "return", "super", "switch", "this", "throw", "true", "try",
    "typeof", "var", "void", "while", "with", "yield", "let", "static",
    "implements", "interface", "package", "private", "protected", "public",
    "await", "async",
];

fn js_key(name: &str) -> String {
    // Object keys can be reserved words or quoted strings — always safe.
    if is_ident(name) && !JS_RESERVED.contains(&name) {
        name.to_string()
    } else {
        format!("{:?}", name)
    }
}

fn js_ident(name: &str) -> String {
    let base: String = if is_ident(name) {
        name.to_string()
    } else {
        name.chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
            .collect()
    };
    if JS_RESERVED.contains(&base.as_str()) {
        format!("{}_", base)
    } else {
        base
    }
}
