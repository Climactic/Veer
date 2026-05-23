//! Emit the TypeScript bindings the frontend consumes.
//!
//! Run with `cargo run --bin gen-bindings` (or via the lefthook pre-commit
//! hook in `lefthook.yml`).

fn main() {
    // Force a reference to the lib crate so its inventory submissions
    // (page + route registrations from `axum_react_todo::todos`) link in.
    let _ = std::mem::size_of::<axum_react_todo::todos::HomeProps>();
    let _ = std::mem::size_of::<axum_react_todo::todos::TodosIndexProps>();
    let _ = std::mem::size_of::<axum_react_todo::todos::TodosCreateProps>();

    let out = concat!(env!("CARGO_MANIFEST_DIR"), "/frontend/gen");
    veer::bindings::generate_split(out).expect("generate");
    println!("wrote {out}/");
}
