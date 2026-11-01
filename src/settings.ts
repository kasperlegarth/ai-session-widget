// Saved settings (fork): small typed helpers around the WebView's
// LocalStorage, so each setting is one line instead of hand-written
// parsing. The parse functions are pure and covered by
// tests/settings.test.ts; the load/save helpers only touch LocalStorage
// when called.

export type WindowLayer = "top" | "normal" | "desktop";
export const WINDOW_LAYERS: readonly WindowLayer[] = ["top", "normal", "desktop"];

/** `raw` if it is one of `values`, else `fallback` (also when nothing is stored). */
export function parseChoice<T extends string>(raw: string | null, values: readonly T[], fallback: T): T {
  return (values as readonly string[]).includes(raw ?? "") ? (raw as T) : fallback;
}

/** "on" / "off" as stored by saveFlag; anything else is `fallback`. */
export function parseFlag(raw: string | null, fallback: boolean): boolean {
  if (raw === "on") return true;
  if (raw === "off") return false;
  return fallback;
}

export function loadChoice<T extends string>(key: string, values: readonly T[], fallback: T): T {
  return parseChoice(localStorage.getItem(key), values, fallback);
}

export function loadFlag(key: string, fallback: boolean): boolean {
  return parseFlag(localStorage.getItem(key), fallback);
}

export function saveFlag(key: string, value: boolean): void {
  localStorage.setItem(key, value ? "on" : "off");
}
