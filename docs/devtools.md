# DevTools

[Inertia DevTools](https://inertiajs.com/docs/v3/advanced/devtools) is a browser extension that shows each Inertia request: the props and their categories, the headers, the route. Veer implements the server side of its [protocol](https://inertiajs.com/docs/v3/advanced/devtools-protocol).

## Setup

1. Enable the `devtools` Cargo feature. Without it, the recorder is not compiled into your binary.

   ```toml
   veer = { version = "0.3", features = ["devtools"] }
   ```

2. Turn the recorder on, in development only:

   ```rust,ignore
   let mut cfg = InertiaConfig::new();
   if cfg!(debug_assertions) {
       cfg = cfg.devtools(veer::DevTools::new());
   }
   ```

3. Install the extension ([Chrome](https://chromewebstore.google.com/detail/inertiajs-devtools/cbaffpghpcbmgbnlpamegieokkpdlnih), [Firefox](https://addons.mozilla.org/en-US/firefox/addon/inertia-js-devtools/)), and use client version 3.6 or later. Open the browser developer tools and select the "Inertia" panel.

Run the Vite dev server for the full feature set: the client exposes its visit data to the extension only in development mode.

## What the recorder does

For each request that goes through `InertiaLayer`, the recorder:

- makes an entry with the method, URL, status, request type, timing, headers, bodies, and for a page: the props and their categories;
- adds `X-Inertia-Devtools-Id` and `X-Inertia-Devtools-Parent-Out` to the response;
- on the first page load, adds a `<script data-inertia-devtools-id>` tag before `</body>`.

The extension then reads the entry from `GET /_inertia/devtools/entries/{id}`. `GET /_inertia/devtools/entries` lists all entries, newest first.

Entries are in memory: the 100 newest for each browser tab (`DevTools::new().limit(n)` changes this). A restart clears them.

## Security

The entries contain the props and bodies of every user. So:

- **Use the recorder in development only.** The read API is open by default.
- If other people can reach your development server, add a guard:

  ```rust,ignore
  DevTools::new().authorize(|request| is_developer(request))
  ```

  The read API then answers `403` when the guard returns `false`.
- Sensitive values are replaced with `[REDACTED]`: the headers `Authorization`, `Proxy-Authorization`, `Cookie`, `Set-Cookie`, `X-CSRF-Token`, `X-XSRF-Token`, and the keys `password`, `password_confirmation`, `current_password`, `token`, `_token`, `access_token`, `refresh_token`, `secret`, `client_secret`, `api_key` in bodies, props and URL query strings.

## Limits

- The entry has the file and line of the `inertia.render` call. The other "open in editor" links (handler, component file) and the route name are not available.
- A request body is recorded only when it is JSON, comes from an Inertia request, and is smaller than 256 KB.
- This implementation follows the protocol document and the extension source. It has automated tests against both, but no test with the installed extension yet. Please report a problem if the panel shows something wrong.
