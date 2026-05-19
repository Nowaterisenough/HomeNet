import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
let workflow = "";
let versionScript = "";
let updaterConfig = "";
try {
  workflow = readFileSync(join(root, ".github/workflows/build.yml"), "utf8");
  versionScript = readFileSync(join(root, "scripts/apply-release-version.mjs"), "utf8");
  updaterConfig = readFileSync(join(root, "src-tauri/tauri.updater.conf.json"), "utf8");
} catch {
  console.error("CI workflow checks failed:");
  console.error("- Missing .github/workflows/build.yml, scripts/apply-release-version.mjs, or src-tauri/tauri.updater.conf.json");
  process.exit(1);
}

const requiredSnippets = [
  "name: Build desktop artifacts",
  "tags:",
  "v*",
  "workflow_dispatch:",
  "contents: write",
  "metadata:",
  "name: Prepare release metadata",
  "release_version: ${{ steps.release.outputs.release_version }}",
  "release_tag: ${{ steps.release.outputs.release_tag }}",
  "release_name: ${{ steps.release.outputs.release_name }}",
  "changelog_to: ${{ steps.release.outputs.changelog_to }}",
  "fetch-depth: 0",
  "base_version=",
  "git rev-list --count HEAD",
  "release_version=\"${base_version}.${commit_count}\"",
  "release_tag=\"v${release_version}\"",
  "echo \"release_version=${release_version}\"",
  "release_name=HomeNet ${release_tag}",
  "macos-15",
  "aarch64-apple-darwin",
  "windows-latest",
  "tauri-apps/tauri-action@v0.6.2",
  "updaterGlob:",
  "updaterSuffix:",
  "updaterTargets:",
  "HOMENET_UPDATER_PUBLIC_KEY: ${{ steps.release_settings.outputs.updater_enabled == 'true' && secrets.TAURI_SIGNING_PUBLIC_KEY || '' }}",
  "--config src-tauri/tauri.updater.conf.json",
  "github.event_name",
  "Import Apple Developer Certificate",
  "Resolve Apple signing identity",
  "KEYCHAIN_PASSWORD: ${{ secrets.KEYCHAIN_PASSWORD }}",
  "security import certificate.p12",
  "APPLE_SIGNING_IDENTITY_RESOLVED",
  "TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}",
  "TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ steps.release_settings.outputs.updater_enabled == 'true' && secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD || '' }}",
  "APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}",
  "APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}",
  "APPLE_SIGNING_IDENTITY: ${{ steps.release_settings.outputs.macos_signing_enabled == 'true' && env.APPLE_SIGNING_IDENTITY_RESOLVED || '' }}",
  "APPLE_ID: ${{ secrets.APPLE_ID }}",
  "APPLE_PASSWORD: ${{ secrets.APPLE_PASSWORD }}",
  "APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}",
  "Staple macOS app notarization ticket",
  "Verify macOS signing and notarization",
  "xcrun stapler staple",
  "codesign --verify --deep --strict --verbose=2",
  "spctl -a -t exec -vv",
  "xcrun stapler validate",
  "spctl -a -t open --context context:primary-signature -vv",
  "Read-UpdaterSignature",
  "Generate updater manifest",
  "latest.json",
  "darwin-aarch64-app,darwin-aarch64",
  "windows-x86_64-nsis,windows-x86_64",
  "--bundles app,dmg",
  "--bundles nsis",
  "portableGlob:",
  "installerGlob:",
  "portableMode: zip",
  "needs: metadata",
  "Apply release version",
  "HOMENET_RELEASE_VERSION: ${{ needs.metadata.outputs.release_version }}",
  "node scripts/apply-release-version.mjs",
  "Resolve release settings",
  "id: release_settings",
  "updater_enabled",
  "macos_signing_enabled",
  "tauri_config_args",
  "Write-Warning \"TAURI_SIGNING_PUBLIC_KEY and TAURI_SIGNING_PRIVATE_KEY must be set together",
  "Write-Warning \"Apple signing secrets are partially configured",
  "if: ${{ steps.release_settings.outputs.macos_signing_enabled == 'true' }}",
  "steps.release_settings.outputs.tauri_config_args",
  "if (\"${{ steps.release_settings.outputs.updater_enabled }}\" -eq \"true\")",
  "if: ${{ steps.release_settings.outputs.updater_enabled == 'true' }}",
  "release:",
  "if: ${{ github.event_name != 'pull_request' }}",
  "- metadata",
  "- build",
  "actions/download-artifact@v5",
  "mikepenz/release-changelog-builder-action@v6",
  "id: build_changelog",
  "mode: COMMIT",
  "toTag: ${{ needs.metadata.outputs.changelog_to }}",
  "offlineMode: true",
  "failOnError: true",
  "configurationJson:",
  "## 功能",
  "## 修复",
  "## CI / 构建",
  "## 文档",
  "## 维护",
  "label_extractor",
  "Move version tag to this build",
  "git tag -fa \"${{ needs.metadata.outputs.release_tag }}\"",
  "Remove lowercase release assets",
  "select(startswith(\"homenet_\"))",
  "gh release delete-asset",
  "softprops/action-gh-release@v3",
  "tag_name: ${{ needs.metadata.outputs.release_tag }}",
  "target_commitish: ${{ github.sha }}",
  "name: ${{ needs.metadata.outputs.release_name }}",
  "body: ${{ steps.build_changelog.outputs.changelog }}",
  "files: release-assets/*",
  "overwrite_files: true",
  "make_latest: true",
  "HomeNet_${{ needs.metadata.outputs.release_tag }}_${{ matrix.portableSuffix }}",
  "HomeNet_${{ needs.metadata.outputs.release_tag }}_${{ matrix.installerSuffix }}",
  "portableSuffix: macos-arm64-app.zip",
  "installerSuffix: macos-arm64.dmg",
  "portableSuffix: windows-x64-portable.zip",
  "installerSuffix: windows-x64-setup.exe",
  "artifactName: HomeNet-macos-arm64",
  "artifactName: HomeNet-windows-x64",
  "actions/upload-artifact@v4",
  "pnpm/action-setup@v4",
  "actions/setup-node@v4",
  "dtolnay/rust-toolchain@stable",
  "pnpm install --frozen-lockfile",
];

const missing = requiredSnippets.filter((snippet) => !workflow.includes(snippet));
const requiredTauriConfigSnippets = [
  '"createUpdaterArtifacts": true',
  '"installMode": "quiet"',
];
const missingTauriConfig = requiredTauriConfigSnippets.filter(
  (snippet) => !updaterConfig.includes(snippet),
);
const requiredVersionScriptSnippets = [
  "HOMENET_RELEASE_VERSION",
  "/^\\d+\\.\\d+\\.\\d+$/",
  'updateJsonVersion("package.json")',
  'updateJsonVersion("src-tauri/tauri.conf.json")',
  'replaceFile("src-tauri/Cargo.toml"',
  'replaceFile("src-tauri/Cargo.lock"',
  'name = "homenet"',
  "Applied HomeNet release version",
];
const missingVersionScript = requiredVersionScriptSnippets.filter(
  (snippet) => !versionScript.includes(snippet),
);
const forbiddenSnippets = [
  "网络管家",
  "鍔熻兘",
  "淇",
  "鏋勫缓",
  "鏂囨。",
  "缁存姢",
  "鍏朵粬",
  "name: homenet ${{ github.ref_name }}",
  "name: HomeNet ${{ github.ref_name }}",
  "if: ${{ startsWith(github.ref, 'refs/tags/') }}",
  "release_tag=\"v${version}\"",
  "homenet_${{ github.ref_name }}_",
  "artifactName: homenet-",
  "--no-bundle",
  "assetSuffix:",
  "assetMode:",
  "artifactGlob:",
  "windows-x64-portable.exe",
  "Write-Error \"TAURI_SIGNING_PUBLIC_KEY and TAURI_SIGNING_PRIVATE_KEY must be set together",
  "Write-Error \"Apple signing secrets are partially configured",
];
const forbidden = forbiddenSnippets.filter((snippet) => workflow.includes(snippet));

if (
  missing.length > 0 ||
  missingTauriConfig.length > 0 ||
  missingVersionScript.length > 0 ||
  forbidden.length > 0
) {
  console.error("CI workflow checks failed:");
  for (const snippet of missing) {
    console.error(`- Missing ${snippet}`);
  }
  for (const snippet of missingTauriConfig) {
    console.error(`- Missing Tauri config ${snippet}`);
  }
  for (const snippet of missingVersionScript) {
    console.error(`- Missing version script ${snippet}`);
  }
  for (const snippet of forbidden) {
    console.error(`- Forbidden ${snippet}`);
  }
  process.exit(1);
}

console.log("CI workflow checks passed.");
