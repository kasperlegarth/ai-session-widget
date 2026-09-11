import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { createMascotElement, mascots, type MascotStatus } from "./mascot";
import { createCodexLogoElement } from "./codex-logo";

const WINDOW_WIDTH = 340;
const appEl = document.getElementById("app") as HTMLDivElement;
const appWindow = getCurrentWindow();

type ThemePreference = "light" | "dark" | "system";
const THEME_STORAGE_KEY = "theme-preference";

function loadThemePreference(): ThemePreference {
  const stored = localStorage.getItem(THEME_STORAGE_KEY);
  return stored === "light" || stored === "dark" || stored === "system" ? stored : "system";
}

let themePreference: ThemePreference = loadThemePreference();

type SizeMode = "auto" | "manual";
const SIZE_MODE_STORAGE_KEY = "size-mode";

function loadSizeMode(): SizeMode {
  const stored = localStorage.getItem(SIZE_MODE_STORAGE_KEY);
  return stored === "manual" ? "manual" : "auto";
}

let sizeMode: SizeMode = loadSizeMode();
const systemDarkQuery = window.matchMedia("(prefers-color-scheme: dark)");

function applyTheme(): void {
  const resolved = themePreference === "system" ? (systemDarkQuery.matches ? "dark" : "light") : themePreference;
  document.documentElement.setAttribute("data-theme", resolved);
}

applyTheme();
systemDarkQuery.addEventListener("change", () => {
  if (themePreference === "system") applyTheme();
});

// Auto-fits the window height to however many session cards are showing.
// Skipped in "manual" size mode, where the user has taken over sizing via
// the OS resize border (see applySizeMode) and we must not fight them.
async function resizeWindowToContent(): Promise<void> {
  if (sizeMode === "manual") return;
  try {
    await appWindow.setSize(new LogicalSize(WINDOW_WIDTH, appEl.scrollHeight));
  } catch (err) {
    console.error("Failed to resize window to content", err);
  }
}

// tauri.conf.json ships with resizable: false and decorations: false, so
// there's no OS-drawn resize border to grab even once resizable is true —
// decorations:false removes the non-client area Windows would otherwise
// hit-test for resize cursors. The #resize-handles strips (shown only in
// manual mode, see style.css) stand in for it via startResizeDragging().
async function applySizeMode(): Promise<void> {
  document.body.dataset.sizeMode = sizeMode;
  try {
    await appWindow.setResizable(sizeMode === "manual");
  } catch (err) {
    console.error("Failed to apply size mode", err);
  }
  if (sizeMode === "auto") {
    await resizeWindowToContent();
  }
}

void applySizeMode();

for (const handle of document.querySelectorAll<HTMLDivElement>(".resize-handle")) {
  const direction = handle.dataset.direction as
    | "North"
    | "South"
    | "East"
    | "West"
    | "NorthEast"
    | "NorthWest"
    | "SouthEast"
    | "SouthWest";
  handle.addEventListener("pointerdown", (e) => {
    e.preventDefault();
    appWindow.startResizeDragging(direction).catch((err) => {
      console.error("Failed to start resize dragging", err);
    });
  });
}

type SessionStatus = "working" | "needsInput" | "waiting";

interface SessionInfo {
  provider: "claude" | "codex";
  pid: number;
  sessionId: string;
  name: string;
  cwd: string;
  status: SessionStatus;
  activity: string | null;
}

interface OrphanProcess {
  pid: number;
  name: string;
  cpuPercent: number;
  diskBytes: number;
}

interface ResourceUsage {
  sessionCpuPercent: number;
  totalCpuPercent: number;
  sessionMemoryBytes: number;
  totalMemoryBytes: number;
  sessionDiskBytes: number;
  totalDiskBytes: number;
  orphanedProcesses: OrphanProcess[];
}

interface SessionsPayload {
  sessions: SessionInfo[];
  usage: ResourceUsage;
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

function statusLabel(status: MascotStatus): string {
  switch (status) {
    case "working":
      return "Working";
    case "needsInput":
      return "Waiting for input";
    case "waitingAlert":
      return "Waiting";
    case "waiting":
      return "Idle";
  }
}

const usageCpuEl = document.getElementById("usage-cpu") as HTMLSpanElement;
const usageMemEl = document.getElementById("usage-mem") as HTMLSpanElement;
const usageDiskEl = document.getElementById("usage-disk") as HTMLSpanElement;
const orphanWarningEl = document.getElementById("orphan-warning") as HTMLDivElement;

function formatPercent(value: number): string {
  return `${Math.round(value)}%`;
}

function formatBytes(bytes: number): string {
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${gb.toFixed(1)} GB`;
  return `${Math.round(bytes / 1024 ** 2)} MB`;
}

function formatBytesPerSec(bytesPerPoll: number): string {
  const bytesPerSec = bytesPerPoll / (POLL_INTERVAL_MS / 1000);
  const mbPerSec = bytesPerSec / 1024 ** 2;
  if (mbPerSec >= 1) return `${mbPerSec.toFixed(1)} MB/s`;
  return `${Math.round(bytesPerSec / 1024)} KB/s`;
}

function renderUsage(usage: ResourceUsage): void {
  usageCpuEl.textContent = `${formatPercent(usage.sessionCpuPercent)} / ${formatPercent(usage.totalCpuPercent)}`;
  const memPercent = usage.totalMemoryBytes > 0 ? (usage.sessionMemoryBytes / usage.totalMemoryBytes) * 100 : 0;
  usageMemEl.textContent = `${formatBytes(usage.sessionMemoryBytes)} (${formatPercent(memPercent)})`;
  const diskPercent = usage.totalDiskBytes > 0 ? (usage.sessionDiskBytes / usage.totalDiskBytes) * 100 : 0;
  usageDiskEl.textContent = `${formatBytesPerSec(usage.sessionDiskBytes)} (${formatPercent(diskPercent)})`;
  renderOrphanWarning(usage.orphanedProcesses);
}

// Flags processes left behind by a tool call whose parent shell has already
// exited (e.g. a backgrounded `find /` that timed out and was never killed)
// — see usage.rs::find_runaway_orphans for the detection rule.
function renderOrphanWarning(orphans: OrphanProcess[]): void {
  if (orphans.length === 0) {
    orphanWarningEl.hidden = true;
    return;
  }
  orphanWarningEl.hidden = false;
  const noun = orphans.length === 1 ? "forældreløs proces" : "forældreløse processer";
  orphanWarningEl.textContent = `⚠ ${orphans.length} ${noun} kører stadig`;
  orphanWarningEl.title = orphans
    .map((o) => `${o.name} (PID ${o.pid}, ${Math.round(o.cpuPercent)}% CPU)`)
    .join("\n");
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
    item.title = `${session.provider === "codex" ? "Codex" : "Claude Code"}: ${session.name}\n${session.cwd}`;

    const visual = visualStatus(session);
    const mascotEl = session.provider === "codex"
      ? createCodexLogoElement(visual, "terminal", session.sessionId)
      : createMascotElement();
    if (session.provider === "claude") mascotEl.classList.add("mascot-svg");
    item.appendChild(mascotEl);

    const text = document.createElement("div");
    text.className = "session-text";

    const dir = document.createElement("span");
    dir.className = "session-name";
    dir.textContent = lastPathSegment(session.cwd);

    const statusRow = document.createElement("span");
    statusRow.className = "session-status-row";

    const statusDot = document.createElement("span");
    statusDot.className = `status-dot status-dot-${visual}`;

    const statusText = document.createElement("span");
    statusText.className = `session-status status-text-${visual}`;
    statusText.textContent = statusLabel(visual);

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
      if (session.pid <= 0) return; // unknown owner or demo row
      // hint disambiguates between several windows sharing one process id
      // (e.g. multiple separate Windows Terminal windows) — see focus.rs.
      void invoke("focus_session", { pid: session.pid, hint: lastPathSegment(session.cwd) });
    });

    listEl.appendChild(item);
    if (session.provider === "claude") {
      mascots.set(`${session.provider}:${session.sessionId}`, mascotEl, visual);
    }
  }

  mascots.prune(new Set(sessions.filter((s) => s.provider === "claude").map((s) => `${s.provider}:${s.sessionId}`)));
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
    provider: "claude",
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

const DEMO_USAGE: ResourceUsage = {
  sessionCpuPercent: 34,
  totalCpuPercent: 61,
  sessionMemoryBytes: 1_200_000_000,
  totalMemoryBytes: 17_000_000_000,
  sessionDiskBytes: 2_400_000,
  totalDiskBytes: 40_000_000,
  orphanedProcesses: [],
};

let refreshing = false;

async function refresh(): Promise<void> {
  if (refreshing) return;
  if (DEMO_MODE) {
    render(DEMO_SESSIONS);
    renderUsage(DEMO_USAGE);
    await resizeWindowToContent();
    return;
  }
  refreshing = true;
  try {
    const payload = await invoke<SessionsPayload>("get_sessions");
    render(payload.sessions);
    renderUsage(payload.usage);
    await resizeWindowToContent();
  } catch (err) {
    console.error("Failed to refresh sessions", err);
  } finally {
    refreshing = false;
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
const themeMenuItems: Record<ThemePreference, HTMLDivElement> = {
  light: document.getElementById("menu-theme-light") as HTMLDivElement,
  dark: document.getElementById("menu-theme-dark") as HTMLDivElement,
  system: document.getElementById("menu-theme-system") as HTMLDivElement,
};
const sizeMenuItems: Record<SizeMode, HTMLDivElement> = {
  auto: document.getElementById("menu-size-auto") as HTMLDivElement,
  manual: document.getElementById("menu-size-manual") as HTMLDivElement,
};

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

function updateThemeCheckmarks(): void {
  for (const [pref, el] of Object.entries(themeMenuItems)) {
    el.classList.toggle("checked", pref === themePreference);
  }
}
updateThemeCheckmarks();

for (const [pref, el] of Object.entries(themeMenuItems)) {
  el.addEventListener("click", () => {
    themePreference = pref as ThemePreference;
    localStorage.setItem(THEME_STORAGE_KEY, themePreference);
    applyTheme();
    updateThemeCheckmarks();
    hideContextMenu();
  });
}

function updateSizeMenuCheckmarks(): void {
  for (const [mode, el] of Object.entries(sizeMenuItems)) {
    el.classList.toggle("checked", mode === sizeMode);
  }
}
updateSizeMenuCheckmarks();

for (const [mode, el] of Object.entries(sizeMenuItems)) {
  el.addEventListener("click", () => {
    sizeMode = mode as SizeMode;
    localStorage.setItem(SIZE_MODE_STORAGE_KEY, sizeMode);
    updateSizeMenuCheckmarks();
    void applySizeMode();
    hideContextMenu();
  });
}

menuRefresh.addEventListener("click", () => {
  (window as unknown as { __refreshSessions: () => void }).__refreshSessions();
  hideContextMenu();
});

menuClose.addEventListener("click", () => {
  void invoke("close_app");
  hideContextMenu();
});
