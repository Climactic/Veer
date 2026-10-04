# Architecture

## Layers

```text
┌────────────────────────────────────────────────────────┐
│  axum Router                                           │
│  ┌──────────────────────────────────────────────────┐  │
│  │ InertiaLayer                                     │  │
│  │   ├─ reads session data (errors, flash)          │  │
│  │   ├─ puts the config into request extensions     │  │
│  │   └─ on response: finishes the InertiaResponse,  │  │
│  │      writes session data, applies protocol rules │  │
│  └──────────────────────────────────────────────────┘  │
│                       │                                │
│                       ▼                                │
│        ┌───────────────────────────┐                   │
│        │  Inertia (extractor)      │                   │
│        │  inertia.render(...) →    │                   │
│        │  InertiaResponse builder  │                   │
│        └───────────────────────────┘                   │
└────────────────────────────────────────────────────────┘
              │
              ▼
   ┌──────────────────────────────────────┐
   │  Protocol core (no framework)        │
   │  • RequestInfo / PageObject          │
   │  • decide(): which response shape    │
   │  • prop resolver                     │
   │  • Traits: RootView, SessionStore,   │
   │    SsrClient, SharedProps            │
   └──────────────────────────────────────┘
```

A handler returns an `InertiaResponse`, which is only a description: the component, the props, the closures. The layer finishes it after the handler returns, because only the layer has the request headers, the session data and the config.

The protocol core has no I/O and no framework types. `protocol::decide` is a pure function from the request to a response shape, and `props::resolver::resolve` is a function from the props and the request headers to the final props and metadata. Both have unit tests. An adapter for another framework can use them without a change.

## Extension points

| Trait | Purpose | Built in |
|---|---|---|
| `RootView` | The HTML shell of the first page load | `MinimalRootView`, `ViteRootView`, `ClosureRootView` |
| `SessionStore` | Errors and flash data between requests | `CookieSessionStore`, `TowerSessionStore` |
| `SsrClient` | Server-side rendering | `HttpSsrClient` |
| `SharedProps` | Props for every page | `InertiaConfig::share` (closure) |
| `IntoErrorBag` | Validation errors from any library | `validator`, `garde`, maps and pairs |

## Feature flags

| Flag | Default | Effect |
|------|---------|--------|
| `axum` | **on** | Axum extractor, tower layer, `InertiaForm`, `Router`, Precognition |
| `multipart` | off | File uploads (`UploadedFile`, `MultipartStream`) |
| `ssr` | off | HTTP SSR client (`reqwest`) |
| `cookie-session` | off | Signed-cookie session store |
| `tower-sessions` | off | Session store on [`tower-sessions`](https://crates.io/crates/tower-sessions) |
| `validator` | off | `Validated<T>`; `IntoErrorBag` for `validator::ValidationErrors` |
| `garde` | off | `GardeValidated<T>`; `IntoErrorBag` for `garde::Report` |
| `csrf` | off | `CsrfLayer` and `CsrfTokens` |
| `embed` | off | `EmbeddedAssets` for a single-binary deploy |
| `devtools` | off | Recorder for the Inertia DevTools extension |
| `ts` | off | TypeScript bindings (`ts-rs`, `inventory`), `Inertia::page` |
| `testing` | off | `veer::testing`: helpers for tests of your handlers |

A feature that is off brings in none of its dependencies.

## Protocol coverage

Veer follows the Inertia v3 protocol as of client 3.8 and `inertia-laravel` 3.5.

| Area | Status |
|---|---|
| HTML and JSON page responses, `Vary` | ✅ |
| Asset versioning (`409`, `X-Inertia-Version`) | ✅ |
| Redirects (`303`, external `409`, fragment `409`) | ✅ |
| Partial reloads (`only`, `except`, dot paths, reset) | ✅ |
| Optional, deferred, once, always props | ✅ |
| Rescued deferred props | ✅ |
| Merge, prepend, deep merge, match-on | ✅ |
| Infinite scroll | ✅ |
| Shared props, `sharedProps` | ✅ |
| Validation errors, error bags | ✅ |
| Flash data | ✅ |
| History encryption, clear history, preserve fragment | ✅ |
| Big integers | ✅ |
| Precognition | ✅ |
| Server-side rendering | ✅ |
| DevTools protocol | ✅ (no handler or component source links) |

The wire formats are checked against the protocol document, the `inertia-laravel` source, and the client source, and the example app is checked in a browser with the React client. Vue and Svelte use the same client core.

## Differences from the Laravel adapter

- **`Prop::try_new` needs `.rescue()`** to rescue a failure; without it an `Err` gives a `500`. This is the same rule, in Rust terms.
- **Sessions hold only veer's data** (errors, flash, previous URL). Veer is not a general session library.
- **The once-prop expiry** is set with a `Duration` (`.until(...)`).
