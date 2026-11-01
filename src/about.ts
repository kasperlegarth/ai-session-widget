// About & help (fork): a short README with the version and a link to the
// repository, shown in place of the session view (#about-panel in
// index.html) and opened from the right-click or tray menu.

import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

const appWindow = getCurrentWindow();
const aboutPanel = document.getElementById("about-panel") as HTMLDivElement;
let layoutChanged: () => void = () => undefined;

export function setAboutOpen(open: boolean): void {
  if (open) void appWindow.show(); // also when opened from the tray menu
  aboutPanel.hidden = !open;
  document.body.classList.toggle("about-open", open);
  layoutChanged();
}

/** `onLayoutChange` re-fits the window (auto size) after opening or closing. */
export function initAbout(onLayoutChange: () => void): void {
  layoutChanged = onLayoutChange;

  getVersion()
    .then((version) => {
      (document.getElementById("about-version") as HTMLSpanElement).textContent = version;
    })
    .catch((err) => console.error("Failed to read the app version", err));

  document.getElementById("about-back-btn")?.addEventListener("click", () => setAboutOpen(false));
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && !aboutPanel.hidden) setAboutOpen(false);
  });
  // The URL is fixed in the backend (Cargo.toml `repository`).
  document.getElementById("about-repo-btn")?.addEventListener("click", () => {
    invoke("open_repository").catch((err) => window.alert(`Open repository: ${err}`));
  });
}
