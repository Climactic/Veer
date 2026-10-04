# TypeScript bindings

With the `ts` feature, veer generates TypeScript from your Rust code: the props type of each page, a `Pages` union, and one module of URL helpers for each controller. The types come from the same structs that produce the JSON, so they cannot drift.

```toml
[dependencies]
veer = { version = "0.2", features = ["ts"] }
ts-rs = "12"
```

## 1. Register pages

```rust,ignore
use serde::Serialize;
use ts_rs::TS;

#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UsersIndexProps {
    pub users: Vec<User>,
}
veer::register_page!(UsersIndexProps, "Users/Index");
```

`ts-rs` follows `#[serde(rename_all)]`, so the type and the wire format agree.

A registered struct knows its component, so a handler renders it with `inertia.page(props)` and the name is written one time:

```rust,ignore
async fn users_index(inertia: Inertia) -> impl IntoResponse {
    inertia.page(UsersIndexProps { users: load_users().await })
}
```

### Closure props

Props that a handler attaches as closures (`once`, `deferred`, `lazy`, `Prop::scroll`, …) are not fields of the props struct. Describe them in a second struct and pass it as the third argument:

```rust,ignore
#[derive(TS)]
#[ts(export)]
pub struct UsersIndexClosureProps {
    pub plans: Vec<Plan>,        // once prop
    #[ts(optional)]
    pub stats: Option<Stats>,    // deferred prop: absent on the first render
    pub feed: Feed,              // scroll prop
}
veer::register_page!(UsersIndexProps, "Users/Index", UsersIndexClosureProps);
```

The props type of the page is then `UsersIndexProps & UsersIndexClosureProps`. The closures return the same Rust types, so one definition serves both sides:

```rust,ignore
inertia
    .page(UsersIndexProps { users })
    .once("plans", || async { load_plans().await })                 // Vec<Plan>
    .deferred("stats", "default", || async { load_stats().await })  // Stats
```

Mark a deferred or lazy prop `#[ts(optional)]`, because it is absent until the client loads it.

## 2. Name the routes

`veer::Router` is a thin layer over `axum::Router` that records a name and a method for each route:

```rust,ignore
use veer::Method::*;

pub fn router() -> veer::Router<AppState> {
    veer::Router::new()
        .named_route(GET,    "users.index",   "/users",      users_index)
        .named_route(GET,    "users.show",    "/users/{id}", users_show)
        .named_route(GET,    "users.create",  "/users/new",  users_create)
        .named_route(POST,   "users.store",   "/users",      users_store)
        .named_route(PATCH,  "users.update",  "/users/{id}", users_update)
        .named_route(DELETE, "users.destroy", "/users/{id}", users_destroy)
}

// In main():
let app = router().build().with_state(state).layer(InertiaLayer::new(cfg));
```

`build()` returns a normal `axum::Router`. Two methods on one path are merged. `.route(path, method_router)` adds a route without a name.

The first part of the name (`users`) is the controller; the names after it follow the Laravel resource convention (`index`, `show`, `create`, `store`, `update`, `destroy`).

## 3. Generate

Add a small binary to your app crate:

```rust,ignore
// src/bin/gen-bindings.rs
fn main() {
    let _ = my_app::router().build();   // fills the route registry
    veer::bindings::generate_split("./frontend/gen").unwrap();
}
```

```bash
cargo run --bin gen-bindings
```

The generator must run inside your crate, because the pages and routes are registered by your compiled code.

```text
frontend/gen/
  index.ts              protocol types, Pages union, prop types, action re-exports
  actions/
    users.ts            index, show, create, store, update, destroy
    _root.ts            routes with no dot in the name
```

## 4. Use on the frontend

```tsx
import { Link, usePage } from "@inertiajs/react";
import { users, type Pages } from "./gen";

type Props = Extract<Pages, { component: "Users/Index" }>["props"];

export default function Index() {
  const { props } = usePage<Props>();
  return (
    <>
      <Link href={users.create.url()}>New user</Link>
      {props.users.map((u) => (
        <Link key={u.id} href={users.show.url({ id: u.id })}>{u.name}</Link>
      ))}
    </>
  );
}
```

Each action has three forms:

| Call | Returns |
|---|---|
| `users.show({ id: 1 })` | `{ url: "/users/1", method: "get" }` |
| `users.show.url({ id: 1 })` | `"/users/1"` |
| `users.show.form({ id: 1 })` | `{ action: "/users/1", method: "get" }` |

## Options

**Layout.** The `Split` builder changes the output layout:

```rust,ignore
veer::bindings::Split::new("./frontend/gen")
    .actions_dir("controllers")    // frontend/gen/controllers/
    .file_suffix("-controller")    // users-controller.ts
    .generate()?;
```

`actions_dir("")` writes the files next to `index.ts`. For one file, use `veer::bindings::generate("./frontend/inertia.gen.ts")`.

**Large integers.** `ts-rs` maps `u64` and `i64` to `bigint`. Turn on [big-integer support](props.md#big-integers) so that large values arrive as `BigInt`; values inside the safe range still arrive as `number`.

**Wrappers.** `Always<T>` and `Merge<T>` become `T`.

**Keep the output current.** The simplest way is to generate when the server starts in a debug build. A file is written only when its content changes, so Vite reloads only when a type or a route changed:

```rust,ignore
let app = router().build();
if cfg!(debug_assertions) {
    veer::bindings::generate_split("./frontend/gen")?;
}
```

Or generate before each commit, with [lefthook](https://github.com/evilmartians/lefthook):

```yaml
pre-commit:
  commands:
    veer-bindings:
      glob: "*.rs"
      run: cargo run -q --bin gen-bindings
      stage_fixed: true
```

In CI, run `cargo run --bin gen-bindings && git diff --exit-code`.

When you remove a controller, the generator deletes its file from the actions subdirectory. It deletes only files that have the veer header and the prefix and suffix of your configuration. With `actions_dir("")` it deletes nothing.
