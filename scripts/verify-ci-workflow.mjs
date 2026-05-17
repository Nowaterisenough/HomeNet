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
  "tags:",
  "v*",
  "workflow_dispatch:",
  "contents: write",
  "macos-15",
  "aarch64-apple-darwin",
  "windows-latest",
  "tauri-apps/tauri-action@v0.6.2",
  "release:",
  "needs: build",
  "actions/download-artifact@v5",
  "mikepenz/release-changelog-builder-action@v6",
  "id: build_changelog",
  "mode: COMMIT",
  "offlineMode: true",
  "failOnError: true",
  "configurationJson:",
  "label_extractor",
  "softprops/action-gh-release@v3",
  "body: ${{ steps.build_changelog.outputs.changelog }}",
  "files: release-assets/*",
  "name: 网络管家",
  "HomeNet_${{ github.ref_name }}_${{ matrix.assetSuffix }}",
  "assetSuffix: macos-arm64.dmg",
  "assetSuffix: windows-x64-setup.exe",
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
