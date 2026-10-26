import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = (file) => readFileSync(new URL(`../${file}`, import.meta.url), "utf8");

test("native fullscreen closing leaves its Space before hiding the background app", () => {
  assert.match(source("src-tauri/src/lib.rs"), /window_chrome::install_close_handler\(&window\)/);
  const handler = source("src-tauri/src/window_chrome.rs");
  assert.match(handler, /api\.prevent_close\(\)/);
  assert.match(handler, /current\.set_fullscreen\(false\)/);
  assert.match(handler, /WindowEvent::Resized\(_\)/);
  assert.match(handler, /!current\.is_fullscreen\(\)\.unwrap_or\(true\)/);
});
