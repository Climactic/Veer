# Redirects and history

## Redirects

```rust,ignore
inertia.redirect("/users")          // 303 See Other
inertia.back()                      // back to the previous page
inertia.location("https://…")       // leave the app
```

`redirect` always answers `303`, so the browser follows with a `GET` after any method. A plain `302` that your handler returns from a `POST`, `PUT`, `PATCH` or `DELETE` (for example `axum::response::Redirect`) is changed to `303` by the layer.

### Back

`inertia.back()` redirects to, in this order:

1. the `Referer` header of the request;
2. the previous URL from the session, if `InertiaConfig::store_previous_url(true)` is set;
3. `/`.

With `store_previous_url`, veer stores the URL of each Inertia page visit in the session. Partial reloads and prefetches are not stored. This is the same rule as the `store_previous_url` option of the Laravel adapter. It needs a session store. With the cookie store the value lives for 60 seconds; use `tower-sessions` for a longer life.

### External redirects

```rust,ignore
async fn oauth_start(inertia: Inertia) -> impl IntoResponse {
    inertia.location("https://accounts.example.com/oauth/authorize?…")
}
```

An XHR cannot follow a redirect to another origin. For an Inertia request, veer answers `409` with `X-Inertia-Location`, and the client does a full browser navigation. For a plain browser request it answers a normal `302`.

### URL fragments

An XHR drops the fragment (`#section`) of a redirect that it follows. Veer handles both cases:

- **The redirect target has a fragment**: `inertia.redirect("/docs#install")`. For an Inertia request, veer answers `409` with `X-Inertia-Redirect`, and the client makes a new visit to the full URL.
- **Keep the fragment of the original request**: `inertia.redirect("/docs").preserve_fragment()`. The next page object has `preserveFragment: true`.

### Empty responses

An Inertia request that gets an empty `200` cannot render anything. As the Laravel adapter does, veer changes it into a redirect back.

## History

Inertia keeps page data in the browser history state. Two controls protect sensitive pages:

```rust,ignore
inertia.render("Account", props).encrypt_history()   // encrypt this page's history state
inertia.redirect("/login").clear_history()           // the next page clears the encryption key
```

`InertiaConfig::encrypt_history(true)` turns encryption on for every page. `clear_history()` on a redirect applies to the page that the redirect lands on. Call it on logout, so that the back button cannot show the pages of the signed-out user.

## Asset version mismatch

When a client sends an old asset version on a `GET`, veer answers `409` with `X-Inertia-Location` and `X-Inertia-Version`. The client then reloads the page. The version header tells the client that this is a deploy and not an external redirect, so it does not reload during a background request such as a poll. Flash data survives the reload.
