import { existsSync, readFileSync } from "node:fs";
import { resolve, join } from "node:path";
import { spawnSync } from "node:child_process";

if (process.platform !== "darwin") throw new Error("This check requires macOS");
if (!process.argv[2])
  throw new Error("Usage: node scripts/verify-macos-bundle.mjs <HomeNet.app>");
const app = resolve(process.argv[2]);
const config = JSON.parse(
  readFileSync(
    new URL("../src-tauri/tauri.conf.json", import.meta.url),
    "utf8",
  ),
);
if (!existsSync(join(app, "Contents/_CodeSignature/CodeResources"))) {
  throw new Error(
    "The app bundle has no signed resource envelope; executable-only signing is insufficient",
  );
}
const verification = spawnSync(
  "codesign",
  ["--verify", "--deep", "--strict", "--verbose=2", app],
  { stdio: "inherit" },
);
if (verification.error) throw verification.error;
if (verification.status !== 0)
  throw new Error("macOS app bundle signature verification failed");
const details = spawnSync("codesign", ["-dv", "--verbose=2", app], {
  encoding: "utf8",
});
if (details.error) throw details.error;
const identifier = `${details.stdout}\n${details.stderr}`
  .match(/^Identifier=(.+)$/m)?.[1]
  ?.trim();
if (details.status !== 0 || identifier !== config.identifier) {
  throw new Error(
    `Signing identifier must match the app identifier: ${config.identifier}`,
  );
}
const plist = spawnSync(
  "plutil",
  [
    "-extract",
    "CFBundleIdentifier",
    "raw",
    "-o",
    "-",
    join(app, "Contents/Info.plist"),
  ],
  { encoding: "utf8" },
);
if (plist.status !== 0 || plist.stdout.trim() !== config.identifier) {
  throw new Error("Bundle metadata and signing identity do not match");
}
console.log(`Verified complete macOS bundle signature: ${identifier}`);
