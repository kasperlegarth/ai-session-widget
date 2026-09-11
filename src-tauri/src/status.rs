use serde::Serialize;
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const TAIL_SEEK_WINDOW: u64 = 65536;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionStatus {
    Working,
    NeedsInput,
    Waiting,
}

pub fn read_tail_lines(path: &Path, n: usize) -> Vec<String> {
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    let Ok(file_len) = file.seek(SeekFrom::End(0)) else {
        return Vec::new();
    };

    let seeked = file_len > TAIL_SEEK_WINDOW;
    let start = if seeked { file_len - TAIL_SEEK_WINDOW } else { 0 };

    if file.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }

    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).is_err() {
        return Vec::new();
    }
    let contents = String::from_utf8_lossy(&buf);

    let mut lines: Vec<String> = contents
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect();

    if seeked && !lines.is_empty() {
        lines.remove(0);
    }

    if lines.len() > n {
        lines = lines.split_off(lines.len() - n);
    }
    lines
}

pub fn compute_status(idle: bool, tail_lines: &[String]) -> SessionStatus {
    if let Some((name, _)) = pending_tool_use(tail_lines) {
        // AskUserQuestion never resolves on its own — it always blocks on a
        // human response — so seeing it pending is unambiguous evidence of
        // NeedsInput even before the idle-ping hook fires (it apparently
        // doesn't fire for this tool the way it does for a permission
        // prompt). An *ordinary* tool call, though, is routinely "pending"
        // for a moment simply because it's still executing — the gap
        // between its tool_use being logged and its tool_result following
        // can outlast one poll interval — so for anything else, only trust
        // `idle` (handled below) rather than flashing NeedsInput on every
        // in-flight tool call.
        if name == "AskUserQuestion" || idle {
            return SessionStatus::NeedsInput;
        }
    }

    if !idle {
        // A missing `status:"idle"` field means "hasn't been marked idle
        // yet", not "is actively working" — a brand-new session (or one
        // whose entrypoint doesn't emit the idle ping the same way, e.g.
        // some VS Code-hosted sessions) can sit here indefinitely with no
        // transcript written at all. Only call it Working if the tail
        // actually shows something happened; an empty tail means there's
        // no evidence of activity, so it's more honest to call it Waiting.
        if tail_lines.is_empty() {
            return SessionStatus::Waiting;
        }
        return SessionStatus::Working;
    }

    SessionStatus::Waiting
}

/// Human-readable label for a tool call, based on its name and (best-effort)
/// input fields. Falls back to the raw tool name for anything unrecognized.
fn tool_label(name: &str, input: &Value) -> String {
    let basename = |field: &str| -> Option<String> {
        input
            .get(field)
            .and_then(Value::as_str)
            .map(|p| {
                p.replace('\\', "/")
                    .rsplit('/')
                    .next()
                    .unwrap_or(p)
                    .to_string()
            })
    };

    match name {
        "Read" => match basename("file_path") {
            Some(f) => format!("Reading {f}"),
            None => "Reading a file".to_string(),
        },
        "Edit" | "Write" | "NotebookEdit" => match basename("file_path") {
            Some(f) => format!("Editing {f}"),
            None => "Editing a file".to_string(),
        },
        "Grep" | "Glob" => "Searching".to_string(),
        "Bash" => "Running a command".to_string(),
        "WebFetch" | "WebSearch" => "Searching the web".to_string(),
        other => format!("Using {other}"),
    }
}

/// Finds the last tool_use in the tail (regardless of whether it already
/// has a matching tool_result) — used while Working, to show what the
/// session is currently doing.
fn last_tool_use(tail_lines: &[String]) -> Option<(String, Value)> {
    let mut found: Option<(String, Value)> = None;
    for line in tail_lines {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(content) = value.pointer("/message/content").and_then(Value::as_array) else {
            continue;
        };
        for item in content {
            if item.get("type").and_then(Value::as_str) == Some("tool_use") {
                let name = item.get("name").and_then(Value::as_str).unwrap_or("");
                let input = item.get("input").cloned().unwrap_or(Value::Null);
                found = Some((name.to_string(), input));
            }
        }
    }
    found
}

/// Finds the pending tool_use (one with no matching tool_result yet) —
/// used while NeedsInput, to show what's waiting on the user.
fn pending_tool_use(tail_lines: &[String]) -> Option<(String, Value)> {
    let mut pending: Vec<(String, String, Value)> = Vec::new(); // (id, name, input)
    for line in tail_lines {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(content) = value.pointer("/message/content").and_then(Value::as_array) else {
            continue;
        };
        for item in content {
            match item.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    let id = item.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                    let name = item.get("name").and_then(Value::as_str).unwrap_or("").to_string();
                    let input = item.get("input").cloned().unwrap_or(Value::Null);
                    pending.push((id, name, input));
                }
                Some("tool_result") => {
                    if let Some(id) = item.get("tool_use_id").and_then(Value::as_str) {
                        pending.retain(|(pid, _, _)| pid != id);
                    }
                }
                _ => {}
            }
        }
    }
    pending.into_iter().last().map(|(_, name, input)| (name, input))
}

/// Last plain assistant text message in the tail, if the tail's very last
/// content-bearing message was plain text (not a tool_use).
fn last_assistant_text(tail_lines: &[String]) -> Option<String> {
    let mut found: Option<String> = None;
    for line in tail_lines {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let Some(content) = value.pointer("/message/content").and_then(Value::as_array) else {
            continue;
        };
        let mut text_here = None;
        for item in content {
            if item.get("type").and_then(Value::as_str) == Some("text") {
                text_here = item.get("text").and_then(Value::as_str).map(str::to_string);
            } else if item.get("type").and_then(Value::as_str) == Some("tool_use") {
                text_here = None; // a trailing tool_use means this message isn't plain text
            }
        }
        if text_here.is_some() {
            found = text_here;
        } else {
            found = None;
        }
    }
    found
}

/// Best-effort human-readable description of what a session is currently
/// doing, shown as small subtext under the main status label. `None` means
/// nothing more precise than the status word itself is known.
pub fn extract_activity(status: SessionStatus, tail_lines: &[String]) -> Option<String> {
    match status {
        SessionStatus::Working => {
            let (name, input) = last_tool_use(tail_lines)?;
            Some(tool_label(&name, &input))
        }
        SessionStatus::NeedsInput => {
            let (name, _input) = pending_tool_use(tail_lines)?;
            if name == "AskUserQuestion" {
                Some("Asked a question".to_string())
            } else {
                Some(format!("Needs permission: {name}"))
            }
        }
        SessionStatus::Waiting => {
            let text = last_assistant_text(tail_lines)?;
            if text.trim_end().ends_with('?') {
                Some("Ended with a question".to_string())
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn working_when_not_idle_and_tail_shows_activity() {
        let tail = vec![
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hi"}]}}"#
                .to_string(),
        ];

        let status = compute_status(false, &tail);

        assert_eq!(status, SessionStatus::Working);
    }

    #[test]
    fn waiting_when_not_idle_but_tail_is_empty() {
        // A fresh session (or one whose entrypoint never emits the idle
        // ping) can have no transcript at all yet — no evidence of
        // activity means this shouldn't be called Working.
        let status = compute_status(false, &[]);
        assert_eq!(status, SessionStatus::Waiting);
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
    fn working_not_needs_input_when_an_ordinary_tool_use_is_still_in_flight() {
        // An auto-approved tool call is briefly "pending" between its
        // tool_use being logged and its tool_result following — that's just
        // normal execution, not a block, and shouldn't flash NeedsInput.
        let tail = vec![
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Bash"}]}}"#
                .to_string(),
        ];

        let status = compute_status(false, &tail);

        assert_eq!(status, SessionStatus::Working);
    }

    #[test]
    fn needs_input_when_not_idle_but_last_tool_use_has_no_result() {
        // Some interactive tools (AskUserQuestion in particular) block on
        // user input without the idle-ping hook having fired — the
        // transcript already shows a pending tool_use, and that should win
        // over the not-idle default of Working.
        let tail = vec![
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"AskUserQuestion"}]}}"#
                .to_string(),
        ];

        let status = compute_status(false, &tail);

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

    #[test]
    fn read_tail_lines_large_file_returns_correct_last_n_lines() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("large-transcript.jsonl");
        let mut f = File::create(&path).unwrap();
        for i in 0..20000 {
            writeln!(f, "line-{i:05}-padding-to-make-this-longer-than-trivial").unwrap();
        }
        drop(f);

        let tail = read_tail_lines(&path, 5);

        let expected: Vec<String> = (19995..20000)
            .map(|i| format!("line-{i:05}-padding-to-make-this-longer-than-trivial"))
            .collect();
        assert_eq!(tail, expected);
    }

    #[test]
    fn activity_for_working_reads_last_tool_use() {
        let tail = vec![
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"C:\\Projects\\foo\\bar.rs"}}]}}"#
                .to_string(),
        ];

        let activity = extract_activity(SessionStatus::Working, &tail);

        assert_eq!(activity, Some("Reading bar.rs".to_string()));
    }

    #[test]
    fn activity_for_working_falls_back_to_tool_name() {
        let tail = vec![
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"SomeCustomTool","input":{}}]}}"#
                .to_string(),
        ];

        let activity = extract_activity(SessionStatus::Working, &tail);

        assert_eq!(activity, Some("Using SomeCustomTool".to_string()));
    }

    #[test]
    fn activity_for_working_none_when_no_tool_use_in_tail() {
        let tail = vec![
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Thinking..."}]}}"#
                .to_string(),
        ];

        let activity = extract_activity(SessionStatus::Working, &tail);

        assert_eq!(activity, None);
    }

    #[test]
    fn activity_for_needs_input_distinguishes_ask_user_question() {
        let tail = vec![
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"AskUserQuestion","input":{}}]}}"#
                .to_string(),
        ];

        let activity = extract_activity(SessionStatus::NeedsInput, &tail);

        assert_eq!(activity, Some("Asked a question".to_string()));
    }

    #[test]
    fn activity_for_needs_input_names_the_pending_tool() {
        let tail = vec![
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}"#
                .to_string(),
        ];

        let activity = extract_activity(SessionStatus::NeedsInput, &tail);

        assert_eq!(activity, Some("Needs permission: Bash".to_string()));
    }

    #[test]
    fn activity_for_waiting_detects_trailing_question_mark() {
        let tail = vec![
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Should I use WPF or Tauri?"}]}}"#
                .to_string(),
        ];

        let activity = extract_activity(SessionStatus::Waiting, &tail);

        assert_eq!(activity, Some("Ended with a question".to_string()));
    }

    #[test]
    fn activity_for_waiting_none_when_plain_completion() {
        let tail = vec![
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Done, all tests pass."}]}}"#
                .to_string(),
        ];

        let activity = extract_activity(SessionStatus::Waiting, &tail);

        assert_eq!(activity, None);
    }
}
