# Error pages

An error response that is not an Inertia page shows in a modal on the client. To show an error as a page of your app, render a component with an HTTP status.

## Not found

Use a fallback handler. `status` sets the HTTP status of the page response:

```rust,ignore
use axum::http::StatusCode;
use veer::Inertia;

async fn not_found(inertia: Inertia) -> impl IntoResponse {
    inertia
        .render("Error", json!({ "status": 404 }))
        .status(StatusCode::NOT_FOUND)
}

let app = router()
    .fallback(not_found)
    .layer(InertiaLayer::new(cfg));   // after the fallback, so that it covers it
```

## Application errors

A handler that returns `Result<_, AppError>` needs `AppError: IntoResponse`, and that impl has no `Inertia` extractor. `InertiaResponse::render` makes a page without one:

```rust,ignore
use veer::InertiaResponse;

enum AppError {
    NotFound,
    Internal(anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::Internal(error) => {
                tracing::error!(%error, "request failed");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        InertiaResponse::render("Error", json!({ "status": status.as_u16() }))
            .status(status)
            .into_response()
    }
}

async fn users_show(inertia: Inertia, Path(id): Path<u64>) -> Result<impl IntoResponse, AppError> {
    let user = find_user(id).await?.ok_or(AppError::NotFound)?;
    Ok(inertia.render("Users/Show", UsersShowProps { user }))
}
```

The page gets the shared props and works with SSR, like any page. The route must be inside `InertiaLayer`.

## The component

```tsx
// pages/Error.tsx
const titles: Record<number, string> = {
  404: "Page not found",
  500: "Server error",
};

export default function Error({ status }: { status: number }) {
  return <h1>{titles[status] ?? "Error"}</h1>;
}
```

## Other failures

- **A panic or a `500` from another layer** is not a page. In development the modal is useful, because it shows the response. In production, keep the body free of internal data.
- **An expired CSRF token** gives `419` from [`CsrfLayer`](csrf.md), which is outside `InertiaLayer`. Handle it on the client.
- **A failed deferred prop** can be rescued, so that the page loads without it; see [Props](props.md).
