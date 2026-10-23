import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const icons = join(root, "src-tauri/icons");
const output = mkdtempSync(join(tmpdir(), "homenet-app-icons-"));
const check = process.argv.includes("--check");
const desktopIcons = [
  "32x32.png",
  "128x128.png",
  "128x128@2x.png",
  "icon.png",
  "icon.icns",
  "icon.ico",
  "Square30x30Logo.png",
  "Square44x44Logo.png",
  "Square71x71Logo.png",
  "Square89x89Logo.png",
  "Square107x107Logo.png",
  "Square142x142Logo.png",
  "Square150x150Logo.png",
  "Square284x284Logo.png",
  "Square310x310Logo.png",
  "StoreLogo.png",
];

function canonicalIcns(data) {
  if (data.toString("ascii", 0, 4) !== "icns" || data.readUInt32BE(4) !== data.length) {
    throw new Error("Invalid ICNS header");
  }
  const chunks = [];
  for (let offset = 8; offset < data.length;) {
    if (offset + 8 > data.length) throw new Error("Truncated ICNS chunk");
    const length = data.readUInt32BE(offset + 4);
    if (length < 8 || offset + length > data.length) {
      throw new Error("Invalid ICNS chunk length");
    }
    chunks.push(data.subarray(offset, offset + length));
    offset += length;
  }
  // Tauri emits ICNS entries in hash-map order; stabilize it for reproducible assets.
  chunks.sort((a, b) => a.subarray(0, 4).compare(b.subarray(0, 4)));
  return Buffer.concat([data.subarray(0, 8), ...chunks]);
}

try {
  const result = spawnSync(
    process.execPath,
    [
      join(root, "node_modules/@tauri-apps/cli/tauri.js"),
      "icon",
      join(icons, "app-icon.svg"),
      "--output",
      output,
    ],
    { cwd: root, stdio: "inherit" },
  );
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error("Application icon generation failed");

  // Copy only desktop bundle assets; never overwrite the unframed tray icon.
  for (const name of desktopIcons) {
    const raw = readFileSync(join(output, name));
    const generated = name === "icon.icns" ? canonicalIcns(raw) : raw;
    const destination = join(icons, name);
    if (check) {
      if (!generated.equals(readFileSync(destination))) {
        throw new Error(`${name} is outdated; run pnpm icons:app`);
      }
    } else {
      writeFileSync(destination, generated);
    }
  }
  console.log(
    check
      ? "Application icons match app-icon.svg."
      : "Desktop application icons generated; tray icon unchanged.",
  );
} finally {
  rmSync(output, { recursive: true, force: true });
}
