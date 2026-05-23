pub mod todos;

use axum::extract::{Path, State};
use serde_json::json;
use todos::{HomeProps, NewTodo, TodoStore, TodosCreateProps, TodosIndexProps};
use validator::Validate;
use veer::{Inertia, InertiaForm, Method::*};

/// Build the named-route table. Used by `main.rs` for serving and by
/// `src/bin/gen-bindings.rs` to populate the TS bindings registry.
pub fn router() -> veer::Router<TodoStore> {
    veer::Router::new()
        .named_route(GET,    "home",           "/",           home)
        .named_route(GET,    "todos.index",    "/todos",      todos_index)
        .named_route(POST,   "todos.store",    "/todos",      todos_create)
        .named_route(GET,    "todos.create",   "/todos/new",  todos_new)
        .named_route(DELETE, "todos.destroy",  "/todos/:id",  todos_delete)
}

async fn home(inertia: Inertia) -> impl axum::response::IntoResponse {
    inertia.render("home", HomeProps {})
}

async fn todos_index(
    inertia: Inertia,
    State(store): State<TodoStore>,
) -> impl axum::response::IntoResponse {
    inertia.render(
        "todos/index",
        TodosIndexProps {
            todos: store.all(),
        },
    )
}

async fn todos_new(inertia: Inertia) -> impl axum::response::IntoResponse {
    inertia.render("todos/create", TodosCreateProps {})
}

async fn todos_create(
    inertia: Inertia,
    State(store): State<TodoStore>,
    InertiaForm(body): InertiaForm<NewTodo>,
) -> impl axum::response::IntoResponse {
    if let Err(errors) = body.validate() {
        return inertia.with_errors(errors).redirect("/todos/new");
    }
    store.add(body.title);
    inertia
        .redirect("/todos")
        .with_flash("success", json!("Todo created"))
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
