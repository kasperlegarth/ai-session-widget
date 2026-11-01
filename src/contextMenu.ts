// Right-click menu (fork), shared by the widget's native popup menu and the
// tray icon's menu (main.ts). Pure: built from a snapshot of the current
// settings on every use, so check marks are always right, and covered by
// tests/contextMenu.test.ts. Native menus have no tooltips; About & help
// explains the settings instead.

import type { Menu } from "@tauri-apps/api/menu";
import type { WindowLayer } from "./settings";

/** One entry of a Menu.new item list. */
export type MenuEntry = NonNullable<NonNullable<Parameters<typeof Menu.new>[0]>["items"]>[number];

export interface MenuState {
  windowLayer: WindowLayer;
  showInTaskbar: boolean;
  autostart: boolean;
  compact: boolean;
  transparent: boolean;
  sizeMode: "auto" | "manual";
  theme: "light" | "dark" | "system";
  openInDesktop: boolean;
  sidebarSelect: boolean;
}

export interface MenuActions {
  setWindowLayer(layer: WindowLayer): void;
  toggleTaskbarButton(): void;
  toggleAutostart(): void;
  toggleCompact(): void;
  toggleTransparent(): void;
  setSizeMode(mode: MenuState["sizeMode"]): void;
  setTheme(theme: MenuState["theme"]): void;
  toggleOpenInDesktop(): void;
  toggleSidebarSelect(): void;
  refresh(): void;
  openAbout(): void;
  close(): void;
  /** Runs after every setting change; main.ts rebuilds the tray menu, which would otherwise keep old check marks. */
  changed(): void;
}

export interface TrayActions {
  bringToFront(): void;
  hide(): void;
}

export function contextMenuEntries(s: MenuState, a: MenuActions): MenuEntry[] {
  const after = (action: () => void) => () => {
    action();
    a.changed();
  };
  const pinned = s.windowLayer === "desktop";
  return [
    {
      text: "Window",
      items: [
        {
          text: "Always on top",
          checked: s.windowLayer === "top",
          action: after(() => a.setWindowLayer(s.windowLayer === "top" ? "normal" : "top")),
        },
        {
          text: "Pin to desktop",
          checked: pinned,
          action: after(() => a.setWindowLayer(pinned ? "top" : "desktop")),
        },
        {
          text: "Show in taskbar",
          checked: s.showInTaskbar && !pinned,
          enabled: !pinned, // pinned: never a taskbar button
          action: after(a.toggleTaskbarButton),
        },
        { item: "Separator" },
        { text: "Start with Windows", checked: s.autostart, action: after(a.toggleAutostart) },
      ],
    },
    {
      text: "View",
      items: [
        { text: "Compact view", checked: s.compact, action: after(a.toggleCompact) },
        { text: "Transparent background", checked: s.transparent, action: after(a.toggleTransparent) },
        { item: "Separator" },
        { text: "Auto size", checked: s.sizeMode === "auto", action: after(() => a.setSizeMode("auto")) },
        { text: "Manual size", checked: s.sizeMode === "manual", action: after(() => a.setSizeMode("manual")) },
        { item: "Separator" },
        { text: "Light theme", checked: s.theme === "light", action: after(() => a.setTheme("light")) },
        { text: "Dark theme", checked: s.theme === "dark", action: after(() => a.setTheme("dark")) },
        { text: "System theme", checked: s.theme === "system", action: after(() => a.setTheme("system")) },
      ],
    },
    {
      text: "On session click",
      items: [
        { text: "Open in Claude Desktop", checked: s.openInDesktop, action: after(a.toggleOpenInDesktop) },
        { text: "Select in sidebar (fallback)", checked: s.sidebarSelect, action: after(a.toggleSidebarSelect) },
      ],
    },
    { item: "Separator" },
    { text: "Refresh", action: a.refresh },
    { text: "About & help", action: a.openAbout },
    { item: "Separator" },
    { text: "Close", action: a.close },
  ];
}

/** Tray icon menu: "Bring to front" and "Hide widget", then the widget's own menu. */
export function trayMenuEntries(s: MenuState, a: MenuActions, t: TrayActions): MenuEntry[] {
  return [
    { text: "Bring to front", action: t.bringToFront },
    { text: "Hide widget", action: t.hide },
    { item: "Separator" },
    ...contextMenuEntries(s, a),
  ];
}
