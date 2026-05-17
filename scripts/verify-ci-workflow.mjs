import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
let workflow = "";
try {
  workflow = readFileSync(join(root, ".github/workflows/build.yml"), "utf8");
} catch {
  console.error("CI workflow checks failed:");
  console.error("- Missing .github/workflows/build.yml");
  process.exit(1);
}

const requiredSnippets = [
  "name: Build desktop packages",
  "workflow_dispatch:",
  "macos-15",
  "aarch64-apple-darwin",
  "windows-latest",
  "tauri-apps/tauri-action@v0.6.2",
  "actions/upload-artifact@v4",
  "artifactGlob:",
  "pnpm/action-setup@v4",
  "actions/setup-node@v4",
  "dtolnay/rust-toolchain@stable",
  "pnpm install --frozen-lockfile",
];

const missing = requiredSnippets.filter((snippet) => !workflow.includes(snippet));

if (missing.length > 0) {
  console.error("CI workflow checks failed:");
  for (const snippet of missing) {
    console.error(`- Missing ${snippet}`);
  }
  process.exit(1);
}

console.log("CI workflow checks passed.");
