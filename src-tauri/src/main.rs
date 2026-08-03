// Prevent an additional windows API layer warning
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `muster <verb> ...` (e.g. `muster split`, `muster send ...`) is a
    // one-shot CLI client that talks to the running app over local IPC.
    // Any other invocation falls through to the normal GUI launch.
    let argv: Vec<String> = std::env::args().collect();
    if let Some(code) = muster_lib::services::ipc::client::dispatch(&argv) {
        std::process::exit(code);
    }
    muster_lib::bootstrap::run();
}