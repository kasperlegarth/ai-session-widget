use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct SessionFile {
    pub pid: u32,
    pub session_id: String,
    pub cwd: String,
    pub name: String,
    /// The hook's own status word — "idle", "busy", "waiting" (with a
    /// `waitingFor` reason, e.g. for an AskUserQuestion prompt), or absent
    /// entirely for a session whose entrypoint doesn't emit it. Kept as the
    /// raw string rather than collapsed to a bool so `status::compute_status`
    /// can trust "waiting" as its own unambiguous signal.
    pub hook_status: Option<String>,
}

#[derive(Deserialize)]
struct RawSessionFile {
    pid: u32,
    #[serde(rename = "sessionId")]
    session_id: String,
    cwd: String,
    name: String,
    #[serde(default)]
    status: Option<String>,
}

pub fn project_dir_for_cwd(cwd: &str) -> String {
    cwd.chars()
        .map(|c| match c {
            ':' | '\\' | ' ' => '-',
            other => other,
        })
        .collect()
}

pub fn read_sessions_dir(dir: &Path) -> Vec<SessionFile> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut sessions = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(raw) = serde_json::from_str::<RawSessionFile>(&contents) else {
            continue;
        };
        sessions.push(SessionFile {
            pid: raw.pid,
            session_id: raw.session_id,
            cwd: raw.cwd,
            name: raw.name,
            hook_status: raw.status,
        });
    }
    sessions
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    fn write_file(dir: &Path, filename: &str, contents: &str) {
        let mut f = File::create(dir.join(filename)).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
    }

    #[test]
    fn parses_valid_session_file() {
        let dir = tempdir().unwrap();
        write_file(
            dir.path(),
            "1234.json",
            r#"{"pid":1234,"sessionId":"abc-123","cwd":"C:\\Projects\\foo","name":"foo-1a","status":"idle"}"#,
        );

        let result = read_sessions_dir(dir.path());

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].pid, 1234);
        assert_eq!(result[0].session_id, "abc-123");
        assert_eq!(result[0].cwd, "C:\\Projects\\foo");
        assert_eq!(result[0].name, "foo-1a");
        assert_eq!(result[0].hook_status.as_deref(), Some("idle"));
    }

    #[test]
    fn treats_missing_status_as_none() {
        let dir = tempdir().unwrap();
        write_file(
            dir.path(),
            "1234.json",
            r#"{"pid":1234,"sessionId":"abc-123","cwd":"C:\\Projects\\foo","name":"foo-1a"}"#,
        );

        let result = read_sessions_dir(dir.path());

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].hook_status, None);
    }

    #[test]
    fn skips_malformed_json_without_panicking() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "1234.json", "{not valid json");
        write_file(
            dir.path(),
            "5678.json",
            r#"{"pid":5678,"sessionId":"ok","cwd":"C:\\x","name":"n"}"#,
        );

        let result = read_sessions_dir(dir.path());

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].pid, 5678);
    }

    #[test]
    fn ignores_non_json_files() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "1234.key", "some binary-ish content");

        let result = read_sessions_dir(dir.path());

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn missing_directory_returns_empty_list() {
        let result = read_sessions_dir(Path::new("C:\\does\\not\\exist\\at\\all"));
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn derives_project_dir_name_from_cwd() {
        assert_eq!(
            project_dir_for_cwd("C:\\Projects\\claude-widget"),
            "C--Projects-claude-widget"
        );
        // Covers a path with spaces as well as the drive colon and separators.
        assert_eq!(
            project_dir_for_cwd("C:\\Work\\Sites\\Platform\\v10\\Acme Corp\\example-site"),
            "C--Work-Sites-Platform-v10-Acme-Corp-example-site"
        );
    }
}
