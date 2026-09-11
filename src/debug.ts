// Standalone animation debug page — not part of the shipped widget.
// Open via `npm run dev` (plain Vite, no Tauri needed) at
// http://localhost:1420/debug.html, alongside the real app, to tune
// mascot proportions/animations at a large zoomed-in scale.
//
// Reuses the real src/mascot.ts renderer directly, so any fix made here
// applies 1:1 to the actual widget — nothing is duplicated or mocked.

import { createMascotElement, mascots, MASCOT_WIDTH, MASCOT_HEIGHT, type MascotStatus } from "./mascot";
import { createCodexLogoElement } from "./codex-logo";

const MAX_ZOOM = 16;

const STATES: { pid: number; label: string; status: MascotStatus }[] = [
  { pid: 1, label: "working", status: "working" },
  { pid: 2, label: "needsInput", status: "needsInput" },
  { pid: 3, label: "waiting (plain, sleeping)", status: "waiting" },
  { pid: 4, label: "waitingAlert (ended with a question)", status: "waitingAlert" },
];

const stage = document.getElementById("stage") as HTMLDivElement;
const zoomSlider = document.getElementById("zoom") as HTMLInputElement;
const zoomValue = document.getElementById("zoom-value") as HTMLSpanElement;
const themeToggle = document.getElementById("theme-toggle") as HTMLButtonElement;

themeToggle.addEventListener("click", () => {
  const dark = document.documentElement.dataset.theme === "dark";
  document.documentElement.dataset.theme = dark ? "light" : "dark";
  themeToggle.textContent = dark ? "Dark mode" : "Light mode";
});

const previews: { svg: SVGSVGElement; wrapper: HTMLDivElement }[] = [];

function addPreview(frame: HTMLDivElement, label: string, svg: SVGSVGElement): void {
  const column = document.createElement("div");
  column.className = "debug-preview";
  const provider = document.createElement("span");
  provider.className = "debug-provider";
  provider.textContent = label;
  const wrapper = document.createElement("div");
  wrapper.className = "debug-animation";
  wrapper.appendChild(svg);
  column.append(provider, wrapper);
  frame.appendChild(column);
  previews.push({ svg, wrapper });
}

for (const state of STATES) {
  const card = document.createElement("div");
  card.className = "debug-card";

  const heading = document.createElement("h2");
  heading.textContent = state.label;

  const frame = document.createElement("div");
  frame.className = "debug-mascot-frame";

  const svg = createMascotElement();
  addPreview(frame, "Claude", svg);
  addPreview(frame, "Codex motor", createCodexLogoElement(state.status));
  addPreview(frame, "Codex portal", createCodexLogoElement(state.status, "portal"));
  addPreview(frame, "Codex >_", createCodexLogoElement(state.status, "terminal"));

  card.appendChild(heading);
  card.appendChild(frame);
  stage.appendChild(card);

  mascots.set(state.pid, svg, state.status);
}

zoomSlider.max = String(MAX_ZOOM);

function applyZoom(value: number): void {
  for (const { svg, wrapper } of previews) {
    svg.style.transform = `scale(${value})`;
    // Reserve exactly the space this zoom level needs, so cards never
    // overlap their neighbors (CSS transform:scale doesn't affect layout)
    // and scrolling stays proportional to what's actually on screen.
    wrapper.style.width = `${MASCOT_WIDTH * value}px`;
    wrapper.style.height = `${MASCOT_HEIGHT * value}px`;
  }
  zoomValue.textContent = `${value}x`;
}

zoomSlider.addEventListener("input", () => {
  applyZoom(Number(zoomSlider.value));
});

applyZoom(Number(zoomSlider.value));
