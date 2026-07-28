// Prevent an additional windows API layer warning
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    muster_lib::bootstrap::run();
}