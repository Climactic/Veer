# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
