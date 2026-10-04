# Upgrading

## From 0.2 to 0.3

Version 0.3 makes failures visible and removes repeated code. Most apps compile without a change. The full list is in the [changelog](../CHANGELOG.md).

**Changes that can need an edit**

- **`ViteManifest::load` and `.parse()` return `ViteManifestError`**, not `String`. It implements `std::error::Error`, so `?` works with `anyhow` and `Box<dyn Error>`. Remove a `.map_err` that you added for the `String`.
- **The rejection of the `Inertia` extractor is `MissingInertiaLayer`**, not `StatusCode`. This is important only if you name the rejection type.
- **Props that do not serialize give a `500`.** Before, the page rendered with `null` props.
- **The asset version of a production `ViteRootView` is the manifest hash** when you set no version. If you have `.version(move || hash.clone().into())`, you can remove it.

- **`RequestInfo::referer` is the path and query of the `Referer`, or `None`.** The scheme and the host are dropped, so `back()` always stays on your site.
- **`CsrfLayer` checks each method except `GET`, `HEAD`, `OPTIONS` and `TRACE`.**
- **Flash cookies from 0.2 are ignored** after the upgrade (they live 60 seconds).

**Recommended**

```diff
- async fn users_store(inertia: Inertia, InertiaForm(body): InertiaForm<NewUser>) -> Response {
-     if let Some(precognition) = inertia.precognition() {
-         return precognition.respond(body.validate());
-     }
-     if let Err(errors) = body.validate() {
-         return inertia.with_errors(errors).redirect("/users/new").into_response();
-     }
+ async fn users_store(inertia: Inertia, Validated(body): Validated<NewUser>) -> impl IntoResponse {
      create_user(body).await;
-     inertia.redirect("/users").with_flash("success", json!("User created")).into_response()
+     inertia.redirect("/users").with_flash("success", "User created")
  }
```

- `InertiaConfig::share(|req| async { … })` replaces `.shared(shared_props_fn(…))`. The closure can read request extensions: `req.extension::<CurrentUser>()`.
- `inertia.page(props)` replaces `inertia.render("Name", props)` for a struct with `register_page!`.
- `ViteRootView::auto(manifest_path)` replaces a hand-written switch between `dev()` and `production()`.
- `InertiaConfig::version_str("v1")` replaces `.version(|| "v1".into())`.
- Call `veer::bindings::generate_split` at startup in a debug build; see [TypeScript bindings](typescript.md#options).
- Use [`veer::testing`](testing.md) in your tests.

## From 0.1 to 0.2

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
