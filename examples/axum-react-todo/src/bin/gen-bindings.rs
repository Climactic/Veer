//! Emit the TypeScript bindings the frontend consumes.
//!
//! Run with `cargo run --bin gen-bindings` (or via the lefthook pre-commit
//! hook in `lefthook.yml`).

fn main() {
    // Building the router populates the runtime route registry as a side
    // effect — every `.named_route()` call recorded its name/path/method.
    // We never actually serve traffic, so we discard the axum::Router.
    let _ = axum_react_todo::router().build();

    let out = concat!(env!("CARGO_MANIFEST_DIR"), "/frontend/gen");
    veer::bindings::generate_split(out).expect("generate");
    println!("wrote {out}/");
}
