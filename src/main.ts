import { invoke } from "@tauri-apps/api/core";

type SessionStatus = "working" | "needsInput" | "waiting";

interface SessionInfo {
  pid: number;
  sessionId: string;
  name: string;
  cwd: string;
  status: SessionStatus;
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
      return "Needs input";
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
    return;
  }

  for (const session of sessions) {
    const item = document.createElement("li");
    item.className = "session-row";
    item.title = session.cwd;

    const dot = document.createElement("span");
    dot.className = `status-dot status-${session.status}`;
    dot.title = statusLabel(session.status);

    const text = document.createElement("span");
    text.className = "session-text";

    const name = document.createElement("span");
    name.className = "session-name";
    name.textContent = session.name;

    const dir = document.createElement("span");
    dir.className = "session-dir";
    dir.textContent = lastPathSegment(session.cwd);

    text.appendChild(name);
    text.appendChild(dir);
    item.appendChild(dot);
    item.appendChild(text);

    item.addEventListener("click", () => {
      void invoke("focus_session", { pid: session.pid });
    });

    listEl.appendChild(item);
  }
}

async function refresh(): Promise<void> {
  try {
    const sessions = await invoke<SessionInfo[]>("get_sessions");
    render(sessions);
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
  contextMenu.style.left = `${x}px`;
  contextMenu.style.top = `${y}px`;
  contextMenu.hidden = false;
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
