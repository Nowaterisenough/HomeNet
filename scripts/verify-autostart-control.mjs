import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
const appSource = readFileSync(join(root, "src/App.tsx"), "utf8");
const runtimeSource = readFileSync(join(root, "src/components/RuntimeSettingsPanel.tsx"), "utf8");
const panelStyles = readFileSync(join(root, "src/styles/panels.css"), "utf8");
const combined = `${appSource}\n${runtimeSource}\n${panelStyles}`;

const requiredSnippets = [
  "const [autoStartEnabled, setAutoStartEnabled] = useState(false);",
  "const [autoStartSaving, setAutoStartSaving] = useState(false);",
  'invokeCommand<boolean>("get_auto_start")',
  'await invokeCommand("set_auto_start", { enabled });',
  "async function loadAutoStart()",
  "async function toggleAutoStart",
  "loadAutoStart();",
  "autoStartEnabled",
  "autoStartSaving",
  "onToggleAutostart",
  'type="checkbox"',
  "开机自动启动",
  ".setting-toggle",
  "button, input, select, textarea, a, label",
];

const missing = requiredSnippets.filter((snippet) => !combined.includes(snippet));

if (missing.length > 0) {
  console.error("Autostart control checks failed:");
  for (const snippet of missing) {
    console.error(`- Missing ${snippet}`);
  }
  process.exit(1);
}

console.log("Autostart control checks passed.");
