import { readdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const DEFAULT_ASSET_DIR = "release-assets";

const UPDATE_TARGETS = [
  {
    assetSuffix: "_windows-x64-setup.exe",
    platforms: ["windows-x86_64-nsis", "windows-x86_64"],
  },
  {
    assetSuffix: "_macos-arm64-app.tar.gz",
    platforms: ["darwin-aarch64-app", "darwin-aarch64"],
  },
];

export async function buildUpdaterManifest({
  assetDir = DEFAULT_ASSET_DIR,
  releaseVersion,
  releaseTag,
  repository,
  pubDate = new Date().toISOString(),
  notes = "",
}) {
  if (!releaseVersion || !/^\d+\.\d+\.\d+$/.test(releaseVersion)) {
    throw new Error("HOMENET_RELEASE_VERSION must be a semver value like 1.2.3");
  }
  if (!releaseTag) {
    throw new Error("HOMENET_RELEASE_TAG is required");
  }
  if (!repository || !repository.includes("/")) {
    throw new Error("GITHUB_REPOSITORY must look like owner/repo");
  }

  const assetNames = await readdir(assetDir);
  const platforms = {};

  for (const target of UPDATE_TARGETS) {
    const assetName = assetNames.find((name) => name.endsWith(target.assetSuffix));
    if (!assetName) {
      throw new Error(`Missing updater asset ending with ${target.assetSuffix}`);
    }

    const signaturePath = join(assetDir, `${assetName}.sig`);
    let signature = "";
    try {
      signature = (await readFile(signaturePath, "utf8")).trim();
    } catch (error) {
      throw new Error(`Missing updater signature ${signaturePath}`, { cause: error });
    }

    const platform = {
      signature,
      url: releaseAssetUrl(repository, releaseTag, assetName),
    };

    for (const platformKey of target.platforms) {
      platforms[platformKey] = platform;
    }
  }

  return {
    version: releaseVersion,
    notes,
    pub_date: pubDate,
    platforms,
  };
}

export async function writeUpdaterManifest(options) {
  const assetDir = options.assetDir ?? DEFAULT_ASSET_DIR;
  const manifest = await buildUpdaterManifest({ ...options, assetDir });
  const outputPath = join(assetDir, "latest.json");
  await writeFile(outputPath, `${JSON.stringify(manifest, null, 2)}\n`);
  return outputPath;
}

function releaseAssetUrl(repository, releaseTag, assetName) {
  return `https://github.com/${repository}/releases/download/${encodeURIComponent(
    releaseTag,
  )}/${encodeURIComponent(assetName)}`;
}

async function main() {
  const outputPath = await writeUpdaterManifest({
    assetDir: process.env.RELEASE_ASSETS_DIR ?? DEFAULT_ASSET_DIR,
    releaseVersion: process.env.HOMENET_RELEASE_VERSION,
    releaseTag: process.env.HOMENET_RELEASE_TAG,
    repository: process.env.GITHUB_REPOSITORY,
    notes: process.env.HOMENET_RELEASE_NOTES ?? "",
  });

  console.log(`Generated updater manifest ${outputPath}`);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error.message);
    if (error.cause) {
      console.error(error.cause.message);
    }
    process.exit(1);
  });
}
