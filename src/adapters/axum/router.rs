//! `veer::Router` — a thin wrapper around [`axum::Router`] that names every
//! route and feeds the [`crate::bindings`] generator without forcing you to
//! duplicate the route table in a separate `register_routes!` block.
//!
//! ```no_run
//! # #[cfg(feature = "ts")] {
//! use axum::routing::{get, post, delete};
//! use veer::Method::*;
//!
//! async fn home() {}
//! async fn index() {}
//! async fn store() {}
//! async fn destroy() {}
//!
//! let app: axum::Router = veer::Router::new()
//!     .named_route(GET,    "home",           "/",          home)
//!     .named_route(GET,    "todos.index",    "/todos",     index)
//!     .named_route(POST,   "todos.store",    "/todos",     store)
//!     .named_route(DELETE, "todos.destroy",  "/todos/:id", destroy)
//!     .build();
//! # }
//! ```
//!
//! Multi-method paths (e.g. GET + POST on `/todos`) are merged into a single
//! [`axum::routing::MethodRouter`] automatically — no panic on duplicate path
//! registration.

use axum::handler::Handler;
use axum::routing::MethodRouter;
use indexmap::IndexMap;

#[cfg(feature = "ts")]
use crate::bindings::register_runtime_route;

/// HTTP method, used by [`Router::named_route`] both for routing and for
/// the generated TS bindings (`{ url, method } as const`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    /// `GET`
    GET,
    /// `POST`
    POST,
    /// `PUT`
    PUT,
    /// `PATCH`
    PATCH,
    /// `DELETE`
    DELETE,
    /// `HEAD`
    HEAD,
    /// `OPTIONS`
    OPTIONS,
}

impl Method {
    /// Lowercase string form (`"get"`, `"post"`, …) used in the TS bundle.
    pub fn as_str(self) -> &'static str {
        match self {
            Method::GET => "get",
            Method::POST => "post",
            Method::PUT => "put",
            Method::PATCH => "patch",
            Method::DELETE => "delete",
            Method::HEAD => "head",
            Method::OPTIONS => "options",
        }
    }

    fn into_router<H, T, S>(self, handler: H) -> MethodRouter<S>
    where
        H: Handler<T, S>,
        T: 'static,
        S: Clone + Send + Sync + 'static,
    {
        use axum::routing as r;
        match self {
            Method::GET => r::get(handler),
            Method::POST => r::post(handler),
            Method::PUT => r::put(handler),
            Method::PATCH => r::patch(handler),
            Method::DELETE => r::delete(handler),
            Method::HEAD => r::head(handler),
            Method::OPTIONS => r::options(handler),
        }
    }
}

/// Router builder that records named routes as it goes.
///
/// Internally accumulates one [`MethodRouter`] per path, merging method
/// handlers for the same path. Call [`Router::build`] to consume the
/// builder and get a regular [`axum::Router`] back, ready for `.with_state`
/// / `.layer` chaining.
pub struct Router<S = ()> {
    routes: IndexMap<&'static str, MethodRouter<S>>,
    names: Vec<NamedRoute>,
    raw: Vec<RawRoute<S>>,
}

#[cfg_attr(not(feature = "ts"), allow(dead_code))]
struct NamedRoute {
    name: &'static str,
    path: &'static str,
    method: &'static str,
}

struct RawRoute<S> {
    path: &'static str,
    router: MethodRouter<S>,
}

impl<S> Default for Router<S> {
    fn default() -> Self {
        Self {
            routes: IndexMap::new(),
            names: Vec::new(),
            raw: Vec::new(),
        }
    }
}

impl<S> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    /// Construct an empty router builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mount a named route and record its name + method for TS bindings.
    ///
    /// Same-path multi-method calls (e.g. GET and POST on `/todos`) are
    /// merged into one [`MethodRouter`] — no extra ceremony required.
    pub fn named_route<H, T>(
        mut self,
        method: Method,
        name: &'static str,
        path: &'static str,
        handler: H,
    ) -> Self
    where
        H: Handler<T, S>,
        T: 'static,
    {
        let mr = method.into_router(handler);
        match self.routes.swap_remove(path) {
            Some(prev) => {
                self.routes.insert(path, prev.merge(mr));
            }
            None => {
                self.routes.insert(path, mr);
            }
        }
        self.names.push(NamedRoute {
            name,
            path,
            method: method.as_str(),
        });
        self
    }

    /// Mount an unnamed route (escape hatch). Equivalent to [`axum::Router::route`];
    /// the path is NOT recorded in the TS bindings registry.
    pub fn route(mut self, path: &'static str, method_router: MethodRouter<S>) -> Self {
        self.raw.push(RawRoute {
            path,
            router: method_router,
        });
        self
    }

    /// Consume the builder, register the named routes in the bindings
    /// registry (under `ts` feature), and produce an [`axum::Router`].
    pub fn build(self) -> axum::Router<S> {
        #[cfg(feature = "ts")]
        for r in &self.names {
            register_runtime_route(r.name, r.path, r.method);
        }
        // Suppress unused-field warning when `ts` is off.
        #[cfg(not(feature = "ts"))]
        let _ = &self.names;

        let mut r = axum::Router::new();
        for (path, mr) in self.routes {
            r = r.route(path, mr);
        }
        for raw in self.raw {
            r = r.route(raw.path, raw.router);
        }
        r
    }
}
