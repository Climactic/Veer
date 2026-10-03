# Upgrading from 0.1 to 0.2

Version 0.2 brings veer to the current Inertia v3 protocol (client 3.8). Most apps need the first two changes; the others apply only if you use the feature. The full list of additions is in the [changelog](../CHANGELOG.md).

## Flash data moved out of props

Flash data is now the top-level `flash` field of the page object. This is what the v3 client expects: it does not keep `flash` in the browser history.

```diff
- const success = usePage().props.flash?.success;
+ const success = usePage().flash.success;
```

No change on the Rust side: `with_flash` works as before.

## `ts-rs` 12

If you use the `ts` feature, update `ts-rs`:

```diff
- ts-rs = "11"
+ ts-rs = "12"
```

Then generate the bindings again. The generated `PageObject` and `Flash` types changed.

## Route paths

Not a veer change, but check it: axum 0.8 writes path parameters as `/users/{id}`, not `/users/:id`.

## `reset_merge` is removed

`InertiaResponse::reset_merge` and the `resetMergeProps` field were not part of the protocol. Delete the call. The client asks for a reset (`router.reload({ reset: ['posts'] })`), and veer then sends the prop without its merge label.

## `location()` for plain requests

`inertia.location(url)` answers `409` + `X-Inertia-Location` only for an Inertia request. A plain browser request now gets a `302`, which a browser can follow.

## `with_errors` on a render

`inertia.render(…).with_errors(errors)` now shows the errors on that page. In 0.1 they went to the next page. `with_errors(…).redirect(…)` is unchanged.

## Partial reloads follow the protocol

- `errors` is always sent.
- A partial reload with only `except` returns all other props (0.1 returned none).
- `only` and `except` accept dot paths.
- A prop that the response leaves out has no merge label.

If frontend code relied on the old behavior, check it.

## Custom session stores

- `Flash::errors` is now `HashMap<String, Vec<String>>`: all messages of a field.
- `Flash` is `#[non_exhaustive]` and has new fields. Build it from `Flash::default()`, and store the complete value; `Flash` implements `Serialize` and `Deserialize`.
- A store must keep the data when a response that is not a page writes it back.

## Lower-level types

- `props::closure::{LazyProp, DeferredProp}` are replaced by `props::Prop`. The builder methods `lazy`, `optional` and `deferred` are unchanged.
- `RequestInfo` and `PageObject` are `#[non_exhaustive]`. Use `RequestInfo::from_parts` and `PageObject::new`.
- `protocol::ResponseShape` has new variants.
- `props::resolver::resolve` returns a `Result`.

## New Cargo feature

The DevTools recorder is behind the new `devtools` feature. Nothing changes if you do not enable it.

## Recommended after the upgrade

- Update the client to `@inertiajs/react` (or `vue3`, `svelte`) 3.8.
- Closure props can return typed values: `.once("plans", || async { load_plans().await })`. `json!` is no longer necessary.
- Give closure props TypeScript types with the [third argument of `register_page!`](typescript.md#closure-props).
