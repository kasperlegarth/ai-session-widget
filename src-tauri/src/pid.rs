use crate::sessions::SessionFile;
use sysinfo::{Pid, System};

pub fn is_pid_alive(sys: &System, pid: u32) -> bool {
    sys.process(Pid::from_u32(pid)).is_some()
}

pub fn filter_alive(
    sessions: Vec<SessionFile>,
    is_alive: impl Fn(u32) -> bool,
) -> Vec<SessionFile> {
    sessions.into_iter().filter(|s| is_alive(s.pid)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session(pid: u32) -> SessionFile {
        SessionFile {
            pid,
            session_id: "s".into(),
            cwd: "C:\\x".into(),
            name: "n".into(),
            hook_status: None,
        }
    }

    #[test]
    fn filter_alive_keeps_only_pids_reported_alive() {
        let sessions = vec![make_session(1), make_session(2), make_session(3)];

        let result = filter_alive(sessions, |pid| pid == 2);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].pid, 2);
    }

    #[test]
    fn filter_alive_returns_empty_when_none_alive() {
        let sessions = vec![make_session(1), make_session(2)];

        let result = filter_alive(sessions, |_| false);

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn current_process_pid_is_reported_alive() {
        let mut sys = System::new_all();
        sys.refresh_all();
        let current_pid = std::process::id();

        assert!(is_pid_alive(&sys, current_pid));
    }

    #[test]
    fn implausible_pid_is_reported_not_alive() {
        let mut sys = System::new_all();
        sys.refresh_all();

        assert!(!is_pid_alive(&sys, u32::MAX));
    }
}
