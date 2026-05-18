import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
const appVue = readFileSync(join(root, "src/App.vue"), "utf8");

const requiredSnippets = [
  'const autoStartEnabled = ref(false);',
  'const autoStartSaving = ref(false);',
  'invoke<boolean>("get_auto_start")',
  'await invoke("set_auto_start", { enabled });',
  'async function loadAutoStart()',
  'async function toggleAutoStart',
  'loadAutoStart();',
  'class="autostart-option"',
  'type="checkbox"',
  'aria-label="开机自启"',
  '@change="toggleAutoStart"',
  '开机自启',
  '.autostart-option',
  '.autostart-checkbox',
  'button, input, select, textarea, a, label',
];

const missing = requiredSnippets.filter((snippet) => !appVue.includes(snippet));

if (missing.length > 0) {
  console.error("Autostart control checks failed:");
  for (const snippet of missing) {
    console.error(`- Missing ${snippet}`);
  }
  process.exit(1);
}

console.log("Autostart control checks passed.");
