# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Developer-experience changes: clear failures, less code in handlers, less setup.

### Added

- `Validated<T>` (`validator` feature) and `GardeValidated<T>` (`garde`
  feature): body extractors that validate, answer Precognition requests, and
  redirect back with the errors.
- `InertiaConfig::share`: shared props from a closure that returns any
  `Serialize` value.
- `RequestInfo::extension` / `extensions`: shared props can read data that a
  middleware put in the request extensions (the signed-in user).
- `Inertia::page(props)` (`ts` feature): the component name comes from
  `register_page!`.
- `InertiaResponse::render`: a page without the `Inertia` extractor, for the
  `IntoResponse` impl of an error type. `InertiaResponse::status` sets the
  HTTP status of a page.
- `ViteRootView::auto`: dev mode in a debug build, production mode in a
  release build.
- `RootView::version`: a production `ViteRootView` gives the manifest hash as
  the default asset version. `InertiaConfig::version_str` for a constant.
- `veer::testing` (`testing` feature): `visit`, `TestPage`, `MemorySession`.
- A warning when errors or flash data are set and there is no session store.
- docs.rs shows all features, with a label on each feature-gated item.
- Guides: error pages, testing.

### Changed

- **Breaking:** page props that do not serialize give a `500` (the cause is in the
  body in a debug build). Before, the page rendered with `null` props.
- **Breaking:** the rejection of the `Inertia` extractor is
  `MissingInertiaLayer`, not `StatusCode`. Its body names the cause in a debug
  build.
- **Breaking:** `ViteManifest::load` and `ViteManifest::from_str` return
  `ViteManifestError`, not `String`.
- **Breaking:** `InertiaConfig::version` has no `"1"` default when the root
  view has a version (see `RootView::version`).
- The mode-specific setters of `ViteRootView` (`dev_server`, `react_refresh`,
  `manifest`, `asset_base`) do nothing in the other mode. Before, they
  panicked.
- The bindings generator writes a file only when its content changes, and
  `Split` deletes the generated files of removed controllers from its actions
  subdirectory.

### Security

- `Inertia::back` and the empty-response redirect use only the path and query
  of the `Referer`. Before, a `Referer` of another site made an open redirect.
  **Breaking:** `RequestInfo::referer` is now that path, or `None`.
- A request URL that starts with `//` is read as a path of this site
  (`/evil.test/x`), so that the page URL and `X-Inertia-Location` cannot point
  at another site.
- `CsrfLayer` checks each method except `GET`, `HEAD`, `OPTIONS` and `TRACE`.
  Before, a method other than `POST`, `PUT`, `PATCH` and `DELETE` was not
  checked.
- An SSR failure with `ssr_required`, and a root view failure, give a `500`
  with a generic body in a release build. Before, the error text went to the
  client, and the root view error had status `200`.
- `EmbeddedAssets` answers `404` for a path with `..`, `\`, or `%`, and does
  not call the resolver. It sets `X-Content-Type-Options: nosniff`.
- `CookieSessionStore` signs the cookie name with the value, so that the value
  of one cookie is not valid as another cookie. Cookies from an older version
  are ignored one time.
- `ViteRootView` and `MinimalRootView` escape URLs in the tags that they emit.
- The `500` bodies that veer makes are `text/plain`.

### Fixed

- The `Router` documentation used the axum 0.7 path syntax (`/:id`).

## [0.2.0] - 2026-10-03

Brings the protocol surface up to Inertia client 3.8.0 / `inertia-laravel` 3.5.1.

### Added

- `Prop`: a composable closure prop (`optional`, `defer`/`group`, `once`,
  `merge`/`prepend`/`deep_merge`, `Prop::try_new` + `rescue` for rescued props,
  `Prop::scroll` + `ScrollMetadata` for infinite scroll), attached with
  `InertiaResponse::prop`. `lazy`, `optional` and `deferred` build on it.
- Once props (`InertiaResponse::once`, `InertiaConfig::share_once`,
  `X-Inertia-Except-Once-Props`, `onceProps`).
- Page object fields `prependProps`, `deepMergeProps`, `matchPropsOn`,
  `scrollProps`, `rescuedProps`, `sharedProps`, `onceProps`, `flash`,
  `preserveFragment`, `preserveBigIntegers`.
- Builder methods `prepend`, `deep_merge`, `match_on`, `preserve_fragment`,
  `preserve_big_integers`; config methods `encrypt_history`,
  `preserve_big_integers`.
- Big-integer transport (`$bigint` markers) for props and flash data.
- `X-Inertia-Version` on the version-mismatch 409, so that the client does not
  hard-reload on background requests.
- 409 + `X-Inertia-Redirect` for redirects whose target has a URL fragment.
- `X-Inertia-Error-Bag` support: errors nest under the bag name.
- Precognition: `Inertia::precognition()` and `Precognition::respond`.
- `RequestInfo` fields `error_bag`, `except_once_props`, `scroll_prepend`,
  `is_prefetch`, `is_precognition`, `validate_only`.
- Closure props return any `Serialize` value, and `register_page!` takes a
  third argument that gives closure props their TypeScript types.
- `InertiaConfig::store_previous_url`: `Inertia::back` uses the session's
  previous URL when the request has no `Referer`.
- `veer::Head`: escaped `<head>` elements for the client's `serverHead` option.
- Inertia DevTools protocol (`devtools` feature): `InertiaConfig::devtools(DevTools::new())` records
  each request, sets the `X-Inertia-Devtools-*` headers and serves the read API.
- All validation messages per field: `IntoErrorBag::into_all_errors`,
  `InertiaConfig::with_all_errors`.
- `Prop` at a nested dot path (`.prop("auth.permissions", …)`).
- An empty `200` response to an Inertia request redirects back.
- `HttpSsrClient::timeout` and `HttpSsrClient::health`.
- A `docs/` folder with one guide per topic; the README is now an overview.
- `shared_props_fn` returns a value that `InertiaConfig::shared` accepts.
- `Vary: X-Inertia` on all responses that pass through `InertiaLayer`.
- Plain handler responses (for example `axum::response::Redirect`) now get the
  version-mismatch 409, fragment redirect and flash carry-over too.

### Changed (breaking)

- Flash data is the page object's top-level `flash` (`usePage().flash`), not
  `props.flash`. It is omitted when empty.
- `InertiaResponse::reset_merge` and the non-protocol `resetMergeProps` field
  are removed. A prop named in `X-Inertia-Reset` is sent without merge labels.
- `Inertia::location` returns a 302 for non-Inertia requests (409 only for
  Inertia requests).
- `props::closure::{LazyProp, DeferredProp}` are replaced by `props::Prop`;
  `ResolveInput` / `ResolvedProps` changed with it. `ResponseShape` has new
  variants.
- `RequestInfo` and `PageObject` are `#[non_exhaustive]`.
- `with_errors` on a render puts the errors on that page (not on the next one).
- Flash data is used only by a page render; other responses pass it on.
- `Flash::errors` is `HashMap<String, Vec<String>>` (all messages per field).
- `SessionStore` has two new methods with defaults, `previous_url` and
  `store_previous_url`.
- `Flash` is `#[non_exhaustive]` and has `clear_history` / `preserve_fragment`;
  `clear_history()` on a redirect now applies to the page the redirect lands on.
- Wrapper paths are dot paths; a nested `Merge<T>` now emits `mergeProps`.
- `ts` feature: `ts-rs` 12 (was 11). Generated `PageObject` type updated.
- `validator` 0.21, `base64` 0.23, `getrandom` 0.4.

### Fixed

- `errors` is an always prop: partial reloads no longer drop it.
- A partial reload with only `X-Inertia-Partial-Except` no longer drops every prop.
- Partial reloads accept dot paths and no longer emit merge labels for props
  that the response leaves out.
- The page JSON in the HTML shell escapes `<`, `>` and `/`. A prop that held
  `<!--<script>` gave a blank page.
- Flash data survives a version-mismatch 409 and redirect chains.
- `Always` / `Merge` wrappers inside shared props are stripped.
- The wrapper marker keys have a random suffix for each process, so user data
  cannot name them.
- Redirect responses no longer run shared props and prop closures.
- SSR errors include the error body of the SSR server.

## [0.1.2] - 2026-05-26

### Added

- `csrf` feature: `CsrfLayer`, a standalone tower layer providing
  Inertia/axios-compatible CSRF protection via stateless HMAC-signed
  double-submit tokens, plus the framework-agnostic `CsrfTokens` core.
- `embed` feature: `EmbeddedAssets`, an axum service for serving build assets
  embedded in the binary (rust-embed / `include_dir` / map) — the single-binary
  deploy counterpart to `ServeDir`.

[0.2.0]: https://github.com/Climactic/Veer/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/Climactic/Veer/compare/v0.1.1...v0.1.2
