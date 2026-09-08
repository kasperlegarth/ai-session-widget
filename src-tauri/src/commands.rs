use crate::model::{build_session_list, SessionInfo};
use crate::pid::is_pid_alive;
use sysinfo::{ProcessRefreshKind, RefreshKind, System};

#[tauri::command]
pub async fn get_sessions() -> Vec<SessionInfo> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let sessions_dir = home.join(".claude").join("sessions");
    let projects_dir = home.join(".claude").join("projects");

    let sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::new()),
    );

    build_session_list(&sessions_dir, &projects_dir, |pid| is_pid_alive(&sys, pid))
}

#[tauri::command]
pub fn focus_session(pid: u32, hint: String) {
    crate::focus::focus_pid(pid, &hint);
}

#[tauri::command]
pub fn set_always_on_top(window: tauri::Window, enabled: bool) {
    let _ = window.set_always_on_top(enabled);
}

#[tauri::command]
pub fn close_app(window: tauri::Window) {
    let _ = window.close();
}
