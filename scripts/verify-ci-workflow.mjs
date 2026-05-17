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
  "name: Build desktop artifacts",
  "tags:",
  "v*",
  "workflow_dispatch:",
  "contents: write",
  "macos-15",
  "aarch64-apple-darwin",
  "windows-latest",
  "tauri-apps/tauri-action@v0.6.2",
  "--bundles dmg",
  "--bundles nsis",
  "portableGlob:",
  "installerGlob:",
  "portableMode: zip",
  "portableMode: file",
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
  "name: homenet ${{ github.ref_name }}",
  "homenet_${{ github.ref_name }}_${{ matrix.portableSuffix }}",
  "homenet_${{ github.ref_name }}_${{ matrix.installerSuffix }}",
  "portableSuffix: macos-arm64-app.zip",
  "installerSuffix: macos-arm64.dmg",
  "portableSuffix: windows-x64-portable.exe",
  "installerSuffix: windows-x64-setup.exe",
  "actions/upload-artifact@v4",
  "pnpm/action-setup@v4",
  "actions/setup-node@v4",
  "dtolnay/rust-toolchain@stable",
  "pnpm install --frozen-lockfile",
];

const missing = requiredSnippets.filter((snippet) => !workflow.includes(snippet));
const forbiddenSnippets = [
  "网络管家",
  "HomeNet_",
  "--no-bundle",
  "assetSuffix:",
  "assetMode:",
  "artifactGlob:",
];
const forbidden = forbiddenSnippets.filter((snippet) => workflow.includes(snippet));

if (missing.length > 0 || forbidden.length > 0) {
  console.error("CI workflow checks failed:");
  for (const snippet of missing) {
    console.error(`- Missing ${snippet}`);
  }
  for (const snippet of forbidden) {
    console.error(`- Forbidden ${snippet}`);
  }
  process.exit(1);
}

console.log("CI workflow checks passed.");
