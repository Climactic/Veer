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
let cfg = InertiaConfig::new().share(|req| {
    // Put in the request extensions by your auth middleware.
    let user = req.extension::<CurrentUser>().cloned();
    async move {
        serde_json::json!({
            "auth": { "user": user },
            "app": { "name": "Acme" },
        })
    }
});
```

The closure returns any `Serialize` value that serializes to an object, so a struct works too. It gets the `RequestInfo`: the URL, the method, the Inertia headers, and the request extensions (`req.extension::<T>()`).

The extensions are those that the request had when it reached `InertiaLayer`. A middleware that sets one must thus be outside it, which in axum means that its layer comes later:

```rust,ignore
let app = router()
    .layer(InertiaLayer::new(cfg))
    .layer(auth_layer);   // runs first; sets `CurrentUser`
```

A page prop with the same key wins. The page object lists the shared keys in `sharedProps`, which the client uses for instant visits. For a type of your own, implement the `SharedProps` trait and use `InertiaConfig::shared`.

## `Always` and `Merge` wrappers

Two wrapper types mark a value inside your props struct:

```rust,ignore
#[derive(serde::Serialize)]
struct DashboardProps {
    csrf_ready: Always<bool>,          // always sent, also on partial reloads
    notifications: Merge<Vec<Notice>>, // merged by the client
}
```

They work at any depth and through any serialization path (structs, `json!`, hand-built values). A nested wrapper acts at its dot path. A nested `Always` value is kept when the partial reload selects its parent (for example `only: ['auth']` with `except: ['auth.user']`); when the parent is not selected, the parent is not sent, because the client replaces top-level props as a whole. For data that every response must carry, put `Always` on a top-level prop. On the TypeScript side they collapse to the inner type.

## Big integers

JavaScript numbers lose precision above 2^53. With big-integer support on, veer sends each integer outside the safe range as `{"$bigint": "…"}`, and the client (3.8.0 or later) turns it into a `BigInt`.

```rust,ignore
// For one response:
inertia.render("Orders/Show", props).preserve_big_integers(true)

// For every response:
let cfg = InertiaConfig::new().preserve_big_integers(true);
```

Integers inside the safe range stay normal numbers, so the same prop can arrive as a `number` or a `bigint`. Flash data gets the same treatment.

## Application HTTP errors (fork)

Use `try_prop` and `try_optional` when the application's error implements Axum's
`IntoResponse`. Unlike `Prop::try_new`, these preserve its status, headers, and body.
Authorization of the route should run before declaring the response.

```rust,ignore
inertia.render("Estates/Show", json!({}))
    .try_prop("estate", move || async move { load_overview(actor, id).await })
    .try_optional("contact_picker", move || async move { search_contacts(actor, query).await })
```

`Prop::try_response(loader)` supplies the same behavior with composable modifiers.
`Prop::try_scroll(loader)` / `response.try_scroll(key, loader)` resolve the page
value and `ScrollMetadata` together. Neither the loader nor its metadata runs
when the prop is excluded. Optional props require an explicit `only` request;
an `except`-only request does not load them.

The fork retains `SharedPropsData`: its `.prop(key, Prop)` uses the same resolver
as page props. A page's explicit value or closure overrides a shared prop with
the same key. `.once_as()` and `.lazy()` are conveniences for scoped remembered
props and on-demand props.
