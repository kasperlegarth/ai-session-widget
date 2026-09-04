use crate::pid::filter_alive;
use crate::sessions::{project_dir_for_cwd, read_sessions_dir};
use crate::status::{compute_status, read_tail_lines, SessionStatus};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub pid: u32,
    pub session_id: String,
    pub name: String,
    pub cwd: String,
    pub status: SessionStatus,
}

pub fn build_session_list(
    sessions_dir: &Path,
    projects_dir: &Path,
    is_alive: impl Fn(u32) -> bool,
) -> Vec<SessionInfo> {
    let sessions = read_sessions_dir(sessions_dir);
    let alive = filter_alive(sessions, is_alive);

    alive
        .into_iter()
        .map(|s| {
            let project_dir = projects_dir.join(project_dir_for_cwd(&s.cwd));
            let transcript_path = project_dir.join(format!("{}.jsonl", s.session_id));
            let tail = read_tail_lines(&transcript_path, 5);
            let status = compute_status(s.idle, &tail);
            SessionInfo {
                pid: s.pid,
                session_id: s.session_id,
                name: s.name,
                cwd: s.cwd,
                status,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn builds_session_info_with_status_from_transcript() {
        let root = tempdir().unwrap();
        let sessions_dir = root.path().join("sessions");
        let projects_dir = root.path().join("projects");
        fs::create_dir_all(&sessions_dir).unwrap();
        fs::create_dir_all(&projects_dir).unwrap();

        fs::write(
            sessions_dir.join("111.json"),
            r#"{"pid":111,"sessionId":"sess-1","cwd":"C:\\Projects\\foo","name":"foo-1a","status":"idle"}"#,
        )
        .unwrap();

        let project_subdir = projects_dir.join("C--Projects-foo");
        fs::create_dir_all(&project_subdir).unwrap();
        fs::write(
            project_subdir.join("sess-1.jsonl"),
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"done"}]}}
"#,
        )
        .unwrap();

        let result = build_session_list(&sessions_dir, &projects_dir, |pid| pid == 111);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].pid, 111);
        assert_eq!(result[0].session_id, "sess-1");
        assert_eq!(result[0].name, "foo-1a");
        assert_eq!(result[0].status, SessionStatus::Waiting);
    }

    #[test]
    fn missing_transcript_file_defaults_to_working_or_waiting_without_panic() {
        let root = tempdir().unwrap();
        let sessions_dir = root.path().join("sessions");
        let projects_dir = root.path().join("projects");
        fs::create_dir_all(&sessions_dir).unwrap();
        fs::create_dir_all(&projects_dir).unwrap();

        fs::write(
            sessions_dir.join("222.json"),
            r#"{"pid":222,"sessionId":"sess-2","cwd":"C:\\Projects\\bar","name":"bar-1a"}"#,
        )
        .unwrap();

        let result = build_session_list(&sessions_dir, &projects_dir, |pid| pid == 222);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].status, SessionStatus::Working);
    }

    #[test]
    fn dead_pid_is_excluded() {
        let root = tempdir().unwrap();
        let sessions_dir = root.path().join("sessions");
        let projects_dir = root.path().join("projects");
        fs::create_dir_all(&sessions_dir).unwrap();
        fs::create_dir_all(&projects_dir).unwrap();

        fs::write(
            sessions_dir.join("333.json"),
            r#"{"pid":333,"sessionId":"sess-3","cwd":"C:\\Projects\\baz","name":"baz-1a"}"#,
        )
        .unwrap();

        let result = build_session_list(&sessions_dir, &projects_dir, |_| false);

        assert_eq!(result.len(), 0);
    }
}
