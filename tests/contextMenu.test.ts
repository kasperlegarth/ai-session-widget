// Unit tests for src/contextMenu.ts: entries, check marks and actions of the
// right-click and tray menus. Run with `npm test`.

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  type MenuActions,
  type MenuEntry,
  type MenuState,
  contextMenuEntries,
  trayMenuEntries,
} from "../src/contextMenu.ts";

const DEFAULT_STATE: MenuState = {
  windowLayer: "top",
  showInTaskbar: true,
  autostart: false,
  compact: false,
  transparent: false,
  sizeMode: "auto",
  theme: "system",
  openInDesktop: true,
  sidebarSelect: false,
};

interface Entry {
  text?: string;
  checked?: boolean;
  enabled?: boolean;
  items?: MenuEntry[];
  action?: () => void;
}

/** Depth-first search by label, through submenus. */
function find(entries: MenuEntry[], text: string): Entry {
  for (const raw of entries) {
    const entry = raw as Entry;
    if (entry.text === text) return entry;
    if (entry.items) {
      try {
        return find(entry.items, text);
      } catch {
        // not in this submenu
      }
    }
  }
  throw new Error(`No menu entry "${text}"`);
}

/** Actions that only record what was called. */
function recordingActions(): { actions: MenuActions; calls: string[] } {
  const calls: string[] = [];
  const record =
    (name: string) =>
    (...args: unknown[]) =>
      calls.push(args.length > 0 ? `${name}:${String(args[0])}` : name);
  const actions = {
    setWindowLayer: record("setWindowLayer"),
    toggleTaskbarButton: record("toggleTaskbarButton"),
    toggleAutostart: record("toggleAutostart"),
    toggleCompact: record("toggleCompact"),
    toggleTransparent: record("toggleTransparent"),
    setSizeMode: record("setSizeMode"),
    setTheme: record("setTheme"),
    toggleOpenInDesktop: record("toggleOpenInDesktop"),
    toggleSidebarSelect: record("toggleSidebarSelect"),
    refresh: record("refresh"),
    openAbout: record("openAbout"),
    close: record("close"),
    changed: record("changed"),
  } as MenuActions;
  return { actions, calls };
}

test("check marks follow the state", () => {
  const { actions } = recordingActions();
  const state: MenuState = { ...DEFAULT_STATE, compact: true, transparent: true, sizeMode: "manual", theme: "dark" };
  const entries = contextMenuEntries(state, actions);
  assert.equal(find(entries, "Always on top").checked, true);
  assert.equal(find(entries, "Pin to desktop").checked, false);
  assert.equal(find(entries, "Compact view").checked, true);
  assert.equal(find(entries, "Transparent background").checked, true);
  assert.equal(find(entries, "Auto size").checked, false);
  assert.equal(find(entries, "Manual size").checked, true);
  assert.equal(find(entries, "Dark theme").checked, true);
  assert.equal(find(entries, "System theme").checked, false);
  assert.equal(find(entries, "Open in Claude Desktop").checked, true);
});

test("pinned to the desktop: Show in taskbar is unchecked and greyed out", () => {
  const { actions } = recordingActions();
  const entries = contextMenuEntries({ ...DEFAULT_STATE, windowLayer: "desktop" }, actions);
  assert.equal(find(entries, "Pin to desktop").checked, true);
  assert.equal(find(entries, "Always on top").checked, false);
  const taskbar = find(entries, "Show in taskbar");
  assert.equal(taskbar.checked, false);
  assert.equal(taskbar.enabled, false);
});

test("layer toggles: on top <-> normal, pinned <-> on top", () => {
  const top = recordingActions();
  find(contextMenuEntries(DEFAULT_STATE, top.actions), "Always on top").action?.();
  find(contextMenuEntries(DEFAULT_STATE, top.actions), "Pin to desktop").action?.();
  assert.deepEqual(top.calls, ["setWindowLayer:normal", "changed", "setWindowLayer:desktop", "changed"]);

  const pinned = recordingActions();
  find(contextMenuEntries({ ...DEFAULT_STATE, windowLayer: "desktop" }, pinned.actions), "Pin to desktop").action?.();
  assert.deepEqual(pinned.calls, ["setWindowLayer:top", "changed"]);
});

test("setting changes report `changed`, plain commands do not", () => {
  const { actions, calls } = recordingActions();
  const entries = contextMenuEntries(DEFAULT_STATE, actions);
  find(entries, "Light theme").action?.();
  find(entries, "Refresh").action?.();
  find(entries, "About & help").action?.();
  assert.deepEqual(calls, ["setTheme:light", "changed", "refresh", "openAbout"]);
});

test("tray menu starts with Bring to front and Hide widget, then the widget's menu", () => {
  const { actions } = recordingActions();
  const tray: string[] = [];
  const entries = trayMenuEntries(DEFAULT_STATE, actions, {
    bringToFront: () => tray.push("front"),
    hide: () => tray.push("hide"),
  });
  const labels = entries.map((e) => (e as Entry).text ?? "-");
  assert.deepEqual(labels.slice(0, 4), ["Bring to front", "Hide widget", "-", "Window"]);
  assert.equal(labels.at(-1), "Close");
  find(entries, "Bring to front").action?.();
  find(entries, "Hide widget").action?.();
  assert.deepEqual(tray, ["front", "hide"]);
});
