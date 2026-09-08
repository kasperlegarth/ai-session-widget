import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { createMascotElement, mascots, type MascotStatus } from "./mascot";

const WINDOW_WIDTH = 340;
const appEl = document.getElementById("app") as HTMLDivElement;
const appWindow = getCurrentWindow();

// Window has no OS-drawn resize handles (decorations: false, resizable:
// false in tauri.conf.json) — this is the only thing that ever changes its
// height, so it always matches however many session cards are showing.
async function resizeWindowToContent(): Promise<void> {
  try {
    await appWindow.setSize(new LogicalSize(WINDOW_WIDTH, appEl.scrollHeight));
  } catch (err) {
    console.error("Failed to resize window to content", err);
  }
}

type SessionStatus = "working" | "needsInput" | "waiting";

interface SessionInfo {
  pid: number;
  sessionId: string;
  name: string;
  cwd: string;
  status: SessionStatus;
  activity: string | null;
}

// "waitingAlert" is a frontend-only presentation variant: a plain "waiting"
// session whose last message read like a question gets a distinct color and
// mascot animation from an ordinary finished/idle "waiting" session, even
// though both share the same backend SessionStatus.
function visualStatus(session: SessionInfo): MascotStatus {
  if (session.status === "waiting" && session.activity === "Ended with a question") {
    return "waitingAlert";
  }
  return session.status;
}

const POLL_INTERVAL_MS = 2000;

const listEl = document.getElementById("session-list") as HTMLUListElement;

function lastPathSegment(cwd: string): string {
  const normalized = cwd.replace(/\\/g, "/").replace(/\/$/, "");
  const parts = normalized.split("/");
  return parts[parts.length - 1] || cwd;
}

function statusLabel(status: SessionStatus): string {
  switch (status) {
    case "working":
      return "Working";
    case "needsInput":
      return "Waiting for input";
    case "waiting":
      return "Waiting";
  }
}

function render(sessions: SessionInfo[]): void {
  listEl.innerHTML = "";

  if (sessions.length === 0) {
    const empty = document.createElement("li");
    empty.className = "empty-state";
    empty.textContent = "Ingen aktive sessioner";
    listEl.appendChild(empty);
    mascots.prune(new Set());
    return;
  }

  for (const session of sessions) {
    const item = document.createElement("li");
    item.className = "session-card";
    item.title = session.cwd;

    const mascotEl = createMascotElement();
    mascotEl.classList.add("mascot-svg");
    item.appendChild(mascotEl);

    const text = document.createElement("div");
    text.className = "session-text";

    const dir = document.createElement("span");
    dir.className = "session-name";
    dir.textContent = lastPathSegment(session.cwd);

    const statusRow = document.createElement("span");
    statusRow.className = "session-status-row";

    const visual = visualStatus(session);

    const statusDot = document.createElement("span");
    statusDot.className = `status-dot status-dot-${visual}`;

    const statusText = document.createElement("span");
    statusText.className = `session-status status-text-${visual}`;
    statusText.textContent = statusLabel(session.status);

    statusRow.appendChild(statusDot);
    statusRow.appendChild(statusText);

    text.appendChild(dir);
    text.appendChild(statusRow);

    if (session.activity) {
      const activity = document.createElement("span");
      activity.className = "session-activity";
      activity.textContent = session.activity;
      text.appendChild(activity);
    }

    item.appendChild(text);

    item.addEventListener("click", () => {
      if (session.pid < 0) return; // fake debug row, nothing to focus
      // hint disambiguates between several windows sharing one process id
      // (e.g. multiple separate Windows Terminal windows) — see focus.rs.
      void invoke("focus_session", { pid: session.pid, hint: lastPathSegment(session.cwd) });
    });

    listEl.appendChild(item);
    mascots.set(session.pid, mascotEl, visual);
  }

  mascots.prune(new Set(sessions.map((s) => s.pid)));
}

// TEMP DEBUG: replaces all real sessions with a fixed set of demo rows
// covering every activity message, for a full visual review. Remove
// before shipping — see DEMO_MODE below.
const DEMO_MODE = false;

function demoSession(
  pid: number,
  name: string,
  status: SessionStatus,
  activity: string | null,
): SessionInfo {
  return {
    pid,
    sessionId: `demo-${pid}`,
    name,
    cwd: `C:\\Projects\\${name}`,
    status,
    activity,
  };
}

const DEMO_SESSIONS: SessionInfo[] = [
  demoSession(-1, "reading-file", "working", "Reading bar.rs"),
  demoSession(-2, "editing-file", "working", "Editing main.ts"),
  demoSession(-3, "searching", "working", "Searching"),
  demoSession(-4, "running-command", "working", "Running a command"),
  demoSession(-5, "searching-web", "working", "Searching the web"),
  demoSession(-6, "custom-tool", "working", "Using SomeCustomTool"),
  demoSession(-7, "working-no-activity", "working", null),
  demoSession(-8, "needs-permission", "needsInput", "Needs permission: Bash"),
  demoSession(-9, "needs-answer", "needsInput", "Asked a question"),
  demoSession(-10, "idle-ended-question", "waiting", "Ended with a question"),
  demoSession(-11, "idle-plain", "waiting", null),
];

async function refresh(): Promise<void> {
  if (DEMO_MODE) {
    render(DEMO_SESSIONS);
    await resizeWindowToContent();
    return;
  }
  try {
    const sessions = await invoke<SessionInfo[]>("get_sessions");
    render(sessions);
    await resizeWindowToContent();
  } catch (err) {
    console.error("Failed to refresh sessions", err);
  }
}

void refresh();
setInterval(() => void refresh(), POLL_INTERVAL_MS);

// Exposed for the context menu (Task 10) to trigger a manual refresh.
(window as unknown as { __refreshSessions: () => void }).__refreshSessions = () => void refresh();

const contextMenu = document.getElementById("context-menu") as HTMLDivElement;
const menuAlwaysOnTop = document.getElementById("menu-always-on-top") as HTMLDivElement;
const menuRefresh = document.getElementById("menu-refresh") as HTMLDivElement;
const menuClose = document.getElementById("menu-close") as HTMLDivElement;

let alwaysOnTop = true; // matches tauri.conf.json default

function updateAlwaysOnTopCheckmark(): void {
  menuAlwaysOnTop.classList.toggle("checked", alwaysOnTop);
}
updateAlwaysOnTopCheckmark();

function showContextMenu(x: number, y: number): void {
  contextMenu.hidden = false;
  contextMenu.style.left = `${x}px`;
  contextMenu.style.top = `${y}px`;

  const margin = 4;
  const rect = contextMenu.getBoundingClientRect();
  const maxX = window.innerWidth - rect.width - margin;
  const maxY = window.innerHeight - rect.height - margin;
  contextMenu.style.left = `${Math.min(x, Math.max(margin, maxX))}px`;
  contextMenu.style.top = `${Math.min(y, Math.max(margin, maxY))}px`;
}

function hideContextMenu(): void {
  contextMenu.hidden = true;
}

document.addEventListener("contextmenu", (e) => {
  e.preventDefault();
  showContextMenu(e.clientX, e.clientY);
});

document.addEventListener("click", (e) => {
  if (!contextMenu.contains(e.target as Node)) {
    hideContextMenu();
  }
});

menuAlwaysOnTop.addEventListener("click", () => {
  alwaysOnTop = !alwaysOnTop;
  updateAlwaysOnTopCheckmark();
  void invoke("set_always_on_top", { enabled: alwaysOnTop });
  hideContextMenu();
});

menuRefresh.addEventListener("click", () => {
  (window as unknown as { __refreshSessions: () => void }).__refreshSessions();
  hideContextMenu();
});

menuClose.addEventListener("click", () => {
  void invoke("close_app");
  hideContextMenu();
});
