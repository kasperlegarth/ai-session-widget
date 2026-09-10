# Codex sessions

The widget automatically reads local Codex sessions from `CODEX_HOME`, or
`%USERPROFILE%\.codex` by default. No hooks, API key or configuration changes
are required. Claude and Codex share the existing status cards. Claude uses the
animated mascot; Codex uses a static, theme-aware logo. Animation identity uses provider and session ID because several Codex
threads can share a process.

On Windows, a held `thread-writer-locks/<thread-id>.lock` identifies a loaded
session. An old, unlocked file is ignored. Windows Restart Manager is used only
to query the owning Codex PID for the existing window-focus action. If ownership
cannot be resolved, the card still appears but clicking it does nothing. Focus
remains best effort when several conversations share one app window.

Rollouts under `sessions/YYYY/MM/DD` supply the project and activity. Each poll
reads newly appended complete JSONL lines; partial writes are retried. Completed
and interrupted turns become Idle, started turns become Working, and pending
`request_user_input` calls become Waiting for input. Ordinary pending commands
remain Working. Subagent rollouts are excluded.

## Limits

This adapter uses internal files observed in Codex 0.154.0, not a stable public
API. Older versions without writer locks, remote/WSL sessions, archived history
and ephemeral sessions without rollouts are not shown. A loaded idle thread may
remain visible until Codex releases its writer lock.

Approval prompts and questions hidden inside code-mode calls are not reliably
recorded in rollouts; those can remain Working while awaiting input. Do not infer
approval from a slow tool call. For fully authoritative approval status, a future
integration must connect to the **existing** app-server transport and consume
`thread/status/changed` (including `waitingOnApproval`). Starting a separate
app-server does not monitor the other server's loaded threads. See the
[official App Server documentation](https://learn.chatgpt.com/docs/app-server).

Validation: `npm run build` and `cargo test --manifest-path src-tauri/Cargo.toml --lib`.
With Codex open, the optional read-only diagnostic is
`cargo test --manifest-path src-tauri/Cargo.toml inspect_local_sessions -- --ignored --nocapture`.
