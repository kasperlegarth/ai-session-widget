use crate::codex::CodexSessions;
use crate::model::{build_session_list, SessionInfo};
use crate::pid::is_pid_alive;
use std::sync::{Mutex, OnceLock};
use sysinfo::{ProcessRefreshKind, RefreshKind, System};

static CODEX: OnceLock<Mutex<CodexSessions>> = OnceLock::new();

#[tauri::command]
pub async fn get_sessions() -> Vec<SessionInfo> {
    tauri::async_runtime::spawn_blocking(collect_sessions)
        .await
        .unwrap_or_default()
}

fn collect_sessions() -> Vec<SessionInfo> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let sessions_dir = home.join(".claude").join("sessions");
    let projects_dir = home.join(".claude").join("projects");

    let sys =
        System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new()));

    let mut sessions =
        build_session_list(&sessions_dir, &projects_dir, |pid| is_pid_alive(&sys, pid));
    let codex_home = std::env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    if let Ok(mut codex) = CODEX
        .get_or_init(|| Mutex::new(CodexSessions::default()))
        .lock()
    {
        sessions.extend(codex.collect(&codex_home, |pid| {
            sys.process(sysinfo::Pid::from_u32(pid)).is_some_and(|p| {
                let name = p.name().to_string_lossy();
                name.eq_ignore_ascii_case("codex.exe") || name.eq_ignore_ascii_case("codex")
            })
        }));
    }
    sessions
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
