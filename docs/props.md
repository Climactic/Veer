# Props

Props are the data of a page. Veer follows the [prop evaluation model](https://inertiajs.com/docs/v3/core-concepts/the-protocol#prop-evaluation-model) of the Inertia protocol: each prop has a category, and the category decides when the server resolves it and what metadata the page object carries.

## Overview

```rust,ignore
inertia
    .render("Users/Index", UsersIndexProps { users, notifications })
    .lazy("stats", || async { load_stats().await })
    .deferred("activity", "default", || async { load_activity().await })
    .once("plans", || async { load_plans().await })
    .prepend("notifications")
    .match_on("notifications.id")
```

Plain props (the struct) are always resolved. The builder methods attach **closure props**: the closure runs only when the response needs the value. A closure returns any `Serialize` value.

| Method | When the closure runs |
|---|---|
| `lazy(key, f)` / `optional(key, f)` | Only when a partial reload asks for the key |
| `deferred(key, group, f)` | Not on the first response; the client asks for it after the first render |
| `once(key, f)` | One time; the client remembers the value across pages |
| `prop(key, Prop)` | As the `Prop` says (see [Composing with `Prop`](#composing-with-prop)) |

## Partial reloads

The client can ask for a subset of the props of the page it is on (`router.reload({ only: ['users'] })`). Veer applies the rules of the protocol:

- `only` and `except` take dot paths: `only: ['user.name']` returns only that nested value.
- With only `except`, all other props are returned.
- `errors` is always returned.
- A value wrapped in [`Always`](#always-and-merge-wrappers) is always returned.
- If the component of the request is not the component of the response, the reload is a full visit.

## Deferred props

```rust,ignore
.deferred("activity", "default", || async { load_activity().await })
.deferred("related", "sidebar", || async { load_related().await })
```

The first response lists the keys under `deferredProps`, by group. The client then makes one request for each group. On the frontend, use `<Deferred data="activity" fallback={…}>`.

### Rescued props

A deferred prop that can fail should not fail the page. Use `Prop::try_new` with `.rescue()`:

```rust,ignore
.prop("permissions", Prop::try_new(|| async { load_permissions().await }).defer().rescue())
```

On `Err`, veer logs the error, leaves the prop out, and lists its key in `rescuedProps`. The client shows the `rescue` slot of `<Deferred>`. Without `.rescue()`, an `Err` gives a `500`.

## Once props

```rust,ignore
.once("plans", || async { load_plans().await })
```

The value is resolved one time. The client remembers it and sends its key in `X-Inertia-Except-Once-Props` on later requests, and veer then skips the closure. To share a once prop with every page, set it on the config:

```rust,ignore
let cfg = InertiaConfig::new()
    .share_once("countries", |_req| async { load_countries().await });
```

For a custom key, an expiry, or a forced refresh, use `Prop`: `.once_as("key")`, `.until(duration)`, `.fresh()`.

## Merging props

By default, a partial reload replaces a prop. A merge label tells the client to combine the new value with the one it has.

```rust,ignore
inertia
    .render("Feed", json!({ "posts": posts, "notifications": notifications, "chat": chat }))
    .merge("posts")                 // append
    .prepend("notifications")       // prepend
    .deep_merge("chat")             // merge objects recursively
    .match_on("posts.id")           // update an item in place when the id is the same
```

A dot path merges a nested array: `.merge("posts.data")`.

When the client sends `X-Inertia-Reset` for a prop (`router.reload({ reset: ['posts'] })`), veer sends the prop without its merge label, so the client replaces the value.

## Infinite scroll

```rust,ignore
.prop("posts", Prop::scroll(move || async move {
    let page = load_posts(page_no).await;
    (
        PostsPage { data: page.items },
        ScrollMetadata::paged("page", page_no, page.has_more),
    )
}))
```

The closure returns the page value and its cursor. The value holds the items under the wrapper key (`data` by default; change it with `.wrapper("items")`). Veer emits `scrollProps` and a merge label for `<key>.data`, and follows the client's append / prepend intent and reset. On the frontend, use `<InfiniteScroll data="posts">`.

`ScrollMetadata::paged(name, current, has_more)` is for numbered pages. For cursor pagination, build it by hand: `ScrollMetadata::new("cursor").current(cur).next(next).previous(prev)`.

## Composing with `Prop`

`Prop` is the general form. The categories compose, as they do in the protocol: a prop can be deferred and merged, or once and deferred.

| Constructor | Behavior |
|---|---|
| `Prop::new(f)` | Resolved on full visits, and on partial reloads that select it |
| `Prop::try_new(f)` | `f` returns `Result`. `Err` gives a `500`, or a rescued prop with `.rescue()` |
| `Prop::scroll(f)` | An infinite-scroll prop; `f` returns `(value, ScrollMetadata)` |

| Modifier | Behavior |
|---|---|
| `.optional()` | Resolve only on a partial reload that selects the prop |
| `.defer()` / `.group("name")` | Defer, in the `default` group or a named group |
| `.once()` / `.once_as("key")` / `.until(duration)` / `.fresh()` | Once behavior: custom key, expiry, forced refresh |
| `.merge()` / `.prepend()` / `.deep_merge()` | Merge behavior for the prop itself |
| `.append_at("data")` / `.prepend_at("data")` | Merge behavior for a nested path |
| `.match_on("id")` | Field that identifies an item, relative to the prop |
| `.rescue()` | Rescue an `Err` from `try_new` |
| `.wrapper("items")` | Wrapper key of a scroll prop |

A dot path as key puts the prop inside a nested object: `.prop("auth.permissions", …)`.

## Shared props

Shared props are sent with every page: the signed-in user, the app name, feature flags.

```rust,ignore
use veer::shared::shared_props_fn;

let cfg = InertiaConfig::new()
    .shared(shared_props_fn(|_req| async move {
        serde_json::json!({
            "auth": { "user": current_user().await },
            "app": { "name": "Acme" },
        })
    }));
```

A page prop with the same key wins. The page object lists the shared keys in `sharedProps`, which the client uses for instant visits. For request-specific data, implement the `SharedProps` trait; its method gets the `RequestInfo`.

## `Always` and `Merge` wrappers

Two wrapper types mark a value inside your props struct:

```rust,ignore
#[derive(serde::Serialize)]
struct DashboardProps {
    csrf_ready: Always<bool>,          // always sent, also on partial reloads
    notifications: Merge<Vec<Notice>>, // merged by the client
}
```

They work at any depth and through any serialization path (structs, `json!`, hand-built values). A nested wrapper acts at its dot path. On the TypeScript side they collapse to the inner type.

## Big integers

JavaScript numbers lose precision above 2^53. With big-integer support on, veer sends each integer outside the safe range as `{"$bigint": "…"}`, and the client (3.8.0 or later) turns it into a `BigInt`.

```rust,ignore
// For one response:
inertia.render("Orders/Show", props).preserve_big_integers(true)

// For every response:
let cfg = InertiaConfig::new().preserve_big_integers(true);
```

Integers inside the safe range stay normal numbers, so the same prop can arrive as a `number` or a `bigint`. Flash data gets the same treatment.
