pub mod todos;

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use serde_json::{json, Value};
use std::time::Duration;
use todos::{HomeProps, NewTodo, ShowcaseProps, TodoStore, TodosCreateProps, TodosIndexProps};
use validator::Validate;
use veer::{Inertia, InertiaForm, Method::*, Prop, ScrollMetadata};

/// Build the named-route table. Used by `main.rs` for serving and by
/// `src/bin/gen-bindings.rs` to populate the TS bindings registry.
pub fn router() -> veer::Router<TodoStore> {
    veer::Router::new()
        .named_route(GET, "home", "/", home)
        .named_route(GET, "todos.index", "/todos", todos_index)
        .named_route(POST, "todos.store", "/todos", todos_create)
        .named_route(GET, "todos.create", "/todos/new", todos_new)
        .named_route(DELETE, "todos.destroy", "/todos/{id}", todos_delete)
        .named_route(GET, "showcase", "/showcase", showcase)
        .named_route(POST, "showcase.jump", "/showcase/jump", showcase_jump)
}

#[derive(serde::Deserialize)]
struct ShowcaseQuery {
    page: Option<u64>,
}

/// One page that uses the closure prop types: once, deferred, rescued, and
/// infinite scroll.
async fn showcase(
    inertia: Inertia,
    State(store): State<TodoStore>,
    Query(query): Query<ShowcaseQuery>,
) -> impl IntoResponse {
    let page = query.page.unwrap_or(1).clamp(1, 3);
    inertia
        .render("showcase", ShowcaseProps {})
        .once("plans", || async { json!(["Free", "Pro", "Team"]) })
        .deferred("stats", "default", move || async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            json!({ "todos": store.all().len() })
        })
        .prop(
            "broken",
            Prop::try_new(|| async { Err::<Value, _>("simulated failure") })
                .group("unstable")
                .rescue(),
        )
        .prop(
            "feed",
            Prop::scroll(move || async move {
                let first = (page - 1) * 20;
                let items: Vec<Value> = (first + 1..=first + 20)
                    .map(|n| json!({ "id": n, "title": format!("Item {n}") }))
                    .collect();
                (
                    json!({ "data": items }),
                    ScrollMetadata::paged("page", page, page < 3),
                )
            })
            .match_on("data.id"),
        )
}

/// A redirect whose target has a URL fragment (409 + `X-Inertia-Redirect`).
async fn showcase_jump(inertia: Inertia) -> impl IntoResponse {
    inertia
        .redirect("/showcase#feed")
        .with_flash("success", json!("Jumped to the feed"))
}

async fn home(inertia: Inertia) -> impl axum::response::IntoResponse {
    inertia.render("home", HomeProps {})
}

async fn todos_index(
    inertia: Inertia,
    State(store): State<TodoStore>,
) -> impl axum::response::IntoResponse {
    inertia.render("todos/index", TodosIndexProps { todos: store.all() })
}

async fn todos_new(inertia: Inertia) -> impl axum::response::IntoResponse {
    inertia.render("todos/create", TodosCreateProps {})
}

async fn todos_create(
    inertia: Inertia,
    State(store): State<TodoStore>,
    InertiaForm(body): InertiaForm<NewTodo>,
) -> axum::response::Response {
    // Live validation (Precognition): answer and do not create the todo.
    if let Some(precognition) = inertia.precognition() {
        return precognition.respond(body.validate());
    }
    if let Err(errors) = body.validate() {
        return inertia
            .with_errors(errors)
            .redirect("/todos/new")
            .into_response();
    }
    store.add(body.title);
    inertia
        .redirect("/todos")
        .with_flash("success", json!("Todo created"))
        .into_response()
}

async fn todos_delete(
    inertia: Inertia,
    State(store): State<TodoStore>,
    Path(id): Path<u64>,
) -> impl axum::response::IntoResponse {
    let msg = if store.delete(id) {
        "Todo deleted"
    } else {
        "Todo not found"
    };
    inertia.redirect("/todos").with_flash("success", json!(msg))
}
