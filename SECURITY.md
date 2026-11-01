# Security policy

## Supported versions

Only the latest published release gets security fixes. Older versions are not patched; update to the latest release instead.

## Reporting a vulnerability

Please report security problems privately, not in a public issue:

1. Open the repository's **Security** tab and choose **Report a vulnerability** ([direct link](https://github.com/migoldschmidt/ai-code-session-widget/security/advisories/new)).
2. Describe what you found, how to reproduce it, the affected version (see About & help in the widget), and your Windows version.

The report is only visible to you and the maintainer until a fix is published. This is a personal project maintained in spare time: expect a first answer within about a week. Once a fix is released, the advisory is published, and you are credited unless you prefer otherwise.

## Scope

The widget runs locally on Windows. Relevant for security reports are, for example:

- reading of local session files (`~/.claude`, `~/.codex`) beyond what the README describes,
- the window and process functions (bringing other applications' windows to the front, ending orphaned processes, the desktop mode's window hook),
- the `claude://` deep link and the repository link handed to the Windows shell,
- the installers and the release workflow.

The widget makes no network requests and starts no other programs; a finding that contradicts this is especially welcome.

Out of scope: vulnerabilities in Claude Code, Claude Desktop or Codex themselves (report those to Anthropic or OpenAI), and findings that need an attacker who already controls the user's account.
