use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use ts_rs::TS;
use validator::Validate;

#[derive(Clone, Serialize, Debug, TS)]
#[ts(export)]
pub struct Todo {
    pub id: u64,
    pub title: String,
    pub done: bool,
}

#[derive(Deserialize, Validate)]
pub struct NewTodo {
    #[validate(length(min = 3, message = "title must have at least 3 characters"))]
    pub title: String,
}

#[derive(Clone, Default)]
pub struct TodoStore {
    pub items: Arc<Mutex<Vec<Todo>>>,
    pub next_id: Arc<Mutex<u64>>,
}

impl TodoStore {
    pub fn all(&self) -> Vec<Todo> {
        self.items.lock().unwrap().clone()
    }
    pub fn add(&self, title: String) -> Todo {
        let mut id = self.next_id.lock().unwrap();
        *id += 1;
        let t = Todo {
            id: *id,
            title,
            done: false,
        };
        self.items.lock().unwrap().push(t.clone());
        t
    }
    pub fn delete(&self, id: u64) -> bool {
        let mut items = self.items.lock().unwrap();
        if let Some(pos) = items.iter().position(|t| t.id == id) {
            items.remove(pos);
            true
        } else {
            false
        }
    }
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct HomeProps {}
veer::register_page!(HomeProps, "home");

#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ShowcaseProps {
    /// Larger than JavaScript's safe integer range: sent as a `BigInt`.
    pub order_id: u64,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct Stats {
    pub todos: usize,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct FeedItem {
    pub id: u32,
    pub title: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct Feed {
    pub data: Vec<FeedItem>,
}

/// The props that the `showcase` handler attaches as closures. This type
/// gives them TypeScript types; the handler's closures return the same types.
#[derive(TS)]
#[ts(export)]
pub struct ShowcaseClosureProps {
    /// Once prop.
    pub plans: Vec<String>,
    /// Deferred prop: absent on the first render.
    #[ts(optional)]
    pub stats: Option<Stats>,
    /// Infinite-scroll prop.
    pub feed: Feed,
}
veer::register_page!(ShowcaseProps, "showcase", ShowcaseClosureProps);

#[derive(Serialize, TS)]
#[ts(export)]
pub struct TodosIndexProps {
    pub todos: Vec<Todo>,
}
veer::register_page!(TodosIndexProps, "todos/index");

#[derive(Serialize, TS)]
#[ts(export)]
pub struct TodosCreateProps {}
veer::register_page!(TodosCreateProps, "todos/create");
