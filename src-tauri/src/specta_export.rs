//! Generates `src/lib/bindings.ts` from the Rust DTOs.
//!
//! This is the source of truth for the frontend's data types: running the
//! `export_bindings` test rewrites the file, and CI fails when the committed
//! file is stale. Command bindings are a separate, later migration — this
//! module currently registers only the DTOs shared with the WebView.

#[cfg(test)]
mod tests {
    use specta::datatype::Primitive;
    use specta::Types;
    use specta_typescript::Typescript;
    use specta_util::Remapper;

    #[test]
    fn export_bindings() {
        let types = Types::default()
            .register::<crate::models::app::AppStateView>()
            .register::<crate::models::app::ProjectView>()
            .register::<crate::models::app::TabView>()
            .register::<crate::models::app::DirtyFileInfo>()
            .register::<crate::models::pane::Pane>()
            .register::<crate::models::pane::PaneColumn>()
            .register::<crate::models::pane::PaneContent>()
            .register::<crate::models::project::RightPanel>()
            .register::<crate::models::session::SessionInfo>()
            .register::<crate::models::diff::DiffTabInfo>()
            .register::<crate::services::search::SearchMatch>()
            .register::<crate::services::explorer::DirEntry>()
            .register::<crate::services::procs::ProcessInfo>()
            .register::<crate::services::procs::ListenPort>()
            .register::<crate::services::git::GitStatusEntry>()
            .register::<crate::services::git::RecentCommit>()
            .register::<crate::services::git::GitStatusInfo>()
            .register::<crate::services::git::GuardEntry>()
            .register::<crate::services::git::GitGuard>()
            .register::<crate::services::git::FileCommit>()
            .register::<crate::services::git::BlameLine>()
            .register::<crate::theme::ThemeColors>()
            .register::<crate::theme::ThemeInfo>()
            .register::<crate::services::shell::ShellOption>()
            .register::<crate::services::agents::AgentKind>()
            .register::<crate::services::agents::AgentState>()
            .register::<crate::services::agents::SessionAgentStatus>();

        // The WebView already receives these as JSON numbers (serde_json has
        // no 64-bit variant), and every value is a count, size or epoch ms
        // well inside Number's exact range. Specta forbids 64-bit ints by
        // default; map them to number explicitly for this UI-only contract.
        // Map to an integer primitive, not f64: the serde format marks floats
        // nullable (NaN/Infinity serialize as JSON null), while these values
        // are always finite counts, sizes or epoch ms.
        let types = Remapper::new()
            .rule(Primitive::i64.into(), Primitive::i32.into())
            .rule(Primitive::u64.into(), Primitive::i32.into())
            .rule(Primitive::usize.into(), Primitive::i32.into())
            .rule(Primitive::isize.into(), Primitive::i32.into())
            .rule(Primitive::i128.into(), Primitive::i32.into())
            .rule(Primitive::u128.into(), Primitive::i32.into())
            .rule(Primitive::f32.into(), Primitive::i32.into())
            .rule(Primitive::f64.into(), Primitive::i32.into())
            .remap_types(types);

        // Relative to the crate, not the process cwd, so the export works from
        // both `cargo test` (src-tauri) and workspace-root invocations.
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../src/lib/bindings.ts");
        Typescript::default()
            .export_to(out, &types, specta_serde::Format)
            .expect("export TypeScript bindings");
    }
}
