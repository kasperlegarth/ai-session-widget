use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionStatus {
    Working,
    NeedsInput,
    Waiting,
}

pub fn read_tail_lines(path: &Path, n: usize) -> Vec<String> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut lines: Vec<String> = contents
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect();
    if lines.len() > n {
        lines = lines.split_off(lines.len() - n);
    }
    lines
}

pub fn compute_status(idle: bool, tail_lines: &[String]) -> SessionStatus {
    if !idle {
        return SessionStatus::Working;
    }

    let mut pending_tool_use_ids: Vec<String> = Vec::new();

    for line in tail_lines {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let msg_type = value.get("type").and_then(Value::as_str).unwrap_or("");
        if msg_type != "assistant" && msg_type != "user" {
            continue;
        }
        let Some(content) = value
            .pointer("/message/content")
            .and_then(Value::as_array)
        else {
            continue;
        };
        for item in content {
            match item.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    if let Some(id) = item.get("id").and_then(Value::as_str) {
                        pending_tool_use_ids.push(id.to_string());
                    }
                }
                Some("tool_result") => {
                    if let Some(id) = item.get("tool_use_id").and_then(Value::as_str) {
                        pending_tool_use_ids.retain(|pending| pending != id);
                    }
                }
                _ => {}
            }
        }
    }

    if pending_tool_use_ids.is_empty() {
        SessionStatus::Waiting
    } else {
        SessionStatus::NeedsInput
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn working_when_not_idle_regardless_of_tail() {
        let status = compute_status(false, &[]);
        assert_eq!(status, SessionStatus::Working);
    }

    #[test]
    fn waiting_when_idle_and_last_message_is_plain_text() {
        let tail = vec![
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Done."}]}}"#
                .to_string(),
        ];

        let status = compute_status(true, &tail);

        assert_eq!(status, SessionStatus::Waiting);
    }

    #[test]
    fn needs_input_when_idle_and_last_tool_use_has_no_result() {
        let tail = vec![
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Bash"}]}}"#
                .to_string(),
        ];

        let status = compute_status(true, &tail);

        assert_eq!(status, SessionStatus::NeedsInput);
    }

    #[test]
    fn waiting_when_idle_and_tool_use_already_has_matching_result() {
        let tail = vec![
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Bash"}]}}"#
                .to_string(),
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1"}]}}"#
                .to_string(),
        ];

        let status = compute_status(true, &tail);

        assert_eq!(status, SessionStatus::Waiting);
    }

    #[test]
    fn malformed_lines_in_tail_are_skipped_without_panicking() {
        let tail = vec![
            "{not valid json".to_string(),
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hi"}]}}"#
                .to_string(),
        ];

        let status = compute_status(true, &tail);

        assert_eq!(status, SessionStatus::Waiting);
    }

    #[test]
    fn read_tail_lines_returns_last_n_non_empty_lines() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("transcript.jsonl");
        let mut f = File::create(&path).unwrap();
        writeln!(f, "line1").unwrap();
        writeln!(f).unwrap();
        writeln!(f, "line2").unwrap();
        writeln!(f, "line3").unwrap();

        let tail = read_tail_lines(&path, 2);

        assert_eq!(tail, vec!["line2".to_string(), "line3".to_string()]);
    }

    #[test]
    fn read_tail_lines_missing_file_returns_empty() {
        let tail = read_tail_lines(Path::new("C:\\does\\not\\exist.jsonl"), 5);
        assert_eq!(tail, Vec::<String>::new());
    }
}
