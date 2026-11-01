// Window layer (fork): always on top, normal, or pinned to the desktop (see
// src-tauri/src/desktop.rs), the taskbar button, and bringing a pinned
// widget up for a while from the tray icon. All of that state lives in this
// module; main.ts calls initWindowLayer() once at startup.

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { type WindowLayer, WINDOW_LAYERS, loadChoice, loadFlag, saveFlag } from "./settings";

const appWindow = getCurrentWindow();

const WINDOW_LAYER_STORAGE_KEY = "window-layer";
const SHOW_IN_TASKBAR_STORAGE_KEY = "show-in-taskbar";

/** Single click on the tray icon while pinned: up for this long. */
const TRAY_CLICK_RAISE_MS = 5_000;
/**
 * "Bring to front" while pinned ends when the widget loses focus, at the
 * latest after this — also when Windows refused the focus and the blur
 * that ends it never comes.
 */
const RAISE_FALLBACK_MS = 60_000;

// "top" and a visible taskbar button are the tauri.conf.json defaults.
let windowLayer: WindowLayer = loadChoice(WINDOW_LAYER_STORAGE_KEY, WINDOW_LAYERS, "top");
let showInTaskbar = loadFlag(SHOW_IN_TASKBAR_STORAGE_KEY, true);
// A pinned widget temporarily on top (bringToFront), and the timer ending it.
let raisedFromDesktop = false;
let raiseTimer: number | undefined;

export function getWindowLayer(): WindowLayer {
  return windowLayer;
}

export function isShownInTaskbar(): boolean {
  return showInTaskbar;
}

export function setWindowLayer(layer: WindowLayer): void {
  raisedFromDesktop = false;
  window.clearTimeout(raiseTimer);
  windowLayer = layer;
  localStorage.setItem(WINDOW_LAYER_STORAGE_KEY, layer);
  invoke("set_window_layer", { layer })
    // After the layer: the desktop owner (desktop.rs) can change the button.
    .then(() => applyTaskbarButton())
    .catch((err) => console.error("Failed to set the window layer", err));
}

// The tray icon stays either way. Pinned to the desktop the widget never
// has a taskbar button, like a gadget.
function applyTaskbarButton(): void {
  appWindow
    .setSkipTaskbar(windowLayer === "desktop" || !showInTaskbar)
    .catch((err) => console.error("Failed to set the taskbar button", err));
}

export function toggleTaskbarButton(): void {
  showInTaskbar = !showInTaskbar;
  saveFlag(SHOW_IN_TASKBAR_STORAGE_KEY, showInTaskbar);
  applyTaskbarButton();
}

/**
 * Shows the widget above all windows (tray click and double click, tray menu,
 * dragging). Pinned to the desktop that is temporary, so the widget can be found
 * under a screen full of windows: it goes back onto the desktop when it
 * loses focus or after `limitMs`.
 */
export async function bringToFront(limitMs = RAISE_FALLBACK_MS): Promise<void> {
  try {
    window.clearTimeout(raiseTimer);
    await appWindow.show();
    await appWindow.unminimize();
    if (windowLayer === "desktop") {
      // Straight from Rust, also while Windows drags the window (desktop.rs).
      await invoke("raise_pinned", { raised: true });
      raisedFromDesktop = true;
      raiseTimer = window.setTimeout(() => dropBackToDesktop(), limitMs);
    }
    await appWindow.setFocus();
  } catch (err) {
    console.error("Failed to bring the widget to the front", err);
  }
}

function dropBackToDesktop(): void {
  if (!raisedFromDesktop) return;
  raisedFromDesktop = false;
  window.clearTimeout(raiseTimer);
  if (windowLayer === "desktop") {
    invoke("raise_pinned", { raised: false }).catch((err) => console.error("Failed to move the widget back", err));
  }
}

/**
 * Single click on the tray icon: always brings the widget to the front, also
 * when hidden or minimized; pinned to the desktop only for 5 s. Hiding is
 * the tray menu's "Hide widget".
 */
export function onTrayClick(): Promise<void> {
  return bringToFront(windowLayer === "desktop" ? TRAY_CLICK_RAISE_MS : undefined);
}

/**
 * Tray menu "Hide widget". With a taskbar button the widget is minimized
 * instead: a hidden window loses its taskbar button, a minimized one keeps
 * it (and the taskbar click brings the widget back).
 */
export function hideWidget(): void {
  const hasTaskbarButton = showInTaskbar && windowLayer !== "desktop";
  (hasTaskbarButton ? appWindow.minimize() : appWindow.hide()).catch((err) =>
    console.error("Failed to hide the widget", err),
  );
}

/**
 * Pressing a drag region of a pinned widget: up above the other windows
 * while it is being moved, so it is not dragged blind behind them. Rust
 * raises it and lowers it shortly after the mouse is released (desktop.rs);
 * nothing here waits on the main thread, which Windows holds during a drag.
 */
function raiseForDrag(): void {
  if (windowLayer !== "desktop" || raisedFromDesktop) return;
  invoke("raise_pinned_for_drag").catch((err) => console.error("Failed to raise the widget for dragging", err));
}

/**
 * Applies the saved layer and with it the taskbar button; ends a temporary
 * raise on blur; raises a pinned widget while it is dragged.
 */
export function initWindowLayer(): void {
  // Drag regions: title, free space of the filter bar, system status bar.
  document.addEventListener("pointerdown", (e) => {
    if (e.button === 0 && (e.target as Element).hasAttribute?.("data-tauri-drag-region")) raiseForDrag();
  });
  void appWindow.onFocusChanged(({ payload: focused }) => {
    // Pinned and not raised: desktop.rs moves the widget back down itself
    // when another window comes to the foreground.
    if (!focused) dropBackToDesktop();
  });
  setWindowLayer(windowLayer);
}
