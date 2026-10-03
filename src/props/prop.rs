//! `Prop` — a closure-resolved prop with composable Inertia behaviors.

use serde::Serialize;
use serde_json::Value;
use std::fmt::Display;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

/// A boxed future returning a JSON value.
pub type BoxedJsonFuture = Pin<Box<dyn Future<Output = Value> + Send>>;

pub(crate) struct Loaded {
    pub value: Value,
    pub scroll: Option<ScrollMetadata>,
}

type Loader =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = Result<Loaded, String>> + Send>> + Send>;

/// When a prop is resolved on a full (non-partial) visit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Load {
    /// Resolved on each full visit.
    Eager,
    /// Never resolved on a full visit and never announced.
    Optional,
    /// Not resolved on a full visit; announced under `deferredProps[group]`.
    Deferred(String),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Once {
    pub key: Option<String>,
    pub ttl: Option<Duration>,
    pub fresh: bool,
}

/// Merge labels relative to the prop. An empty path means the prop itself.
#[derive(Debug, Clone, Default)]
pub(crate) struct MergeSpec {
    pub append: Vec<String>,
    pub prepend: Vec<String>,
    pub deep: bool,
    pub match_on: Vec<String>,
}

/// Cursor data for an infinite-scroll prop (`scrollProps`).
///
/// Page values are JSON so that they can hold page numbers or cursor strings.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrollMetadata {
    /// Name of the query parameter that holds the page.
    pub page_name: String,
    /// Page before the current one, or `null`.
    pub previous_page: Value,
    /// Page after the current one, or `null`.
    pub next_page: Value,
    /// The page that this response carries.
    pub current_page: Value,
}

impl ScrollMetadata {
    /// Metadata with all pages set to `null`.
    pub fn new(page_name: impl Into<String>) -> Self {
        Self {
            page_name: page_name.into(),
            previous_page: Value::Null,
            next_page: Value::Null,
            current_page: Value::Null,
        }
    }

    /// Metadata for 1-based numbered pages.
    pub fn paged(page_name: impl Into<String>, current: u64, has_more: bool) -> Self {
        Self::new(page_name)
            .previous((current > 1).then(|| current - 1))
            .current(current)
            .next(has_more.then(|| current + 1))
    }

    /// Set the previous page.
    pub fn previous(mut self, page: impl Into<Value>) -> Self {
        self.previous_page = page.into();
        self
    }

    /// Set the next page.
    pub fn next(mut self, page: impl Into<Value>) -> Self {
        self.next_page = page.into();
        self
    }

    /// Set the current page.
    pub fn current(mut self, page: impl Into<Value>) -> Self {
        self.current_page = page.into();
        self
    }
}

/// A prop that a closure resolves only when the response needs it.
///
/// Attach it with [`crate::InertiaResponse::prop`]. The modifiers compose, as
/// the Inertia categories do: `Prop::new(f).defer().once()` is a deferred once
/// prop.
pub struct Prop {
    pub(crate) loader: Loader,
    pub(crate) load: Load,
    pub(crate) once: Option<Once>,
    pub(crate) merge: MergeSpec,
    /// Wrapper key of a scroll prop (the array that the client merges).
    pub(crate) scroll_wrapper: Option<String>,
    pub(crate) rescue: bool,
}

impl Prop {
    fn from_loader(loader: Loader) -> Self {
        Self {
            loader,
            load: Load::Eager,
            once: None,
            merge: MergeSpec::default(),
            scroll_wrapper: None,
            rescue: false,
        }
    }

    /// A prop resolved on each full visit, and on partial reloads that select it.
    pub fn new<F, Fut>(f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Value> + Send + 'static,
    {
        Self::from_loader(Box::new(|| {
            Box::pin(async {
                Ok(Loaded {
                    value: f().await,
                    scroll: None,
                })
            })
        }))
    }

    /// A prop that can fail. On `Err` the response is a `500`, unless the prop
    /// has [`Self::rescue`].
    pub fn try_new<F, Fut, E>(f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<Value, E>> + Send + 'static,
        E: Display,
    {
        Self::from_loader(Box::new(|| {
            Box::pin(async {
                match f().await {
                    Ok(value) => Ok(Loaded {
                        value,
                        scroll: None,
                    }),
                    Err(e) => Err(e.to_string()),
                }
            })
        }))
    }

    /// An infinite-scroll prop. The closure returns the page value (an object
    /// that holds the items under the wrapper key, `data` by default) and its
    /// cursor metadata.
    pub fn scroll<F, Fut>(f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = (Value, ScrollMetadata)> + Send + 'static,
    {
        let mut prop = Self::from_loader(Box::new(|| {
            Box::pin(async {
                let (value, metadata) = f().await;
                Ok(Loaded {
                    value,
                    scroll: Some(metadata),
                })
            })
        }));
        prop.scroll_wrapper = Some("data".into());
        prop
    }

    /// Rescue a failure of a [`Self::try_new`] prop: the error is logged, the
    /// prop is left out of `props`, and its key is listed in `rescuedProps`, so
    /// that the client shows the `rescue` slot of `<Deferred>`.
    pub fn rescue(mut self) -> Self {
        self.rescue = true;
        self
    }

    /// Resolve only when a partial reload selects the prop.
    pub fn optional(mut self) -> Self {
        self.load = Load::Optional;
        self
    }

    /// Defer the prop: the client fetches it after the first render (group `default`).
    pub fn defer(self) -> Self {
        self.group("default")
    }

    /// Defer the prop in a named group. The client makes one request per group.
    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.load = Load::Deferred(group.into());
        self
    }

    /// Resolve once; the client remembers the value across pages.
    pub fn once(mut self) -> Self {
        self.once.get_or_insert_with(Once::default);
        self
    }

    /// Resolve once under a custom key, shared by all pages that use the key.
    pub fn once_as(mut self, key: impl Into<String>) -> Self {
        self.once.get_or_insert_with(Once::default).key = Some(key.into());
        self
    }

    /// Resolve once; the remembered value expires after `ttl`.
    pub fn until(mut self, ttl: Duration) -> Self {
        self.once.get_or_insert_with(Once::default).ttl = Some(ttl);
        self
    }

    /// Send a fresh value although the client already holds this once prop.
    pub fn fresh(mut self) -> Self {
        self.once.get_or_insert_with(Once::default).fresh = true;
        self
    }

    /// The client appends the value to its existing state (`mergeProps`).
    pub fn merge(self) -> Self {
        self.append_at("")
    }

    /// The client prepends the value to its existing state (`prependProps`).
    pub fn prepend(self) -> Self {
        self.prepend_at("")
    }

    /// The client deep-merges the value into its existing state (`deepMergeProps`).
    pub fn deep_merge(mut self) -> Self {
        self.merge.deep = true;
        self
    }

    /// The client appends at a nested path, e.g. `data`.
    pub fn append_at(mut self, path: impl Into<String>) -> Self {
        self.merge.append.push(path.into());
        self
    }

    /// The client prepends at a nested path, e.g. `data`.
    pub fn prepend_at(mut self, path: impl Into<String>) -> Self {
        self.merge.prepend.push(path.into());
        self
    }

    /// Field that identifies an item during a merge, relative to the prop
    /// (`id`, or `data.id` for a nested array).
    pub fn match_on(mut self, field: impl Into<String>) -> Self {
        self.merge.match_on.push(field.into());
        self
    }

    /// Wrapper key of a [`Self::scroll`] prop (default `data`). No effect on
    /// other props.
    pub fn wrapper(mut self, wrapper: impl Into<String>) -> Self {
        if let Some(current) = &mut self.scroll_wrapper {
            *current = wrapper.into();
        }
        self
    }
}
