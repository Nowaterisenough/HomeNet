import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = (file) => readFileSync(new URL(`../${file}`, import.meta.url), "utf8");
const base = JSON.parse(source("src-tauri/tauri.conf.json"));
const mac = JSON.parse(source("src-tauri/tauri.macos.conf.json"));

test("macOS keeps a native frame without changing shared window sizing or other platforms", () => {
  const window = mac.app.windows[0];
  assert.equal(base.app.windows[0].decorations, false);
  assert.equal(window.decorations, true);
  assert.equal(window.titleBarStyle, "Overlay");
  assert.equal(window.hiddenTitle, true);
  assert.equal(window.shadow, true);
  assert.equal(window.transparent, undefined);
  const { decorations, ...shared } = base.app.windows[0];
  for (const [key, value] of Object.entries(shared)) {
    assert.deepEqual(window[key], value, `Platform override must preserve ${key}`);
  }
  assert.deepEqual(window.trafficLightPosition, { x: 20, y: 27.75 });
  assert.equal(mac.app.windows.length, 1);
});

test("native chrome suppresses the synthetic border and duplicate traffic lights", () => {
  const app = source("src/App.tsx");
  const styles = source("src/styles/global.css");
  assert.match(app, /isMac && !nativeMacChrome \? windowControls : null/);
  assert.match(app, /!isMac \? windowControls : null/);
  assert.match(styles, /\.window-frame\[data-native-chrome="macos"\]\s*\{[^}]*border: 0;[^}]*border-radius: 0;/);
  assert.match(styles, /\.window-frame\[data-native-chrome="macos"\] \.titlebar\s*\{[^}]*padding-left: 96px;/);
});

test("AppKit appearance follows the app theme with explicit window permission", () => {
  const hook = source("src/hooks/useWindowChrome.ts");
  assert.match(hook, /setTheme\(theme === "system" \? null : theme\)/);
  const permissions = JSON.parse(source("src-tauri/capabilities/default.json")).permissions;
  assert.ok(permissions.includes("core:window:allow-set-theme"));
});

test("native fullscreen closing leaves its Space before hiding the background app", () => {
  assert.match(source("src-tauri/src/lib.rs"), /window_chrome::install_close_handler\(&window\)/);
  const handler = source("src-tauri/src/window_chrome.rs");
  assert.match(handler, /api\.prevent_close\(\)/);
  assert.match(handler, /current\.set_fullscreen\(false\)/);
  assert.match(handler, /WindowEvent::Resized\(_\)/);
  assert.match(handler, /!current\.is_fullscreen\(\)\.unwrap_or\(true\)/);
});
