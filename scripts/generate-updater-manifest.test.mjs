import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { buildUpdaterManifest } from "./generate-updater-manifest.mjs";

test("builds Tauri updater manifest for Windows and macOS release assets", async () => {
  const assetDir = await mkdtemp(join(tmpdir(), "homenet-updater-manifest-"));
  try {
    await writeFile(join(assetDir, "HomeNet_v1.2.3_windows-x64-setup.exe"), "");
    await writeFile(join(assetDir, "HomeNet_v1.2.3_windows-x64-setup.exe.sig"), "win-signature\n");
    await writeFile(join(assetDir, "HomeNet_v1.2.3_macos-arm64-app.tar.gz"), "");
    await writeFile(join(assetDir, "HomeNet_v1.2.3_macos-arm64-app.tar.gz.sig"), "mac-signature\n");

    const manifest = await buildUpdaterManifest({
      assetDir,
      releaseVersion: "1.2.3",
      releaseTag: "v1.2.3",
      repository: "Nowaterisenough/HomeNet",
      pubDate: "2026-05-19T00:00:00.000Z",
      notes: "release notes",
    });

    assert.equal(manifest.version, "1.2.3");
    assert.equal(manifest.pub_date, "2026-05-19T00:00:00.000Z");
    assert.equal(manifest.notes, "release notes");
    assert.deepEqual(Object.keys(manifest.platforms).sort(), [
      "darwin-aarch64",
      "darwin-aarch64-app",
      "windows-x86_64",
      "windows-x86_64-nsis",
    ]);
    assert.deepEqual(manifest.platforms["windows-x86_64-nsis"], {
      signature: "win-signature",
      url: "https://github.com/Nowaterisenough/HomeNet/releases/download/v1.2.3/HomeNet_v1.2.3_windows-x64-setup.exe",
    });
    assert.deepEqual(manifest.platforms["windows-x86_64"], manifest.platforms["windows-x86_64-nsis"]);
    assert.deepEqual(manifest.platforms["darwin-aarch64-app"], {
      signature: "mac-signature",
      url: "https://github.com/Nowaterisenough/HomeNet/releases/download/v1.2.3/HomeNet_v1.2.3_macos-arm64-app.tar.gz",
    });
    assert.deepEqual(manifest.platforms["darwin-aarch64"], manifest.platforms["darwin-aarch64-app"]);
  } finally {
    await rm(assetDir, { recursive: true, force: true });
  }
});

test("fails clearly when an updater signature is missing", async () => {
  const assetDir = await mkdtemp(join(tmpdir(), "homenet-updater-manifest-"));
  try {
    await writeFile(join(assetDir, "HomeNet_v1.2.3_windows-x64-setup.exe"), "");
    await writeFile(join(assetDir, "HomeNet_v1.2.3_macos-arm64-app.tar.gz"), "");
    await writeFile(join(assetDir, "HomeNet_v1.2.3_macos-arm64-app.tar.gz.sig"), "mac-signature\n");

    await assert.rejects(
      () =>
        buildUpdaterManifest({
          assetDir,
          releaseVersion: "1.2.3",
          releaseTag: "v1.2.3",
          repository: "Nowaterisenough/HomeNet",
        }),
      /Missing updater signature/,
    );
  } finally {
    await rm(assetDir, { recursive: true, force: true });
  }
});
