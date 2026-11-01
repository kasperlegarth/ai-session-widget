// Unit tests for src/settings.ts (parse functions only; LocalStorage is not
// touched). Run with `npm test`.

import { test } from "node:test";
import assert from "node:assert/strict";
import { WINDOW_LAYERS, parseChoice, parseFlag } from "../src/settings.ts";

test("parseChoice keeps a known value", () => {
  assert.equal(parseChoice("desktop", WINDOW_LAYERS, "top"), "desktop");
  assert.equal(parseChoice("normal", WINDOW_LAYERS, "top"), "normal");
});

test("parseChoice falls back for nothing stored, unknown or differently cased values", () => {
  assert.equal(parseChoice(null, WINDOW_LAYERS, "top"), "top");
  assert.equal(parseChoice("", WINDOW_LAYERS, "top"), "top");
  assert.equal(parseChoice("Desktop", WINDOW_LAYERS, "top"), "top");
  assert.equal(parseChoice("bottom", WINDOW_LAYERS, "top"), "top");
});

test("parseFlag reads on/off and falls back otherwise", () => {
  assert.equal(parseFlag("on", false), true);
  assert.equal(parseFlag("off", true), false);
  assert.equal(parseFlag(null, true), true);
  assert.equal(parseFlag(null, false), false);
  assert.equal(parseFlag("true", false), false);
});
