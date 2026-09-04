// Crate skeleton for the Claude Session Widget.
// Later tasks add tauri::command handlers and register them here.

mod sessions;
mod pid;
mod status;
mod model;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
