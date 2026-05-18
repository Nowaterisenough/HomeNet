import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
const version = process.argv[2] ?? process.env.HOMENET_RELEASE_VERSION;

if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  console.error("Usage: HOMENET_RELEASE_VERSION=0.1.123 node scripts/apply-release-version.mjs");
  process.exit(1);
}

function updateJsonVersion(relativePath) {
  const path = join(root, relativePath);
  const data = JSON.parse(readFileSync(path, "utf8"));
  data.version = version;
  writeFileSync(path, `${JSON.stringify(data, null, 2)}\n`);
}

function replaceFile(relativePath, replacer) {
  const path = join(root, relativePath);
  const source = readFileSync(path, "utf8");
  const updated = replacer(source);
  if (updated === source) {
    throw new Error(`No version field updated in ${relativePath}`);
  }
  writeFileSync(path, updated);
}

updateJsonVersion("package.json");
updateJsonVersion("src-tauri/tauri.conf.json");

replaceFile("src-tauri/Cargo.toml", (source) =>
  source.replace(/^version = "\d+\.\d+\.\d+"/m, `version = "${version}"`),
);

replaceFile("src-tauri/Cargo.lock", (source) =>
  source.replace(
    /(\[\[package\]\]\r?\nname = "homenet"\r?\nversion = ")\d+\.\d+\.\d+(")/,
    `$1${version}$2`,
  ),
);

console.log(`Applied HomeNet release version ${version}.`);
