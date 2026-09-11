// Crate skeleton for the Claude Session Widget.
// Later tasks add tauri::command handlers and register them here.

mod commands;
mod codex;
mod sessions;
mod pid;
mod status;
mod model;
mod focus;
mod usage;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::get_sessions, commands::focus_session, commands::set_always_on_top, commands::close_app])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
