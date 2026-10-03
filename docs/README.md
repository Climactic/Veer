# Veer documentation

Veer is the server side of the [Inertia.js v3 protocol](https://inertiajs.com/docs/v3/core-concepts/the-protocol) for Rust. These guides cover the Rust side. For the client side (React, Vue, Svelte), use the [Inertia documentation](https://inertiajs.com/docs/v3).

## Start here

| Guide | What it covers |
|---|---|
| [Getting started](getting-started.md) | Install, first page, how a request flows, the frontend entry point |
| [Upgrading from 0.1](upgrading.md) | Every breaking change in 0.2 and what to do |

## Building pages

| Guide | What it covers |
|---|---|
| [Props](props.md) | Partial reloads, lazy / deferred / once props, merging, infinite scroll, shared props, big integers |
| [Forms and validation](forms-and-validation.md) | `InertiaForm`, validation errors, error bags, flash data, Precognition, file uploads |
| [Redirects and history](redirects-and-history.md) | `redirect`, `back`, external redirects, URL fragments, history encryption |
| [Sessions](sessions.md) | The cookie store, `tower-sessions`, writing your own store |

## Frontend integration

| Guide | What it covers |
|---|---|
| [Vite, SSR and assets](vite-ssr-assets.md) | `ViteRootView`, server-side rendering, embedded assets, `<head>` elements |
| [TypeScript bindings](typescript.md) | Typed page props and route helpers generated from Rust |

## Operations

| Guide | What it covers |
|---|---|
| [CSRF protection](csrf.md) | `CsrfLayer` and the `XSRF-TOKEN` convention |
| [DevTools](devtools.md) | The recorder for the Inertia DevTools browser extension |
| [Architecture](architecture.md) | Crate layout, feature flags, protocol coverage, extension points |

API reference: [docs.rs/veer](https://docs.rs/veer). Working example: [`examples/axum-react-todo`](../examples/axum-react-todo).
