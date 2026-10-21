//! Regenerates `src/lib/bindings.ts` from the Rust command/DTO definitions.
//!
//! Run from `src-tauri`: `cargo run --bin export_bindings`.
//! CI runs this and fails when the committed bindings differ.

fn main() {
    let builder = muster_lib::commands::specta_builder();
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/bindings.ts");
    builder
        .export(specta_typescript::Typescript::default(), out)
        .expect("export TypeScript bindings");
}
