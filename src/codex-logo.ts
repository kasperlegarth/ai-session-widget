import codexSvg from "./mascot-svgs/codex.svg?raw";

const template = new DOMParser().parseFromString(codexSvg, "image/svg+xml").documentElement;

export function createCodexLogoElement(): SVGSVGElement {
  const logo = document.importNode(template, true) as unknown as SVGSVGElement;
  logo.classList.add("codex-logo");
  return logo;
}
