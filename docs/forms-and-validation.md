# Forms and validation

Inertia handles a form submit like a classic server-rendered app: the handler validates, then redirects. Errors and messages travel to the next page through the session. You need a session store for this; see [Sessions](sessions.md).

## A form handler

With the `validator` feature, `Validated<T>` decodes the body and validates it before your handler runs:

```rust,ignore
use veer::{Inertia, Validated};
use validator::Validate;

#[derive(serde::Deserialize, Validate)]
struct NewUser {
    #[validate(length(min = 1, message = "name is required"))]
    name: String,
    #[validate(email(message = "email is not valid"))]
    email: String,
}

async fn users_store(inertia: Inertia, Validated(body): Validated<NewUser>) -> impl IntoResponse {
    create_user(body).await;
    inertia.redirect("/users").with_flash("success", "User created")
}
```

The handler runs only for valid input. For invalid input, `Validated` redirects back to the page that the user came from ([`back`](redirects-and-history.md)), with the errors. `back` needs the `Referer` header, which browsers send by default. If your site sets `Referrer-Policy: no-referrer`, turn on `InertiaConfig::store_previous_url(true)`, or validate by hand. It also answers [Precognition](#precognition-live-validation) requests. With the `garde` feature, use `GardeValidated<T>` in the same way.

`Validated<T>` reads the body as `InertiaForm<T>` does: `application/json`, `application/x-www-form-urlencoded`, and, with the `multipart` feature, `multipart/form-data`. One handler works for all three.

### Validation by hand

Use `InertiaForm<T>` when you want control of the steps, for example to redirect to a different page or to add an error from a database check:

```rust,ignore
async fn users_store(
    inertia: Inertia,
    InertiaForm(body): InertiaForm<NewUser>,
) -> impl IntoResponse {
    if let Err(errors) = body.validate() {
        return inertia.with_errors(errors).redirect("/users/new");
    }
    create_user(body).await;
    inertia.redirect("/users").with_flash("success", "User created")
}
```

## Validation errors

`with_errors` takes anything that implements `IntoErrorBag`:

- `validator::ValidationErrors` (feature `validator`)
- `garde::Report` (feature `garde`)
- `HashMap<String, String>`, `Vec<(String, String)>`, `Vec<(&'static str, &'static str)>`

On the next page, the errors are in `props.errors`, which is always present (`{}` when there are none). On the frontend, `useForm` puts them in `form.errors`.

- **All messages.** By default each field has its first message, as a string. With `InertiaConfig::with_all_errors(true)`, each field has an array of all messages.
- **Error bags.** When a request names a bag (the `errorBag` visit option, sent as `X-Inertia-Error-Bag`), veer nests the errors under that name: `{ "createUser": { "email": "…" } }`. This keeps the errors of two forms on one page apart.
- **Errors on a render.** `inertia.render(…).with_errors(errors)` puts the errors on that page, with no redirect.

## Flash data

```rust,ignore
inertia.redirect("/users").with_flash("success", "User created")
```

Flash data is one-shot. It is the top-level `flash` field of the page object (`usePage().flash` on the frontend), and the client does not keep it in the browser history. It survives a chain of redirects, and responses that are not pages do not use it up.

The value is anything that converts to JSON: a string, a number, or `json!({ … })`. `with_flash` on a render puts the data on that page.

## Precognition (live validation)

[Precognition](https://inertiajs.com/docs/v3/the-basics/forms#precognition) validates a field while the user fills in the form, with the validation rules of the server. The client sends the normal request with a `Precognition: true` header.

`Validated<T>` and `GardeValidated<T>` answer these requests; your handler does not run. With `InertiaForm<T>`, answer the request before the action runs:

```rust,ignore
async fn users_store(
    inertia: Inertia,
    InertiaForm(body): InertiaForm<NewUser>,
) -> axum::response::Response {
    if let Some(precognition) = inertia.precognition() {
        return precognition.respond(body.validate());
    }
    if let Err(errors) = body.validate() {
        return inertia.with_errors(errors).redirect("/users/new").into_response();
    }
    create_user(body).await;
    inertia.redirect("/users").into_response()
}
```

> **Do the check first.** A Precognition request is the same request as the real submit, with one more header. A handler that does not call `inertia.precognition()` runs its action on each validation request. The client reports "Did not receive a Precognition response" in that case.
>
> **The body must deserialize.** `InertiaForm<T>` runs before your handler. A validation request sends the form as it is at that moment, so use types that accept incomplete input (`String`, `Option<T>`, `#[serde(default)]`) and put the rules in the validator.

`respond` returns `204` when the requested fields are valid, and `422` with the errors when they are not. It reports only the fields that the client named in `Precognition-Validate-Only`.

On the frontend, give `useForm` the method and URL:

```tsx
const form = useForm("post", "/users", { name: "", email: "" });

<input
  value={form.data.email}
  onChange={(e) => form.setData("email", e.target.value)}
  onBlur={() => form.validate("email")}
/>
{form.errors.email}
```

The client validates a field only after its value changes.

## File uploads

Enable the `multipart` feature and add `UploadedFile` fields:

```rust,ignore
use veer::{InertiaForm, UploadedFile};

#[derive(serde::Deserialize)]
struct CreateAvatar {
    user_id: String,
    avatar: UploadedFile,
}

async fn upload_avatar(
    inertia: Inertia,
    InertiaForm(form): InertiaForm<CreateAvatar>,
) -> impl IntoResponse {
    save_avatar(&form.user_id, &form.avatar.bytes, form.avatar.filename.as_deref()).await;
    inertia.redirect("/profile").with_flash("success", "Avatar updated")
}
```

The client's `useForm` changes to `multipart/form-data` when a field is a `File`. `UploadedFile` holds the file in memory. For large uploads, read the stream:

```rust,ignore
use veer::MultipartStream;

async fn huge_upload(MultipartStream(mut m): MultipartStream) -> impl IntoResponse {
    while let Some(field) = m.next_field().await.unwrap() {
        // Write each chunk to disk or object storage.
    }
}
```

## CSRF

Form submits need CSRF protection. See [CSRF protection](csrf.md).
