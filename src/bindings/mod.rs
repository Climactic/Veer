//! End-to-end TypeScript bindings: generate a single `.ts` bundle from
//! Rust prop structs and registered routes.
//!
//! Inspired by Laravel's Ziggy / Wayfinder: downstream apps mark their prop
//! structs with [`register_page!`](crate::register_page) and register routes
//! via [`register_routes!`](crate::register_routes), then a test calls
//! [`generate`] to emit a single bundled TypeScript file the frontend imports.
//!
//! Gated behind the `ts` feature.
//!
//! # Convention
//!
//! Apply `#[serde(rename_all = "camelCase")]` to your prop structs so the
//! wire format matches idiomatic JS. `ts-rs` mirrors serde's rename rules
//! into the generated TypeScript automatically — the same Rust struct then
//! drives both serialization and the TS type.
//!
//! # Example
//!
//! ```ignore
//! use serde::Serialize;
//! use ts_rs::TS;
//!
//! #[derive(Serialize, TS)]
//! #[ts(export)]
//! #[serde(rename_all = "camelCase")]
//! struct UsersIndexProps {
//!     users: Vec<User>,
//! }
//! veer::register_page!(UsersIndexProps, "Users/Index");
//!
//! // In `src/bin/gen-bindings.rs`:
//! fn main() {
//!     veer::bindings::generate_split("./frontend/gen").unwrap();
//! }
//! ```
//!
//! Run with `cargo run --bin gen-bindings`. The codegen has to live in a
//! binary inside your own crate (not a standalone CLI) because route and
//! page registrations are link-time `inventory` submissions.

use std::any::TypeId;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::io;
use std::path::Path;

mod routes;
pub use routes::{register_runtime_route, registered_routes, RouteEntry};

/// Implemented by prop structs that back an Inertia page component.
///
/// Normally produced via [`register_page!`](crate::register_page). The
/// associated constant is the Inertia component name (the string passed to
/// `inertia.render(...)`).
pub trait InertiaPageProps {
    /// Inertia component name (e.g. `"Users/Index"`).
    const COMPONENT: &'static str;
}

/// Register a Rust prop struct as an Inertia page component for TS bindings.
///
/// The struct must already derive `serde::Serialize` and `ts_rs::TS`.
/// Pass `shared = SharedProps` to compose the generated page contract with the
/// shared resolver's Rust type without repeating those fields in every page.
///
/// # Example
/// ```ignore
/// use serde::Serialize;
/// use ts_rs::TS;
///
/// #[derive(Serialize, TS)]
/// #[ts(export)]
/// #[serde(rename_all = "camelCase")]
/// struct UsersIndexProps { users: Vec<User> }
///
/// veer::register_page!(UsersIndexProps, "Users/Index");
/// ```
#[macro_export]
macro_rules! register_page {
    ($ty:ty, $component:literal, shared = $shared:ty) => {
        impl $crate::bindings::InertiaPageProps for $ty {
            const COMPONENT: &'static str = $component;
        }
        $crate::__private::inventory::submit! {
            $crate::bindings::PageEntry {
                component: $component,
                ts_name: || <$ty as $crate::__private::ts_rs::TS>::ident(),
                collect_decls: |out| $crate::bindings::collect_page_decls::<$ty, $shared>(out),
            }
        }
    };
    ($ty:ty, $component:literal) => {
        impl $crate::bindings::InertiaPageProps for $ty {
            const COMPONENT: &'static str = $component;
        }
        $crate::__private::inventory::submit! {
            $crate::bindings::PageEntry {
                component: $component,
                ts_name: || <$ty as $crate::__private::ts_rs::TS>::ident(),
                collect_decls: |out| $crate::bindings::collect_decls::<$ty>(out),
            }
        }
    };
}

/// Register a Rust type for TypeScript generation without associating it with
/// an Inertia page component.
///
/// Use this for typed action payloads and other frontend contracts that are
/// not reachable through a registered page's props.
#[macro_export]
macro_rules! register_type {
    ($ty:ty) => {
        $crate::__private::inventory::submit! {
            $crate::bindings::TypeEntry {
                collect_decls: |out| $crate::bindings::collect_decls::<$ty>(out),
            }
        }
    };
}

/// An entry submitted by `#[derive(InertiaPage)]` into the link-time registry.
///
/// You normally never construct this by hand.
pub struct PageEntry {
    /// Inertia component name.
    pub component: &'static str,
    /// Returns the TS identifier of the props type (e.g. `"UsersIndexProps"`).
    pub ts_name: fn() -> String,
    /// Populates `out` with `(ts_name, decl)` pairs for this type and all of
    /// its transitive TS dependencies.
    pub collect_decls: fn(&mut HashMap<TypeId, (String, String)>),
}

inventory::collect!(PageEntry);

/// A standalone TypeScript binding submitted through [`register_type!`](crate::register_type).
///
/// You normally never construct this by hand.
pub struct TypeEntry {
    /// Populates the output with this type and its transitive dependencies.
    pub collect_decls: fn(&mut HashMap<TypeId, (String, String)>),
}

inventory::collect!(TypeEntry);

/// Walk a TS-derived type and all its dependencies, populating `out` with
/// `(ts_name, decl)` pairs. Used internally by the derive-emitted
/// `collect_decls` closures.
///
/// This is part of the public surface only because proc-macro output needs
/// to reference it; it isn't intended to be called by hand.
pub fn collect_decls<T: ts_rs::TS + 'static + ?Sized>(out: &mut HashMap<TypeId, (String, String)>) {
    let id = TypeId::of::<T>();
    if out.contains_key(&id) {
        return;
    }
    if T::output_path().is_some() {
        out.insert(id, (T::ident(), T::decl()));
    } else {
        // mark seen so we don't loop, but don't emit a decl
        out.insert(id, (String::new(), String::new()));
    }

    struct Visit<'a> {
        out: &'a mut HashMap<TypeId, (String, String)>,
    }
    impl ts_rs::TypeVisitor for Visit<'_> {
        fn visit<U: ts_rs::TS + 'static + ?Sized>(&mut self) {
            collect_decls::<U>(self.out);
        }
    }

    T::visit_dependencies(&mut Visit { out });
}

/// Collect a page contract composed of its own props and the configured shared props.
pub fn collect_page_decls<T: ts_rs::TS + 'static, S: ts_rs::TS + 'static>(
    out: &mut HashMap<TypeId, (String, String)>,
) {
    collect_decls::<T>(out);
    collect_decls::<S>(out);
    let own_props = T::inline();
    // ts-rs uses an empty index signature for empty Rust structs. Intersecting
    // that with shared fields would forbid those fields and break Vue's compiler.
    let contract = if own_props == "Record<string, never>" {
        S::ident()
    } else {
        format!("{} & {}", S::ident(), own_props)
    };
    out.insert(
        TypeId::of::<T>(),
        (T::ident(), format!("type {} = {};", T::ident(), contract)),
    );
}

/// Errors that can occur while emitting the bindings file.
#[derive(Debug, thiserror::Error)]
pub enum GenerateError {
    /// Failure writing the output file.
    #[error("write: {0}")]
    Io(#[from] io::Error),
}

/// Generate the bundled `inertia.gen.ts` at `out`.
///
/// The emitted file contains:
/// - A header marker (`// veer auto-generated`)
/// - The canonical `PageObject<P>` envelope type
/// - All registered prop type declarations
/// - A discriminated `Pages` union mapping `component` literal → props type
/// - A nested `routes` object with Wayfinder-style URL builders for every
///   registered route (each leaf is callable + `.url` + `.form`)
///
/// Parent directories are created if missing.
///
/// For per-controller file splitting (one TS module per route group),
/// use [`generate_split`] instead.
pub fn generate(out: impl AsRef<Path>) -> Result<(), GenerateError> {
    let out = out.as_ref();
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = render();
    std::fs::write(out, body)?;
    Ok(())
}

/// Shortcut for `Split::new(dir).generate()` — Wayfinder-style split output
/// with default subdir (`actions/`) and no filename prefix/suffix.
///
/// For customization (rename the `actions/` subdir, add a `-controller`
/// suffix to filenames, …) construct a [`Split`] builder explicitly.
pub fn generate_split(dir: impl AsRef<Path>) -> Result<(), GenerateError> {
    Split::new(dir).generate()
}

/// Builder for Wayfinder-style split output: one TS file per controller
/// (first dot-segment of the route name), plus an `index.ts` with the
/// protocol envelope, prop types, and the `Pages` discriminated union.
///
/// Default layout (no customization):
///
/// ```text
/// <dir>/
///   index.ts                  protocol types, Pages, prop types, action re-exports
///   actions/
///     users.ts                routes whose name starts with "users."
///     posts.ts                routes whose name starts with "posts."
///     _root.ts                routes without a dot (e.g. "home")
/// ```
///
/// Frontend imports:
///
/// ```ts
/// import { users, type UsersIndexProps } from "./gen";
/// users.show({ id: 1 });        // { url: "/users/1", method: "get" }
/// users.show.url({ id: 1 });    // "/users/1"
/// users.show.form({ id: 1 });   // { action: "/users/1", method: "get" }
/// ```
///
/// # Example with customization
///
/// ```no_run
/// # #[cfg(feature = "ts")] {
/// veer::bindings::Split::new("./frontend/gen")
///     .actions_dir("controllers")    // -> frontend/gen/controllers/
///     .file_suffix("-controller")    // -> users-controller.ts
///     .generate()
///     .unwrap();
/// # }
/// ```
///
/// Existing files inside the actions directory are NOT cleaned up. Stale
/// files can appear if you rename or remove a controller — delete the
/// directory manually before regenerating if that matters.
pub struct Split {
    dir: std::path::PathBuf,
    actions_dir: String,
    file_prefix: String,
    file_suffix: String,
}

impl Split {
    /// Start a new split-generation config rooted at `dir`.
    pub fn new(dir: impl AsRef<Path>) -> Self {
        Self {
            dir: dir.as_ref().to_path_buf(),
            actions_dir: "actions".into(),
            file_prefix: String::new(),
            file_suffix: String::new(),
        }
    }

    /// Name of the subdirectory inside `dir` that holds per-controller
    /// files. Default `"actions"`. Set to `""` to write controller files
    /// directly alongside `index.ts`.
    pub fn actions_dir(mut self, name: impl Into<String>) -> Self {
        self.actions_dir = name.into();
        self
    }

    /// String prepended to every controller filename (before the `.ts`).
    /// E.g. `file_prefix("Resource")` → `ResourceUsers.ts`.
    pub fn file_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.file_prefix = prefix.into();
        self
    }

    /// String appended to every controller filename (before the `.ts`).
    /// E.g. `file_suffix("-controller")` → `users-controller.ts`.
    pub fn file_suffix(mut self, suffix: impl Into<String>) -> Self {
        self.file_suffix = suffix.into();
        self
    }

    /// Write the bindings.
    pub fn generate(&self) -> Result<(), GenerateError> {
        let actions_path = if self.actions_dir.is_empty() {
            self.dir.clone()
        } else {
            self.dir.join(&self.actions_dir)
        };
        std::fs::create_dir_all(&actions_path)?;

        let controllers = routes::render_per_controller();

        for (name, body) in &controllers {
            let mut file = String::new();
            file.push_str(HEADER);
            file.push('\n');
            file.push_str(body);
            let filename = format!("{}{}{}.ts", self.file_prefix, name, self.file_suffix);
            std::fs::write(actions_path.join(filename), file)?;
        }

        let mut index = String::new();
        index.push_str(HEADER);
        index.push('\n');
        index.push_str(PROTOCOL_TYPES);
        index.push('\n');
        index.push_str(&render_prop_decls_and_pages());
        index.push('\n');
        for name in controllers.keys() {
            let rel = if self.actions_dir.is_empty() {
                format!("./{}{}{}", self.file_prefix, name, self.file_suffix)
            } else {
                format!(
                    "./{}/{}{}{}",
                    self.actions_dir, self.file_prefix, name, self.file_suffix
                )
            };
            let _ = writeln!(index, "export * as {name} from \"{rel}\";");
        }
        std::fs::write(self.dir.join("index.ts"), index)?;
        Ok(())
    }
}

/// Build the single-bundle output as a string. Exposed for users who want
/// to embed the output in a custom build script.
pub fn render() -> String {
    let mut s = String::new();
    s.push_str(HEADER);
    s.push('\n');
    s.push_str(PROTOCOL_TYPES);
    s.push('\n');
    s.push_str(&render_prop_decls_and_pages());
    s.push('\n');
    s.push_str(&routes::render_routes());
    s
}

fn render_prop_decls_and_pages() -> String {
    let mut s = String::new();

    let mut decls: HashMap<TypeId, (String, String)> = HashMap::new();
    let mut pages: BTreeMap<&'static str, String> = BTreeMap::new();
    for entry in inventory::iter::<PageEntry>() {
        (entry.collect_decls)(&mut decls);
        pages.insert(entry.component, (entry.ts_name)());
    }
    for entry in inventory::iter::<TypeEntry>() {
        (entry.collect_decls)(&mut decls);
    }

    let mut ordered: Vec<(String, String)> = decls
        .into_values()
        .filter(|(name, _)| !name.is_empty())
        .collect();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));

    for (_, decl) in &ordered {
        s.push_str("export ");
        s.push_str(decl);
        s.push('\n');
        s.push('\n');
    }

    s.push_str("export type Pages =\n");
    if pages.is_empty() {
        s.push_str("  never;\n");
    } else {
        let mut iter = pages.iter().peekable();
        while let Some((component, ts_name)) = iter.next() {
            let _ = write!(s, "  | {{ component: {component:?}; props: {ts_name} }}");
            if iter.peek().is_some() {
                s.push('\n');
            } else {
                s.push_str(";\n");
            }
        }
    }
    s
}

const HEADER: &str = "// This file is auto-generated by veer. Do not edit manually.\n";

const PROTOCOL_TYPES: &str = r#"
export interface PageObject<P = Pages> {
  component: P extends { component: infer C } ? C : string;
  props: P extends { props: infer Props } ? Props : Record<string, unknown>;
  url: string;
  version: string;
  encryptHistory?: boolean;
  clearHistory?: boolean;
  mergeProps?: string[];
  prependProps?: string[];
  scrollProps?: Record<string, { pageName: string; currentPage: number | string; previousPage: number | string | null; nextPage: number | string | null; reset: boolean }>;
  resetMergeProps?: string[];
  deferredProps?: Record<string, string[]>;
  onceProps?: Record<string, { prop: string }>;
}

export type ErrorBag = Record<string, string>;

export interface Flash {
  errors: ErrorBag;
  bags: Record<string, unknown>;
}
"#;

#[cfg(test)]
mod shared_contract_tests {
    use super::*;
    #[derive(serde::Serialize, ts_rs::TS)]
    pub struct Shared {
        pub user: String,
    }
    #[derive(serde::Serialize, ts_rs::TS)]
    pub struct Page {
        pub title: String,
    }

    #[derive(serde::Serialize, ts_rs::TS)]
    struct EmptyPage {}

    #[test]
    fn shared_only_page_has_no_empty_index_signature() {
        let mut declarations = HashMap::new();
        collect_page_decls::<EmptyPage, Shared>(&mut declarations);
        assert_eq!(
            declarations[&TypeId::of::<EmptyPage>()].1,
            "type EmptyPage = Shared;"
        );
    }

    #[test]
    fn shared_page_contract_collects_both_types() {
        let mut declarations = HashMap::new();
        collect_page_decls::<Page, Shared>(&mut declarations);
        assert_eq!(
            declarations[&TypeId::of::<Page>()].1,
            "type Page = Shared & { title: string, };"
        );
        assert!(declarations[&TypeId::of::<Shared>()]
            .1
            .contains("user: string"));
    }
}
