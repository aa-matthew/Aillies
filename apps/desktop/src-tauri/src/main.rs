// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = aillies_node::node::permission_helper_entry() {
        std::process::exit(code);
    }
    aillies_desktop_lib::run();
}
